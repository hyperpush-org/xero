defmodule XeroWeb.GameStatsController do
  use XeroWeb, :controller

  alias Xero.Arcade
  alias Xero.GitHubAuth

  def index(conn, _params) do
    conn
    |> session_id()
    |> Arcade.list_stats()
    |> render_result(conn)
  end

  def create(conn, params) do
    conn
    |> session_id()
    |> Arcade.record_run(params)
    |> render_result(conn)
  end

  defp session_id(conn) do
    conn
    |> get_req_header(GitHubAuth.session_header())
    |> List.first()
  end

  defp render_result({:ok, payload}, conn), do: json(conn, payload)

  defp render_result({:error, %{"code" => "github_session_required"} = error}, conn) do
    render_error(conn, :unauthorized, error)
  end

  defp render_result({:error, %{"code" => "github_session_invalid"} = error}, conn) do
    render_error(conn, :unauthorized, error)
  end

  defp render_result({:error, %{"code" => code} = error}, conn)
       when code in [
              "game_id_invalid",
              "game_score_invalid",
              "game_time_played_invalid",
              "game_run_invalid"
            ] do
    render_error(conn, :unprocessable_entity, error)
  end

  defp render_result({:error, error}, conn), do: render_error(conn, :internal_server_error, error)

  defp render_error(conn, status, error) do
    conn
    |> put_status(status)
    |> json(%{error: error})
  end
end
