defmodule XeroWeb.RemoteSessionChannel do
  use XeroWeb, :channel

  alias Xero.Remote
  alias Xero.Remote.Turn
  alias XeroWeb.RemoteChannelRateLimit

  @stream_token_salt "computer-use-stream:v1"
  @stream_token_max_age_seconds 600
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
  @control_session_ids ~w(__sessions__ __projects__ __theme__)

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
          session_id: session_id
        )

      reply =
        %{
          desktop_device_id: desktop_device_id,
          session_id: session_id,
          ice_servers: Turn.ice_servers()
        }
        |> put_stream_token(socket, desktop_device_id, session_id, stream_run_id)

      {:ok, reply, socket}
    else
      _ -> {:error, %{reason: "unauthorized"}}
    end
  end

  @impl true
  def handle_in("frame", payload, socket) when is_map(payload) do
    case validate_frame_authorization(socket, payload) do
      :ok ->
        case rate_limit_frame(socket) do
          :ok ->
            direction = direction(socket.assigns.device_kind)
            bytes = payload_size(payload)

            :telemetry.execute([:xero, :remote, :frame, :forwarded], %{bytes: bytes, count: 1}, %{
              direction: direction,
              session_id: socket.assigns.session_id,
              desktop_device_id: socket.assigns.desktop_device_id
            })

            emit_computer_use_command_telemetry(:forwarded, socket, payload, bytes, nil)

            broadcast_from!(socket, "frame", %{
              from_device_id: socket.assigns.device_id,
              from_kind: Atom.to_string(socket.assigns.device_kind),
              direction: Atom.to_string(direction),
              payload: payload
            })

            {:reply, :ok, socket}

          {:error, retry_after_ms} ->
            emit_computer_use_command_telemetry(
              :rejected,
              socket,
              payload,
              payload_size(payload),
              "rate_limited"
            )

            {:reply, {:error, %{reason: "rate_limited", retry_after_ms: retry_after_ms}}, socket}
        end

      {:error, reason} ->
        emit_computer_use_command_telemetry(
          :rejected,
          socket,
          payload,
          payload_size(payload),
          reason
        )

        {:reply, {:error, %{reason: reason}}, socket}
    end
  end

  def handle_in("frame", _payload, socket) do
    {:reply, {:error, %{reason: "invalid_payload"}}, socket}
  end

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

  defp rate_limit_frame(%{assigns: %{device_kind: :web}} = socket),
    do: RemoteChannelRateLimit.hit(socket, "frame")

  defp rate_limit_frame(%{assigns: %{device_kind: :desktop}}), do: :ok

  defp direction(:desktop), do: :desktop_to_web
  defp direction(:web), do: :web_to_desktop

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
