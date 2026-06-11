defmodule XeroWeb.RemoteSessionChannel do
  use XeroWeb, :channel

  alias Xero.Remote
  alias Xero.Remote.{ControlSessionRegistry, Turn}
  alias XeroWeb.RemoteChannelRateLimit

  intercept ["frame"]

  @stream_token_salt "computer-use-stream:v1"
  @stream_token_max_age_seconds 600
  @computer_use_command_max_bytes 512 * 1024
  @stream_token_command_kinds ~w(
    computer_use_stream_request
    computer_use_stream_offer
    computer_use_stream_answer
    computer_use_stream_ice_candidate
    computer_use_stream_stop
    computer_use_stream_status
    computer_use_stream_set_quality
    computer_use_stream_request_keyframe
    computer_use_manual_control_request
    computer_use_manual_control_grant
    computer_use_manual_control_heartbeat
    computer_use_manual_control_input
    computer_use_manual_control_release
  )
  @stream_command_kinds ~w(
    computer_use_stream_request
    computer_use_stream_offer
    computer_use_stream_answer
    computer_use_stream_ice_candidate
    computer_use_stream_stop
    computer_use_stream_status
    computer_use_stream_set_quality
    computer_use_stream_request_keyframe
  )
  @manual_control_command_kinds ~w(
    computer_use_manual_control_request
    computer_use_manual_control_grant
    computer_use_manual_control_heartbeat
    computer_use_manual_control_input
    computer_use_manual_control_release
  )
  @critical_reliable_command_kinds @manual_control_command_kinds ++
                                     ~w(
                                       computer_use_stream_offer
                                       computer_use_stream_answer
                                       computer_use_stream_ice_candidate
                                     )
  @coalesced_command_kinds ~w(
    computer_use_stream_status
    computer_use_stream_set_quality
  )
  @control_session_ids ~w(__sessions__ __projects__ __theme__)
  @active_connection_reason "computer_use_connection_already_active"
  @active_connection_message "Xero Cloud is already connected to this desktop from another app instance or location. Stop that running connection first to use it here."

  @impl true
  def join("session:" <> rest, payload, socket) do
    with [desktop_device_id, session_id] <- String.split(rest, ":", parts: 2),
         true <- authorized_for_desktop?(socket, desktop_device_id),
         {:ok, stream_run_id} <-
           authorize_session_join(socket, desktop_device_id, session_id, payload) do
      :telemetry.execute([:xero, :remote, :channel, :join], %{count: 1}, %{
        kind: socket.assigns.device_kind,
        topic: "session",
        desktop_device_id: desktop_device_id
      })

      socket =
        assign(socket,
          desktop_device_id: desktop_device_id,
          session_id: session_id,
          cloud_instance_id: cloud_instance_id_from_join_payload(socket, payload)
        )

      remote_control_state = remote_control_join_state(socket)

      reply =
        %{
          desktop_device_id: desktop_device_id,
          session_id: session_id,
          ice_servers: Turn.ice_servers()
        }
        |> put_stream_token(socket, desktop_device_id, session_id, stream_run_id)
        |> maybe_put_remote_control_state(remote_control_state)

      {:ok, reply, socket}
    else
      _ -> {:error, %{reason: "unauthorized"}}
    end
  end

  @impl true
  def handle_in("frame", payload, socket) when is_map(payload) do
    case validate_frame_authorization(socket, payload) do
      :ok ->
        case validate_command_freshness(payload) do
          :ok ->
            case validate_command_payload_size(payload) do
              :ok ->
                forward_authorized_frame(socket, payload)

              {:error, size_limit} ->
                reject_sized_command_frame(socket, payload, size_limit)
            end

          {:error, reason} ->
            reject_stale_command_frame(socket, payload, reason)
        end

      {:error, reason} ->
        emit_computer_use_command_telemetry(
          :rejected,
          socket,
          payload,
          payload_size(payload),
          reason
        )

        outcome = command_outcome(socket, payload, "rejected", reason, nil)
        push_computer_use_command_outcome(socket, outcome)

        {:reply, {:error, %{reason: reason, command: maybe_command_reply(outcome)}}, socket}
    end
  end

  def handle_in("frame", _payload, socket) do
    {:reply, {:error, %{reason: "invalid_payload"}}, socket}
  end

  defp forward_authorized_frame(socket, payload) do
    case rate_limit_frame(socket, payload) do
      :ok ->
        case authorize_remote_control_frame(socket, payload) do
          :ok ->
            direction = direction(socket.assigns.device_kind)
            bytes = payload_size(payload)

            :telemetry.execute(
              [:xero, :remote, :frame, :forwarded],
              %{bytes: bytes, count: 1},
              %{
                direction: direction,
                session_id: socket.assigns.session_id,
                desktop_device_id: socket.assigns.desktop_device_id
              }
            )

            emit_computer_use_command_telemetry(:forwarded, socket, payload, bytes, nil)
            outcome = command_outcome(socket, payload, "accepted", nil, nil)
            push_computer_use_command_outcome(socket, outcome)

            broadcast_from!(
              socket,
              "frame",
              socket
              |> frame_payload(direction, payload)
              |> target_remote_control_owner(socket, payload)
            )

            after_forward_remote_control_frame(socket, payload)

            {:reply, {:ok, maybe_command_reply(outcome)}, socket}

          {:error, reason} ->
            reject_remote_control_frame(socket, payload, reason)
        end

      {:error, rate_limit} ->
        emit_computer_use_command_telemetry(
          :rejected,
          socket,
          payload,
          payload_size(payload),
          "rate_limited"
        )

        outcome = command_outcome(socket, payload, "rate_limited", "rate_limited", rate_limit)
        push_computer_use_command_outcome(socket, outcome)

        {:reply,
         {:error,
          %{
            reason: "rate_limited",
            retry_after_ms: rate_limit.retry_after_ms,
            retryAfterMs: rate_limit.retry_after_ms,
            rateLimit: rate_limit,
            command: outcome
          }}, socket}
    end
  end

  defp reject_stale_command_frame(socket, payload, reason) do
    emit_computer_use_command_telemetry(
      :rejected,
      socket,
      payload,
      payload_size(payload),
      reason
    )

    outcome = command_outcome(socket, payload, "stale", reason, nil)
    push_computer_use_command_outcome(socket, outcome)

    {:reply, {:error, %{reason: reason, command: maybe_command_reply(outcome)}}, socket}
  end

  defp reject_sized_command_frame(socket, payload, size_limit) do
    emit_computer_use_command_telemetry(
      :rejected,
      socket,
      payload,
      size_limit.size_bytes,
      size_limit.reason
    )

    outcome = command_outcome(socket, payload, "rejected", size_limit.reason, nil)
    push_computer_use_command_outcome(socket, outcome)

    {:reply,
     {:error,
      %{
        reason: size_limit.reason,
        maxBytes: size_limit.max_bytes,
        max_bytes: size_limit.max_bytes,
        sizeBytes: size_limit.size_bytes,
        size_bytes: size_limit.size_bytes,
        command: maybe_command_reply(outcome)
      }}, socket}
  end

  @impl true
  def handle_out("frame", payload, socket) do
    if deliver_frame_to_socket?(payload, socket) do
      push(socket, "frame", public_frame_payload(payload))
    end

    {:noreply, socket}
  end

  @impl true
  def terminate(_reason, %{assigns: %{device_kind: :web}} = socket) do
    with %{desktop_device_id: desktop_device_id, session_id: session_id} <- socket.assigns do
      ControlSessionRegistry.release(
        desktop_device_id,
        session_id,
        remote_control_owner_id(socket),
        self()
      )
    end

    :ok
  end

  def terminate(_reason, _socket), do: :ok

  defp reject_remote_control_frame(socket, payload, reason) do
    emit_computer_use_command_telemetry(
      :rejected,
      socket,
      payload,
      payload_size(payload),
      reason
    )

    outcome = command_outcome(socket, payload, "rejected", reason, nil)
    push_computer_use_command_outcome(socket, outcome)

    {:reply,
     {:error,
      %{
        reason: reason,
        message: rejection_message(reason),
        command: maybe_command_reply(outcome)
      }}, socket}
  end

  defp frame_payload(socket, direction, payload) do
    %{
      from_device_id: socket.assigns.device_id,
      from_kind: Atom.to_string(socket.assigns.device_kind),
      direction: Atom.to_string(direction),
      payload: payload
    }
  end

  defp target_remote_control_owner(frame, %{assigns: %{device_kind: :desktop}} = socket, payload) do
    if computer_use_desktop_payload?(payload) do
      case ControlSessionRegistry.active_owner(
             socket.assigns.desktop_device_id,
             socket.assigns.session_id
           ) do
        %{owner_id: owner_id, web_device_id: web_device_id} ->
          frame
          |> Map.put(:target_owner_id, owner_id)
          |> Map.put(:target_web_device_id, web_device_id)

        nil ->
          frame
      end
    else
      frame
    end
  end

  defp target_remote_control_owner(frame, _socket, _payload), do: frame

  defp after_forward_remote_control_frame(_socket, _payload), do: :ok

  defp authorize_remote_control_frame(%{assigns: %{device_kind: :web}} = socket, %{
         "kind" => kind
       }) do
    if remote_control_read_command?(socket, kind) do
      :ok
    else
      authorize_remote_control_owner(socket)
    end
  end

  defp authorize_remote_control_frame(%{assigns: %{device_kind: :web}} = socket, _payload) do
    authorize_remote_control_owner(socket)
  end

  defp authorize_remote_control_frame(_socket, _payload), do: :ok

  defp authorize_remote_control_owner(socket) do
    owner_id = remote_control_owner_id(socket)

    case ControlSessionRegistry.active_owner(
           socket.assigns.desktop_device_id,
           socket.assigns.session_id
         ) do
      nil ->
        case ControlSessionRegistry.acquire(
               socket.assigns.desktop_device_id,
               socket.assigns.session_id,
               owner_id,
               socket.assigns.device_id,
               self()
             ) do
          {:ok, _owner} -> :ok
          {:error, {:already_active, _owner}} -> {:error, @active_connection_reason}
        end

      %{owner_id: ^owner_id} ->
        :ok

      _owner ->
        {:error, @active_connection_reason}
    end
  end

  defp remote_control_read_command?(_socket, "session_attached"), do: true

  defp remote_control_read_command?(%{assigns: %{session_id: "__sessions__"}}, "list_sessions"),
    do: true

  defp remote_control_read_command?(%{assigns: %{session_id: "__projects__"}}, "list_projects"),
    do: true

  defp remote_control_read_command?(_socket, _kind), do: false

  defp deliver_frame_to_socket?(
         %{payload: payload} = frame,
         %{assigns: %{device_kind: :web}} = socket
       ) do
    cond do
      computer_use_inbound_command?(payload) ->
        false

      !computer_use_desktop_payload?(payload) ->
        true

      Map.get(frame, :target_owner_id) == remote_control_owner_id(socket) ->
        true

      true ->
        false
    end
  end

  defp deliver_frame_to_socket?(_frame, _socket), do: true

  defp public_frame_payload(frame) do
    Map.drop(frame, [:target_owner_id, :target_web_device_id])
  end

  defp computer_use_desktop_payload?(payload) do
    case payload_schema(payload) do
      "xero.computer_use_stream_" <> _ -> true
      "xero.computer_use_manual_control_" <> _ -> true
      _ -> false
    end
  end

  defp computer_use_inbound_command?(%{"kind" => kind}) when kind in @stream_token_command_kinds,
    do: true

  defp computer_use_inbound_command?(%{kind: kind}) when kind in @stream_token_command_kinds,
    do: true

  defp computer_use_inbound_command?(_payload), do: false

  defp payload_schema(%{"schema" => schema}) when is_binary(schema), do: schema
  defp payload_schema(%{schema: schema}) when is_binary(schema), do: schema
  defp payload_schema(_payload), do: nil

  defp rejection_message(@active_connection_reason), do: @active_connection_message
  defp rejection_message(_reason), do: nil

  defp authorized_for_desktop?(
         %{assigns: %{device_kind: :desktop, device_id: device_id}},
         desktop_id
       ) do
    device_id == desktop_id
  end

  defp authorized_for_desktop?(
         %{assigns: %{device_kind: :web, account_id: account_id}},
         desktop_id
       ) do
    Remote.desktop_device_for_account(account_id, desktop_id) != nil
  end

  defp authorized_for_desktop?(_socket, _desktop_id), do: false

  defp authorize_session_join(
         %{assigns: %{device_kind: :desktop}},
         _desktop_device_id,
         _session_id,
         _payload
       ),
       do: {:ok, nil}

  defp authorize_session_join(
         %{assigns: %{device_kind: :web}} = socket,
         desktop_device_id,
         "__sessions__",
         payload
       ),
       do: notify_desktop_control_session_join(socket, desktop_device_id, "__sessions__", payload)

  defp authorize_session_join(
         %{assigns: %{device_kind: :web}} = socket,
         desktop_device_id,
         "__projects__",
         payload
       ),
       do: notify_desktop_control_session_join(socket, desktop_device_id, "__projects__", payload)

  defp authorize_session_join(
         %{assigns: %{device_kind: :web}} = socket,
         desktop_device_id,
         "__theme__",
         payload
       ),
       do: notify_desktop_control_session_join(socket, desktop_device_id, "__theme__", payload)

  defp authorize_session_join(
         %{assigns: %{device_kind: :web}} = socket,
         desktop_device_id,
         session_id,
         payload
       ) do
    join_ref =
      Map.get(payload, "join_ref") ||
        "join:#{socket.assigns.device_id}:#{System.unique_integer([:positive])}"

    auth_topic = "desktop:#{desktop_device_id}:session_join:#{join_ref}"
    :ok = Phoenix.PubSub.subscribe(Xero.PubSub, auth_topic)

    XeroWeb.Endpoint.broadcast("desktop:#{desktop_device_id}", "session_join_requested", %{
      join_ref: join_ref,
      auth_topic: auth_topic,
      web_device_id: socket.assigns.device_id,
      session_id: session_id,
      last_seq: Map.get(payload, "last_seq")
    })

    receive do
      %Phoenix.Socket.Broadcast{
        event: "session_authorized",
        payload: %{"join_ref" => ^join_ref, "authorized" => true} = authorization
      } ->
        {:ok, optional_string(authorization["run_id"] || authorization["runId"])}

      %Phoenix.Socket.Broadcast{event: "session_authorized", payload: %{"join_ref" => ^join_ref}} ->
        {:error, :session_not_visible}
    after
      2_000 -> {:error, :session_authorization_timeout}
    end
  end

  defp authorize_session_join(_socket, _desktop_device_id, _session_id, _payload),
    do: {:error, :unauthorized}

  defp notify_desktop_control_session_join(socket, desktop_device_id, session_id, payload) do
    join_ref =
      Map.get(payload, "join_ref") ||
        "join:#{socket.assigns.device_id}:#{System.unique_integer([:positive])}"

    auth_topic = "desktop:#{desktop_device_id}:session_join:#{join_ref}"

    XeroWeb.Endpoint.broadcast("desktop:#{desktop_device_id}", "session_join_requested", %{
      join_ref: join_ref,
      auth_topic: auth_topic,
      web_device_id: socket.assigns.device_id,
      session_id: session_id,
      last_seq: Map.get(payload, "last_seq")
    })

    {:ok, nil}
  end

  defp remote_control_join_state(
         %{
           assigns: %{
             device_kind: :web,
             desktop_device_id: desktop_device_id,
             device_id: web_device_id,
             session_id: session_id
           }
         } = socket
       ) do
    case ControlSessionRegistry.acquire(
           desktop_device_id,
           session_id,
           remote_control_owner_id(socket),
           web_device_id,
           self()
         ) do
      {:ok, owner} ->
        remote_control_state(true, nil, nil, owner)

      {:error, {:already_active, owner}} ->
        remote_control_state(false, @active_connection_reason, @active_connection_message, owner)
    end
  end

  defp remote_control_join_state(_socket), do: nil

  defp remote_control_state(available, reason, message, owner) do
    %{
      available: available,
      reason: reason,
      message: message,
      ownerDeviceId: owner && owner.web_device_id,
      startedAt: owner && owner.started_at
    }
  end

  defp cloud_instance_id_from_join_payload(%{assigns: %{device_kind: :web}} = socket, payload) do
    payload_id =
      optional_string(
        Map.get(payload, "cloud_instance_id") || Map.get(payload, "cloudInstanceId")
      )

    payload_id || socket.assigns.device_id
  end

  defp cloud_instance_id_from_join_payload(_socket, _payload), do: nil

  defp remote_control_owner_id(socket) do
    Map.get(socket.assigns, :cloud_instance_id) || socket.assigns.device_id
  end

  defp put_stream_token(
         reply,
         %{assigns: %{device_kind: :web, device_id: web_device_id}},
         desktop_device_id,
         session_id,
         stream_run_id
       ) do
    if session_id in @control_session_ids do
      reply
    else
      claims = %{
        "desktop_device_id" => desktop_device_id,
        "session_id" => session_id,
        "web_device_id" => web_device_id
      }

      claims =
        case stream_run_id do
          nil -> claims
          run_id -> Map.put(claims, "run_id", run_id)
        end

      reply
      |> Map.put(:stream_token, Phoenix.Token.sign(XeroWeb.Endpoint, @stream_token_salt, claims))
      |> maybe_put_stream_run_id(stream_run_id)
    end
  end

  defp put_stream_token(reply, _socket, _desktop_device_id, _session_id, _stream_run_id),
    do: reply

  defp maybe_put_remote_control_state(reply, nil), do: reply
  defp maybe_put_remote_control_state(reply, state), do: Map.put(reply, :remote_control, state)

  defp maybe_put_stream_run_id(reply, nil), do: reply
  defp maybe_put_stream_run_id(reply, run_id), do: Map.put(reply, :stream_run_id, run_id)

  defp validate_frame_authorization(
         %{assigns: %{device_kind: :web}} = socket,
         %{"kind" => kind} = payload
       )
       when kind in @stream_token_command_kinds do
    with token when is_binary(token) and token != "" <- stream_token_from_payload(payload),
         {:ok, claims} <-
           Phoenix.Token.verify(XeroWeb.Endpoint, @stream_token_salt, token,
             max_age: @stream_token_max_age_seconds
           ),
         true <- stream_token_claims_match?(claims, socket, payload) do
      :ok
    else
      _ -> {:error, "invalid_stream_token"}
    end
  end

  defp validate_frame_authorization(_socket, _payload), do: :ok

  defp validate_command_freshness(%{"kind" => kind, "expiresAt" => expires_at})
       when kind in @stream_token_command_kinds do
    if command_expired?(expires_at), do: {:error, "stale_command"}, else: :ok
  end

  defp validate_command_freshness(%{"kind" => kind, "expires_at" => expires_at})
       when kind in @stream_token_command_kinds do
    if command_expired?(expires_at), do: {:error, "stale_command"}, else: :ok
  end

  defp validate_command_freshness(%{"kind" => kind}) when kind in @stream_token_command_kinds,
    do: :ok

  defp validate_command_freshness(_payload), do: :ok

  defp command_expired?(expires_at) when is_integer(expires_at),
    do: expires_at <= System.system_time(:millisecond)

  defp command_expired?(expires_at) when is_binary(expires_at) do
    case Integer.parse(expires_at) do
      {millis, ""} -> command_expired?(millis)
      _ -> true
    end
  end

  defp command_expired?(_expires_at), do: true

  defp validate_command_payload_size(%{"kind" => kind} = payload)
       when kind in @stream_token_command_kinds do
    size_bytes = payload_size(payload)

    if size_bytes <= @computer_use_command_max_bytes do
      :ok
    else
      {:error,
       %{
         reason: "command_payload_too_large",
         size_bytes: size_bytes,
         max_bytes: @computer_use_command_max_bytes
       }}
    end
  end

  defp validate_command_payload_size(_payload), do: :ok

  defp stream_token_from_payload(%{"payload" => payload}) when is_map(payload) do
    Map.get(payload, "streamToken") || Map.get(payload, "stream_token")
  end

  defp stream_token_from_payload(_payload), do: nil

  defp stream_token_claims_match?(
         %{
           "desktop_device_id" => desktop_device_id,
           "session_id" => session_id,
           "web_device_id" => web_device_id
         } = claims,
         %{
           assigns: %{
             desktop_device_id: desktop_device_id,
             session_id: session_id,
             device_id: web_device_id
           }
         },
         payload
       ),
       do: stream_token_run_claim_matches?(claims, payload)

  defp stream_token_claims_match?(_claims, _socket, _payload), do: false

  defp stream_token_run_claim_matches?(%{"run_id" => run_id}, %{"payload" => payload})
       when is_binary(run_id) and is_map(payload) do
    command_run_id = optional_string(Map.get(payload, "runId") || Map.get(payload, "run_id"))
    command_run_id == run_id
  end

  defp stream_token_run_claim_matches?(_claims, _payload), do: true

  defp optional_string(value) when is_binary(value) do
    case String.trim(value) do
      "" -> nil
      trimmed -> trimmed
    end
  end

  defp optional_string(_value), do: nil

  defp rate_limit_frame(%{assigns: %{device_kind: :web}} = socket, payload),
    do: RemoteChannelRateLimit.hit(socket, "frame", payload)

  defp rate_limit_frame(%{assigns: %{device_kind: :desktop}}, _payload), do: :ok

  defp direction(:desktop), do: :desktop_to_web
  defp direction(:web), do: :web_to_desktop

  defp push_computer_use_command_outcome(_socket, nil), do: :ok

  defp push_computer_use_command_outcome(socket, outcome) do
    push(socket, "computer_use_command_outcome", outcome)

    latency_ms =
      case {outcome[:sentAt], System.system_time(:millisecond)} do
        {sent_at, now} when is_integer(sent_at) -> max(now - sent_at, 0)
        _ -> 0
      end

    :telemetry.execute(
      [:xero, :remote, :computer_use, :command, :outcome],
      %{count: 1, ack_latency_ms: latency_ms},
      %{
        kind: outcome.kind,
        outcome: outcome.outcome,
        priority: outcome.priority,
        reason: outcome.reason || "none",
        direction: direction(socket.assigns.device_kind),
        session_id: socket.assigns.session_id,
        desktop_device_id: socket.assigns.desktop_device_id
      }
    )
  end

  defp maybe_command_reply(nil), do: %{}
  defp maybe_command_reply(outcome), do: outcome

  defp command_outcome(_socket, %{"kind" => kind}, _outcome, _reason, _rate_limit)
       when kind not in @stream_token_command_kinds,
       do: nil

  defp command_outcome(socket, %{"kind" => kind} = payload, outcome, reason, rate_limit)
       when kind in @stream_token_command_kinds do
    client_command_id =
      payload_string(payload, ["clientCommandId", "client_command_id"]) ||
        "relay:#{socket.assigns.device_id}:#{System.unique_integer([:positive])}"

    %{
      schema: "xero.remote_command_outcome.v1",
      clientCommandId: client_command_id,
      clientSeq: payload_integer(payload, ["clientSeq", "client_seq"]),
      kind: kind,
      outcome: outcome,
      priority:
        payload_string(payload, ["priority"]) ||
          command_priority(kind),
      reason: reason,
      message: rejection_message(reason),
      sentAt: payload_integer(payload, ["sentAt", "sent_at"]),
      receivedAt: DateTime.utc_now() |> DateTime.to_iso8601(),
      acceptedAt: accepted_at_for_outcome(outcome),
      retryAfterMs: rate_limit_retry_after(rate_limit),
      rateLimit: rate_limit
    }
  end

  defp command_outcome(_socket, _payload, _outcome, _reason, _rate_limit), do: nil

  defp accepted_at_for_outcome("accepted"), do: DateTime.utc_now() |> DateTime.to_iso8601()
  defp accepted_at_for_outcome(_outcome), do: nil

  defp rate_limit_retry_after(%{retry_after_ms: retry_after_ms}), do: retry_after_ms
  defp rate_limit_retry_after(_rate_limit), do: nil

  defp command_priority(kind) when kind in @critical_reliable_command_kinds,
    do: "critical_reliable"

  defp command_priority(kind) when kind in @coalesced_command_kinds,
    do: "coalesced_best_effort"

  defp command_priority(_kind), do: "reliable_idempotent"

  defp payload_string(payload, keys) do
    Enum.find_value(keys, fn key ->
      case Map.get(payload, key) do
        value when is_binary(value) and value != "" -> value
        _ -> nil
      end
    end)
  end

  defp payload_integer(payload, keys) do
    Enum.find_value(keys, fn key ->
      case Map.get(payload, key) do
        value when is_integer(value) ->
          value

        value when is_binary(value) ->
          case Integer.parse(value) do
            {parsed, ""} -> parsed
            _ -> nil
          end

        _ ->
          nil
      end
    end)
  end

  defp emit_computer_use_command_telemetry(result, socket, %{"kind" => kind}, bytes, reason)
       when kind in @stream_token_command_kinds do
    measurements = %{count: 1, bytes: bytes}

    metadata = %{
      family: computer_use_command_family(kind),
      kind: kind,
      direction: direction(socket.assigns.device_kind),
      session_id: socket.assigns.session_id,
      desktop_device_id: socket.assigns.desktop_device_id,
      reason: reason || "none"
    }

    :telemetry.execute([:xero, :remote, :computer_use, :command, result], measurements, metadata)
  end

  defp emit_computer_use_command_telemetry(_result, _socket, _payload, _bytes, _reason), do: :ok

  defp computer_use_command_family(kind) when kind in @stream_command_kinds, do: :stream

  defp computer_use_command_family(kind) when kind in @manual_control_command_kinds,
    do: :manual_control

  defp payload_size(payload) do
    payload
    |> Jason.encode!()
    |> byte_size()
  end
end
