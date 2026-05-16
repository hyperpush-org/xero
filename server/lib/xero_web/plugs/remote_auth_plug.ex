defmodule XeroWeb.RemoteAuthPlug do
  @moduledoc false

  import Plug.Conn

  alias Xero.Remote

  @behaviour Plug

  @impl true
  def init(opts), do: opts

  @impl true
  def call(conn, opts) do
    required_kind = Keyword.get(opts, :kind)

    with {:ok, token} <- bearer_token(conn),
         {:ok, device} <- Remote.authenticate_device_token(token),
         true <- is_nil(required_kind) or device.kind == required_kind do
      assign(conn, :remote_device, device)
    else
      _ ->
        conn
        |> put_resp_content_type("application/json")
        |> send_resp(401, Jason.encode!(%{error: "unauthorized"}))
        |> halt()
    end
  end

  defp bearer_token(conn) do
    case get_req_header(conn, "authorization") do
      ["Bearer " <> token | _] when token != "" -> {:ok, token}
      _ -> {:error, :missing_bearer}
    end
  end
end
