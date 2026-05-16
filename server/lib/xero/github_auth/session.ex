defmodule Xero.GitHubAuth.Session do
  @moduledoc false

  use Ecto.Schema

  import Ecto.Changeset

  @primary_key {:session_id, :string, []}
  schema "github_auth_sessions" do
    field :encrypted_access_token, :string
    field :token_type, :string, default: "bearer"
    field :scope, :string, default: ""
    field :user, :map
    field :created_at, :string
    field :kind, :string
    field :account_id, :binary_id
    field :device_id, :binary_id
    field :csrf_token, :string

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(session, attrs) do
    session
    |> cast(attrs, [
      :session_id,
      :encrypted_access_token,
      :token_type,
      :scope,
      :user,
      :created_at,
      :kind,
      :account_id,
      :device_id,
      :csrf_token
    ])
    |> validate_required([
      :session_id,
      :encrypted_access_token,
      :token_type,
      :scope,
      :user,
      :created_at,
      :kind,
      :account_id,
      :device_id
    ])
    |> validate_inclusion(:kind, ["desktop", "web"])
  end
end
