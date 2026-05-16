defmodule Xero.GitHubAuth do
  @moduledoc """
  Server-owned GitHub OAuth flow for Xero desktop account linking.

  The desktop app receives only public session metadata plus an opaque session id.
  GitHub client secrets, access tokens, and token exchange logic remain server-side.
  """

  use GenServer

  alias Xero.Remote
  alias Xero.GitHubAuth.Session
  alias Xero.Repo

  @authorize_url "https://github.com/login/oauth/authorize"
  @token_url "https://github.com/login/oauth/access_token"
  @user_url "https://api.github.com/user"
  @default_scopes ["read:user", "user:email"]
  @session_header "x-xero-github-session-id"
  @session_token_salt "github auth session access token v1"
  @session_token_max_age_seconds 365 * 24 * 60 * 60
  @user_agent "Xero/0.1"

  def start_link(_opts) do
    GenServer.start_link(__MODULE__, %{}, name: __MODULE__)
  end

  @impl true
  def init(_opts) do
    {:ok, %{flows: %{}, states: %{}, sessions: %{}}}
  end

  def session_header, do: @session_header

  def reset! do
    GenServer.call(__MODULE__, :reset)
  end

  def clear_in_memory_state_for_test! do
    GenServer.call(__MODULE__, :clear_in_memory_state_for_test)
  end

  def start_login(kind \\ "desktop", attrs \\ %{}) do
    with {:ok, config} <- oauth_config(),
         {:ok, kind} <- login_kind(kind) do
      flow_id = random_token()
      state_token = random_token()

      flow = %{
        flow_id: flow_id,
        state: state_token,
        kind: kind,
        name: string_attr(attrs, "name"),
        user_agent: string_attr(attrs, "user_agent"),
        redirect_to: string_attr(attrs, "redirect_to"),
        status: :pending,
        inserted_at: DateTime.utc_now()
      }

      :ok = GenServer.call(__MODULE__, {:put_flow, flow})

      {:ok,
       %{
         authorization_url: authorization_url(config, state_token),
         redirect_uri: config.redirect_uri,
         flow_id: flow_id,
         kind: Atom.to_string(kind)
       }}
    end
  end

  def complete_callback(%{"state" => state_token} = params) when is_binary(state_token) do
    cond do
      error_code = params["error"] ->
        error =
          github_error(
            error_code,
            params["error_description"] || "GitHub did not authorize this Xero login."
          )

        fail_state(state_token, error)
        {:error, error}

      code = params["code"] ->
        complete_code_callback(state_token, code)

      true ->
        error("github_oauth_callback_invalid", "GitHub callback did not include an OAuth code.")
        |> then(fn error -> {:error, error} end)
    end
  end

  def complete_callback(_params) do
    {:error,
     error(
       "github_oauth_state_missing",
       "GitHub callback did not include the OAuth state token."
     )}
  end

  def poll_flow(flow_id) when is_binary(flow_id) and flow_id != "" do
    GenServer.call(__MODULE__, {:poll_flow, flow_id})
  end

  def poll_flow(_flow_id) do
    {:error, error("github_oauth_flow_missing", "GitHub login flow id is required.")}
  end

  def get_session(session_id) when is_binary(session_id) and session_id != "" do
    GenServer.call(__MODULE__, {:get_session, session_id})
  end

  def get_session(_session_id), do: {:ok, nil}

  def logout(session_id) when is_binary(session_id) and session_id != "" do
    GenServer.call(__MODULE__, {:logout, session_id})
  end

  def logout(_session_id), do: :ok

  def complete_state(state_token, session_id, stored_session, attrs \\ %{})
      when is_binary(state_token) and is_binary(session_id) do
    GenServer.call(__MODULE__, {:complete_state, state_token, session_id, stored_session, attrs})
  end

  def public_session(%{user: user, scope: scope, created_at: created_at} = session) do
    %{
      "user" => user,
      "scope" => scope || "",
      "createdAt" => created_at
    }
    |> merge_public_remote_session(session)
  end

  def stored_session(access_token, token_type, scope, user) do
    %{
      access_token: access_token,
      token_type: token_type || "bearer",
      scope: scope || "",
      user: user,
      created_at: DateTime.utc_now() |> DateTime.to_iso8601()
    }
  end

  defp merge_public_remote_session(public, session) do
    public
    |> put_optional("kind", session_value(session, :kind))
    |> put_optional("accountId", session_value(session, :account_id))
    |> put_optional("deviceId", session_value(session, :device_id))
    |> put_optional("relayToken", session_value(session, :relay_token))
    |> put_optional("relayTokenExpiresAt", session_value(session, :relay_token_expires_at))
    |> put_optional("csrfToken", session_value(session, :csrf_token))
    |> put_optional("account", session_value(session, :account))
  end

  defp put_optional(map, _key, nil), do: map
  defp put_optional(map, _key, ""), do: map
  defp put_optional(map, key, value), do: Map.put(map, key, value)

  @impl true
  def handle_call(:reset, _from, _state) do
    :ok = delete_all_sessions()
    {:reply, :ok, %{flows: %{}, states: %{}, sessions: %{}}}
  end

  def handle_call(:clear_in_memory_state_for_test, _from, _state) do
    {:reply, :ok, %{flows: %{}, states: %{}, sessions: %{}}}
  end

  def handle_call({:put_flow, flow}, _from, state) do
    next_state = %{
      state
      | flows: Map.put(state.flows, flow.flow_id, flow),
        states: Map.put(state.states, flow.state, flow.flow_id)
    }

    {:reply, :ok, next_state}
  end

  def handle_call({:poll_flow, flow_id}, _from, state) do
    reply =
      case Map.get(state.flows, flow_id) do
        nil ->
          {:error,
           error("github_oauth_flow_not_found", "GitHub login flow was not found or expired.")}

        %{status: :pending} ->
          :pending

        %{status: :complete, session_id: session_id, session: session} ->
          {:complete, session_id, session}

        %{status: :error, error: error} ->
          {:error, error}
      end

    {:reply, reply, state}
  end

  def handle_call({:get_session, session_id}, _from, state) do
    case Map.fetch(state.sessions, session_id) do
      {:ok, session} ->
        {:reply, {:ok, session}, state}

      :error ->
        case fetch_persisted_session(session_id) do
          {:ok, nil} ->
            {:reply, {:ok, nil}, state}

          {:ok, session} ->
            {:reply, {:ok, session},
             %{state | sessions: Map.put(state.sessions, session_id, session)}}

          {:error, error} ->
            {:reply, {:error, error}, state}
        end
    end
  end

  def handle_call({:logout, session_id}, _from, state) do
    _ = revoke_session_device(Map.get(state.sessions, session_id))

    case delete_session(session_id) do
      :ok ->
        {:reply, :ok, %{state | sessions: Map.delete(state.sessions, session_id)}}

      {:error, error} ->
        {:reply, {:error, error}, state}
    end
  end

  def handle_call(
        {:complete_state, state_token, session_id, stored_session, attrs},
        _from,
        state
      ) do
    case Map.get(state.states, state_token) do
      nil ->
        {:reply,
         {:error,
          error("github_oauth_flow_not_found", "GitHub login flow was not found or expired.")},
         state}

      flow_id ->
        flow = Map.fetch!(state.flows, flow_id)

        with {:ok, linked} <-
               Remote.complete_github_login(stored_session_user(stored_session), flow.kind, %{
                 name: string_attr(attrs, "name") || flow.name,
                 user_agent: string_attr(attrs, "user_agent") || flow.user_agent
               }),
             session <- remote_session(stored_session, flow.kind, linked),
             :ok <- persist_session(session_id, session) do
          flow =
            flow
            |> Map.merge(%{status: :complete, session_id: session_id, session: session})

          next_state = %{
            state
            | flows: Map.put(state.flows, flow_id, flow),
              sessions: Map.put(state.sessions, session_id, session)
          }

          {:reply, :ok, next_state}
        else
          {:error, %{"code" => _code} = error} ->
            {:reply, {:error, error}, state}

          {:error, reason} when is_atom(reason) ->
            {:reply, {:error, remote_error(reason)}, state}

          {:error, {:validation, _changeset}} ->
            {:reply,
             {:error,
              error(
                "github_remote_account_link_failed",
                "Could not link the GitHub account to a remote relay device."
              )}, state}
        end
    end
  end

  def handle_call({:fail_state, state_token, error}, _from, state) do
    next_state =
      case Map.get(state.states, state_token) do
        nil ->
          state

        flow_id ->
          flow =
            state.flows
            |> Map.fetch!(flow_id)
            |> Map.merge(%{status: :error, error: error})

          %{state | flows: Map.put(state.flows, flow_id, flow)}
      end

    {:reply, :ok, next_state}
  end

  def handle_call({:flow_for_state, state_token}, _from, state) do
    reply =
      with {:ok, flow_id} <- Map.fetch(state.states, state_token),
           {:ok, flow} <- Map.fetch(state.flows, flow_id) do
        {:ok, flow}
      end

    {:reply, reply, state}
  end

  defp complete_code_callback(state_token, code) do
    with {:ok, flow} <- flow_for_state(state_token),
         {:ok, config} <- oauth_config(),
         {:ok, token} <- exchange_code_for_token(config, code),
         {:ok, user} <- fetch_github_user(token.access_token) do
      session_id = random_token()
      session = stored_session(token.access_token, token.token_type, token.scope, user)

      case complete_state(state_token, session_id, session) do
        :ok ->
          case get_session(session_id) do
            {:ok, completed_session} ->
              {:ok,
               %{
                 session_id: session_id,
                 kind: Atom.to_string(flow.kind),
                 redirect_to: flow.redirect_to,
                 session: public_session(completed_session)
               }}

            {:error, error} ->
              {:error, error}
          end

        {:error, error} -> {:error, error}
      end
    else
      {:error, error} ->
        fail_state(state_token, error)
        {:error, error}
    end
  end

  defp flow_for_state(state_token) do
    case GenServer.call(__MODULE__, {:flow_for_state, state_token}) do
      {:ok, flow} ->
        {:ok, flow}

      :error ->
        {:error,
         error("github_oauth_flow_not_found", "GitHub login flow was not found or expired.")}
    end
  end

  defp fail_state(state_token, error) do
    GenServer.call(__MODULE__, {:fail_state, state_token, error})
  end

  defp persist_session(session_id, stored_session) do
    with {:ok, attrs} <- persisted_session_attrs(session_id, stored_session),
         {:ok, _session} <-
           %Session{}
           |> Session.changeset(attrs)
           |> Repo.insert(
             on_conflict:
               {:replace,
                [
                  :encrypted_access_token,
                  :token_type,
                  :scope,
                  :user,
                  :created_at,
                  :kind,
                  :account_id,
                  :device_id,
                  :csrf_token,
                  :updated_at
                ]},
             conflict_target: :session_id
           ) do
      :ok
    else
      {:error, %Ecto.Changeset{}} ->
        {:error, error("github_session_store_failed", "Could not save the GitHub session.")}

      {:error, %{"code" => _code} = error} ->
        {:error, error}
    end
  rescue
    exception ->
      {:error,
       error(
         "github_session_store_unavailable",
         "Could not save the GitHub session: #{Exception.message(exception)}"
       )}
  end

  defp persisted_session_attrs(session_id, stored_session) do
    access_token =
      Map.get(stored_session, :access_token) || Map.get(stored_session, "access_token")

    if is_binary(access_token) and access_token != "" do
      encrypted_access_token =
        Phoenix.Token.encrypt(XeroWeb.Endpoint, @session_token_salt, access_token,
          max_age: @session_token_max_age_seconds
        )

      {:ok,
       %{
         session_id: session_id,
         encrypted_access_token: encrypted_access_token,
         token_type:
           Map.get(stored_session, :token_type) || Map.get(stored_session, "token_type") ||
             "bearer",
         scope: Map.get(stored_session, :scope) || Map.get(stored_session, "scope") || "",
         user: Map.get(stored_session, :user) || Map.get(stored_session, "user") || %{},
         created_at:
           Map.get(stored_session, :created_at) || Map.get(stored_session, "created_at") ||
             DateTime.to_iso8601(DateTime.utc_now()),
         kind: Map.get(stored_session, :kind) || Map.get(stored_session, "kind") || "desktop",
         account_id: Map.get(stored_session, :account_id) || Map.get(stored_session, "account_id"),
         device_id: Map.get(stored_session, :device_id) || Map.get(stored_session, "device_id"),
         csrf_token: Map.get(stored_session, :csrf_token) || Map.get(stored_session, "csrf_token")
       }}
    else
      {:error,
       error(
         "github_session_store_failed",
         "GitHub session did not include an access token."
       )}
    end
  end

  defp fetch_persisted_session(session_id) do
    case Repo.get(Session, session_id) do
      nil ->
        {:ok, nil}

      session ->
        persisted_session_to_stored_session(session)
    end
  rescue
    exception ->
      {:error,
       error(
         "github_session_store_unavailable",
         "Could not read the saved GitHub session: #{Exception.message(exception)}"
       )}
  end

  defp persisted_session_to_stored_session(session) do
    case Phoenix.Token.decrypt(
           XeroWeb.Endpoint,
           @session_token_salt,
           session.encrypted_access_token,
           max_age: @session_token_max_age_seconds
         ) do
      {:ok, access_token} when is_binary(access_token) ->
        stored_session = %{
          access_token: access_token,
          token_type: session.token_type || "bearer",
          scope: session.scope || "",
          user: session.user || %{},
          created_at: session.created_at,
          kind: session.kind || "desktop",
          account_id: session.account_id,
          device_id: session.device_id,
          csrf_token: session.csrf_token
        }

        case refresh_public_relay_token(stored_session) do
          %{relay_token: relay_token} = refreshed when is_binary(relay_token) ->
            {:ok, refreshed}

          _stale_session ->
            _ = Repo.delete(session)
            {:ok, nil}
        end

      {:error, _reason} ->
        _ = Repo.delete(session)
        {:ok, nil}
    end
  end

  defp persisted_session_to_plain_map(session) do
    %{
      kind: session.kind || "desktop",
      account_id: session.account_id,
      device_id: session.device_id,
      csrf_token: session.csrf_token,
      user: session.user || %{},
      scope: session.scope || "",
      created_at: session.created_at
    }
  end

  defp remote_session(stored_session, kind, linked) do
    Map.merge(stored_session, %{
      kind: Atom.to_string(kind),
      account_id: linked.account.id,
      device_id: linked.device.id,
      relay_token: linked.token,
      relay_token_expires_at: linked.token_expires_at,
      csrf_token: linked.csrf_token,
      account: %{
        "id" => linked.account.id,
        "githubUserId" => linked.account.github_user_id,
        "githubLogin" => linked.account.github_login,
        "githubAvatarUrl" => linked.account.github_avatar_url
      }
    })
  end

  defp refresh_public_relay_token(%{device_id: device_id} = session) when is_binary(device_id) do
    case Remote.device_for_session(session) do
      {:ok, device} ->
        Map.merge(session, %{
          relay_token: Remote.issue_relay_token(device),
          relay_token_expires_at: Remote.relay_token_expires_at()
        })

      {:error, _reason} ->
        session
    end
  end

  defp refresh_public_relay_token(session), do: session

  defp revoke_session_device(%{device_id: device_id} = session) when is_binary(device_id) do
    with {:ok, device} <- Remote.device_for_session(session) do
      Remote.revoke_device(device, device_id)
    end
  end

  defp revoke_session_device(_session), do: :ok

  defp stored_session_user(session) do
    session_value(session, :user) || %{}
  end

  defp session_value(map, key) when is_map(map) do
    Map.get(map, key) || Map.get(map, Atom.to_string(key))
  end

  defp session_value(_map, _key), do: nil

  defp delete_session(session_id) do
    case Repo.get(Session, session_id) do
      nil -> :ok
      session ->
        _ = revoke_session_device(persisted_session_to_plain_map(session))
        Repo.delete(session) |> then(fn _result -> :ok end)
    end
  rescue
    exception ->
      {:error,
       error(
         "github_session_store_unavailable",
         "Could not delete the saved GitHub session: #{Exception.message(exception)}"
       )}
  end

  defp delete_all_sessions do
    Repo.delete_all(Session)
    :ok
  end

  defp oauth_config do
    with {:ok, client_id} <- env("GITHUB_CLIENT_ID"),
         {:ok, client_secret} <- env("GITHUB_CLIENT_SECRET") do
      {:ok,
       %{
         client_id: client_id,
         client_secret: client_secret,
         redirect_uri: github_redirect_uri()
       }}
    end
  end

  defp env(name) do
    case system_env(name) do
      {:ok, value} ->
        {:ok, value}

      :error ->
        load_dotenv_files()

        case system_env(name) do
          {:ok, value} -> {:ok, value}
          :error -> missing_env(name)
        end
    end
  end

  defp system_env(name) do
    case System.get_env(name) do
      value when is_binary(value) ->
        case String.trim(value) do
          "" -> :error
          trimmed -> {:ok, trimmed}
        end

      _ ->
        :error
    end
  end

  defp load_dotenv_files do
    if System.get_env("XERO_GITHUB_AUTH_SKIP_DOTENV") in ["1", "true"] do
      :ok
    else
      mix_env = System.get_env("MIX_ENV") || "dev"

      inputs =
        [
          ".env",
          ".env.#{mix_env}",
          ".env.#{mix_env}.local",
          "server/.env",
          "server/.env.#{mix_env}",
          "server/.env.#{mix_env}.local",
          System.get_env()
        ]
        |> Enum.uniq()

      case Dotenvy.source(inputs) do
        {:ok, parsed_env} -> System.put_env(parsed_env)
        {:error, _error} -> :ok
      end
    end
  end

  defp missing_env(name) do
    {:error,
     error(
       "github_oauth_unconfigured",
       "#{name} is not set on the Xero server. Configure server/.env and restart Phoenix."
     )}
  end

  defp github_redirect_uri do
    case env("GITHUB_REDIRECT_URI") do
      {:ok, value} ->
        value

      {:error, _error} ->
        "#{server_public_url()}/auth/github/callback"
    end
  end

  defp public_url_env(name) do
    case System.get_env(name) do
      value when is_binary(value) ->
        case String.trim(value) do
          "" -> nil
          trimmed -> trimmed
        end

      _ ->
        nil
    end
  end

  defp server_public_url do
    case public_url_env("XERO_SERVER_PUBLIC_URL") || public_url_env("PUBLIC_SERVER_URL") do
      value when is_binary(value) ->
        String.trim_trailing(value, "/")

      _ ->
        "http://127.0.0.1:#{System.get_env("PORT", "4000")}"
    end
  end

  defp authorization_url(config, state_token) do
    query =
      URI.encode_query(%{
        "client_id" => config.client_id,
        "redirect_uri" => config.redirect_uri,
        "state" => state_token,
        "scope" => Enum.join(@default_scopes, " "),
        "allow_signup" => "true"
      })

    "#{@authorize_url}?#{query}"
  end

  defp exchange_code_for_token(config, code) do
    response =
      Req.post(@token_url,
        headers: [
          {"accept", "application/json"},
          {"user-agent", @user_agent}
        ],
        form: [
          client_id: config.client_id,
          client_secret: config.client_secret,
          code: code,
          redirect_uri: config.redirect_uri
        ]
      )

    case response do
      {:ok, %Req.Response{status: status, body: body}} when status in 200..299 ->
        token_from_body(body)

      {:ok, %Req.Response{status: status, body: body}} ->
        {:error,
         error(
           "github_token_exchange_rejected",
           "GitHub rejected the token exchange with HTTP #{status}: #{response_error(body)}"
         )}

      {:error, reason} ->
        {:error,
         error(
           "github_token_exchange_failed",
           "Could not reach GitHub token endpoint: #{Exception.message(reason)}"
         )}
    end
  end

  defp token_from_body(body) do
    access_token = body_value(body, "access_token")

    if is_binary(access_token) and access_token != "" do
      {:ok,
       %{
         access_token: access_token,
         token_type: body_value(body, "token_type") || "bearer",
         scope: body_value(body, "scope") || ""
       }}
    else
      {:error,
       error(
         "github_token_exchange_unknown",
         "GitHub token endpoint returned a response without an access token."
       )}
    end
  end

  defp fetch_github_user(access_token) do
    response =
      Req.get(@user_url,
        headers: [
          {"accept", "application/vnd.github+json"},
          {"authorization", "Bearer #{access_token}"},
          {"user-agent", @user_agent},
          {"x-github-api-version", "2022-11-28"}
        ]
      )

    case response do
      {:ok, %Req.Response{status: status, body: body}} when status in 200..299 ->
        user_from_body(body)

      {:ok, %Req.Response{status: status, body: body}} ->
        {:error,
         error(
           "github_user_fetch_rejected",
           "GitHub returned HTTP #{status} for the authenticated user: #{response_error(body)}"
         )}

      {:error, reason} ->
        {:error,
         error(
           "github_user_fetch_failed",
           "Could not fetch GitHub user: #{Exception.message(reason)}"
         )}
    end
  end

  defp user_from_body(body) do
    id = body_value(body, "id")
    login = body_value(body, "login")
    avatar_url = body_value(body, "avatar_url")
    html_url = body_value(body, "html_url")

    cond do
      !is_integer(id) ->
        {:error,
         error("github_user_decode_failed", "GitHub user response did not include an id.")}

      !is_binary(login) or login == "" ->
        {:error,
         error("github_user_decode_failed", "GitHub user response did not include a login.")}

      true ->
        {:ok,
         %{
           "id" => id,
           "login" => login,
           "name" => body_value(body, "name"),
           "email" => body_value(body, "email"),
           "avatarUrl" => avatar_url || "",
           "htmlUrl" => html_url || ""
         }}
    end
  end

  defp body_value(body, key) when is_map(body) do
    Map.get(body, key) || Map.get(body, atom_body_key(key))
  end

  defp body_value(_body, _key), do: nil

  defp atom_body_key("access_token"), do: :access_token
  defp atom_body_key("token_type"), do: :token_type
  defp atom_body_key("scope"), do: :scope
  defp atom_body_key("id"), do: :id
  defp atom_body_key("login"), do: :login
  defp atom_body_key("avatar_url"), do: :avatar_url
  defp atom_body_key("html_url"), do: :html_url
  defp atom_body_key("name"), do: :name
  defp atom_body_key("email"), do: :email
  defp atom_body_key("error_description"), do: :error_description
  defp atom_body_key("message"), do: :message
  defp atom_body_key("error"), do: :error
  defp atom_body_key(_key), do: nil

  defp response_error(body) when is_map(body) do
    body_value(body, "error_description") || body_value(body, "message") ||
      body_value(body, "error") || "unexpected response"
  end

  defp response_error(body) when is_binary(body), do: body
  defp response_error(_body), do: "unexpected response"

  defp github_error(code, message) do
    error("github_oauth_rejected", "GitHub rejected the login (#{code}): #{message}")
  end

  defp remote_error(:invalid_kind) do
    error("github_oauth_kind_invalid", "GitHub login kind must be desktop or web.")
  end

  defp remote_error(:missing_github_user_id) do
    error("github_user_decode_failed", "GitHub user response did not include an id.")
  end

  defp remote_error(_reason) do
    error("github_remote_account_link_failed", "Could not link the GitHub account.")
  end

  defp error(code, message) do
    %{"code" => code, "message" => message}
  end

  defp login_kind(kind) when kind in [:desktop, :web], do: {:ok, kind}
  defp login_kind("desktop"), do: {:ok, :desktop}
  defp login_kind("web"), do: {:ok, :web}
  defp login_kind(_kind), do: {:error, remote_error(:invalid_kind)}

  defp string_attr(attrs, key) when is_map(attrs) do
    Map.get(attrs, key) || Map.get(attrs, known_attr_key(key))
  end

  defp string_attr(_attrs, _key), do: nil

  defp known_attr_key("name"), do: :name
  defp known_attr_key("user_agent"), do: :user_agent
  defp known_attr_key("redirect_to"), do: :redirect_to
  defp known_attr_key(_key), do: nil

  defp random_token do
    32
    |> :crypto.strong_rand_bytes()
    |> Base.url_encode64(padding: false)
  end
end
