---
description: "Implementation plan — milestone 029 cargo-auditable extraction"
status: plan
milestone: 029
---

# Plan: cargo-auditable extraction

## Architecture

Pure additive scanning extension. New module
`mikebom-cli/src/scan_fs/binary/cargo_auditable.rs` provides a single
public entry point: `parse_dep_v0(bytes: &[u8]) -> Option<CargoAuditableManifest>`.
The function walks: zlib-decompress (`flate2::read::ZlibDecoder`) →
JSON parse (`serde_json::from_slice` into a typed struct) → return
`Option<CargoAuditableManifest>`. Defensive on every failure.

`scan.rs::scan_binary` calls the new module when a `.dep-v0` section
is present. The result attaches to `BinaryScan::cargo_auditable:
Option<CargoAuditableManifest>`. Two emission paths consume it:

1. **File-level component**: `entry.rs::make_file_level_component`
   emits `mikebom:detected-cargo-auditable = true` (Value::Bool) into
   the bag when `scan.cargo_auditable.is_some()`. Symmetric to
   `mikebom:detected-go` from milestone 005.

2. **Per-crate components**: a new helper
   `entry.rs::cargo_auditable_packages_to_entries(scan, file_level_purl)
   -> Vec<PackageDbEntry>` converts each manifest entry into a
   `pkg:cargo/<name>@<version>` `PackageDbEntry` with
   `evidence-kind = "cargo-auditable"`, `confidence = "high"`,
   `parent_purl = file_level_purl`. Index-based `dependencies`
   resolve to PURL-keyed `depends: Vec<Purl>` edges.

`binary/mod.rs::scan_binary_walker` (or the per-binary entry-list
dispatch site) emits the per-crate entries alongside the existing
file-level + linkage-evidence + curated-version-string emissions.

No new types in `mikebom-common`. No public-API surface change. No
schema migration. No new crate dependencies (`flate2 = "1"` and
`serde / serde_json` are already in `mikebom-cli/Cargo.toml`).

## Reuse inventory

These existing items handle the work; this milestone consumes them:

- `flate2 = "1"` (mikebom-cli/Cargo.toml:48) — already pulled in for
  tar handling. Re-used here for `ZlibDecoder`.
- `serde / serde_json` — already pulled in across the workspace.
  Used for the `CargoAuditableManifest` / `CargoAuditablePackage`
  struct deserialization.
- `object::read::File::section_by_name_bytes` — proven for
  `.note.package` / `.note.gnu.build-id` / `.gnu_debuglink` /
  `.dynamic` / `.dynstr` reads; handles ELF / Mach-O / PE
  uniformly. cargo-auditable's section name `.dep-v0` slots in.
- `BinaryScan` — gains one new field (`cargo_auditable:
  Option<CargoAuditableManifest>`); 4 struct-literal sites updated
  (scan.rs non-fat ELF/PE arm, scan.rs fat-Mach-O arm, scan.rs
  non-fat Mach-O arm, entry.rs::tests::fake_binary_scan). Same
  surgery shape as 023 / 024 / 028.
- `extra_annotations` bag — gains `mikebom:detected-cargo-auditable`.
  5th consumer. Plumbing untouched.
- `PackageDbEntry` — gains nothing new; per-crate emissions reuse
  existing fields (purl / name / version / evidence_kind /
  confidence / parent_purl / depends / source_path).
- `cdx_anno!`, `spdx23_anno!`, `spdx3_anno!` macros — one-line
  registration per format for the new annotation key.
- 5-row C-section catalog pattern — gains row C36.

## Touched files

| File | Change | LOC |
|---|---|---|
| `mikebom-cli/src/scan_fs/binary/cargo_auditable.rs` | NEW — `parse_dep_v0` + types + 5 inline tests | +280 |
| `mikebom-cli/src/scan_fs/binary/mod.rs` | wire `cargo_auditable` module + per-crate-emission call site | +35 |
| `mikebom-cli/src/scan_fs/binary/scan.rs` | + 1 BinaryScan field + 1 call site (with .dep-v0 read) + 4 struct-literal updates | +25 |
| `mikebom-cli/src/scan_fs/binary/entry.rs` | + `cargo_auditable_packages_to_entries` helper + bag annotation insert | +85 |
| `mikebom-cli/src/parity/extractors/cdx.rs` | + 1 `cdx_anno!` for C36 | +1 |
| `mikebom-cli/src/parity/extractors/spdx2.rs` | + 1 `spdx23_anno!` for C36 | +1 |
| `mikebom-cli/src/parity/extractors/spdx3.rs` | + 1 `spdx3_anno!` for C36 | +1 |
| `mikebom-cli/src/parity/extractors/mod.rs` | + 1 EXTRACTORS row + 3 fn imports | +6 |
| `docs/reference/sbom-format-mapping.md` | + 1 C-section row for C36 | +1 |
| `mikebom-cli/tests/scan_binary.rs` | + 1 fixture-driven integration test | +90 |

Total Rust source: ~525 LOC across 8 files. Sits between 023's
~250-LOC scope and 024's ~700-LOC scope — closer to 028 in shape
since the parser is small (no byte-level header walks; just
zlib + serde_json).

## Phasing

Three atomic commits in dependency order:

### Commit 1: `029/parsers`
- Promote `cargo_auditable.rs` from non-existence to a working
  module: `parse_dep_v0` + `CargoAuditableManifest` /
  `CargoAuditablePackage` types + 5 inline tests.
- Module added to `mod.rs` declarations.
- `#[allow(dead_code)]` on the parser since it's not yet wired up.

### Commit 2: `029/wire-up-bag-and-entries`
- `BinaryScan::cargo_auditable: Option<CargoAuditableManifest>`
  field added.
- `scan.rs::scan_binary` reads `.dep-v0` section bytes when present
  and stores parsed result. Updates 4 BinaryScan struct-literal
  sites.
- `entry.rs::make_file_level_component` inserts
  `mikebom:detected-cargo-auditable = true` annotation when
  manifest present.
- `entry.rs::cargo_auditable_packages_to_entries` helper added.
- `binary/mod.rs` calls the new helper alongside existing
  file-level / linkage-evidence / version-string emissions.
- Lift `#[allow(dead_code)]` from `cargo_auditable.rs` parsers.
- Integration test in `tests/scan_binary.rs` against synthetic ELF
  fixture.
- 2 new tests in `entry.rs::tests` for bag annotation +
  per-crate-entries helper.

### Commit 3: `029/parity-row`
- 1 new C-section row (C36) in
  `docs/reference/sbom-format-mapping.md`.
- 3 new `*_anno!` macro invocations across cdx.rs / spdx2.rs /
  spdx3.rs.
- 1 new EXTRACTORS row + 3 fn imports in `parity/extractors/mod.rs`.

Per FR-012, each commit's `./scripts/pre-pr.sh` is clean. Commit 1's
`#[allow(dead_code)]` is the only intermediate-state wart (matches
023 / 024 / 028 pattern).

## Estimated effort

| Phase | Effort | Notes |
|---|---|---|
| Phase 1 (recon + baseline) | done | confirmed during scoping |
| Phase 2 (parsers) | 2 hr | small surface — zlib + serde_json |
| Phase 3 (wire-up + integration test) | 3 hr | 4 struct-literal sites + per-crate-emission helper + fixture-driven test |
| Phase 4 (parity row) | 30 min | mechanical |
| Phase 5 (verify + PR) | 1 hr | golden regen + CI watch |
| **Total** | **~6-7 hr** | sits between 026 (3 hr) and 023 (9 hr). |

## Risks

- **R1: cargo-auditable format version drift.** cargo-auditable's
  manifest format has evolved (early versions lacked the `kind`
  field; newer ones may add fields). Mitigation: deserialize with
  `#[serde(default)]` on every non-required field and `#[serde(
  flatten)] extra: serde_json::Value` if forward-compat is desired.
  Spec uses optional `Option<String>` + `#[serde(default)]` on
  `dependencies` / `root` to handle this.

- **R2: Mach-O segment-prefix on `.dep-v0`.** cargo-auditable
  documents Mach-O placement as `__DATA,.dep-v0`. object's
  `section_by_name_bytes(b".dep-v0")` ignores the segment prefix on
  Mach-O — verified for `__DATA,__cstring` reads in
  `scan.rs::collect_string_region`. Mitigation: cross-format
  fixture coverage (the FR-010 integration test exercises ELF; if
  Mach-O-specific friction surfaces, add `__DATA,` fallback in
  Commit 2 of this milestone).

- **R3: PURL qualifier shape for `source = "git"`.** cargo-auditable's
  git entries carry a URL but the field name varies. Mitigation:
  parse what's documented (the canonical `source: "git+<url>"`
  shape that cargo-auditable serializes); if a real-world fixture
  reveals other shapes, extend in a follow-on bug-fix.

- **R4: Massive manifests degrade SBOM-emit performance.** A 500+
  crate manifest produces 500+ components plus 500+ dep-graph
  edges. Mitigation: O(n) processing; no algorithmic concerns.
  Real wall-clock measurement deferred to a synthetic-stress test
  if user reports it.

## Constitution alignment

- **Principle I (Pure Rust, Zero C):** no new crates; `flate2` is
  already proven pure-Rust. ✓
- **Principle IV (no `.unwrap()` in production):** `parse_dep_v0`
  returns `Option<CargoAuditableManifest>` with `?` on every
  fallible step. No panics. ✓
- **Principle VI (Three-Crate Architecture):** untouched. ✓
- **Principle IX (Accuracy):** new evidence is at the `high`
  confidence tier (build-time truth, not heuristic). ✓
- **Per-commit verification (lessons from 016-028):** FR-012 enforced.
- **Recon-first discipline:** every claim in the spec backed by a
  file:line reference (`mikebom-cli/Cargo.toml:48` for flate2;
  `scan.rs:122-135` for the section-reader pattern;
  `binary/entry.rs:21` for the version_match_to_entry shape this
  milestone mirrors).
- **Bag amortization (lessons from 023/024/025/028):** SC-005
  verifies zero churn outside `binary/` + `parity/extractors/`.
  5th consecutive amortization-proof consumer.

## What this milestone does NOT do

- Does not add any new crate dependencies.
- Does not change CLI args or output flags.
- Does not implement cross-binary cargo-auditable deduplication.
- Does not validate manifest entries against `Cargo.lock` or a
  registry.
- Does not surface the cargo-auditable format-version field.
- Does not touch container-layer attribution (027's concern).
- Does not surface the `kind` filter — emit all kinds, let
  consumers filter.
