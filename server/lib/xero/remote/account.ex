defmodule Xero.Remote.Account do
  use Ecto.Schema
  import Ecto.Changeset

  @primary_key {:id, :binary_id, autogenerate: true}
  @derive {Jason.Encoder,
           only: [:id, :github_user_id, :github_login, :github_avatar_url, :created_at]}
  schema "accounts" do
    field :github_user_id, :integer
    field :github_login, :string
    field :github_avatar_url, :string
    field :created_at, :utc_datetime_usec
  end

  def changeset(account, attrs) do
    account
    |> cast(attrs, [:github_user_id, :github_login, :github_avatar_url, :created_at])
    |> validate_required([:github_user_id, :created_at])
    |> unique_constraint(:github_user_id)
  end
end
