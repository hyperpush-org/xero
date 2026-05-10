defmodule XeroWeb.HealthController do
  use XeroWeb, :controller

  def show(conn, _params) do
    json(conn, %{status: "ok"})
  end
end
