---
description: "Task list for milestone 135 — Arch Linux pacman/alpm package database reader"
---

# Tasks: Arch Linux pacman/alpm package database reader

**Input**: Design documents from `/specs/135-arch-alpm-reader/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ ✓, quickstart.md ✓

**Tests**: INCLUDED — the spec embeds explicit "Independent Test" sections per user story and SC-001..SC-006 each name a concrete fixture-based test. Test tasks ride alongside their owning user story.

**Organization**: Tasks grouped by user story so each story can be implemented, merged, and shipped independently as an MVP increment.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Parallelizable (different files, no dependencies on incomplete tasks)
- **[Story]**: User story label (US1 / US2 / US3); omitted in Setup / Foundational / Polish phases
- Every task lists exact file paths

## Path conventions

Brownfield extension to the existing mikebom workspace. All paths are relative to the repo root `/Users/mlieberman/Projects/mikebom`.

- New reader: `mikebom-cli/src/scan_fs/package_db/alpm.rs`
- Dispatcher integration: `mikebom-cli/src/scan_fs/package_db/mod.rs`
- Tests: `mikebom-cli/tests/`
- Docs: no changes (PURL is native per Principle V audit — no new C-row)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Branch + spec scaffolding. Already complete via `/speckit.specify` + `/speckit.plan`.

- [X] T001 Verify branch `135-arch-alpm-reader` is checked out and the `specs/135-arch-alpm-reader/` directory contains `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, and `quickstart.md`. No file edits in this task — pure verification (`ls` + `git branch --show-current`).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Reader-internal types + parser + module skeleton. MUST complete before any user story phase can start because every user story depends on the reader emitting `PackageDbEntry` instances.

- [X] T002 Create `mikebom-cli/src/scan_fs/package_db/alpm.rs` with the module docblock, error enum `AlpmError` (using `thiserror`), and the public `read(rootfs: &Path, namespace: &str, distro_version: Option<&str>) -> Result<Vec<PackageDbEntry>, AlpmError>` entry-point function stub returning `Ok(vec![])` initially. Mirrors the shape of `dpkg.rs::read` at lines 46–88 (don't copy text — keep the alpm module self-contained).
- [X] T002b In `mikebom-cli/src/generate/cyclonedx/builder.rs` (around line 942–961), extend the `debug_assert!` canonical-enum check for `mikebom:evidence-kind` to include `"alpm-local-db"`. Update the diagnostic string to list the new value. SPDX 2.3 + SPDX 3 emitters are unconditional-push and require no change. Verify via `cargo +stable test --workspace` — failure here would panic in any milestone-135 test fixture that emits an alpm component (closes analysis finding C1).
- [X] T003 Add `pub(crate) mod alpm;` to `mikebom-cli/src/scan_fs/package_db/mod.rs` (alongside `dpkg`, `apk`, `rpm`, `opkg`). This is the smallest possible diff that makes the new module visible to the orchestrator without yet calling it. Compile clean (`cargo +stable check -p mikebom`).
- [X] T004 In `alpm.rs`, implement the reader-private `PacmanDescStanza` struct per `data-model.md`, plus a parser `parse_desc(text: &str) -> Option<PacmanDescStanza>` that consumes the `%KEY%`-block format. Required fields: `%NAME%`, `%VERSION%`, `%ARCH%`. Optional: `%DESC%`, `%URL%`, `%LICENSE%`, `%PACKAGER%`, `%DEPENDS%`, `%OPTDEPENDS%`, `%CONFLICTS%`, `%REPLACES%`, `%PROVIDES%`, `%REASON%`. Return `None` (warn-and-skip semantics) when required fields are missing. Add unit tests under `#[cfg(test)] mod tests` covering: well-formed stanza, missing-name, missing-version, missing-arch, multi-value `%LICENSE%`, multi-value `%DEPENDS%`, `%OPTDEPENDS%` with reason-text suffix.
- [X] T005 In `alpm.rs`, implement `build_alpm_purl(namespace: &str, name: &str, version: &str, arch: &str, distro_qualifier: Option<&str>) -> Result<Purl, AlpmError>` returning a validated `pkg:alpm/<namespace>/<name>@<version>?arch=<arch>[&distro=<namespace>-<verid>]` PURL via `mikebom_common::types::purl::Purl::new`. Wire-format per `contracts/alpm-component-purl.md`. Unit tests: stock Arch (no distro qualifier), SteamOS (with qualifier), noarch package, percent-encoded name (e.g., `lib32-glibc`).

**Checkpoint**: at this point the module exists, parses `desc` files, builds PURLs, and integrates into the crate's module graph — but `read()` still returns an empty vec and the dispatcher does not call it yet. The workspace MUST still compile clean (`cargo +stable check --workspace`).

---

## Phase 3: User Story 1 — Operator gets a complete SBOM of an Arch container image (Priority: P1) 🎯 MVP

**Goal**: A scan of an Arch-based rootfs (container image, desktop install, WSL) emits one component per pacman-installed package with the canonical `pkg:alpm/arch/<name>@<version>?arch=<arch>` PURL identity and accurate dep edges. Existing dpkg/apk/rpm-only scans stay byte-identical.

**Independent Test** (from spec SC-001 + US1 acceptance scenarios): Synthetic fixture with 3–5 packages under `<tmp>/var/lib/pacman/local/<name>-<ver>/desc`. Run `mikebom sbom scan --path <tmp>`. Parse CDX JSON. Assert exactly those packages emit with correct PURLs and dep edges.

### Reader-side (alpm.rs)

- [X] T006 [US1] In `mikebom-cli/src/scan_fs/package_db/alpm.rs`, flesh out `read(rootfs, namespace, distro_version)`: enumerate `<rootfs>/var/lib/pacman/local/*/desc`, call `parse_desc` on each, convert each `PacmanDescStanza` to a `PackageDbEntry` per the field mapping in `data-model.md` §"PackageDbEntry field mapping". Per-package read failures emit `tracing::warn!` and skip per FR-009. Returns `Ok(vec![])` cleanly when the `local/` directory is absent or empty (FR-008 — no-op).
- [X] T007 [US1] In `read`, derive the `distro=` qualifier from `(namespace, distro_version)`: present when `distro_version.is_some()`, omitted otherwise (FR-005). Pass to `build_alpm_purl`.
- [X] T008 [US1] In `read`, populate `PackageDbEntry.depends` from the parsed `%DEPENDS%` lines: split each dep spec on the first comparison operator (`<`, `<=`, `=`, `>=`, `>`) and keep only the name half. Multi-value `%DEPENDS%` MUST produce multiple dep edges. `%OPTDEPENDS%` MUST NOT appear in `depends` (FR-006). Add unit tests covering: bare name, name with version constraint, multiple deps, optdepends exclusion.

### Dispatcher integration

- [X] T009 [US1] In `mikebom-cli/src/scan_fs/package_db/mod.rs` around line 1200, derive `alpm_namespace` from the existing `id_raw` variable per research §R5: `id_raw.as_deref().map(|s| s.to_lowercase()).unwrap_or_else(|| "arch".to_string())`. This sits adjacent to the existing `deb_namespace` derivation.
- [X] T010 [US1] In `mod.rs` immediately after the existing `apk::read(...)` block (around line 1239), add the parallel `alpm::read(rootfs, &alpm_namespace, distro_version.as_deref())` invocation. On `Ok(entries)` extend `out`; on `Err(e)` debug-log per the existing dpkg/apk/rpm convention. Defer `collect_claimed_paths` to US3 (P3) — for US1 the binary walker's duplicate emission is acceptable noise.

### Tests (US1)

- [X] T011 [US1] Create `mikebom-cli/tests/alpm_arch_baseline.rs` containing the SC-001 acceptance test. Use `tempfile::tempdir()` to construct three synthetic packages under `<tmp>/var/lib/pacman/local/{bash-5.2.026-1,glibc-2.40-1,curl-8.5.0-1}/desc` per the format in research §R2 (the `5.2.026-1` form matches realistic pacman versioning: dotted upstream version + `-N` pkgrel). Include declared `%DEPENDS%` so curl depends on glibc. Run `mikebom sbom scan` via `Command::new(env!("CARGO_BIN_EXE_mikebom"))`. Parse the emitted CDX JSON. Assert: (a) exactly 3 `pkg:alpm/arch/*` components emit, (b) each has the expected PURL, (c) curl's dependsOn relationship targets glibc's bom-ref.
- [X] T012 [US1] Add to the same file a no-pacman-DB regression test: scan a tempdir containing only an unrelated file (no `/var/lib/pacman/`). Assert: (a) zero `pkg:alpm/*` components, (b) no `WARN`/`ERROR` lines in stderr mentioning pacman/alpm, (c) exit code 0. Asserts FR-008.
- [X] T013 [P] [US1] Add a no-op-preservation test: assert that the existing 11-ecosystem cargo regression goldens (`mikebom-cli/tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/cargo.*.json`) are byte-identical pre/post this milestone by running `cargo +stable test --workspace --test cdx_regression --test spdx_regression --test spdx3_regression`. (Goldens unchanged → SC-003 invariant gate.) This task is procedural — it amounts to confirming the test suite passes after T006–T010 land; no test-file edits required.

**Checkpoint**: US1 ships independently — operators scanning Arch container images get complete pacman-component coverage in their SBOMs. The binary walker may still emit `pkg:generic/bash` duplicates (deferred to US3); the standard OS-package-DB dedup posture from milestone 002 (PURL-key `seen_purls` collision check at `package_db/mod.rs:~1042`) is preserved bit-for-bit. Can be merged as a standalone PR before US2 or US3 ships.

---

## Phase 4: User Story 2 — Operator scans an Arch derivative and gets the correct distro namespace (Priority: P2)

**Goal**: SteamOS, Manjaro, EndeavourOS, CachyOS (and any future derivative) get the correct `pkg:alpm/<id>/...` PURL namespace + `distro=<id>-<verid>` qualifier convention, with stock rolling-release Arch correctly omitting the qualifier.

**Independent Test** (from spec SC-002 + US2 acceptance scenarios): Synthetic fixture pair — one with `ID=steamos` + `VERSION_ID=3.5.7`, one with `ID=arch` and no `VERSION_ID`. Scan both. Assert namespace + qualifier semantics per the contract.

**Dependency**: US1 must complete first (US2 extends the reader's PURL construction in T005/T007).

### Tests (US2)

- [X] T014 [US2] Create `mikebom-cli/tests/alpm_derivative_distros.rs` with one test per recognized derivative (`steamos`, `manjaro`, `endeavouros`, `cachyos`) plus the unknown-derivative case (`mydistro`). Each test constructs a tempdir with a matching `/etc/os-release` (set `ID=<distro>` + `VERSION_ID=<value>` when applicable per the spec's US2 acceptance scenarios) plus a single synthetic pacman package. Scan. Assert: (a) the emitted PURL's namespace matches the `ID`, (b) the `distro=` qualifier presence matches whether `VERSION_ID` was set.
- [X] T015 [US2] Add to the same file a rolling-release-Arch test: tempdir with `/etc/os-release` containing only `ID=arch` (no `VERSION_ID`). Assert the emitted PURL has the `arch` namespace AND no `distro=` qualifier (matches the existing dpkg/apk/rpm pattern when `VERSION_ID` is absent).
- [X] T016 [US2] Add to the same file a no-os-release test: tempdir with a synthetic pacman package but no `/etc/os-release` at all. Assert the namespace defaults to `arch` per FR-004.

**Checkpoint**: US2 ships as a follow-up PR after US1 is merged. SteamOS / Manjaro / CachyOS / EndeavourOS scans now emit correct distro-attributed PURLs. The data-model code change footprint is zero (T007 already implemented the qualifier handling correctly); US2 is essentially a test-coverage expansion that validates the derivative-distro behavior holds end-to-end.

---

## Phase 5: User Story 3 — Binary walker skips pacman-claimed files (Priority: P3)

**Goal**: A pacman-owned `/usr/bin/bash` no longer produces both a `pkg:alpm/arch/bash` AND a `pkg:generic/bash` entry — the binary walker's file-claim tracker skips the path because the alpm reader has registered it.

**Independent Test** (from spec SC-004): Synthetic fixture with a real ELF at `/usr/bin/bash` and a corresponding `<tmp>/var/lib/pacman/local/bash-5.2-1/files` manifest declaring `usr/bin/bash` as owned. Scan. Assert exactly one `bash` component (the alpm one).

**Dependency**: US1 must complete first (US3 builds on the reader's registration in `read_all`). Independent of US2 — US3 can ship before or after US2.

### Reader-side (file-claim integration)

- [X] T017 [US3] In `mikebom-cli/src/scan_fs/package_db/alpm.rs`, implement `pub fn collect_claimed_paths(rootfs: &Path, claimed: &mut HashSet<PathBuf>, #[cfg(unix)] claimed_inodes: &mut HashSet<(u64, u64)>)` per the shape in `data-model.md` §"File-claim contribution". Walk `<rootfs>/var/lib/pacman/local/*/files`, parse the `%FILES%` block, insert each non-directory path (resolved against `rootfs`) into `claimed`. On Unix, `stat()` each resolved path and insert `(dev_id, inode)` into `claimed_inodes`. Per-file resolve errors are warn-and-skip per FR-009.
- [X] T018 [US3] In `mikebom-cli/src/scan_fs/package_db/mod.rs`, extend the dispatcher's alpm `Ok(...)` arm (T010) to also call `alpm::collect_claimed_paths(rootfs, &mut claimed, /* cfg(unix) */ &mut claimed_inodes)`. Mirrors the existing dpkg/apk/rpm invocations exactly.
- [X] T019 [US3] Add unit tests in `alpm.rs` for `collect_claimed_paths` covering: well-formed `files` manifest (multiple paths inserted), trailing-slash directory entries (NOT inserted), missing `files` file (warn-and-skip), empty `%FILES%` block (no-op).

### Tests (US3)

- [X] T020 [US3] Create `mikebom-cli/tests/alpm_file_claim_dedupe.rs` containing the SC-004 acceptance test. Build a fixture: tempdir with a real ELF binary at `<tmp>/usr/bin/bash` (smallest possible — empty ELF header is fine for the file-walk path) AND `<tmp>/var/lib/pacman/local/bash-5.2.026-1/{desc,files}` where `files` declares ownership of `usr/bin/bash`. Run `mikebom sbom scan`. Assert: (a) exactly one component with name `bash`, (b) its PURL is `pkg:alpm/arch/bash@5.2.026-1?arch=x86_64`, (c) no `pkg:generic/bash` entry exists.
- [X] T021 [US3] Add to the same file the unclaimed-binary control case: same fixture but with an additional ELF at `<tmp>/opt/custom-tool` that is NOT claimed by any pacman package. Assert that custom-tool DOES surface (the file-claim only suppresses claimed paths; unclaimed binaries continue to emit per milestone-004).

**Checkpoint**: US3 ships as the third PR. Operators get clean dedup behavior — no more spurious `pkg:generic/*` duplicates for pacman-owned binaries on Arch images.

---

## Phase 6: Edge cases + polish + pre-PR gate

**Purpose**: Hardening + the mandatory pre-PR gate per `CLAUDE.md`.

- [X] T022 [P] Create `mikebom-cli/tests/alpm_edge_cases.rs` covering the spec's "Edge Cases" section: (a) malformed `desc` file alongside three valid packages → SC-005's exit-0 + 3 components + warn; (b) `%ARCH%=any` noarch package → emits `?arch=any`; (c) multi-version coexistence (`bash-5.2-1` + `bash-5.1-1` in same `local/` dir) → both emit as separate components; (d) group packages → skipped (no component emitted); (e) lib32 multilib package → correct PURL.
- [X] T023 [P] Add a SC-006 verification test: scan an Arch fixture and assert that `jq '.components[] | select(.purl | startswith("pkg:alpm/")) | .purl' /tmp/out.cdx.json` returns the expected component count. Lives in `mikebom-cli/tests/alpm_arch_baseline.rs` as an additional test case.
- [X] T024 Update `CHANGELOG.md` with a milestone-135 entry under the `[Unreleased]` section. Cite #429 as the closing issue. Mention: (a) `pkg:alpm/*` native PURL emission across CDX/SPDX 2.3/SPDX 3, (b) `distro=` qualifier convention matching dpkg/apk/rpm precedent, (c) file-claim tracker integration, (d) zero new Cargo deps + no new `mikebom:*` annotation (Principle V audit cited).
- [X] T025 Run the mandatory pre-PR gate per `CLAUDE.md`: `./scripts/pre-pr.sh` (which runs `cargo +stable clippy --workspace --all-targets -- -D warnings` followed by `cargo +stable test --workspace`). Both MUST report zero errors / `0 failed`. If clippy flags any lints, fix them locally before pushing — `feedback-clippy-before-async-patterns` memory note applies.
- [X] T026 Open the PRs in order: one for US1 (Phase 3 alone is the MVP slice), one for US2 (Phase 4), one for US3 + Polish (Phase 5 + Phase 6). Each PR closes #429 partially; the third PR (US3 + Polish) closes it.

---

## Dependencies

```text
Phase 1 (Setup)                  → Phase 2
Phase 2 (Foundational)           → Phase 3 (US1)  [REQUIRED — module + parser + PURL builder must exist before reader wiring]
Phase 3 (US1, P1)                → Phase 4 (US2, P2)   [US2 extends US1's PURL construction site]
Phase 3 (US1, P1)                → Phase 5 (US3, P3)   [US3 builds on the dispatcher integration]
Phase 4 (US2)                    ⫪ Phase 5 (US3)        [US2 and US3 are independent — either order]
Phase 5 (US3)                    → Phase 6 (Polish)
```

## Parallel-execution opportunities

Within Phase 2 (Foundational), once T002 + T003 land:

```text
T004 (parser)         |
T005 (PURL builder)   | — different concerns within the same file, but no shared state; can run concurrently
```

Within Phase 3 (US1), once T006 (reader skeleton) + T007 (qualifier) land:

```text
T008 (depends parsing) |
T011 (Arch baseline test) | — depends parsing is internal; test is end-to-end; can write in parallel
T012 (no-pacman-DB test) |
T013 (regression goldens) |
```

Within Phase 5 (US3):

```text
T017 (collect_claimed_paths impl) → T018 (dispatcher wiring) → T019 (unit tests)
T020 (file-claim integration test)
T021 (unclaimed-binary control)
```

Within Phase 6:

```text
T022 (edge cases) |
T023 (SC-006 test) | — independent test files; can land in parallel
```

## MVP scope

**Phase 3 (US1, P1) alone is the shipping MVP slice.** It delivers the headline value: operators scanning Arch container images get a complete pacman-derived SBOM. The `pkg:generic/bash` duplicate is a known soft regression deferred to US3; the namespace defaulting to `arch` is correct for the stock-Arch case and stays correct for SteamOS/Manjaro/CachyOS via existing code paths (US2 is a test-coverage expansion that validates the cross-derivative behavior, not new code).

US2 and US3 are strictly additive — neither is required for US1 to ship useful capability, and either can land in a follow-up PR.

## Format validation

All 27 tasks follow the strict checklist format: `- [ ] T<NNN> [P?] [Story?] Description with file path`. Setup (T001), Foundational (T002, T002b, T003–T005), and Polish (T022–T026) tasks omit the story label per spec. User-story phases (T006–T021) carry the [US1] / [US2] / [US3] labels. Parallelizable tasks across independent files are marked [P].
