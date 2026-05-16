defmodule XeroWeb.RemoteChannelRateLimit do
  @moduledoc false

  def hit(socket, event) do
    scale = :timer.minutes(1)

    limit =
      Application.get_env(:xero, Xero.RateLimiter, [])
      |> Keyword.get(:per_minute, 60)

    key = "channel:#{socket.assigns.device_id}:#{event}"

    case Xero.RateLimiter.hit(key, scale, limit) do
      {:allow, _count} -> :ok
      {:deny, retry_after_ms} -> {:error, retry_after_ms}
    end
  end
end
