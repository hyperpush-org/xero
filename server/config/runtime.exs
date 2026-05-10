import Config

# Load .env files for non-prod environments. In production, env vars are
# expected to be provided by the host (systemd, fly.io, k8s, etc.) — except
# when running via `pnpm start` from a source checkout, signalled by
# `XERO_LAUNCH_MODE=local-source`. That path auto-generates `server/.env`
# with sensible defaults so first-time users don't hand-edit anything; real
# prod deploys never set the var, so they remain unaffected.
load_dotenv? =
  config_env() != :prod or System.get_env("XERO_LAUNCH_MODE") == "local-source"

if load_dotenv? do
  import Dotenvy

  {:ok, parsed_env} =
    source([
      ".env",
      ".env.#{config_env()}",
      ".env.#{config_env()}.local",
      System.get_env()
    ])

  System.put_env(parsed_env)
end

# config/runtime.exs is executed for all environments, including
# during releases. It is executed after compilation and before the
# system starts, so it is typically used to load production configuration
# and secrets from environment variables or elsewhere. Do not define
# any compile-time configuration in here, as it won't be applied.
# The block below contains prod specific runtime configuration.

# ## Using releases
#
# If you use `mix release`, you need to explicitly enable the server
# by passing the PHX_SERVER=true when you start it:
#
#     PHX_SERVER=true bin/xero start
#
# Alternatively, you can use `mix phx.gen.release` to generate a `bin/server`
# script that automatically sets the env var above.
if System.get_env("PHX_SERVER") do
  config :xero, XeroWeb.Endpoint, server: true
end

config :xero, XeroWeb.Endpoint, http: [port: String.to_integer(System.get_env("PORT", "4000"))]

# --- CORS (cors_plug) ---
# Comma-separated list of allowed origins. "*" is allowed for dev only.
default_cors_origins = ["http://localhost:3000", "http://127.0.0.1:3000", "tauri://localhost"]

cors_origins =
  System.get_env("CORS_ORIGINS", "")
  |> String.split(",", trim: true)
  |> Enum.map(&String.trim/1)
  |> Kernel.++(default_cors_origins)
  |> Enum.uniq()

config :cors_plug,
  origin: cors_origins,
  max_age: 86_400,
  methods: ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"],
  headers: ["authorization", "content-type", "x-xero-github-session-id"]

# --- Rate limiting (Hammer) ---
config :xero, Xero.RateLimiter,
  per_minute: String.to_integer(System.get_env("RATE_LIMIT_PER_MINUTE", "60"))

# --- Oban background jobs ---
oban_queues =
  System.get_env("OBAN_QUEUES", "default:10,mailers:5")
  |> String.split(",", trim: true)
  |> Enum.map(fn pair ->
    [name, limit] = String.split(pair, ":", parts: 2)
    {String.to_atom(String.trim(name)), String.to_integer(String.trim(limit))}
  end)

config :xero, Oban,
  repo: Xero.Repo,
  queues: oban_queues,
  plugins: [
    {Oban.Plugins.Pruner, max_age: 60 * 60 * 24 * 7},
    Oban.Plugins.Reindexer
  ]

if config_env() == :prod do
  database_url =
    System.get_env("DATABASE_URL") ||
      raise """
      environment variable DATABASE_URL is missing.
      For example: ecto://USER:PASS@HOST/DATABASE
      """

  maybe_ipv6 = if System.get_env("ECTO_IPV6") in ~w(true 1), do: [:inet6], else: []

  config :xero, Xero.Repo,
    # ssl: true,
    url: database_url,
    pool_size: String.to_integer(System.get_env("POOL_SIZE") || "10"),
    # For machines with several cores, consider starting multiple pools of `pool_size`
    # pool_count: 4,
    socket_options: maybe_ipv6

  # The secret key base is used to sign/encrypt cookies and other secrets.
  # A default value is used in config/dev.exs and config/test.exs but you
  # want to use a different value for prod and you most likely don't want
  # to check this value into version control, so we use an environment
  # variable instead.
  secret_key_base =
    System.get_env("SECRET_KEY_BASE") ||
      raise """
      environment variable SECRET_KEY_BASE is missing.
      You can generate one by calling: mix phx.gen.secret
      """

  host = System.get_env("PHX_HOST") || "example.com"

  config :xero, :dns_cluster_query, System.get_env("DNS_CLUSTER_QUERY")

  config :xero, XeroWeb.Endpoint,
    url: [host: host, port: 443, scheme: "https"],
    http: [
      # Enable IPv6 and bind on all interfaces.
      # Set it to  {0, 0, 0, 0, 0, 0, 0, 1} for local network only access.
      # See the documentation on https://hexdocs.pm/bandit/Bandit.html#t:options/0
      # for details about using IPv6 vs IPv4 and loopback vs public addresses.
      ip: {0, 0, 0, 0, 0, 0, 0, 0}
    ],
    secret_key_base: secret_key_base

  # ## SSL Support
  #
  # To get SSL working, you will need to add the `https` key
  # to your endpoint configuration:
  #
  #     config :xero, XeroWeb.Endpoint,
  #       https: [
  #         ...,
  #         port: 443,
  #         cipher_suite: :strong,
  #         keyfile: System.get_env("SOME_APP_SSL_KEY_PATH"),
  #         certfile: System.get_env("SOME_APP_SSL_CERT_PATH")
  #       ]
  #
  # The `cipher_suite` is set to `:strong` to support only the
  # latest and more secure SSL ciphers. This means old browsers
  # and clients may not be supported. You can set it to
  # `:compatible` for wider support.
  #
  # `:keyfile` and `:certfile` expect an absolute path to the key
  # and cert in disk or a relative path inside priv, for example
  # "priv/ssl/server.key". For all supported SSL configuration
  # options, see https://hexdocs.pm/plug/Plug.SSL.html#configure/1
  #
  # We also recommend setting `force_ssl` in your config/prod.exs,
  # ensuring no data is ever sent via http, always redirecting to https:
  #
  #     config :xero, XeroWeb.Endpoint,
  #       force_ssl: [hsts: true]
  #
  # Check `Plug.SSL` for all available options in `force_ssl`.

  # ## Configuring the mailer
  #
  # In production you need to configure the mailer to use a different adapter.
  # Here is an example configuration for Mailgun:
  #
  #     config :xero, Xero.Mailer,
  #       adapter: Swoosh.Adapters.Mailgun,
  #       api_key: System.get_env("MAILGUN_API_KEY"),
  #       domain: System.get_env("MAILGUN_DOMAIN")
  #
  # Most non-SMTP adapters require an API client. Swoosh supports Req, Hackney,
  # and Finch out-of-the-box. This configuration is typically done at
  # compile-time in your config/prod.exs:
  #
  #     config :swoosh, :api_client, Swoosh.ApiClient.Req
  #
  # See https://hexdocs.pm/swoosh/Swoosh.html#module-installation for details.
end
