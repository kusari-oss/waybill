# Contracts — milestone 169

**Feature**: [spec.md](../spec.md) | **Plan**: [plan.md](../plan.md) | **Data model**: [data-model.md](../data-model.md)

## No new external interfaces

Milestone 169 is a per-format ecosystem-coverage addition. No new CLI flags, no new wire format shape, no new parity-catalog rows.

## Contract deltas (VALUE additions to existing rows)

### Existing C50 `mikebom:evidence-kind` row — 2 new legal values

| Existing values (pre-169) | New in m169 |
|---|---|
| `rpm-file`, `rpmdb-sqlite`, `rpmdb-bdb`, `dynamic-linkage`, `elf-note-package`, `embedded-version-string`, ... | `ipk-file` (FR-009) — archive-file source, parity with `rpm-file` |
| | `opkg-status-db` (FR-015) — installed-DB source, parity with `rpmdb-sqlite` |

Wire shape (CDX property / SPDX 2.3 annotation / SPDX 3 Annotation) is UNCHANGED. Only the value set expands.

### NEW annotation `mikebom:dep-alternative-alternates` (Q2 clarification)

- **Wire shape**: JSON array of package names, e.g., `["libmbedtls-12", "libssl3"]`
- **Semantics**: attached to a SOURCE component whose `Depends:` field contained an alternative-list (`Depends: pkg-a | pkg-b`). The dep-edge points to the first-listed alternative (opkg's runtime default per Q2); this annotation lists the fallback alternatives so downstream consumers can access them without BFS reachability inflation.
- **Emission scope**: `pkg:opkg/*` components only (this milestone). Future ecosystems that need alternative-list handling can reuse the annotation name.
- **Parity-catalog placement**: TBD in the tasks phase. Two options:
  - (a) Extend existing C48 `mikebom:resolver-step` row's value vocabulary — questionable fit (resolver-step values are enum-like; alternative-alternates values are JSON arrays)
  - (b) Add new parity-catalog row C170 — cleaner, matches m167's approach of documenting a new C-row per new annotation shape

### Existing annotations reused (unchanged wire shape)

- `mikebom:archive-size-skipped` (m069) — reused for FR-012 ipk archive-size cap
- Any m152/153/154 SPDX-license annotations — the ipk_file License field routes through the same pipeline, so all downstream license annotations flow unchanged

## Non-goals

- No new CLI flag surface.
- No new SBOM wire shape.
- No parity-catalog rows for `.ipk` filename convention or ar-envelope parsing (these are implementation details, not wire contracts).
- No changes to CDX/SPDX validation gates (existing per-format validators cover the new components without adjustment).

## Consumer example — filter honest-signal ipk orphans by evidence source

```bash
jq '.components[] |
    select(.properties[]?.name == "mikebom:evidence-kind") |
    {purl: .purl,
     evidence_kind: (.properties[] |
                     select(.name == "mikebom:evidence-kind") |
                     .value)}' mikebom.cdx.json |
jq -s 'group_by(.evidence_kind) |
       map({kind: .[0].evidence_kind, count: length, examples: [.[0:3][].purl]})'
```

Post-169 output includes `ipk-file` and `opkg-status-db` groups on ipk-based scans.
