defmodule Xero.Repo.Migrations.CreateRemoteAgenticWorkflowTables do
  use Ecto.Migration

  def up do
    execute "CREATE TYPE remote_device_kind AS ENUM ('desktop', 'mobile')"

    create table(:accounts, primary_key: false) do
      add :id, :uuid, primary_key: true
      add :created_at, :utc_datetime_usec, null: false
    end

    create table(:devices, primary_key: false) do
      add :id, :uuid, primary_key: true
      add :account_id, references(:accounts, type: :uuid, on_delete: :delete_all), null: false
      add :kind, :remote_device_kind, null: false
      add :public_key, :text, null: false
      add :name, :text
      add :last_seen, :utc_datetime_usec
      add :created_at, :utc_datetime_usec, null: false
      add :revoked_at, :utc_datetime_usec
    end

    create index(:devices, [:account_id])
    create unique_index(:devices, [:public_key], where: "revoked_at IS NULL")

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

  def down do
    drop table(:pairings)
    drop table(:devices)
    drop table(:accounts)

    execute "DROP TYPE remote_device_kind"
  end
end
