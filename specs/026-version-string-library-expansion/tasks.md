---
description: "Task list — milestone 026 version-string library expansion (easy 4)"
---

# Tasks: Curated version-string library expansion — Tighter Spec

**Input**: Design documents from `/specs/026-version-string-library-expansion/`
**Prerequisites**: spec.md (✅), plan.md (✅), checklists/requirements.md (✅)

**Tests**: 8+ new inline tests in `version_strings::tests` (positive +
negative per library; plus a `libressl_distinct_from_openssl` cross-
validation test). 27-golden regen produces zero diff.

**Organization**: Single user story (US1, P1). One or two atomic
commits.

## Path Conventions

- Touches `mikebom-cli/src/scan_fs/binary/version_strings.rs`
  (the only Rust source file affected — confirmed by SC-003).
- Touches `docs/design-notes.md` (additive: deferred-backlog entry).
- Does NOT touch `mikebom-common/`, `mikebom-cli/src/cli/`,
  `mikebom-cli/src/resolve/`, `mikebom-cli/src/generate/`,
  `mikebom-cli/src/scan_fs/binary/{elf,macho,pe,scan,entry,mod,linkage,packer,predicates,jdk_collapse,python_collapse}.rs`,
  or any `mikebom-cli/src/scan_fs/package_db/` file.

---

## Phase 1: Setup + baseline

- [X] T001 Recon done. Confirmed:
      - `version_strings.rs` is 452 LOC with 7 working libraries
        (OpenSSL, BoringSSL, zlib, SQLite, curl, PCRE, PCRE2).
      - `parse_semver_triple` (line 240) is reusable for clean
        `X.Y.Z` patterns — fits GnuTLS, LibreSSL, LLVM.
      - `at_boundary` check (line 80) prevents mid-string false
        positives — applies uniformly to all new prefixes.
      - `match_prefix`'s "PCRE2 before PCRE" precedence (lines 136-154)
        documents how to handle prefix-overlap (LibreSSL vs OpenSSL
        don't overlap so this doesn't apply, but the pattern is
        established).
      - `entry.rs::version_match_to_entry` (line 21) auto-consumes
        new variants via `slug()` — no downstream changes needed.
- [ ] T002 Snapshot baseline: `./scripts/pre-pr.sh 2>&1 | tee /tmp/baseline-026.txt | grep -cE '^test [a-z_:]+ \.\.\. ok' > /tmp/baseline-026-count.txt`.

---

## Phase 2: Commit 1 — `026/parsers`

**Goal**: 4 new libraries detected by `version_strings::scan`; new
inline tests; TODO marker for the deferred 3.

- [ ] T003 [US1] Edit `version_strings.rs::CuratedLibrary` enum: add
      4 new variants in alphabetical order with the existing 7:
      `BoringSsl, Curl, GnuTls, LibreSsl, Llvm, OpenJdk, OpenSsl,
      Pcre, Pcre2, Sqlite, Zlib`. (Existing order: OpenSsl,
      BoringSsl, Zlib, Sqlite, Curl, Pcre, Pcre2 — append the 4
      new at the end to minimize diff churn.)
- [ ] T004 [US1] Edit `slug()` impl: add 4 arms returning the
      lowercase library name:
      - `GnuTls => "gnutls"`
      - `LibreSsl => "libressl"`
      - `Llvm => "llvm"`
      - `OpenJdk => "openjdk"`
- [ ] T005 [US1] Edit `match_prefix`: add 4 new prefix arms after
      the existing 7. Each follows the established `if window.starts_with(...)`
      → `parse_*` → `Some(EmbeddedVersionMatch { ... })` pattern:
      ```rust
      // GnuTLS — "GnuTLS "
      if window.starts_with(b"GnuTLS ") {
          let tail = &region[pos + 7..];
          if let Some(v) = parse_semver_triple(tail) {
              return Some(EmbeddedVersionMatch {
                  library: CuratedLibrary::GnuTls,
                  version: v,
              });
          }
      }
      // LibreSSL — "LibreSSL "
      if window.starts_with(b"LibreSSL ") {
          let tail = &region[pos + 9..];
          if let Some(v) = parse_semver_triple(tail) {
              return Some(EmbeddedVersionMatch {
                  library: CuratedLibrary::LibreSsl,
                  version: v,
              });
          }
      }
      // LLVM — "LLVM version "  (strict prefix; bare "LLVM " is too noisy)
      if window.starts_with(b"LLVM version ") {
          let tail = &region[pos + 13..];
          if let Some(v) = parse_semver_triple(tail) {
              return Some(EmbeddedVersionMatch {
                  library: CuratedLibrary::Llvm,
                  version: v,
              });
          }
      }
      // OpenJDK — "OpenJDK "  (modern X.Y.Z+B or legacy 8uX-bY)
      if window.starts_with(b"OpenJDK ") {
          let tail = &region[pos + 8..];
          if let Some(v) = parse_openjdk_version(tail) {
              return Some(EmbeddedVersionMatch {
                  library: CuratedLibrary::OpenJdk,
                  version: v,
              });
          }
      }
      ```
- [ ] T006 [US1] Add `parse_openjdk_version(tail: &[u8]) -> Option<String>`
      after the existing parsers. Two-scheme accepting:
      - **Modern**: `<digits>.<digits>.<digits>(+<digits>)?`
      - **Legacy**: `8u<digits>(-b<digits>)?`
      - Falls through to None on anything else.
      - Returns the full matched string verbatim (preserves
        `+12` / `-b09` suffix).
      - Terminates on whitespace / NUL / non-version-char.
      - Tries modern first; if `dots != 2`, tries legacy.
- [ ] T007 [US1] Add 8+ new tests in the existing `#[cfg(test)] mod tests`:
      - `gnutls_positive` — `region(b"GnuTLS 3.7.10")` → 1 match
        `{ GnuTls, "3.7.10" }`.
      - `gnutls_no_version_no_match` — `region(b"GnuTLS\0")` → 0 matches.
      - `libressl_positive` — `region(b"LibreSSL 3.8.2")` → 1 match.
      - `libressl_distinct_from_openssl` — region with both
        `LibreSSL 3.8.2` AND `OpenSSL 3.0.11 19 Sep 2023`
        signatures → exactly 2 matches (one of each, no double-emit).
      - `llvm_positive` — `region(b"LLVM version 17.0.6")` → 1 match.
      - `llvm_bare_name_does_not_match` — `region(b"LLVM ERROR: foo")`
        AND mid-string `\0Compiled by LLVM 17.0.0.\0` → 0 matches.
      - `openjdk_modern_version_with_build` —
        `region(b"OpenJDK 21.0.1+12")` → 1 match,
        version `"21.0.1+12"`.
      - `openjdk_legacy_8u_version` — `region(b"OpenJDK 8u362-b09")`
        → 1 match, version `"8u362-b09"`.
      - (Optional bonus: `openjdk_modern_no_build_suffix` —
        `region(b"OpenJDK 17.0.5")` → 1 match, version `"17.0.5"`.)
- [ ] T008 [US1] Add a doc-comment block at the top of
      `version_strings.rs` (between the existing module doc and the
      first `pub` declaration) marking the deferred libraries:
      ```rust
      // TODO(milestone-026.x): three additional libraries deferred
      // from milestone 026 because they don't have clean self-
      // identifying strings in the read-only string region:
      //   - glibc:  GLIBC_X.Y lives in `.gnu.version_r`, not `.rodata`.
      //             Needs a separate ELF-section reader.
      //   - musl:   typically doesn't self-identify in compiled
      //             output. Research milestone needed to find a
      //             reliable signature.
      //   - V8:     version strings buried in stack-trace formatting
      //             code; non-deterministic across builds.
      // Tracking: see docs/design-notes.md "Deferred backlog".
      ```
- [ ] T009 [US1] Verify: `cargo +stable test -p mikebom --bin mikebom
      scan_fs::binary::version_strings` includes the 8+ new tests
      and they pass. `./scripts/pre-pr.sh` clean.
- [ ] T010 [US1] Commit: `feat(026/parsers): add GnuTLS, LibreSSL, LLVM, OpenJDK to curated version-string scanner`.

---

## Phase 3: Commit 2 — `026/deferred-backlog-doc`

**Goal**: persistent reference to the 3 deferred libraries.

- [ ] T011 [US1] Edit `docs/design-notes.md`: add a new subsection
      titled `### Curated version-string scanner — hard cohort
      (deferred from milestone 026)` under the "Deferred backlog"
      heading. One paragraph per library:
      - **glibc**: GLIBC_X.Y symbol-version markers live in the
        `.gnu.version_r` section. Detection requires an ELF-
        section reader different from the curated string scanner.
      - **musl**: rarely self-identifies in compiled output; the
        `__libc_get_version()` path is rarely exercised in static
        binaries. Research milestone needed.
      - **V8**: version strings buried in stack-trace formatting
        code; tend to be non-deterministic across builds. May
        require an inline-data scan rather than a string scan.
- [ ] T012 [US1] Verify: `./scripts/pre-pr.sh` clean.
- [ ] T013 [US1] Commit: `docs(026): record deferred-backlog entry for glibc/musl/V8 (hard cohort)`.

---

## Phase 4: Verification

- [ ] T014 SC-001 verification: `./scripts/pre-pr.sh` clean.
- [ ] T015 SC-002 verification: 8+ new tests in
      `version_strings::tests`; ≥1 positive + ≥1 negative per
      library.
- [ ] T016 SC-003 verification: `git diff main..HEAD --
      mikebom-cli/src/scan_fs/binary/{elf,macho,pe,scan,entry,mod,linkage,packer,predicates,jdk_collapse,python_collapse}.rs`
      empty.
- [ ] T017 SC-004 verification: `wc -l mikebom-cli/src/scan_fs/binary/version_strings.rs`
      ≤ 700.
- [ ] T018 SC-005 verification: `git diff main..HEAD --
      mikebom-common/ mikebom-cli/src/cli/ mikebom-cli/src/resolve/
      mikebom-cli/src/generate/ mikebom-cli/src/scan_fs/package_db/`
      empty.
- [ ] T019 SC-007 verification: 27-golden regen
      (`MIKEBOM_UPDATE_*_GOLDENS=1`) produces zero diff.
- [ ] T020 SC-008 verification: `grep -n "TODO(milestone-026.x)"
      mikebom-cli/src/scan_fs/binary/version_strings.rs` finds the
      block.
- [ ] T021 Push branch; observe all 3 CI lanes green (SC-006).
- [ ] T022 Author the PR description: 2-commit summary, library
      coverage delta (7 → 11), deferred-backlog pointer,
      byte-identity attestation.

---

## Dependency graph

```text
T001-T002 (recon + baseline, recon done)
   │
   ↓
T003-T010 [Commit 1: parsers + 8+ tests + TODO]
   │
   ↓
T011-T013 [Commit 2: design-notes deferred-backlog]
   │
   ↓
T014-T022 (verification + PR)
```

## Estimated effort

| Phase | Effort | Notes |
|---|---|---|
| Phase 1 (baseline) | 5 min | T001 done; just snapshot |
| Phase 2 (parsers + tests) | 2 hr | Mechanical extension; 4 prefix arms + 1 new parser + 8 tests |
| Phase 3 (deferred-backlog doc) | 30 min | One subsection in design-notes |
| Phase 4 (verify + PR) | 30 min | Goldens regen + CI watch |
| **Total** | **~3 hr** | Tightest milestone in the recent series. |
