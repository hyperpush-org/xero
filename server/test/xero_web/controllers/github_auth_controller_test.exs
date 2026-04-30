defmodule XeroWeb.GitHubAuthControllerTest do
  use XeroWeb.ConnCase

  alias Xero.GitHubAuth
  alias Xero.GitHubAuth.Session
  alias Xero.Repo

  setup do
    GitHubAuth.reset!()
    :ok
  end

  test "POST /api/github/login creates a server-owned OAuth flow", %{conn: conn} do
    with_github_env(fn ->
      conn = post(conn, ~p"/api/github/login")
      body = json_response(conn, 200)

      assert body["flowId"]
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
      start_conn = post(conn, ~p"/api/github/login")
      start_body = json_response(start_conn, 200)

      pending_conn = get(conn, ~p"/api/github/session?flowId=#{start_body["flowId"]}")
      assert json_response(pending_conn, 202) == %{"status" => "pending"}

      state_token =
        start_body["authorizationUrl"]
        |> URI.parse()
        |> Map.fetch!(:query)
        |> URI.decode_query()
        |> Map.fetch!("state")

      stored_session =
        GitHubAuth.stored_session(
          "server-only-access-token",
          "bearer",
          "read:user",
          %{
            "id" => 42,
            "login" => "octo",
            "name" => "Octo",
            "email" => nil,
            "avatarUrl" => "https://avatars.githubusercontent.com/u/42?v=4",
            "htmlUrl" => "https://github.com/octo"
          }
        )

      assert :ok = GitHubAuth.complete_state(state_token, "session-test", stored_session)
      persisted = Repo.get!(Session, "session-test")
      refute persisted.encrypted_access_token =~ "server-only-access-token"

      ready_conn = get(conn, ~p"/api/github/session?flowId=#{start_body["flowId"]}")
      ready_body = json_response(ready_conn, 200)

      assert ready_body["status"] == "ready"
      assert ready_body["sessionId"] == "session-test"
      assert ready_body["session"]["user"]["login"] == "octo"
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

      signed_out_conn =
        conn
        |> put_req_header(GitHubAuth.session_header(), "session-test")
        |> get(~p"/api/github/session")

      assert json_response(signed_out_conn, 200) == %{"session" => nil}
    end)
  end

  test "session lookup survives cleared server memory", %{conn: conn} do
    with_github_env(fn ->
      start_conn = post(conn, ~p"/api/github/login")
      start_body = json_response(start_conn, 200)

      state_token =
        start_body["authorizationUrl"]
        |> URI.parse()
        |> Map.fetch!(:query)
        |> URI.decode_query()
        |> Map.fetch!("state")

      stored_session =
        GitHubAuth.stored_session(
          "durable-server-token",
          "bearer",
          "read:user user:email",
          %{
            "id" => 42,
            "login" => "octo",
            "name" => "Octo",
            "email" => nil,
            "avatarUrl" => "https://avatars.githubusercontent.com/u/42?v=4",
            "htmlUrl" => "https://github.com/octo"
          }
        )

      assert :ok = GitHubAuth.complete_state(state_token, "session-durable", stored_session)
      assert :ok = GitHubAuth.clear_in_memory_state_for_test!()

      current_conn =
        conn
        |> put_req_header(GitHubAuth.session_header(), "session-durable")
        |> get(~p"/api/github/session")

      body = json_response(current_conn, 200)
      assert body["session"]["user"]["login"] == "octo"
      assert body["session"]["scope"] == "read:user user:email"
      refute inspect(body) =~ "durable-server-token"
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

  defp with_github_env(fun) do
    with_env(
      %{
        "GITHUB_CLIENT_ID" => "test-github-client",
        "GITHUB_CLIENT_SECRET" => "test-github-secret",
        "GITHUB_REDIRECT_URI" => "http://127.0.0.1:4002/auth/github/callback"
      },
      fun
    )
  end

  defp without_github_env(fun) do
    with_env(
      %{
        "GITHUB_CLIENT_ID" => nil,
        "GITHUB_CLIENT_SECRET" => nil,
        "GITHUB_REDIRECT_URI" => nil,
        "XERO_GITHUB_AUTH_SKIP_DOTENV" => "1"
      },
      fun
    )
  end

  defp with_env(vars, fun) do
    previous = Map.new(vars, fn {key, _value} -> {key, System.get_env(key)} end)

    Enum.each(vars, fn
      {key, nil} -> System.delete_env(key)
      {key, value} -> System.put_env(key, value)
    end)

    try do
      fun.()
    after
      Enum.each(previous, fn
        {key, nil} -> System.delete_env(key)
        {key, value} -> System.put_env(key, value)
      end)
    end
  end
end
