# Implementation Plan: Opt-in import-from-derivation for Nix closure scans

**Branch**: `1283-nix-allow-ifd` | **Date**: 2026-11-11 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/1283-nix-allow-ifd/spec.md`

## Summary

Add `--nix-allow-ifd`, an opt-in that requires a Nix tier. With it, every Nix evaluation passes `allow-import-from-derivation true`. The setting is verified the same way the refusal is today, and a type makes evaluation impossible on an unverified policy. Scans warn when nix's sandbox is off. A new document row, C217 `waybill:nix-evaluation-policy`, records the policy, whether it was verified, and nix's sandbox setting, on success and degradation alike. slack-web, openai and slacklinker join the nightly corpus with the opt-in, and their recall is checked against a committed truth baseline produced on Linux.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate.
**Primary Dependencies**: Existing only — `clap` (flag + `ArgGroup`), `serde`/`serde_json` (C217), `tracing` (WARNs), `anyhow`/`thiserror`; the `eval::invoke` helpers (`run_bounded`, `argv_is_safe`) and `eval::preflight`. **Zero new Cargo dependencies** (FR-012).
**Storage**: N/A. Everything is in-process per scan.
**Testing**: `cargo test`.
- stub `nix` (`tests/common/nix_stub.rs`) for allowed, verified, unverified, IFD-build-failed and sandbox-off;
- unit tests for preflight parsing;
- a grep test that no hard-coded `"false"` IFD literal remains;
- `#[ignore]` corpus targets for the live path.
**Target Platform**: macOS and Linux, wherever `--nix-closure` runs. Nightly measurement runs on x86_64-linux.
**Project Type**: CLI.
**Performance Goals**: Warm evaluation with IFD allowed is 0.25–1.18 s (research R3). Cold cost is bounded by the existing 300 s per-call budget.
**Constraints**:
- The opt-in relaxes nothing else (FR-004).
- Default behaviour is byte-identical to today (FR-001).
- Network reach is unchanged, apart from what nix itself does when building IFD derivations; the docs state this.
**Scale/Scope**: 7 Nix call sites (research R1), 1 new document row, 3 new corpus targets, 1 dispatch workflow.

## Constitution Check

| Principle | Status | Note |
|---|---|---|
| I Pure Rust, statically linked | Pass | No new dependencies; uses the existing nix subprocess. |
| III Fail closed (trace mode only) / X transparency in scan mode | Pass | An unverified allowance degrades with a reason. A refusal or a build failure is recorded distinctly in C190. The policy and the sandbox are recorded in C217. |
| IV Type-driven correctness | Pass | The `IfdPolicyVerified` enum gates evaluation, and only the preflight constructs it. No new `.unwrap()`. |
| V Native fields first | Pass | The R4 audit found no native construct; C217 is KEEP-NO-NATIVE with that justification. It has parity extractors in all three formats. |
| VII Test isolation | Pass | Everything is tested with a stub `nix`. Live third-party builds happen only in the nightly Linux corpus job, which is disposable and sandboxed. |
| XII External sources | Pass | Provenance recorded (C217). IFD builds are performed by the user's nix, under the user's configured substituters and trust. |
| Security posture | Pass, documented | Off by default. Opt-in only, with the untrusted-flake caution in the help text, the WARN and the docs (FR-005–FR-007, FR-013). |

No violations.

**Re-check after Phase 1:** still passes.

## Project Structure

### Documentation (this feature)

```text
specs/1283-nix-allow-ifd/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── cli-nix-allow-ifd.md
│   └── c217-nix-evaluation-policy.md
├── measurements/          # README.md, ifd-eval/, stage2-*, sbomnix-and-host.txt
└── tasks.md               # /speckit-tasks
```

### Source Code

```text
waybill-cli/src/cli/scan_cmd.rs                          # the flag + ArgGroup; IfdPolicy into both tiers; C217 into the summary
waybill-cli/src/scan_fs/package_db/nix/eval/preflight.rs # verify the requested value; IfdAllowanceVerified; sandbox
waybill-cli/src/scan_fs/package_db/nix/eval/{invoke,result,reason,stderr,licenses}.rs  # policy-driven argv; new reasons
waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs    # closure_argv / attributes_argv take the policy
waybill-cli/src/scan_fs/package_db/nix/declarations/evaluate.rs  # the same
waybill-cli/src/generate/{cyclonedx/metadata,spdx/annotations,spdx/v3_annotations}.rs  # C217
waybill-cli/src/parity/extractors/{mod,cdx,spdx2,spdx3}.rs  # C217
docs/reference/sbom-format-mapping.md                    # C217 row; C190 reasons
docs/user-guide/ (Nix closure section)                   # FR-007
waybill-cli/tests/nix_allow_ifd.rs                       # new
waybill-cli/tests/corpus_harness_195/{manifest,harness,expectations}.rs  # allow_ifd targets + recall proxy
.github/workflows/recall-truth.yml                       # dispatch-only baseline producer
```

## Phasing

1. **Foundation:** `IfdPolicy` / `IfdPolicyVerified`, preflight generalisation, C190 reasons, C217 type, and the parity extractors.
2. **US1:** thread the policy to all 7 sites; flag + `ArgGroup`; grep test.
3. **US2:** help text, both WARNs, user-guide section.
4. **US3:** emit C217 in all three formats.
5. **Corpus:** `allow_ifd` targets, the recall proxy expectation, `recall-truth.yml`, and the baselines produced on Linux (truth, then goldens).
6. **Polish:** CHANGELOG, a real-grype recall check against the proxy, and the gate.
