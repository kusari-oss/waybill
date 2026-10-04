# Tasks: Closure SBOMs for Nix system-configuration flakes

**Input**: `specs/1066-nix-system-config-closure/` (spec.md, plan.md, research.md, data-model.md, contracts/attribute-selection.md, quickstart.md, measurements/)

**Tests**: requested by SC-001…SC-005 and research R8. Write each test first and show it fails before implementing it. For enforcement tests, also show they fail with the feature switched off (the m1065 practice).

Paths: `closure/` = `waybill-cli/src/scan_fs/package_db/nix/closure/`, `eval/` = `waybill-cli/src/scan_fs/package_db/nix/eval/`.

**Naming rule (lesson from #1111):** under `waybill-cli/src/scan_fs/`, no new function may be named `walk_*` or `walk(`. The walker-audit gate (`scripts/check-walker-audit.sh`, now part of `pre-pr.sh`) is name-based.

**Implementation notes (recorded during /speckit-implement):**
- **Fails-first evidence:**
  - T007/T008 failed against unchanged behaviour (only `packages.aarch64-darwin` listed, then degraded) before T009.
  - T011b/d and T012 failed before T013.
  - T017b(a) failed (C190 `None`) before T017c.
  - T017a was mutation-checked: with "packages present" decided from the host's `packages.<system>` (the pre-analysis rule), it fails with the configuration selected on a Mac. Restored, it passes.
- **T011c** first passed only by accident: the full path was treated as a package name and failed the package listing. It now also asserts that the path itself was queried and its "does not provide attribute" classified.
- **T014** needed no change: the existing degradation log line already prints the reason's detail, so T011a passed as soon as US1 landed.
- **Real `nix` tests** (T008, T012, T017) ran against Determinate Nix 3.20.0 / Nix 2.34.6 on `aarch64-darwin`. T012 confirms an `x86_64-linux` configuration evaluates on that host.

## Phase 1: Setup

- [X] T001 Baseline: run `cargo test -p waybill --bin waybill nix::` and `cargo test -p waybill --test nix_eval_tier --test nixpkgs_declarations`, and record them green. Note `nix --version` on this host.

## Phase 2: Foundational (pure logic shared by every story)

- [X] T002 [P] Unit tests in `closure/mod.rs` (`#[cfg_attr(test, allow(clippy::unwrap_used))]`) for `classify_attr(Option<&str>) -> AttrRequest` (contract table):
  - `None` → `Auto`;
  - `"default"`, `"pkg-ghc96"`, `"some_pkg.sub"` → `PackageName` (first segment not standard);
  - `"darwinConfigurations.laptop.system"`, `"nixosConfigurations.web01.config.system.build.toplevel"`, `"packages.x86_64-linux.hello"`, `"homeConfigurations.me.activationPackage"` → `FullPath`;
  - `"darwinConfigurations"` (no `.`) → `PackageName`;
  - `"nixosConfigurations.a b"` and `"packages.x#y"` → `FullPath`, later refused by safety (T004).
- [X] T003 [P] Unit tests in `closure/mod.rs` for `select(packages_output_present, darwin: Option<Vec<String>>, nixos: Option<Vec<String>>) -> Selection` (contract auto-selection table; analysis I1):
  - a `packages` output exists for **any** platform → `PackageDefault` even with configurations, so today's path runs;
  - one darwin → `Configuration(Darwin, n)` with `system_path() == "darwinConfigurations.n.system"`;
  - one nixos → `…config.system.build.toplevel`;
  - one in each → `Degrade(AmbiguousSystemConfiguration)` listing `darwinConfigurations.<a>`, `nixosConfigurations.<b>` sorted;
  - none / `None` → `Degrade(NoEvaluableAttribute)`;
  - names failing `is_safe_attribute_name` (e.g. `"a b"`, `"x\"y"`) are dropped before counting: `["ok", "a b"]` selects `ok`.
- [X] T004 [P] Unit test in `eval/reason.rs`: the new variant's wire code is `several-system-configurations` and its detail lists names sorted, plus an example `--nix-closure-attr`.
- [X] T005 Implement `AttrRequest`, `classify_attr`, `SystemConfiguration { kind, name }` with `system_path()`, and `select(...)` in `closure/mod.rs`. Add `DegradationReason::AmbiguousSystemConfiguration(Vec<String>)` (wire `several-system-configurations`) in `eval/reason.rs`, and extend every exhaustive `match` on `DegradationReason` that the compiler flags. Run T002–T004 to green.
- [X] T006 Change `ClosureConfig.attribute` to `AttrRequest` in `closure/mod.rs`. `ClosureConfig::from_flags(Option<String>, …)` calls `classify_attr`. Update the construction in `waybill-cli/src/cli/scan_cmd.rs` (around `scan_cmd.rs:4448`) and every test constructing `ClosureConfig`. Behaviour must be unchanged at this point: an explicit `default` or `None` takes today's path. Run the existing closure tests to green.

**Checkpoint**: everything compiles, existing tests green, nothing new reachable yet.

## Phase 3: User Story 1 — one configuration, no flags (P1) 🎯 MVP

**Goal**: with no `--nix-closure-attr` and no `packages.<system>.default`, a single system configuration is evaluated.

**Independent test**: a stub `nix` that serves one darwin configuration. The closure query is issued for `<root>#darwinConfigurations.laptop.system`, and C184 `attribute` equals that path.

### Tests for User Story 1

- [X] T007 [P] [US1] New test file `waybill-cli/tests/nix_config_closure.rs`, gated `#![cfg(unix)]` for the whole file per the repository's memory note on unix-only helpers. Use the stub-`nix` harness pattern from `nix_eval_tier.rs::scan_with_stub_nix`, so it runs in CI without Nix:
  - the stub logs each argv to a file. **Any argv it does not recognise exits 1 with a fixed stderr line** (analysis U2), so other tiers' `nix` calls (host detection, the 1050 declarations pass) fail cleanly, and the tests assert only on closure-tier argv;
  - it answers `config show` the way `nix_eval_tier.rs`'s passing stub does (the IFD pre-flight, if reached);
  - `eval --raw --expr builtins.currentSystem` → `aarch64-darwin`;
  - `eval … --apply builtins.attrNames <root>#packages` → exit 1 with the measured message `error: flake '…' does not provide attribute … 'packages'`;
  - `… <root>#darwinConfigurations` → `["laptop"]`;
  - `… <root>#nixosConfigurations` → exit 1;
  - `derivation show -r … <root>#darwinConfigurations.laptop.system` → a minimal one-derivation JSON, copied from an existing closure unit-test fixture.

  Assert:
  - the scan succeeds;
  - the argv log contains a `derivation show` for exactly `…#darwinConfigurations.laptop.system`;
  - the CycloneDX C184 `attribute` is `darwinConfigurations.laptop.system`.
- [X] T008 [P] [US1] Same file, real `nix` (skip with `eprintln!` when `nix` is not on PATH, like `nix_eval_tier.rs`). Fixture `waybill-cli/tests/fixtures/nix_config_closure/one_darwin/flake.nix`: no inputs, `darwinConfigurations.laptop.system = derivation { name = "darwin-system-laptop"; system = "aarch64-darwin"; builder = "/bin/sh"; args = ["-c" "echo > $out"]; }`. Assert C184 `attribute` is `darwinConfigurations.laptop.system` and `derivations` ≥ 1.
  - Note: nix reads a flake inside the waybill repo through git, so the fixture must be `git add`ed before the test can see it locally (CI checks out committed files).

### Implementation for User Story 1

- [X] T009 [US1] In `closure::resolve` (`closure/mod.rs`), branch on `cfg.attribute`:
  - **`PackageName(n)`:** today's code, unchanged.
  - **`Auto`:**
    - list `<root>#packages` with `attributes_argv`. If it succeeds (a `packages` output exists for any platform), run today's package path unchanged with `default`, including today's degradation when the host platform lacks it (I1).
    - Otherwise list `<root>#darwinConfigurations` and `<root>#nixosConfigurations` with `attributes_argv`. A failed call counts as `None`; log the stderr head at debug.
    - Call `select`. On `Configuration(c)`, evaluate `format!("{root}#{}", c.system_path())` with `closure_argv` (no `<system>` inserted; R3) and record `attribute = c.system_path()`. On `Degrade(r)`, return `Err(r)`.
  - **`FullPath(p)`:** handled in T013 (US2).

  Every argv passes `guard` (`argv_is_safe`). Keep function names free of `walk`.
- [X] T010 [US1] Run T007/T008 to green. Then show T007 fails with the `Auto` branch replaced by today's behaviour (temporary local edit, reverted), and record that in this file's notes.

**Checkpoint**: US1 complete; a single-configuration flake gets a closure.

## Phase 4: User Story 2 — several configurations; the operator names one (P1)

**Goal**: several configurations degrade with a listing; a full path evaluates exactly that output.

**Independent test**: a stub serving one darwin and one nixos configuration. Without a path the scan degrades with `several-system-configurations` and both names on stderr; with a full path, exactly that path is queried.

### Tests for User Story 2

- [X] T011 [P] [US2] In `waybill-cli/tests/nix_config_closure.rs` (stub `nix`):
  - **(a) Several, no path:** two configurations, no path. The scan succeeds; stderr contains `several-system-configurations`, `darwinConfigurations.laptop` and `nixosConfigurations.web01` in that order, and `--nix-closure-attr`. The argv log contains **no** `derivation show`. The document has no C184.
  - **(b) Full path:** `--nix-closure-attr nixosConfigurations.web01.config.system.build.toplevel`. The argv log shows `derivation show` for exactly `…#nixosConfigurations.web01.config.system.build.toplevel` and **no** `packages.<system>` listing. C184 `attribute` is the full path.
  - **(c) Absent full path:** `--nix-closure-attr darwinConfigurations.nope.system`, with the stub failing that path using the measured text `error: flake '…' does not provide attribute 'darwinConfigurations.nope.system'`. Degrades `no-evaluable-attribute`, not `evaluation-failed` (analysis U1).
  - **(d) Unsafe full path:** `--nix-closure-attr "nixosConfigurations.web01#evil"`. No `derivation show` is issued at all, and the scan degrades `no-evaluable-attribute`, with a log line saying the path was refused (R6).
- [X] T012 [P] [US2] Real `nix` (skip if absent). Fixture `waybill-cli/tests/fixtures/nix_config_closure/two_configs/flake.nix`: the `one_darwin` shape plus `nixosConfigurations.web01.config.system.build.toplevel = derivation { …; system = "x86_64-linux"; }`. Assert:
  - with no path, the scan degrades with both names listed;
  - with the nixos full path, C184 `attribute` is that path. This also proves FR-004: an `x86_64-linux` derivation evaluates on any host.

### Implementation for User Story 2

- [X] T013 [US2] `FullPath(p)` branch in `closure::resolve`:
  - if `!is_safe_attribute_name(&p)`, log `nix-closure: refusing unsafe attribute path` and return `Err(NoEvaluableAttribute)`;
  - otherwise evaluate `format!("{root}#{p}")` with `closure_argv` (no listing, no `<system>`);
  - a failed evaluation whose stderr contains `does not provide attribute` (measured Nix 2.34 wording; the existing `attribute`+`missing` check in `eval/invoke.rs:449` does not match it) or the existing pattern → `NoEvaluableAttribute`; other failures → today's `EvaluationFailed`;
  - record `attribute = p`.
- [X] T014 [US2] Make sure the degradation log line at `scan_cmd.rs` (around 4636) prints the new reason's detail, so the names reach the operator (FR-008). If it already prints `reason.detail()` (or equivalent), no change. Run T011/T012 to green; show T011(a) fails with `select` changed to pick the first configuration (local edit, reverted).

## Phase 5: User Story 3 — package flakes unchanged (P1)

- [X] T015 [P] [US3] Stub test in `waybill-cli/tests/nix_config_closure.rs`: `packages.aarch64-darwin` lists `["default","other"]` and a darwin configuration also exists. With no path, the argv log shows `derivation show` for `…#packages.aarch64-darwin.default` and **no** configuration listing. C184 `attribute` is `default`.
- [X] T016 [P] [US3] Stub test: an explicit `--nix-closure-attr default` where `packages.<system>` lacks `default` but a configuration exists. It degrades `no-evaluable-attribute` exactly as today and does **not** auto-select (FR-009, R2).
- [X] T017a [P] [US3] Stub test for host independence (FR-003, SC-004; analysis C2). Run the one-configuration and the two-configuration stubs twice, with `builtins.currentSystem` answered as `aarch64-darwin` and then `x86_64-linux`. The closure-tier argv (or the degradation) must be identical across the two hosts. Also run the "Linux-only packages plus a darwin configuration" shape (`<root>#packages` → `["x86_64-linux"]`): on both hosts no configuration is listed or selected.
- [X] T017 [US3] Real `nix` (skip if absent). Fixture `waybill-cli/tests/fixtures/nix_config_closure/package_and_config/flake.nix` with `packages.<each system>.default` plus one configuration. C184 `attribute` is `default`.

## Phase 6: Transparency — C190 `waybill:nix-closure-degraded` (FR-012; analysis C1, absorbs #1115)

**Goal**: a requested closure that is not recorded leaves a document-scope reason in all three formats, for every reason.

- [X] T017b [P] Tests in `waybill-cli/tests/nix_config_closure.rs` (stub `nix`; all three formats via `--format cyclonedx-json --format spdx-2.3-json --format spdx-3-json`):
  - **(a)** several configurations → C190 = `several-system-configurations`, identical in all three formats;
  - **(b)** no `packages` and no configurations → C190 = `no-evaluable-attribute`;
  - **(c)** `--offline --nix-closure` → C190 = `offline-requested`;
  - **(d)** a successful closure (one configuration) → no C190;
  - **(e)** no `--nix-closure` → no C190.
- [X] T017c Thread the value like C180:
  - compute `nix_closure_degraded: Option<&'static str>` in `waybill-cli/src/cli/scan_cmd.rs` from the closure result (`Some(reason.wire())` iff requested and `Err`), next to `nix_eval_degraded` (around `scan_cmd.rs:4948`);
  - carry it through the generate context (`waybill-cli/src/generate/mod.rs`, where C180/C184 are carried);
  - emit it as a document-scope property / annotation in the CycloneDX, SPDX 2.3 and SPDX 3 emitters, exactly where C180 is emitted.
- [X] T017d Add parity extractors: `c190_cdx` / `c190_spdx23` / `c190_spdx3` in `waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs` (the `cdx_anno!`/`spdx23_anno!`/`spdx3_anno!` macros, `document` scope), and the `ParityExtractor { row_id: "C190", label: "waybill:nix-closure-degraded", …, Directionality::SymmetricEqual, order_sensitive: false }` entry in `mod.rs`, next to C180.
- [X] T017e Add the C190 row to `docs/reference/sbom-format-mapping.md`, modelled on C180's row and **on one line**. List the closed reason set, and say it is emitted only when `--nix-closure` was requested and no closure was recorded. Check `every_catalog_row_has_an_extractor` and `every_mikebom_emitted_field_has_a_map_row` pass. Run T017b to green.

## Phase 7: Polish & Cross-Cutting

- [X] T018 [P] Update the `--nix-closure` and `--nix-closure-attr` help text in `waybill-cli/src/cli/scan_cmd.rs` (FR-007):
  - full output paths, the auto-selection rule, and the degradation when several configurations exist;
  - a system configuration's closure is what it is *built from*, not what is installed.

  Do not change the IFD wording here (#1114 owns it).
- [X] T019 [P] Update the C184 row in `docs/reference/sbom-format-mapping.md`: `attribute` may be a full output path, plus the built-from note. Keep the row on **one line**; line breaks inside a table row broke the parity parser in #1111.
- [X] T020 [P] Add an Unreleased entry to `CHANGELOG.md`: system-configuration closures, auto-selection (only for flakes with no `packages` output), full paths, the new reason, C190 for every closure degradation (#1115), and the narrowed FR-001 rule (the standard output names).
- [X] T021 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo EXIT=$?`. Require `EXIT=0`, `Walker-audit allow-list check: OK`, the passed line, and no failing `test result`.
- [ ] T022 Push. Dispatch the read-only public-corpus run. Take the run ID from the dispatch output (`gh workflow run … 2>&1 | grep -o 'runs/[0-9]*'`), never from a branch filter. Expect no golden change, including `nix-closure-moat` (SC-005).
- [X] T023 Run quickstart §1–§3 with real `nix`, and §4 (`measurements/probe_config_closure.sh`) once to confirm SC-002 end to end with the built binary. Append the results to `measurements/README.md`.
- [ ] T024 Open the PR (closes #1052 and #1115; references #1114). The PR checklist notes FR-010: no new flag. Merge when green, then run `cargo clean`.

## Dependencies & Execution Order

- **Phase order:** Phase 1 → Phase 2 (T002–T006) → US1 (T007–T010) → US2 (T011–T014) → US3 (T015–T017a) → Transparency (T017b–T017e) → Polish.
- **Shared code:** US1 and US2 both edit `closure::resolve`, so they run sequentially. US3 adds tests only, plus verification of the unchanged path.
- **Within each story:** tests first, failing; then the implementation.

### Parallel opportunities

- **Phase 2 tests:** T002, T003 and T004 are independent test functions; T005 then T006.
- **US1:** T007 ∥ T008, both in the new file, different functions.
- **US2:** T011 ∥ T012.
- **US3:** T015 ∥ T016.
- **Polish:** T018 ∥ T019 ∥ T020.

## Implementation Strategy

- **MVP:** Phase 2 + US1 (single-configuration flakes, the case #1052 came from).
- **Ship:** US2 and US3 go in the same PR. They are small, share the selection function, and US3 is the regression guard.
