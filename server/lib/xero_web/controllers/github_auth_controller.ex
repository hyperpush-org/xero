defmodule XeroWeb.GitHubAuthController do
  use XeroWeb, :controller

  alias Xero.GitHubAuth
  alias Xero.Remote

  @web_session_cookie "_xero_web_session"
  @csrf_cookie "xero_csrf_token"

  def start(conn, params) do
    kind = Map.get(params, "kind") || "desktop"

    case GitHubAuth.start_login(kind, %{
           name: Map.get(params, "name"),
           user_agent: user_agent(conn),
           redirect_to: Map.get(params, "redirectTo") || Map.get(params, "redirect_to")
         }) do
      {:ok, started} ->
        json(conn, %{
          authorizationUrl: started.authorization_url,
          redirectUri: started.redirect_uri,
          flowId: started.flow_id,
          kind: started.kind
        })

      {:error, error} ->
        render_error(conn, :internal_server_error, error)
    end
  end

  def callback(conn, params) do
    case GitHubAuth.complete_callback(params) do
      {:ok, %{kind: "web", session_id: session_id, session: session} = result} ->
        conn
        |> put_web_session_cookies(session_id, session)
        |> redirect(external: result.redirect_to || web_app_url())

      {:ok, _result} ->
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
    session_id = request_session_id(conn)

    case GitHubAuth.get_session(session_id) do
      {:ok, nil} -> json(conn, %{session: nil})
      {:ok, session} -> json(conn, %{session: GitHubAuth.public_session(session)})
      {:error, error} -> render_error(conn, :internal_server_error, error)
    end
  end

  def delete_session(conn, _params) do
    session_id = request_session_id(conn)

    case GitHubAuth.logout(session_id) do
      :ok ->
        conn
        |> clear_web_session_cookies()
        |> send_resp(:no_content, "")

      {:error, error} ->
        render_error(conn, :internal_server_error, error)
    end
  end

  def refresh_relay_token(conn, params) do
    result =
      case bearer_token(conn) || Map.get(params, "relayToken") || Map.get(params, "relay_token") do
        token when is_binary(token) and token != "" ->
          with {:ok, device} <- Remote.authenticate_device_token(token) do
            Remote.refresh_relay_token(device)
          end

        _ ->
          with session_id when is_binary(session_id) <- request_session_id(conn),
               {:ok, session} when is_map(session) <- GitHubAuth.get_session(session_id),
               {:ok, device} <- Remote.device_for_session(session) do
            Remote.refresh_relay_token(device)
          else
            _ -> {:error, :unauthorized}
          end
      end

    case result do
      {:ok, payload} ->
        json(conn, %{
          relayToken: payload.token,
          relayTokenExpiresAt: payload.expires_at,
          deviceId: payload.device_id,
          deviceKind: payload.device_kind,
          accountId: payload.account_id,
          account: %{
            githubLogin: payload.account.github_login,
            githubAvatarUrl: payload.account.github_avatar_url
          }
        })

      {:error, _reason} ->
        conn
        |> put_status(:unauthorized)
        |> json(%{error: "unauthorized"})
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

  defp request_session_id(conn) do
    header_session_id =
      conn
      |> get_req_header(GitHubAuth.session_header())
      |> List.first()

    case header_session_id do
      nil ->
        conn
        |> fetch_cookies()
        |> Map.get(:cookies, %{})
        |> Map.get(@web_session_cookie)

      session_id ->
        session_id
    end
  end

  defp bearer_token(conn) do
    case get_req_header(conn, "authorization") do
      ["Bearer " <> token | _] when token != "" -> token
      _ -> nil
    end
  end

  defp put_web_session_cookies(conn, session_id, session) do
    csrf_token = Map.get(session, "csrfToken")

    conn
    |> put_resp_cookie(@web_session_cookie, session_id, web_cookie_opts(http_only: true))
    |> maybe_put_csrf_cookie(csrf_token)
  end

  defp maybe_put_csrf_cookie(conn, csrf_token) when is_binary(csrf_token) and csrf_token != "" do
    put_resp_cookie(conn, @csrf_cookie, csrf_token, web_cookie_opts(http_only: false))
  end

  defp maybe_put_csrf_cookie(conn, _csrf_token), do: conn

  defp clear_web_session_cookies(conn) do
    conn
    |> delete_resp_cookie(@web_session_cookie, web_cookie_opts(http_only: true))
    |> delete_resp_cookie(@csrf_cookie, web_cookie_opts(http_only: false))
  end

  defp web_cookie_opts(extra) do
    [
      domain: cookie_domain(),
      secure: secure_cookie?(),
      http_only: Keyword.fetch!(extra, :http_only),
      same_site: "Lax"
    ]
    |> Enum.reject(fn {_key, value} -> is_nil(value) end)
  end

  defp cookie_domain do
    case Application.fetch_env(:xero, :web_session_cookie_domain) do
      {:ok, domain} ->
        domain

      :error ->
        System.get_env("XERO_WEB_SESSION_COOKIE_DOMAIN") || ".xeroshell.com"
    end
  end

  defp secure_cookie? do
    Application.get_env(:xero, :web_session_cookie_secure, true)
  end

  defp web_app_url do
    Application.get_env(:xero, :web_app_url) ||
      System.get_env("XERO_WEB_APP_URL") ||
      "https://cloud.xeroshell.com"
  end

  defp user_agent(conn) do
    conn |> get_req_header("user-agent") |> List.first()
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
