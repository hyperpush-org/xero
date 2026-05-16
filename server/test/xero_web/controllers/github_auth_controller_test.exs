defmodule XeroWeb.GitHubAuthControllerTest do
  use XeroWeb.ConnCase

  import Xero.RemoteFixtures

  alias Xero.GitHubAuth
  alias Xero.GitHubAuth.Session
  alias Xero.Remote.Device
  alias Xero.Repo

  setup do
    GitHubAuth.reset!()
    :ok
  end

  test "POST /api/github/login creates a server-owned OAuth flow", %{conn: conn} do
    with_github_env(fn ->
      conn = post(conn, ~p"/api/github/login", %{kind: "desktop"})
      body = json_response(conn, 200)

      assert body["flowId"]
      assert body["kind"] == "desktop"
      assert body["redirectUri"] == "http://127.0.0.1:4002/auth/github/callback"

      uri = URI.parse(body["authorizationUrl"])
      query = URI.decode_query(uri.query)

      assert uri.scheme == "https"
      assert uri.host == "github.com"
      assert uri.path == "/login/oauth/authorize"
      assert query["client_id"] == "test-github-client"
      assert query["redirect_uri"] == "http://127.0.0.1:4002/auth/github/callback"
      assert query["scope"] == "read:user user:email"
      assert query["state"]
    end)
  end

  test "POST /api/github/login reports server-side missing configuration", %{conn: conn} do
    without_github_env(fn ->
      conn = post(conn, ~p"/api/github/login")
      body = json_response(conn, 500)

      assert body["error"]["code"] == "github_oauth_unconfigured"
      assert body["error"]["message"] =~ "Xero server"
    end)
  end

  test "session polling returns completed public session and keeps token server-side", %{
    conn: conn
  } do
    with_github_env(fn ->
      start_conn = post(conn, ~p"/api/github/login", %{kind: "desktop"})
      start_body = json_response(start_conn, 200)

      pending_conn = get(conn, ~p"/api/github/session?flowId=#{start_body["flowId"]}")
      assert json_response(pending_conn, 202) == %{"status" => "pending"}

      state_token =
        start_body["authorizationUrl"]
        |> URI.parse()
        |> Map.fetch!(:query)
        |> URI.decode_query()
        |> Map.fetch!("state")

      stored_session = stored_github_session(access_token: "server-only-access-token")

      assert :ok = GitHubAuth.complete_state(state_token, "session-test", stored_session)
      persisted = Repo.get!(Session, "session-test")
      refute persisted.encrypted_access_token =~ "server-only-access-token"
      assert persisted.kind == "desktop"
      assert persisted.account_id
      assert persisted.device_id

      ready_conn = get(conn, ~p"/api/github/session?flowId=#{start_body["flowId"]}")
      ready_body = json_response(ready_conn, 200)

      assert ready_body["status"] == "ready"
      assert ready_body["sessionId"] == "session-test"
      assert ready_body["session"]["user"]["login"] == "octo"
      assert ready_body["session"]["kind"] == "desktop"
      assert ready_body["session"]["accountId"] == persisted.account_id
      assert ready_body["session"]["deviceId"] == persisted.device_id
      assert ready_body["session"]["relayToken"]
      refute Map.has_key?(ready_body["session"], "accessToken")
      refute inspect(ready_body) =~ "server-only-access-token"

      current_conn =
        conn
        |> put_req_header(GitHubAuth.session_header(), "session-test")
        |> get(~p"/api/github/session")

      assert json_response(current_conn, 200)["session"]["user"]["login"] == "octo"

      logout_conn =
        conn
        |> put_req_header(GitHubAuth.session_header(), "session-test")
        |> delete(~p"/api/github/session")

      assert response(logout_conn, 204) == ""
      assert Repo.get(Session, "session-test") == nil
      assert Repo.get!(Device, persisted.device_id).revoked_at

      signed_out_conn =
        conn
        |> put_req_header(GitHubAuth.session_header(), "session-test")
        |> get(~p"/api/github/session")

      assert json_response(signed_out_conn, 200) == %{"session" => nil}
    end)
  end

  test "session lookup survives cleared server memory", %{conn: conn} do
    with_github_env(fn ->
      start_conn = post(conn, ~p"/api/github/login", %{kind: "desktop"})
      start_body = json_response(start_conn, 200)

      state_token =
        start_body["authorizationUrl"]
        |> URI.parse()
        |> Map.fetch!(:query)
        |> URI.decode_query()
        |> Map.fetch!("state")

      stored_session =
        stored_github_session(access_token: "durable-server-token")
        |> Map.put(:scope, "read:user user:email")

      assert :ok = GitHubAuth.complete_state(state_token, "session-durable", stored_session)
      assert :ok = GitHubAuth.clear_in_memory_state_for_test!()

      current_conn =
        conn
        |> put_req_header(GitHubAuth.session_header(), "session-durable")
        |> get(~p"/api/github/session")

      body = json_response(current_conn, 200)
      assert body["session"]["user"]["login"] == "octo"
      assert body["session"]["scope"] == "read:user user:email"
      assert body["session"]["relayToken"]
      refute inspect(body) =~ "durable-server-token"
    end)
  end

  test "desktop and web logins for the same GitHub user reuse one account", %{conn: conn} do
    with_github_env(fn ->
      desktop = complete_github_flow!(conn, :desktop, session_id: "desktop-session")
      web = complete_github_flow!(conn, :web, session_id: "web-session")

      assert desktop["session"]["accountId"] == web["session"]["accountId"]
      assert desktop["session"]["deviceId"] != web["session"]["deviceId"]
      assert desktop["session"]["kind"] == "desktop"
      assert web["session"]["kind"] == "web"

      devices_conn =
        conn
        |> put_req_header("authorization", "Bearer #{desktop["session"]["relayToken"]}")
        |> get(~p"/api/devices")

      device_kinds =
        devices_conn
        |> json_response(200)
        |> Map.fetch!("devices")
        |> Enum.map(& &1["kind"])
        |> Enum.sort()

      assert device_kinds == ["desktop", "web"]
    end)
  end

  test "web session cookies are host-only when the configured domain is nil", %{conn: conn} do
    original_domain = Application.fetch_env(:xero, :web_session_cookie_domain)
    Application.put_env(:xero, :web_session_cookie_domain, nil)

    on_exit(fn ->
      case original_domain do
        {:ok, value} -> Application.put_env(:xero, :web_session_cookie_domain, value)
        :error -> Application.delete_env(:xero, :web_session_cookie_domain)
      end
    end)

    conn =
      conn
      |> put_req_cookie("_xero_web_session", "missing-session")
      |> delete(~p"/api/github/session")

    web_session_cookie =
      conn
      |> get_resp_header("set-cookie")
      |> Enum.find(&String.starts_with?(&1, "_xero_web_session="))

    assert web_session_cookie
    refute String.contains?(String.downcase(web_session_cookie), "domain=")
  end

  test "relay token refresh accepts bearer JWT and rejects revoked devices", %{conn: conn} do
    with_github_env(fn ->
      desktop = complete_github_flow!(conn, :desktop, session_id: "refresh-session")
      token = desktop["session"]["relayToken"]

      refresh_conn =
        conn
        |> put_req_header("authorization", "Bearer #{token}")
        |> post(~p"/api/relay/token/refresh", %{})

      refresh_body = json_response(refresh_conn, 200)
      assert refresh_body["relayToken"]
      assert refresh_body["relayTokenExpiresAt"]

      revoke_conn =
        conn
        |> put_req_header("authorization", "Bearer #{token}")
        |> post(~p"/api/devices/#{desktop["session"]["deviceId"]}/revoke", %{})

      assert response(revoke_conn, 204)

      rejected_conn =
        conn
        |> put_req_header("authorization", "Bearer #{refresh_body["relayToken"]}")
        |> post(~p"/api/relay/token/refresh", %{})

      assert json_response(rejected_conn, 401)["error"] == "unauthorized"
    end)
  end

  test "browser callback stores GitHub rejection on the server flow", %{conn: conn} do
    with_github_env(fn ->
      start_conn = post(conn, ~p"/api/github/login")
      start_body = json_response(start_conn, 200)

      state_token =
        start_body["authorizationUrl"]
        |> URI.parse()
        |> Map.fetch!(:query)
        |> URI.decode_query()
        |> Map.fetch!("state")

      callback_conn =
        get(
          conn,
          ~p"/auth/github/callback?state=#{state_token}&error=access_denied&error_description=Denied"
        )

      assert html_response(callback_conn, 200) =~ "GitHub sign in failed"

      poll_conn = get(conn, ~p"/api/github/session?flowId=#{start_body["flowId"]}")
      body = json_response(poll_conn, 422)

      assert body["error"]["code"] == "github_oauth_rejected"
      assert body["error"]["message"] =~ "access_denied"
    end)
  end
end
