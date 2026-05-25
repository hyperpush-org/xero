defmodule XeroWeb.RemoteChannelTest do
  use XeroWeb.ChannelCase

  import Xero.RemoteFixtures

  setup do
    Xero.GitHubAuth.reset!()
    :ok
  end

  test "desktop and web clients for one GitHub account exchange opaque frames", %{conn: conn} do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, desktop_session_reply, _desktop_session} =
        subscribe_and_join(
          desktop_socket,
          "session:#{desktop["desktop_device_id"]}:session-1",
          %{}
        )

      assert desktop_session_reply.session_id == "session-1"

      {:ok, web_socket} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web["web_jwt"]})

      {:ok, account_reply, web_socket} =
        subscribe_and_join(web_socket, "account:#{desktop["account_id"]}", %{})

      assert account_reply.account_id == desktop["account_id"]

      join_task =
        Task.async(fn ->
          subscribe_and_join(web_socket, "session:#{desktop["desktop_device_id"]}:session-1", %{
            "join_ref" => "join-1",
            "last_seq" => 4
          })
        end)

      assert_push "session_join_requested", %{
        auth_topic: auth_topic,
        web_device_id: web_device_id,
        session_id: "session-1",
        join_ref: "join-1",
        last_seq: 4
      }

      assert web_device_id == web["web_device_id"]

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-1",
          "auth_topic" => auth_topic,
          "authorized" => true
        })

      assert_reply ref, :ok

      {:ok, web_session_reply, web_session} = Task.await(join_task)

      assert web_session_reply.session_id == "session-1"

      refute_push "session_attached", %{}, 50

      ref = push(web_session, "frame", %{"body" => "opaque"})
      assert_reply ref, :ok

      assert_push "frame", %{
        from_kind: "web",
        direction: "web_to_desktop",
        payload: %{"body" => "opaque"}
      }
    end)
  end

  test "web clients can subscribe to a desktop session list and notify the desktop", %{
    conn: conn
  } do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, _desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, web_socket} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web["web_jwt"]})

      {:ok, reply, _web_session} =
        subscribe_and_join(
          web_socket,
          "session:#{desktop["desktop_device_id"]}:__sessions__",
          %{"last_seq" => 0}
        )

      assert reply.desktop_device_id == desktop["desktop_device_id"]
      assert reply.session_id == "__sessions__"

      assert_push "session_join_requested", %{
        auth_topic: auth_topic,
        web_device_id: web_device_id,
        session_id: "__sessions__",
        last_seq: 0
      }

      assert is_binary(auth_topic)
      assert web_device_id == web["web_device_id"]
    end)
  end

  test "web clients can subscribe to a desktop theme channel and notify the desktop", %{
    conn: conn
  } do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, _desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, web_socket} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web["web_jwt"]})

      {:ok, reply, _web_session} =
        subscribe_and_join(
          web_socket,
          "session:#{desktop["desktop_device_id"]}:__theme__",
          %{"last_seq" => 0}
        )

      assert reply.desktop_device_id == desktop["desktop_device_id"]
      assert reply.session_id == "__theme__"

      assert_push "session_join_requested", %{
        auth_topic: auth_topic,
        web_device_id: web_device_id,
        session_id: "__theme__",
        last_seq: 0
      }

      assert is_binary(auth_topic)
      assert web_device_id == web["web_device_id"]
    end)
  end

  test "web account channel receives desktop online presence", %{conn: conn} do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)

      {:ok, web_socket} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web["web_jwt"]})

      {:ok, account_reply, _web_account} =
        subscribe_and_join(web_socket, "account:#{desktop["account_id"]}", %{})

      assert account_reply.account_id == desktop["account_id"]
      assert_push "presence_state", %{}

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, _desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      desktop_device_id = desktop["desktop_device_id"]

      assert_push "presence_diff", %{
        joins: %{
          ^desktop_device_id => %{metas: [%{kind: "desktop", online_at: online_at} | _]}
        },
        leaves: %{}
      }

      assert is_binary(online_at)
    end)
  end

  test "web account channel includes already-online desktops in initial presence", %{conn: conn} do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, _desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, web_socket} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web["web_jwt"]})

      {:ok, _account_reply, _web_account} =
        subscribe_and_join(web_socket, "account:#{desktop["account_id"]}", %{})

      desktop_device_id = desktop["desktop_device_id"]

      assert_push "presence_state", %{
        ^desktop_device_id => %{metas: [%{kind: "desktop", online_at: online_at} | _]}
      }

      assert is_binary(online_at)
    end)
  end

  test "desktop stream frames are not throttled by the web command limit", %{conn: conn} do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)
      original_rate_limit_config = Application.get_env(:xero, Xero.RateLimiter, [])
      Application.put_env(:xero, Xero.RateLimiter, per_minute: 1)

      on_exit(fn ->
        Application.put_env(:xero, Xero.RateLimiter, original_rate_limit_config)
      end)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, _desktop_session_reply, desktop_session} =
        subscribe_and_join(
          desktop_socket,
          "session:#{desktop["desktop_device_id"]}:session-1",
          %{}
        )

      {:ok, web_socket} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web["web_jwt"]})

      join_task =
        Task.async(fn ->
          subscribe_and_join(web_socket, "session:#{desktop["desktop_device_id"]}:session-1", %{
            "join_ref" => "join-rate-limit"
          })
        end)

      assert_push "session_join_requested", %{
        auth_topic: auth_topic,
        session_id: "session-1",
        join_ref: "join-rate-limit"
      }

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-rate-limit",
          "auth_topic" => auth_topic,
          "authorized" => true
        })

      assert_reply ref, :ok
      {:ok, _web_session_reply, web_session} = Task.await(join_task)
      refute_push "session_attached", %{}, 50

      first_ref = push(desktop_session, "frame", %{"delta" => "first"})
      second_ref = push(desktop_session, "frame", %{"delta" => "second"})

      assert_reply first_ref, :ok
      assert_reply second_ref, :ok

      assert_push "frame", %{from_kind: "desktop", payload: %{"delta" => "first"}}
      assert_push "frame", %{from_kind: "desktop", payload: %{"delta" => "second"}}

      allowed_ref = push(web_session, "frame", %{"command" => "allowed"})
      throttled_ref = push(web_session, "frame", %{"command" => "throttled"})

      assert_reply allowed_ref, :ok
      assert_reply throttled_ref, :error, %{reason: "rate_limited"}
    end)
  end

  test "web clients cannot connect with invalid tokens or join another account's desktop", %{
    conn: conn
  } do
    with_github_env(fn ->
      desktop_a = desktop_login!(conn, github_user_id: 42, github_login: "octo")
      desktop_b = desktop_login!(conn, github_user_id: 99, github_login: "mona")
      web = web_login!(conn, github_user_id: 42, github_login: "octo")

      assert desktop_a["account_id"] != desktop_b["account_id"]
      assert :error = connect(XeroWeb.RemoteWebSocket, %{"token" => "not-a-token"})

      {:ok, web_socket} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web["web_jwt"]})

      assert {:error, %{reason: "unauthorized"}} =
               subscribe_and_join(
                 web_socket,
                 "session:#{desktop_b["desktop_device_id"]}:session-1",
                 %{}
               )
    end)
  end
end
