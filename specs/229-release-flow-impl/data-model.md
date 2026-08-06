# Data Model: release-flow entities

**Feature**: 229-release-flow-impl
**Phase**: 1

Entities here are the workflow-configuration + build-time state that this feature manipulates. Not runtime Rust structs (except for the version-override wrapper).

## Entity 1: Release tag

The identifier for every release; drives every workflow trigger + retention decision.

### Attributes

- **Tag name**: string matching one of the SemVer patterns enumerated in research §C's trigger list.
- **Channel classification** (derived from tag pattern):
  - `nightly` iff regex `^v[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}$` matches
  - `stable` iff regex `^v[0-9]+\.[0-9]+\.[0-9]+$` matches (no pre-release suffix)
  - `bridge` iff pre-release suffix is present AND not `-nightly.*` (e.g., `-alpha`, `-beta`, `-rc`, `-preview`, or maintainer-chosen)
- **Created-at timestamp**: ISO 8601 UTC (from GitHub release-page metadata).
- **Signed?**: always YES per Q2 clarification; recorded as a `.sig` file next to every release-artifact SBOM.

### Lifecycle

- **stable / bridge**: created by human `git push origin <tag>`. Persists forever (never auto-deleted per FR-011a).
- **nightly**: created by `nightly.yml` cron. Auto-deleted 30 days after creation (per Q1 + FR-011a). Tag deletion cascades to release-page-entry deletion (via `gh release delete --cleanup-tag`).

### Validation rules

- Tag name MUST match one of the 6 regex patterns in `release.yml`'s trigger list post-229 (per research §C).
- Nightly tag name's date component MUST be a valid `YYYYMMDD` per Rust's `chrono::NaiveDate::parse_from_str("%Y%m%d")` semantics.
- No two nightly tags on the same date (`YYYYMMDD`) — enforced by nightly.yml's tag-existence check.

## Entity 2: Nightly.yml workflow

### Configuration attributes

- **Cron schedule**: `0 6 * * *` (06:00 UTC daily). Chosen per 228 survey precedent; overrideable via workflow_dispatch for manual runs.
- **Permissions grant**: `contents: write` at workflow level. Enables tag creation via `GITHUB_TOKEN`.
- **Trigger events**: `schedule.cron` (primary) + `workflow_dispatch` (manual escape hatch, no inputs required).

### Step order (per research §A contract)

1. `actions/checkout@<sha>` with `fetch-depth: 0` and `ref: main`.
2. Compute skip condition: query last nightly tag via `git describe --match 'v*-nightly.*' --tags --abbrev=0` (or `git tag -l 'v*-nightly.*' | sort -V | tail -1`), compare pointed-at SHA with `HEAD` SHA. Skip-if-equal → exit success with FR-001(d)'s stable log substring.
3. Compute today's `YYYYMMDD`: `date -u +%Y%m%d`.
4. Compute nightly tag: `v0.2.0-nightly.<YYYYMMDD>`. Baseline version `0.2.0` is read from `Cargo.toml` `[workspace.package].version` (post-release-bump-PR); when running BEFORE the release-bump PR merges, the workflow falls back to a "no baseline yet" no-op.
5. `git tag <tag>` (annotated with the head SHA + workflow-run URL as tag message) + `git push origin <tag>`.
6. `gh workflow run release.yml -f tag=<tag>` — dispatches release.yml.
7. Retention cleanup (§B step contract).

### Validation rules

- Workflow file `.github/workflows/nightly.yml` must pass GitHub Actions workflow linter (per FR-011).
- The `gh` CLI shell-out must use `GITHUB_TOKEN` env for auth (auto-injected by GitHub Actions).

## Entity 3: `WAYBILL_VERSION` env-var override

### Attributes

- **Scope**: build-time only (read by `build.rs`); NOT read at runtime.
- **Type**: `String` (env-var value); expected format: valid SemVer.
- **Absence**: falls back to `env!("CARGO_PKG_VERSION")` (existing behavior).
- **Emission surface**: `cargo:rustc-env=WAYBILL_VERSION_OVERRIDE=<value>` printed by `build.rs`.

### Lifecycle

- Set by nightly.yml's release-dispatch step (workflow_dispatch input propagates the tag name → nightly.yml derives WAYBILL_VERSION from the tag).
- Set by developers locally for testing.
- **NOT set** by release.yml's stable-tag-triggered path — stable builds use `Cargo.toml`'s version directly.

### Runtime consumption pattern

```rust
// In waybill-common (or wherever version is currently read):
pub fn version() -> &'static str {
    option_env!("WAYBILL_VERSION_OVERRIDE").unwrap_or(env!("CARGO_PKG_VERSION"))
}
```

Every current `env!("CARGO_PKG_VERSION")` call site migrates to `waybill_common::version()`.

### Validation rules per FR-012

- (a) When set to a valid SemVer, override applies at runtime version reporting.
- (b) When unset, `env!("CARGO_PKG_VERSION")` is used.
- (c) Invalid values produce a build-time compile error via `build.rs` panic.

## Entity 4: `release.yml` modifications

### Modification attributes

- **Tag-trigger regex** (research §C): expand from 3 patterns to 6.
- **Signing invocation**: NEW step after artifact-build, invokes `waybill sbom scan --sign` per FR-003/FR-004. Unconditional (no branch on tag format).
- **OIDC permissions**: `id-token: write` at workflow level (already present per m222; verify at implementation time).
- **Removed**: any per-tag-format conditional logic. Q2 clarification removes the "if nightly, skip signing" branch that a naive reading of FR-003 might have included.

### Validation rules

- Post-modification workflow file MUST pass workflow linter.
- `--sign` step is fail-closed: if it fails, the entire workflow-run fails.
- No unsigned release-artifact SBOMs are produced under any code path.

## Entity 5: `RELEASING.md` document

Companion release-cutting guide (new file per FR-008).

### Required sections (per contracts/releasing-md-structure.md)

- **1. Two-channel model overview** — stable + nightly + bridge governance summary.
- **2. Cutting a stable release** — 5-step procedure: bump `Cargo.toml`, regenerate 6 goldens, verify normalized diff, tag manually, push tag.
- **3. Nightly channel operational notes** — how nightly.yml works, how to disable a given day's cron (temp commit, workflow_dispatch=cancel), how to reproduce a nightly locally.
- **4. Cutting a bridge pre-release** — Q3 always-acceptable governance, tag-format guidance (must avoid nightly regex collision, must have valid SemVer pre-release suffix), signing invariant (always signed per Q2).
- **5. Retirement of the `alpha.N` sequence** — one-time transition note; `alpha.70` is the last, `v0.2.0` is the first stable under the new model.
- **6. Retirement of `auto-tag-release.yml`** — one-line note that the workflow was deleted in feature 229; manual `git push origin <tag>` is the canonical trigger.

### Validation rules

- Every section MUST have a runnable step where applicable (grep for `git tag` / `gh workflow run` / `WAYBILL_UPDATE_*` env vars).
- Line budget: ~150 lines (contracts/releasing-md-structure.md).

## Entity 6: Retention cleanup criteria

### Filter attributes

- **Include-regex** (must ALL be true for deletion candidacy):
  - `isPrerelease` = true
  - `tagName` matches `^v[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}$` (anchored — no substring matches, no accidental deletions of tags containing the substring)
  - `createdAt` < now − 30 days
- **Exclude conditions**: any of the following disqualifies a tag from deletion:
  - Not a prerelease (stables)
  - Tag matches a non-nightly pre-release suffix (bridge tags)
  - Created within the last 30 days

### Failure handling

- Cleanup step runs with `continue-on-error: true` — failures logged, don't fail workflow-run.
- Per-tag deletion failures logged individually; workflow proceeds to next tag.

## Entity relationships

```text
Entity 1 (Release tag)
    ↓ triggers        ↓ classified by
Entity 4 (release.yml)  Entity 6 (Retention cleanup)
    ↓ dispatched by         ↑ scoped to nightly only
Entity 2 (nightly.yml)
    ↓ uses build-time
Entity 3 (WAYBILL_VERSION)
    ↓ documented in
Entity 5 (RELEASING.md)
```
