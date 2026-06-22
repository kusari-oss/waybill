# Feature Specification: Arch Linux pacman/alpm package database reader

**Feature Branch**: `135-arch-alpm-reader`
**Created**: 2026-06-22
**Status**: Draft
**Input**: User description: "let's look at arch/alpm linux"

## Background

mikebom currently has installed-package-database readers for dpkg (Debian/Ubuntu), apk (Alpine), rpm (RHEL family), and opkg (Yocto/embedded). It has none for **pacman/alpm**, the package manager of Arch Linux and its derivatives.

That gap means operators scanning Arch-based images get an SBOM with **zero** OS-level components — every package installed via pacman is invisible to the scan. The binary walker may then emit `pkg:generic/*` entries for binaries that ARE pacman-owned (a typical false-positive pattern on Arch images today).

Arch and its derivatives are increasingly visible in production targets:

- **Container base images** (`archlinux:latest` on Docker Hub has substantial pull volume; Wolfi/Chainguard-style "stripped" Arch derivatives are appearing)
- **Steam Deck** ships SteamOS, an Arch derivative — game-distribution SBOMs land here
- **CachyOS** and similar performance-tuned variants in HPC / gaming infrastructure
- **Manjaro** desktop installations being inventoried for IT-asset audit
- **Arch-on-WSL** developer-workstation scans

This feature closes the gap: an operator scanning any of these targets gets a complete, accurate SBOM with every pacman-installed component represented.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator gets a complete SBOM of an Arch container image (Priority: P1) 🎯 MVP

An operator pulls `archlinux:latest`, runs `mikebom sbom scan --image archlinux:latest`, and receives an SBOM containing one component per package that `pacman -Q` would list inside the running container. Each component carries the canonical `pkg:alpm/arch/<name>@<version>?arch=<arch>` PURL identity and accurate dep edges.

**Why this priority**: The headline use case. Without it, the entire feature has no operator value — every other story (derivatives, file-claim) is a refinement.

**Independent Test**: Build a small synthetic pacman DB fixture (3–5 packages with deps and an `arch=x86_64` field), run `mikebom sbom scan` against a rootfs containing it, and assert the emitted SBOM lists exactly those packages with correct PURLs + dep relationships.

**Acceptance Scenarios**:

1. **Given** a rootfs containing `/var/lib/pacman/local/glibc-2.40-1/desc` declaring `%NAME%` / `%VERSION%` / `%ARCH%` / `%DEPENDS%`, **When** the operator runs `mikebom sbom scan --path <rootfs>`, **Then** the emitted SBOM contains a component with PURL `pkg:alpm/arch/glibc@2.40-1?arch=x86_64` and its declared dependency edges.
2. **Given** the same scan, **When** the operator inspects the emitted SBOM's components, **Then** the distro identity reflects Arch (`arch` as the namespace on the alpm-derived PURLs).
3. **Given** a rootfs WITHOUT `/var/lib/pacman/local/`, **When** the operator scans, **Then** no pacman-related components or annotations appear AND no warning fires (clean no-op).

---

### User Story 2 — Operator scans an Arch derivative and gets the correct distro namespace (Priority: P2)

A SteamOS, Manjaro, EndeavourOS, or CachyOS rootfs uses the same pacman DB format but identifies as a different distro via `/etc/os-release` (`ID=steamos`, `ID=manjaro`, etc.). The operator wants the PURL namespace to reflect the actual distro — `pkg:alpm/steamos/<name>@<version>` not `pkg:alpm/arch/...` — so downstream vulnerability matching and license tracking attribute findings to the right ecosystem.

**Why this priority**: Important for SteamOS (large user base) and Manjaro (broadly deployed) operators, but the Arch base case is the foundation; this is a refinement that does NOT block US1 shipping.

**Independent Test**: Synthetic fixture with `/etc/os-release` containing `ID=steamos\nVERSION_ID=3.5.7` and the same pacman DB shape. Scan. Assert PURLs are `pkg:alpm/steamos/...` and the components carry a `distro=steamos-3.5.7` qualifier consistent with the existing dpkg/apk/rpm pattern.

**Acceptance Scenarios**:

1. **Given** a rootfs whose `/etc/os-release` declares `ID=steamos` and `VERSION_ID=3.5.7`, **When** the operator scans, **Then** every alpm-derived component PURL carries the `steamos` namespace and a `distro=steamos-3.5.7` qualifier.
2. **Given** a rootfs declaring `ID=manjaro` and `VERSION_ID=24.0.0`, **When** the operator scans, **Then** the PURL namespace is `manjaro` and the qualifier is `distro=manjaro-24.0.0`.
3. **Given** a stock Arch rootfs (no `VERSION_ID` — Arch is rolling-release), **When** the operator scans, **Then** PURLs use the `arch` namespace AND omit the `distro=` qualifier entirely.
4. **Given** a rootfs declaring an unknown derivative (`ID=mydistro` that is not in the recognized derivative set), **When** the operator scans, **Then** PURLs use whatever `ID` value was declared as the namespace verbatim (no hardcoded list of "blessed" derivatives) so future distros work without code changes.

---

### User Story 3 — Binary walker skips pacman-claimed files (Priority: P3)

The mikebom binary walker enumerates ELF files in the rootfs and emits `pkg:generic/*` components for unclaimed binaries. After pacman support lands, binaries owned by pacman packages (e.g., `/usr/bin/bash` from the `bash` package) should be excluded from generic-binary emission — they belong to their owning package, not a free-floating generic component. This mirrors what dpkg / apk / rpm already do via the file-claim tracker.

**Why this priority**: Quality-of-output refinement — without it, the SBOM has duplicate entries (a `pkg:alpm/arch/bash` AND a `pkg:generic/bash`) which makes downstream consumption noisier. Ship-blocking for production polish; deferrable for an MVP slice.

**Independent Test**: Synthetic fixture with `/var/lib/pacman/local/bash-5.2-1/files` declaring `usr/bin/bash` as an owned path, plus a real `bash` binary at that path. Scan. Assert the emitted SBOM contains exactly one component for `bash` — the pacman one — and no `pkg:generic/bash` entry.

**Acceptance Scenarios**:

1. **Given** a rootfs where pacman's `files` manifest declares ownership of `/usr/bin/bash`, **When** the operator scans, **Then** the emitted SBOM contains exactly one `bash` component (the alpm one) — no duplicate `pkg:generic/bash` entry.
2. **Given** an ELF binary at a path NOT claimed by any pacman package, **When** the operator scans, **Then** that binary still surfaces via the generic-binary walker (file-claim only suppresses claimed paths; unclaimed binaries continue to emit per the existing milestone-004 behavior).

---

### Edge Cases

- **Rolling-release version absence**: stock Arch has no `VERSION_ID` in `/etc/os-release`. The reader MUST emit alpm-derived PURLs without a `distro=` qualifier in this case (do NOT invent a synthetic version).
- **Missing or empty pacman DB**: a rootfs containing `/var/lib/pacman/` but with no `local/` subdir, or a `local/` directory containing no package subdirs, MUST be treated as "no pacman packages" — no error, no warning, no annotation.
- **Malformed desc file**: a single corrupted `desc` file MUST NOT abort the whole scan — log a warning naming the affected package and skip it; continue parsing the rest.
- **Same package name, multiple versions in `local/`**: pacman enforces one version per name on a live system, but archive scans or partial chroots may contain multiple. The reader emits each as a candidate `PackageDbEntry`; entries with distinct PURLs (different version strings — e.g., `bash-5.2.026-1` vs `bash-5.2.026-2`) survive as separate components per the standard `seen_purls` dedup at `package_db/mod.rs:~1042`. PURL-identical entries (same name + version in two different `local/` directories — pathological and not pacman-compliant) collapse via the existing dedup and MAY trigger milestone-134's divergent-PURL annotation if their declared dep sets disagree.
- **Optional dependencies** (`%OPTDEPENDS%`): MUST NOT participate in the main dependency graph. They MAY be surfaced as evidence-tier metadata for transparency.
- **Foreign / locally-built / AUR packages**: pacman's DB does not natively distinguish "official Arch repo" from "AUR / locally-built". The reader treats them identically; the PURL identity carries no provenance distinction at this stage.
- **Group packages**: pacman supports group entries (e.g., `base-devel`); these are aliases, not real packages. The reader MUST emit only real packages, not group aliases.
- **Architecture `any`**: noarch packages declare `%ARCH%` as `any`. The PURL qualifier MUST reflect this value verbatim (`?arch=any`).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect a pacman-managed rootfs by the presence of `/var/lib/pacman/local/` containing per-package subdirectories (named `<pkg>-<ver>`).
- **FR-002**: System MUST parse each per-package `desc` file (stanza format with `%KEY%` headers) and extract at minimum: `%NAME%`, `%VERSION%`, `%ARCH%`, `%DEPENDS%`, `%LICENSE%`, `%URL%`, `%PACKAGER%`, `%DESC%`, `%REPLACES%`, `%CONFLICTS%`, `%PROVIDES%`.
- **FR-003**: System MUST emit one component per parsed package with PURL of the form `pkg:alpm/<distro-namespace>/<package-name>@<version>?arch=<architecture>` per the purl-spec `alpm` type.
- **FR-004**: System MUST derive the distro namespace from `/etc/os-release` `ID` value (verbatim, lowercased). When `/etc/os-release` is absent or its `ID` is empty, default to `arch`.
- **FR-005**: When `/etc/os-release` contains a non-empty `VERSION_ID`, system MUST emit a `distro=<namespace>-<version-id>` qualifier on each alpm-derived component PURL. When `VERSION_ID` is absent, the qualifier MUST be omitted entirely.
- **FR-006**: System MUST emit dependency edges from each component to the components named in its `%DEPENDS%` list. `%OPTDEPENDS%` MUST NOT participate in the dependency graph but MAY surface as a separate evidence-tier annotation.
- **FR-007**: System MUST register every file path declared in each package's `files` manifest in the cross-reader file-claim tracker, so the binary walker skips emission of `pkg:generic/*` components for paths owned by a pacman package (mirrors the dpkg/apk/rpm behavior introduced in milestone 004).
- **FR-008**: System MUST treat a missing pacman DB (`/var/lib/pacman/local/` absent or empty) as a clean no-op — no components emitted, no warnings logged, no annotations attached. Existing dpkg/apk/rpm-only and binary-only scans MUST stay byte-identical pre/post this feature on rootfs scans that contain no pacman DB.
- **FR-009**: System MUST tolerate per-package parse errors (malformed `desc`, missing required fields, encoding issues) without aborting the whole scan — log a structured warning naming the affected package and continue.
- **FR-010**: System MUST recognize at minimum the following derivative distro IDs and route them through the same code path as plain Arch: `arch`, `manjaro`, `endeavouros`, `steamos`, `cachyos`. Unrecognized IDs MUST still produce a working scan — the `ID` value is used verbatim as the PURL namespace (no allowlist gate).
- **FR-011**: System MUST surface declared package licenses through the existing `licenses[]` field on the emitted component, applying the same SPDX-canonicalization rules used by the dpkg/apk/rpm readers.
- **FR-012** *(deferred — out of milestone-135 scope)*: Surfacing the declared `%URL%` (homepage) as a wire-level external reference is deferred to a follow-up. Existing dpkg/apk/rpm/opkg readers do not surface URL/Homepage today; adding it requires a cross-cutting OS-reader enhancement out of scope here. The alpm reader MUST parse `%URL%` into the in-memory `PacmanDescStanza.homepage` for future use, but MUST NOT emit it on the wire in this milestone.
- **FR-013**: System MUST NOT make any network calls during the scan — the pacman DB is fully self-contained on the rootfs (matches the constitution's offline-by-default expectation for OS package readers).

### Key Entities

- **Pacman installed package**: A unit recorded under `/var/lib/pacman/local/<name>-<version>/`. Attributes: name, version, architecture, dependency list, optional-dependency list, license expression, homepage URL, packager identity, replaces / conflicts / provides lists, owned file paths.
- **Distro identity**: The `(ID, VERSION_ID)` pair from `/etc/os-release`. Drives the PURL namespace and the optional `distro=` qualifier. Arch is rolling-release and lacks `VERSION_ID`; derivatives typically have one.
- **File-ownership claim**: The set of filesystem paths owned by a package, recorded in its `files` manifest. Drives binary-walker dedup so OS-managed binaries do not get re-emitted as `pkg:generic/*`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of the stock `archlinux:latest` container produces a CycloneDX 1.6 SBOM whose alpm component count matches the count reported by `pacman -Q` executed inside the same image, modulo packages installed mid-scan. (Steady-state expectation: exact match.)
- **SC-002**: A scan of a SteamOS rootfs fixture produces every alpm component with the `pkg:alpm/steamos/...` PURL namespace AND the `distro=steamos-<VERSION_ID>` qualifier present on every one. No PURL leaks the `arch` namespace on a SteamOS scan.
- **SC-003**: A rootfs containing neither a pacman DB nor any pacman-style packages produces an SBOM byte-identical (modulo timestamps + serial numbers) to a pre-feature baseline scan of the same rootfs. (No-op preservation invariant.)
- **SC-004**: A scan of a fixture where a known pacman package owns `/usr/bin/X` (and X is a real ELF binary at that path) emits exactly one component for X — the alpm one. No `pkg:generic/X` duplicate appears.
- **SC-005**: A scan completes successfully (exit code 0, valid SBOM) on a fixture with a single deliberately-corrupted `desc` file alongside three valid packages. The output contains the three valid components plus a warning naming the corrupted package; the corrupted package is silently dropped from the component list.
- **SC-006**: An external SBOM consumer reading the emitted CDX JSON can enumerate every pacman-installed package via the standard `components[]` array filtered on `purl =~ "^pkg:alpm/"`. No alpm-specific consumer code is required — the standard PURL filter works.

## Assumptions

- **Pacman DB v9 format**: the on-disk layout of `/var/lib/pacman/local/<pkg>-<ver>/` has been stable since pacman 4.0 (~2012). The reader targets the current format; pre-4.0 DBs are out of scope.
- **No live pacman invocation**: the reader parses the on-disk DB directly. It does NOT shell out to `pacman -Q` or `expac` — neither tool is guaranteed present on the scanned rootfs (same posture the dpkg/apk/rpm readers take).
- **SQLite cache files ignored**: pacman maintains `/var/lib/pacman/sync/*.db` cache files for the remote repos; these describe AVAILABLE packages, not INSTALLED ones. They are explicitly out of scope — only the `local/` directory is parsed.
- **AUR vs official-repo provenance is not preserved**: pacman's DB does not natively distinguish where a package came from. The reader treats all installed packages identically; provenance enrichment (if needed later) is a follow-up that would read `/etc/pacman.conf` repo configuration.
- **Package signatures are not validated**: signature verification is pacman's job at install time; the reader treats the on-disk state as authoritative.
- **Group packages are not first-class**: groups (`base-devel`, etc.) are aliases for sets of real packages. Only the real packages emit as components.
- **Soft conflicts with derivative-distro detection**: SteamOS in particular has been known to ship custom `/etc/os-release` overrides in different deployment modes (Steam Deck Game Mode vs Desktop Mode); the reader takes `/etc/os-release` verbatim and does NOT attempt to second-guess.
- **Existing milestone-002 OS-reader pattern is the template**: the reader will share architectural shape with the dpkg reader (closest sibling) — discovery → stanza parse → component emission → file-claim registration.

## Out of Scope

- **Live invocation of `pacman` or any pacman client tool**: read-only DB parse only.
- **AUR-specific provenance**: distinguishing official-repo vs AUR-built packages requires reading `/etc/pacman.conf` or `pacman.log`; deferred.
- **Pre-pacman-4.0 DB formats**: legacy `desc` formats from pacman 3.x are not supported.
- **Sync DB / available-package enumeration**: only installed packages (`local/`) are in scope. Available-but-not-installed packages from `sync/*.db` are explicitly excluded.
- **Pacman hooks** (`/usr/share/libalpm/hooks/`): hook scripts are runtime behavior, not declared dependencies.
- **Signature verification**: pacman's signature check is performed at install time; the reader does not re-verify on the scanned rootfs.
- **Mirroring derivative-distro CVE feeds**: this feature only emits SBOMs. Vulnerability matching is a separate concern.
- **A `mikebom alpm`-namespaced subcommand**: alpm is an OS-package reader and runs as part of the existing `mikebom sbom scan` pipeline; no new top-level subcommand is added.
- **Surfacing `%URL%` (homepage) as a wire-level external reference**: parsed into the reader's intermediate representation, but not emitted. Tracked as a follow-up that should cover dpkg/apk/rpm/opkg simultaneously (cross-reader change). File as a sibling issue to #429 if/when picked up.

## Dependencies and Constraints

- **Builds on milestone 002** (initial OS package-DB reader architecture).
- **Builds on milestone 004** (file-claim tracker that gates binary-walker emission).
- **Reuses the existing `/etc/os-release` reader** — no new os-release parsing logic.
- **Does NOT touch the existing dpkg, apk, rpm, or opkg readers** — alpm support is strictly additive.
- **Does NOT introduce new external dependencies** — `desc` and `files` are plain text stanza formats parseable with stdlib + existing crates.

## Related

- Closes: #429 (Add Arch Linux pacman/alpm package database reader)
- Adjacent: #430 (Gentoo portage reader) — sibling distro-package-DB issue
- Adjacent: #432 (Homebrew detection) — adjacent niche-distro package manager
- Foundational reference: milestone 002 (dpkg/apk/rpm initial readers), milestone 004 (file-claim tracker)
