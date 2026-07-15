# Research: m190 + m191 Follow-Up Bundle

**Date**: 2026-07-15
**Purpose**: Resolve 6 investigation-level unknowns before task decomposition — chiefly locating existing code paths in the readers being audited / extended, and confirming the choice of fuzz-generator shape.

## R1 — Epoch-emission audit findings (dpkg / apk / rpm)

**Investigation via grep**:
- `mikebom-cli/src/scan_fs/package_db/dpkg.rs` — NO `epoch` handling in the file. The dpkg reader parses `Version:` field and emits it verbatim into the PURL version segment. **US1: BROKEN — needs fix.**
- `mikebom-cli/src/scan_fs/package_db/apk.rs` — NO `epoch` handling. Same pattern as dpkg. **US2: BROKEN — needs fix.**
- `mikebom-cli/src/scan_fs/package_db/rpm.rs` — HAS full epoch handling (`epoch_val: Option<i64>` field extracted from rpm header at line 415, passed to `build_purl(..., epoch)` at line 454; qualifier form `?epoch=<N>` documented in PURL-construction comment at line 548). **US2b: LIKELY ALREADY CORRECT — audit closes with a non-regression fixture, not a code fix.**

**Reference pattern for the fix** (opkg-side, `ipk_file.rs::parse_opkg_version_with_epoch` + `build_opkg_purl`):

```rust
// Pattern to mirror for dpkg + apk:
fn parse_opkg_version_with_epoch(raw: &str) -> (Option<i64>, String) {
    // Version encoded as `<digits>:<upstream>-<release>` when epoch present.
    if let Some((epoch_str, naked)) = raw.split_once(':') {
        if let Ok(epoch) = epoch_str.parse::<i64>() {
            return (Some(epoch), naked.to_string());
        }
    }
    (None, raw.to_string())
}

fn build_opkg_purl(name, version, arch, distro_tag, epoch: Option<i64>) -> Result<Purl> {
    let mut qualifiers = vec![...];
    if let Some(e) = epoch {
        qualifiers.push(("epoch", e.to_string()));
    }
    // ... construct PURL with qualifiers ...
}
```

**Decision**: Mirror the pattern for dpkg + apk. Add parallel helpers `parse_deb_version_with_epoch` (dpkg.rs) and `parse_apk_version_with_epoch` (apk.rs). Deb + apk version-grammar is nearly identical to opkg (Debian ancestry) — split on first `:`, parse the prefix as `i64`, use the remainder as the naked version. For rpm, no code change; just add a fixture proving the existing code path handles a synthetic epoch-versioned `.rpm`.

**Alternatives considered**:
- Extract a shared `parse_debian_style_version_with_epoch()` helper into `mikebom-common` — rejected as premature abstraction for a 5-line function per reader; if a fourth reader appears, factor then.

## R2 — Versionless-PURL extension: 6 additional ecosystems

**Investigation via `ls`**: All 6 target readers exist as standalone files:
`composer.rs`, `dart.rs`, `cocoapods.rs`, `scala.rs`, `haskell.rs`, `erlang.rs`.

**Investigation via grep on the m191-fixed 5** (npm, cargo, maven, gem, pip):
Each has a `build_<eco>_purl(name, version)` helper that in m191 was extended to short-circuit when `version.is_empty()`:

```rust
// Post-m191 pattern (npm example):
fn build_npm_purl(name: &str, version: &str) -> Option<Purl> {
    let purl_str = if version.is_empty() {
        format!("pkg:npm/{}", encode_purl_segment(name))  // versionless canonical
    } else {
        format!("pkg:npm/{}@{}", encode_purl_segment(name), encode_purl_segment(version))
    };
    Purl::new(&purl_str).ok()
}
```

**Decision**: Locate the equivalent `build_*_purl` (or PURL-construction path) in each of the 6 additional reader files. Apply the same version-empty short-circuit. Zero grammar customization needed — the 6 ecosystems' PURL types all follow purl-spec canonical: `pkg:<type>/<name>` for versionless, `pkg:<type>/<name>@<version>` when versioned.

**Ecosystem-specific quirks** to watch for during implementation:
- **cocoapods**: PURL type uses `pkg:cocoapods/<name>@<version>`; no namespace usually. Straightforward.
- **haskell**: PURL type `pkg:hackage/<name>@<version>`. Straightforward.
- **scala**: PURL type `pkg:maven/<groupId>/<artifactId>@<version>` (scala publishes via Maven Central). If sbt reader uses maven-PURL construction, the m191 maven fix already covers it — audit whether scala.rs has its OWN build helper or delegates to maven.rs.
- **erlang**: PURL type `pkg:hex/<name>@<version>`. Straightforward.
- **dart**: PURL type `pkg:pub/<name>@<version>`. Straightforward.
- **composer**: PURL type `pkg:composer/<vendor>/<package>@<version>`. Namespace segment present.

**Alternatives considered**:
- Move all `build_*_purl` helpers into a single `mikebom_common::purl_builders` module — rejected as scope creep. Each reader retains its own helper; DRY-ness can wait for a future refactor.

## R3 — Fuzz-test framework choice

**Decision**: Hand-rolled catalog-driven generator. No new Cargo dependency.

**Rationale**: Spec Assumption 3 already commits to this. `proptest` and `quickcheck` are the Rust ecosystem's standard property-based-test crates but both add a workspace-level dep for a 1100-input test that's fundamentally deterministic (fixed-catalog of name shapes × ecosystems). A hand-rolled loop over `const NAME_SHAPES: &[&str] = &[...]` × 11 ecosystems × N = 100 = 1100 invocations is 50 lines of code, zero new deps, zero maintenance surface.

**Catalog contents** (roughly):
- Empty-string name (edge)
- Single-char name
- Max-length name per ecosystem (npm 214, cargo 200, maven 200, etc.)
- Unicode names where the ecosystem permits (rare — most reject)
- Scoped names (npm `@scope/name`, maven `com.example:artifact`, composer `vendor/package`)
- URL-encoded segments (`foo/bar` needing `%2F`)
- Names with hyphens, underscores, dots
- Names beginning with digits (some ecosystems disallow)
- Names at the boundary of ecosystem-specific validation regexes

For each of the 11 ecosystems × ~10 shape variants × repeat = 100+ inputs each.

**Alternatives considered**:
- `proptest = "1"` for real property-based testing — rejected: adds a dep for value m197 doesn't need. Sufficient corner-case coverage via curated catalog.
- `libfuzzer-sys` / `cargo-fuzz` for coverage-guided fuzzing — rejected: overkill for a purl-serialization round-trip; requires nightly toolchain per m020 feature-flag posture.

## R4 — m191 reconciler code path

**Investigation via grep**: `mikebom-cli/src/resolve/reconciler.rs` is the single-file implementation. Key spots:
- Line 85-105: transfers `mikebom:requirement-range` (line 89-94) and `mikebom:source-manifest` (line 102-106) from design-tier to source-tier survivor via `contains_key` guard + string insert.
- Line 54-56: match-key is `(ecosystem, canonical_name, source-manifest-directory)` per m191 spec.

**Decision (US6 always-array shape)**: Modify the transfer logic at line 85+ to:
1. On first design-tier match, initialize `mikebom:requirement-ranges` as a JSON array containing the design-tier's range, and `mikebom:source-manifests` as a JSON array with the design-tier's manifest path.
2. On subsequent matches (same `(ecosystem, canonical_name, ?)` — the m191 uniqueness key may need loosening for the multi-declaration case per US6), append to the arrays.
3. Remove any code path that would write the singular `mikebom:requirement-range` / `mikebom:source-manifest` scalar keys.

**Decision (US5 npm-alias resolved-identity matching)**: Extend the match-key with an alias-aware lookup:
- When a design-tier component's `mikebom:declared-as` field is set (populated by the npm reader per R5), the reconciler matches against source-tier by the RESOLVED name (from the alias's `npm:<actual>@<ver>` value), not the alias name.
- The survivor gets `mikebom:declared-as` populated as an array of alias names encountered across all reconciled design-tier hits.

**Alternatives considered**:
- Reconciler unchanged; do the always-array rewrite in a post-pass — rejected as extra pipeline stage for negligible benefit. The reconciler already touches these annotations; doing it in-place is atomically simpler.

## R5 — npm-alias parsing extraction

**Investigation via grep**: `mikebom-cli/src/scan_fs/package_db/npm/alias_mapping.rs` already exists (m159 pattern), with `detect_pnpm_alias()` handling the pnpm-lock.yaml side. `package_lock.rs` and yarn readers likely have similar detection points.

**Decision**: Extend `alias_mapping.rs` to also handle `package.json`-declared aliases (`"name": "npm:actual@ver"` form). The parser is stateless — regex or `str::split_once("npm:")`. Return an `AliasResolution { alias_name, resolved_name, resolved_version }`.

The npm reader's design-tier emission path stamps `mikebom:declared-as: <alias_name>` on the component when an `AliasResolution` was produced. The reconciler then consumes this annotation per R4 to match against the resolved-identity source-tier.

**Alternatives considered**:
- Handle aliases entirely in the reconciler by treating design-tier names as fuzzy-matchable — rejected. The npm-side has authoritative context ("this was declared as an alias"). Pushing that context into the reconciler as a `mikebom:declared-as` annotation is cleaner than fuzzy-matching.

## R6 — Golden regen scope

**Decision**: Regen the subset of goldens that exercise the m191 reconciler survivor code path.

**Discovery method**: Grep the existing golden fixtures for the two m191 singular scalar strings `"mikebom:requirement-range"` and `"mikebom:source-manifest"`. Every hit is a golden that needs regen (per Q1 clarification: those strings no longer appear on m197 output). The regen operation is `MIKEBOM_UPDATE_CDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX_GOLDENS=1 MIKEBOM_UPDATE_SPDX3_GOLDENS=1 cargo test`.

Post-regen, diff-review each affected golden. Every diff MUST be exclusively the singular-→-array shape rotation:
- `mikebom:requirement-range` (string) → `mikebom:requirement-ranges` (single-element array)
- `mikebom:source-manifest` (string) → `mikebom:source-manifests` (single-element array)

Any diff class beyond that is a red flag (unintended reader regression).

**Alternatives considered**:
- Emit both singular AND array on the survivor for backwards compat — rejected per Q1 Option C (spec explicitly picked Option B always-array).
- Version-gate the annotation shape (m191-clients see singular, m197-clients see array) — rejected: adds shape-detection logic to consumers for no real benefit; consumers should upgrade.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Epoch fix pattern | Mirror opkg helpers (per-reader helper + Option<i64> epoch) | Shared helper in mikebom-common | Premature abstraction for 5-line function |
| Versionless-PURL fix | In-place edit to each ecosystem's build_*_purl helper | Central purl-builder module refactor | Preserves reader-file locality; DRY refactor deferred |
| Fuzz-test framework | Hand-rolled catalog-driven | proptest / quickcheck | No new Cargo dep; 1100 inputs is deterministic-space |
| Reconciler shape | Always-array in-place at existing transfer site | Post-pass rewrite | Atomic + reviewer-simple |
| npm-alias source | `alias_mapping.rs` extension + `mikebom:declared-as` annotation on design-tier | Reconciler fuzzy-matching | Authoritative context in reader, not classifier |
| Golden regen scope | Grep existing goldens for m191 singular scalars; regen every hit | Regen all goldens | Bounded change surface; FR-007 additive-only for non-reconciler paths |
| New Cargo deps | Zero | Add proptest | Not needed for catalog fuzzing |
| New `mikebom:*` annotations | 1: `mikebom:declared-as` | Native CDX/SPDX construct | Audited per Principle V — no native alternative |
