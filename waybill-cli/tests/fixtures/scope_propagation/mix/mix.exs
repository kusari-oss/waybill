defmodule WaybillFixtureApp.MixProject do
  use Mix.Project

  def project do
    [
      app: :waybill_fixture_app,
      version: "1.0.0",
      elixir: "~> 1.16",
      deps: deps()
    ]
  end

  defp deps do
    [
      {:waybill_fixture_runtime, "~> 1.5"},
      {:waybill_fixture_devtool, "~> 2.0", only: [:dev, :test], runtime: false}
    ]
  end
end
