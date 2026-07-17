# Research: Cargo Workspace-Root [package] Runtime Classification

**Date**: 2026-07-16
**Purpose**: Resolve 4 mechanical unknowns before task decomposition.

## R1 — parse_cargo_toml edit site

**Investigation** (`grep -n` of `mikebom-cli/src/scan_fs/package_db/cargo.rs`):

- Line 689: `struct CargoTomlSections { prod_deps: HashSet<String>, dev_deps: HashSet<String>, build_deps: HashSet<String>, optional_deps: HashSet<String> }` — the accumulator.
- Line 721: `pub(crate) fn parse_cargo_toml(path: &Path) -> Option<CargoTomlSections>` — the parse site.
- Line 736: `collect_section_keys(&parsed, "dependencies", &mut out.prod_deps);` — inserts `[dependencies]` keys.
- Lines 737-738: same pattern for `[dev-dependencies]` + `[build-dependencies]`.
- Line 793: `fn collect_section_keys` — the helper (already correctly extracts table keys; no change needed here).

**Decision**: Add exactly one code block to `parse_cargo_toml` right after line 738 (the `build-dependencies` extract), reading the root-level `[package].name` key and inserting into `out.prod_deps`. Guarded on presence — a virtual workspace has no `[package]` block, so the insertion no-ops cleanly.

```rust
// Milestone 200 (FR-001): seed the prod-set BFS with the workspace-root
// [package].name so it classifies as Runtime rather than falling through
// to the Development branch at cargo.rs:1106. Closes #585.
if let Some(root_name) = parsed
    .get("package")
    .and_then(|v| v.as_table())
    .and_then(|t| t.get("name"))
    .and_then(|v| v.as_str())
{
    out.prod_deps.insert(root_name.to_string());
}
```

**Alternatives considered + rejected**:
- Post-classification override at `cargo.rs:1102` (pattern-match on `pkg.source.is_none()` for workspace-root detection): rejected — Cargo.lock's `source = None` also matches path-deps between workspace members, would over-tag. Seed-based approach is precise: only what Cargo.toml declares as `[package].name` gets the seed.
- Add a NEW field `workspace_root_names: HashSet<String>` and short-circuit in the classifier: rejected — extra plumbing for identical semantic; the additive seed IS the fix.
- Rename `.name` per `package.name.workspace = true` inheritance: NOT needed — root [package] can't inherit its own name from `[workspace.package]`; only optional metadata inherits. This is a Cargo grammar guarantee.

**References**:
- `mikebom-cli/src/scan_fs/package_db/cargo.rs:689-757` — `CargoTomlSections` + `parse_cargo_toml`.
- `mikebom-cli/src/scan_fs/package_db/cargo.rs:1098-1107` — the fallback classifier this seed fixes.

## R2 — Workspace-member discovery already exists

**Investigation**:
- Line 813-830 comment block: "Discover every `Cargo.toml` reachable from the lockfile's project root: the immediate sibling, plus any workspace members declared via `[workspace] members = [...]` (with simple glob expansion for `crate-*` / `crates/*` patterns)."
- Line 1178+ in `read()`: loop over `find_cargo_manifests` results, call `parse_cargo_toml(manifest_path)` for each, and `workspace_sections.union(&sections)` accumulates all workspace-member seeds.

**Decision**: No new workspace-discovery code needed. The existing loop already calls `parse_cargo_toml` for every workspace member's Cargo.toml. The fix inside `parse_cargo_toml` (R1) inherits this loop naturally — every member's `[package].name` gets seeded, not just the root's. This is CORRECT: sibling workspace members are also part of the Runtime deliverable set (they compile into the same target artifact via path deps).

**Alternatives considered + rejected**:
- Restrict the seed to ONLY the root Cargo.toml (skip member Cargo.tomls' `[package].name`): rejected. If a helper crate's own `[package].name` isn't seeded, and no one references it from `[dependencies]` (weird but possible in a virtual-workspace-of-two-independent-libs pattern), it would still fall through to Development. Extending the seed to every parseable member's `[package].name` covers this edge case at zero extra cost.

**References**:
- `mikebom-cli/src/scan_fs/package_db/cargo.rs:812-830` — discovery comment.
- `mikebom-cli/src/scan_fs/package_db/cargo.rs:1178-1183` — the `workspace_sections.union(...)` loop.

## R3 — Golden regen scope (post-m199 empirical-verification convention)

**Investigation** (grep of pre-existing cargo goldens for workspace-root [package] scope-excluded pattern):

```bash
# Check every cargo-related golden JSON for excluded scope on cargo entries
jq '[.components[] | select(.purl != null and (.purl | startswith("pkg:cargo/"))) | select(.scope == "excluded") | .name] | length' \
  mikebom-cli/tests/fixtures/golden/cyclonedx/cargo.cdx.json  # → 0
jq '[.components[] | select(.purl != null and (.purl | startswith("pkg:cargo/ripgrep"))) | .purl]' \
  mikebom-cli/tests/fixtures/public_corpus/rust-ripgrep/cdx.json  # → []
```

Findings:
- `cargo.cdx.json` (lockfile-v3 fixture): 0 cargo entries with `scope: "excluded"`. The fixture is a synthetic lockfile-only test with a `pkg:generic/lockfile-v3` synthetic root — no workspace-root [package] pattern → orthogonal to this fix.
- `rust-ripgrep` public-corpus golden: NO `pkg:cargo/ripgrep@<ver>` component exists at all. The metadata.component is `pkg:generic/rust-ripgrep@0e8390a` (m195 synthetic anchor). The ripgrep source repo DOES have `[package] name = "ripgrep"` at Cargo.toml root — either the current code skips workspace-root [[package]] emission entirely for corpus targets, or there's a name-mismatch pathway I haven't traced. **Needs implement-time investigation.**
- `produces_binaries/cargo/workspace/Cargo.toml`: virtual workspace (`[workspace]` only, no `[package]`) → orthogonal to fix, no regen.
- `optional_dep/cargo/Cargo.toml`: single crate, no `[workspace]` → the fix's [workspace] gate doesn't apply. Actually — a single-crate has just a [package] block, no [workspace]; my fix seeds `prod_deps` with `[package].name` regardless of workspace presence. Post-fix, this fixture's root [package] would ALSO get seeded (it wasn't before). Does that affect its golden? Probably not (single-crate root was likely already Runtime via other paths), but **needs implement-time verification.**

**Decision**: **Empirically likely: 0 golden regen files.** At implement time, re-run:
1. `cargo test --workspace --no-fail-fast` — any golden-based test that fails identifies regen scope.
2. Manual grep of every `mikebom-cli/tests/fixtures/**/*.json` before/after for the change to `scope` on cargo entries.
3. If rust-ripgrep golden drifts (new `pkg:cargo/ripgrep@<ver>` entry appears), regen via `public-corpus.yml` workflow_dispatch per m196 pattern.

Following the m199 lesson (feedback_verify_research_empirical_claims memory), this claim is TREATED AS UNVERIFIED until implement-time re-audit — not as a research-phase certainty. The plan explicitly commits to re-verification via cargo test failures.

**Alternatives considered + rejected**:
- Preemptively regen all cargo goldens: rejected — bulks the PR unnecessarily; if 0 goldens drift, we get a clean minimal PR.
- Split fix into two PRs (fix + separate regen): rejected — no expected drift means no split needed.

**References**:
- `mikebom-cli/tests/fixtures/public_corpus/rust-ripgrep/cdx.json` — target of post-implementation re-check.
- Memory `feedback_verify_research_empirical_claims`.

## R4 — Regression test fixture layout

**Investigation** (fixtures with the specific pattern the fix targets):

None found in the existing fixture set. The closest patterns:
- `produces_binaries/cargo/workspace/`: virtual workspace + member crates. Different pattern (no root [package]).
- `optional_dep/cargo/`: single-crate root [package]. Different pattern (no [workspace]).

**Decision**: Create a new fixture at `mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/` with:

```text
Cargo.toml           # [package] name = "app" + [workspace] members = ["helper"]
                     #   + [dependencies] helper = { path = "helper" }
Cargo.lock           # Resolved lockfile — includes app + helper as [[package]] entries
src/main.rs          # fn main() {}
helper/
  Cargo.toml         # [package] name = "helper", no deps
  src/lib.rs         # pub fn stub() {}
```

Integration test at `mikebom-cli/tests/cargo_workspace_root_lifecycle_m200.rs`:
- `scan_cargo_workspace_root_is_runtime_m200`: scan fixture → assert `pkg:cargo/app@<ver>` component has `scope: null` and NO `mikebom:lifecycle-scope: "development"` annotation.
- `scan_cargo_workspace_root_wins_root_election_m200`: scan fixture (no `--root-name` override) → assert `metadata.component.name == "app"` (not "helper").

**Test invocation pattern**: mirrors `scan_npm.rs` — shells out to `env!("CARGO_BIN_EXE_mikebom")` with `--offline --path <fixture> --format cyclonedx-json --output <tempfile> --no-deep-hash`, parses the CDX JSON via `serde_json::Value`.

**Alternatives considered + rejected**:
- Reuse `test-vaultwarden` as an integration test dep: rejected — external repo dep is a stability + isolation risk. In-tree fixture is the standard mikebom convention.
- Skip the fixture and rely on the vaultwarden manual SC-001 verification: rejected — SC-001 is a one-time-in-PR-body verification, not an ongoing regression guard. The fixture prevents re-regression.

**References**:
- `mikebom-cli/tests/scan_npm.rs::scan_path` — invocation pattern to mirror.
- FR-006 in spec.md.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Edit site | In-place in `parse_cargo_toml` at cargo.rs:721+ | Post-classification override at 1102 | Precise: only Cargo.toml-declared root [package] gets seeded, no path-dep over-tagging |
| Member coverage | Every workspace-member `[package].name` (not just root) | Only root Cargo.toml | Additive seed covers helper-lib edge cases at zero cost |
| Golden regen scope | Empirically 0 files (verified at implement time) | Preemptive full-corpus regen | Zero-drift-on-existing-goldens is the expected outcome per pattern analysis |
| Regression fixture | New in-tree at `tests/fixtures/cargo/root_package_lifecycle/` | Reuse external test-vaultwarden | Standard mikebom convention; stable + isolated |
| New Cargo deps | Zero | (n/a) | Nothing needed |
