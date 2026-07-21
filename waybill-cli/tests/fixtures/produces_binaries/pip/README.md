# pip fixtures — `waybill:produces-binaries` (milestone 116 PR-B)

| Sub-fixture | Manifest shape | Expected |
|---|---|---|
| `pyproject/` | `pyproject.toml` with `[project.scripts]` + `[project.gui-scripts]` | `["baz", "baz-gui"]` |
| `setupcfg-fallback/` | `pyproject.toml` carries package metadata but no scripts; `setup.cfg` declares `[options.entry_points] console_scripts` | `["baz"]` |
| `library-only/` | `pyproject.toml` with only `[project]` metadata, no scripts; no setup.cfg | property OMITTED |
