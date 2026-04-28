defmodule Joe.Repo.Migrations.CreateGithubAuthSessions do
  use Ecto.Migration

  def change do
    create table(:github_auth_sessions, primary_key: false) do
      add :session_id, :text, primary_key: true
      add :encrypted_access_token, :text, null: false
      add :token_type, :text, null: false, default: "bearer"
      add :scope, :text, null: false, default: ""
      add :user, :map, null: false
      add :created_at, :text, null: false

      timestamps(type: :utc_datetime_usec)
    end
  end
end
