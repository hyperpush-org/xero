defmodule XeroWeb.RemoteWebAccountChannel do
  use XeroWeb, :channel

  @impl true
  def join("account:" <> account_topic, _payload, socket) do
    expected = "#{socket.assigns.account_id}"

    if socket.assigns.device_kind == :web and account_topic == expected do
      :telemetry.execute([:xero, :remote, :channel, :join], %{count: 1}, %{
        kind: :web,
        topic: socket.topic
      })

      {:ok, %{account_id: socket.assigns.account_id}, socket}
    else
      {:error, %{reason: "unauthorized"}}
    end
  end
end
