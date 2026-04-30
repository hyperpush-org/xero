defmodule XeroWeb.PageController do
  use XeroWeb, :controller

  def home(conn, _params) do
    render(conn, :home)
  end
end
