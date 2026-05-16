defmodule XeroWeb.RemoteWebSocket do
  use Phoenix.Socket

  alias Xero.Remote

  channel "account:*", XeroWeb.RemoteWebAccountChannel
  channel "session:*", XeroWeb.RemoteSessionChannel

  @impl true
  def connect(%{"token" => token}, socket, _connect_info) do
    with {:ok, device} <- Remote.authenticate_device_token(token),
         :web <- device.kind do
      {:ok,
       assign(socket,
         account_id: device.account_id,
         device_id: device.id,
         device_kind: device.kind
       )}
    else
      _ -> :error
    end
  end

  def connect(_params, _socket, _connect_info), do: :error

  @impl true
  def id(socket), do: "web:#{socket.assigns.device_id}"
end
