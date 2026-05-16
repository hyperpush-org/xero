defmodule Xero.Remote.Device do
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @derive {Jason.Encoder,
           only: [
             :id,
             :account_id,
             :kind,
             :name,
             :user_agent,
             :last_seen,
             :created_at,
             :revoked_at
           ]}
  schema "devices" do
    field :account_id, :binary_id
    field :kind, Ecto.Enum, values: [:desktop, :web]
    field :name, :string
    field :user_agent, :string
    field :last_seen, :utc_datetime_usec
    field :created_at, :utc_datetime_usec
    field :revoked_at, :utc_datetime_usec
  end

  def changeset(device, attrs) do
    device
    |> cast(attrs, [
      :account_id,
      :kind,
      :name,
      :user_agent,
      :last_seen,
      :created_at,
      :revoked_at
    ])
    |> validate_required([:account_id, :kind, :created_at])
  end
end
