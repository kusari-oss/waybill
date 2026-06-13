# Cargo fixtures — `mikebom:produces-binaries` (milestone 116 PR-A)

Three sub-fixtures exercising the per-source-rule extractor (FR-005) and the
union-merge rule (FR-012):

| Sub-fixture | Manifest shape | Expected `mikebom:produces-binaries` |
|---|---|---|
| `multi-source/` | `name = "fixture-baz"` + `[[bin]] name = "fixture-baz-alt"` + `src/main.rs` + `src/bin/fixture-baz-helper.rs` | `["fixture-baz", "fixture-baz-alt", "fixture-baz-helper"]` |
| `library-only/` | `[lib]` only — no `[[bin]]`, no `src/main.rs`, no `src/bin/*.rs` | property OMITTED |
| `workspace/` | Two members `crate-a` (one binary `crate-a`) and `crate-b` (one binary `crate-b`); top-level virtual workspace | EACH member's main-module component carries its OWN declaration (per spec clarification Q1 — declarations land on every workspace member, NOT consolidated onto the workspace root) |
