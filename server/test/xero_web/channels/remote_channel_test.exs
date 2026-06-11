defmodule XeroWeb.RemoteChannelTest do
  use XeroWeb.ChannelCase

  import Xero.RemoteFixtures

  setup do
    Xero.GitHubAuth.reset!()
    Xero.Remote.ControlSessionRegistry.reset!()
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

  test "session joins include short-lived WebRTC ICE servers", %{conn: conn} do
    original_turn_config = Application.get_env(:xero, Xero.Remote.Turn)

    Application.put_env(:xero, Xero.Remote.Turn,
      stun_urls: ["stun:stun.example.test:3478"],
      turn_urls: ["turn:turn.example.test:3478?transport=udp"],
      shared_secret: "relay-secret",
      ttl_seconds: 600
    )

    on_exit(fn ->
      case original_turn_config do
        nil -> Application.delete_env(:xero, Xero.Remote.Turn)
        value -> Application.put_env(:xero, Xero.Remote.Turn, value)
      end
    end)

    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, _desktop_session_reply, _desktop_session} =
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
            "join_ref" => "join-ice"
          })
        end)

      assert_push "session_join_requested", %{auth_topic: auth_topic, join_ref: "join-ice"}

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-ice",
          "auth_topic" => auth_topic,
          "authorized" => true,
          "run_id" => "run-ice"
        })

      assert_reply ref, :ok
      {:ok, web_session_reply, _web_session} = Task.await(join_task)

      assert [
               %{urls: ["stun:stun.example.test:3478"]},
               %{
                 urls: ["turn:turn.example.test:3478?transport=udp"],
                 username: username,
                 credential: credential,
                 credential_type: "password",
                 ttl_seconds: 600
               }
             ] = web_session_reply.ice_servers

      assert credential == :crypto.mac(:hmac, :sha, "relay-secret", username) |> Base.encode64()
      assert is_binary(web_session_reply.stream_token)
      assert web_session_reply.stream_run_id == "run-ice"
    end)
  end

  test "web stream and manual-control commands require the session stream token", %{conn: conn} do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, _desktop_session_reply, _desktop_session} =
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
            "join_ref" => "join-stream-token"
          })
        end)

      assert_push "session_join_requested", %{
        auth_topic: auth_topic,
        join_ref: "join-stream-token"
      }

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-stream-token",
          "auth_topic" => auth_topic,
          "authorized" => true,
          "run_id" => "run-1"
        })

      assert_reply ref, :ok
      {:ok, web_session_reply, web_session} = Task.await(join_task)
      assert is_binary(web_session_reply.stream_token)
      assert web_session_reply.stream_run_id == "run-1"
      attach_remote_command_telemetry(self())

      missing_token_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_stream_request",
          "clientCommandId" => "cmd-missing-token",
          "payload" => %{"quality" => "balanced"}
        })

      assert_reply missing_token_ref, :error, %{reason: "invalid_stream_token"}

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_stream_request",
        clientCommandId: "cmd-missing-token",
        outcome: "rejected",
        reason: "invalid_stream_token"
      }

      assert_receive {:remote_command_telemetry,
                      [:xero, :remote, :computer_use, :command, :rejected],
                      %{count: 1, bytes: rejected_bytes},
                      %{
                        family: :stream,
                        kind: "computer_use_stream_request",
                        direction: :web_to_desktop,
                        reason: "invalid_stream_token"
                      }}

      assert rejected_bytes > 0

      valid_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_stream_request",
          "clientCommandId" => "cmd-run-mismatch",
          "payload" => %{
            "quality" => "balanced",
            "streamToken" => web_session_reply.stream_token
          }
        })

      assert_reply valid_ref, :error, %{reason: "invalid_stream_token"}

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_stream_request",
        clientCommandId: "cmd-run-mismatch",
        outcome: "rejected",
        reason: "invalid_stream_token"
      }

      assert_receive {:remote_command_telemetry,
                      [:xero, :remote, :computer_use, :command, :rejected],
                      %{count: 1, bytes: run_mismatch_bytes},
                      %{
                        family: :stream,
                        kind: "computer_use_stream_request",
                        direction: :web_to_desktop,
                        reason: "invalid_stream_token"
                      }}

      assert run_mismatch_bytes > 0

      valid_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_stream_request",
          "clientCommandId" => "cmd-valid-stream",
          "payload" => %{
            "quality" => "balanced",
            "runId" => "run-1",
            "streamToken" => web_session_reply.stream_token
          }
        })

      assert_reply valid_ref, :ok

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_stream_request",
        clientCommandId: "cmd-valid-stream",
        outcome: "accepted",
        priority: "reliable_idempotent"
      }

      assert_receive {:remote_command_telemetry,
                      [:xero, :remote, :computer_use, :command, :forwarded],
                      %{count: 1, bytes: stream_bytes},
                      %{
                        family: :stream,
                        kind: "computer_use_stream_request",
                        direction: :web_to_desktop,
                        reason: "none"
                      }}

      assert stream_bytes > 0

      assert_push "frame", %{
        from_kind: "web",
        direction: "web_to_desktop",
        payload: %{
          "kind" => "computer_use_stream_request",
          "payload" => %{
            "quality" => "balanced",
            "runId" => "run-1",
            "streamToken" => stream_token
          }
        }
      }

      assert stream_token == web_session_reply.stream_token

      manual_payload = %{
        "runId" => "run-1",
        "streamToken" => web_session_reply.stream_token,
        "manualControlId" => "manual-web-1",
        "action" => "type_text",
        "text" => "hello"
      }

      manual_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_manual_control_input",
          "clientCommandId" => "cmd-manual-input",
          "payload" => manual_payload
        })

      assert_reply manual_ref, :ok

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_manual_control_input",
        clientCommandId: "cmd-manual-input",
        outcome: "accepted",
        priority: "critical_reliable"
      }

      assert_receive {:remote_command_telemetry,
                      [:xero, :remote, :computer_use, :command, :forwarded],
                      %{count: 1, bytes: manual_bytes},
                      %{
                        family: :manual_control,
                        kind: "computer_use_manual_control_input",
                        direction: :web_to_desktop,
                        reason: "none"
                      }}

      assert manual_bytes > 0

      assert_push "frame", %{
        from_kind: "web",
        direction: "web_to_desktop",
        payload: %{
          "kind" => "computer_use_manual_control_input",
          "payload" => ^manual_payload
        }
      }
    end)
  end

  test "expired web Computer Use commands are rejected before relay forwarding", %{conn: conn} do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, _desktop_session_reply, _desktop_session} =
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
            "join_ref" => "join-expired-command"
          })
        end)

      assert_push "session_join_requested", %{
        auth_topic: auth_topic,
        join_ref: "join-expired-command"
      }

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-expired-command",
          "auth_topic" => auth_topic,
          "authorized" => true,
          "run_id" => "run-1"
        })

      assert_reply ref, :ok
      {:ok, web_session_reply, web_session} = Task.await(join_task)

      expired_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_manual_control_input",
          "clientCommandId" => "cmd-expired",
          "expiresAt" => System.system_time(:millisecond) - 1,
          "payload" => %{
            "runId" => "run-1",
            "streamToken" => web_session_reply.stream_token,
            "manualControlId" => "manual-web-1",
            "action" => "mouse_click",
            "x" => 10,
            "y" => 10
          }
        })

      assert_reply expired_ref, :error, %{reason: "stale_command"}

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_manual_control_input",
        clientCommandId: "cmd-expired",
        outcome: "stale",
        reason: "stale_command"
      }

      refute_push "frame", %{payload: %{"clientCommandId" => "cmd-expired"}}, 50
    end)
  end

  test "oversized web Computer Use commands are rejected before relay forwarding", %{conn: conn} do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, _desktop_session_reply, _desktop_session} =
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
            "join_ref" => "join-oversized-command"
          })
        end)

      assert_push "session_join_requested", %{
        auth_topic: auth_topic,
        join_ref: "join-oversized-command"
      }

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-oversized-command",
          "auth_topic" => auth_topic,
          "authorized" => true,
          "run_id" => "run-1"
        })

      assert_reply ref, :ok
      {:ok, web_session_reply, web_session} = Task.await(join_task)

      oversized_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_manual_control_input",
          "clientCommandId" => "cmd-oversized",
          "expiresAt" => System.system_time(:millisecond) + 8_000,
          "payload" => %{
            "runId" => "run-1",
            "streamToken" => web_session_reply.stream_token,
            "manualControlId" => "manual-web-1",
            "action" => "type_text",
            "text" => String.duplicate("a", 530_000)
          }
        })

      assert_reply oversized_ref, :error, %{
        reason: "command_payload_too_large",
        maxBytes: max_bytes,
        sizeBytes: size_bytes
      }

      assert max_bytes == 512 * 1024
      assert size_bytes > max_bytes

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_manual_control_input",
        clientCommandId: "cmd-oversized",
        outcome: "rejected",
        reason: "command_payload_too_large"
      }

      refute_push "frame", %{payload: %{"clientCommandId" => "cmd-oversized"}}, 50
    end)
  end

  test "computer-use stream e2e exchanges WebRTC signaling without a run id", %{conn: conn} do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)
      session_id = "__computer_use__"

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, _desktop_session_reply, desktop_session} =
        subscribe_and_join(
          desktop_socket,
          "session:#{desktop["desktop_device_id"]}:#{session_id}",
          %{}
        )

      {:ok, web_socket} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web["web_jwt"]})

      join_task =
        Task.async(fn ->
          subscribe_and_join(
            web_socket,
            "session:#{desktop["desktop_device_id"]}:#{session_id}",
            %{
              "join_ref" => "join-computer-use-stream"
            }
          )
        end)

      assert_push "session_join_requested", %{
        auth_topic: auth_topic,
        join_ref: "join-computer-use-stream",
        session_id: "__computer_use__"
      }

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-computer-use-stream",
          "auth_topic" => auth_topic,
          "authorized" => true
        })

      assert_reply ref, :ok
      {:ok, web_session_reply, web_session} = Task.await(join_task)
      assert is_binary(web_session_reply.stream_token)
      refute Map.has_key?(web_session_reply, :stream_run_id)

      start_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_stream_request",
          "clientCommandId" => "cmd-computer-use-start",
          "payload" => %{
            "quality" => "balanced",
            "streamToken" => web_session_reply.stream_token
          }
        })

      assert_reply start_ref, :ok

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_stream_request",
        clientCommandId: "cmd-computer-use-start",
        outcome: "accepted"
      }

      assert_push "frame", %{
        from_kind: "web",
        direction: "web_to_desktop",
        payload: %{
          "kind" => "computer_use_stream_request",
          "payload" => %{
            "quality" => "balanced",
            "streamToken" => stream_token
          }
        }
      }

      assert stream_token == web_session_reply.stream_token

      offer_ref =
        push(desktop_session, "frame", %{
          "schema" => "xero.computer_use_stream_offer.v1",
          "streamId" => "stream-computer-use",
          "ok" => true,
          "payload" => %{"type" => "offer", "sdp" => "v=0\r\n"}
        })

      assert_reply offer_ref, :ok

      assert_push "frame", %{
        from_kind: "desktop",
        direction: "desktop_to_web",
        payload: %{
          "schema" => "xero.computer_use_stream_offer.v1",
          "streamId" => "stream-computer-use",
          "payload" => %{"type" => "offer", "sdp" => "v=0\r\n"}
        }
      }

      answer_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_stream_answer",
          "clientCommandId" => "cmd-computer-use-answer",
          "payload" => %{
            "streamId" => "stream-computer-use",
            "type" => "answer",
            "sdp" => "v=0\r\n",
            "streamToken" => web_session_reply.stream_token
          }
        })

      assert_reply answer_ref, :ok

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_stream_answer",
        clientCommandId: "cmd-computer-use-answer",
        outcome: "accepted"
      }

      assert_push "frame", %{
        from_kind: "web",
        direction: "web_to_desktop",
        payload: %{
          "kind" => "computer_use_stream_answer",
          "payload" => %{
            "streamId" => "stream-computer-use",
            "type" => "answer",
            "sdp" => "v=0\r\n",
            "streamToken" => ^stream_token
          }
        }
      }
    end)
  end

  test "computer-use desktop stream connects to only one cloud client at a time", %{conn: conn} do
    with_github_env(fn ->
      Process.flag(:trap_exit, true)

      desktop = desktop_login!(conn)
      web_a = web_login!(conn, name: "Test Web A")
      web_b = web_login!(conn, name: "Test Web B")
      session_id = "__computer_use__"

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, _desktop_session_reply, desktop_session} =
        subscribe_and_join(
          desktop_socket,
          "session:#{desktop["desktop_device_id"]}:#{session_id}",
          %{}
        )

      {:ok, web_socket_a} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web_a["web_jwt"]})

      {:ok, web_socket_b} =
        connect(XeroWeb.RemoteWebSocket, %{"token" => web_b["web_jwt"]})

      join_a =
        Task.async(fn ->
          subscribe_and_join(
            web_socket_a,
            "session:#{desktop["desktop_device_id"]}:#{session_id}",
            %{
              "cloud_instance_id" => "cloud-a",
              "join_ref" => "join-exclusive-a"
            }
          )
        end)

      assert_push "session_join_requested", %{
        auth_topic: auth_topic_a,
        join_ref: "join-exclusive-a"
      }

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-exclusive-a",
          "auth_topic" => auth_topic_a,
          "authorized" => true,
          "run_id" => "run-exclusive"
        })

      assert_reply ref, :ok
      {:ok, web_reply_a, web_session_a} = Task.await(join_a)
      assert web_reply_a.remote_control.available == true

      {:ok, web_control_reply_a, web_control_a} =
        subscribe_and_join(
          web_socket_a,
          "session:#{desktop["desktop_device_id"]}:__sessions__",
          %{"cloud_instance_id" => "cloud-a", "last_seq" => 0}
        )

      assert web_control_reply_a.remote_control.available == true

      assert_push "session_join_requested", %{
        session_id: "__sessions__",
        last_seq: 0
      }

      join_b =
        Task.async(fn ->
          subscribe_and_join(
            web_socket_b,
            "session:#{desktop["desktop_device_id"]}:#{session_id}",
            %{
              "cloud_instance_id" => "cloud-b",
              "join_ref" => "join-exclusive-b"
            }
          )
        end)

      assert_push "session_join_requested", %{
        auth_topic: auth_topic_b,
        join_ref: "join-exclusive-b"
      }

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-exclusive-b",
          "auth_topic" => auth_topic_b,
          "authorized" => true,
          "run_id" => "run-exclusive"
        })

      assert_reply ref, :ok
      {:ok, web_reply_b, web_session_b} = Task.await(join_b)
      assert web_reply_b.remote_control.available == false
      assert web_reply_b.remote_control.reason == "computer_use_connection_already_active"

      assert web_reply_b.remote_control.message ==
               "Xero Cloud is already connected to this desktop from another app instance or location. Stop that running connection first to use it here."

      first_ref =
        push(web_session_a, "frame", %{
          "kind" => "computer_use_stream_request",
          "clientCommandId" => "cmd-stream-a",
          "payload" => %{
            "quality" => "balanced",
            "runId" => "run-exclusive",
            "streamToken" => web_reply_a.stream_token
          }
        })

      assert_reply first_ref, :ok

      assert_push "computer_use_command_outcome", %{
        clientCommandId: "cmd-stream-a",
        outcome: "accepted"
      }

      assert_push "frame", %{
        from_device_id: web_device_id_a,
        from_kind: "web",
        payload: %{"kind" => "computer_use_stream_request"}
      }

      assert web_device_id_a == web_a["web_device_id"]

      second_ref =
        push(web_session_b, "frame", %{
          "kind" => "computer_use_stream_request",
          "clientCommandId" => "cmd-stream-b",
          "payload" => %{
            "quality" => "balanced",
            "runId" => "run-exclusive",
            "streamToken" => web_reply_b.stream_token
          }
        })

      assert_reply second_ref, :error, %{
        reason: "computer_use_connection_already_active",
        message:
          "Xero Cloud is already connected to this desktop from another app instance or location. Stop that running connection first to use it here."
      }

      assert_push "computer_use_command_outcome", %{
        clientCommandId: "cmd-stream-b",
        outcome: "rejected",
        reason: "computer_use_connection_already_active"
      }

      refute_push "frame", %{payload: %{"clientCommandId" => "cmd-stream-b"}}, 50

      blocked_message_ref =
        push(web_session_b, "frame", %{
          "kind" => "send_message",
          "clientCommandId" => "cmd-message-b",
          "payload" => %{"message" => "check the screen"}
        })

      assert_reply blocked_message_ref, :error, %{
        reason: "computer_use_connection_already_active",
        message:
          "Xero Cloud is already connected to this desktop from another app instance or location. Stop that running connection first to use it here."
      }

      refute_push "frame", %{payload: %{"clientCommandId" => "cmd-message-b"}}, 50

      offer_ref =
        push(desktop_session, "frame", %{
          "schema" => "xero.computer_use_stream_offer.v1",
          "streamId" => "stream-a",
          "ok" => true
        })

      assert_reply offer_ref, :ok

      assert_push "frame", %{
        from_kind: "desktop",
        payload: %{
          "schema" => "xero.computer_use_stream_offer.v1",
          "streamId" => "stream-a"
        }
      }

      refute_push "frame",
                  %{
                    from_kind: "desktop",
                    payload: %{
                      "schema" => "xero.computer_use_stream_offer.v1",
                      "streamId" => "stream-a"
                    }
                  },
                  50

      stop_ref =
        push(web_session_a, "frame", %{
          "kind" => "computer_use_stream_stop",
          "clientCommandId" => "cmd-stop-a",
          "payload" => %{
            "runId" => "run-exclusive",
            "streamToken" => web_reply_a.stream_token,
            "streamId" => "stream-a"
          }
        })

      assert_reply stop_ref, :ok

      assert_push "computer_use_command_outcome", %{
        clientCommandId: "cmd-stop-a",
        outcome: "accepted"
      }

      assert_push "frame", %{
        from_kind: "web",
        payload: %{"kind" => "computer_use_stream_stop"}
      }

      still_blocked_ref =
        push(web_session_b, "frame", %{
          "kind" => "computer_use_stream_request",
          "clientCommandId" => "cmd-stream-b-still-blocked",
          "payload" => %{
            "quality" => "balanced",
            "runId" => "run-exclusive",
            "streamToken" => web_reply_b.stream_token
          }
        })

      assert_reply still_blocked_ref, :error, %{reason: "computer_use_connection_already_active"}

      leave_ref = leave(web_session_a)
      assert_reply leave_ref, :ok
      assert_receive {:socket_close, _pid, {:shutdown, :left}}, 100

      still_blocked_by_control_ref =
        push(web_session_b, "frame", %{
          "kind" => "computer_use_stream_request",
          "clientCommandId" => "cmd-stream-b-blocked-by-control-channel",
          "payload" => %{
            "quality" => "balanced",
            "runId" => "run-exclusive",
            "streamToken" => web_reply_b.stream_token
          }
        })

      assert_reply still_blocked_by_control_ref, :error, %{
        reason: "computer_use_connection_already_active"
      }

      leave_control_ref = leave(web_control_a)
      assert_reply leave_control_ref, :ok
      assert_receive {:socket_close, _pid, {:shutdown, :left}}, 100

      retry_ref =
        push(web_session_b, "frame", %{
          "kind" => "computer_use_stream_request",
          "clientCommandId" => "cmd-stream-b-retry",
          "payload" => %{
            "quality" => "balanced",
            "runId" => "run-exclusive",
            "streamToken" => web_reply_b.stream_token
          }
        })

      assert_reply retry_ref, :ok

      assert_push "computer_use_command_outcome", %{
        clientCommandId: "cmd-stream-b-retry",
        outcome: "accepted"
      }

      assert_push "frame", %{
        from_device_id: web_device_id_b,
        from_kind: "web",
        payload: %{"kind" => "computer_use_stream_request"}
      }

      assert web_device_id_b == web_b["web_device_id"]
    end)
  end

  test "computer-use command telemetry reports rate-limit rejections", %{conn: conn} do
    with_github_env(fn ->
      desktop = desktop_login!(conn)
      web = web_login!(conn)
      original_rate_limit_config = Application.get_env(:xero, Xero.RateLimiter, [])

      Application.put_env(:xero, Xero.RateLimiter,
        computer_use_per_minute: %{"stream_status" => 1, "manual_critical" => 10}
      )

      on_exit(fn ->
        Application.put_env(:xero, Xero.RateLimiter, original_rate_limit_config)
      end)

      {:ok, desktop_socket} =
        connect(XeroWeb.RemoteDesktopSocket, %{"token" => desktop["desktop_jwt"]})

      {:ok, _desktop_reply, desktop_socket} =
        subscribe_and_join(desktop_socket, "desktop:#{desktop["desktop_device_id"]}", %{})

      {:ok, _desktop_session_reply, _desktop_session} =
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
            "join_ref" => "join-command-rate-limit"
          })
        end)

      assert_push "session_join_requested", %{
        auth_topic: auth_topic,
        join_ref: "join-command-rate-limit"
      }

      ref =
        push(desktop_socket, "session_authorized", %{
          "join_ref" => "join-command-rate-limit",
          "auth_topic" => auth_topic,
          "authorized" => true,
          "run_id" => "run-rate-limit"
        })

      assert_reply ref, :ok
      {:ok, web_session_reply, web_session} = Task.await(join_task)
      attach_remote_command_telemetry(self())

      first_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_stream_status",
          "clientCommandId" => "cmd-status-1",
          "payload" => %{
            "runId" => "run-rate-limit",
            "streamToken" => web_session_reply.stream_token
          }
        })

      second_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_stream_status",
          "clientCommandId" => "cmd-status-2",
          "payload" => %{
            "runId" => "run-rate-limit",
            "streamToken" => web_session_reply.stream_token
          }
        })

      assert_reply first_ref, :ok

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_stream_status",
        clientCommandId: "cmd-status-1",
        outcome: "accepted",
        priority: "coalesced_best_effort"
      }

      assert_reply second_ref, :error, %{
        reason: "rate_limited",
        retry_after_ms: retry_after_ms,
        rateLimit: %{class: "stream_status", kind: "computer_use_stream_status"}
      }

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_stream_status",
        clientCommandId: "cmd-status-2",
        outcome: "rate_limited",
        priority: "coalesced_best_effort",
        rateLimit: %{class: "stream_status", kind: "computer_use_stream_status"}
      }

      assert retry_after_ms > 0

      manual_ref =
        push(web_session, "frame", %{
          "kind" => "computer_use_manual_control_heartbeat",
          "clientCommandId" => "cmd-heartbeat-after-status-limit",
          "payload" => %{
            "runId" => "run-rate-limit",
            "streamToken" => web_session_reply.stream_token,
            "manualControlId" => "manual-after-status-limit"
          }
        })

      assert_reply manual_ref, :ok

      assert_push "computer_use_command_outcome", %{
        kind: "computer_use_manual_control_heartbeat",
        clientCommandId: "cmd-heartbeat-after-status-limit",
        outcome: "accepted",
        priority: "critical_reliable"
      }

      assert_receive {:remote_command_telemetry,
                      [:xero, :remote, :computer_use, :command, :forwarded], %{count: 1},
                      %{
                        family: :stream,
                        kind: "computer_use_stream_status",
                        reason: "none"
                      }}

      assert_receive {:remote_command_telemetry,
                      [:xero, :remote, :computer_use, :command, :rejected],
                      %{count: 1, bytes: rejected_bytes},
                      %{
                        family: :stream,
                        kind: "computer_use_stream_status",
                        reason: "rate_limited"
                      }}

      assert rejected_bytes > 0
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

  defp attach_remote_command_telemetry(test_pid) do
    handler_id = {:remote_command_telemetry, self(), System.unique_integer([:positive])}

    :ok =
      :telemetry.attach_many(
        handler_id,
        [
          [:xero, :remote, :computer_use, :command, :forwarded],
          [:xero, :remote, :computer_use, :command, :rejected]
        ],
        fn event, measurements, metadata, _config ->
          send(test_pid, {:remote_command_telemetry, event, measurements, metadata})
        end,
        nil
      )

    on_exit(fn -> :telemetry.detach(handler_id) end)
  end
end
