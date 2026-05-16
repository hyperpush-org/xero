defmodule Xero.Remote do
  @moduledoc """
  Relay-side account and device operations for remote agentic workflow.
  """

  import Ecto.Query

  alias Xero.Remote.{Account, Device, Jwt}
  alias Xero.Repo

  @type login_kind :: :desktop | :web

  def complete_github_login(user, kind, attrs \\ %{}) do
    with {:ok, kind} <- normalize_kind(kind),
         {:ok, github_user_id} <- github_user_id(user) do
      now = now()

      account_attrs = %{
        github_user_id: github_user_id,
        github_login: github_login(user),
        github_avatar_url: github_avatar_url(user),
        created_at: now
      }

      case upsert_account(account_attrs) do
        {:ok, account} ->
          create_device_for_account(account, kind, attrs, now)

        {:error, changeset} ->
          {:error, {:validation, changeset}}
      end
    end
  end

  def issue_relay_token(%Device{revoked_at: nil} = device) do
    Jwt.issue_relay_token(device)
  end

  def issue_relay_token(_device), do: nil

  def relay_token_expires_at do
    System.system_time(:second) + Jwt.default_ttl_seconds()
  end

  def refresh_relay_token(%Device{revoked_at: nil} = device) do
    account = Repo.get!(Account, device.account_id)

    {:ok,
     %{
       token: Jwt.issue_relay_token(device),
       expires_at: relay_token_expires_at(),
       device_id: device.id,
       device_kind: Atom.to_string(device.kind),
       account_id: account.id,
       account: %{
         github_login: account.github_login,
         github_avatar_url: account.github_avatar_url
       }
     }}
  end

  def refresh_relay_token(_device), do: {:error, :unauthorized}

  def list_devices(%Device{revoked_at: nil} = device) do
    Device
    |> where([d], d.account_id == ^device.account_id)
    |> order_by([d], asc: d.created_at)
    |> Repo.all()
  end

  def list_devices(_device), do: []

  def revoke_device(%Device{revoked_at: nil} = current_device, device_id)
      when is_binary(device_id) do
    now = now()

    case Repo.get_by(Device, id: device_id, account_id: current_device.account_id) do
      nil ->
        {:error, :not_found}

      %Device{} = device ->
        device
        |> Device.changeset(%{revoked_at: now})
        |> Repo.update()
    end
  end

  def revoke_device(_device, _device_id), do: {:error, :unauthorized}

  def authenticate_device_token(token) do
    with {:ok, %{"account_id" => account_id, "kind" => kind} = claims} <- Jwt.verify(token),
         device_id when is_binary(device_id) <- claims["device_id"] || claims["sub"],
         %Device{} = device <- Repo.get(Device, device_id),
         true <- is_nil(device.revoked_at),
         true <- device.account_id == account_id,
         true <- Atom.to_string(device.kind) == kind do
      touch_device(device)
      {:ok, device}
    else
      _ -> {:error, :unauthorized}
    end
  end

  def device_for_session(%{device_id: device_id}) when is_binary(device_id) do
    device_by_id(device_id)
  end

  def device_for_session(%{"device_id" => device_id}) when is_binary(device_id) do
    device_by_id(device_id)
  end

  def device_for_session(_session), do: {:error, :unauthorized}

  def desktop_device_for_account(account_id, desktop_device_id) do
    Device
    |> where(
      [d],
      d.id == ^desktop_device_id and d.account_id == ^account_id and d.kind == :desktop and
        is_nil(d.revoked_at)
    )
    |> Repo.one()
  end

  defp upsert_account(attrs) do
    %Account{}
    |> Account.changeset(attrs)
    |> Repo.insert(
      on_conflict: {:replace, [:github_login, :github_avatar_url]},
      conflict_target: :github_user_id,
      returning: true
    )
  end

  defp create_device_for_account(account, kind, attrs, now) do
    device_attrs = %{
      account_id: account.id,
      kind: kind,
      name: device_name(kind, attrs),
      user_agent: string_attr(attrs, "user_agent"),
      last_seen: now,
      created_at: now
    }

    case %Device{} |> Device.changeset(device_attrs) |> Repo.insert() do
      {:ok, device} ->
        token = Jwt.issue_relay_token(device)

        :telemetry.execute([:xero, :remote, :oauth, :login], %{count: 1}, %{
          kind: kind,
          account_id: account.id
        })

        {:ok,
         %{
           account: account,
           device: device,
           token: token,
           token_expires_at: relay_token_expires_at(),
           csrf_token: csrf_token(kind)
         }}

      {:error, changeset} ->
        {:error, {:validation, changeset}}
    end
  end

  defp normalize_kind(kind) when kind in [:desktop, :web], do: {:ok, kind}
  defp normalize_kind("desktop"), do: {:ok, :desktop}
  defp normalize_kind("web"), do: {:ok, :web}
  defp normalize_kind(_kind), do: {:error, :invalid_kind}

  defp github_user_id(user) do
    case value(user, "id") do
      id when is_integer(id) -> {:ok, id}
      id when is_binary(id) -> parse_positive_integer(id)
      _ -> {:error, :missing_github_user_id}
    end
  end

  defp parse_positive_integer(value) do
    case Integer.parse(value) do
      {id, ""} when id > 0 -> {:ok, id}
      _ -> {:error, :missing_github_user_id}
    end
  end

  defp github_login(user), do: value(user, "login")
  defp github_avatar_url(user), do: value(user, "avatarUrl") || value(user, "avatar_url")

  defp device_name(:desktop, attrs), do: string_attr(attrs, "name") || "Xero Desktop"
  defp device_name(:web, attrs), do: string_attr(attrs, "name") || "Xero Web"

  defp csrf_token(:web), do: secure_token()
  defp csrf_token(:desktop), do: nil

  defp device_by_id(device_id) do
    case Repo.get(Device, device_id) do
      %Device{revoked_at: nil} = device -> {:ok, device}
      _ -> {:error, :unauthorized}
    end
  end

  defp touch_device(device) do
    device
    |> Device.changeset(%{last_seen: now()})
    |> Repo.update()
  end

  defp now do
    DateTime.utc_now() |> DateTime.truncate(:microsecond)
  end

  defp secure_token do
    32 |> :crypto.strong_rand_bytes() |> Base.url_encode64(padding: false)
  end

  defp value(map, key) when is_map(map) do
    Map.get(map, key) || Map.get(map, known_atom_key(key))
  end

  defp value(_map, _key), do: nil

  defp known_atom_key("id"), do: :id
  defp known_atom_key("login"), do: :login
  defp known_atom_key("avatarUrl"), do: :avatarUrl
  defp known_atom_key("avatar_url"), do: :avatar_url
  defp known_atom_key(_key), do: nil

  defp string_attr(attrs, key) when is_map(attrs) do
    Map.get(attrs, key) || Map.get(attrs, known_attr_key(key))
  end

  defp string_attr(_attrs, _key), do: nil

  defp known_attr_key("name"), do: :name
  defp known_attr_key("user_agent"), do: :user_agent
  defp known_attr_key(_key), do: nil
end
