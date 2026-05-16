defmodule Xero.Repo.Migrations.RemoteAgenticWorkflowV2OauthAccounts do
  use Ecto.Migration

  def up do
    execute "DROP TABLE IF EXISTS pairings"
    execute "DELETE FROM github_auth_sessions"
    execute "DELETE FROM devices"
    execute "DELETE FROM accounts"

    execute "ALTER TYPE remote_device_kind RENAME TO remote_device_kind_old"
    execute "CREATE TYPE remote_device_kind AS ENUM ('desktop', 'web')"

    execute """
    ALTER TABLE devices
    ALTER COLUMN kind TYPE remote_device_kind
    USING kind::text::remote_device_kind
    """

    execute "DROP TYPE remote_device_kind_old"

    alter table(:accounts) do
      add :github_user_id, :bigint, null: false
      add :github_login, :text
      add :github_avatar_url, :text
    end

    create unique_index(:accounts, [:github_user_id])

    drop_if_exists index(:devices, [:public_key], name: :devices_public_key_index)

    alter table(:devices) do
      remove :public_key
      add :user_agent, :text
    end

    alter table(:github_auth_sessions) do
      add :kind, :text, null: false
      add :account_id, references(:accounts, type: :uuid, on_delete: :delete_all), null: false
      add :device_id, references(:devices, type: :uuid, on_delete: :delete_all), null: false
      add :csrf_token, :text
    end

    create index(:github_auth_sessions, [:account_id])
    create index(:github_auth_sessions, [:device_id])
  end

  def down do
    execute "DELETE FROM github_auth_sessions"
    execute "DELETE FROM devices"
    execute "DELETE FROM accounts"

    execute "ALTER TYPE remote_device_kind RENAME TO remote_device_kind_old"
    execute "CREATE TYPE remote_device_kind AS ENUM ('desktop', 'mobile')"

    execute """
    ALTER TABLE devices
    ALTER COLUMN kind TYPE remote_device_kind
    USING kind::text::remote_device_kind
    """

    execute "DROP TYPE remote_device_kind_old"

    drop_if_exists index(:github_auth_sessions, [:device_id])
    drop_if_exists index(:github_auth_sessions, [:account_id])

    alter table(:github_auth_sessions) do
      remove :csrf_token
      remove :device_id
      remove :account_id
      remove :kind
    end

    alter table(:devices) do
      remove :user_agent
      add :public_key, :text, null: false
    end

    create unique_index(:devices, [:public_key], where: "revoked_at IS NULL")

    drop_if_exists index(:accounts, [:github_user_id])

    alter table(:accounts) do
      remove :github_avatar_url
      remove :github_login
      remove :github_user_id
    end

    create table(:pairings, primary_key: false) do
      add :token, :text, primary_key: true
      add :account_id, references(:accounts, type: :uuid, on_delete: :delete_all), null: false

      add :desktop_device_id, references(:devices, type: :uuid, on_delete: :delete_all),
        null: false

      add :expires_at, :utc_datetime_usec, null: false
      add :consumed_at, :utc_datetime_usec
      add :created_at, :utc_datetime_usec, null: false
    end

    create index(:pairings, [:account_id])
    create index(:pairings, [:desktop_device_id])
    create index(:pairings, [:expires_at])
  end
end
