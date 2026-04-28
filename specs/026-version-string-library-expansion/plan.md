---
description: "Implementation plan — milestone 026 version-string library expansion (easy 4)"
status: plan
milestone: 026
---

# Plan: Curated version-string library expansion

## Architecture

Pure additive extension of the existing curated scanner at
`mikebom-cli/src/scan_fs/binary/version_strings.rs`. Same shape as
the established 7 libraries:

1. Prefix-match-then-version-parse, anchored on a NUL boundary
   (no mid-string false positives).
2. Returns `Vec<EmbeddedVersionMatch { library: CuratedLibrary,
   version: String }>`, dedup'd by `(library, version)`.
3. Downstream: `binary/entry.rs::version_match_to_entry` converts
   each match into a `pkg:generic/<library>@<version>`
   `PackageDbEntry` with `evidence_kind = "embedded-version-string"`
   + `confidence = "heuristic"`.

No new types beyond 4 enum variants (`GnuTls`, `LibreSsl`, `Llvm`,
`OpenJdk`). No public API change beyond the enum gaining variants.
No `Cargo.toml` change. No new modules.

## Reuse inventory

These existing items handle the work; this milestone consumes them:

- `version_strings::CuratedLibrary` enum + `slug()` accessor — gain
  4 variants.
- `version_strings::scan(region: &[u8]) -> Vec<EmbeddedVersionMatch>` —
  unchanged; iterates positions and dispatches to `match_prefix`.
- `version_strings::match_prefix` — gains 4 new prefix arms.
- `version_strings::parse_semver_triple` — reused by GnuTLS,
  LibreSSL, and LLVM (all three are clean `X.Y.Z`).
- `version_strings::tests::region(inner: &[u8])` test helper —
  unchanged; reused by all 8+ new tests.
- `binary/entry.rs::version_match_to_entry` — unchanged; auto-
  consumes the 4 new variants because it dispatches on
  `m.library.slug()`.
- `BinaryScan::string_region` — unchanged; already populated for
  ELF / Mach-O / PE.

## Touched files

| File | Change | LOC |
|---|---|---|
| `mikebom-cli/src/scan_fs/binary/version_strings.rs` | + 4 enum variants + 4 prefix arms + `parse_openjdk_version` + 8 tests + TODO block | +180 |
| `docs/design-notes.md` | + "Deferred backlog" entry naming the 3 hard libraries | +20 |

Total Rust source: ~180 LOC in **one file**. Smaller than
023/024/028 (~250-360 LOC). This is the tightest milestone yet.

## Phasing

One or two atomic commits — both options viable:

**Option A: 2 commits (preferred for cleaner history)**

### Commit 1: `026/parsers` (~150 LOC)
- 4 enum variants + slug() arms.
- 4 prefix-match arms in `match_prefix`.
- `parse_openjdk_version` helper covering both schemes.
- 8+ inline tests (positive + negative per library, plus the
  cross-validation `libressl_distinct_from_openssl` test).
- `TODO(milestone-026.x)` doc-comment block at the top of
  `version_strings.rs` listing the 3 deferred libraries.

### Commit 2: `026/deferred-backlog-doc` (~20 LOC)
- New "Curated version-string scanner — hard cohort (deferred from
  milestone 026)" subsection in `docs/design-notes.md`'s "Deferred
  backlog" area. One paragraph per deferred library naming the
  technical blocker.

**Option B: 1 commit (acceptable; milestone is small enough)**

Combine the two — same content, single commit. Since the docs
update is small and tightly bound to the implementation, single-
commit is reasonable. Decide at PR time based on diff readability.

Per FR-009, each commit's `./scripts/pre-pr.sh` is clean.

## Estimated effort

| Phase | Effort | Notes |
|---|---|---|
| Phase 1 (recon + baseline) | done | T001 done in scoping conversation |
| Phase 2 (4 parsers + tests) | 2 hr | Mechanical; reuses existing helpers |
| Phase 3 (TODO + design-notes) | 30 min | Doc-only |
| Phase 4 (verify + PR) | 30 min | Goldens regen → zero diff (verified by SC-007) |
| **Total** | **~3 hr** | Smallest milestone in the recent series. |

## Risks

- **R1: Real-world canonical-string verification.** The spec asserts
  that GnuTLS / LibreSSL / LLVM / OpenJDK each emit their canonical
  form (`GnuTLS X.Y.Z`, `LibreSSL X.Y.Z`, `LLVM version X.Y.Z`,
  `OpenJDK X.Y.Z+B`) into `string_region`. Mitigation: this is
  testable post-implementation by running mikebom against a
  control-set binary that links the library. If any library's
  real-world signature doesn't match the spec'd prefix, the
  prefix gets adjusted; the test surface is small enough that
  iteration cost is low. The unit tests run against synthetic
  byte buffers so they always exercise the implemented prefix
  regardless of what real binaries embed.
- **R2: OpenJDK parser scope creep.** JDK versioning has corner
  cases (early-access `21-ea+11`, milestone builds, vendor-specific
  tags like Corretto's `21.0.1.12.1` or Zulu's `21.0.1+12-LTS`).
  Mitigation: the parser strictly accepts the two documented
  schemes (modern `<M>.<N>.<S>(+<B>)?` and legacy `8u<U>(-b<B>)?`).
  Vendor-tag variants beyond those two are out of scope. If a
  control-set binary turns up an `-LTS` suffix or `21-ea+11`
  early-access form, treat as a follow-on bug-fix.
- **R3: LLVM strict-prefix gate is too strict.** Some lld banners
  use `LLVM X.Y.Z` (no `version`). Mitigation: the spec
  deliberately accepts only `LLVM version X.Y.Z` to keep the
  false-positive surface tight. If real-world data shows the
  shorter form is also valuable, add a second prefix arm in a
  follow-on; the change is one line.

## Constitution alignment

- **Principle I (Pure Rust, Zero C):** no new deps. ✓
- **Principle IV (no `.unwrap()` in production):** new parsers
  return `Option<String>` on every failure path. ✓
- **Principle VI (Three-Crate Architecture):** untouched. ✓
- **Principle IX (Accuracy):** new evidence is at the `heuristic`
  confidence tier (existing convention for the curated scanner)
  — no risk of false-positive promotion to higher tiers. ✓
- **Per-commit verification (lessons from 016-028):** FR-009 enforced.
- **Recon-first discipline:** every claim in the spec backed by
  a file:line reference from the pre-spec investigation
  (`version_strings.rs:53` for `scan`; lines 161-296 for the 5
  existing parsers; line 80 for the boundary check; line 21 for
  `EmbeddedVersionMatch`; `binary/entry.rs:21` for the
  match-to-entry conversion).

## What this milestone does NOT do

- Does not detect glibc / musl / V8 (deferred — see Out of scope
  in spec.md).
- Does not walk new binary sections (no `.gnu.version_r` reader).
- Does not change the `extra_annotations` bag — output flows
  through the existing `pkg:generic` component channel.
- Does not promote any match to higher confidence tiers.
- Does not change CLI args or output flags.
- Does not introduce new format-specific code (the existing
  `string_region` machinery handles ELF / Mach-O / PE identically).
