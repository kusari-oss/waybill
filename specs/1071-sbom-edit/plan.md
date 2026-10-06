# Implementation Plan: Edit an emitted SBOM — filter, redact, derivation record, signature chain

**Branch**: `1071-sbom-edit` | **Date**: 2026-10-06 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/1071-sbom-edit/spec.md`

## Summary

Two new commands:
- **`waybill sbom edit`** reads a CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1 document and applies ordered operations: drop components by selector, drop annotations by namespace, and redact paths, hosts or names. It writes the same format, with a derivation record and an optional signature.
- **`waybill sbom verify-chain`** checks a derivative's signature and each link back to its originals.

**How the edit works.** It operates on each format's own JSON through per-format adapters, never through a neutral model (research R2). waybill's sorted-key, 2-space output then keeps untouched content byte-identical. The parity extractors keep the three formats in agreement.

**Validated choices.**
- The derivation link uses each format's native vocabulary, measured as accepted by its validator: CycloneDX `bom` external reference, SPDX 2.3 `AMENDS`, SPDX 3 `amendedBy` (R1).
- The original's signature is embedded in the record (R7).

**Two post-condition checks** make the command fail closed. Dropped identifiers must be absent and the output must conform. Redacted values must be absent.

Re-identification and policy files are the next milestone. The `EditOp` type is their shared vocabulary.

## Technical Context

**Language/Version**: Rust stable, workspace toolchain pinned by `rust-toolchain.toml`. No nightly; `waybill-ebpf` untouched.
**Primary Dependencies**: Existing only:
- `serde` / `serde_json`, `clap`, `globset` (selector globs), `regex`;
- `sha2` + `data-encoding` (hashes, base32);
- the existing signer (`W/sbom/signer.rs`) and DSSE verifier (`W/attestation/verifier.rs`);
- the parity extractors (tests, and envelope parsing);
- `tracing`, `anyhow` / `thiserror`.

`hmac 0.12.1` is promoted from transitive to direct (it's already in `Cargo.lock` through sigstore). **No new crates in the lockfile.** Test and validation: the bundled CycloneDX / SPDX 2.3 schemas, `jsonschema` (dev), and `spdx3-validate` 0.0.5.
**Storage**: None. File in, file out; no state between runs, unlike bomctl's cache.
**Testing**:
- **Adapter unit tests** per format: selection, drop and bridge, completeness, annotation removal, redaction and pseudonyms, the derivation record.
- **Integration tests** on the three outputs of one fixture scan:
  - conformance;
  - no dangling references;
  - agreement under the parity extractors;
  - the redaction search;
  - the SC-002 diff;
  - chain verification, and the tamper matrix (SC-004).
- **A public-corpus edit** for SC-001.
- **The SC-006 measurement.**
- `./scripts/pre-pr.sh`.

**Target Platform**: All. Pure userspace, with no eBPF.
**Project Type**: CLI (`waybill-cli`).
**Performance Goals**: SC-006, a ratio to generation time, measured on `image-postgres16` (R9).
**Constraints**:
- byte-identical untouched content for waybill documents (FR-002);
- fail closed on post-conditions;
- no value in the derivation record;
- the pseudonym key is never written;
- no new lockfile crate.

**Scale/Scope**:
- new `W/edit/`: `mod.rs` (the `EditOp` / `Selector` model, the pipeline, post-conditions), `select.rs`, `cdx.rs`, `spdx23.rs`, `spdx3.rs`, `redact.rs` (collection, rewriting, HMAC), `derivation.rs` (the record and native links);
- new `W/cli/edit.rs` and `W/cli/verify_chain.rs`, wired into `W/cli/sbom_cmd.rs`;
- the `W/sbom/signer.rs` reuse (strip and re-sign), plus a small JSF verify helper lifted from its tests;
- catalogue row C194 plus its parity extractors;
- `docs/` (a user guide for editing, including what redaction does not hide) and `CHANGELOG.md`.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Pure Rust | ✅ No new crate. `hmac` is promoted from transitive. |
| II / III (trace mode only) | ✅ Not applicable: this command neither observes builds nor resolves dependencies. Fail-closed still holds for its own guarantees: post-conditions refuse to write a document that leaks or dangles. |
| IV. Type-driven | ✅ `EditOp`, `Selector`, `RedactMode` and the `ChainReport` enums are typed. No `unwrap` in production paths. |
| V. Native fields first | ✅ Derivation uses CycloneDX `bom` references, SPDX 2.3 `AMENDS` and SPDX 3 `amendedBy`, each measured against its validator (R1). Completeness uses CycloneDX compositions and SPDX 3 `completeness`. The `waybill:derivation` annotation carries only what no format has: operation categories, embedded original signature, ancestors. |
| VI. Three-crate architecture | ✅ A new module in `waybill-cli`; no new crate. |
| VII. Test isolation | ✅ Fixtures and goldens; validators at test time only. |
| IX. Accuracy | ✅ Completeness is downgraded where graphs change. Bridged edges keep scope by a stated rule. Post-conditions verify the claims the output makes. |
| X. Transparency | ✅ Every derivative states it is one, what categories were changed and by how much, and its original's hash and signature. `verify-chain` distinguishes verified, delegated and unavailable. The docs state what redaction doesn't hide. |
| Measurement rule | ✅ The native links and #1147 were measured (`measurements/derivation_probe.*`). The byte-identity premise was measured. The SC-006 baseline is a task, stated as a ratio. |

No violations.

## Project Structure

### Documentation (this feature)

```text
specs/1071-sbom-edit/
├── spec.md · plan.md · research.md · data-model.md · quickstart.md
├── contracts/cli.md
├── measurements/derivation_probe.{py,txt}
└── checklists/requirements.md
```

### Source Code (repository root)

```text
waybill-cli/src/
├── edit/
│   ├── mod.rs         # EditOp, Selector, pipeline, post-conditions, report
│   ├── select.rs      # selector parsing + matching (R3)
│   ├── cdx.rs         # CycloneDX 1.6 adapter
│   ├── spdx23.rs      # SPDX 2.3 adapter
│   ├── spdx3.rs       # SPDX 3.0.1 adapter
│   ├── redact.rs      # collection, everywhere-rewrite, HMAC pseudonyms (R5)
│   └── derivation.rs  # record, native links, embedded original signature (R1, R7)
├── cli/edit.rs        # `waybill sbom edit`
├── cli/verify_chain.rs# `waybill sbom verify-chain`
└── parity/extractors/ # C194
waybill-cli/tests/sbom_edit_*.rs
docs/user-guide/sbom-edit.md
```

**Structure Decision**: one new module in `waybill-cli`, using the per-format adapter pattern that the emitters and parity extractors already use.

## Phasing

1. **The model and the pipeline:** `EditOp`, `Selector`, the adapter trait, format detection, post-conditions, report. Unit-tested on in-memory documents.
2. **US1, filter:** drop, bridge, completeness, annotation removal. Three adapters, then the cross-format and SC-001 / SC-002 tests.
3. **US2, redact:** collection, rewriting, modes, HMAC, the fail-closed search, then the SC-003 / SC-008 tests.
4. **US3, chain:** the derivation record, native links, embedded original signature, re-signing, then `verify-chain` and the tamper matrix (SC-004 / SC-005), plus C194.
5. **SC-006 measurement, docs, CHANGELOG.**

## Complexity Tracking

None.
