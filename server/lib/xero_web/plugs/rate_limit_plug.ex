defmodule XeroWeb.RateLimitPlug do
  @moduledoc """
  Per-IP rate limit plug. Bucket size + window come from app config under
  `:xero, Xero.RateLimiter` (see `config/runtime.exs`).

  Override per-route by passing `:scale_ms` and `:limit` in the plug opts:

      plug XeroWeb.RateLimitPlug, scale_ms: :timer.minutes(1), limit: 10
  """

  import Plug.Conn

  @behaviour Plug

  @default_scale_ms :timer.minutes(1)

  @impl true
  def init(opts), do: opts

  @impl true
  def call(conn, opts) do
    scale = Keyword.get(opts, :scale_ms, @default_scale_ms)

    limit =
      Keyword.get_lazy(opts, :limit, fn ->
        Application.get_env(:xero, Xero.RateLimiter, [])
        |> Keyword.get(:per_minute, 60)
      end)

    key = "ip:" <> client_ip(conn)

    case Xero.RateLimiter.hit(key, scale, limit) do
      {:allow, _count} ->
        conn

      {:deny, retry_after_ms} ->
        retry_after_s = div(retry_after_ms, 1000) + 1

        conn
        |> put_resp_header("retry-after", Integer.to_string(retry_after_s))
        |> put_resp_content_type("application/json")
        |> send_resp(429, ~s({"error":"rate_limited","retry_after":#{retry_after_s}}))
        |> halt()
    end
  end

  defp client_ip(conn) do
    case Plug.Conn.get_req_header(conn, "x-forwarded-for") do
      [first | _] -> first |> String.split(",") |> List.first() |> String.trim()
      [] -> conn.remote_ip |> :inet.ntoa() |> to_string()
    end
  end
end
