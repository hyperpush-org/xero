defmodule XeroWeb.RemoteFallbackController do
  use XeroWeb, :controller

  def call(conn, {:error, {:validation, changeset}}) do
    conn
    |> put_status(:unprocessable_entity)
    |> json(%{error: %{code: "validation_failed", details: errors_on(changeset)}})
  end

  def call(conn, {:error, reason}) do
    {status, message} = status_and_message(reason)

    conn
    |> put_status(status)
    |> json(%{error: %{code: Atom.to_string(reason), message: message}})
  end

  defp status_and_message(:desktop_required), do: {:forbidden, "A desktop device token is required."}
  defp status_and_message(:unauthorized), do: {:unauthorized, "Remote credentials are invalid."}
  defp status_and_message(:missing_field), do: {:bad_request, "A required field is missing."}
  defp status_and_message(:invalid_kind), do: {:bad_request, "Device kind must be desktop or web."}
  defp status_and_message(:missing_github_user_id),
    do: {:bad_request, "GitHub user id is required."}
  defp status_and_message(:not_found), do: {:not_found, "Device was not found."}
  defp status_and_message(:cannot_revoke_self), do: {:bad_request, "Desktop device cannot revoke itself."}
  defp status_and_message(_reason), do: {:bad_request, "Remote request failed."}

  defp errors_on(changeset) do
    Ecto.Changeset.traverse_errors(changeset, fn {message, opts} ->
      Regex.replace(~r"%{(\w+)}", message, fn _, key ->
        opts |> Keyword.get(String.to_existing_atom(key), key) |> to_string()
      end)
    end)
  end
end
