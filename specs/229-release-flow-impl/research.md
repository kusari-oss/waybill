# Research: nightly.yml architecture + build.rs env-override + release.yml modifications

**Feature**: 229-release-flow-impl
**Phase**: 0 (research)
**Date**: 2026-08-06

Resolves the three architectural-detail tradeoffs surfaced in plan.md's Complexity Tracking, plus verifies each memory reference against current repo state.

## §A — nightly.yml → release.yml handoff mechanism

### Decision: `workflow_dispatch` API via `gh workflow run`

Matches the existing `auto-tag-release.yml` pattern (verified via `head .github/workflows/auto-tag-release.yml`). Nightly.yml pushes a tag via `GITHUB_TOKEN` + `contents: write` permission, then explicitly invokes `gh workflow run release.yml -f tag=v0.2.0-nightly.YYYYMMDD` to trigger the multi-arch build.

### Rationale

GitHub's anti-loop policy prevents a `GITHUB_TOKEN`-pushed tag from triggering downstream workflows via the `push:` trigger. Documented in the current `auto-tag-release.yml` header comment: *"A tag push made via GITHUB_TOKEN does NOT trigger downstream workflows by design — GitHub's anti-loop policy disables event triggers for refs created by the actions token."* Nightly.yml faces the same constraint; the same workaround applies.

The existing `release.yml` already exposes a `workflow_dispatch` entry-point with a `tag` input parameter (verified at `release.yml:15-24`). Nightly.yml reuses this entry-point verbatim.

### Alternatives considered

- **`workflow_call` (reusable workflow)** — requires refactoring release.yml to expose a callable interface with all its build inputs. Bigger surface change; rejected as premature since the existing `workflow_dispatch` entry-point already works.
- **Personal Access Token (PAT) instead of `GITHUB_TOKEN`** — would bypass the anti-loop restriction but requires managing a long-lived secret. The `RELEASE_TAG_TOKEN` failure history (memory `reference_release_process`) argues against reintroducing a PAT-shaped dependency. Rejected.
- **GitHub App with elevated permissions** — cleanest anti-loop bypass but requires provisioning a GitHub App for the repo; overkill for a nightly-cadence use case. Rejected.

### Contract

- Nightly.yml uses `permissions: contents: write` on the workflow-level permissions grant.
- Nightly.yml step order: (1) checkout main; (2) skip-check via `git rev-parse HEAD` vs last nightly tag; (3) compute today's `YYYYMMDD`; (4) `git tag && git push origin <tag>`; (5) `gh workflow run release.yml -f tag=<tag>`; (6) retention-cleanup step (see §B).
- release.yml is invoked once per nightly tag creation; the tag itself is not re-triggered by release.yml's `push:` filter (anti-loop policy).

## §B — Retention cleanup implementation

### Decision: inline shell step using `gh` CLI

Nightly.yml final step invokes:

```bash
# Pseudocode; full YAML step in contracts/nightly-workflow.md
gh release list --limit 200 --exclude-drafts \
  --json name,tagName,createdAt,isPrerelease \
  | jq -r --arg cutoff "$(date -u -d '30 days ago' +%Y-%m-%dT%H:%M:%SZ)" \
      '.[] | select(
        .isPrerelease
        and (.tagName | test("^v[0-9]+\\.[0-9]+\\.[0-9]+-nightly\\.[0-9]{8}$"))
        and (.createdAt < $cutoff)
      ) | .tagName' \
  | xargs -I {} gh release delete "{}" --yes --cleanup-tag
```

The `jq` filter enforces the FR-011a inclusion invariant: **only** tags matching the anchored nightly regex are candidates for deletion. Bridge pre-releases (any tag with a non-nightly pre-release suffix) and stables (no suffix) are excluded by the regex; the anchor prevents accidental matches.

### Rationale

- `gh` CLI is preinstalled on every GitHub Actions runner; no marketplace-action dependency.
- Shell + `jq` is more auditable than a JavaScript step in `actions/github-script`; the filter logic is inspectable at a glance and doesn't require running a Node runtime.
- Failure mode: if any single deletion fails (rare — permissions, race with a concurrent workflow), the step logs the failure but does NOT fail the workflow-run (per FR-011a's "tag push succeeded is the primary success signal").

### Alternatives considered

- **`actions/github-script` with typed REST API access** — cleaner types + better error handling but adds a Node-runtime dependency + marketplace action. Rejected for the same "no new deps" argument.
- **Cron-triggered separate cleanup workflow** — decouples cleanup from tag creation, so cleanup runs even on no-op nightly days. Rejected because 30-day retention is coarse enough that running on tag-creation days is sufficient (cleanup lag = at most ~1 day between the last change and cleanup, which is fine for a 30-day retention window).
- **API-side release filtering** — GitHub API doesn't support server-side filtering by createdAt < X; client-side `jq` is the only option today. Not an alternative, just a constraint.

### Contract

- Step name: `retention-cleanup`
- Runs on `if: success()` (only after tag push succeeded)
- Continues on error via `continue-on-error: true` (cleanup failures don't fail the run)
- 30-day cutoff computed at step-execution time via `date -u -d '30 days ago' +%Y-%m-%dT%H:%M:%SZ`
- Regex anchor: `^v[0-9]+\.[0-9]+\.[0-9]+-nightly\.[0-9]{8}$` (exact match on the nightly tag format)

## §C — release.yml tag-trigger regex broadening

### Decision: enumerate all valid release tag formats, don't use catch-all

Current trigger (verified at `release.yml:3-7`):

```yaml
push:
  tags:
    - 'v*-alpha.*'
    - 'v*-beta.*'
    - 'v*-rc.*'
```

Post-229 trigger:

```yaml
push:
  tags:
    - 'v*-alpha.*'       # bridge alphas (Q3 always-acceptable)
    - 'v*-beta.*'        # existing; not currently used but reserve for future beta channel
    - 'v*-rc.*'          # existing; bridge RCs (Q3 always-acceptable)
    - 'v*-nightly.*'     # NEW — nightly channel (US2)
    - 'v*-preview.*'     # NEW — Q3 bridge with -preview suffix
    - 'v[0-9]+.[0-9]+.[0-9]+' # NEW — bare-SemVer stables (v0.2.0, v0.2.1, ...)
```

### Rationale

- **Defensive anchoring**: a `v*` catch-all would fire on non-release tags a maintainer might use for internal purposes (e.g., `v-audit-baseline`, `v-benchmark-2026-08`). Explicit enumeration prevents accidental release triggers.
- **Future-compat with Q3 bridge governance**: Q3 allows any SemVer-valid pre-release suffix. The enumeration must cover the common suffixes (`-alpha`, `-beta`, `-rc`, `-preview`, `-nightly`). If a maintainer picks an exotic suffix (e.g., `-xyz.20260814`), they'd need to add it to the trigger — noted as a Q3-follow-up in RELEASING.md.
- **Bare-SemVer for stables**: pattern `v[0-9]+.[0-9]+.[0-9]+` matches `v0.2.0`, `v1.5.3`, etc. GitHub Actions supports basic glob-style tag patterns (`*` = any chars, `[0-9]` = char class); this specific pattern is confirmed to work per GitHub docs.

### Alternatives considered

- **Catch-all `v*`** — simpler + auto-covers exotic suffixes. Rejected for the false-trigger risk on internal tags.
- **Regex on-workflow (filter within release.yml, trigger on `v*`, no-op mismatched tags)** — moves the filter inside the workflow instead of at trigger time. More expensive (workflow spins up before deciding no-op); less traceable. Rejected.

### Contract

- `release.yml`'s trigger list post-229 includes all 6 patterns above.
- `RELEASING.md` documents which pre-release suffixes are supported by the trigger; adding a new one requires editing both `release.yml` and `RELEASING.md`.

## §D — `WAYBILL_VERSION` env-override in `build.rs`

### Decision: rerun-if-env-changed + fallback via `cargo:rustc-env` emission

Existing `waybill-cli/build.rs` (verified via file read) already uses `cargo:rerun-if-changed` for the fixture + fingerprint pinning steps. Adding version override follows the same pattern:

```rust
// Pseudocode; full contract in contracts/build-rs-version-override.md
fn main() {
    // ... existing build.rs body ...

    println!("cargo:rerun-if-env-changed=WAYBILL_VERSION");
    if let Ok(v) = std::env::var("WAYBILL_VERSION") {
        // Emit a version override that overrides env!("CARGO_PKG_VERSION")
        // consumption in the runtime code.
        // Validate SemVer at build time; compile-error on invalid input.
        validate_semver(&v).unwrap_or_else(|e| {
            panic!("WAYBILL_VERSION={v} is not a valid SemVer: {e}");
        });
        println!("cargo:rustc-env=WAYBILL_VERSION_OVERRIDE={v}");
    }
    // ... rest of build.rs ...
}
```

Runtime code that today reads `env!("CARGO_PKG_VERSION")` gets a new wrapper: `waybill_common::version()` returns `option_env!("WAYBILL_VERSION_OVERRIDE").unwrap_or(env!("CARGO_PKG_VERSION"))`.

### Rationale

- `cargo:rerun-if-env-changed` tells Cargo to re-run `build.rs` only when the specific env var changes — preserves compile cache for unrelated changes.
- `cargo:rustc-env=` emission makes the override string available at compile time as an env-var visible to `option_env!()`, without touching `Cargo.toml`.
- Validation at build time (per FR-012c) — invalid SemVer produces a compile error via `panic!` in build.rs, which cargo surfaces as a build failure.
- No `.unwrap()` in production code path (Principle IV / SB-4) — `unwrap_or_else` with `panic!` is acceptable IN `build.rs` (which is a build-time script, not production code), but the runtime `waybill_common::version()` wrapper uses `option_env!()` + `unwrap_or()` (both compile-time-safe, no runtime panic).

### Alternatives considered

- **Read WAYBILL_VERSION at runtime instead of build time** — would allow overriding after build without recompile. Rejected because the runtime version needs to be baked into the emitted SBOM's Tool.version field at build time for reproducibility (memory `feedback_release_bump_regen_all_golden_tests` — every version-string change invalidates goldens).
- **`env!("WAYBILL_VERSION")` with fallback** — `env!` panics at compile time if the env var isn't set. `option_env!()` returns `Option<&str>` so we can fall back gracefully. Rejected the `env!` option in favor of `option_env!`.
- **Modify `Cargo.toml` via build.rs** — not supported by cargo; build.rs is read-only w.r.t. workspace metadata. Not an alternative.

### Contract per FR-012

- (a) Override applies when set: `WAYBILL_VERSION=1.2.3-test cargo build && ./target/debug/waybill --version` reports `1.2.3-test`.
- (b) Fallback when unset: `cargo build && ./target/debug/waybill --version` reports the `Cargo.toml`-declared version.
- (c) Invalid values (non-SemVer, empty, whitespace-only) produce a build-time compile error — `cargo build` fails with an informative panic from `build.rs`.

## §E — memory-reference verification

Every memory referenced in the spec was verified against current repo state:

- **`reference_release_process`** ("push tag manually") — verified: `auto-tag-release.yml` still fails today (nightly.yml + retirement of auto-tag-release.yml addresses this). ✅
- **`feedback_release_bump_prepr_slow`** ("30+ min PR") — verified: golden-fixture cache invalidation is real; WAYBILL_VERSION override in this feature is the mitigation for nightly-cadence releases. Stable release-bump PR remains 30+ min (accepted). ✅
- **`feedback_release_bump_regen_all_golden_tests`** ("6 golden files") — verified: `grep -rln "WAYBILL_UPDATE_" waybill-cli/tests/` returns the 6 named files. Release-bump PR regenerates all 6 (per PR #664 precedent). ✅
- **`feedback_verify_golden_churn_normalized`** — verified: same normalized-diff protocol used in PR #664 applies to the `v0.2.0` bump. ✅
- **`feedback_release_pr_title_format`** ("`release: bump workspace to v`") — verified against PR #664's title; the same prefix applies to the release-bump PR for `v0.2.0`. ✅

## §F — Follow-up-issue check (from 228 §7 open questions)

The 228 survey filed 4 follow-up issues (#665/#666/#667/#668). This spec (229) is #665's implementation. The other three:

- **#666** (Sigstore OIDC identity for cron-triggered workflows) — SC-011 makes verification part of this feature's acceptance. If OIDC doesn't work for cron-triggered nightlies, this feature falls back to unsigning nightlies (contradicting Q2) OR to a per-cron IdP setup (separate spec). Phase-0 finding: `sigstore/cosign-installer` and OIDC token audience work identically for cron-triggered as for tag-triggered workflows (verified via GHA docs — the OIDC token audience is per-repo, not per-trigger). Expected outcome: SC-011 closes #666 as verified-working.
- **#667** (per-channel reproducibility contract docs) — served indirectly by SC-007's reproducibility test + RELEASING.md documentation. Not the primary deliverable but touched.
- **#668** (Homebrew SemVer pre-release compatibility) — out of scope for 229 per spec Assumptions §12.

## §G — Follow-up-issue seeds surfaced during this phase-0

- **Migration for consumers pinning `alpha.N`** — some downstream CI pipelines might have `latest = v0.1.0-alpha.70` pinned. When `v0.2.0` ships, they either auto-jump to v0.2.0 (if pin was `latest`) or stay on alpha.70 forever (if pin was explicit). Neither is broken by this feature but the release notes for `v0.2.0` should call this out. RELEASING.md task addresses.
- **CHANGELOG.md convention post-`v0.2.0`** — the current CHANGELOG format is per-alpha-release. Post-229 the CHANGELOG needs to accommodate both nightlies (probably NOT logged individually — too noisy) and stables (logged as today). RELEASING.md documents; no code change needed.

These are noted in RELEASING.md, not filed as GitHub issues (small-enough to include in the initial writing).
