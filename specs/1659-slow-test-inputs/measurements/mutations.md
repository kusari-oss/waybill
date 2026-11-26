# Mutations (FR-007)

Each mutation is applied with an exact replacement, the named test is run, then the file is reverted with `cp` and checked with `cmp`. The test is `closure_runtime_scope::us4_drop_scope_excluded_yields_valid_runtime_document`, which carries the reference-removal conformance check after T003.

## #1725: what the moat `spdx3-validate` run guarded (T004, research R2)

All mutations are in `waybill-cli/src/edit/spdx3.rs` `remove_openvex_references`. Each leaves something on the `SpdxDocument` where the OpenVEX reference was removed.

| mutation | result | caught by |
|---|---|---|
| M1: keep the empty `externalRef: []` (drop the `is_empty` removal) | PASS | nothing; the empty array is conformant |
| M2: leave a locator-less OpenVEX `ExternalRef` | PASS | nothing; `locator` is optional, so it is conformant |
| M3: leave an `ExternalRef` with `externalRefType: "notAnSpdx3RefType"` | FAIL | JSON schema check (`closure_runtime_scope.rs:2349`) |
| M4: leave an OpenVEX `ExternalRef` with `contentType: "not a media type"` | FAIL | T003's "reference survived" assertion (`:2345`) |
| M5: the same with `contentType: 7` | FAIL | T003's "reference survived" assertion (`:2345`) |
| M6: leave an `ExternalRef` with `externalRefType: "other"` and `contentType: "not a media type"` | FAIL | JSON schema check (`:2349`), whose schema constrains the `contentType` pattern (`spdx-3.0.1.json:1076`) |
| Control: the same with a valid `contentType: "application/json"` | PASS | (the injection itself is conformant) |

**Finding:** on the reference-removal path, every non-conformance produced was caught by the JSON schema check, or by T003's new assertion, before `spdx3-validate` ran. No mutation was found that the schema passes and `spdx3-validate` rejects.

The removal only touches the `SpdxDocument`'s `externalRef[]`. That is within what the JSON schema describes (types, enums, patterns), not the cross-element relations SHACL adds.

So on this path, the moat validator run in i1454 guarded nothing that the schema check, which i1454 keeps, doesn't already catch. `us4` still runs `spdx3-validate` on its small edited document, which costs 1.8 s for the whole test.

## T003 control (test side)

Mutation C1: the input handed to the "carries the reference" control has its references stripped, standing in for a stub that stops emitting the sidecar.

```
        FAIL [   0.745s] (1/1) waybill::closure_runtime_scope us4_drop_scope_excluded_yields_valid_runtime_document
    assertion `left == right` failed: spdx23: the input must reference its OpenVEX sidecar, or nothing below removes one
```

The control fires, so `us4` cannot silently stop exercising the reference-removal path.

## #1716: test-side mutations (T007, research R4)

Each one injects a defect into the parsed SBOM before the guards run (`let mut sbom = scan_waybill_itself(); <inject>`).

### cpe

```
[no openssl:openssl CPE] milestone-097 SC-007 spurious-emission guard: waybill binary should not emit an openssl:openssl CPE (waybill uses rustls). If this fires, check (
1 self-scan guard(s) failed:
FAIL [   4.825s] (1/1) waybill::self_scan_guards waybill_self_scan_guards
```
reverted: cmp ok

### verstr

```
[no spurious version strings] SC-007 false-positive guard: waybill binary should not trip any v1 version-string pattern. Found 1 spurious matches: ["pkg:generic/zlib@1.2.
1 self-scan guard(s) failed:
FAIL [   4.755s] (1/1) waybill::self_scan_guards waybill_self_scan_guards
```
reverted: cmp ok

### fingerprint

```
[no spurious symbol fingerprints] SC-007 false-positive guard: waybill should not trip any v1 symbol fingerprint. Found 1 spurious matches: ["pkg:generic/openssl"]
1 self-scan guard(s) failed:
FAIL [   4.712s] (1/1) waybill::self_scan_guards waybill_self_scan_guards
```
reverted: cmp ok

### provenance

```
[build provenance] Mach-O binary should emit waybill:macho-build-version
1 self-scan guard(s) failed:
FAIL [   4.711s] (1/1) waybill::self_scan_guards waybill_self_scan_guards
```
reverted: cmp ok

### two

```
[no spurious symbol fingerprints] SC-007 false-positive guard: waybill should not trip any v1 symbol fingerprint. Found 1 spurious matches: ["pkg:generic/openssl"]
[no spurious version strings] SC-007 false-positive guard: waybill binary should not trip any v1 version-string pattern. Found 1 spurious matches: ["pkg:generic/zlib@1.2.
2 self-scan guard(s) failed:
FAIL [   4.748s] (1/1) waybill::self_scan_guards waybill_self_scan_guards
```
reverted: cmp ok

## #1716: mutation in waybill itself

**How a CPE reaches a component in a binary scan:** the milestone-096 version-string scanner (`scan_fs/binary/version_strings.rs`) matches an anchored literal such as `OpenSSL ` followed by a version. That yields `pkg:generic/openssl@<v>`, and the milestone-097 CPE table maps the `openssl` slug to `cpe:2.3:a:openssl:openssl:<v>`. This matches the path named in `cpe_binary_id.rs`'s comment.

The debug binary holds no `OpenSSL N.N` string, but it does hold `SPDX 3.0.1` (6 occurrences, found with `strings`). Mutation P1 points the OpenSSL anchor at `SPDX ` (`version_strings.rs:371`, tail offset 8 → 5), so the real scan of waybill should produce an `openssl@3.0.1`.

```
PASS [   4.733s] (1/1) waybill::self_scan_guards waybill_self_scan_guards
```
reverted: cmp ok; rebuilt before the next step.

P1 did **not** fail the test, and that is not evidence about the guard: the scanner requires a NUL byte immediately before an anchor (`main_rule_accepts`, `version_strings.rs:303`), and every `SPDX 3.0.1` in the binary is mid-string (`… SPDX-2.3 and SPDX 3.0.1`).

A scan of the binary for a NUL followed by `Word N.N` finds only `TLS 1.x` (3 occurrences). Mutation P2 points the OpenSSL anchor at `TLS ` (tail offset 8 → 4):

```
PASS [   4.718s] (1/1) waybill::self_scan_guards waybill_self_scan_guards
```
reverted: cmp ok; rebuilt before the next step.

P2 also passed, for a second reason: `parse_openssl_version` requires exactly two dots (`version_strings.rs:1046`), and `TLS 1.3` has one. Mutation P3 keeps the `TLS ` anchor and relaxes that to at least one dot, so the real scan reaches the matcher, the PURL and the CPE table:

```
[no openssl:openssl CPE] milestone-097 SC-007 spurious-emission guard: waybill binary should not emit an openssl:openssl CPE (waybill uses rustls). If this fires, check (a) milestone-096 version-string scanner for a fals
[no spurious version strings] SC-007 false-positive guard: waybill binary should not trip any v1 version-string pattern. Found 1 spurious matches: ["pkg:generic/openssl@1.2"]
2 self-scan guard(s) failed:
FAIL [   4.757s] (1/1) waybill::self_scan_guards waybill_self_scan_guards
```
reverted: cmp ok; rebuilt before the next step.

**Conclusion:** the guards fail when waybill itself produces the defect, not only when it is injected. P1 and P2 passing was a fact about the mutations (the anchor needs a NUL before it, and the parser needs `X.Y.Z`), not about the guards.
