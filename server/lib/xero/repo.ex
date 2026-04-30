defmodule Xero.Repo do
  use Ecto.Repo,
    otp_app: :xero,
    adapter: Ecto.Adapters.Postgres
end
