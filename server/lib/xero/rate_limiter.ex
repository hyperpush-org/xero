defmodule Xero.RateLimiter do
  @moduledoc """
  ETS-backed rate limiter (Hammer v7).

  Usage in a router pipeline:

      pipeline :api do
        plug :accepts, ["json"]
        plug Xero.RateLimitPlug
      end

  Or call `hit/3` directly in a controller for custom buckets.
  """

  use Hammer, backend: :ets
end
