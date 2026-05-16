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

      assert_push "session_attached", %{
        web_device_id: ^web_device_id,
        session_id: "session-1",
        last_seq: 4
      }

      ref = push(web_session, "frame", %{"body" => "opaque"})
      assert_reply ref, :ok

      assert_push "frame", %{
        from_kind: "web",
        direction: "web_to_desktop",
        payload: %{"body" => "opaque"}
      }
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
