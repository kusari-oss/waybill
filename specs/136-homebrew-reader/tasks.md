---
description: "Task list for milestone 136 — Homebrew (brew + Linuxbrew) package detection"
---

# Tasks: Homebrew (brew + Linuxbrew) package detection

**Input**: Design documents from `/specs/136-homebrew-reader/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ ✓, quickstart.md ✓

**Tests**: INCLUDED — the spec embeds explicit "Independent Test" sections per user story and SC-001..SC-007 each name a concrete fixture-based test. Test tasks ride alongside their owning user story.

**Organization**: Tasks grouped by user story so each story can be implemented, merged, and shipped independently as an MVP increment.

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Parallelizable (different files, no dependencies on incomplete tasks)
- **[Story]**: User story label (US1 / US2 / US3); omitted in Setup / Foundational / Polish phases
- Every task lists exact file paths

## Path conventions

Brownfield extension to the existing mikebom workspace. All paths are relative to the repo root `/Users/mlieberman/Projects/mikebom`.

- New reader: `mikebom-cli/src/scan_fs/package_db/brew.rs`
- Dispatcher integration: `mikebom-cli/src/scan_fs/package_db/mod.rs`
- Evidence-kind enum extension: `mikebom-cli/src/generate/cyclonedx/builder.rs`
- Tests: `mikebom-cli/tests/`
- Docs: no changes (PURL is the native carrier per Principle V audit; the `brew` type-name is informally industry-standard pending purl-spec extension follow-up)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Branch + spec scaffolding. Already complete via `/speckit.specify` + `/speckit.plan`.

- [X] T001 Verify branch `136-homebrew-reader` is checked out and the `specs/136-homebrew-reader/` directory contains `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/`, and `quickstart.md`. No file edits in this task — pure verification (`ls` + `git branch --show-current`).

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Reader-internal types + parsers + PURL builder + module skeleton + evidence-kind enum extension. MUST complete before any user story phase can start because every user story depends on the reader emitting `PackageDbEntry` instances with valid `mikebom:evidence-kind` values.

- [X] T002 Create `mikebom-cli/src/scan_fs/package_db/brew.rs` with the module docblock, error enum `BrewError` (using `thiserror`), and the public `read(rootfs: &Path) -> Result<Vec<PackageDbEntry>, BrewError>` entry-point function stub returning `Ok(vec![])` initially. Use `alpm.rs` (milestone 135) as the **architectural template only** — reimplement parsing per the brew-specific JSON shape; don't share parsing helpers with alpm. NOTE: `brew::read` takes ONLY `rootfs: &Path` (no `namespace` / `distro_version` params unlike alpm) because Homebrew components don't carry an OS-distro namespace — the three prefix locations ARE the discrimination signal, not `/etc/os-release` (closes analysis-finding I3).
- [X] T002b In `mikebom-cli/src/generate/cyclonedx/builder.rs` (around line 942–961), extend the `debug_assert!` canonical-enum check for `mikebom:evidence-kind` to include `"brew-install-receipt"` AND `"brew-cask-metadata"` (TWO new values for this milestone). Update the diagnostic string to list both. SPDX 2.3 + SPDX 3 emitters are unconditional-push and require no change. Verify via `cargo +stable test --workspace` — failure here would panic in any milestone-136 test fixture that emits a brew component (closes the same class of analysis-finding C1 issue from milestone 135).
- [X] T003 Add `pub mod brew;` to `mikebom-cli/src/scan_fs/package_db/mod.rs` (alongside `alpm`, `dpkg`, `apk`, `rpm`, `opkg`). This is the smallest possible diff that makes the new module visible to the orchestrator without yet calling it. Compile clean (`cargo +stable check -p mikebom`).
- [X] T004 In `brew.rs`, implement the reader-private `InstallReceipt` + `ReceiptSource` + `RuntimeDep` serde structs per `data-model.md`. Use `#[serde(default)]` on every optional field to handle older receipts gracefully. Implement `parse_install_receipt(text: &str) -> Option<InstallReceipt>` that wraps `serde_json::from_str` and returns `None` on parse error (warn-and-skip semantics handled by the caller). Add unit tests under `#[cfg(test)] mod tests` covering: minimal modern receipt (curl 8.5.0), receipt with third-party `source.tap` (hashicorp/tap), receipt with empty `runtime_dependencies`, receipt with missing `source` block, malformed JSON (returns None), receipt with `pkg_version` vs `version` on the dep entry.
- [X] T005 In `brew.rs`, implement the reader-private `CaskMetadata` serde struct per `data-model.md`. Same `#[serde(default)]` discipline. Implement `parse_cask_metadata(text: &str) -> Option<CaskMetadata>`. Unit tests: minimal modern cask JSON (visual-studio-code), cask with multiple `name[]` aliases, cask with `depends_on.formula` (parsed but ignored per FR-005), malformed JSON, missing required `token` or `version` (returns None).
- [X] T006 In `brew.rs`, implement `build_brew_purl(name: &str, version: &str, tap: Option<&str>, kind: BrewKind) -> Option<Purl>` returning a validated `pkg:brew/<name>@<version>[?tap=<owner>/<tap>][&type=cask]` PURL via `mikebom_common::types::purl::Purl::new`. `BrewKind` is a reader-local enum `{ Formula, Cask }`. Tap qualifier rules per `contracts/brew-component-purl.md`: omit when tap is `None`, empty, `"homebrew/core"`, or `"homebrew/cask"`; emit verbatim otherwise. Cask emits `type=cask`; formula does not. Qualifier ordering follows sorted-key convention (`tap` < `type`). Unit tests: core formula no qualifier, third-party tap formula, noarch n/a (Homebrew doesn't use arch in PURL), cask default, cask third-party tap, formula with `null` tap → no qualifier.

**Checkpoint**: at this point the module exists, parses receipt + cask JSON, builds PURLs, and is wired into the crate's module graph — but `read()` still returns an empty vec and the dispatcher does not call it yet. The workspace MUST still compile clean (`cargo +stable check --workspace`).

---

## Phase 3: User Story 1 — Operator scans an Apple Silicon macOS developer machine (Priority: P1) 🎯 MVP

**Goal**: A scan of an Apple Silicon macOS rootfs (or any rootfs containing `/opt/homebrew/Cellar/`) emits one component per pacman-installed formula with the canonical `pkg:brew/<formula>@<version>` PURL identity and accurate dep edges from `runtime_dependencies`. Existing dpkg/apk/rpm/alpm-only scans stay byte-identical.

**Independent Test** (from spec SC-001 + US1 acceptance scenarios): Synthetic fixture with 3–5 formulae under `<tmp>/opt/homebrew/Cellar/<formula>/<version>/INSTALL_RECEIPT.json`, one with declared `runtime_dependencies`. Run `mikebom sbom scan --path <tmp>`. Parse CDX JSON. Assert exactly those formulae emit with correct PURLs and dependsOn edges.

### Reader-side (brew.rs)

- [X] T007 [US1] In `mikebom-cli/src/scan_fs/package_db/brew.rs`, implement `read_formulae(rootfs: &Path, prefix: &str) -> Vec<PackageDbEntry>`: walk `<rootfs>/<prefix>/Cellar/<formula>/<version>/INSTALL_RECEIPT.json`, call `parse_install_receipt` on each, convert each into a `PackageDbEntry` per the formula field mapping in `data-model.md` §"PackageDbEntry field mapping (formula)". Per-formula read/parse failures emit `tracing::warn!` and skip per FR-007. Use `evidence_kind = Some("brew-install-receipt")`. **Extract dep names from `runtime_dependencies[].full_name` by NORMALIZING to the bare form** — `full_name.rsplit('/').next().unwrap_or(full_name)` — so a core dep `"openssl@3"` stays `"openssl@3"` and a third-party-tap dep `"hashicorp/tap/terraform"` becomes `"terraform"` (matches the emitted component's bare name; see data-model.md §"Dep-name extraction"). Without this normalization, third-party-tap dep edges silently fail to resolve in `scan_fs/mod.rs::name_to_purl` (closes analysis-finding I1). Add unit tests covering: 3-formula fixture with dep edges (mix of core + tap-qualified deps), fixture with one formula missing `runtime_dependencies`, fixture with non-core tap formula that's the dep TARGET of another formula (asserts the resolver finds it via the bare name).
- [X] T008 [US1] In `brew.rs`, implement the public `read(rootfs: &Path) -> Result<Vec<BrewError>>` entry-point: iterate the three prefixes per research §R4 (`opt/homebrew`, `usr/local`, `home/linuxbrew/.linuxbrew`); for each, check `<rootfs>/<prefix>/Cellar/.is_dir()` and call `read_formulae` when present. Aggregate into a single `Vec<PackageDbEntry>`. Returns `Ok(vec![])` cleanly when NONE of the three prefixes have a `Cellar/` directory (FR-006 — no-op).

### Dispatcher integration

- [X] T009 [US1] In `mikebom-cli/src/scan_fs/package_db/mod.rs` immediately after the milestone-135 `alpm::read(...)` block, add the parallel `brew::read(rootfs)` invocation. On `Ok(entries)` extend `out`; on `Err(e)` debug-log per the existing dpkg/apk/rpm/alpm convention. Do NOT call `collect_claimed_paths` — file-claim integration is OUT OF SCOPE per spec (research §R5).

### Tests (US1)

- [X] T010 [US1] Create `mikebom-cli/tests/brew_apple_silicon_baseline.rs` containing the SC-001 acceptance test. Use `tempfile::tempdir()` to construct three synthetic formulae under `<tmp>/opt/homebrew/Cellar/{curl-8.5.0,openssl@3-3.4.0,brotli-1.1.0}/INSTALL_RECEIPT.json` per the format in research §R2. Include `runtime_dependencies` so curl depends on openssl@3 + brotli. Run `mikebom sbom scan` via `Command::new(env!("CARGO_BIN_EXE_mikebom"))`. Parse the emitted CDX JSON. Assert: (a) exactly 3 `pkg:brew/*` components emit, (b) each has the expected PURL (no `tap=` qualifier — all core), (c) curl's dependsOn relationship targets openssl@3 and brotli bom-refs.
- [X] T011 [US1] Add to the same file an SC-007 test: synthetic fixture with a non-core-tap formula (e.g., `terraform-1.10.0` with `source.tap = "hashicorp/tap"` in the receipt). Assert the emitted PURL is `pkg:brew/terraform@1.10.0?tap=hashicorp/tap` AND a default-tap companion (e.g., curl from core) emits WITHOUT the `tap=` qualifier in the same scan.
- [X] T012 [US1] Add to the same file the no-Homebrew regression test (FR-006): scan a tempdir containing only an unrelated file (no `/opt/homebrew/`, no `/usr/local/Cellar/`, no `/home/linuxbrew/`). Assert: (a) zero `pkg:brew/*` components, (b) no `WARN`/`ERROR` lines in stderr mentioning brew/homebrew, (c) exit code 0.
- [X] T013 [P] [US1] Procedural gate: run `cargo +stable test --workspace --test cdx_regression --test spdx_regression --test spdx3_regression` to confirm the existing 33 golden fixtures stay byte-identical post-milestone-136 (SC-004 invariant). No test-file edits; this task is just CI-gate confirmation.

**Checkpoint**: US1 ships independently — operators scanning Apple Silicon macOS developer machines get full Homebrew formula coverage in their SBOMs. The known soft regression (no file-claim integration → `pkg:generic/curl` duplicates on Linuxbrew rootfs scans where the binary walker fires) is acceptable per spec Out-of-Scope and is documented in the quickstart's "Known soft regression" section. Can be merged as a standalone PR.

---

## Phase 4: User Story 2 — Operator scans Intel macOS or Linuxbrew installations (Priority: P2)

**Goal**: Intel macOS (`/usr/local/Cellar/`) and Linuxbrew (`/home/linuxbrew/.linuxbrew/Cellar/`) get the same component PURLs as Apple Silicon. The reader detects all three prefixes independently; the install location does NOT leak into the PURL identity.

**Independent Test** (from spec SC-002 + US2 acceptance scenarios): Three synthetic fixture variants, one per prefix. Same formula in each. Assert identical PURL emission.

**Dependency**: US1 must complete first (US2 is test-coverage validation of US1's three-prefix iteration logic; no new reader code).

### Tests (US2)

- [X] T014 [US2] Create `mikebom-cli/tests/brew_alternate_prefixes.rs` with one test per prefix: Intel macOS (`<tmp>/usr/local/Cellar/curl-8.5.0/INSTALL_RECEIPT.json`), Linuxbrew (`<tmp>/home/linuxbrew/.linuxbrew/Cellar/curl-8.5.0/INSTALL_RECEIPT.json`). Each asserts the emitted PURL is `pkg:brew/curl@8.5.0` (no prefix-dependent variation — IDENTICAL to the Apple Silicon emission per SC-002).
- [X] T015 [US2] Add to the same file the `/usr/local`-without-`Cellar/` non-match test: tempdir with `<tmp>/usr/local/share/README.txt` (a NON-ELF file — avoids spurious binary-walker emission per analysis-finding U2) but no `/usr/local/Cellar/`. Assert ZERO `pkg:brew/*` components emit (use the specific PURL-prefix filter rather than total component count, so any unrelated walker output doesn't pollute the assertion). `/usr/local/` alone is not a Homebrew signal per FR-001.
- [X] T016 [US2] Add to the same file a multi-prefix coexistence test: tempdir with BOTH `/opt/homebrew/Cellar/curl-8.5.0/` AND `/usr/local/Cellar/curl-8.5.0/` (pathological hybrid). Assert exactly ONE `pkg:brew/curl@8.5.0` component emits — the standard `seen_purls` dedup at `package_db/mod.rs:~1042` collapses PURL-identical entries from different prefixes.
- [X] T016b [US2] Add cross-reader coexistence test (closes analysis-finding C1 / FR-009 / US2 acceptance scenario 2): construct a synthetic Linuxbrew-on-Debian rootfs — `<tmp>/home/linuxbrew/.linuxbrew/Cellar/curl-8.5.0/INSTALL_RECEIPT.json` PLUS a synthetic dpkg DB at `<tmp>/var/lib/dpkg/status` declaring at least one unrelated package (e.g., `bash 5.2.15-2+b8`). Scan. Assert: (a) the brew component `pkg:brew/curl@8.5.0` emits, (b) the dpkg component `pkg:deb/debian/bash@5.2.15-2+b8?arch=amd64` (or similar) emits, (c) both surface in the same SBOM — neither reader suppresses the other.

**Checkpoint**: US2 ships as a follow-up PR. Intel macOS + Linuxbrew users get verified parity with Apple Silicon. The data-model code change footprint is zero (T008 already handles all three prefixes); US2 is essentially a test-coverage expansion that validates cross-prefix behavior end-to-end.

---

## Phase 5: User Story 3 — Operator scans GUI app installations via Cask (Priority: P3)

**Goal**: macOS Casks (GUI apps installed via `brew install --cask`) emit as `pkg:brew/<token>@<version>?type=cask` components. Modern Homebrew 4.0+ JSON-backed casks parse cleanly; pre-4.0 `.rb`-only casks warn-and-skip per Principle I.

**Independent Test** (from spec SC-003): Synthetic fixture with one Cask under `<tmp>/opt/homebrew/Caskroom/visual-studio-code/1.95.3/.metadata/<version>/<timestamp>/Casks/visual-studio-code.json`. Scan. Assert a `pkg:brew/visual-studio-code@1.95.3?type=cask` component emits with no dep edges.

**Dependency**: US1 must complete first (US3 extends the dispatcher integration to also walk Caskroom). Independent of US2 — US3 can ship before or after US2.

### Reader-side (cask parsing)

- [X] T017 [US3] In `mikebom-cli/src/scan_fs/package_db/brew.rs`, implement `read_casks(rootfs: &Path, prefix: &str) -> Vec<PackageDbEntry>` per data-model.md §"CaskMetadata" + research §R3. Walk `<rootfs>/<prefix>/Caskroom/<token>/<version>/.metadata/` and look for `Casks/<token>.json` at the nested `.metadata/<version>/<timestamp>/Casks/` path. When found: `parse_cask_metadata` + convert to `PackageDbEntry` with `evidence_kind = Some("brew-cask-metadata")` and the `BrewKind::Cask` PURL discriminator. When `.json` absent but `Casks/<token>.rb` present: emit `tracing::warn!("brew: cask {token} at {path} has only Ruby-DSL metadata (no Casks/{token}.json); skipping — Ruby parsing is out of scope per Constitution Principle I")` and skip. When the `.metadata/` directory is empty entirely: skip silently (Homebrew sentinel for uninstalled-but-not-cleaned-up cask). Add unit tests covering: well-formed cask, .rb-only warn-and-skip, empty `.metadata/` silent skip.
- [X] T018 [US3] In `read()` (T008), invoke `read_casks(rootfs, prefix)` for each of the three prefixes alongside `read_formulae` and aggregate the combined output.

### Tests (US3)

- [X] T019 [US3] Create `mikebom-cli/tests/brew_casks.rs` containing the SC-003 acceptance test. Construct a synthetic Cask under `<tmp>/opt/homebrew/Caskroom/visual-studio-code/1.95.3/.metadata/1.95.3/20251001120000.000/Casks/visual-studio-code.json` per the format in research §R3. Run `mikebom sbom scan`. Assert: (a) exactly one component emits with PURL `pkg:brew/visual-studio-code@1.95.3?type=cask`, (b) the component has no `dependsOn` edges (casks emit without dep graph per FR-005), (c) the component carries `mikebom:source-type = "brew"` + `mikebom:evidence-kind = "brew-cask-metadata"` annotations.
- [X] T020 [US3] Add to the same file the Ruby-DSL-only warn-and-skip test: synthetic cask with `.rb` but no `.json` at the `Casks/<token>.*` path. Assert: (a) zero components emit for that cask, (b) stderr contains `WARN` line mentioning the cask name AND `Ruby-DSL`.
- [X] T021 [US3] Add to the same file the formula + cask coexistence test: tempdir with ONE formula in `Cellar/` AND ONE cask in `Caskroom/`. Assert both emit (2 components total) with correct PURLs distinguishable by the `type=cask` qualifier.

**Checkpoint**: US3 ships as the third PR. macOS users get GUI app inventory alongside CLI tool inventory. Ruby-DSL casks (pre-4.0 installs) are transparently skipped with operator-visible diagnostics.

---

## Phase 6: Polish

**Purpose**: Edge-case hardening + CHANGELOG + mandatory pre-PR gate.

- [X] T022 [P] Create `mikebom-cli/tests/brew_edge_cases.rs` covering: (a) malformed `INSTALL_RECEIPT.json` alongside three valid formulae → SC-005's exit-0 + 3 components + warn naming the broken formula; (b) receipt missing `runtime_dependencies` array entirely → component emits with empty `depends` list (older-receipt compat); (c) receipt with `source.tap = null` → emits without `tap=` qualifier; (d) multi-version formula coexistence (`openssl@1.1-1.1.1w` + `openssl@3-3.4.0` in same Cellar) → both emit as distinct components; (e) empty Cellar dir (exists but no formula subdirs) → silent no-op, no warnings; (f) third-party tap end-to-end (formula installed from `hashicorp/tap`) → PURL has `?tap=hashicorp/tap` qualifier.
- [X] T023 Update `CHANGELOG.md` with a milestone-136 entry under the `[Unreleased]` section. Cite #432 as the closing issue. Mention: (a) `pkg:brew/*` native PURL emission across CDX/SPDX 2.3/SPDX 3, (b) three-prefix detection (Apple Silicon, Intel macOS, Linuxbrew), (c) `tap=` qualifier convention for non-default taps, (d) cask support with `type=cask` discriminator, (e) Ruby-DSL casks warn-and-skip per Principle I, (f) file-claim integration deferred (known soft regression documented), (g) `brew` PURL type informal pending purl-spec extension follow-up, (h) license emission deferred (license not in `INSTALL_RECEIPT.json` — cross-reader follow-up tracked alongside milestone-135's FR-012 URL deferral).
- [X] T024 Run the mandatory pre-PR gate per `CLAUDE.md`: `./scripts/pre-pr.sh` (which runs `cargo +stable clippy --workspace --all-targets -- -D warnings` followed by `cargo +stable test --workspace`). Both MUST report zero errors / `0 failed`. If clippy flags any lints, fix them locally before pushing — `feedback-clippy-before-async-patterns` memory note applies.
- [X] T025 Open the PR — single-PR for the whole milestone per the milestone-134 / milestone-135 precedent (US1+US2+US3 are tightly coupled; splitting into three PRs creates pointless churn). PR title: `feat(scan_fs/package_db): Homebrew (brew + Linuxbrew) reader (closes #432)`.
- [X] T026 Post-merge follow-up: file a sibling issue proposing `brew` (or `homebrew`) PURL type for [package-url/purl-spec PURL-TYPES.rst](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst). Reference this milestone + the de-facto convergence with syft + cyclonedx-bom-gen. NOT a code change; just an upstream issue.

---

## Dependencies

```text
Phase 1 (Setup)                  → Phase 2
Phase 2 (Foundational)           → Phase 3 (US1)  [REQUIRED — module + parsers + PURL builder + evidence-kind enum must exist before reader wiring]
Phase 3 (US1, P1)                → Phase 4 (US2, P2)   [US2 is test-coverage validation of US1's three-prefix logic]
Phase 3 (US1, P1)                → Phase 5 (US3, P3)   [US3 extends US1's read() with cask walking]
Phase 4 (US2)                    ⫪ Phase 5 (US3)        [US2 and US3 are independent — either order]
Phase 5 (US3)                    → Phase 6 (Polish)
```

## Parallel-execution opportunities

Within Phase 2 (Foundational), once T002 + T002b + T003 land:

```text
T004 (InstallReceipt parser)   |
T005 (CaskMetadata parser)     | — different concerns within the same file, can write concurrently
T006 (build_brew_purl helper)  |
```

Within Phase 3 (US1), once T007 + T008 + T009 land:

```text
T010 (Apple Silicon baseline test) |
T011 (tap qualifier test)          | — independent test sections within the same file
T012 (no-Homebrew regression test) |
T013 (regression-goldens gate)     | — procedural CI gate; independent of above
```

Within Phase 5 (US3):

```text
T019 (cask emission test)       |
T020 (Ruby-DSL warn-and-skip)   | — independent test sections within the same file
T021 (formula+cask coexistence) |
```

Within Phase 6, all polish tasks except T024 (pre-PR gate) and T025 (PR open) can run in parallel:

```text
T022 (edge cases test file)     |
T023 (CHANGELOG entry)          | — independent files; can write in parallel
```

## MVP scope

**Phase 3 (US1, P1) alone is the shipping MVP slice.** It delivers the headline value: operators scanning Apple Silicon macOS developer machines (the dominant target) get full Homebrew formula coverage in their SBOMs. The `pkg:brew/curl@8.5.0` PURL is the unambiguous identity carrier; cross-format parity rides on the native PURL field.

US2 and US3 are strictly additive — neither is required for US1 to ship useful capability:

- **US2** (Intel + Linuxbrew prefix detection) is test-coverage validation; the code path is identical to US1's three-prefix iteration. Splitting US2 from US1 in implementation is artificial — they ship together in practice.
- **US3** (Casks) is a meaningfully different code path (separate metadata format, separate dispatcher invocation, Ruby-DSL warn-and-skip). Naturally splittable into a follow-up PR.

In practice, the milestone-134 + milestone-135 pattern (US1+US2+US3 + polish bundled in one PR) is the right model for milestone 136 too — see T025.

## Format validation

All 28 tasks follow the strict checklist format: `- [ ] T<NNN> [P?] [Story?] Description with file path`. Setup (T001), Foundational (T002, T002b, T003–T006), and Polish (T022–T026) tasks omit the story label per spec. User-story phases (T007–T021 + T016b) carry the [US1] / [US2] / [US3] labels. Parallelizable tasks across independent files are marked [P].
