# Implementation Plan: Rpm FILEDIGESTS Cross-Reference

**Branch**: `041-rpm-filedigests` | **Date**: 2026-04-29 | **Spec**: [spec.md](spec.md)

## Summary

Mirror milestone 040 US2 (apk SHA-1 cross-ref) for rpm. The rpm
HeaderBlob already carries per-file digests inline via the
FILEDIGESTS (1035) and FILEDIGESTALGO (5011) tags; mikebom's
existing `RpmHeader::string_array` and `int32_array` accessors
expose them with no new parsing infrastructure. The work is:

1. Extend `RpmHeader` with a `file_digests()` accessor that
   returns the per-file digests + algorithm code.
2. Thread the digests through `iter_rpmdb`'s visitor signature so
   `read_file_lists` can return them alongside the path list.
3. Add a new optional `rpm_file_digest: Option<String>` field on
   `mikebom_common::resolution::FileOccurrence`.
4. Surface the value in CDX `additionalContext` under the key
   `rpm_filedigest`.

Total ~200 LOC across 5 files; single PR with 3 atomic commits.

## Technical Context

**Language**: Rust stable.
**Primary Dependencies**: existing only. No new top-level deps.
**Project Type**: CLI / library (three-crate workspace).
**Performance**: zero impact — FILEDIGESTS extraction reuses the
HeaderBlob walk that already happens for BASENAMES / DIRNAMES /
DIRINDEXES. Per-file emission is a String clone.
**Constraints**: Constitution Principles I, IV, V (output schema
unchanged at the CDX/SPDX wire level — `additionalContext` is a
JSON-string opaque to the spec), VI (three-crate preserved).

## Constitution Check

| Principle | Status |
|---|---|
| I. Pure Rust, Zero C | ✅ no new deps |
| IV. Type-Driven Correctness | ✅ optional field; algorithm-prefixed string preserves provenance |
| V. Specification Compliance | ✅ wire shape unchanged; new key carried opaquely in `additionalContext` |
| VI. Three-Crate Architecture | ✅ touches mikebom-cli + one additive field on mikebom-common |
| VIII. Completeness | ✅ closes a known false-negative on cross-ref symmetry |
| X. Transparency | ✅ rpm_filedigest is annotated upstream-provenance metadata |

**No constitution violations.**

## Touched files

| File | Change | LOC |
|---|---|---|
| `mikebom-cli/src/scan_fs/package_db/rpmdb_sqlite/rpm_header.rs` | + TAG_FILEDIGESTS / TAG_FILEDIGESTALGO consts; + `file_digests()` accessor + algo-code → name mapper | +60 |
| `mikebom-cli/src/scan_fs/package_db/rpm.rs` | + `iter_rpmdb` visitor signature gains a 3rd arg (digests); + `read_file_lists` returns `Vec<RpmFileEntry>` not `Vec<String>` | +80 |
| `mikebom-cli/src/scan_fs/package_db/file_hashes.rs` | thread digest into per-file emission; update `hash_rpm_package_files` signature; tests | +50 |
| `mikebom-cli/src/scan_fs/mod.rs` | mechanical signature update at the call site | +5 |
| `mikebom-cli/src/generate/cyclonedx/evidence.rs` | + `rpm_filedigest` key emission alongside `sha1` | +5 |
| `mikebom-common/src/resolution.rs` | + `rpm_file_digest: Option<String>` field on `FileOccurrence` | +6 |
| `mikebom-cli/tests/oci_registry_smoke.rs` | OPTIONAL: add gated fedora smoke test that asserts the new key | +60 |

Total ~265 LOC.

## Phasing

Three commits.

1. **`feat(041/extract-filedigests)`** — Add tag consts +
   `RpmHeader::file_digests` accessor; thread through
   `iter_rpmdb` + `read_file_lists`. New `RpmFileEntry` struct.
   Behind dead-code allow for the wire-up gap.
2. **`feat(041/thread-and-emit)`** — `FileOccurrence` field;
   `hash_rpm_package_files` signature change; emitter clause.
   Smoke-test extension (optional gated test).
3. **`docs(041)`** — User-guide rpm paragraph update; CHANGELOG.

## Risks

- **R1 (algorithm registry)**: rpm uses IANA hash-algorithm
  numbers per RFC 4880-ish convention. The set
  `{1: md5, 2: sha1, 8: sha256, 9: sha384, 10: sha512}` is well-
  documented. SHA-3 family codes (11+) aren't typically used by
  rpm yet — silently skip the cross-ref for unknown codes.
- **R2 (FILEDIGESTS array length)**: should always match
  BASENAMES per the rpm spec. If they differ (damaged
  HeaderBlob), use `min(len)` and skip the over-extension —
  matches the `iter_rpmdb` posture for DIRINDEXES out-of-range
  entries.

## Constitution alignment

All twelve principles green. No new C deps; no `.unwrap()` in
production paths; three-crate preserved; spec compliance
preserved (wire format unchanged at the CycloneDX / SPDX level).
