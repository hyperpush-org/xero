defmodule JoeWeb.PageController do
  use JoeWeb, :controller

  def home(conn, _params) do
    render(conn, :home)
  end
end
