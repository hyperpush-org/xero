defmodule XeroWeb.RemoteDesktopChannel do
  use XeroWeb, :channel

  alias XeroWeb.RemoteChannelRateLimit

  @impl true
  def join("desktop:" <> desktop_device_id, _payload, socket) do
    if socket.assigns.device_kind == :desktop and socket.assigns.device_id == desktop_device_id do
      :telemetry.execute([:xero, :remote, :channel, :join], %{count: 1}, %{
        kind: :desktop,
        topic: socket.topic
      })

      {:ok, %{desktop_device_id: desktop_device_id}, socket}
    else
      {:error, %{reason: "unauthorized"}}
    end
  end

  @impl true
  def handle_in("session_authorized", payload, socket) do
    with :ok <- RemoteChannelRateLimit.hit(socket, "session_authorized"),
         %{"join_ref" => join_ref, "auth_topic" => auth_topic} <- payload do
      XeroWeb.Endpoint.broadcast(auth_topic, "session_authorized", %{
        "join_ref" => join_ref,
        "desktop_device_id" => socket.assigns.device_id,
        "authorized" => Map.get(payload, "authorized", true)
      })

      broadcast!(
        socket,
        "session_authorized",
        Map.put(payload, "desktop_device_id", socket.assigns.device_id)
      )

      {:reply, {:ok, %{join_ref: join_ref}}, socket}
    else
      {:error, retry_after_ms} ->
        {:reply, {:error, %{reason: "rate_limited", retry_after_ms: retry_after_ms}}, socket}

      _ ->
        {:reply, {:error, %{reason: "invalid_payload"}}, socket}
    end
  end
end
