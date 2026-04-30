defmodule Xero.Application do
  # See https://hexdocs.pm/elixir/Application.html
  # for more information on OTP Applications
  @moduledoc false

  use Application

  @impl true
  def start(_type, _args) do
    children = [
      XeroWeb.Telemetry,
      Xero.Repo,
      {DNSCluster, query: Application.get_env(:xero, :dns_cluster_query) || :ignore},
      {Phoenix.PubSub, name: Xero.PubSub},
      Xero.GitHubAuth,
      # ETS-backed rate limiter (Hammer v7).
      Xero.RateLimiter,
      # Background jobs (Oban). Config in config/runtime.exs.
      {Oban, Application.fetch_env!(:xero, Oban)},
      # Start a worker by calling: Xero.Worker.start_link(arg)
      # {Xero.Worker, arg},
      # Start to serve requests, typically the last entry
      XeroWeb.Endpoint
    ]

    # See https://hexdocs.pm/elixir/Supervisor.html
    # for other strategies and supported options
    opts = [strategy: :one_for_one, name: Xero.Supervisor]
    Supervisor.start_link(children, opts)
  end

  # Tell Phoenix to update the endpoint configuration
  # whenever the application is updated.
  @impl true
  def config_change(changed, _new, removed) do
    XeroWeb.Endpoint.config_change(changed, removed)
    :ok
  end
end
