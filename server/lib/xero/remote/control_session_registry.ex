defmodule Xero.Remote.ControlSessionRegistry do
  @moduledoc false

  use GenServer

  @default_owner_idle_timeout_ms :timer.minutes(5)

  defstruct sessions: %{}, monitors: %{}

  def start_link(opts \\ []) do
    GenServer.start_link(__MODULE__, %__MODULE__{}, Keyword.put_new(opts, :name, __MODULE__))
  end

  def acquire(desktop_device_id, session_id, web_device_id, owner_pid \\ self()) do
    acquire(desktop_device_id, session_id, web_device_id, web_device_id, owner_pid)
  end

  def acquire(desktop_device_id, session_id, owner_id, web_device_id, owner_pid) do
    GenServer.call(
      __MODULE__,
      {:acquire, desktop_device_id, session_id, owner_id, web_device_id, owner_pid}
    )
  end

  def release(desktop_device_id, session_id, owner_pid \\ self()) do
    GenServer.call(__MODULE__, {:release_pid, desktop_device_id, session_id, owner_pid})
  end

  def release(desktop_device_id, session_id, owner_id, owner_pid) do
    GenServer.call(__MODULE__, {:release, desktop_device_id, session_id, owner_id, owner_pid})
  end

  def active_owner(desktop_device_id, session_id) do
    GenServer.call(__MODULE__, {:active_owner, desktop_device_id, session_id})
  end

  def reset! do
    GenServer.call(__MODULE__, :reset)
  end

  @impl true
  def init(state), do: {:ok, state}

  @impl true
  def handle_call(
        {:acquire, desktop_device_id, session_id, owner_id, web_device_id, owner_pid},
        _from,
        state
      ) do
    key = key(desktop_device_id, session_id)
    now_ms = monotonic_ms()
    state = prune_inactive_owner(state, key, now_ms)
    state = prune_dead_owner(state, key)

    case Map.get(state.sessions, key) do
      nil ->
        {entry, state} = put_owner(state, key, owner_id, web_device_id, owner_pid, now_ms)
        {:reply, {:ok, public_entry(entry)}, state}

      %{owner_id: ^owner_id} = entry ->
        {entry, state} = touch_owner(state, key, entry, now_ms)
        {entry, state} = track_owner_pid(state, key, entry, owner_pid)
        {:reply, {:ok, public_entry(entry)}, state}

      entry ->
        {:reply, {:error, {:already_active, public_entry(entry)}}, state}
    end
  end

  def handle_call({:release_pid, desktop_device_id, session_id, owner_pid}, _from, state) do
    key = key(desktop_device_id, session_id)
    state = prune_inactive_owner(state, key, monotonic_ms())
    state = prune_dead_owner(state, key)

    case Map.get(state.sessions, key) do
      nil ->
        {:reply, :ok, state}

      %{owner_pids: owner_pids} = entry when is_map_key(owner_pids, owner_pid) ->
        {:reply, :ok, untrack_owner_pid(state, key, entry, owner_pid)}

      entry ->
        {:reply, {:error, {:not_owner, public_entry(entry)}}, state}
    end
  end

  def handle_call({:release, desktop_device_id, session_id, owner_id, owner_pid}, _from, state) do
    key = key(desktop_device_id, session_id)
    state = prune_inactive_owner(state, key, monotonic_ms())
    state = prune_dead_owner(state, key)

    case Map.get(state.sessions, key) do
      nil ->
        {:reply, :ok, state}

      %{owner_id: ^owner_id, owner_pids: owner_pids} = entry
      when is_map_key(owner_pids, owner_pid) ->
        {:reply, :ok, untrack_owner_pid(state, key, entry, owner_pid)}

      entry ->
        {:reply, {:error, {:not_owner, public_entry(entry)}}, state}
    end
  end

  def handle_call({:active_owner, desktop_device_id, session_id}, _from, state) do
    key = key(desktop_device_id, session_id)
    state = prune_inactive_owner(state, key, monotonic_ms())
    state = prune_dead_owner(state, key)
    owner = state.sessions |> Map.get(key) |> public_entry()
    {:reply, owner, state}
  end

  def handle_call(:reset, _from, state) do
    Enum.each(state.sessions, fn {_key, entry} ->
      Enum.each(entry.owner_pids, fn {_pid, monitor_ref} ->
        Process.demonitor(monitor_ref, [:flush])
      end)
    end)

    {:reply, :ok, %__MODULE__{}}
  end

  @impl true
  def handle_info({:DOWN, monitor_ref, :process, owner_pid, _reason}, state) do
    case Map.pop(state.monitors, monitor_ref) do
      {nil, monitors} ->
        {:noreply, %{state | monitors: monitors}}

      {{key, owner_id, ^owner_pid}, monitors} ->
        entry = Map.get(state.sessions, key)

        sessions =
          if entry && entry.owner_id == owner_id do
            updated_entry = %{entry | owner_pids: Map.delete(entry.owner_pids, owner_pid)}

            if map_size(updated_entry.owner_pids) == 0 do
              Map.delete(state.sessions, key)
            else
              Map.put(state.sessions, key, updated_entry)
            end
          else
            state.sessions
          end

        {:noreply, %{state | sessions: sessions, monitors: monitors}}
    end
  end

  defp put_owner(state, key, owner_id, web_device_id, owner_pid, now_ms) do
    entry = %{
      owner_id: owner_id,
      owner_pids: %{},
      web_device_id: web_device_id,
      started_at: DateTime.utc_now() |> DateTime.truncate(:second) |> DateTime.to_iso8601(),
      last_seen_ms: now_ms
    }

    track_owner_pid(state, key, entry, owner_pid)
  end

  defp touch_owner(state, key, entry, now_ms) do
    entry = Map.put(entry, :last_seen_ms, now_ms)
    {entry, %{state | sessions: Map.put(state.sessions, key, entry)}}
  end

  defp delete_owner(state, key) do
    case Map.pop(state.sessions, key) do
      {nil, sessions} ->
        %{state | sessions: sessions}

      {entry, sessions} ->
        Enum.each(entry.owner_pids, fn {_pid, monitor_ref} ->
          Process.demonitor(monitor_ref, [:flush])
        end)

        monitors =
          Enum.reduce(entry.owner_pids, state.monitors, fn {_pid, monitor_ref}, monitors ->
            Map.delete(monitors, monitor_ref)
          end)

        %{
          state
          | sessions: sessions,
            monitors: monitors
        }
    end
  end

  defp prune_dead_owner(state, key) do
    case Map.get(state.sessions, key) do
      %{owner_pids: owner_pids} ->
        owner_pids
        |> Map.keys()
        |> Enum.reduce(state, fn owner_pid, state ->
          entry = Map.get(state.sessions, key)

          cond do
            entry == nil -> state
            owner_pid_alive?(owner_pid) -> state
            true -> untrack_owner_pid(state, key, entry, owner_pid)
          end
        end)

      %{owner_pid: _owner_pid} ->
        delete_owner(state, key)

      nil ->
        state
    end
  end

  defp prune_inactive_owner(state, key, now_ms) do
    case Map.get(state.sessions, key) do
      %{last_seen_ms: last_seen_ms} when is_integer(last_seen_ms) ->
        if now_ms - last_seen_ms >= owner_idle_timeout_ms() do
          delete_owner(state, key)
        else
          state
        end

      %{owner_pids: _owner_pids} ->
        delete_owner(state, key)

      _ ->
        state
    end
  end

  defp owner_idle_timeout_ms do
    :xero
    |> Application.get_env(__MODULE__, [])
    |> Keyword.get(:owner_idle_timeout_ms, @default_owner_idle_timeout_ms)
  end

  defp monotonic_ms, do: System.monotonic_time(:millisecond)

  defp owner_pid_alive?(owner_pid) when is_pid(owner_pid), do: Process.alive?(owner_pid)
  defp owner_pid_alive?(_owner_pid), do: false

  defp track_owner_pid(state, key, entry, owner_pid) do
    if Map.has_key?(entry.owner_pids, owner_pid) do
      {entry, state}
    else
      monitor_ref = Process.monitor(owner_pid)
      entry = %{entry | owner_pids: Map.put(entry.owner_pids, owner_pid, monitor_ref)}

      {entry,
       %{
         state
         | sessions: Map.put(state.sessions, key, entry),
           monitors: Map.put(state.monitors, monitor_ref, {key, entry.owner_id, owner_pid})
       }}
    end
  end

  defp untrack_owner_pid(state, key, entry, owner_pid) do
    case Map.pop(entry.owner_pids, owner_pid) do
      {nil, _owner_pids} ->
        state

      {monitor_ref, owner_pids} ->
        Process.demonitor(monitor_ref, [:flush])

        monitors = Map.delete(state.monitors, monitor_ref)

        if map_size(owner_pids) == 0 do
          %{state | sessions: Map.delete(state.sessions, key), monitors: monitors}
        else
          entry = %{entry | owner_pids: owner_pids}
          %{state | sessions: Map.put(state.sessions, key, entry), monitors: monitors}
        end
    end
  end

  defp public_entry(nil), do: nil

  defp public_entry(entry) do
    %{
      owner_id: entry.owner_id,
      web_device_id: entry.web_device_id,
      started_at: entry.started_at
    }
  end

  defp key(desktop_device_id, _session_id), do: desktop_device_id
end
