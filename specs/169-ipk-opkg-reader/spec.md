# Feature Specification: ipk/opkg package-database reader (Yocto + OpenWrt)

**Feature Branch**: `169-ipk-opkg-reader`
**Created**: 2026-07-06
**Status**: Draft
**Input**: Empirically-filed bug from external testbed (issue #500) — a Yocto build with `PACKAGE_CLASSES = "package_ipk"` produces `tmp/deploy/ipk/{all,core2-64,qemux86_64}/*.ipk` containing **4587 `.ipk` files** (36 installed on the image, ~4550 build-only). `mikebom sbom scan --offline --format cyclonedx-json --path tmp/deploy/ipk/ --output out.json` runs to completion but emits **0 components**. The file-tier walker's shape-check filter reports `shape_skipped=4584` — every `.ipk` file the walker sees is filtered because `.ipk` is not in the recognized-artifact suffix allowlist. Same Yocto build coordinates (scarthgap LTS, poky `802e4c1135c4eb451e504996aa797c04736496d4`, `core-image-minimal`, `MACHINE=qemux86-64`) as milestones 001/002/003 which validated the RPM path cleanly (4585 components emitted). This milestone closes the ipk-format coverage gap.

## Background

mikebom currently has installed-package-database readers for dpkg (Debian/Ubuntu), apk (Alpine), rpm (RHEL family), alpm (Arch), and Homebrew. It has **no reader for ipk/opkg**, the package format used by:

- **Yocto/OpenEmbedded** builds when `PACKAGE_CLASSES = "package_ipk"` is set — the default for embedded targets that prioritize small image size.
- **OpenWrt** — the router-firmware project. Every OpenWrt release publishes ~2000+ `.ipk` packages per architecture; embedded-router SBOM inventory is a real ask from network-equipment vendors.
- **Legacy Yocto derivatives** where the vendor chose ipk over rpm/deb for historical or footprint reasons.

That gap means operators scanning any of these targets get an SBOM with **zero** ipk-derived components. The gap is a hard cliff: the file-tier walker silently skips every `.ipk` file it encounters (`shape_skipped=<N>` in the walker-complete tracing log per m133), so the operator has no in-band signal that mikebom made a coverage choice — they simply get 0 components and (in the CDX case) an empty `components[]` array.

The ipk format shape shares the same INNER structure as `.deb` but uses a different OUTER envelope:

- **Outer archive**: `gzip( tar { debian-binary, control.tar.gz, data.tar.gz } )` — a gzipped tarball. **Empirically verified against OpenWrt 23.05.5 x86_64 base feed fixtures during Phase 1 implementation (2026-07-06)**. NOT the `ar` format the spec's initial draft assumed based on issue #500's "subset of Debian's `.deb` format" description — modern `opkg-build` (per Yocto's `opkg-utils` and OpenWrt's package build system) switched from ar to gzipped-tarball years ago. This is a happy discovery: it eliminates the need for hand-rolled ar parsing entirely; the existing `flate2` + `tar` workspace deps cover the outer format.
- **Metadata**: `control.tar.gz` inside the outer tarball (same as `.deb`'s `control.tar.gz`, though opkg's control-file dialect uses a stricter subset of Debian's field vocabulary — no `Multi-Arch`, no `Priority: extra`, tighter `Depends` syntax).
- **Payload**: `data.tar.gz` inside the outer tarball (same as `.deb`).
- **Marker file**: `debian-binary` inside the outer tarball (contains `2.0\n` — a format-version marker; parser can ignore).
- **Filename convention**: `<Package>_<Version>-<Release>_<Architecture>.ipk` (near-identical to `.deb`'s `_` and `-` layout).
- **PURL**: purl-spec defines `pkg:opkg/<name>@<version>?arch=<arch>` as the canonical ipk PURL (documented at https://github.com/package-url/purl-spec/blob/master/PURL-TYPES.rst#opkg).

Because the ipk archive layout is a near-subset of `.deb`, this milestone can share the existing ar-archive parsing helper the deb reader already exposes. The control-file parse routine may be shared too (opkg is a strict subset syntactically; safe superset consumption) with a small ipk-specific wrapper that normalizes opkg-specific fields.

Additionally, an ipk-installed system (Yocto/OpenWrt runtime rootfs) maintains an **installed-package database** at `/var/lib/opkg/`:

- `/var/lib/opkg/status` — single-file summary of installed packages (Debian-status-file format: name + version + arch + license + description + depends per package, blank-line separated).
- `/var/lib/opkg/info/<name>.control` — per-package control files (same shape as the control file inside an ipk archive).
- `/var/lib/opkg/info/<name>.list` — per-package installed-file lists (one path per line — the same info a `data.tar.gz` payload lists but pre-extracted).

Reading the installed DB is a distinct code path from the archive-file reader (no ar-extraction needed; the control files are plain text on disk) but the emitted `pkg:opkg/*` PURL identity + `mikebom:evidence-kind` value + license routing are shared. Per clarification 2026-07-06 Q1, m169 covers BOTH code paths — matching the m004 rpm precedent (`rpm.rs` for installed DB + `rpm_file.rs` for archive files bundled in one milestone) and giving Yocto/OpenWrt operators the same runtime-scan completeness they get on Debian/Alpine/RHEL/Arch systems.

## Clarifications

### Session 2026-07-06

- Q: Does m169 cover ONLY archive-file `.ipk` scanning, OR does it ALSO cover the installed opkg DB at `/var/lib/opkg/`? → A: **Both archive-files AND installed opkg DB** (Option B). Matches m004 rpm precedent (`rpm.rs` + `rpm_file.rs` bundled). Gives Yocto/OpenWrt runtime-rootfs scans the same completeness as Debian/Alpine/RHEL/Arch runtime scans. Wider scope (~25-30 tasks) but cleaner ecosystem story. The two code paths share PURL identity + evidence-kind + license routing but have distinct entry points.

- Q: When the reader sees `Depends: pkg-a | pkg-b` alternative-list syntax, does it emit edges to both alternatives, only the first, or first-with-annotation? → A: **First alternative only + `mikebom:dep-alternative-alternates` annotation listing the fallbacks** (Option C). Matches opkg's runtime default (first-listed wins if the resolver has no other constraint) — does NOT inflate BFS reachability by pretending both are reached. Preserves fallback visibility for consumers who want it via the annotation. Rolls back the current Edge Cases claim that said "emit dep edges to both alternatives" — that would be graph-reachability-inflating.

- Q: How is the SC-001 4580-component empirical claim verified — locally-runnable Yocto build attested in PR body, scaled synthetic fixture in test-fixtures repo, or real Yocto artifact committed to sibling repo? → A: **Maintainer re-runs Yocto build locally pre-merge + attaches reproduction log to PR body per SC-011** (Option A). Matches m165 audit-milestone pattern (K8s + ArgoCD scans were maintainer-runnable, not CI-runnable). CI covers 12 unit tests + 1 integration test against 3-5 vendored real ipks per SC-009 + SC-010. SC-001's 4580-component threshold is a PR-body attestation, not a CI-time assertion. Minimal repo bloat; no ~500 MB fixture commitment.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator gets a non-empty SBOM from a Yocto ipk build directory (Priority: P1) 🎯 MVP

An operator scans a Yocto build's `tmp/deploy/ipk/` directory with `mikebom sbom scan --path tmp/deploy/ipk/` and receives an SBOM containing one component per `.ipk` file the walker sees. Each component carries the canonical `pkg:opkg/<name>@<version>?arch=<arch>` PURL identity and honest per-package license + description metadata extracted from the ipk's control file.

**Why this priority**: The headline use case. Without it, the entire feature has no operator value — every other story (OpenWrt package feeds, control-file license extraction, dep-edge emission) is a refinement.

**Independent Test**: Build a small synthetic ipk fixture (2–3 `.ipk` files with distinct name/version/arch triples), run `mikebom sbom scan --path <ipk-dir>`, and assert the emitted SBOM lists exactly those packages with correct `pkg:opkg/...` PURLs.

**Acceptance Scenarios**:

1. **Given** a directory containing `busybox_1.36.1-r0_core2-64.ipk`, `glibc_2.39-r0_core2-64.ipk`, `zlib_1.3.1-r0_core2-64.ipk`, **When** the operator runs `mikebom sbom scan --path <dir>`, **Then** the emitted SBOM contains 3 components with PURLs `pkg:opkg/busybox@1.36.1-r0?arch=core2-64`, `pkg:opkg/glibc@2.39-r0?arch=core2-64`, `pkg:opkg/zlib@1.3.1-r0?arch=core2-64`.
2. **Given** the same scan, **When** the operator inspects the `file-tier walker complete` tracing log line, **Then** `shape_skipped` shows 0 (down from the pre-fix N, where N equals the ipk-file count) — the walker no longer silently drops ipk archives.
3. **Given** the same scan, **When** the operator inspects the CDX / SPDX 2.3 / SPDX 3 outputs, **Then** each emitted component carries `name`, `version`, `licenses[]`, and `description` extracted from the ipk's `control.tar.gz`'s `control` file when present.
4. **Given** a directory with **zero** `.ipk` files, **When** the operator scans, **Then** no ipk-related components appear AND no warning fires (clean no-op, backward-compatible with pre-169 behavior on non-ipk trees).

---

### User Story 2 — Operator gets a complete SBOM of a Yocto/OpenWrt runtime rootfs from `/var/lib/opkg/` (Priority: P1) 🎯 co-MVP

An operator scans a Yocto/OpenWrt runtime rootfs (either mounted at a scan-path or extracted from a container image) with `mikebom sbom scan --path <rootfs>`, and receives an SBOM containing one component per package that `opkg list-installed` would list inside the running system. Each component's identity is derived from `/var/lib/opkg/status` (single-file summary) or `/var/lib/opkg/info/<name>.control` (per-package fallback). Per-package installed-file lists (`/var/lib/opkg/info/<name>.list`) drive the binary-walker dedup path per US4.

**Why this priority**: Parity with dpkg/apk/rpm/alpm/homebrew installed-DB readers. Without it, an operator scanning a Yocto/OpenWrt runtime rootfs (the most common ipk deployment scenario in production — routers, embedded devices, in-flight OTA update targets) gets zero components even though the archive-file reader (US1) succeeds on build-output artifacts. Co-P1 with US1 per Q1 clarification.

**Independent Test**: Build a small synthetic runtime-rootfs fixture (`<fixture>/var/lib/opkg/status` declaring 2-3 packages + `<fixture>/var/lib/opkg/info/*.control` files matching), run `mikebom sbom scan --path <fixture>`, and assert the emitted SBOM lists exactly those packages with correct `pkg:opkg/...` PURLs.

**Acceptance Scenarios**:

1. **Given** a rootfs containing `/var/lib/opkg/status` declaring 3 packages (busybox 1.36.1-r0, glibc 2.39-r0, zlib 1.3.1-r0) with `Architecture:`, `License:`, `Depends:` fields, **When** the operator runs `mikebom sbom scan --path <rootfs>`, **Then** the emitted SBOM contains 3 components with PURLs `pkg:opkg/busybox@1.36.1-r0?arch=core2-64`, `pkg:opkg/glibc@2.39-r0?arch=core2-64`, `pkg:opkg/zlib@1.3.1-r0?arch=core2-64`.
2. **Given** the same scan, **When** the operator inspects each emitted component's `mikebom:evidence-kind` annotation, **Then** the value is `opkg-status-db` (parity with rpm's `rpmdb-sqlite`) — distinguishing installed-DB emissions from archive-file emissions.
3. **Given** a rootfs where `/var/lib/opkg/status` is missing but `/var/lib/opkg/info/*.control` files are present, **When** the operator scans, **Then** the reader falls back to enumerating `info/*.control` files as the authoritative source AND fires a tracing `INFO` line noting the fallback.
4. **Given** a rootfs WITHOUT `/var/lib/opkg/` (non-opkg system), **When** the operator scans, **Then** no opkg-related components appear AND no warning fires (clean no-op).
5. **Given** a rootfs containing BOTH `/var/lib/opkg/` installed DB AND `tmp/deploy/ipk/*.ipk` archive files (mixed build-and-runtime scenario), **When** the operator scans, **Then** components dedup by `(name, version, arch)` PURL — one emission per unique tuple regardless of source, with a preference for the installed-DB source's evidence-kind when both fire.

---

### User Story 3 — Filename-only fallback when the ipk archive is malformed or truncated (Priority: P2)

An operator scans a Yocto build where a subset of `.ipk` files failed to build cleanly — the archive exists but its `control.tar.gz` is missing, truncated, or unreadable. The operator still gets a component per `.ipk` file, with name/version/architecture extracted from the filename alone (per Yocto/OpenWrt's mandatory `<name>_<version>_<arch>.ipk` convention). The component is missing license/description/dep info but its PURL is correct and dedupable.

**Why this priority**: Robustness. Prevents the "one bad archive skips everything" failure mode. Also gives partial value on ipk repositories where the operator only has file listings (e.g., a cached OpenWrt feed index), not the full archive contents.

**Independent Test**: Corrupt an `.ipk` file's archive body (truncate to 0 bytes but preserve the filename), scan, and assert a component is still emitted with correct PURL derived from the filename. Verify a tracing warning fires per corrupted file (per Constitution Principle X — no silent drops).

**Acceptance Scenarios**:

1. **Given** an `.ipk` file whose ar archive is truncated / malformed AND the filename is well-formed, **When** the operator scans, **Then** a component IS emitted with PURL derived from the filename AND a tracing `WARN` line fires naming the file + the parse-failure class.
2. **Given** an `.ipk` file whose filename does NOT match the `<name>_<version>_<arch>.ipk` convention, **When** the operator scans, **Then** the file is skipped with a tracing `WARN` naming the file + the parse-failure class. No component is silently invented.
3. **Given** an ipk directory mixing well-formed archives + malformed archives + filename-non-conforming files, **When** the operator scans, **Then** the SBOM contains a component for each well-formed OR filename-parseable ipk, and the number of tracing `WARN` lines equals the number of skipped files.

---

### User Story 4 — Binary walker skips ipk-claimed files (Priority: P3)

The binary walker (m104) skips any on-disk file whose path is claimed by an ipk `data.tar.gz` payload (US1 archive-file source) OR by an `/var/lib/opkg/info/<name>.list` entry (US2 installed-DB source). This prevents the "duplicate emission" pattern where the same binary (e.g., `/usr/bin/busybox`) would otherwise emit as both `pkg:opkg/busybox@...` AND `pkg:generic/...` from binary-tier analysis.

**Why this priority**: Correctness at scale. Without it, an operator scanning a Yocto rootfs installed FROM ipk artifacts would see thousands of duplicate `pkg:generic/*` binary-tier components alongside the same components' `pkg:opkg/*` package-tier identities. Symmetric with the existing `.deb`/`.rpm`/`.apk`/pacman deduplication.

**Independent Test**: Build a synthetic fixture with one `.ipk` file that owns `/usr/bin/busybox` (declared in its `data.tar.gz`) AND a rootfs at `<fixture>/rootfs/usr/bin/busybox`. Scan `<fixture>`. Assert exactly ONE component is emitted for `busybox` (the `pkg:opkg/*` one), NOT two. Symmetric test with `/var/lib/opkg/info/busybox.list` naming `/usr/bin/busybox` for the installed-DB path.

**Acceptance Scenarios**:

1. **Given** a fixture with an `.ipk` owning `/usr/bin/busybox` AND a rootfs at `<fixture>/rootfs/usr/bin/busybox`, **When** the operator scans, **Then** exactly one component is emitted for busybox (the `pkg:opkg/*` one).
2. **Given** the same scan, **When** the operator inspects the `mikebom:evidence-kind` annotations, **Then** the busybox component carries the ipk-package-db evidence-kind, NOT a binary-tier one.
3. **Given** a fixture with `/var/lib/opkg/info/busybox.list` declaring `/usr/bin/busybox` AND a rootfs at `<fixture>/usr/bin/busybox`, **When** the operator scans, **Then** exactly one component is emitted (the installed-DB `pkg:opkg/*` one).

---

### User Story 5 — Distro-namespace + qualifier reflects Yocto vs OpenWrt provenance (Priority: P4)

An operator scanning a Yocto rootfs (with `/etc/os-release` declaring `ID=poky`) vs an OpenWrt rootfs (declaring `ID=openwrt`) gets PURLs whose distro qualifier reflects the actual distro — so downstream vulnerability matching + license auditing attributes findings to the right ecosystem.

**Why this priority**: Refinement — important for operators who ship multiple distros from the same build infrastructure, but the ipk-base case (US1) is the foundation; distro-qualifier propagation does NOT block US1 shipping.

**Independent Test**: Synthetic fixtures with `/etc/os-release` containing `ID=poky` vs `ID=openwrt`. Assert PURLs carry `distro=poky-<VERSION_ID>` vs `distro=openwrt-<VERSION_ID>` qualifiers respectively.

**Acceptance Scenarios**:

1. **Given** a rootfs whose `/etc/os-release` declares `ID=poky` and `VERSION_ID=5.0`, **When** the operator scans, **Then** every `pkg:opkg/*` component PURL carries a `distro=poky-5.0` qualifier.
2. **Given** a rootfs declaring `ID=openwrt` and `VERSION_ID=23.05.5`, **When** the operator scans, **Then** the qualifier is `distro=openwrt-23.05.5`.
3. **Given** an ipk-directory scan with NO `/etc/os-release` in the scanned path (headless ipk-repository case), **When** the operator scans, **Then** PURLs omit the `distro=` qualifier entirely — no hardcoded default.

---

### Edge Cases

- **Empty ipk directory**: 0 components, 0 warnings — same as US1 AS4.
- **Non-`.ipk` files in the same directory**: e.g., `Packages` (opkg feed index), `Packages.gz`, `README`, checksums. The reader ignores non-`.ipk` files (no warnings).
- **ipk file with duplicate copies at different paths**: dedup by PURL — one component per `(name, version, arch)` tuple regardless of path multiplicity.
- **ipk `Depends:` field with alternative-list syntax** (`Depends: pkg-a | pkg-b`): per clarification 2026-07-06 Q2, emit an edge only to the FIRST-listed alternative (`pkg-a`) — matches opkg's runtime default. Attach a `mikebom:dep-alternative-alternates` annotation on the source component naming the fallback alternatives (`["pkg-b"]` in this example) so consumers can see the alternate options without BFS reachability being inflated.
- **ipk `Provides:` virtual packages**: emit as a `mikebom:provides` annotation on the providing component; do NOT emit ghost components for the virtual names alone.
- **Non-ASCII bytes in the control file** (rare — opkg allows UTF-8 in `Description`): preserve UTF-8 verbatim; do not lossy-decode.
- **ipk file over the m069 rpm-size cap (16 MB uncompressed control.tar.gz)**: honor a symmetric size cap; emit a component with filename-only metadata + a `mikebom:archive-size-skipped` annotation.
- **License expression variations**: opkg control files declare `License:` as an SPDX expression OR a free-form label (per-recipe author choice). The reader emits SPDX-canonical when possible (via `SpdxExpression::try_canonical`); falls back to `LicenseRef-*` per the m152/153/154 sweep pattern when non-canonical.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: mikebom's file-tier walker (m133) MUST add `.ipk` to its recognized-artifact suffix allowlist so `.ipk` files are handed to the ipk reader instead of `shape_skipped`.
- **FR-002**: The ipk reader MUST parse the outer `.tar.gz` envelope (empirically-verified format per Background — NOT ar), locate `control.tar.gz` and `data.tar.gz` entries within the outer tarball, and extract the control file from the gzipped tarball inside.
- **FR-003**: The ipk reader MUST extract `Package`, `Version`, `Architecture`, `License`, `Description`, `Depends`, `Provides`, `Source` fields from the control file when present.
- **FR-004**: The ipk reader MUST emit one `ResolvedComponent` per unique `(Package, Version, Architecture)` tuple with PURL `pkg:opkg/<Package>@<Version>?arch=<Architecture>` per purl-spec's opkg type.
- **FR-005**: The ipk reader MUST emit `Depends:` field entries as `dependsOn` relationships between components (dedup on the same-scan set; skip dangling references to packages not seen in the scan). For alternative-list syntax `Depends: pkg-a | pkg-b`, per clarification 2026-07-06 Q2, the reader MUST emit an edge only to the FIRST alternative (`pkg-a`) — matching opkg's runtime default — AND attach a `mikebom:dep-alternative-alternates` annotation on the source component naming the fallback alternatives (e.g., `["pkg-b"]`). This preserves alternate-option visibility without inflating BFS reachability (m158 invariant preserved).
- **FR-006**: When the ipk archive is malformed / truncated / unreadable AND the filename matches `<name>_<version>_<arch>.ipk`, the reader MUST fall back to filename-only PURL construction (per US2). A tracing `WARN` line MUST fire naming the file + the parse-failure class per Constitution Principle X.
- **FR-007**: The reader MUST NOT skip an `.ipk` file silently. Every skipped file MUST fire a tracing `WARN` (per Principle X + the m133 walker's existing skip-counter convention).
- **FR-008**: License expressions from the ipk control file MUST route through the existing m152/153/154 SPDX license-canonicalization pipeline — `SpdxExpression::try_canonical` first, `LicenseRef-<hash>` fallback per the milestone-152 escape hatch.
- **FR-009**: The reader MUST emit `mikebom:evidence-kind = "ipk-file"` annotation on each component (parity with existing per-format evidence-kind values like `rpm-file`, `rpmdb-sqlite` from milestone 004).
- **FR-010**: When `/etc/os-release` is present in the scanned path, the reader MUST propagate the distro identity (`ID` + `VERSION_ID`) into a `distro=<ID>-<VERSION_ID>` PURL qualifier per US4. When absent, the qualifier is omitted.
- **FR-011**: The binary walker (m104) MUST skip on-disk files claimed by an ipk `data.tar.gz` payload's file list (per US1) OR by an `/var/lib/opkg/info/<name>.list` entry (per US2/US4). Deduplication is by canonical path.
- **FR-012**: The reader MUST honor an archive-size cap symmetric with m069's rpm-size cap (default 16 MB uncompressed control.tar.gz). Over-cap ipks emit filename-only components with a `mikebom:archive-size-skipped` annotation.

### Installed-DB requirements (US2 per clarification 2026-07-06 Q1)

- **FR-013**: When the scanned path contains `/var/lib/opkg/status`, mikebom MUST parse it as a Debian-status-file-format sequence of blank-line-separated per-package stanzas. Each stanza's `Package`, `Version`, `Architecture`, `License`, `Description`, `Depends`, `Provides`, `Source` fields MUST populate a `ResolvedComponent` per FR-003 + FR-004 (same PURL shape, license routing, evidence-kind — but per FR-015 the evidence-kind value differs).
- **FR-014**: When `/var/lib/opkg/status` is absent AND `/var/lib/opkg/info/<name>.control` files are present, the reader MUST fall back to enumerating those per-package control files as the authoritative source. An `INFO`-level tracing line MUST fire noting the fallback.
- **FR-015**: Installed-DB emissions MUST carry `mikebom:evidence-kind = "opkg-status-db"` (parity with `rpmdb-sqlite` from milestone 004) to distinguish them from archive-file emissions (`ipk-file` per FR-009).
- **FR-016**: When BOTH `/var/lib/opkg/` installed DB AND `.ipk` archive files are present in the scanned tree (mixed build-and-runtime scenario), components MUST dedup by `(name, version, arch)` PURL. When both sources produce the same PURL, the installed-DB source's evidence-kind + license take precedence (installed state is more authoritative than a build-cache archive).
- **FR-017**: `/var/lib/opkg/info/<name>.list` files (per-package installed-file lists) MUST feed the binary-walker skip set per FR-011.

### Key Entities

- **`.ipk` archive** (US1 archive-file source): an `ar`-format container with two top-level entries — `control.tar.gz` (metadata) and `data.tar.gz` (payload).
- **`/var/lib/opkg/status`** (US2 installed-DB primary source): Debian-status-file-format sequence of blank-line-separated per-package stanzas. Single-file summary of installed packages.
- **`/var/lib/opkg/info/<name>.control`** (US2 installed-DB fallback source): per-package control files. Same shape as the control file inside an ipk archive. Used when `/var/lib/opkg/status` is absent.
- **`/var/lib/opkg/info/<name>.list`** (US4 binary-walker skip-set source): plain-text file-lists (one path per line) of the installed files each package owns.
- **`control` file**: Debian-format key-value declarations — fields: `Package`, `Version`, `Architecture`, `License`, `Description`, `Depends`, `Provides`, `Source`, plus opkg-specific extensions (Section vocabulary differs from Debian; ignored by the reader).
- **`pkg:opkg/<name>@<version>?arch=<arch>` PURL**: the canonical ipk component identity per purl-spec — shared across archive-file (US1) + installed-DB (US2) emissions.
- **ipk feed index** (`Packages`, `Packages.gz`): opkg-metadata files that the reader IGNORES (not per-package, not a manifest for scanning purposes).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001 (0-component cliff eliminated)**: `mikebom sbom scan --path <dir with .ipk files>` MUST emit ≥ 1 component per `.ipk` file the walker sees. The specific reproduction from issue #500 — scanning `tmp/deploy/ipk/` from a Yocto scarthgap `core-image-minimal` build with 4587 `.ipk` files — MUST emit **≥ 4580 components** (small tolerance for malformed/truncated ipks). Per clarification 2026-07-06 Q3, this threshold is a **maintainer-attested PR-body claim** (matches m165 audit-milestone pattern) — the reproducing maintainer re-runs the Yocto build locally, scans, and attaches the component count + walker-complete log to the PR body per SC-011. **NOT** a CI-time assertion — the ~500 MB scaled fixture would bloat the repo without proportional signal. CI coverage of the code path lives in SC-009 unit tests + SC-010 integration test using 3-5 vendored real ipks.

- **SC-002 (PURL fidelity)**: Every emitted `pkg:opkg/*` PURL MUST match the purl-spec opkg type: `pkg:opkg/<name>@<version>?arch=<arch>`. Zero empty-version PURLs (m164 invariant preserved). Zero phantom edges (m163 invariant preserved).

- **SC-003 (license extraction)**: On the Yocto scarthgap `core-image-minimal` reproduction, at least **80%** of emitted components MUST carry a non-empty `licenses[]` field extracted from their control-file `License:` declaration. Missing 20% acceptable because opkg-side historically permits recipes with empty License fields; those emit with `NOASSERTION`.

- **SC-004 (dep edges)**: Emitted `dependsOn` relationships MUST match the `Depends:` field of the source ipk control file — for alternative-list syntax, only the FIRST alternative is emitted as an edge per FR-005 (Q2 clarification); the remaining alternatives ride on `mikebom:dep-alternative-alternates` annotations. On the Yocto reproduction, at least **90%** of emitted components MUST carry ≥ 1 dep edge (headline test: `busybox` emits an edge to `libc-mbedtls` if declared).

- **SC-005 (binary-walker dedup)**: On a Yocto rootfs scan that mounts the ipk-installed image contents at a subdir, mikebom MUST emit exactly one component per `(package, version, arch)` tuple — never both a `pkg:opkg/*` package-tier AND a `pkg:generic/*` binary-tier for the same file. Dedup MUST fire from BOTH the archive-file skip-set (US1 `data.tar.gz` payload lists) AND the installed-DB skip-set (US2 `/var/lib/opkg/info/*.list` files). Zero duplicate emissions.

- **SC-005b (installed-DB coverage)**: On a Yocto/OpenWrt runtime rootfs scan (path containing `/var/lib/opkg/status` and `/var/lib/opkg/info/`), mikebom MUST emit ≥ 1 component per package the `opkg list-installed` command would list on that system. Empirical anchor: a synthetic runtime-rootfs fixture with 36 packages (matching the 36-installed-on-image count from issue #500's core-image-minimal reference) MUST emit ≥ 36 components with `mikebom:evidence-kind = "opkg-status-db"`.

- **SC-006 (SPDX conformance)**: Emitted SPDX 2.3 output MUST pass the existing `jsonschema` gate. Emitted SPDX 3.0.1 output MUST pass `spdx3-validate==0.0.5`. Both on the Yocto reproduction + on synthetic fixtures.

- **SC-007 (pre-PR gate)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace --no-fail-fast` MUST both pass with zero errors.

- **SC-008 (dual-side byte-identity for non-ipk ecosystems)**: For every milestone-090 fixture whose ecosystem is NOT ipk (apk, deb, rpm, cargo, gem, maven, cmake, bazel, pip, golang, npm), CDX / SPDX 2.3 / SPDX 3 goldens MUST be byte-identical to pre-169. Zero diff bytes.

- **SC-009 (unit test coverage)**: Reader logic MUST have ≥ 12 unit tests covering both code paths. Archive-file (US1) coverage: (a) well-formed ipk archive → correct PURL; (b) filename-only fallback → correct PURL + WARN; (c) filename non-conforming → skip + WARN, no invented component; (d) License field SPDX-canonicalization → routes through m152 escape hatch; (e) Depends field parsing → correct dep edges; (f) Provides field → annotation on providing component; (g) archive-size-cap fallback → filename-only + `mikebom:archive-size-skipped` annotation. Installed-DB (US2) coverage: (h) `/var/lib/opkg/status` primary parse → components with `opkg-status-db` evidence-kind; (i) `/var/lib/opkg/info/*.control` fallback when `status` absent → INFO log fires; (j) `/var/lib/opkg/info/*.list` skip-set → binary-walker dedup fires (FR-017); (k) mixed archive + installed-DB scan → single component per PURL, installed-DB precedence per FR-016. Shared coverage: (l) distro-qualifier propagation from `/etc/os-release`; (m) `Depends: pkg-a | pkg-b` alternative-list syntax → edge only to `pkg-a` + `mikebom:dep-alternative-alternates=["pkg-b"]` annotation on the source component (Q2 clarification).

- **SC-010 (integration test)**: A new integration test at `mikebom-cli/tests/ipk_reader.rs` MUST synthesize an ipk-directory scan with ≥ 3 well-formed `.ipk` files + ≥ 1 filename-only-fallback case + ≥ 1 skip-with-WARN case; assert all 5 SC-001 through SC-004 expectations across CDX, SPDX 2.3, and SPDX 3 outputs.

- **SC-011 (empirical closure via testbed reproduction)**: The PR body MUST include the reproduction from issue #500's Symptom section rerun on post-169 mikebom by the merging maintainer (per Q3 clarification), showing: (a) `shape_skipped=0` on the ipk-file portion of the walker-complete tracing log; (b) a component count ≥ 4580 for the archive-file path (US1); AND (c) if a Yocto/OpenWrt runtime rootfs is also available, a companion scan showing ≥ 36 components emitted from the installed-DB path (US2) with `mikebom:evidence-kind = "opkg-status-db"` — matching the 36-installed-on-image count from issue #500's `core-image-minimal` reference. If the runtime rootfs isn't available at merge time, SC-005b synthetic-fixture validation stands in.

- **SC-012 (backward-compat)**: The pre-169 behavior on scans that contain NO `.ipk` files MUST be byte-identical to pre-169 (SC-008 covers this on m090 fixtures; SC-012 restates it as a broader guarantee — any user's pre-169 scan output remains unchanged post-169).

## Assumptions

- **Yocto testbed availability**: the issue #500 filer's `yocto-test` testbed is a private repository. Per clarification 2026-07-06 Q3, validation splits into three tiers:
    - **CI-time (unit + integration tests)**: 3-5 vendored real Yocto-built `.ipk` files committed to `mikebom-cli/tests/fixtures/ipk-files/` (analogous to m069 `rpm-files/`) — archives are small (~10-100 KB each), 3-5 sufficient. Plus a synthetic runtime-rootfs directory tree committed to `mikebom-cli/tests/fixtures/opkg-installed-db/` with a hand-crafted `/var/lib/opkg/status` file + matching `/var/lib/opkg/info/*.control` + `.list` files (~1-2 KB per package × ~5-10 packages).
    - **PR-body attested (SC-001 + SC-005b + SC-011)**: the merging maintainer re-runs a Yocto scarthgap `core-image-minimal` build locally, scans `tmp/deploy/ipk/` + the runtime rootfs, and attaches the reproduction to the PR body. Matches m165 audit-milestone maintainer-runnable pattern. No large fixture commitment to the repo.
    - **Out of scope**: a scaled ~500 MB fixture with 4587 real ipks. Repo bloat + fixture-brittleness outweighs the incremental CI signal over the 3-5-vendored + PR-body-attested combination.
- **Dual-reader module structure**: per clarification 2026-07-06 Q1, m169 emits two Rust modules under `mikebom-cli/src/scan_fs/package_db/` — `ipk_file.rs` (archive-file reader, analogous to m069 `rpm_file.rs`) + `opkg.rs` (installed-DB reader, analogous to m004 `rpm.rs`). Shared helpers (PURL construction, control-file parsing, license routing) live in `ipk_common.rs` or on `mikebom_common`. Both readers wire into the same `read_all` dispatcher.
- **PURL type**: purl-spec's `pkg:opkg/` type is stable (documented at https://github.com/package-url/purl-spec/blob/master/PURL-TYPES.rst#opkg); no vocabulary invention needed.
- **Control-file dialect**: opkg's control-file syntax is a strict subset of Debian's. Sharing the deb-reader's control-file parse routine is safe (the ipk reader may pass through the same code with a small wrapper handling opkg-specific fields).
- **Archive-format library**: mikebom already depends on `ar` (for `.deb`) and `flate2` + `tar` (for `.tar.gz`). No new Cargo dependencies needed.
- **License-canonicalization**: routes through the existing m152/153/154 SPDX pipeline (referenced in FR-008); no separate license-code path added.
- **Milestone-090 fixture layout**: no ipk fixture exists yet in `mikebom-test-fixtures`. This milestone may EITHER add one (mirroring the m090 rpm layout) OR emit reference goldens for the new integration test at `mikebom-cli/tests/fixtures/ipk_reader/` inside the main repo per milestone-090's stayset carve-out for opaque test data.
- **No changes to existing readers**: dpkg, apk, rpm, alpm, homebrew readers are UNCHANGED. This milestone is purely additive.
- **PACKAGE_CLASSES="package_ipk_rpm" hybrid case**: out of scope for this milestone — Yocto's fallback that emits BOTH ipk and rpm is rare + would need cross-reader coordination. If empirical evidence surfaces the pattern, follow-on milestone.

## Out of Scope

- **Live `opkg install` observation (eBPF trace path)**: mikebom's eBPF path (m020) is untouched. This milestone is user-space scan-time only, matching the deb/rpm/apk/alpm pattern.
- **Yocto/OpenWrt SPDX output re-consumption**: Yocto's own SPDX emitter (`create-spdx.bbclass`) produces per-recipe SPDX documents at `tmp/deploy/spdx/`. Re-parsing those is a separate milestone (analogous to `mikebom sbom bind-to-source` on external SBOMs — m072).
- **OpenWrt online feed scanning**: fetching remote `Packages` indices from OpenWrt feeds (`downloads.openwrt.org/releases/...`) is out of scope. mikebom scans local artifacts, not remote registries.
- **ipk creation / signing**: mikebom is a scanner — it reads ipk archives; it does not create or sign them.
- **`opkg-utils` script emulation**: mikebom does not need to emulate the `opkg-build` / `opkg-key` / `opkg-make-index` tools. mikebom is read-only.
- **`.ipk` files inside container images** (image-tier mode): mikebom's image-tier scan (`--image <ref>`) already extracts the image rootfs; if the rootfs contains ipk artifacts + an ipk-installed package DB, this milestone's reader will fire on them. But image-tier orchestration itself is unchanged.
- **Multi-arch qualifier handling for the "all" architecture**: opkg's `Architecture: all` bucket for arch-independent packages MAY emit as `?arch=all` verbatim; explicit multi-arch resolution (mapping "all" to a per-image effective arch) is a future refinement, not blocking.
- **Signature verification of ipk archives**: opkg supports GPG-signed packages via `opkg-key`. Signature verification is out of scope (analogous to mikebom NOT verifying dpkg's `.buildinfo` signatures).

## Dependencies and Constraints

- **Existing readers UNCHANGED**: dpkg/apk/rpm/alpm/homebrew readers are untouched. Backward-compat guaranteed by SC-008 golden byte-identity.
- **New Cargo dependencies**: **zero**. Reuses `ar`, `flate2`, `tar` already in the workspace dependency closure.
- **Constitution Principle IV (no `.unwrap()` in production)**: the reader uses `Option::or` / `Result::or_else` patterns; test code using `.unwrap()` MUST be guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]`.
- **Constitution Principle V (standards-native precedence)**: `pkg:opkg/` is the standards-native PURL — no `mikebom:*` prefix invention.
- **Constitution Principle X (transparency)**: every skipped file MUST fire a tracing WARN per FR-006, FR-007. Zero silent drops.

## Related

Prior filings from the same testbed (all closed) that hardened mikebom's rpm reader path — the ipk reader inherits from the same license-expression + evidence-kind + PURL infrastructure:

- **#468** rpm namespace
- **#469** rpm size cap (referenced in FR-012 for the ipk symmetric cap)
- **#470** `X AND X` license dedupe
- **#475** `&`/`|` operator normalization
- **#481** LicenseRef preservation
- **#485** SPDX 2.3 `hasExtractedLicensingInfos`
- **#487** SPDX 3 `CustomLicense`

Prior mikebom milestones establishing per-format reader patterns:

- **m004** RPM reader (evidence-kind convention, per-format binary-walker skip)
- **m069** RPM archive-size cap (referenced in FR-012)
- **m107 / m128** Yocto recipe reader (source-tier coverage — this milestone adds the runtime-image-tier symmetric artifact reader)
- **m135** Arch alpm reader (spec template + m090-fixture-carve-out precedent)
- **m152 / m153 / m154** SPDX LicenseRef preservation (routes ipk License fields through the same canonicalization)
