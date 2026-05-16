defmodule XeroWeb.ChannelCase do
  @moduledoc """
  Test case for Phoenix Channels that need the SQL sandbox.
  """

  use ExUnit.CaseTemplate

  using do
    quote do
      @endpoint XeroWeb.Endpoint

      import Phoenix.ChannelTest
      import Phoenix.ConnTest, except: [connect: 2, connect: 3]
      import XeroWeb.ChannelCase
    end
  end

  setup tags do
    Xero.DataCase.setup_sandbox(tags)
    {:ok, conn: Phoenix.ConnTest.build_conn()}
  end
end
