defmodule XeroWeb.RemoteSessionChannel do
  use XeroWeb, :channel

  alias Xero.Remote
  alias XeroWeb.RemoteChannelRateLimit

  @impl true
  def join("session:" <> rest, payload, socket) do
    with [desktop_device_id, session_id] <- String.split(rest, ":", parts: 2),
         true <- authorized_for_desktop?(socket, desktop_device_id),
         :ok <- authorize_session_join(socket, desktop_device_id, session_id, payload) do
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

      maybe_broadcast_session_attached(socket, desktop_device_id, session_id, payload)

      {:ok, %{desktop_device_id: desktop_device_id, session_id: session_id}, socket}
    else
      _ -> {:error, %{reason: "unauthorized"}}
    end
  end

  @impl true
  def handle_in("frame", payload, socket) when is_map(payload) do
    case RemoteChannelRateLimit.hit(socket, "frame") do
      :ok ->
        direction = direction(socket.assigns.device_kind)
        bytes = payload_size(payload)

        :telemetry.execute([:xero, :remote, :frame, :forwarded], %{bytes: bytes, count: 1}, %{
          direction: direction,
          session_id: socket.assigns.session_id,
          desktop_device_id: socket.assigns.desktop_device_id
        })

        broadcast_from!(socket, "frame", %{
          from_device_id: socket.assigns.device_id,
          from_kind: Atom.to_string(socket.assigns.device_kind),
          direction: Atom.to_string(direction),
          payload: payload
        })

        {:reply, :ok, socket}

      {:error, retry_after_ms} ->
        {:reply, {:error, %{reason: "rate_limited", retry_after_ms: retry_after_ms}}, socket}
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
       do: :ok

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
        payload: %{"join_ref" => ^join_ref, "authorized" => true}
      } ->
        :ok

      %Phoenix.Socket.Broadcast{event: "session_authorized", payload: %{"join_ref" => ^join_ref}} ->
        {:error, :session_not_visible}
    after
      2_000 -> {:error, :session_authorization_timeout}
    end
  end

  defp authorize_session_join(_socket, _desktop_device_id, _session_id, _payload),
    do: {:error, :unauthorized}

  defp maybe_broadcast_session_attached(
         %{assigns: %{device_kind: :web, device_id: web_device_id}},
         desktop_device_id,
         session_id,
         payload
       ) do
    XeroWeb.Endpoint.broadcast("desktop:#{desktop_device_id}", "session_attached", %{
      web_device_id: web_device_id,
      session_id: session_id,
      last_seq: Map.get(payload, "last_seq")
    })
  end

  defp maybe_broadcast_session_attached(_socket, _desktop_device_id, _session_id, _payload),
    do: :ok

  defp direction(:desktop), do: :desktop_to_web
  defp direction(:web), do: :web_to_desktop

  defp payload_size(payload) do
    payload
    |> Jason.encode!()
    |> byte_size()
  end
end
