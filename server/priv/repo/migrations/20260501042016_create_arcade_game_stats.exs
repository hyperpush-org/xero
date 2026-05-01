defmodule Xero.Repo.Migrations.CreateArcadeGameStats do
  use Ecto.Migration

  def change do
    create table(:game_players, primary_key: false) do
      add :github_user_id, :bigint, primary_key: true
      add :login, :text, null: false
      add :name, :text
      add :avatar_url, :text
      add :html_url, :text

      timestamps(type: :utc_datetime_usec)
    end

    create table(:game_stats, primary_key: false) do
      add :github_user_id,
          references(:game_players,
            column: :github_user_id,
            type: :bigint,
            on_delete: :delete_all
          ),
          primary_key: true

      add :game_id, :text, primary_key: true
      add :personal_best, :bigint, null: false, default: 0
      add :runs, :integer, null: false, default: 0
      add :time_played_ms, :bigint, null: false, default: 0
      add :last_played_at, :utc_datetime_usec

      timestamps(type: :utc_datetime_usec)
    end

    create index(:game_stats, [:game_id, :personal_best])
  end
end
