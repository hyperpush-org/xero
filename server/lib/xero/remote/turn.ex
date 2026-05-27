defmodule Xero.Remote.Turn do
  @moduledoc """
  Issues WebRTC ICE server configuration for remote desktop streaming.

  TURN credentials use coturn's REST API format: the username embeds an expiry
  timestamp and the password is HMAC-SHA1(username, shared_secret), base64
  encoded. The shared secret never leaves the relay.
  """

  @default_ttl_seconds 600

  @spec ice_servers(keyword()) :: [map()]
  def ice_servers(opts \\ []) do
    config = Application.get_env(:xero, __MODULE__, [])
    now_seconds = Keyword.get(opts, :now_seconds, System.system_time(:second))
    nonce = Keyword.get(opts, :nonce, secure_nonce())
    ttl_seconds = ttl_seconds(Keyword.get(config, :ttl_seconds, @default_ttl_seconds))

    stun_servers =
      config
      |> Keyword.get(:stun_urls, [])
      |> normalize_urls()
      |> case do
        [] -> []
        urls -> [%{urls: urls}]
      end

    turn_servers =
      with urls when urls != [] <- normalize_urls(Keyword.get(config, :turn_urls, [])),
           secret when is_binary(secret) <- clean_secret(Keyword.get(config, :shared_secret)) do
        username = "#{now_seconds + ttl_seconds}:#{nonce}"

        [
          %{
            urls: urls,
            username: username,
            credential: turn_credential(secret, username),
            credential_type: "password",
            ttl_seconds: ttl_seconds
          }
        ]
      else
        _ -> []
      end

    stun_servers ++ turn_servers
  end

  defp normalize_urls(value) when is_binary(value) do
    value
    |> String.split(",", trim: true)
    |> Enum.map(&String.trim/1)
    |> Enum.reject(&(&1 == ""))
  end

  defp normalize_urls(value) when is_list(value) do
    value
    |> Enum.flat_map(&normalize_urls/1)
    |> Enum.uniq()
  end

  defp normalize_urls(_value), do: []

  defp clean_secret(value) when is_binary(value) do
    case String.trim(value) do
      "" -> nil
      secret -> secret
    end
  end

  defp clean_secret(_value), do: nil

  defp ttl_seconds(value) when is_integer(value) do
    value
    |> max(60)
    |> min(3600)
  end

  defp ttl_seconds(value) when is_binary(value) do
    case Integer.parse(String.trim(value)) do
      {seconds, ""} -> ttl_seconds(seconds)
      _ -> @default_ttl_seconds
    end
  end

  defp ttl_seconds(_value), do: @default_ttl_seconds

  defp turn_credential(secret, username) do
    :crypto.mac(:hmac, :sha, secret, username)
    |> Base.encode64()
  end

  defp secure_nonce do
    16
    |> :crypto.strong_rand_bytes()
    |> Base.url_encode64(padding: false)
  end
end
