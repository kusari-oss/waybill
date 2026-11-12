# Tasks: Opt-in import-from-derivation for Nix closure scans

**Input**: `specs/1283-nix-allow-ifd/` — plan.md, spec.md, research.md (R1–R8), data-model.md, contracts/, quickstart.md
**Tests**: required (FR-011). Write each test first, confirm it fails for the stated reason, then implement.
**Paths**: relative to the repo root. `NIX` = `waybill-cli/src/scan_fs/package_db/nix`.

**Conventions**
- Test modules that use `.unwrap()` carry `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- Never run `cargo fmt`.
- Revert mutations with `cp`, never `git checkout`.
- The gate is `./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`, with no `+toolchain`.

## Phase 1: Setup

- [X] T001 Promote the recall probe into the feature directory.
  - Copy `$CLAUDE_JOB_DIR/tmp/recall-corpus/scripts/{recall.py,survey.py,scan.sh,realise.sh}` into `specs/1283-nix-allow-ifd/measurements/recall/`.
  - Extract the name-collision classifier from `recall.py` into `specs/1283-nix-allow-ifd/measurements/recall/classify.py`. It is the committed truth rule R7 cites.
  - Add a `README.md` there. It states how truth is defined: sbomnix runtime grype findings, minus name-collision false positives, plus HSEC advisories. It also gives the invocation.
  - Scripts run as `python3 -I -B`.
- [X] T002 Amend `specs/1283-nix-allow-ifd/spec.md`. FR-001, US3 scenario 2 and SC-004 say "unchanged / exactly as today", but C217 is new for every scan that admits a Nix tier (contract C217 "Rules").
  - FR-001 and SC-004: everything except C217 is unchanged, and goldens of scans without a Nix tier are byte-identical.
  - US3-2: the document records `refused` / `verified: true`.
  - FR-008: change "every `--nix-closure` document" to "every document from a scan where a Nix tier (`--nix-closure` or `--nix-eval`) ran". Under `--offline` no tier runs, and C190 `offline-requested` takes C217's place (contract C217 "Present when").

  The plan's Constraints line was already brought into line with this.

## Phase 2: Foundational (blocks all stories)

- [X] T003 Add `IfdPolicy { Refuse, Allow }` with `fn setting(&self) -> &'static str` (`"false"` / `"true"`) and `Default = Refuse` in `NIX/eval/preflight.rs` (data-model §IfdPolicy).
- [X] T004 Add `IfdAllowanceVerified(pub(super) ())` beside `IfdRefusalVerified` in `NIX/eval/result.rs`.
  - Add `enum IfdPolicyVerified { Refused(IfdRefusalVerified), Allowed(IfdAllowanceVerified) }`, constructible only inside `eval`.
  - Add a `fn policy(&self) -> IfdPolicy` accessor.
- [X] T005 Extend `DegradationReason` in `NIX/eval/reason.rs` with `IfdAllowanceUnverified(String)` → `ifd-allowance-unverified` and `ImportFromDerivationBuildFailed(String)` → `import-from-derivation-build-failed`.
  - Every `match` stays exhaustive (no `_ =>`).
  - Update the reason-string table test in that file.
- [X] T006 Unit tests first in `NIX/eval/preflight.rs` `mod tests`, written so they fail now:
  - `verify_for(IfdPolicy::Allow, "allow-import-from-derivation = true\nsandbox = false\n")` → `Allowed`, with sandbox `Some("false")`.
  - The same policy with `= false` → `Err(IfdAllowanceUnverified)`.
  - `Refuse` with `= false` → `Refused`.
  - `Refuse` with `= true` → `Err(IfdRefusalUnverified)`.
  - A missing `sandbox` line → `None`.
  - `sandbox = relaxed` → `Some("relaxed")`.
- [X] T007 Implement T006 in `NIX/eval/preflight.rs`:
  - generalise `verify_from_config_output(stdout)` to `verify_for(policy, stdout) -> Result<(IfdPolicyVerified, NixEnvironment), DegradationReason>`;
  - add `struct NixEnvironment { sandbox: Option<String> }` (data-model §NixEnvironment);
  - keep the old function as a thin `Refuse` wrapper only if callers outside `eval` need it, otherwise remove it.
- [X] T008 Make `NIX/eval/invoke.rs:124` (`config show`) pass `policy.setting()` instead of `"false"`. Change `verify_ifd_refusal_with_config` to `verify_ifd_policy_with_config(policy, budget)`, returning `(IfdPolicyVerified, NixEnvironment)`. Make the same change to:
  - the free function `verify_ifd_refusal` (`invoke.rs:107`, called at `:229`), which becomes `verify_ifd_policy(policy, budget)`;
  - the `Evaluator` trait method in `NIX/eval/mod.rs:120`, called at `:87`, together with its test impl at `:162`.

  `eval::run` takes the policy from its config and passes the verified value on.
- [X] T009 [P] Add the C217 value type `NixEvaluationPolicy { import_from_derivation: IfdPolicy, verified: bool, sandbox: Option<String> }` with sorted-key JSON serialisation `{"import-from-derivation","sandbox","schema":1,"verified"}`.
  - Place it in `NIX/closure/summary.rs`, beside the C216 value, and follow C216's encoding helper.
  - Unit test the exact string for allowed / refused / unverified / sandbox-null.
- [X] T010 [P] Add the C217 catalog row (KEEP-NO-NATIVE, citing research R4) to `docs/reference/sbom-format-mapping.md` after C216. Add the two new C190 reasons to the C190 row's closed list.
- [X] T011 Add C217 parity extractors in `waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs`, mirroring C216's three extractors and the `SymmetricEqual` registration. The lib-target catalog gate needs the doc row (T010) and the extractors in the same commit.

**Checkpoint:** `cargo test -p waybill --lib nix::eval::preflight parity` passes, and every existing test is still green (default policy `Refuse`).

## Phase 3: US1, scan a callCabal2nix flake I trust (P1) 🎯 MVP

**Goal:** with `--nix-allow-ifd`, every Nix evaluation allows IFD, verified. Default behaviour is unchanged.
**Independent test:** a stub-nix scan with the flag records no IFD degradation and C216 is present; without the flag, today's `import-from-derivation-required` is produced.

- [X] T012 [US1] Write the integration tests first in a new `waybill-cli/tests/nix_allow_ifd.rs`, using `tests/common/nix_stub.rs` (follow `tests/nix_ifd_reason.rs` for stub setup):
  - (a) the flag with stub config `= true` passes and the closure is read, with `--option allow-import-from-derivation true` in every recorded argv;
  - (b) without the flag, every recorded argv has `false` (FR-001);
  - (c) the flag with stub config still reporting `= false` → C190 `ifd-allowance-unverified`, and no evaluation argv recorded after the preflight;
  - (d) the flag where the stub's `derivation show` fails with an IFD build error → C190 `import-from-derivation-build-failed`, distinct from `import-from-derivation-required`;
  - (e) `--nix-allow-ifd` with neither `--nix-closure` nor `--nix-eval` → clap rejects it (non-zero exit, "requires" in stderr);
  - (f) the flag on: no recorded argv contains `--accept-flake-config` or `--impure` (FR-004).
  - (f′) the flag on, with the #1461 selection fixture (`closure-outputs.json`): the recorded `eval --apply` selection argv carries `allow-import-from-derivation true`, and C216 `outputs-source` is not `default-output`. Without the flag it carries `false`.

  Extend `nix_stub.rs` (with `..Default::default()` on the existing literals) for: a configurable `config show` IFD value and `sandbox` line, and an IFD-build-failure stderr.
- [X] T013 [US1] Thread `IfdPolicyVerified` through the closure tier in `NIX/closure/mod.rs`:
  - `resolve()` calls `verify_ifd_policy_with_config(cfg.ifd_policy, cfg.budget)?`;
  - `closure_argv` and `attributes_argv` take `&IfdPolicyVerified` and emit `policy().setting()`;
  - add `ifd_policy: IfdPolicy` to the closure config struct;
  - update the unit test at `closure/mod.rs:829` to cover both values.
- [X] T013a [US1] Do the same in `NIX/closure/runtime.rs::evaluate_selection` (from #1461, `:136`). `evaluate_selection` takes `&IfdPolicyVerified` and emits `policy().setting()`. `resolve()` passes the value it verified in T013, with no second preflight. Update the runtime.rs unit tests that assert the selection argv.
  - **Why it matters:** if this site still refuses, an IFD flake reads its closure and then quietly falls back to the default output (research R1).
  - **Precondition:** PR #1461 has landed and this branch has been rebased onto it. If it hasn't landed, stop and report rather than port the selection code.
- [X] T014 [P] [US1] Do the same in `NIX/eval/invoke.rs:~306`: the version eval argv uses the verified policy, and the eval entry point takes `IfdPolicyVerified`. Update the test at `invoke.rs:500`.
- [X] T015 [P] [US1] Do the same in `NIX/eval/licenses.rs:212` and its tests at `:365` and `:600`.
- [X] T016 [P] [US1] Do the same in `NIX/declarations/evaluate.rs:141` and `:237`. They receive the verified policy from their caller rather than re-verifying, unless the call path today verifies separately; if it does, keep that, and verify the same policy.
- [X] T017 [US1] Classify IFD build failures in `NIX/eval/stderr.rs::classify_failure`. When the policy is `Allow` and stderr shows a failed build of a `.drv` during evaluation, return `ImportFromDerivationBuildFailed(salient line)`. Add unit tests with captured stderr shapes. Take a real one from a deliberately broken local IFD expression and commit it under `specs/1283-nix-allow-ifd/measurements/ifd-eval/build-failed.err`.
- [X] T018 [US1] Add the flag in `waybill-cli/src/cli/scan_cmd.rs`:
  - `#[arg(long = "nix-allow-ifd")] nix_allow_ifd: bool`, with an `ArgGroup` so it requires `nix_closure` or `nix_eval` (contract cli-nix-allow-ifd);
  - map it to `IfdPolicy` and pass it to the closure, eval, licence and declaration tiers;
  - under `--offline`, log at INFO that the flag had no effect.
- [X] T019 [US1] Add a grep guard test in `waybill-cli/tests/nix_allow_ifd.rs`, so the IFD value always comes from the policy (FR-002, R1):
  - Read every `.rs` under `NIX/`, keeping only the text **before** the file's first `#[cfg(test)]` line. Live nix tests inside test modules, such as `eval/licenses.rs:~600`, keep their literals.
  - Join each `IFD_SETTING` occurrence with the next non-blank token, since argv vectors are often one element per line.
  - Assert that no occurrence is followed by a `"false"` or `"true"` string literal.
  - Assert that the number of `IFD_SETTING` uses in non-test code is at least 8, so the guard can't pass by finding none.

  Mutation check: re-introduce a literal at `closure/runtime.rs`, see the test fail, restore with `cp`.
- [X] T020 [US1] Run T012 (a)–(f) and confirm all pass. Run the existing `nix_ifd_reason.rs`, `nix_eval_tier.rs`, `nixpkgs_declarations.rs` and `closure_*.rs` tests unchanged.

**Checkpoint:** US1 works end to end with stub nix.

## Phase 4: US2, understand the risk before opting in (P1)

**Independent test:** `--help`, the WARN and the docs each contain the untrusted-flake caution.

- [X] T021 [US2] Add tests to `waybill-cli/tests/nix_allow_ifd.rs`:
  - (g) `waybill sbom scan --help` contains "--nix-allow-ifd", "untrusted flake" and "unsandboxed";
  - (h) the default policy, with IFD degradation → stderr WARN contains "--nix-allow-ifd" and "trust";
  - (i) the flag with stub `sandbox = false` → one WARN containing "sandbox is off", and the scan proceeds (closure read);
  - (j) the flag with `sandbox = true` → no sandbox WARN.
- [X] T022 [US2] Write the help text from contracts/cli-nix-allow-ifd.md on the T018 arg in `waybill-cli/src/cli/scan_cmd.rs`.
- [X] T023 [US2] Extend the degradation WARN at `waybill-cli/src/cli/scan_cmd.rs:~4754`. When the reason is `ImportFromDerivationRequired`, append "Pass --nix-allow-ifd to allow it, only for a flake you trust."
- [X] T024 [US2] Add the sandbox WARN in `waybill-cli/src/cli/scan_cmd.rs`, or wherever the preflight result is first available: once per scan, when the policy is `Allow` and `NixEnvironment.sandbox == Some("false")`.
- [X] T025 [P] [US2] Add an "Import-from-derivation" section to `docs/reference/nix-evaluation.md` (FR-007):
  - what IFD is;
  - why waybill refuses it by default;
  - the flag and its caution;
  - the sandbox caveat (macOS default off);
  - that sbomnix 1.9.0 inherits nix's default `true` and performs IFD silently (cite `measurements/sbomnix-and-host.txt`);
  - that `--accept-flake-config`/`--impure` stay refused.

  Add the flag row to `docs/user-guide/cli-reference.md`, and add IFD to the degradation paragraph in `docs/ecosystems.md`.

**Checkpoint:** US2 tests (g)–(j) pass.

## Phase 5: US3, know from the document how it was produced (P2)

**Independent test:** C217 is present with the expected value in all three formats, both with and without the flag.

- [X] T026 [US3] Add tests to `waybill-cli/tests/nix_allow_ifd.rs`:
  - (k) the flag, verified, sandbox false → C217 `{"import-from-derivation":"allowed","sandbox":"false","schema":1,"verified":true}` in CDX `metadata.properties`, SPDX 2.3 document annotations and SPDX 3 document Annotation, and equal across the three formats;
  - (l) the default → `refused` / `verified: true`;
  - (m) the allowance unverified → `allowed` / `verified: false`, plus C190;
  - (n) no Nix tier requested → C217 absent;
  - (o) `--offline` with `--nix-closure` → C217 absent.
- [X] T027 [US3] Carry C217 from the preflight to the generators. Add `nix_evaluation_policy: Option<NixEvaluationPolicy>` beside the C216 field, following C216's path:
  - `waybill-cli/src/generate/mod.rs`
  - `waybill-cli/src/generate/cyclonedx/builder.rs`
  - `waybill-cli/src/cli/scan_cmd.rs`

  Populate it whenever a Nix tier was admitted, including on degradation (`verified: false` when the preflight failed).
- [X] T028 [P] [US3] Emit C217 in `waybill-cli/src/generate/cyclonedx/metadata.rs` next to C216.
- [X] T029 [P] [US3] Emit C217 in `waybill-cli/src/generate/spdx/annotations.rs` next to C216.
- [X] T030 [P] [US3] Emit C217 in `waybill-cli/src/generate/spdx/v3_annotations.rs` next to C216, with a deterministic annotation IRI via the existing `hash_prefix` helper.
- [X] T031 [US3] Regenerate the six golden suites that contain Nix-tier fixtures with `WAYBILL_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1`. Verify with a normalised, sorted diff that the only change is the added C217 line (SC-004/SC-005). Goldens of scans without a Nix tier must be byte-identical.
- [X] T032 [US3] Add an FR-009 test to `waybill-cli/tests/nix_allow_ifd.rs`. A stub flake that needs no IFD, scanned with and without the flag, yields documents equal after removing C217 from both: same components, relationships, scope, VEX.

**Checkpoint:** US1–US3 complete; all hermetic tests pass.

## Phase 6: Nightly corpus targets and recall (FR-014, SC-001–SC-003)

- [X] T033 Change `ScanMode::NixClosure { attr }` to `{ attr, allow_ifd: bool }` in `waybill-cli/tests/corpus_harness_195/manifest.rs`. Set `allow_ifd: false` on the existing targets (`manifest.rs:241`, `:268`). Update the pattern matches in `harness.rs:218` and `cache.rs:377`.
- [X] T034 Add `--nix-allow-ifd` to the scan argv when `allow_ifd` is true in `waybill-cli/tests/corpus_harness_195/harness.rs`.
- [X] T035 Add three targets in `waybill-cli/tests/corpus_harness_195/manifest.rs`, each `NixClosure { attr: "default", allow_ifd: true }`, Linux-only like moat:
  - `nix-closure-slack-web` (MercuryTechnologies/slack-web@7b46273d)
  - `nix-closure-openai` (@8a357137)
  - `nix-closure-slacklinker` (@72af26a7)

  Copy the full SHAs from `specs/1283-nix-allow-ifd/measurements/stage2-target-shas.txt`.
- [X] T036 Add the recall proxy expectation in `waybill-cli/tests/corpus_harness_195/expectations.rs` (data-model §RecallTruth):
  - load `waybill-cli/tests/fixtures/public_corpus/<target>/recall-truth.json`;
  - take the runtime-only view: walk components recursively and drop CycloneDX `scope == "excluded"`, matching `--drop scope=excluded`;
  - count the truth findings matched by the exact rule in data-model §RecallTruth "Proxy match": normalized name, exact version, then CPE vendor:product from `cpe` or `waybill:cpe-candidates`, or purl ignoring qualifiers;
  - assert the ratio ≥ 0.90, and on failure list the misses.

  Also assert: no C190 `import-from-derivation-required`; C217 `allowed` and `verified: true`; C216 present. Unit-test the matcher with a hand-made three-finding truth file (one CPE hit, one purl hit, one miss → 0.667).
- [X] T037 [P] Create `.github/workflows/recall-truth.yml`, `workflow_dispatch` only, on x86_64-linux, with input `targets`.
  - Install nix through the action public-corpus.yml already pins.
  - Realise each target at its pinned SHA.
  - Run `nix run nixpkgs/<pinned rev>#sbomnix` (runtime), grype (pinned version, pinned DB) on its CycloneDX, and osv-scanner for HSEC.
  - Apply `classify.py` (T001) and upload the artifacts `recall-truth.json` per target.

  Follow `feedback_github_actions_security`: SHA-pinned actions, `permissions: contents: read`, no untrusted `${{ }}` interpolation into `run:`.
- [X] T038 Dispatch `recall-truth.yml` on this branch (operator action; prepare the exact `gh workflow run` command in the PR description). Review the artifact's excluded list against classify.py, and commit `waybill-cli/tests/fixtures/public_corpus/nix-closure-{slack-web,openai,slacklinker}/recall-truth.json`.
- [X] T039 Dispatch `public-corpus.yml` with `regen_goldens=true` (operator action). Commit the three new targets' goldens plus the C217 line on the existing nix-closure goldens (moat, shellcheck). Read the diffs: only C217 on existing targets.

## Phase 7: Polish

- [X] T040 [P] Add a CHANGELOG `[Unreleased]` entry in `CHANGELOG.md` (Added: `--nix-allow-ifd`, C217; Changed: the WARN names the flag). Use `Fixes #1283` in the PR.
- [ ] T041 Live check on macOS per quickstart.md "Local live check":
  - scan slack-web and openai with the flag; record C216, C217 and the absence of C190;
  - run `measurements/recall/recall.py` against sbomnix runtime grype truth;
  - write recall (min/median/max over 3 scans) to `specs/1283-nix-allow-ifd/measurements/recall/macos-results.txt`.

  SC-001/SC-002 must be ≥ 90%, or the shortfall is explained with each missed finding's cause.
- [X] T042 Check the proxy against real grype. For one target, run grype on waybill's runtime-only CycloneDX and compare its recalled set with the T036 proxy's recalled set. Record the agreement in `specs/1283-nix-allow-ifd/measurements/recall/proxy-vs-grype.txt`.
- [X] T043 Run the full gate. Check `GATE_EXIT=0` and the `>>> all pre-PR checks passed.` line, and enumerate every `test result:` line. Then diff the `#[cfg]` attributes around edited functions in `scan_cmd.rs` against `main`.

## Dependencies

- **External:** PR #1461 (#1458) must land, and this branch be rebased onto it, before T013a. The other tasks don't depend on it.
- Setup (T001–T002) → Foundational (T003–T011) → US1 (T012–T020) → US2 and US3. US2 and US3 are independent of each other; both need T018's flag.
- Corpus (T033–T039) needs US1 + US3 (expectations assert C217). T038 must precede T039: goldens are regenerated after the truth is committed, so the expectation can run.
- Polish follows everything; T041 needs only US1 + US3.

## Parallel examples

- Foundational: T009 ∥ T010 after T003–T008. T011 waits for T010.
- US1: T013a ∥ T014 ∥ T015 ∥ T016 after T013 (different files).
- US2: T025 (docs) ∥ T022–T024 (code).
- US3: T028 ∥ T029 ∥ T030 after T027.
- Corpus: T037 (workflow) ∥ T033–T036 (harness).

## Implementation strategy

MVP is US1 (T001–T020): the flag works, verified, with default behaviour unchanged. Then US2, which must ship with US1 in the same PR, since the flag must not ship without its caution (FR-005/FR-006). Then US3 and the golden regeneration. The corpus phase can be a follow-up PR if the operator dispatches stall it, but SC-001–SC-003 are the release gate, so the release waits for T041 at least.
