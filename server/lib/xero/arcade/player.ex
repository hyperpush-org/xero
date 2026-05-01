defmodule Xero.Arcade.Player do
  @moduledoc false

  use Ecto.Schema

  import Ecto.Changeset

  @primary_key {:github_user_id, :integer, []}
  schema "game_players" do
    field :login, :string
    field :name, :string
    field :avatar_url, :string
    field :html_url, :string

    timestamps(type: :utc_datetime_usec)
  end

  def changeset(player, attrs) do
    player
    |> cast(attrs, [:github_user_id, :login, :name, :avatar_url, :html_url])
    |> validate_required([:github_user_id, :login])
    |> validate_number(:github_user_id, greater_than: 0)
  end
end
