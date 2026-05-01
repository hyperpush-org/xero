defmodule Xero.Arcade.GameStat do
  @moduledoc false

  use Ecto.Schema

  import Ecto.Changeset

  @primary_key false
  schema "game_stats" do
    field :github_user_id, :integer, primary_key: true
    field :game_id, :string, primary_key: true
    field :personal_best, :integer, default: 0
    field :runs, :integer, default: 0
    field :time_played_ms, :integer, default: 0
    field :last_played_at, :utc_datetime_usec

    timestamps(type: :utc_datetime_usec)
  end

  def create_changeset(stat, attrs) do
    stat
    |> cast(attrs, [
      :github_user_id,
      :game_id,
      :personal_best,
      :runs,
      :time_played_ms,
      :last_played_at
    ])
    |> validate_required([:github_user_id, :game_id, :personal_best, :runs, :time_played_ms])
    |> validate_number(:github_user_id, greater_than: 0)
    |> validate_number(:personal_best, greater_than_or_equal_to: 0)
    |> validate_number(:runs, greater_than_or_equal_to: 0)
    |> validate_number(:time_played_ms, greater_than_or_equal_to: 0)
  end

  def record_run_changeset(stat, attrs) do
    stat
    |> cast(attrs, [:personal_best, :runs, :time_played_ms, :last_played_at])
    |> validate_required([:personal_best, :runs, :time_played_ms, :last_played_at])
    |> validate_number(:personal_best, greater_than_or_equal_to: 0)
    |> validate_number(:runs, greater_than: 0)
    |> validate_number(:time_played_ms, greater_than_or_equal_to: 0)
  end
end
