defmodule Xero.RemoteFixtures do
  @moduledoc false

  import ExUnit.Assertions

  alias Xero.GitHubAuth

  def start_github_flow!(conn, kind, opts \\ []) do
    conn =
      Phoenix.ConnTest.dispatch(conn, XeroWeb.Endpoint, :post, "/api/github/login", %{
        "kind" => Atom.to_string(kind),
        "name" => Keyword.get(opts, :name, default_name(kind))
      })

    Phoenix.ConnTest.json_response(conn, 200)
  end

  def complete_github_flow!(conn, kind, opts \\ []) do
    started = start_github_flow!(conn, kind, opts)
    state_token = state_from_authorization_url(started["authorizationUrl"])
    session_id = Keyword.get(opts, :session_id, "session-#{kind}-#{System.unique_integer()}")

    assert :ok =
             GitHubAuth.complete_state(
               state_token,
               session_id,
               stored_github_session(opts),
               %{
                 name: Keyword.get(opts, :name, default_name(kind)),
                 user_agent: Keyword.get(opts, :user_agent)
               }
             )

    conn =
      Phoenix.ConnTest.dispatch(
        conn,
        XeroWeb.Endpoint,
        :get,
        "/api/github/session?flowId=#{started["flowId"]}"
      )

    Phoenix.ConnTest.json_response(conn, 200)
  end

  def desktop_login!(conn, opts \\ []) do
    body = complete_github_flow!(conn, :desktop, opts)
    session = body["session"]

    %{
      "account_id" => session["accountId"],
      "desktop_device_id" => session["deviceId"],
      "desktop_jwt" => session["relayToken"],
      "session_id" => body["sessionId"],
      "session" => session
    }
  end

  def web_login!(conn, opts \\ []) do
    body = complete_github_flow!(conn, :web, opts)
    session = body["session"]

    %{
      "account_id" => session["accountId"],
      "web_device_id" => session["deviceId"],
      "web_jwt" => session["relayToken"],
      "session_id" => body["sessionId"],
      "session" => session
    }
  end

  def stored_github_session(opts \\ []) do
    id = Keyword.get(opts, :github_user_id, 42)
    login = Keyword.get(opts, :github_login, "octo")

    GitHubAuth.stored_session(
      Keyword.get(opts, :access_token, "server-only-access-token"),
      "bearer",
      "read:user",
      %{
        "id" => id,
        "login" => login,
        "name" => Keyword.get(opts, :github_name, "Octo"),
        "email" => nil,
        "avatarUrl" => "https://avatars.githubusercontent.com/u/#{id}?v=4",
        "htmlUrl" => "https://github.com/#{login}"
      }
    )
  end

  def state_from_authorization_url(url) do
    url
    |> URI.parse()
    |> Map.fetch!(:query)
    |> URI.decode_query()
    |> Map.fetch!("state")
  end

  def with_github_env(fun) do
    with_env(
      %{
        "GITHUB_CLIENT_ID" => "test-github-client",
        "GITHUB_CLIENT_SECRET" => "test-github-secret",
        "GITHUB_REDIRECT_URI" => "http://127.0.0.1:4002/auth/github/callback"
      },
      fun
    )
  end

  def without_github_env(fun) do
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

  defp default_name(:desktop), do: "Test Desktop"
  defp default_name(:web), do: "Test Web"
end
