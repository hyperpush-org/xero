defmodule Xero.Arcade do
  @moduledoc """
  Server-owned arcade statistics tied to authenticated GitHub users.
  """

  import Ecto.Query

  alias Xero.Arcade.{GameStat, Player}
  alias Xero.GitHubAuth
  alias Xero.Repo

  @game_ids ~w(tetris space-invaders snake pacman breakout asteroids galaga)
  @leaderboard_limit 5
  @max_score 1_000_000_000
  @max_time_played_ms 24 * 60 * 60 * 1000

  def list_stats(session_id) do
    with {:ok, user} <- authenticated_user(session_id),
         {:ok, _player} <- upsert_player(user) do
      {:ok, build_stats(user.id)}
    end
  end

  def record_run(session_id, attrs) do
    with {:ok, user} <- authenticated_user(session_id),
         {:ok, run} <- validate_run(attrs),
         {:ok, _player} <- upsert_player(user),
         {:ok, _stat} <- upsert_run(user.id, run) do
      {:ok, build_stats(user.id)}
    end
  end

  def game_ids, do: @game_ids

  defp authenticated_user(session_id) when is_binary(session_id) and session_id != "" do
    case GitHubAuth.get_session(session_id) do
      {:ok, nil} -> {:error, error("github_session_required", "Sign in with GitHub first.")}
      {:ok, session} -> session |> Map.get(:user) |> normalize_user()
      {:error, error} -> {:error, error}
    end
  end

  defp authenticated_user(_session_id) do
    {:error, error("github_session_required", "Sign in with GitHub first.")}
  end

  defp normalize_user(%{"id" => id, "login" => login} = user)
       when is_integer(id) and id > 0 and is_binary(login) and login != "" do
    {:ok,
     %{
       id: id,
       login: login,
       name: blank_to_nil(user["name"]),
       avatar_url: blank_to_nil(user["avatarUrl"]),
       html_url: blank_to_nil(user["htmlUrl"])
     }}
  end

  defp normalize_user(%{id: id, login: login} = user)
       when is_integer(id) and id > 0 and is_binary(login) and login != "" do
    {:ok,
     %{
       id: id,
       login: login,
       name: blank_to_nil(Map.get(user, :name)),
       avatar_url: blank_to_nil(Map.get(user, :avatar_url) || Map.get(user, :avatarUrl)),
       html_url: blank_to_nil(Map.get(user, :html_url) || Map.get(user, :htmlUrl))
     }}
  end

  defp normalize_user(_user) do
    {:error, error("github_session_invalid", "GitHub session is missing user metadata.")}
  end

  defp upsert_player(user) do
    attrs = %{
      github_user_id: user.id,
      login: user.login,
      name: user.name,
      avatar_url: user.avatar_url,
      html_url: user.html_url
    }

    %Player{}
    |> Player.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace, [:login, :name, :avatar_url, :html_url, :updated_at]},
      conflict_target: :github_user_id
    )
  end

  defp validate_run(attrs) when is_map(attrs) do
    game_id = Map.get(attrs, "gameId") || Map.get(attrs, :game_id) || Map.get(attrs, :gameId)
    score = Map.get(attrs, "score") || Map.get(attrs, :score)

    time_played_ms =
      Map.get(attrs, "timePlayedMs") || Map.get(attrs, :time_played_ms) ||
        Map.get(attrs, :timePlayedMs)

    cond do
      game_id not in @game_ids ->
        {:error, error("game_id_invalid", "Xero does not know that arcade game.")}

      !valid_integer?(score, @max_score) ->
        {:error, error("game_score_invalid", "Game score must be a non-negative integer.")}

      !valid_integer?(time_played_ms, @max_time_played_ms) ->
        {:error,
         error("game_time_played_invalid", "Game time played must be a non-negative integer.")}

      true ->
        {:ok, %{game_id: game_id, score: score, time_played_ms: time_played_ms}}
    end
  end

  defp validate_run(_attrs),
    do: {:error, error("game_run_invalid", "Game run payload is invalid.")}

  defp upsert_run(github_user_id, run) do
    now = DateTime.utc_now()

    Repo.transaction(fn ->
      current = Repo.get_by(GameStat, github_user_id: github_user_id, game_id: run.game_id)

      stat =
        current ||
          %GameStat{
            github_user_id: github_user_id,
            game_id: run.game_id,
            personal_best: 0,
            runs: 0,
            time_played_ms: 0
          }

      attrs = %{
        personal_best: max(stat.personal_best || 0, run.score),
        runs: (stat.runs || 0) + 1,
        time_played_ms: (stat.time_played_ms || 0) + run.time_played_ms,
        last_played_at: now
      }

      changeset =
        if current do
          GameStat.record_run_changeset(stat, attrs)
        else
          GameStat.create_changeset(stat, Map.merge(attrs, %{github_user_id: github_user_id}))
        end

      case Repo.insert_or_update(changeset) do
        {:ok, stat} -> stat
        {:error, changeset} -> Repo.rollback(changeset)
      end
    end)
    |> case do
      {:ok, stat} ->
        {:ok, stat}

      {:error, %Ecto.Changeset{}} ->
        {:error, error("game_stats_store_failed", "Could not save game stats.")}
    end
  end

  defp build_stats(current_github_user_id) do
    stats_by_game_id =
      GameStat
      |> where([s], s.github_user_id == ^current_github_user_id)
      |> Repo.all()
      |> Map.new(&{&1.game_id, &1})

    %{
      "stats" =>
        Enum.map(@game_ids, fn game_id ->
          stat = Map.get(stats_by_game_id, game_id)

          %{
            "gameId" => game_id,
            "personalBest" => stat_value(stat, :personal_best),
            "runs" => stat_value(stat, :runs),
            "timePlayedMs" => stat_value(stat, :time_played_ms),
            "lastPlayedAt" => last_played_at(stat),
            "leaderboard" => leaderboard(game_id, current_github_user_id)
          }
        end)
    }
  end

  defp leaderboard(game_id, current_github_user_id) do
    GameStat
    |> join(:inner, [s], p in Player, on: p.github_user_id == s.github_user_id)
    |> where([s, _p], s.game_id == ^game_id)
    |> order_by([s, _p], desc: s.personal_best, asc: s.inserted_at)
    |> limit(^@leaderboard_limit)
    |> select([s, p], %{
      "githubUserId" => p.github_user_id,
      "login" => p.login,
      "name" => p.name,
      "avatarUrl" => p.avatar_url,
      "score" => s.personal_best,
      "you" => p.github_user_id == ^current_github_user_id
    })
    |> Repo.all()
  end

  defp stat_value(nil, _field), do: 0
  defp stat_value(stat, field), do: Map.get(stat, field) || 0

  defp last_played_at(nil), do: nil
  defp last_played_at(%{last_played_at: nil}), do: nil
  defp last_played_at(%{last_played_at: value}), do: DateTime.to_iso8601(value)

  defp valid_integer?(value, max) when is_integer(value), do: value >= 0 and value <= max
  defp valid_integer?(_value, _max), do: false

  defp blank_to_nil(value) when is_binary(value) do
    case String.trim(value) do
      "" -> nil
      trimmed -> trimmed
    end
  end

  defp blank_to_nil(_value), do: nil

  defp error(code, message), do: %{"code" => code, "message" => message}
end
