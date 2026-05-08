# Contract — milestone 087 cargo version-disambiguation

The milestone's only contract. Documents the cargo dep-encoding rule + the `name_to_purl` dual-key insert + the per-format scope.

## CLI surface

**No new operator-facing CLI flags.** This is an internal correctness fix. `mikebom sbom scan` keeps its existing flag set.

## Library surface (`mikebom-cli` crate)

**No new public Rust API.** Internal changes only:
- `mikebom-cli/src/scan_fs/package_db/cargo.rs::package_to_entry` — internal function; signature unchanged. Depends parser preserves the `name [version]` form, stripping only ` (source)` suffix.
- `mikebom-cli/src/scan_fs/package_db/cargo.rs::read` (workspace_root_deps builder, lines ~900-925) — same depends-parser update applied here so that milestone-064's main-module entry merging sees version-preserved deps.
- `mikebom-cli/src/scan_fs/package_db/cargo.rs::parse_lockfile` — the over-zealous `pkg.source.is_none()` skip is removed. Workspace ROOT + workspace MEMBERS + path deps are emitted as PackageDbEntry rows. The original conformance-bug-2 self-referential FP no longer exists because milestone-064's Phase A augment-in-place merge labels the workspace root with the C40 supplementary tag rather than dropping it. This expansion of scope was required to fix the underlying #172: workspace-member multi-version-same-name lookups can't resolve unless the workspace member is in the component set.
- `mikebom-cli/src/scan_fs/mod.rs` `name_to_purl` build loop — internal site; not exposed. Cargo entries now get a `(cargo, "name version")` dual-key alongside the existing `(cargo, name)` key.

## Cargo dep-string encoding rule

For each cargo `[[package]] dependencies = [...]` entry, the resulting `PackageDbEntry.depends` string MUST be:

```text
strip_source_suffix(d) =
    "name"                if d == "name"
    "name version"        if d == "name version"
    "name version"        if d == "name version (source)"  (strip " (source)")
```

In Rust:

```rust
let depends: Vec<String> = pkg.dependencies.iter()
    .map(|d| match d.find(" (") {
        Some(idx) => d[..idx].to_string(),
        None => d.clone(),
    })
    .collect();
```

This contract is enforced by VR-087-001 (data-model.md).

## `name_to_purl` dual-key contract

For every cargo entry inserted into `name_to_purl`, the lookup table MUST contain BOTH:

1. `(ecosystem="cargo", normalized_name=normalize_dep_name("cargo", entry.name))` — points at the entry's PURL string. (Existing behavior.)
2. `(ecosystem="cargo", normalized_name=normalize_dep_name("cargo", "name version"))` — points at the SAME PURL string. (New per milestone 087.)

Both keys live in the same map; `entry.depends` lookups hit whichever key matches the dep-string Cargo wrote. Single-version case → key 1. Multi-version case → key 2.

This contract is enforced by VR-087-002.

## Per-format scope contract

| Format | Affected? | Verification |
|---|---|---|
| **CDX 1.6 cargo** | YES — dep-edge `dependsOn[]` array values change for multi-version-same-name workspaces | `cargo.cdx.json` golden regenerates |
| **SPDX 2.3 cargo** | YES — `relationships[]` `DEPENDS_ON` `relatedSpdxElement` SPDXIDs resolve to different packages[] entries | `cargo.spdx.json` golden regenerates |
| **SPDX 3 cargo** | YES — `dependsOn` Relationship `to[]` IRIs resolve to different software_Package elements | `cargo.spdx3.json` golden regenerates |
| **Other ecosystems' goldens** | NO — only cargo's `package_to_entry` changes; only cargo's `name_to_purl` insert gains the dual key | All other goldens byte-identical |

VR-087-009 + VR-087-010 enforce these.

## Test invocation contract

```bash
# Confirm the cargo regression test fails on the alpha.25 baseline:
cargo +stable test -p mikebom --test transitive_parity_cargo
# Should fail with edge-count drift.

# Bump the baseline per quickstart.md Recipe 3, then re-run:
# (after editing EXPECTED_MIKEBOM_EDGE_COUNT + EXPECTED_REPRESENTATIVE_EDGES)
cargo +stable test -p mikebom --test transitive_parity_cargo
# Should pass.

# Regenerate cargo goldens:
MIKEBOM_UPDATE_CDX_GOLDENS=1 cargo +stable test -p mikebom --test cdx_regression
MIKEBOM_UPDATE_SPDX_GOLDENS=1 cargo +stable test -p mikebom --test spdx_regression
MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo +stable test -p mikebom --test spdx3_regression
# Verify ONLY cargo.* goldens regenerate (other 24 stay byte-identical).

# Closure invariant continues to hold:
cargo +stable test -p mikebom --test cdx_ref_closure_invariant
# Should pass without modification.

# Standard pre-PR gate:
./scripts/pre-pr.sh
# Should pass clean.
```

## Performance contract

- Emission wall-time: byte-identical to milestone-082 baseline (one extra HashMap insert per cargo entry; same shape as milestone-085's maven dual-key, which had no measured perf regression).
- Test wall-time: regression test runs in <5s (existing pattern).
- Goldens regen wall-time: <30s for the 3 cargo goldens.

## Backward-compatibility contract

- Operators of mikebom-emitted CDX/SPDX 2.3/SPDX 3 documents see correctly-versioned cargo dep edges post-fix. Pre-fix wrong-version edges (mikebom alpha.25 emits `clap@4.5.21 → clap_builder@4.5.9` against the clap-rs/clap fixture; post-fix emits `→ clap_builder@4.5.21`). The fix is observable, never silent — operators using cargo workspace SBOMs see strictly-correct edges.
- Operators of non-cargo SBOMs see no diff.
- Operators using strict CDX/SPDX consumers (e.g., the team's pico ingestion mentioned in milestone 084) see graph topology improvements — their reverse-impact analysis returns correct answers for cargo workspaces post-fix.
