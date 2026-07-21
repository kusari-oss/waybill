# Data Model: mikebom → waybill rename

**Feature**: 214-rename-to-waybill
**Date**: 2026-07-21

This rename has no runtime data model — no new types, no schema changes. Instead, this document catalogs the **rename-category taxonomy**: the classification each `mikebom` occurrence falls into determines whether it renames (functional identifier) or preserves (historical artifact).

## E1 — Functional identifier

**Definition**: Any string, symbol, path, or name that participates in program execution or emitted output at runtime.

**Renaming rule**: Every occurrence rewrites to the `waybill`-prefixed form.

**Instances**:

| Sub-category                    | Pre-rename example                                | Post-rename example                                | Count (approx) |
|---|---|---|---|
| Binary name                     | `mikebom`                                         | `waybill`                                          | 1              |
| Workspace crate name            | `mikebom-cli`                                     | `waybill-cli`                                      | 3              |
| Rust module path                | `mikebom_common::events::FileEvent`               | `waybill_common::events::FileEvent`                | ~800           |
| Cargo `[package].name`          | `name = "mikebom-common"`                         | `name = "waybill-common"`                          | 3              |
| Cargo intra-workspace dep       | `mikebom-common = { path = "../mikebom-common" }` | `waybill-common = { path = "../waybill-common" }`  | 2              |
| Environment variable            | `MIKEBOM_HELM_RENDER_TIMEOUT_SECS`                | `WAYBILL_HELM_RENDER_TIMEOUT_SECS`                 | 73             |
| Annotation key                  | `"mikebom:build-inclusion"`                       | `"waybill:build-inclusion"`                        | 192            |
| Log-line tool identifier        | `tracing::info!("mikebom trace start")`           | `tracing::info!("waybill trace start")`            | ~50            |
| SBOM tool-metadata name         | `tools[].name = "mikebom"`                        | `tools[].name = "waybill"`                         | 1 (× N formats)|
| Docker image tag                | `ghcr.io/kusari-oss/mikebom:v0.1.0-alpha.65`      | `ghcr.io/kusari-oss/waybill:v0.1.0-alpha.66`       | (release-workflow) |
| Release artifact filename       | `mikebom-v0.1.0-alpha.65-x86_64-unknown-linux-gnu.tar.gz` | `waybill-v0.1.0-alpha.66-x86_64-unknown-linux-gnu.tar.gz` | (release-workflow) |
| eBPF binary path                | `mikebom-ebpf/target/bpfel-unknown-none/release/mikebom-ebpf` | `waybill-ebpf/target/bpfel-unknown-none/release/waybill-ebpf` | 1 (loader.rs default_ebpf_path) |
| Directory name (in-repo)        | `mikebom-cli/`                                    | `waybill-cli/`                                     | 3              |
| Repository URL (Cargo.toml `repository`) | `github.com/kusari-sandbox/mikebom`       | `github.com/kusari-oss/waybill`                    | 1              |
| CI workflow file references     | `MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1`               | `WAYBILL_REQUIRE_SPDX3_VALIDATOR=1`                | ~20            |
| Constitution project name (§heading) | `# mikebom Constitution`                     | `# Waybill Constitution`                           | 1              |

**Invariants**:
- Every rename is a mechanical string substitution — no field reshuffling, no semantic change, no format change.
- `snake_case` identifiers (`mikebom_common`) preserve `snake_case`: → `waybill_common`.
- `kebab-case` identifiers (`mikebom-cli`) preserve `kebab-case`: → `waybill-cli`.
- `SCREAMING_SNAKE_CASE` (`MIKEBOM_*`) preserves the case: → `WAYBILL_*`.
- Prefixed literal (`"mikebom:foo"`) — swap the prefix only, preserve the suffix: → `"waybill:foo"`. The 192 distinct annotation keys become 192 distinct waybill-prefixed keys.

## E2 — Historical artifact

**Definition**: Any string, symbol, path, or name that documents past state, attribution, or authorship — not consumed at runtime, not part of the wire-shape contract.

**Preservation rule**: Left unchanged. May optionally add attribution text ("formerly known as mikebom") but existing mikebom references are preserved as authored.

**Instances**:

| Sub-category                    | Example                                                        | Rationale                                        |
|---|---|---|
| Historical spec directories     | `specs/001-*/` through `specs/213-*/`                          | Milestone artifacts authored under mikebom name  |
| Historical spec prose           | `specs/213-*/spec.md` — "The mikebom kernel-side filter…"      | Authorship record                                |
| Git commit messages             | `impl(213): kernel-side trace-noise filter…`                   | Git history preserves; not rewritten             |
| Prior tag names                 | `v0.1.0-alpha.7`..`v0.1.0-alpha.65`                            | Immutable release history                        |
| Prior Docker image tags on GHCR | `ghcr.io/kusari-oss/mikebom:v0.1.0-alpha.65`                   | Pre-rename image tags remain accessible          |
| Migration guide document title  | `docs/migration/mikebom-to-waybill.md`                         | Filename intentionally names the old→new mapping |
| README heritage sentence        | "Waybill (previously known as mikebom) is…"                    | One attribution sentence permitted per FR-009    |
| Audit reports                   | `docs/audits/2026-07-06-tauri-airflow.md` "…using mikebom v0.1.0-alpha.51…" | Historical audit result preserves the version + tool name at time of audit |
| `.git/` directory               | (all contents)                                                 | Immutable                                        |
| CHANGELOG entries (if present)  | "0.1.0-alpha.13: added mikebom scan --target-pid"              | Historical release entry                         |
| Personal user memory index      | `MEMORY.md`                                                    | User-personal; out of rename scope               |

**Invariants**:
- Historical artifacts are read-only during this rename. If a historical spec doc happens to reference an identifier that got renamed, we do NOT rewrite the historical doc; the reader can trace the rename via the migration guide.
- Preservation applies to WHOLE FILES for `specs/001-*` through `specs/213-*` — no partial edits inside those directories, even for cross-references (e.g., a m213 spec that mentions `waybill-cli` in a future-looking sentence stays as `mikebom-cli` because that's how it was authored).

## E3 — Wire-shape contract (subclass of Functional identifier)

**Definition**: The externally-observable JSON structure of emitted SBOMs (CDX 1.6, SPDX 2.3, SPDX 3.0.1). The annotation prefix `mikebom:*` is part of this contract; renaming it to `waybill:*` is a wire-shape change that downstream tooling must adapt to.

**Rename rule**: Prefix swap only. Every `mikebom:<X>` → `waybill:<X>`. The suffix (the part after the colon) is UNCHANGED. The 192 distinct suffixes remain 192 distinct suffixes.

**Fields affected on each SBOM format**:

- **CycloneDX 1.6 JSON**:
  - `metadata.tools[].name` = `"waybill"` (was `"mikebom"`)
  - `metadata.properties[].name` prefix — every `"mikebom:*"` key → `"waybill:*"`
  - Component-level `properties[].name` — same prefix swap
  - `metadata.properties[].name = "waybill:file-inventory-mode"` — full-mode override marker (per Strict Boundary #5)
- **SPDX 2.3 JSON**:
  - `creationInfo.creators[]` — one entry updates from `"Tool: mikebom-<version>"` to `"Tool: waybill-<version>"`
  - `annotations[].annotator` — same tool-name update
  - `annotations[].comment` — internal comment shapes (`MikebomAnnotationCommentV1` envelope) may embed the project name; JSON key names in the envelope preserve their existing case
- **SPDX 3.0.1 JSON**:
  - `Element.creationInfo.createdBy[]` — creator identity updates
  - `Annotation.subject` + `Annotation.subjectValue` — prefix swap
  - `Annotation.contentType` (if present) — `application/json` unchanged (mime type is not project-namespaced)

**Not affected** (preserved byte-identical):
- Every field VALUE except the prefix substring
- Field ORDERING (JSON key order where deterministic)
- NUL padding, whitespace, numeric formatting
- All specification-defined field names (`bomFormat`, `specVersion`, `serialNumber`, `metadata`, `components`, `dependencies`, `relationships`, etc.)

## Rename lifecycle (per instance)

```
[pre-rename: exists as `mikebom` form]
         │
         │  substitution pass N (one of 6, per R1 order)
         ▼
[post-rename: exists as `waybill` form]
         │
         │  CI grep-gate step
         ▼
[verified: `grep -rE '\bmikebom\b' <in-scope>` returns 0 hits]
         │
         │  merge to main
         ▼
[shipped: waybill v0.1.0-alpha.66 release]
```

No entity has a state transition beyond "pre-rename" and "post-rename". No entity has multiple valid states simultaneously in the target release (hard-break, no dual-emit per Clarification Q1).
