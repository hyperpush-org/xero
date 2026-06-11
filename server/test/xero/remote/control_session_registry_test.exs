defmodule Xero.Remote.ControlSessionRegistryTest do
  use ExUnit.Case, async: false

  alias Xero.Remote.ControlSessionRegistry

  setup do
    previous_env = Application.get_env(:xero, ControlSessionRegistry)
    ControlSessionRegistry.reset!()

    on_exit(fn ->
      if previous_env do
        Application.put_env(:xero, ControlSessionRegistry, previous_env)
      else
        Application.delete_env(:xero, ControlSessionRegistry)
      end

      ControlSessionRegistry.reset!()
    end)

    :ok
  end

  test "active owner expires after inactivity timeout" do
    Application.put_env(:xero, ControlSessionRegistry, owner_idle_timeout_ms: 1)

    assert {:ok, %{owner_id: "owner-1"}} =
             ControlSessionRegistry.acquire("desktop-1", "session-1", "owner-1", "web-1", self())

    Process.sleep(5)

    assert ControlSessionRegistry.active_owner("desktop-1", "session-1") == nil
  end

  test "same owner reacquire refreshes inactivity timeout" do
    Application.put_env(:xero, ControlSessionRegistry, owner_idle_timeout_ms: 80)

    assert {:ok, %{owner_id: "owner-1"}} =
             ControlSessionRegistry.acquire("desktop-1", "session-1", "owner-1", "web-1", self())

    Process.sleep(40)

    assert {:ok, %{owner_id: "owner-1"}} =
             ControlSessionRegistry.acquire("desktop-1", "session-1", "owner-1", "web-1", self())

    Process.sleep(50)

    assert %{owner_id: "owner-1"} = ControlSessionRegistry.active_owner("desktop-1", "session-1")
  end
end
