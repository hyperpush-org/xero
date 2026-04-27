defmodule Joe.Application do
  # See https://hexdocs.pm/elixir/Application.html
  # for more information on OTP Applications
  @moduledoc false

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      JoeWeb.Telemetry,
      Joe.Repo,
      {DNSCluster, query: Application.get_env(:joe, :dns_cluster_query) || :ignore},
      {Phoenix.PubSub, name: Joe.PubSub},
      Joe.GitHubAuth,
      # ETS-backed rate limiter (Hammer v7).
      Joe.RateLimiter,
      # Background jobs (Oban). Config in config/runtime.exs.
      {Oban, Application.fetch_env!(:joe, Oban)},
      # Start a worker by calling: Joe.Worker.start_link(arg)
      # {Joe.Worker, arg},
      # Start to serve requests, typically the last entry
      JoeWeb.Endpoint
    ]

    # See https://hexdocs.pm/elixir/Supervisor.html
    # for other strategies and supported options
    opts = [strategy: :one_for_one, name: Joe.Supervisor]
    Supervisor.start_link(children, opts)
  end

  # Tell Phoenix to update the endpoint configuration
  # whenever the application is updated.
  @impl true
  def config_change(changed, _new, removed) do
    JoeWeb.Endpoint.config_change(changed, removed)
    :ok
  end
end
