# pip-pyproject-poetry-only fixture (milestone 068)

Minimal `pyproject.toml` declaring ONLY the pre-PEP-621
`[tool.poetry]` schema. Used by `tests/scan_pip.rs` to verify the
FR-002 skip rule: when a `pyproject.toml` has no `[project]` table
but DOES have `[tool.poetry]`, mikebom must skip main-module
emission and emit a `tracing::info!` noting the deferred Poetry
schema (per #104).

If demand for Poetry coverage materializes, a follow-up issue
will extend the reader; this fixture would then be repointed at
the new behavior.
