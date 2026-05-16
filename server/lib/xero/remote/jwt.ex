defmodule Xero.Remote.Jwt do
  @moduledoc """
  Minimal HS256 JWT implementation for relay device credentials.

  Keeping this local avoids pulling another auth dependency into the relay while
  still producing standard JWTs for the desktop bridge and web clients.
  """

  @alg "HS256"
  @typ "JWT"
  @default_ttl_seconds 30 * 60

  def default_ttl_seconds, do: @default_ttl_seconds

  def issue_relay_token(device, opts \\ []) do
    now = System.system_time(:second)
    ttl = Keyword.get(opts, :ttl_seconds, @default_ttl_seconds)

    claims = %{
      "sub" => device.id,
      "device_id" => device.id,
      "account_id" => device.account_id,
      "kind" => Atom.to_string(device.kind),
      "iat" => now,
      "exp" => now + ttl
    }

    sign(claims)
  end

  def issue_device_token(device, opts \\ []), do: issue_relay_token(device, opts)

  def sign(claims) when is_map(claims) do
    header = encode_json(%{"alg" => @alg, "typ" => @typ})
    payload = encode_json(claims)
    data = header <> "." <> payload
    data <> "." <> signature(data)
  end

  def verify(token) when is_binary(token) do
    with [header_segment, payload_segment, signature_segment] <- String.split(token, ".", parts: 3),
         data = header_segment <> "." <> payload_segment,
         true <- Plug.Crypto.secure_compare(signature(data), signature_segment),
         {:ok, header} <- decode_json(header_segment),
         {:ok, payload} <- decode_json(payload_segment),
         :ok <- verify_header(header),
         :ok <- verify_expiry(payload) do
      {:ok, payload}
    else
      _ -> {:error, :invalid_token}
    end
  end

  def verify(_token), do: {:error, :invalid_token}

  defp verify_header(%{"alg" => @alg, "typ" => @typ}), do: :ok
  defp verify_header(_header), do: {:error, :invalid_header}

  defp verify_expiry(%{"exp" => exp}) when is_integer(exp) do
    if exp > System.system_time(:second), do: :ok, else: {:error, :expired}
  end

  defp verify_expiry(_payload), do: {:error, :missing_exp}

  defp encode_json(value) do
    value
    |> Jason.encode!()
    |> Base.url_encode64(padding: false)
  end

  defp decode_json(segment) do
    with {:ok, json} <- Base.url_decode64(segment, padding: false),
         {:ok, decoded} <- Jason.decode(json) do
      {:ok, decoded}
    else
      _ -> {:error, :invalid_json}
    end
  end

  defp signature(data) do
    :hmac
    |> :crypto.mac(:sha256, signing_key(), data)
    |> Base.url_encode64(padding: false)
  end

  defp signing_key do
    configured =
      Application.get_env(:xero, __MODULE__, [])
      |> Keyword.get(:signing_key)

    System.get_env("XERO_REMOTE_JWT_SIGNING_KEY") || configured ||
      raise "XERO_REMOTE_JWT_SIGNING_KEY is required for remote relay JWTs"
  end
end
