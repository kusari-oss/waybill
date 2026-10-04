# Implementation Plan: Closure SBOMs for Nix system-configuration flakes

**Branch**: `1066-nix-system-config-closure` | **Date**: 2026-10-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1066-nix-system-config-closure/spec.md`

## Summary

The m1035 closure tier will also evaluate system configurations:

- **Full paths:** `--nix-closure-attr` accepts a full output path, recognised lexically by a standard output name as its first segment (R1).
- **Auto-selection:** with no flag and no `packages.<system>.default`, the tier lists `darwinConfigurations` and `nixosConfigurations`. Exactly one configuration is evaluated at its system output; several degrade with a new, log-only reason that lists them (R2, R5).
- **Everything else unchanged:** evaluation, classification, emission and annotations stay as they are. C184 `attribute` carries the full path when one was used (R7).
- **Safety:** full paths and listed names pass the existing attribute-name safety check (R6).

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only: `serde_json` (listings), `tracing`, and the m1034/m1035 `eval::invoke` helpers (`run_bounded`, `argv_is_safe`, `is_safe_attribute_name`). External: the host's `nix`, already required by the tier. **Zero new Cargo dependencies.**
**Storage**: N/A.
**Testing**: Pure unit tests for the classifier, selection and path builder. Integration tests against synthetic, input-free fixture flakes, skipped when `nix` is absent (the `nix_eval_tier.rs` pattern). `./scripts/pre-pr.sh`; read-only corpus run (byte-identical).
**Target Platform**: Any host with `nix`. A configuration is evaluated for its own platform (R3).
**Project Type**: CLI (`waybill-cli`).
**Performance Goals**: Within the existing 300 s budget. Measured worst case: 11.8 s cold (minimal NixOS); 1.24 s warm (real darwin system, 5,325 derivations).
**Constraints**: FR-009 byte-identity for package closures; no new flag (FR-010); no host property in selection (FR-003).
**Scale/Scope**: `nix/closure/mod.rs` (selection, argv), `nix/eval/reason.rs` (new reason), `cli/scan_cmd.rs` (flag help, `ClosureConfig` construction), the C184 catalogue text, CHANGELOG, about 3 fixture flakes, and 1 integration test file.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust | ✅ No new crates; `nix` stays an external, opt-in tool. |
| III. Fail Closed | ➖ Trace-mode principle (#987). Scan-mode degradations stay as today. |
| VII. Test isolation | ✅ Fixture flakes have no inputs and need no network; tests skip without `nix`. |
| IX. Accuracy | ✅ No guessing: a configuration is chosen only when it is the only one, and the hostname is never read (FR-003). |
| X / XII.3 Transparency | ✅ Every closure degradation, the new reason included, is recorded in the document as C190 (FR-012). This closes the pre-existing log-only gap (#1115), which analysis found the new reason would otherwise have widened. |
| XII. External data sources | ✅ Evaluate-only, IFD refused (verification of that refusal is #1114). |
| Measurement rule (CLAUDE.md) | ✅ Every cost figure is measured (`measurements/`); cold cost included. |

No violations.

## Project Structure

### Documentation (this feature)

```text
specs/1066-nix-system-config-closure/
├── spec.md
├── plan.md            # this file
├── research.md        # R1–R8
├── data-model.md
├── quickstart.md
├── contracts/attribute-selection.md
├── measurements/      # probe + README (real darwin system, minimal NixOS / nix-darwin)
└── checklists/requirements.md
```

### Source Code

```text
waybill-cli/src/scan_fs/package_db/nix/
├── closure/mod.rs     # AttrRequest, classify_attr (R1), select (R2),
│                      # SystemConfiguration::system_path, resolve() branches
└── eval/reason.rs     # DegradationReason::AmbiguousSystemConfiguration
waybill-cli/src/cli/scan_cmd.rs                       # ClosureConfig::from_flags(Option); flag help (FR-007); C190 value (FR-012)
waybill-cli/src/generate/                             # carry C190 to the three emitters, as C180
waybill-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs  # C190 extractor row
docs/reference/sbom-format-mapping.md                 # C184 attribute text (FR-006/007); new C190 row (FR-012)
CHANGELOG.md
waybill-cli/tests/fixtures/nix_config_closure/{one_darwin,two_configs,package_and_config}/flake.nix
waybill-cli/tests/nix_config_closure.rs
```

**Structure Decision**: confined to the closure tier. No emitter or classification changes.

## Delivery

One PR. The three stories share the selection function and its tests.

## Complexity Tracking

None.
