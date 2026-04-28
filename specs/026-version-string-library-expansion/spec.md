---
description: "Expand the curated embedded-version-string scanner to cover GnuTLS, LibreSSL, LLVM, and OpenJDK. Defers glibc / musl / V8 to a research-and-attempt follow-on."
status: spec
milestone: 026
---

# Spec: Curated version-string library expansion (easy-4 cohort)

## Background

mikebom already has a working curated embedded-version-string scanner
at `mikebom-cli/src/scan_fs/binary/version_strings.rs:53` (`pub fn
scan(region: &[u8]) -> Vec<EmbeddedVersionMatch>`). It detects 7
self-identifying libraries via prefix-match-then-version-parse:
**OpenSSL, BoringSSL, zlib, SQLite, curl, PCRE, PCRE2**. Each match
becomes a `pkg:generic/<library>@<version>` `PackageDbEntry` with
`evidence_kind = "embedded-version-string"` + `confidence =
"heuristic"` (`binary/entry.rs::version_match_to_entry`). The scan
runs against the same format-appropriate read-only string region the
binary scanner already collects (`scan.rs::collect_string_region` —
ELF `.rodata` + `.data.rel.ro`, Mach-O `__TEXT,__cstring` +
`__TEXT,__const`, PE `.rdata`).

Four high-value libraries with **clean self-identifying signatures
in `string_region`** are not yet covered:

- **GnuTLS** — emits `GnuTLS <semver-triple>` in initialization /
  banner code. Canonical reference is `gnutls_check_version()` and
  the GnuTLS init log line. Used by curl (when built against
  GnuTLS instead of OpenSSL), wget, GnuPG, many GNU-stack tools.
- **LibreSSL** — emits `LibreSSL <semver-triple>` in the OpenSSL-
  API-compatibility banner. Used by macOS system tools (system curl
  was LibreSSL-backed for years), OpenBSD-derived utilities,
  HTTPie's `requests` build options.
- **LLVM** — emits `LLVM version <semver-triple>` in the canonical
  driver banner (the form `clang --version` / `opt --version`
  print). Useful for clang-based binaries where the embedded LLVM
  version answers "what middle-end pipeline did this binary go
  through" — relevant for security advisories on LLVM IR-level
  vulnerabilities (e.g., LLVM/Clang CVEs).
- **OpenJDK** — emits `OpenJDK <jep-322-version>` in HotSpot banner
  strings. Two schemes coexist: post-Java-9 `<major>.<minor>.<security>[+<build>]`
  (e.g. `21.0.1+12`, `17.0.5`) and legacy Java-8 `8u<update>-b<build>`
  (e.g. `8u362-b09`). Both are ubiquitous in HotSpot-derived JVMs
  and answer "which Java runtime is statically linked into this
  bundled binary".

Three additional libraries the user originally mentioned —
**glibc / musl / V8** — are **deferred to a follow-on milestone**
because they don't have clean self-identifying strings in the
binary's read-only string region. glibc's version (`GLIBC_X.Y`)
lives in the `.gnu.version_r` section (symbol-version table), not
in `.rodata` — would require a new ELF-section reader. musl
typically doesn't self-identify in compiled output. V8's version
strings are buried in stack-trace formatting code and are
non-deterministic across builds. See the **Out of scope** section
for the full deferred-backlog text.

## User story (US1, P1)

**As an SBOM consumer correlating a compiled binary to known CVEs in
its statically-linked dependencies**, I want mikebom's embedded-
version-string scanner to detect GnuTLS, LibreSSL, LLVM, and OpenJDK
versions when present, so that downstream vulnerability-matching
tools have four more pre-resolved `pkg:generic/<library>@<version>`
components to query against (Vex / OSV / NVD / Kusari Inspector).

**Why P1 (not P2):** these four libraries are common
statically-linked dependencies in distributed binaries (curl bundled
with GnuTLS, HTTPie/Python tooling with LibreSSL, clang/lld with
LLVM, JNI bundles with OpenJDK). Without them, mikebom's
heuristic-tier scan misses high-CVE-volume components — same
data-quality argument that justified the original 7. The output
channel (per-library `pkg:generic` entries with `evidence-kind =
embedded-version-string`) is already plumbed end-to-end; this is
pure coverage breadth.

### Independent test

After implementation:
- `cargo +stable test -p mikebom --bin mikebom scan_fs::binary::version_strings`
  exercises new positive + negative tests for each of the 4 libraries.
- `cargo +stable test -p mikebom --test scan_binary` (and any sibling
  fixture-driven scans) continues green; existing 7 libraries unaffected.
- `cargo +stable test -p mikebom --test holistic_parity` green.
- 27 byte-identity goldens regen produces zero diff (no fixture
  binary contains GnuTLS / LibreSSL / LLVM / OpenJDK strings —
  same null-deltas invariant that held for 023/024/028).
- `mikebom sbom scan` of a control-set binary that legitimately
  contains one of the 4 signatures emits the corresponding
  `pkg:generic/<library>@<version>` component.

## Acceptance scenarios

**Scenario 1: GnuTLS detection**
```
Given: a `string_region` containing the byte sequence
       `\0GnuTLS 3.7.10\0` (or any same-shaped match at a NUL
       boundary)
When:  `version_strings::scan(region)` runs
Then:  the returned Vec contains exactly one
       `EmbeddedVersionMatch { library: GnuTls, version: "3.7.10" }`
       and no false-positive matches from other libraries.
```

**Scenario 2: LibreSSL detection**
```
Given: a region containing `\0LibreSSL 3.8.2\0`
When:  scan runs
Then:  one match `{ LibreSsl, "3.8.2" }`. Crucially, this MUST NOT
       also fire the OpenSSL detector (`LibreSSL` does not start
       with `OpenSSL `, and the at-boundary check should confirm
       the prefix anchors).
```

**Scenario 3: LLVM detection (specific prefix avoids generic-substring confusion)**
```
Given: a region containing `\0LLVM version 17.0.6\0`
When:  scan runs
Then:  one match `{ Llvm, "17.0.6" }`. The detector must NOT fire
       on bare `LLVM ` (e.g., `LLVM ERROR: ...` or
       `LLVM IR module ...` — both common in clang/lld binaries).
       Required prefix is `LLVM version ` (12 chars), not `LLVM ` (5).
```

**Scenario 4: OpenJDK — modern JEP 322 version**
```
Given: a region containing `\0OpenJDK 21.0.1+12\0`
When:  scan runs
Then:  one match `{ OpenJdk, "21.0.1+12" }` — full version string
       including the `+<build>` suffix.
```

**Scenario 5: OpenJDK — legacy 8uXXX-bXX version**
```
Given: a region containing `\0OpenJDK 8u362-b09\0`
When:  scan runs
Then:  one match `{ OpenJdk, "8u362-b09" }`.
```

**Scenario 6: Negative — substring confusion**
```
Given: a region containing `\0Compiled by LLVM 17.0.0.\0`
       (mid-string LLVM mention; not a banner)
When:  scan runs
Then:  zero LLVM matches. The required prefix is `LLVM version `
       AND the at-boundary check enforces that the byte before
       `LLVM` is NUL (or the region start). The substring `by LLVM`
       has neither — `by ` is not NUL, and the longer-prefix
       `LLVM version` is absent.
```

**Scenario 7: Dedup within a single binary**
```
Given: a region with `\0GnuTLS 3.7.10\0` repeated 3 times
       (e.g., embedded across `.rodata` + `.data.rel.ro`)
When:  scan runs
Then:  exactly one match — same dedup-by-(library, version)
       contract that already governs the existing 7 libraries.
```

## Edge cases

- **Version-string proximity to NUL**: same boundary contract as the
  existing 7 — a match anchors only when the byte before the prefix
  is NUL or the region starts there. Mid-string matches are rejected
  per FR-002. (Already enforced in `match_prefix`'s `at_boundary`
  guard at line 80.)

- **Multiple format scheme on OpenJDK**: the parser must accept BOTH
  the modern `<major>.<minor>.<security>[+<build>]` form AND the
  legacy `8u<update>-b<build>` form. The parser tries the modern
  form first; if that fails, it tries the legacy form. Returning
  the version string verbatim (preserving the `+12` or `-b09` suffix)
  matches consumer expectation — `pkg:generic/openjdk@21.0.1+12`
  is what symbol/CVE databases key on.

- **OpenJDK's optional `+build` suffix**: `21.0.1` (no build) AND
  `21.0.1+12` (with build) are both valid; the parser treats
  `+<digits>` as optional. The legacy `8u362` (no `-b09`) is
  technically also valid; parser treats `-b<digits>` as optional.

- **LLVM strict-prefix gate**: the canonical clang/opt banner is
  `LLVM version X.Y.Z` (with `version` between the name and the
  number). Bare `LLVM X.Y.Z` (no `version`) does sometimes appear
  in lld banners but is much noisier — explicitly out of scope
  to keep the false-positive surface tight. Add it later if a
  control-set binary requires it.

- **GnuTLS's `<semver-triple>` may have a 4th `.W` component on
  master / pre-release builds** (e.g., `3.8.0.1`). The parser
  reuses `parse_semver_triple` which strictly accepts X.Y.Z;
  4-segment versions won't match. Acceptable — pre-release builds
  aren't a target audience and the noise reduction is worth it.

- **LibreSSL version may have 4 segments** (rarely — e.g.,
  `3.8.2.1`). Same treatment as GnuTLS: strict semver triple only.

- **Case sensitivity**: all four prefixes are case-sensitive (matches
  the canonical embedded form — `OpenJDK` not `openjdk`, `LLVM`
  not `Llvm`). This avoids false positives on lowercased mentions
  in error messages.

- **Format applicability**: all four signatures appear in `.rodata`-
  equivalent regions across ELF, Mach-O, and PE — the existing
  `string_region` collection already handles all three formats
  identically.

## Functional requirements

- **FR-001**: `mikebom-cli/src/scan_fs/binary/version_strings.rs`'s
  `CuratedLibrary` enum gains four new variants: `GnuTls`,
  `LibreSsl`, `Llvm`, `OpenJdk`. Each gets a corresponding `slug()`
  arm: `"gnutls"`, `"libressl"`, `"llvm"`, `"openjdk"`.

- **FR-002**: `match_prefix` gains four new prefix-match arms:
  - `b"GnuTLS "` → `parse_semver_triple` → `CuratedLibrary::GnuTls`
  - `b"LibreSSL "` → `parse_semver_triple` → `CuratedLibrary::LibreSsl`
  - `b"LLVM version "` → `parse_semver_triple` → `CuratedLibrary::Llvm`
  - `b"OpenJDK "` → `parse_openjdk_version` (new) → `CuratedLibrary::OpenJdk`
  Each preserves the existing `at_boundary` check (NUL or pos==0).

- **FR-003**: Add a new helper `parse_openjdk_version(tail: &[u8]) -> Option<String>`
  that accepts:
  - **Modern form**: `<digits>.<digits>.<digits>(+<digits>)?` —
    e.g. `21.0.1`, `21.0.1+12`, `17.0.5`, `17.0.5+9`. Terminates
    on whitespace / NUL / non-version-char.
  - **Legacy form**: `8u<digits>(-b<digits>)?` — e.g. `8u362`,
    `8u362-b09`. Terminates on whitespace / NUL / non-version-char.
  - Falls through to None on anything else.
  - Returns the full matched version string verbatim (preserves
    `+12` or `-b09` suffix).

- **FR-004**: Inline `#[cfg(test)] mod tests` gains at least 8 new
  tests:
  - `gnutls_positive` — `\0GnuTLS 3.7.10\0` → 1 match
  - `gnutls_no_false_positive_on_library_name_alone` — `\0GnuTLS\0`
    (no version) → 0 matches
  - `libressl_positive` — `\0LibreSSL 3.8.2\0` → 1 match
  - `libressl_distinct_from_openssl` — region with both
    `\0LibreSSL 3.8.2\0` and `\0OpenSSL 3.0.11 19 Sep 2023\0`
    → exactly 2 matches (LibreSsl + OpenSsl), no double-emit
  - `llvm_positive` — `\0LLVM version 17.0.6\0` → 1 match
  - `llvm_bare_name_does_not_match` — `\0LLVM ERROR: foo\0` and
    `\0Compiled by LLVM 17.0.0.\0` → 0 matches (strict prefix
    requires `LLVM version `)
  - `openjdk_modern_version_with_build` — `\0OpenJDK 21.0.1+12\0`
    → 1 match, version `"21.0.1+12"`
  - `openjdk_legacy_8u_version` — `\0OpenJDK 8u362-b09\0` → 1
    match, version `"8u362-b09"`

- **FR-005**: TODO marker added in `version_strings.rs` as a `//
  TODO(milestone-026.x):` doc comment listing the deferred 3
  (glibc / musl / V8) with a one-line note on why each is hard.

- **FR-006**: `docs/design-notes.md`'s "Deferred backlog" section
  gains a new entry: `### Curated version-string scanner — hard
  cohort (deferred from milestone 026)`. Names the 3 libraries
  and the technical blocker for each.

- **FR-007**: No new types beyond the 4 enum variants. No new
  modules. No `Cargo.toml` changes. No public-API change beyond
  the `CuratedLibrary` enum gaining variants (which is `pub`).

- **FR-008**: 27 byte-identity goldens regen produces zero diff
  (no fixture binary embeds GnuTLS / LibreSSL / LLVM / OpenJDK
  signatures).

- **FR-009**: Per-commit `./scripts/pre-pr.sh` clean. Two atomic
  commits (one for parsers + tests, one for the TODO + docs).
  Optionally one commit if scope feels tight enough — the user
  has been comfortable with single-commit milestones for
  mechanical-extension work.

## Success criteria

- **SC-001**: `./scripts/pre-pr.sh` clean.

- **SC-002**: New tests in `version_strings::tests` cover all 4
  libraries with at least one positive + one negative case each
  (8+ new tests minimum).

- **SC-003**: `git diff main..HEAD --
  mikebom-cli/src/scan_fs/binary/{elf,macho,pe,scan,entry,mod,linkage,packer,predicates,jdk_collapse,python_collapse,version_strings_*}.rs`
  affects ONLY `version_strings.rs`. No other binary-scanner file
  touched (this is a contained-helper-only milestone).

- **SC-004**: `wc -l mikebom-cli/src/scan_fs/binary/version_strings.rs`
  ≤ 700 LOC. (Current: 452. Budget ceiling: 700; expected
  delta: ~150-200 LOC for 4 prefix arms + `parse_openjdk_version`
  + 8 inline tests.)

- **SC-005**: `git diff main..HEAD -- mikebom-common/
  mikebom-cli/src/cli/ mikebom-cli/src/resolve/ mikebom-cli/src/generate/
  mikebom-cli/src/scan_fs/package_db/` is empty. No surface
  changes outside the scanner.

- **SC-006**: All 3 CI lanes (Linux default + Linux ebpf + macOS) green.

- **SC-007**: 27-golden regen zero diff (no fixture has the new
  library signatures).

- **SC-008**: `version_strings.rs` carries an explicit
  `// TODO(milestone-026.x):` block naming the 3 deferred libraries
  + their blockers. Discoverable to future contributors via grep.

## Clarifications

- **Strict semver only on GnuTLS / LibreSSL / LLVM**: 4-segment
  pre-release versions (e.g. GnuTLS `3.8.0.1`) won't match. This
  is intentional — pre-release builds aren't a target consumer
  and 4-segment matching invites false positives. If a real-world
  control-set binary turns up a 4-segment release, revisit.

- **Two-scheme parser for OpenJDK**: deliberate — both schemes are
  in active distribution (Java 8 LTS still ships `8u<update>-b<build>`
  through Eclipse Temurin and Amazon Corretto). Skipping legacy
  Java 8 misses a meaningful fraction of the target binaries.

- **`LLVM version ` prefix, not `LLVM `**: deliberate to avoid the
  enormous false-positive surface of bare `LLVM ` mentions in
  clang/lld error strings, IR debug strings, etc. The clang
  driver's `--version` banner reliably uses the full
  `LLVM version <triple>` form.

- **Output shape unchanged**: each match still flows through
  `version_match_to_entry` → `pkg:generic/<library>@<version>`
  with `evidence-kind = "embedded-version-string"` + `confidence
  = "heuristic"`. No new annotation keys, no new bag entries.

- **Not a bag consumer**: this milestone differs from 023/024/025/028
  — it produces NEW components (one per detected library), not
  annotations on existing components. The bag-amortization streak
  stays at 4. This is purely scanner-coverage breadth.

## Out of scope

### Hard cohort — deferred to milestone 026.x (research-and-attempt)

Three libraries from the original wishlist do **not** have clean
self-identifying strings in `string_region` and are deferred:

- **glibc** — version markers (`GLIBC_X.Y`) live in the
  `.gnu.version_r` ELF section (symbol version table), not in
  `.rodata`. Detecting them requires a new ELF-section reader
  that walks `.gnu.version_r` entries and aggregates the maximum
  GLIBC version string. Different mechanism than the curated
  string scanner, so a separate small milestone.

- **musl** — typically doesn't self-identify in compiled output.
  Some bundled-musl binaries embed a `musl libc (x86_64)` banner
  via `__libc_get_version()` calls but that path is rarely
  exercised in static binaries. Research milestone needed to
  find a reliable signature (or conclude there isn't one and
  document the gap).

- **V8** — version strings live in stack-trace formatting code
  (e.g., `v8::internal::Version::GetString()` callers) and tend
  to be non-deterministic across builds + dependent on V8 build
  flags. Research milestone needed to find a reliable
  string-region signature; may end up needing an inline-data
  blob scan rather than a string scan.

Tracking: the deferred-backlog entry in `docs/design-notes.md`
captures the technical blocker per library so a future
contributor (or future-mike) can pick this up without re-doing
the research.

### Not in this milestone

- Walking new binary-format sections (e.g., `.gnu.version_r`).
- Detecting libraries via `imports[]` (DT_NEEDED / LC_LOAD_DYLIB
  / IMAGE_IMPORT_DIRECTORY) rather than via embedded strings.
  That's a different signal class and would emit `linkage-evidence`
  rather than `embedded-version-string` entries.
- Promoting `evidence-kind = "embedded-version-string"` matches
  to higher-confidence tiers via cross-validation against
  other signals.
- Any change to how `pkg:generic` PURLs are formed or how
  `evidence-kind` is emitted.
- Any change to the `extra_annotations` bag (this milestone
  doesn't touch the bag).
