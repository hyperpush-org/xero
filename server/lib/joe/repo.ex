defmodule Joe.Repo do
  use Ecto.Repo,
    otp_app: :joe,
    adapter: Ecto.Adapters.Postgres
end
