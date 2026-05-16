defmodule XeroWeb.RemoteDeviceController do
  use XeroWeb, :controller

  alias Xero.Remote

  action_fallback XeroWeb.RemoteFallbackController

  def index(conn, _params) do
    devices =
      conn.assigns.remote_device
      |> Remote.list_devices()
      |> Enum.map(&device_json/1)

    json(conn, %{devices: devices})
  end

  def revoke(conn, %{"id" => device_id}) do
    with {:ok, _device} <- Remote.revoke_device(conn.assigns.remote_device, device_id) do
      send_resp(conn, 204, "")
    end
  end

  defp device_json(device) do
    %{
      id: device.id,
      account_id: device.account_id,
      kind: Atom.to_string(device.kind),
      name: device.name,
      user_agent: device.user_agent,
      last_seen: device.last_seen,
      created_at: device.created_at,
      revoked_at: device.revoked_at
    }
  end
end
