defmodule XeroWeb.GitHubAuthController do
  use XeroWeb, :controller

  alias Xero.GitHubAuth

  def start(conn, _params) do
    case GitHubAuth.start_login() do
      {:ok, started} ->
        json(conn, %{
          authorizationUrl: started.authorization_url,
          redirectUri: started.redirect_uri,
          flowId: started.flow_id
        })

      {:error, error} ->
        render_error(conn, :internal_server_error, error)
    end
  end

  def callback(conn, params) do
    case GitHubAuth.complete_callback(params) do
      {:ok, _session} ->
        html(conn, success_html())

      {:error, _error} ->
        html(conn, error_html())
    end
  end

  def session(conn, %{"flowId" => flow_id}) do
    render_flow_session(conn, flow_id)
  end

  def session(conn, %{"flow_id" => flow_id}) do
    render_flow_session(conn, flow_id)
  end

  def session(conn, _params) do
    session_id = conn |> get_req_header(GitHubAuth.session_header()) |> List.first()

    case GitHubAuth.get_session(session_id) do
      {:ok, nil} -> json(conn, %{session: nil})
      {:ok, session} -> json(conn, %{session: GitHubAuth.public_session(session)})
      {:error, error} -> render_error(conn, :internal_server_error, error)
    end
  end

  def delete_session(conn, _params) do
    session_id = conn |> get_req_header(GitHubAuth.session_header()) |> List.first()

    case GitHubAuth.logout(session_id) do
      :ok -> send_resp(conn, :no_content, "")
      {:error, error} -> render_error(conn, :internal_server_error, error)
    end
  end

  defp render_flow_session(conn, flow_id) do
    case GitHubAuth.poll_flow(flow_id) do
      :pending ->
        conn
        |> put_status(:accepted)
        |> json(%{status: "pending"})

      {:complete, session_id, session} ->
        json(conn, %{
          status: "ready",
          sessionId: session_id,
          session: GitHubAuth.public_session(session)
        })

      {:error, %{"code" => "github_oauth_flow_not_found"} = error} ->
        render_error(conn, :not_found, error)

      {:error, error} ->
        render_error(conn, :unprocessable_entity, error)
    end
  end

  defp render_error(conn, status, error) do
    conn
    |> put_status(status)
    |> json(%{error: error})
  end

  defp success_html do
    """
    <!doctype html>
    <html>
      <head>
        <meta charset="utf-8">
        <title>Xero - Signed in</title>
      </head>
      <body style="font-family: -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif; min-height: 100vh; display: grid; place-items: center; margin: 0;">
        <main style="max-width: 32rem; text-align: center;">
          <h1>Signed in to GitHub</h1>
          <p>You can return to Xero.</p>
        </main>
      </body>
    </html>
    """
  end

  defp error_html do
    """
    <!doctype html>
    <html>
      <head>
        <meta charset="utf-8">
        <title>Xero - Sign in failed</title>
      </head>
      <body style="font-family: -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif; min-height: 100vh; display: grid; place-items: center; margin: 0;">
        <main style="max-width: 32rem; text-align: center;">
          <h1>GitHub sign in failed</h1>
          <p>Return to Xero and try again.</p>
        </main>
      </body>
    </html>
    """
  end
end
