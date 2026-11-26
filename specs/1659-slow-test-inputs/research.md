# Research: spec 1659

## R1. Does an existing small, validated input carry an OpenVEX reference? (#1725, the open question)

**Observed** on 2026-11-25 by temporarily instrumenting `closure_runtime_scope::us4_drop_scope_excluded_yields_valid_runtime_document`, then reverting it with `cp` and checking with `cmp`:

| format | input bytes | OpenVEX mentions in | out | edit report says removed |
|---|---|---|---|---|
| CycloneDX | 11,435 | 0 | 0 | no; CycloneDX carries VEX inline |
| SPDX 2.3 | 25,021 | 1 | 0 | yes |
| SPDX 3 | 40,285 | 1 | 0 | yes |

`us4` already runs `--drop scope=excluded` on this 40 KB SPDX 3 document. That edit drops components and removes the OpenVEX reference, and the test already runs `spdx3-validate` on the result (`closure_runtime_scope.rs:2337`).

**Decision:** i1454 stops running `spdx3-validate` on the moat output. Its conformance check after a reference-removing edit is already made, on a document 206× smaller, by `us4`. But `us4` checks the reference only implicitly; nothing in it asserts the reference is there or gone. So `us4` gains both assertions (FR-003): the SPDX 3 and SPDX 2.3 inputs carry the reference, and the output doesn't.

**Note:** `us4`'s doc comment says SPDX's OpenVEX sidecar "is not edited". That is still true of the sidecar file itself, but the edit now removes the *reference* to it (#1454). The comment gets one added sentence; nothing else changes.

**Alternatives considered:**
- Generating a new small input: unnecessary, since one exists.
- Moving the moat validation to its own test: that keeps its 24 s locally and about 100 s of Linux CPU, which is what FR-002 removes.

## R2. Mutation for #1725 (FR-007)

**The defect the validator run guards:** reference removal leaving the `SpdxDocument` non-conformant.

**Candidate mutation:** in `edit/spdx3.rs::remove_openvex_references`, drop the `if refs.is_empty() { obj.remove("externalRef") }` block, so the edit leaves `"externalRef": []`.

**Prediction, not yet observed:** `us4` fails, either at the JSON schema check or at `spdx3-validate`. If it's the schema check, a second mutation must show `spdx3-validate` failing while the schema check passes (tasks.md T004), because that is what the moat validator run would add over the schema check. Either way the check that now carries the guard catches it, and the task records which one fired. If neither fires, that removal is not a conformance defect, and the task must find one that is (for example, a locator-less `ExternalRef` left behind) before claiming FR-007.

## R3. #1716: one scan, four guards

**Observed (code):** all four tests run `sbom scan --path <dir holding only a copy of the debug binary> --output <file> --no-deep-hash`, with CycloneDX as the default format.
- `cpe_binary_id.rs:37` builds the command inline.
- `binary_id_enrich.rs:42` and `binary_build_provenance.rs` use a `scan()` helper that takes `binary_path()`, which is `env!("CARGO_BIN_EXE_waybill")`.

**Decision:**
- **Where:** a new test file, `waybill-cli/tests/self_scan_guards.rs`, with one test, `waybill_self_scan_guards`.
- **The scan:** copy the binary, scan once, then evaluate four guard functions, each returning `Result<(), String>`.
- **Messages:** each function's failure text is the original test's assertion message.
- **Failures:** collected, then a single `assert!` lists every failure.
- **Provenance skip:** returns `Ok` with a recorded skip note, printed with `eprintln!` as before.
- **Removed:** the four original tests are deleted from their files. Helpers still used by the other tests in those files stay.

**Rationale:** each guard's input, scan arguments and claim stay the same, and the scan is the cost.

**Alternatives considered:**
- An on-disk cache keyed by the binary's mtime: rejected by FR-006.
- A small committed binary for the provenance guard alone: it still needs a host-format binary, and the shared scan already provides one at no extra cost.

## R4. Mutations for #1716 (FR-007)

Each guard gets a mutation that should make it fail:
- **CPE:** inject a CPE `openssl:openssl` into the parsed SBOM in the test, before evaluation.
- **Version strings:** inject a `pkg:generic/zlib@1.2.13` component.
- **Fingerprints:** inject a `pkg:generic/openssl` component.
- **Provenance:** remove the format-specific property from the file-level component.

The injection mutates the test's view of the output, not the product. That is the point of the check: the evaluation must notice a spurious entry. These are test-side mutations, applied temporarily and reverted with `cp`.

**Two more mutations:**
- Two injected defects together must produce one failure naming both guards (FR-005).
- **A product-side mutation**, to show the guard tests the product and not just itself: add a CPE table entry that matches a string known to be in waybill's binary. The CPE guard must fail on the real scan.

## R5. Timing method

- **Local:** same as spec 1658. Run each affected test alone 3 times with `cargo nextest run -E 'test(=…)'`, idle, after a warm build, and take nextest's per-test duration.
- **CI:** use `ci_steps.py` from spec 1658 for the `Tests` step, and nextest's per-test lines for summed time.
