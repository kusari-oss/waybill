# Feature Specification: Cargo workspace-member version-disambiguation fix

**Feature Branch**: `087-fix-cargo-workspace-version`
**Created**: 2026-05-08
**Status**: Draft
**Input**: GitHub issue #172 — "cargo reader: workspace-member version mismatch — clap@4.5.21 → clap_builder@4.5.9 instead of @4.5.21". Surfaced by milestone 083's transitive-parity audit on `clap-rs/clap @ v4.5.21`.

## Overview

When a `Cargo.lock` contains multiple `[[package]]` blocks for the same crate name at different versions (a common pattern in workspaces with both a runtime workspace member AND a transitive copy of an older release of the same crate), mikebom's dep-edge emission resolves the target crate to whichever version it processed last — not the version Cargo.lock's `dependencies = [...]` entry actually points at.

**Reproduction** (clap-rs/clap @ v4.5.21 fixture, post-milestone-083 audit):

```
mikebom emits:                    | should emit:
                                  |
pkg:cargo/clap@4.5.9              | pkg:cargo/clap@4.5.9
  → pkg:cargo/clap_builder@4.5.9  |   → pkg:cargo/clap_builder@4.5.9   (correct)
                                  |
pkg:cargo/clap@4.5.21             | pkg:cargo/clap@4.5.21
  → pkg:cargo/automod@1.0.14      |   → pkg:cargo/automod@1.0.14       (correct)
  → pkg:cargo/clap_builder@4.5.9  |   → pkg:cargo/clap_builder@4.5.21  ❌ wrong version
```

The wrong version-dep edge propagates to every consumer of mikebom's CDX 1.6 / SPDX 2.3 / SPDX 3 output for any cargo workspace where this multi-version pattern exists. Trivy and syft both correctly resolve to `@4.5.21` per the milestone-083 audit.

## Why this matters — root cause

`mikebom-cli/src/scan_fs/mod.rs:371-379` builds the `name_to_purl` lookup keyed by `(ecosystem, name)`:

```rust
let mut name_to_purl: HashMap<(String, String), String> = HashMap::new();
for e in &db_entries {
    let ecosystem = e.purl.ecosystem().to_string();
    name_to_purl.insert(
        (ecosystem.clone(), normalize_dep_name(e.purl.ecosystem(), &e.name)),
        e.purl.as_str().to_string(),
    );
    // milestone 085 added a maven dual-key insert here for groupId:artifactId
}
```

For maven (milestone 085) we already added a second key with `groupId:artifactId` for disambiguation. For cargo, no such fix — same crate name at different versions collides; last-write-wins.

The fix is per-edge-time resolution: when emitting a Relationship from a cargo crate, look at Cargo.lock's `[[package]] dependencies = [...]` entries. Cargo's lockfile format encodes version-when-ambiguous: `"clap_builder"` (one version exists) vs `"clap_builder 4.5.21"` (multiple versions exist, this dep targets specifically `4.5.21`). The cargo reader should propagate this version through to the edge target.

## Why this matters — semantic impact on consumers

| Consumer pattern | Impact |
|---|---|
| **Vulnerability scanners** querying `clap@4.5.21`'s transitive closure | See an outdated `clap_builder@4.5.9` in the closure that isn't actually present in the build, generating phantom CVE noise from older versions |
| **Reverse-impact analysis** ("who depends on clap_builder@4.5.21?") | Returns nothing — the edge is mis-routed to clap_builder@4.5.9 |
| **License-compatibility analysis** | Pulls licenses for the wrong version (rare problem since clap_builder has stable licensing, but the principle is unsafe) |
| **Pico ingestion / strict graph-walking consumers** | Graph topology is wrong; downstream visualizations show the wrong version of clap_builder under clap@4.5.21 |

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Cargo workspace dep edges resolve to the correct version (Priority: P1)

A regulatory-compliance pipeline consuming a mikebom-emitted CDX 1.6 / SPDX 2.3 / SPDX 3 document for a Cargo workspace where `Cargo.lock` has multiple `[[package]]` blocks for the same crate name (a common pattern when the workspace's main module depends on a newer version of crate X AND a transitive includes an older version of crate X) needs every dep edge to point at the version Cargo.lock actually pins for that specific dep relationship — not whichever copy of the crate name happened to be processed last.

**Why this priority**: Headline correctness bug. Surfaced by milestone 083's audit; trivy + syft both get this right; mikebom is the only one wrong. Fix shape is clear (per-edge version disambiguation from Cargo.lock's `dependencies = [...]` field).

**Independent Test**: Scan `mikebom-cli/tests/fixtures/transitive_parity/cargo/` (clap-rs/clap @ v4.5.21). Assert: `pkg:cargo/clap@4.5.21 → pkg:cargo/clap_builder@4.5.21` is in the emitted edge set. Currently mikebom emits `→ pkg:cargo/clap_builder@4.5.9`.

**Acceptance Scenarios**:

1. **Given** a cargo workspace whose `Cargo.lock` has `[[package]] name = "clap" version = "4.5.21" dependencies = ["clap_builder 4.5.21", ...]`, **When** mikebom emits dep edges, **Then** the edge from `clap@4.5.21` resolves to `clap_builder@4.5.21` (NOT `clap_builder@4.5.9`).
2. **Given** the same cargo workspace, **When** mikebom emits dep edges from a different `[[package]]` `clap version = "4.5.9"` whose `dependencies = ["clap_builder 4.5.9", ...]`, **Then** that edge resolves to `clap_builder@4.5.9`.
3. **Given** a cargo workspace where a dep declaration uses the unversioned form `"clap_builder"` (only one version exists in Cargo.lock for that name), **When** mikebom emits the edge, **Then** the edge resolves to that single version (current behavior preserved).

---

### User Story 2 — Audit baseline regenerates cleanly (Priority: P2)

Milestone 083's `transitive_parity_cargo.rs` regression test pinned the alpha.24 baseline at 319 mikebom edges with a representative-edge set documenting the gaps surfaced. Post-087 the baseline shifts: the version-mismatch edges resolve correctly, which may slightly change the edge count and definitely changes the representative-edge sample.

**Why this priority**: P2 because the regression test exists to catch silent drift; deliberate fix-bumps the baseline are the expected workflow per milestone-083 quickstart Recipe 3.

**Independent Test**: Run `cargo +stable test -p mikebom --test transitive_parity_cargo`; observe the test fails on the alpha.24 baseline (count drift); update `EXPECTED_MIKEBOM_EDGE_COUNT` + `EXPECTED_REPRESENTATIVE_EDGES` in the test file; re-run; observe pass.

**Acceptance Scenarios**:

1. **Given** the milestone-083 cargo regression test post-087, **When** it runs against the post-087 mikebom binary, **Then** the test fails with an edge-count drift assertion.
2. **Given** the test file is updated with the new baseline (per quickstart Recipe 3), **When** it runs, **Then** the test passes AND the audit-row in `specs/083-transitive-correctness/research.md §8 — Ecosystem: cargo` is updated to remove gap #1 from the `Specific gaps surfaced (mikebom-side)` list (gap #2 — clap_derive zero outgoing edges — remains; that's #173).

### Edge Cases

- **Cargo.lock without explicit version-disambiguation in `dependencies = [...]`**: When a crate name has only one version in Cargo.lock, Cargo writes the dep entry as `"crate_name"` (no version suffix). Current name-keyed `name_to_purl` works fine. Fix preserves this case: only override the lookup when the dep entry includes an explicit version.
- **Cargo.lock with same name + same version + different `source`**: e.g., a git override of a registry crate. Cargo.lock encodes this with the source URL in the dep entry: `"crate_name 1.0.0 (git+https://...)"`. Out of scope — covered by milestone-064's source-discrimination handling. The fix here only addresses the `name + version` ambiguity case.
- **Cargo workspace with renamed deps**: `[dependencies] my_alias = { package = "real_crate" }` — the `[[package]]` block uses the real crate name, but the `dependencies = [...]` array also uses the real name. No new ambiguity; covered by current logic + this fix.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When a cargo `[[package]]` block's `dependencies = [...]` entry includes an explicit version (e.g., `"clap_builder 4.5.21"`), the emitted dep edge MUST target that exact `(name, version)` PURL, not whichever same-name component happened to land in `name_to_purl` last.
- **FR-002**: When a `dependencies = [...]` entry omits the version (e.g., `"clap_builder"` because only one version exists in Cargo.lock), the emitted dep edge MUST target the unique same-name component (current behavior preserved).
- **FR-003**: The fix MUST work for every `[[package]]` block whose `dependencies = [...]` entries reference a same-name multi-version crate, not just the workspace-root entry. Transitive edges between dep components MUST also resolve correctly.
- **FR-004**: Existing milestone-064 cargo main-module emission MUST be unaffected. The bug is in edge resolution, not in component identity.
- **FR-005**: Existing milestone-085 maven `groupId:artifactId` lookup MUST be unaffected. The cargo fix is independent of maven's name-disambiguation.
- **FR-006**: Existing milestone-052 cargo dev-dep classification (`scope: excluded`) MUST be unaffected. The version-disambiguation logic runs orthogonally to scope classification.
- **FR-007**: Milestone-083's `transitive_parity_cargo.rs` baseline MUST be deliberately bumped per quickstart Recipe 3 to reflect the post-087 correct edge resolution. Update `EXPECTED_MIKEBOM_EDGE_COUNT` + the workspace-internal entries in `EXPECTED_REPRESENTATIVE_EDGES` to point at the now-correctly-resolved versions. Update `research.md §8 — Ecosystem: cargo` audit-row to mark gap #1 closed.
- **FR-008**: A new dedicated regression test MUST exercise the multi-version cargo fixture explicitly: scan `tests/fixtures/transitive_parity/cargo/` and assert the specific edge `pkg:cargo/clap@4.5.21 → pkg:cargo/clap_builder@4.5.21` is present (and `→ pkg:cargo/clap_builder@4.5.9` is absent for the `clap@4.5.21` source).
- **FR-009**: SPDX 2.3 + SPDX 3 emission MUST stay byte-identical for non-cargo fixtures. Cargo SPDX goldens regenerate via the standard `MIKEBOM_UPDATE_*_GOLDENS` env var workflow.
- **FR-010**: Pre-PR gate stays clean: clippy zero warnings; `cargo test --workspace` `0 failed`.

### Key Entities

- **`name_to_purl`** (existing internal at `scan_fs/mod.rs:371`): the lookup map keyed by `(ecosystem, name)`. Post-087, the cargo edge-emission path bypasses this when the dep declaration includes an explicit version, performing per-edge `(name, version)` resolution against the cargo PackageDbEntry set instead.
- **`CargoPackage.dependencies`** (existing internal in `scan_fs/package_db/cargo.rs`): the parsed `dependencies = [...]` list per `[[package]]` block. Post-087 the cargo reader preserves the version when present in each entry and uses it during edge emission.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Scanning `tests/fixtures/transitive_parity/cargo/` (clap-rs/clap @ v4.5.21) emits `pkg:cargo/clap@4.5.21 → pkg:cargo/clap_builder@4.5.21` (correct) and NOT `pkg:cargo/clap@4.5.21 → pkg:cargo/clap_builder@4.5.9`.
- **SC-002**: Cross-tool parity vs trivy on the cargo fixture improves: the `mikebom_only` set (currently 56 edges per `research.md §8 — Ecosystem: cargo`) shrinks; the `agreement` set (currently 41) grows. Exact post-087 numbers documented in the regenerated audit row.
- **SC-003**: Milestone-083 cargo regression test passes against the new baseline.
- **SC-004**: Pre-PR gate clean.
- **SC-005**: SPDX 2.3 + SPDX 3 cargo goldens regenerate with diffs containing only the version-string corrections (no other field changes).

## Assumptions

- The fix lives in `mikebom-cli/src/scan_fs/package_db/cargo.rs`'s lockfile-parsing + edge-emission logic. ~30-50 LOC change.
- No new Cargo dependencies needed.
- Cargo.lock format: per the [Cargo documentation](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html), `[[package]] dependencies = [...]` entries use the format `"name"` (single version) or `"name version"` (multiple versions). The `(source)` suffix appears only when source disambiguation is needed (out of scope per Edge Cases above).
- Cargo CDX 1.6 + SPDX 2.3 + SPDX 3 goldens for the cargo fixture exist at `mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/cargo.{cdx,spdx,spdx3}.json`. They regenerate via the standard `MIKEBOM_UPDATE_*_GOLDENS=1` env vars.
- Milestone 083 cargo regression test (`mikebom-cli/tests/transitive_parity_cargo.rs`) deliberately bumps; quickstart.md Recipe 3 documents this maintainer workflow.

## Out of scope

- **Sister cargo bug #173** (`clap_derive` proc-macro emits zero outgoing edges). Same audit-surfaced issue, different root cause (proc-macro classification skip rather than version-disambiguation). Could bundle into a single milestone with #172 if desired, but spec'd separately to keep scope tight.
- **Cargo Cargo.lock `(git+https://...)` source-disambiguation cases**: out of scope per Edge Cases. Different ambiguity dimension; covered by milestone 064 if needed.
- **Per-format closure-invariant test extension**: the milestone-084 closure-invariant test (`cdx_ref_closure_invariant.rs`) doesn't need extension here — the version-disambiguation fix doesn't change the closure set, only which edges resolve correctly.
- **Trivy's 56-edge-only set** (per research §8): the `trivy_only` was 0; after this fix, mikebom-only edges drop from 56 toward 0 (or close to it). Whether this fixes the entire `mikebom_only` divergence depends on what other sub-bugs the cargo reader has — out of scope to investigate here; would surface in the regenerated audit row.

## Dependencies

- Milestone 083 (transitive-parity audit) — surfaced this bug; the regression test pins the baseline this fix bumps.
- Milestone 064 (cargo main-module emission) — must continue to work; component identity is unaffected by this fix.
- Milestone 085 (maven SPDX dep edges + name_to_purl groupId:artifactId disambiguation) — sister fix shape (per-ecosystem version disambiguation); not modified.
