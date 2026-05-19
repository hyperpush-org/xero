defmodule XeroWeb.Presence do
  @moduledoc false

  use Phoenix.Presence,
    otp_app: :xero,
    pubsub_server: Xero.PubSub
end
