defmodule XeroWeb.GameStatsControllerTest do
  use XeroWeb.ConnCase

  import Ecto.Query

  alias Xero.GitHubAuth
  alias Xero.Repo

  setup do
    GitHubAuth.reset!()
    :ok
  end

  test "game stats require a GitHub session", %{conn: conn} do
    stats_conn = get(conn, ~p"/api/games/stats")
    assert json_response(stats_conn, 401)["error"]["code"] == "github_session_required"

    run_conn =
      post(conn, ~p"/api/games/runs", %{
        "gameId" => "tetris",
        "score" => 120,
        "timePlayedMs" => 3_000
      })

    assert json_response(run_conn, 401)["error"]["code"] == "github_session_required"
  end

  test "records game runs against the authenticated GitHub user", %{conn: conn} do
    session_id = seed_session!("state-a", "session-a", 42, "octo")

    first_conn =
      conn
      |> put_req_header(GitHubAuth.session_header(), session_id)
      |> post(~p"/api/games/runs", %{
        "gameId" => "tetris",
        "score" => 120,
        "timePlayedMs" => 3_000
      })

    first = json_response(first_conn, 200)
    tetris = stat_for(first, "tetris")

    assert tetris["personalBest"] == 120
    assert tetris["runs"] == 1
    assert tetris["timePlayedMs"] == 3_000
    assert [%{"login" => "octo", "score" => 120, "you" => true}] = tetris["leaderboard"]

    second_conn =
      conn
      |> recycle()
      |> put_req_header(GitHubAuth.session_header(), session_id)
      |> post(~p"/api/games/runs", %{
        "gameId" => "tetris",
        "score" => 80,
        "timePlayedMs" => 1_250
      })

    second = json_response(second_conn, 200)
    tetris = stat_for(second, "tetris")

    assert tetris["personalBest"] == 120
    assert tetris["runs"] == 2
    assert tetris["timePlayedMs"] == 4_250

    db_stat =
      Repo.one!(
        from(s in "game_stats",
          where: s.github_user_id == 42 and s.game_id == "tetris",
          select: %{
            personal_best: s.personal_best,
            runs: s.runs,
            time_played_ms: s.time_played_ms
          }
        )
      )

    assert db_stat == %{personal_best: 120, runs: 2, time_played_ms: 4_250}
  end

  test "keeps stats isolated between GitHub users", %{conn: conn} do
    session_a = seed_session!("state-a", "session-a", 42, "octo")
    session_b = seed_session!("state-b", "session-b", 99, "mona")

    conn
    |> put_req_header(GitHubAuth.session_header(), session_a)
    |> post(~p"/api/games/runs", %{
      "gameId" => "snake",
      "score" => 500,
      "timePlayedMs" => 2_000
    })
    |> json_response(200)

    body =
      conn
      |> recycle()
      |> put_req_header(GitHubAuth.session_header(), session_b)
      |> post(~p"/api/games/runs", %{
        "gameId" => "snake",
        "score" => 700,
        "timePlayedMs" => 1_000
      })
      |> json_response(200)

    snake = stat_for(body, "snake")

    assert snake["personalBest"] == 700
    assert snake["runs"] == 1

    assert [
             %{"login" => "mona", "score" => 700, "you" => true},
             %{"login" => "octo", "score" => 500, "you" => false}
           ] = snake["leaderboard"]
  end

  test "rejects unknown games and malformed run stats", %{conn: conn} do
    session_id = seed_session!("state-a", "session-a", 42, "octo")

    unknown_conn =
      conn
      |> put_req_header(GitHubAuth.session_header(), session_id)
      |> post(~p"/api/games/runs", %{
        "gameId" => "pong",
        "score" => 120,
        "timePlayedMs" => 3_000
      })

    assert json_response(unknown_conn, 422)["error"]["code"] == "game_id_invalid"

    negative_conn =
      conn
      |> recycle()
      |> put_req_header(GitHubAuth.session_header(), session_id)
      |> post(~p"/api/games/runs", %{
        "gameId" => "tetris",
        "score" => -1,
        "timePlayedMs" => 3_000
      })

    assert json_response(negative_conn, 422)["error"]["code"] == "game_score_invalid"
  end

  defp stat_for(body, game_id) do
    Enum.find(body["stats"], &(&1["gameId"] == game_id))
  end

  defp seed_session!(state_token, session_id, user_id, login) do
    stored_session =
      GitHubAuth.stored_session(
        "server-token-#{session_id}",
        "bearer",
        "read:user",
        %{
          "id" => user_id,
          "login" => login,
          "name" => String.capitalize(login),
          "email" => nil,
          "avatarUrl" => "https://avatars.githubusercontent.com/u/#{user_id}?v=4",
          "htmlUrl" => "https://github.com/#{login}"
        }
      )

    # Tests can complete server-side flows directly; the state token only needs
    # to map to an active flow.
    {:ok, started} = start_login_for_test(state_token)
    assert :ok = GitHubAuth.complete_state(started.state, session_id, stored_session)

    session_id
  end

  defp start_login_for_test(state_token) do
    flow_id = "flow-#{state_token}"

    flow = %{
      flow_id: flow_id,
      state: state_token,
      status: :pending,
      inserted_at: DateTime.utc_now()
    }

    :sys.replace_state(GitHubAuth, fn state ->
      %{
        state
        | flows: Map.put(state.flows, flow_id, flow),
          states: Map.put(state.states, state_token, flow_id)
      }
    end)

    {:ok, %{state: state_token}}
  end
end
