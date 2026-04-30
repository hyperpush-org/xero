defmodule Xero.Repo.Migrations.AddOban do
  use Ecto.Migration

  def up, do: Oban.Migration.up()

  # Bump the version number when calling `up/0` over time. Down rolls back to
  # version 1, which is the initial schema for Oban.
  def down, do: Oban.Migration.down(version: 1)
end
