defmodule XeroWeb.RemoteChannelRateLimit do
  @moduledoc false

  @default_window_ms :timer.minutes(1)
  @default_per_minute 60
  @computer_use_default_limits %{
    "manual_critical" => 600,
    "manual_pointer" => 180,
    "stream_signaling" => 300,
    "stream_status" => 120,
    "stream_keyframe" => 60,
    "stream_quality" => 60,
    "stream_request" => 120,
    "computer_use_other" => 120
  }

  def hit(socket, event, payload \\ nil) do
    scale = :timer.minutes(1)
    config = Application.get_env(:xero, Xero.RateLimiter, [])
    {bucket, limit, metadata} = bucket_for(socket, event, payload, config)

    key = "channel:#{socket.assigns.device_id}:#{bucket}"

    case Xero.RateLimiter.hit(key, scale, limit) do
      {:allow, _count} ->
        :ok

      {:deny, retry_after_ms} ->
        {:error,
         Map.merge(metadata, %{
           reason: "rate_limited",
           retry_after_ms: retry_after_ms,
           retryAfterMs: retry_after_ms,
           limit: limit,
           window_ms: @default_window_ms,
           windowMs: @default_window_ms,
           bucket: bucket
         })}
    end
  end

  defp bucket_for(%{assigns: %{device_kind: :web}}, "frame", %{"kind" => kind} = payload, config)
       when is_binary(kind) do
    class = computer_use_class(kind, payload)
    limit = computer_use_limit(config, kind, class)

    {"frame:computer_use:#{class}", limit,
     %{
       class: class,
       kind: kind
     }}
  end

  defp bucket_for(_socket, event, _payload, config) do
    limit = Keyword.get(config, :per_minute, @default_per_minute)
    {event, limit, %{class: event}}
  end

  defp computer_use_limit(config, kind, class) do
    per_kind =
      config
      |> Keyword.get(:computer_use_per_minute, %{})
      |> normalize_limit_map()

    Map.get(per_kind, kind) ||
      Map.get(per_kind, class) ||
      Map.get(@computer_use_default_limits, class, @default_per_minute)
  end

  defp normalize_limit_map(map) when is_map(map), do: map
  defp normalize_limit_map(list) when is_list(list), do: Map.new(list)
  defp normalize_limit_map(_), do: %{}

  defp computer_use_class("computer_use_manual_control_input", payload)
       when is_map(payload) do
    case payload do
      %{"payload" => %{"action" => "mouse_move"}} -> "manual_pointer"
      _ -> "manual_critical"
    end
  end

  defp computer_use_class("computer_use_manual_control_input", _payload),
    do: "manual_critical"

  defp computer_use_class("computer_use_manual_control_request", _payload), do: "manual_critical"
  defp computer_use_class("computer_use_manual_control_grant", _payload), do: "manual_critical"

  defp computer_use_class("computer_use_manual_control_heartbeat", _payload),
    do: "manual_critical"

  defp computer_use_class("computer_use_manual_control_release", _payload), do: "manual_critical"
  defp computer_use_class("computer_use_stream_offer", _payload), do: "stream_signaling"
  defp computer_use_class("computer_use_stream_answer", _payload), do: "stream_signaling"
  defp computer_use_class("computer_use_stream_ice_candidate", _payload), do: "stream_signaling"
  defp computer_use_class("computer_use_stream_status", _payload), do: "stream_status"
  defp computer_use_class("computer_use_stream_set_quality", _payload), do: "stream_quality"
  defp computer_use_class("computer_use_stream_request_keyframe", _payload), do: "stream_keyframe"
  defp computer_use_class("computer_use_stream_request", _payload), do: "stream_request"
  defp computer_use_class("computer_use_stream_stop", _payload), do: "stream_request"
  defp computer_use_class(_kind, _payload), do: "computer_use_other"
end
