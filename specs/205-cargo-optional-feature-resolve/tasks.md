---
description: "Task list for m205 — fix cargo optional-dep over-exclusion via feature-activation resolution"
---

# Tasks: Fix Cargo Optional-Dep Over-Exclusion — Resolve Feature Activation

**Input**: Design documents from `/specs/205-cargo-optional-feature-resolve/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, quickstart.md ✓

**Tests**: Tests-included. Every P1 story gets its own integration test + FR-004 gets a dedicated warn-and-fallback test.

**Organization**: 7 phases — setup (baseline recon), foundational (subprocess resolver + error enum + timeout helper), then 3 P1 story phases, then the dedicated FR-004 warn-and-fallback phase, then polish. US1 + US2 + US3 all complete when the Phase-3 classifier delta lands (single-line change); each phase adds the story-specific integration test that pins its acceptance criteria.

## Format: `[ID] [P?] [Story] Description with file path`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1, US2, US3 mapping to spec.md user stories
- **File paths**: absolute or repo-relative — every task cites exact target

## Phase 1: Setup (Baseline + Recon)

**Purpose**: Establish pre-m205 baseline for SC-004 (byte-identity) and SC-005 (pre-PR delta). Re-verify all quickstart.md `Empirical re-verification` grep results actually match the current tree.

- [ ] T001 Verify pre-m205 baseline pre-PR is green by running `./scripts/pre-pr.sh` on branch `205-cargo-optional-feature-resolve` HEAD (post-checkout, pre-implementation) and capture wall-clock time to `/tmp/m205-prepr-baseline.txt` for SC-005 delta measurement.
- [ ] T002 [P] Golden-drift baseline: `git diff --stat main -- mikebom-cli/tests/fixtures/` (expected: empty — branch is spec+plan only) — record to `/tmp/m205-golden-baseline.txt`. Post-implementation the diff MUST show ZERO drift for non-Cargo fixtures (SC-004 assertion).
- [ ] T003 [P] Recon: verify every line number cited in plan.md / data-model.md is still valid by running quickstart.md's `Empirical re-verification at implement time` block. Record grep outputs to `/tmp/m205-recon.txt` for downstream tasks to consume. Concretely:
  - `grep -n "optional_names.contains" mikebom-cli/src/scan_fs/package_db/cargo.rs` — expect single site at line 1155.
  - `grep -n "collect_optional_dep_keys\|fn parse_lockfile" mikebom-cli/src/scan_fs/package_db/cargo.rs` — confirm parse_lockfile def at line 1057; collect_optional_dep_keys at line 813.
  - `grep -c "parse_lockfile(" mikebom-cli/src/scan_fs/package_db/cargo.rs` — count call sites (~1 production at line 1259 + test callsites; tally so T008's signature update is precise).
  - `command -v cargo && cargo --version` — confirm cargo is on PATH for the shell-out.

## Phase 2: Foundational (Prerequisites for ALL user stories)

**Purpose**: Add the subprocess resolver + error enum + timeout helper. NO classifier behavior changes yet — those land in Phase 3. Every user story test (Phases 3-6) transitively depends on these types.

- [ ] T004 Add `CargoMetadataResolveFailure` enum to `mikebom-cli/src/scan_fs/package_db/cargo.rs` per data-model E1 (adjacent to existing `CargoError` enum). 5 variants: `BinaryNotFound`, `NonZeroExit { code, stderr_head }`, `Timeout { timeout_secs }`, `ParseError { source: serde_json::Error }`, `IoError(#[from] std::io::Error)`. Derive `Debug, thiserror::Error`. Display strings exactly as data-model E1 specifies (whitespace + backtick placement matters for the WARN log fixture in T013).
- [ ] T005 [P] Add `resolve_cargo_metadata_timeout() -> Duration` helper to `cargo.rs` per data-model E2 — reads `MIKEBOM_CARGO_METADATA_TIMEOUT_SECS`, parses as `u64`, clamps to `[1, 3600]`, defaults 60 on absent/parse-fail. 4-8 LOC. Include doc-comment citing m203 precedent.
- [ ] T006 Add `resolve_activated_deps_via_cargo_metadata(workspace_root: &Path, timeout: Duration) -> Result<HashSet<String>, CargoMetadataResolveFailure>` to `cargo.rs` per data-model E3 + research R1's cargo metadata shape + R3's m055 subprocess pattern. Structure:
  - Probe `Command::new("cargo").arg("--version").output()` → BinaryNotFound on `ErrorKind::NotFound`, IoError otherwise.
  - Spawn worker thread: `Command::new("cargo").args(["metadata", "--format-version", "1"]).current_dir(workspace_root).output()`; send result via `mpsc::channel`.
  - `rx.recv_timeout(timeout)` → Timeout on channel timeout, IoError on subprocess I/O err, NonZeroExit with `stderr_head` (capped 20 lines per m203 `cap_stderr_lines` precedent — reuse helper OR inline equivalent) on non-zero exit.
  - Zero exit → `serde_json::from_slice(&output.stdout)?` → navigate to `.resolve.nodes[]` → for each node, extend `HashSet` with `deps[i].name`. Return `Ok(activated_names)`. ParseError on serde err.
- [ ] T007 [P] Add unit tests to `cargo.rs::tests` covering T004-T006:
  - `resolve_cargo_metadata_timeout_default_when_env_var_absent_m205` — unset env → `Duration::from_secs(60)`.
  - `resolve_cargo_metadata_timeout_honors_env_var_m205` — `=42` → 42s.
  - `resolve_cargo_metadata_timeout_clamps_below_min_m205` — `=0` → 1s.
  - `resolve_cargo_metadata_timeout_clamps_above_max_m205` — `=99999` → 3600s.
  - `resolve_cargo_metadata_timeout_ignores_parse_error_m205` — `=notanumber` → 60s.
  - `cargo_metadata_resolve_failure_display_formats_all_variants_m205` — construct all 5 variants + format each, assert human-readable Display matches data-model E1 exactly (needed by T013 which greps stderr for these strings).
  - Use a `with_env` helper mirroring m203's `with_helm_render_timeout_env` (env-var-mutating tests require serial execution per `--test-threads=1`).
- [ ] T008 Post-T004/T005/T006/T007 sanity: run `CARGO_TARGET_DIR=/tmp/m205-c cargo +stable check --workspace --tests 2>&1 | tail -20`. Expected: clean compile. The `as_wire_str`-style dead-code warning may appear on `resolve_activated_deps_via_cargo_metadata` until Phase 3 wires it in — acceptable at this checkpoint (will resolve at T009).

## Phase 3: User Story 1 — Feature-activated optional dep is Runtime (Priority: P1)

**Story Goal**: A default-feature-activated optional dep classifies as `LifecycleScope::Runtime`, NOT `Optional`. CDX `scope: "runtime"`; NO `mikebom:optional-derivation` annotation.

**Independent Test Criterion**: Synthetic workspace with `[dependencies] serde = { optional = true }` + `[features] default = ["serde"]` produces `serde`'s CDX scope as `"runtime"` (or absent, defaulting to runtime). The `mikebom:optional-derivation` property is absent for `serde`.

- [ ] T009 [US1] Classifier delta + caller wiring in `mikebom-cli/src/scan_fs/package_db/cargo.rs` per data-model E4 + E5:
  - **E4** at `cargo.rs:1155`: change the classifier check from `optional_names.contains(&pkg.name)` to `optional_names.contains(&pkg.name) && !activated_names.contains(&pkg.name)`. Add doc-comment explaining the fallback interaction (per data-model E4 verbatim comment).
  - Add `activated_names: &HashSet<String>` as a new parameter to `parse_lockfile` (line 1057; insert between `optional_names` and `root_names` params per data-model E4).
  - **E5** at `cargo.rs:1259` (parse_lockfile invocation site): before calling `parse_lockfile`, compute `let workspace_root = lock_path.parent().unwrap_or(&lock_path);`, call `resolve_activated_deps_via_cargo_metadata(workspace_root, resolve_cargo_metadata_timeout())`. On `Ok(names)`, pass `&names`. On `Err(e)`, emit the FR-004 WARN log per data-model E5 verbatim (`tracing::warn!(workspace = %workspace_root.display(), reason = %e, "cargo metadata failed; falling back to name-only optional classification (safe over-inclusion — deps marked Runtime instead of Optional so vuln-scanners never miss shipped deps)")`), then populate `activated_names = workspace_sections.optional_deps.clone()` (safe over-inclusion).
  - Update every test callsite of `parse_lockfile` inside `cargo.rs::tests` (per T003 recon count) to pass an appropriate `activated_names`. For most unit tests: pass `&HashSet::new()` — the tests scope to non-optional-dep classification and don't exercise the new gate.
- [ ] T010 [US1] Integration test `us1_default_feature_activated_optional_dep_is_runtime` in a NEW file `mikebom-cli/tests/cargo_optional_feature_resolve.rs`:
  - Build synthetic Cargo workspace via `tempfile::tempdir()`: `Cargo.toml` with `[package]`, `[dependencies] serde = { version = "1", optional = true }`, `[features] default = ["serde"]`; `src/main.rs` with `fn main() {}`.
  - Shell out to `cargo generate-lockfile` in the tempdir (matches m087 test pattern) so Cargo.lock exists for the scan. Skip cleanly with `eprintln!` if `cargo` not on PATH (default CI has cargo; per m173/m203 precedent).
  - Shell out to the mikebom binary via `env!("CARGO_BIN_EXE_mikebom")`: `sbom scan --offline --path <tempdir> --format cyclonedx-json --output <out>`.
  - Parse output. Assert:
    - `.components[]` contains a component whose `.purl` matches `pkg:cargo/serde@`.
    - That component's `.scope` is `"runtime"` OR the `scope` field is absent (CDX 1.6 defaults to runtime when omitted).
    - That component's `.properties[]` does NOT contain `{name: "mikebom:optional-derivation"}`.
    - That component's `.properties[]` does NOT contain `{name: "mikebom:lifecycle-scope", value: "optional"}`.

## Phase 4: User Story 2 — Truly-optional dep stays Optional (Priority: P1)

**Story Goal**: A dep declared `optional = true` AND NOT activated by any enabled feature still classifies as `LifecycleScope::Optional` + `mikebom:optional-derivation = "cargo-optional-true"`. m179 signal preserved.

**Independent Test Criterion**: Synthetic workspace with `[dependencies] regex = { optional = true }` + `[features] enable-regex = ["regex"]` (NOT in `default`) produces `regex`'s CDX scope as `"excluded"` + `mikebom:optional-derivation = "cargo-optional-true"` annotation.

- [ ] T011 [US2] Integration test `us2_truly_optional_dep_stays_optional` in `mikebom-cli/tests/cargo_optional_feature_resolve.rs`:
  - Build synthetic Cargo workspace via tempdir: `Cargo.toml` with `[dependencies] regex = { version = "1", optional = true }` + `[features] enable-regex = ["regex"]` (NO `default = [...]` entry, or `default = []`).
  - `cargo generate-lockfile` + mikebom scan (same shape as T010).
  - Assert:
    - `.components[]` contains `pkg:cargo/regex@`.
    - Component `.scope` is `"excluded"`.
    - Component `.properties[]` contains `{name: "mikebom:optional-derivation", value: "cargo-optional-true"}`.
    - Component `.properties[]` contains `{name: "mikebom:lifecycle-scope", value: "optional"}`.

## Phase 5: User Story 3 — Non-Cargo scan byte-identity (Priority: P1)

**Story Goal**: Non-Cargo scans see zero drift versus pre-m205 output. The cargo reader is not invoked; no cargo metadata shell-out; no classifier change.

**Independent Test Criterion**: Scanning an existing non-Cargo public_corpus fixture (npm-express, go-cobra, python-flask, maven-guice) produces byte-identical output pre-m205 vs post-m205.

- [ ] T012 [US3] Integration test `us3_non_cargo_scan_byte_identical` in `mikebom-cli/tests/cargo_optional_feature_resolve.rs`:
  - Sanity assertion approach (byte-identity vs pre-existing golden is verified via T015's audit and the workspace-wide test suite; this integration test is a lighter-weight in-process regression guard).
  - Shell out to mikebom binary scanning `mikebom-cli/tests/fixtures/public_corpus/npm-express/` (a fixture with no Cargo files at all).
  - Assert: emitted CDX contains NO component whose `.properties[]` has `{name: "mikebom:optional-derivation"}` (would indicate the cargo reader spuriously fired) AND stderr contains NO substring `"cargo metadata"` (would indicate the resolver invoked on a non-Cargo scan).

## Phase 6: FR-004 — Fallback + WARN when cargo absent

**Purpose**: THE ask the user surfaced explicitly. FR-004 mandates that when `cargo metadata` fails, mikebom (a) WARNs with workspace + reason, (b) falls back to safe over-inclusion (all optional → Runtime), (c) never silently under-reports vulns. This phase adds the dedicated regression test.

- [ ] T013 [P] Integration test `fr004_cargo_absent_warns_and_falls_back` in `mikebom-cli/tests/cargo_optional_feature_resolve.rs`, `#[cfg(unix)]` per m203 precedent (PATH scrubbing is POSIX-only; Windows uses PATHEXT resolution + skips):
  - Reuse the synthetic workspace from T010 (default-feature-activated optional dep). Requires cargo present to build the initial lockfile (via `cargo generate-lockfile`), BUT the mikebom scan invocation runs with `PATH=""` (scrubbed) so mikebom itself cannot find cargo → `BinaryNotFound` fallback fires.
  - Shell out to mikebom binary with `.env("PATH", "")` (matches m203's `scan_dir_with_env` pattern at `helm_reader.rs` verbatim; may copy helper from there or introduce local equivalent).
  - Assert:
    - (a) Scan exits 0 (mikebom never aborts on cargo-absent fallback; FR-004 + Constitution Principle III).
    - (b) stderr contains BOTH substrings `"cargo metadata"` AND `"falling back"` (WARN log fired; matches the exact log fixture pinned in T009's data-model E5 wording).
    - (c) stderr contains ONE OF `"BinaryNotFound"` / `"cargo\` binary not found on $PATH"` (matches `CargoMetadataResolveFailure::BinaryNotFound` Display from T004; pinned in T007's `cargo_metadata_resolve_failure_display_formats_all_variants_m205` unit test).
    - (d) Emitted CDX: `serde`'s component `.scope` is `"runtime"` (or absent = default runtime), NOT `"excluded"`. Safe over-inclusion verified — even without cargo resolving activation, optional deps default to Runtime.
    - (e) Emitted CDX: `serde`'s component `.properties[]` does NOT contain `{name: "mikebom:optional-derivation"}` (the Optional branch was unreachable → no derivation annotation emitted).

## Phase 7: Polish & Delivery

**Purpose**: Verification, quickstart re-verify, PR body draft.

- [ ] T014 [P] Run every existing cargo-related test to confirm zero regression: `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --lib -- scan_fs::package_db::cargo::tests --no-fail-fast 2>&1 | tail -3` (expected: `ok. N passed; 0 failed`). Also run `cargo +stable test --manifest-path mikebom-cli/Cargo.toml --test transitive_parity_cargo --no-fail-fast 2>&1 | tail -3` (m083 cargo audit fixture — must stay green; may need minor adjustment if a fixture dep was mis-Optional-classified pre-fix and now flips to Runtime, but the shape of that fixture's audit script is scope-agnostic).
- [ ] T015 Re-run T002 audit post-implementation: `git diff --stat mikebom-cli/tests/fixtures/`. Compare to /tmp/m205-golden-baseline.txt. Assert delta is limited to `public_corpus/rust-ripgrep/*` (the sole Cargo fixture in the corpus — post-fix, its goldens may need regeneration IF the ripgrep workspace has any mis-Optional-classified deps under alpha.63; if none, ZERO drift is expected). If drift extends beyond that path, STOP and diagnose (indicates the cargo reader is spuriously invoking on non-Cargo scans, violating FR-005).
- [ ] T016 Regenerate `rust-ripgrep` goldens if T015 shows drift there. Use the standard `MIKEBOM_UPDATE_PUBLIC_CORPUS_GOLDEN=1` env var per `docs/dev/regen-goldens.md`. Verify per-file diff is limited to reclassifying misclassified optional deps (typically 0-3 components flipping `scope: excluded` → `scope: runtime`).
- [ ] T017 Run `./scripts/pre-pr.sh` post-implementation. Capture wall-clock time; compute delta vs T001 baseline; MUST be ≤ 10 seconds per SC-005. Enumerate every `^---- .+ stdout ----` line if any test binary fails (per `feedback_prepr_gate_bails_on_first_failure` memory).
- [ ] T018 [P] (Requires `test-vaultwarden` cloned locally) Manually execute quickstart.md Reproducer 1 against the reporter's original test case. Confirm `reqsign-aws-v4@3.0.1`'s CDX `.scope` is `"runtime"` (NOT `"excluded"`) and `quick-xml@0.40.1` reappears (was orphaned pre-fix). SC-001 verified against the reporter's exact scenario.
- [ ] T019 [P] Manually execute quickstart.md Reproducer 5 (non-Cargo byte-identity) — build baseline + postfix mikebom binaries, scan `npm-express` fixture with each, `diff` the outputs. Expected: byte-identical. SC-004 verified end-to-end.
- [ ] T020 Draft PR body with `Closes #593` per SC-006. Include:
  - (a) 1-paragraph summary: bug direction (m179 over-exclusion of feature-activated optional deps → CDX `scope: excluded` → downstream vuln-scanners silently drop them), fix mechanism (`cargo metadata --format-version 1` shell-out → intersect with `optional_names`).
  - (b) FR-004 warn-on-fallback emphasized: even without cargo binary, mikebom safely over-includes and warns.
  - (c) Reporter attribution (@nchelluri gist link).
  - (d) Test coverage: US1/US2/US3 integration + T013 FR-004 fallback + T014 regression + T018 manual Reproducer 1 against reporter's exact case.
  - (e) Code-diff LOC + files: ~200 LOC across 1 source file (`cargo.rs`) + 1 new test file (`cargo_optional_feature_resolve.rs`).
  - (f) Golden-regen scope: rust-ripgrep only if any of its deps were mis-classified pre-fix (0-3 components expected).
  - (g) Follow-up note: sibling classifiers for npm (m180), pip (m183), maven (m184) may share the same underlying assumption — investigate whether they need analogous feature-resolution work in a future milestone (out of scope for m205 per spec Assumptions).

---

## Dependencies

Sequential within phases; phases mostly sequential across the milestone:

```
Phase 1 (Setup) ── T001, T002, T003 in parallel
     ↓
Phase 2 (Foundational) ── T004, T005 [P] → T006 → T007 [P] → T008 (sanity)
     ↓
Phase 3 (US1) ── T009 (classifier delta + caller wiring, single-file sequential)
     ↓ → T010 (US1 integration test)
Phase 4 (US2) ── T011 (US2 integration test — reuses T009's classifier delta)
     ↓
Phase 5 (US3) ── T012 (US3 in-process regression guard)
     ↓
Phase 6 (FR-004) ── T013 (WARN + fallback integration test — user-explicit ask)
     ↓
Phase 7 (Polish) ── T014, T018, T019 in parallel → T015 → T016 (if drift) → T017 → T020
```

**MVP** = Phase 1 + Phase 2 + Phase 3 (US1 only). Delivers: default-feature-activated optional deps stop being silently excluded → downstream vuln-scanners see them again. US2 (truly-optional preserved) + US3 (byte-identity) + FR-004 (warn-and-fallback) are all satisfied by the same Phase-3 classifier delta; each adds a story-specific regression test on top.

## Parallel opportunities

- **Setup** (T002, T003): both read-only.
- **Foundational** (T005, T007): different helper functions vs different test module.
- **Phase 7** (T014, T018, T019): all read-only assertions.

## Implementation strategy

Ship as a single PR — Phase 2 types + Phase 3 classifier delta + all 4 tests (T010, T011, T012, T013) form a coherent bugfix. Splitting would either leave the classifier changed without regression coverage (T010-T012) OR the FR-004 fallback path untested (T013). Reporter's specific case (SC-001) is verified manually via T018 since it requires an external repo.

**Total task count**: 20 tasks.
**By story**: US1 = 2 tasks (T009-T010), US2 = 1 task (T011), US3 = 1 task (T012), FR-004 = 1 task (T013). Phase 1 = 3, Phase 2 = 5, Phase 7 = 7 = 15 non-story.
