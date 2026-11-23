# Implementation Plan: one library, one component, when the curated scanner and an installed-library record name or version it differently

**Branch**: `1645-split-library-identity` | **Date**: 2026-11-22 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/1645-split-library-identity/spec.md`. Refs #1662, #1526, #1531, #1543, #1641.

## Summary

TheRock reports ncurses and LLVM twice each, because the curated banner match and the installed-library record produce different PURLs for one library file. The plan fixes the cause of each split where it lies.

1. **LLVM (FR-002, R4)**: the curated scanner keeps the suffix the banner states (`23.0.0git`). Its PURL then equals the CMake record's, and the existing PURL merge applies.
2. **ncurses version (FR-013, R5)**: the curated scanner keeps the patch date (`6.6.20251230`). The versions then agree by equality.
3. **ncurses name (FR-003 to FR-006, R1, R3)**: one new pass, `reconcile`, runs after the installed-library entries are spliced in. A curated entry and a record become one component when:
   - the record's own library files include the curated file;
   - the record's name is on the curated library's closed list of names (ncurses: `ncursesw`, `ncursest`, `ncursestw`, sourced from ncurses `INSTALL`);
   - the versions agree at a `.` boundary;
   - exactly one record qualifies.

   The pass rewrites the PURLs (and the `depends` that name the old one, R2), and the deduplicator does the merge.
4. **ncurses CPE (FR-008, R6)**: `X.Y.YYYYMMDD` is split into the CPE version `X.Y` and update `YYYYMMDD`, NVD's form, for every `pkg:generic/ncurses` component.

The census comes first (R8). A log-only commit reports every shared file, version change and CPE change on TheRock, the quality corpus and the public corpus.

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).
**Primary Dependencies**: existing only. `tracing` for the census, `serde_json` for the event values, `waybill_common::types::purl::Purl`, and test-only `strum::EnumIter`. **Zero new Cargo dependencies.**
**Storage**: N/A. In process, per scan.
**Testing**: `./scripts/pre-pr.sh` through `scripts/gate.sh`. The tests:
- unit tests in `version_strings.rs`, `cpe.rs` and the new `library_identity.rs`;
- a new integration file, `waybill-cli/tests/library_identity.rs`, reusing `tests/common/installed_lib.rs::elf_shared_lib`;
- mutations M1–M8 (R9).

**Target Platform**: every host; the change is format- and path-level.
**Project Type**: CLI (`waybill sbom scan`).
**Performance Goals**: SC-007, TheRock within 1.05× `main` (median of ≥3 interleaved runs per arm). The pass is linear in entries (R10).
**Constraints**:
- no new annotation, row or field (R7);
- byte-identical output outside FR-011's three counted classes.

**Scale/Scope**:
- two parsers (`version_strings.rs`);
- one name list and one slug lookup (`CuratedLibrary`);
- one new pass (`scan_fs/library_identity.rs`) and its call site (`scan_fs/mod.rs`, after the installed-library splice at ~line 696);
- one CPE builder and one branch (`generate/cpe.rs`);
- one new test file;
- B-1's golden regenerated;
- the fixture repository's `census/versions.py` extended.

## Constitution Check

*GATE: must pass before Phase 0 research, and again after Phase 1 design.*

| Principle | Check | Status |
|---|---|---|
| I. Pure Rust, statically linked | No new dependency. | Pass |
| III. Fail closed | Ambiguous, disagreeing or unrelated records are never merged (FR-006, FR-009). A name outside the closed list stays separate. | Pass |
| IV. Type-driven correctness | `names()` and `from_slug()` are exhaustive matches with no wildcard. The verdict is an enum. | Pass |
| V. Specification compliance | The CPE form follows NVD's measured convention (R6). No new `waybill:*` row; the original name stays in C18 and C56 (R7). | Pass |
| VII. Test isolation | Synthetic fixtures in tempdirs; `waybill-fixture-*` names wherever the name is not the identity under test. | Pass |
| VIII. Completeness | A merge keeps the union of files and evidence; no edge is left dangling (R2). | Pass |
| IX. Accuracy | The versions are the ones the binaries state (FR-002, FR-013). The merge needs a shared file, a sourced name and agreeing versions. | Pass |
| X. Transparency | The census reports every verdict before anything is wired. C56 shows the merge. | Pass |

No violations, so there is no Complexity Tracking entry. Re-checked after Phase 1: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1645-split-library-identity/
├── spec.md
├── plan.md            # this file
├── research.md        # R1–R10
├── data-model.md
├── quickstart.md
├── contracts/wire.md  # census events, cases
├── measurements/      # NVD probe and output; census, control, corpus, therock, gate, mutations
└── tasks.md           # /speckit-tasks
```

### Source Code

```text
waybill-cli/src/
├── scan_fs/binary/version_strings.rs   # parse_llvm_version (R4); parse_ncurses_version keeps the date (R5);
│                                       # CuratedLibrary::names, ::from_slug (R3); census events for version changes
├── scan_fs/library_identity.rs         # NEW: reconcile() + shared_file census (R1, R2, R8)
├── scan_fs/mod.rs                      # call reconcile after the installed-library splice
└── generate/cpe.rs                     # format_cpe_with_update; the ncurses split (R6); cpe_form_change census
waybill-cli/tests/
├── library_identity.rs                 # NEW: M-1..M-3, N-1..N-4, E-1, C-1, C-2, P-1 (contracts/wire.md)
├── version_string_boundary.rs          # NCURSES_V → 6.6.20251230
└── fixtures/version_string_boundary/b1-golden/   # regenerated (ncurses version and CPE only)
```

Fixture repository (`kusari-sandbox/waybill-fixture-therock-sdk`): `census/versions.py` parses `shared_file`, `curated_version_change` and `cpe_form_change`.

## Order of work

1. **Log-only (R8)**: `names`/`from_slug`; `reconcile` computes verdicts and logs `shared_file` but rewrites nothing; the parsers compute the new versions and log `curated_version_change` but emit the old; `cpe.rs` logs `cpe_form_change`. Gate. Output must equal `main` (public corpus 166 of 166).
2. **Census**: TheRock and the quality corpus through `census-versions.yml` (`--ref main`, full SHAs, `refs.txt` verified); the public corpus with `-f branch=`. Checkpoint: amend the spec if a count contradicts FR-002 to FR-013.
3. **Wire** FR-002, FR-013, FR-008, then `reconcile`'s rewrite. Regenerate B-1 and diff it.
4. **Mutations** M1–M8.
5. **Trace**: the public corpus (expected: `nix-closure-openai`'s ncurses CPE); TheRock (`measure-full` against `main`): ncurses and llvm become one component each; parity-check; timing.
6. **Docs**: `docs/reference/binary-identity.md` (LLVM suffix, ncurses date, the merge rule) and the CHANGELOG.

## Complexity Tracking

None.
