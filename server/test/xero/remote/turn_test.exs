defmodule Xero.Remote.TurnTest do
  use ExUnit.Case, async: false

  alias Xero.Remote.Turn

  setup do
    original = Application.get_env(:xero, Turn)

    on_exit(fn ->
      case original do
        nil -> Application.delete_env(:xero, Turn)
        value -> Application.put_env(:xero, Turn, value)
      end
    end)

    :ok
  end

  test "issues coturn REST credentials without exposing the shared secret" do
    Application.put_env(:xero, Turn,
      stun_urls: ["stun:stun.example.test:3478"],
      turn_urls: ["turn:turn.example.test:3478?transport=udp"],
      shared_secret: "relay-secret",
      ttl_seconds: 900
    )

    assert [
             %{urls: ["stun:stun.example.test:3478"]},
             %{
               urls: ["turn:turn.example.test:3478?transport=udp"],
               username: "1710000900:test-nonce",
               credential: credential,
               credential_type: "password",
               ttl_seconds: 900
             }
           ] = Turn.ice_servers(now_seconds: 1_710_000_000, nonce: "test-nonce")

    assert credential ==
             :crypto.mac(:hmac, :sha, "relay-secret", "1710000900:test-nonce")
             |> Base.encode64()
  end

  test "omits TURN when the relay secret is not configured" do
    Application.put_env(:xero, Turn,
      stun_urls: [],
      turn_urls: ["turn:turn.example.test:3478"],
      shared_secret: nil
    )

    assert Turn.ice_servers(now_seconds: 1, nonce: "n") == []
  end
end
