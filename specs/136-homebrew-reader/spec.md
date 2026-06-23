# Feature Specification: Homebrew (brew + Linuxbrew) package detection

**Feature Branch**: `136-homebrew-reader`
**Created**: 2026-06-22
**Status**: Draft
**Input**: User description: "let's look at homebrew"

## Background

mikebom has installed-package-database readers for dpkg, apk, rpm, opkg, and (as of milestone 135) alpm. It has none for **Homebrew**, the dominant macOS developer-machine package manager and an increasingly common installer on Linux (as `linuxbrew`).

The gap means scanning a developer laptop, a CI runner that uses Homebrew, an IT-asset audit of macOS-fleet machines, or any rootfs that contains a Homebrew install produces an SBOM with **zero** Homebrew-installed components. Every CLI tool installed via `brew install`, every dependency Homebrew managed, every GUI app installed as a Cask — invisible.

Three deployment shapes are in play:

- **Apple Silicon macOS**: prefix is `/opt/homebrew`. The default since macOS 11 + Apple Silicon (~2020). Dominates new developer machines.
- **Intel macOS**: prefix is `/usr/local`. The default for pre-2020 macOS installs. Still common on existing machines.
- **Linuxbrew** (Homebrew on Linux): prefix is `/home/linuxbrew/.linuxbrew`. Used on CI runners, in HPC contexts, and by developers who prefer Homebrew's formula ecosystem over distro packages.

This feature closes the gap so an operator scanning any of these targets gets the full Homebrew-installed inventory in their SBOM.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator scans an Apple Silicon macOS developer machine (Priority: P1) 🎯 MVP

A developer runs `mikebom sbom scan --path /` on their MacBook (or scans `/opt/homebrew` directly). They receive an SBOM containing one component per installed Homebrew formula. Each component carries a `pkg:brew/<formula>@<version>` PURL identity, the formula's tap (when not `homebrew/core`), and the formula's declared `runtime_dependencies` as dep edges.

**Why this priority**: Apple Silicon is the dominant new developer-machine target. Without this, the headline macOS use case is empty.

**Independent Test** (SC-001): Synthetic fixture with 3–5 formula directories under `<tmp>/opt/homebrew/Cellar/<formula>/<version>/INSTALL_RECEIPT.json`. Run `mikebom sbom scan --path <tmp>`. Assert the emitted SBOM contains exactly those components with correct PURLs + dep edges.

**Acceptance Scenarios**:

1. **Given** an Apple Silicon macOS rootfs with `/opt/homebrew/Cellar/curl/8.5.0/INSTALL_RECEIPT.json` declaring `runtime_dependencies` against `openssl@3`, **When** the operator runs `mikebom sbom scan --path <rootfs>`, **Then** the emitted SBOM contains a `pkg:brew/curl@8.5.0` component with a dependsOn edge targeting `pkg:brew/openssl@3@<version>`.
2. **Given** the same rootfs, **When** the operator inspects the emitted SBOM, **Then** each Homebrew component carries provenance evidence identifying its source-tap (default `homebrew/core` omitted as a qualifier; non-default taps like `homebrew/cask-fonts` surfaced as a `tap=` qualifier).
3. **Given** a rootfs WITHOUT a `/opt/homebrew/Cellar/` directory, **When** the operator scans, **Then** no Homebrew-related components or annotations appear AND no warning fires (clean no-op).

---

### User Story 2 — Operator scans Intel macOS or Linuxbrew installations (Priority: P2)

The same operator workflow as US1, but the Homebrew prefix is `/usr/local` (Intel macOS) or `/home/linuxbrew/.linuxbrew` (Linux). The reader must detect ALL three prefix locations independently and produce correct components from any combination that exists on the scanned rootfs.

**Why this priority**: Intel macOS still has substantial installed base (pre-2020 machines, dual-architecture CI). Linuxbrew is concentrated but real (HPC, CI runners, opinionated Linux developers). Important for parity but does NOT block the Apple Silicon MVP.

**Independent Test** (SC-002): Three synthetic fixtures, one per prefix location. Each contains the same minimal formula. Assert the emitted SBOM has the same component PURL regardless of which prefix the formula was found at (the PURL identity doesn't encode the install prefix).

**Acceptance Scenarios**:

1. **Given** an Intel macOS rootfs with a formula installed under `/usr/local/Cellar/`, **When** the operator scans, **Then** the formula emits as a `pkg:brew/<name>@<version>` component (no prefix-dependent variation in the PURL).
2. **Given** a Linux rootfs with Homebrew installed at `/home/linuxbrew/.linuxbrew/Cellar/`, **When** the operator scans, **Then** Homebrew-managed components emit alongside any dpkg/apk/rpm packages on the same rootfs (cross-reader coexistence per the alpm-reader / dpkg-reader / opkg-reader pattern).
3. **Given** a rootfs where `/usr/local/` exists but contains NO `Cellar/` subdirectory (typical Linux distro `/usr/local` for sysadmin-installed binaries, NOT Homebrew), **When** the operator scans, **Then** no Homebrew components emit — `/usr/local/` alone is not a Homebrew signal.
4. **Given** a hybrid rootfs with Homebrew installed at multiple prefixes simultaneously (pathological — Apple Silicon machine with both `/opt/homebrew` and a leftover `/usr/local/Cellar/` from a pre-migration install), **When** the operator scans, **Then** each prefix is processed independently; same-name-same-version formulae from both prefixes collapse via the standard PURL-key dedup at `package_db/mod.rs`.

---

### User Story 3 — Operator scans GUI app installations via Cask (Priority: P3)

A macOS user installs GUI apps via `brew install --cask <app>`. The cask metadata lives at `<prefix>/Caskroom/<cask>/<version>/.metadata/` with a different shape from formula `INSTALL_RECEIPT.json` — no transitive deps, but identity (name, version, install date) and the cask definition file.

**Why this priority**: Casks are macOS-only and have a fundamentally different data model from formulae (GUI app installers with no dep trees). They're a meaningful operator concern (Slack, VS Code, Docker Desktop, browsers all install as casks) but the formula slice in US1 is the headline value. Defer casks so the macOS-CLI baseline ships independently.

**Independent Test** (SC-003): Synthetic fixture with one cask under `<tmp>/opt/homebrew/Caskroom/visual-studio-code/1.95.3/.metadata/`. Scan. Assert a `pkg:brew/visual-studio-code@1.95.3?type=cask` component emits.

**Acceptance Scenarios**:

1. **Given** an Apple Silicon rootfs with a cask installed under `/opt/homebrew/Caskroom/visual-studio-code/`, **When** the operator scans, **Then** a Cask component emits with a `type=cask` PURL qualifier distinguishing it from formula components.
2. **Given** the same rootfs, **When** the operator inspects the cask component, **Then** it carries no dep edges (casks have no `runtime_dependencies` in their metadata format) and the `mikebom:source-type` evidence reflects the cask shape.

---

### Edge Cases

- **Multiple keg versions of one formula**: Homebrew supports multiple installed versions per formula (`openssl@1.1` and `openssl@3` both present). Each version has its own `Cellar/<name>/<version>/` directory and its own `INSTALL_RECEIPT.json`. Each MUST emit as a separate component (distinct PURLs by version).
- **Pinned formulae**: a `<prefix>/.pinned/<formula>` marker file indicates the formula is pinned to prevent auto-upgrade. Informational; surfaces as evidence-tier metadata if surfaced at all (out of scope for v1).
- **Keg-only formulae** (`keg_only` flag in `INSTALL_RECEIPT.json`): the formula is not symlinked into `<prefix>/bin/`. Identity is unaffected; the keg-only flag is informational only.
- **Third-party taps**: a formula from `<user>/<tap>` (e.g., `mongodb/brew/mongodb-community`) instead of the default `homebrew/core`. The receipt's `source.tap` field carries the origin tap. Non-default taps MUST surface in the PURL as a `tap=<owner>/<tap>` qualifier (downstream consumers care about tap provenance for supply-chain risk).
- **Tap formulae with `/` in the name**: tap-namespaced formula names use `/` separators in CLI invocation (`brew install mongodb/brew/mongodb-community`). On disk, only the formula name half (`mongodb-community`) appears in `Cellar/`. The tap origin lives in the receipt's `source.tap`, not the directory name.
- **Custom HOMEBREW_PREFIX**: an operator may install Homebrew at a non-standard prefix via `HOMEBREW_PREFIX` env var. Such installs are out of scope for v1 — only the three documented standard prefixes are detected.
- **Malformed `INSTALL_RECEIPT.json`**: per-formula JSON-parse failures MUST warn-and-skip without aborting the whole scan; partial output is preserved.
- **Missing `INSTALL_RECEIPT.json`**: very old Homebrew installs that predate the install-receipt format lack this file. The reader skips such directories (the directory exists but has no parseable identity); no component emits.
- **Empty Cellar dir**: `<prefix>/Cellar/` exists but contains no formulae (fresh Homebrew install, or all formulae uninstalled). No components emit; no warnings fire.
- **No Homebrew at all**: none of the three prefix locations contain a `Cellar/` subdirectory. Clean no-op — no warnings, no annotations, byte-identical SBOM output to pre-feature baseline.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect Homebrew installations by checking the three standard prefix locations independently: `<rootfs>/opt/homebrew/Cellar/`, `<rootfs>/usr/local/Cellar/`, and `<rootfs>/home/linuxbrew/.linuxbrew/Cellar/`. The presence of the `Cellar/` subdirectory is the discrimination signal — `<prefix>/` alone is not (especially for `/usr/local/` which is a generic Linux sysadmin path).
- **FR-002**: System MUST parse each `<prefix>/Cellar/<formula>/<version>/INSTALL_RECEIPT.json` file and extract at minimum: formula name (from the directory name), version (from the directory name), `runtime_dependencies` (array of dep specs each carrying `full_name` + `version`), `source.tap` (originating tap, default `homebrew/core`), and `source.spec` (typically `stable`).
- **FR-003**: System MUST emit one component per parsed formula with PURL of the form `pkg:brew/<formula-name>@<version>[?tap=<tap-owner>/<tap-name>]`. The `tap=` qualifier MUST be omitted when the tap is `homebrew/core` (default) and MUST be present otherwise.
- **FR-004**: System MUST emit dependency edges from each formula to the formulae named in its `runtime_dependencies` array. The dep target's PURL MUST be resolved using the same naming convention so dependsOn edges target real emitted components.
- **FR-005**: For each Cask discovered under `<prefix>/Caskroom/<cask>/<version>/.metadata/`, system MUST emit a component with PURL `pkg:brew/<cask-name>@<version>?type=cask` distinguishing it from formula components. Casks MUST NOT participate in the formula dep graph (the cask metadata format carries no transitive deps).
- **FR-006**: System MUST treat the absence of all three Homebrew prefixes as a clean no-op — no components emitted, no warnings logged, no annotations attached. Existing dpkg/apk/rpm/alpm-only and binary-only scans MUST stay byte-identical pre/post this feature on rootfs scans that contain no Homebrew install.
- **FR-007**: System MUST tolerate per-formula and per-cask parse errors (malformed JSON, missing required fields, encoding issues) without aborting the whole scan — log a structured warning naming the affected formula/cask and continue.
- **FR-008**: System MUST emit multiple components when the same formula has multiple installed versions (`<prefix>/Cellar/openssl@1.1/1.1.1w/` and `<prefix>/Cellar/openssl@3/3.4.0/` both present). Each version gets its own PURL (distinct via the version segment) and its own component.
- **FR-009**: System MUST coexist independently with the other OS package-DB readers (dpkg, apk, rpm, alpm, opkg). On a Linuxbrew-on-Debian rootfs, both deb-managed and Homebrew-managed components emit cooperatively; neither suppresses the other.
- **FR-010**: System MUST NOT make any network calls during the scan — `INSTALL_RECEIPT.json` and Cask metadata are fully self-contained on the rootfs (offline-by-default posture matching every other OS package reader).
- **FR-011** *(deferred — out of milestone-136 scope)*: License surfacing on emitted brew components is deferred to a follow-up. Per research §R2, `INSTALL_RECEIPT.json` does NOT carry license information — it lives in the formula's `.rb` source (Ruby DSL — Principle I conflict) or in the `formulae.brew.sh` JSON API (network call — FR-010 conflict). The reader MUST emit brew components with an empty `licenses[]` field; cross-reader license enrichment is a separate concern parallel to the milestone-135 FR-012 URL deferral.

### Key Entities

- **Homebrew installation**: A populated `Cellar/` (or `Caskroom/`) directory under one of the three standard prefixes. Each `Cellar/<formula>/<version>/` directory describes one installed formula; each `Caskroom/<cask>/<version>/` directory describes one installed cask.
- **Formula**: A CLI tool or library installed via `brew install`. Carries identity (name + version), declared runtime dependencies, source tap, and an install receipt (`INSTALL_RECEIPT.json`) with full provenance.
- **Cask**: A GUI app installer (or other non-formula installable) installed via `brew install --cask`. Different metadata shape from a formula — no `runtime_dependencies`; identity comes from `Caskroom/<name>/<version>/.metadata/<name>.rb`.
- **Tap**: The git repository providing a formula or cask. Default for formulae: `homebrew/core`; default for casks: `homebrew/cask`. Third-party taps (`mongodb/brew`, `hashicorp/tap`, etc.) are a supply-chain concern and surface in the PURL `tap=` qualifier.
- **Install prefix**: One of the three standard locations (`/opt/homebrew`, `/usr/local`, `/home/linuxbrew/.linuxbrew`). The prefix is a discovery signal; the resulting PURL identity does NOT encode the prefix (a `curl@8.5.0` formula has the same PURL whether installed under `/opt/homebrew` or `/usr/local`).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of a synthetic Apple Silicon Homebrew install with 3+ formulae (one with declared `runtime_dependencies`) produces a CDX SBOM whose Homebrew component count matches the fixture count exactly, and the dependsOn relationships from the receipt are present in the SBOM's `dependencies[]` block targeting real bom-refs.
- **SC-002**: A scan of a rootfs containing Homebrew at one of the alternate prefixes (`/usr/local/Cellar/` Intel macOS OR `/home/linuxbrew/.linuxbrew/Cellar/` Linux) produces the same component PURLs as the equivalent Apple-Silicon install (the prefix MUST NOT leak into the PURL identity).
- **SC-003**: A scan of a synthetic Cask fixture produces a `pkg:brew/<name>@<version>?type=cask` component with the `type=cask` qualifier present and no dep edges.
- **SC-004**: A rootfs containing none of the three Homebrew prefixes' `Cellar/` directories produces an SBOM byte-identical (modulo timestamps + serial numbers) to a pre-feature baseline scan of the same rootfs. (No-op preservation invariant — protects every non-macOS / non-Homebrew scan.)
- **SC-005**: A scan completes successfully (exit code 0, valid SBOM) on a fixture where one formula has a deliberately-corrupted `INSTALL_RECEIPT.json` alongside three valid formulae. The output contains the three valid components plus a warning naming the corrupted formula; the corrupted formula is silently dropped.
- **SC-006**: An external SBOM consumer reading the emitted CDX JSON can enumerate every Homebrew-installed component via the standard `components[]` array filtered on `purl =~ "^pkg:brew/"`. No Homebrew-specific consumer code is required — the standard PURL filter works.
- **SC-007**: A formula installed from a non-default tap (e.g., `hashicorp/tap/terraform`) emits a PURL containing `?tap=hashicorp/tap` (or `&tap=...` when combined with other qualifiers). A formula from the default `homebrew/core` tap MUST NOT carry a `tap=` qualifier (consumer queries on default-tap installs stay stable across the Homebrew namespace evolution).

## Assumptions

- **Standard prefixes only**: `HOMEBREW_PREFIX` env-var overrides and other non-default install locations are out of scope for v1. The three documented prefixes cover the realistic 99%+ of installs.
- **`INSTALL_RECEIPT.json` is the authoritative source**: this file has been the receipt format since at least 2011 (per research §R2) and is universally present on modern Homebrew installs. Installs old enough to predate the receipt format are out of scope.
- **The `pkg:brew/` PURL type is informal**: the purl-spec does not currently define a `brew` (or `homebrew`) type. mikebom emits `pkg:brew/...` per industry convention; a follow-up issue should propose extending the purl-spec. Parallels milestone 128's Yocto `pkg:yocto` informal extension and the existing informal `pkg:alpm/` (which IS in purl-spec — alpm has it; brew does not yet).
- **No live `brew` invocation**: the reader parses on-disk metadata directly. It does NOT shell out to `brew list` or `brew info` — Homebrew may not be runnable on the scan host (mikebom is host-portable; the scanned target may be a Linux rootfs scanned on macOS or vice versa). Matches the dpkg/apk/rpm/alpm posture.
- **Cask metadata is shallower than formulae**: casks have no transitive dependency information in their on-disk metadata (`<cask>.rb` is a Ruby DSL that could in principle declare deps but conventionally doesn't). The cask component emits without dep edges.
- **Soft conflicts with derivative taps**: SteamOS, Linuxbrew, and various Linux-side derivatives may ship custom default taps; the reader takes `source.tap` from the receipt verbatim and applies the `homebrew/core` omission rule literally (FR-003).
- **File-claim tracker integration is out of scope for v1**: Homebrew's symlink-heavy bottling (binaries in `<prefix>/Cellar/<formula>/<ver>/bin/<bin>` symlinked into `<prefix>/bin/<bin>`) makes file-claim integration meaningfully more complex than the alpm reader's flat `%FILES%` parse. Deferred to a follow-up that can handle Homebrew's symlink resolution properly.
- **Existing milestone-002 OS-reader pattern is the template**: the reader will share architectural shape with `dpkg.rs` / `alpm.rs` (closest siblings) — discovery → per-package parse → component emission.

## Out of Scope

- **Live invocation of `brew` or any brew client tool**: read-only metadata parse only.
- **`HOMEBREW_PREFIX` env-var override detection**: only the three documented standard prefixes (`/opt/homebrew`, `/usr/local`, `/home/linuxbrew/.linuxbrew`) are scanned.
- **Very old Homebrew installs that predate the install-receipt format (introduced ≥2011 per research §R2)**: deferred indefinitely (these are exceptionally rare in 2026 production environments — modern installs since at least 2011 universally carry `INSTALL_RECEIPT.json`).
- **File-claim tracker integration**: deferred to a follow-up due to Homebrew's symlink-heavy install topology. Without this, the binary walker may emit `pkg:generic/<binary>` duplicates alongside `pkg:brew/<formula>` components — a known soft regression.
- **Pinned-formula marker surfacing**: `<prefix>/.pinned/<formula>` indicators are informational; not emitted in v1.
- **Cask runtime-dependency extraction**: while Cask DSL can in principle declare `depends_on formula:` clauses, parsing the Ruby DSL is out of scope. Casks emit without dep edges.
- **Brew bundle (`Brewfile`) parsing**: Brewfiles are declarative manifests separate from the install state. They could be a separate design-tier reader but are not part of this milestone.
- **Tap source-URL emission**: the receipt's `source.tap` is captured as a PURL qualifier; the underlying tap's git URL is NOT emitted as a separate external reference in v1.
- **A `mikebom brew`-namespaced subcommand**: Homebrew is an OS-package-style reader that runs as part of `mikebom sbom scan`; no new top-level subcommand is added.
- **License surfacing on brew components**: license metadata is NOT carried by `INSTALL_RECEIPT.json` (research §R2); extracting it requires either Ruby DSL parsing of the formula `.rb` source (Principle I conflict) or `formulae.brew.sh` JSON API calls (FR-010 conflict). Deferred to a follow-up issue. Parallels milestone-135's FR-012 URL/homepage deferral.

## Dependencies and Constraints

- **Builds on milestone 002** (initial OS package-DB reader architecture).
- **Builds on milestone 135** (alpm reader — most recent OS-reader prior art; same shape).
- **Reuses the existing JSON parsing infrastructure** (`serde_json` already a workspace dep).
- **Does NOT touch the existing dpkg, apk, rpm, opkg, or alpm readers** — brew support is strictly additive.
- **Does NOT introduce new external dependencies** — `INSTALL_RECEIPT.json` is plain JSON parseable with the existing workspace `serde_json`.
- **Does NOT integrate with the milestone-004 file-claim tracker** in v1 (deferred follow-up per Assumptions / Out of Scope).

## Related

- Closes: #432 (Add Homebrew package detection (Cellar/*/INSTALL_RECEIPT.json))
- Adjacent: #429 (Arch alpm reader — same OS-package-DB reader pattern, shipped as milestone 135)
- Adjacent: #430 (Gentoo portage reader — another niche-distro package manager)
- Foundational reference: milestone 002 (dpkg/apk/rpm initial readers), milestone 135 (alpm reader — closest sibling)
- Purl-spec extension: a follow-up issue should propose adding a native `brew` (or `homebrew`) PURL type to the purl-spec (currently mikebom emits `pkg:brew/...` informally pending spec adoption).
