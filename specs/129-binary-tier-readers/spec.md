# Feature Specification: Image-tier binary-extracted package readers

**Feature Branch**: `129-binary-tier-readers`
**Created**: 2026-06-18
**Status**: Draft
**Input**: User description: "Image-tier coverage expansion: binary-extracted NuGet/.deps.json, cargo-auditable, and nested-JAR readers"

## Context

A side-by-side audit of mikebom against syft on a real production container image
(`767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner:latest`, a Wolfi-based polyglot dev-tooling
image carrying the dotnet SDK, uv, Java SDK, and ~500 npm packages) found that mikebom emits
**1,049 components** versus syft's **3,771 library components** — a real gap of ~2,700 packages with the
following ecosystem breakdown:

| Ecosystem | mikebom | syft | gap | syft cataloger |
|-----------|---------|------|-----|----------------|
| nuget | 0 | 1,489 | **-1,489** | `dotnet-deps-binary-cataloger` |
| cargo | 58 | 986 | **-928** | `cargo-auditable-binary-cataloger` |
| maven | 72 | 372 | **-300** | `java-archive-cataloger` (with nested-archive recursion) |
| golang | 61 | 83 | -22 | `go-module-binary-cataloger` |
| pypi | 64 | 76 | -12 | various |
| npm | 531 | 503 | +28 | (mikebom catches more — dev-edges + platform packages) |
| apk | 177 | 177 | 0 | parity |
| gem | 85 | 85 | 0 | parity |

The same audit independently confirmed that mikebom **wins on every quality dimension** (license coverage,
dependency-graph completeness, supplier attribution, PURL quality, CPE candidates, annotations/transparency)
and ties on version accuracy. The gap is purely **completeness on image-tier coverage**: every missing package
lives inside a compiled artifact (a `.deps.json` sidecar to a .NET assembly, a `.dep-v0` ELF section of a Rust
binary, or a nested JAR inside a fat JAR / WAR). mikebom's existing source-tier readers (csproj/Cargo.toml/pom.xml)
fire on source trees but find nothing in a production container image where only the compiled output is shipped.

This feature closes that gap with three new image-tier extractors, prioritized by coverage impact.

## Clarifications

### Session 2026-06-18

- Q: For `cargo-auditable` `kind: "dev"` entries, map to which `mikebom:lifecycle-scope` value? → A: `"test"` (matches the existing source-tier Cargo reader's handling of `[dev-dependencies]` per milestone 088; avoids introducing a new `dev` scope variant that's absent from the rest of the binary-tier vocabulary).
- Q: Nested-JAR walker — descend into `.zip` files? → A: No. Restrict to `.jar`/`.war`/`.ear` only. False positives on `.zip` distribution archives (maven-assembly-plugin output, locale bundles, sample data) outweigh the marginal coverage gain. Matches syft's `java-archive-cataloger` defaults. Operator opt-in via a future flag remains possible.
- Q: For PE/CLR managed assembly metadata, which version field becomes the PURL version? → A: `AssemblyInformationalVersion → AssemblyFileVersion → AssemblyVersion` fallback ladder. Prioritizes vulnerability-matching fidelity (closest to NuGet ship-version); all three surface as separate `mikebom:assembly-version-*` annotations for transparency-audit purposes.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — .NET / NuGet from compiled assemblies (Priority: P1)

A platform engineer scans a container image that ships a .NET application (an ASP.NET service, a dotnet SDK
installation, or any image built with `dotnet publish`). The image carries no `.csproj` or `Directory.Packages.props`
source manifests — only the compiled assemblies (`.dll` files) and their `.deps.json` sidecars. The engineer
expects mikebom to enumerate every NuGet package the application depends on, with the same fidelity it provides
for source-tier scans.

**Why this priority**: Largest single coverage gap in the audit (1,489 missing packages, ~39% of the total
library gap). .NET is one of the four largest enterprise ecosystems by deployment footprint (Java, Node, .NET,
Python). mikebom claiming "polyglot SBOM coverage" while emitting zero NuGet PURLs on .NET-bearing images is
the single most reputation-shaping limitation surfaced by the audit.

**Independent Test**: Run `mikebom sbom scan --image <ref>` against a Microsoft-published .NET runtime image
(e.g. `mcr.microsoft.com/dotnet/runtime:8.0-alpine`). The emitted SBOM MUST contain `pkg:nuget/<name>@<version>`
components for every entry in every `*.deps.json` file inside the image rootfs, plus zero `mikebom:parse-failure`
annotations for well-formed `.deps.json` files. Verifiable without any other reader changes.

**Acceptance Scenarios**:

1. **Given** an image with `/usr/share/dotnet/sdk/8.0.127/dotnet-watch/dotnet-watch.deps.json` declaring 14
   library entries, **When** the engineer runs `mikebom sbom scan --image <ref>`, **Then** the emitted SBOM
   contains 14 `pkg:nuget/<name>@<version>` components whose names and versions exactly match the entries
   in the `libraries` map of that `.deps.json` file.
2. **Given** an image with a `.dll` carrying an `AssemblyVersion("1.2.3.4")` attribute and NO neighboring
   `.deps.json`, **When** the engineer runs the scan, **Then** the emitted SBOM contains a
   `pkg:nuget/<assembly-name>@1.2.3.4` component sourced from PE metadata with a
   `mikebom:source-mechanism = "dotnet-assembly-metadata"` annotation.
3. **Given** an image that has BOTH a `.deps.json` AND a `.csproj` source manifest declaring the same package,
   **When** the scan runs, **Then** the emitted SBOM contains exactly ONE component for that package with a
   `mikebom:also-detected-via` annotation listing both mechanisms.

---

### User Story 2 — Rust crates via `cargo-auditable` ELF section (Priority: P2)

The same platform engineer scans a container image that ships Rust binaries built with `cargo auditable` (the
upstream-blessed mechanism for embedding dependency metadata in the binary itself: tools like `uv`, `uvx`,
`rustup`, `cargo-binstall`, plus an increasing share of Astral / Rust Foundation tooling). The image carries
no `Cargo.toml` or `Cargo.lock`. The engineer expects mikebom to enumerate every crate listed in the
`.dep-v0` ELF section of every audit-enabled binary in the rootfs.

**Why this priority**: Second-largest single coverage gap in the audit (928 missing packages, ~24% of the
total). The `cargo auditable` format is the upstream Rust security WG's recommended pattern for shipping
auditable production binaries; adoption is growing (Astral's uv, several CNCF tools, increasing share of
distro packages). Without a binary reader, mikebom claims "Cargo support" but only sees the source-tree
version — which is exactly the artifact NOT present in production container images.

**Independent Test**: Run `mikebom sbom scan --image <ref>` against an image carrying `uv` or `uvx`. The
emitted SBOM MUST contain `pkg:cargo/<crate>@<version>` components corresponding to the entries in the
binary's `.dep-v0` ELF section, parseable independently with the `rust-audit-info` reference tool.
Verifiable without `.deps.json` or nested-JAR support landing.

**Acceptance Scenarios**:

1. **Given** an image whose `/usr/bin/uv` is a `cargo auditable`-built ELF carrying a `.dep-v0` section
   declaring 200 crate dependencies, **When** the engineer runs the scan, **Then** the emitted SBOM contains
   200 `pkg:cargo/<crate>@<version>` components whose names and versions exactly match the JSON inside the
   `.dep-v0` section (decompressed via deflate per the cargo-auditable wire format).
2. **Given** a binary without a `.dep-v0` section (a plain `cargo build` output, or any non-Rust binary),
   **When** the engineer runs the scan, **Then** the scan does NOT fail and the binary is silently skipped
   with at most a single `debug`-level log line.
3. **Given** a binary with a malformed `.dep-v0` section (truncated, wrong deflate magic, invalid JSON),
   **When** the engineer runs the scan, **Then** the scan does NOT fail; the binary is skipped with a
   single `warn`-level log line naming the binary path and the parse error.

---

### User Story 3 — Maven dependencies inside nested JARs (Priority: P3)

The same platform engineer scans a container image carrying a Java application packaged as a fat JAR / Spring
Boot uber JAR / shaded JAR / WAR file. The application's runtime classpath includes dozens of transitive
dependencies whose `META-INF/maven/.../pom.properties` entries live INSIDE the application JAR, not as separate
top-level JAR files in the image rootfs. The engineer expects mikebom to descend into the nested archives
and enumerate every embedded dependency.

**Why this priority**: Smaller absolute coverage gap (300 packages) but high per-image impact for Java
enterprise images (fat JARs are how Spring Boot, Quarkus, Micronaut, and most modern Java microservice
frameworks ship). Covered as an EXTENSION of the existing milestone-009 maven JAR reader, not a new reader
— scope is tighter than US1/US2.

**Independent Test**: Run `mikebom sbom scan --path <dir>` against a directory containing a single
Spring Boot uber JAR (e.g. one built from `spring-projects/spring-petclinic`). The emitted SBOM MUST contain
a `pkg:maven/<group>/<artifact>@<version>` component for every nested JAR's `META-INF/maven/.../pom.properties`
entry, with depth bounded to prevent zip-bomb hangs. Verifiable independently of US1/US2.

**Acceptance Scenarios**:

1. **Given** a Spring Boot uber JAR carrying 50 nested dependency JARs in its `BOOT-INF/lib/` directory,
   **When** the engineer runs the scan, **Then** the emitted SBOM contains 50 `pkg:maven/.../...@<version>`
   components whose coordinates match each nested JAR's `META-INF/maven/.../pom.properties`.
2. **Given** a fat JAR with deeply-nested archives (an EAR file containing WARs containing JARs containing
   JARs, three or more levels deep), **When** the engineer runs the scan, **Then** the walker descends to
   a configurable depth limit (default: 8 levels, matching milestone-128's include-depth convention) and
   stops gracefully without infinite recursion.
3. **Given** a malformed JAR (corrupt central directory, truncated entries, invalid `pom.properties` inside
   a nested JAR), **When** the engineer runs the scan, **Then** the scan does NOT fail; the JAR is recorded
   with a `mikebom:parse-failure` annotation and processing continues on sibling files.

---

### Edge Cases

- **PE assembly without managed metadata**: A `.dll` that's actually a native (Win32) DLL with no .NET
  metadata tables MUST be silently skipped — the reader detects "is this a CLR-managed assembly" before
  attempting to parse AssemblyVersion.
- **Stripped Rust binaries** (`strip --strip-all` removed the `.dep-v0` section): silently skip; the binary
  is indistinguishable from a non-Rust binary at the wire level.
- **`.deps.json` referencing an out-of-tree library** (the `libraries` entry has a `runtime/<rid>/...`
  path but the file isn't present in the rootfs): emit the component anyway — the `.deps.json` IS the
  ground truth declaration. Annotate `mikebom:image-presence = "declared-not-installed"`.
- **Nested JAR cycle** (a fat JAR somehow contains itself, e.g. via an embedded test fixture): cycle-detect
  via a visited-set keyed on the archive's SHA-256, breaking before the depth limit fires.
- **Zip bomb**: a malicious nested JAR designed to exhaust memory. Bounded by depth-limit + per-archive
  size cap (1 GB decompressed per individual nested archive, matching milestone-104's container-image cap).
- **Mixed-arch images** (multi-arch image where the wrong-arch `.dep-v0` section is present): emit
  components from each arch's binaries, deduped on `(arch, purl)` via the existing milestone-105 pipeline.
- **Same NuGet package declared in BOTH `.deps.json` AND `.csproj`**: dedup via the existing
  milestone-105 pipeline; emit ONE component with `mikebom:also-detected-via = "dotnet-deps-json,nuget-csproj"`.
- **Same crate declared in BOTH `.dep-v0` AND `Cargo.lock`** (rare; only possible if both the source tree
  AND the compiled binary are in the same scan target): dedup via the existing milestone-105 pipeline.
- **`cargo auditable` v0 format vs future versions**: the wire format is versioned via section name
  (`.dep-v0`); reader only handles v0 and silently skips future versions with a `warn`-level log.

## Requirements *(mandatory)*

### Functional Requirements

#### Cross-cutting (apply to all three readers)

- **FR-001**: Every component emitted by a new image-tier reader MUST carry the existing
  `mikebom:sbom-tier = "image"` annotation (consistent with the rest of the image-tier readers).
- **FR-002**: Every component emitted by a new image-tier reader MUST carry a `mikebom:source-mechanism`
  annotation naming the specific extractor: `dotnet-deps-json`, `dotnet-assembly-metadata`,
  `cargo-auditable-binary`, `maven-jar-nested`.
- **FR-003**: When the same package coordinate is detected by both a new image-tier reader AND an existing
  source-tier reader, the dedup pipeline (milestone 105) MUST merge them into one component with
  `mikebom:also-detected-via` listing both source mechanisms.
- **FR-004**: Every new reader MUST respect the `--offline` flag (no network calls, no subprocess calls).
- **FR-005**: Every new reader MUST respect the `--exclude-path` flag (milestone 113) and the `safe_walk`
  centralization (milestone 114) — no ad-hoc filesystem recursion.
- **FR-006**: Every new reader MUST handle malformed input gracefully — a single parse failure on any
  individual file MUST NOT abort the surrounding scan; the failure surfaces via a single `warn`-level log
  line plus a `mikebom:parse-failure` annotation on the synthetic component slot (if any).

#### US1 — .NET / NuGet

- **FR-007**: System MUST locate every `*.deps.json` file under the scan target's rootfs and parse the
  `libraries` map at the top level.
- **FR-008**: For each `(name, version)` entry in a `.deps.json` `libraries` map whose `type` field is
  `"package"`, the system MUST emit a `pkg:nuget/<name>@<version>` component.
- **FR-009**: For each `.deps.json` entry whose `type` field is `"project"` (the application's own assembly),
  the system MUST NOT emit a separate NuGet component — these are first-party, not third-party dependencies.
- **FR-010**: System MUST locate every `*.dll` file that carries a valid CLR header (PE file with a
  `COR20_HEADER` data directory entry) and extract its `AssemblyName`, `AssemblyVersion`,
  `AssemblyFileVersion`, and `AssemblyInformationalVersion` from the managed metadata tables. The PURL
  version MUST be selected via the fallback ladder
  `AssemblyInformationalVersion → AssemblyFileVersion → AssemblyVersion` (resolved 2026-06-18
  clarification — prioritizes vulnerability-matching fidelity vs. NuGet ship-versions). All three
  extracted version strings MUST be emitted as separate annotations
  (`mikebom:assembly-version-informational`, `mikebom:assembly-version-file`,
  `mikebom:assembly-version-runtime`) so auditors can resolve identity disputes.
- **FR-011**: When an assembly's metadata is also covered by a `.deps.json` declaration in the same image,
  the system MUST suppress the duplicate (the `.deps.json` declaration takes precedence because it carries
  the full PURL-grade version including pre-release suffix; AssemblyVersion is 4-tuple SemVer-stripped).
- **FR-012**: System MUST handle `.deps.json` files in BOTH the .NET runtime store layout
  (`/usr/share/dotnet/shared/Microsoft.NETCore.App/<ver>/`) AND the per-application layout
  (`<app-dir>/<app>.deps.json`) — they have identical schema but different discovery paths.
- **FR-013**: System MUST emit a `mikebom:cpe-candidates` annotation for every NuGet component, using the
  same heuristic as other ecosystems (vendor and product derived from name, version from version).

#### US2 — `cargo-auditable` binary reader

- **FR-014**: System MUST locate every ELF binary under the scan target's rootfs and check for a
  `.dep-v0` section.
- **FR-015**: When `.dep-v0` is present, system MUST deflate-decompress the section payload and parse the
  resulting JSON as a `cargo-auditable` v0 manifest (schema: `{packages: [{name, version, source,
  kind, dependencies, root}]}`).
- **FR-016**: For each entry in `packages[]` whose `kind` field is one of `"runtime"` or absent (default),
  system MUST emit a `pkg:cargo/<name>@<version>` component.
- **FR-017**: For each entry whose `kind` is `"build"`, the resolved component MUST carry lifecycle
  scope "build". For each entry whose `kind` is `"dev"`, the resolved component MUST carry lifecycle
  scope "test" — matching the milestone-052 source-tier Cargo reader's handling of `[dev-dependencies]`
  (resolved 2026-06-18 clarification). No `"dev"` scope variant is introduced.
  **Implementation note**: routes via `ResolvedComponent.lifecycle_scope` per Principle V
  (standards-native fields take precedence) — see plan.md Complexity Tracking. No
  `mikebom:lifecycle-scope` wire-format annotation is emitted; the lifecycle signal flows to CDX
  `scope`, SPDX 2.3 `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` typed relationship types, and
  SPDX 3 `LifecycleScopeType` natively.
- **FR-018**: For each entry whose `source` field is `"local"` (a path-dependency inside the build root),
  system MUST emit the component but tag it with `mikebom:cargo-source-mechanism = "local-path"` so
  downstream tools can suppress it from external-dependency lists.
- **FR-019**: System MUST handle 32-bit and 64-bit ELF. The reader MUST be verified end-to-end on
  `x86_64` and `aarch64` fixtures (per T032). `arm` and `riscv64` support is inherited from the
  upstream `object` crate's cross-architecture handling — these MUST work in principle but are NOT
  required to ship with dedicated fixtures in this milestone. A follow-up milestone may add
  fixture-level verification once a real-world arm/riscv64 cargo-auditable binary is identified in the
  audit corpus.
- **FR-020**: System MUST silently skip non-ELF binaries (PE, Mach-O on Linux container scans) and ELF
  binaries lacking a `.dep-v0` section. No log entry per skipped binary.

#### US3 — Maven nested-JAR recursion

- **FR-021**: System MUST extend the existing milestone-009 maven JAR reader to descend into nested
  archives (JARs inside JARs, WARs inside EARs, etc.) up to a depth limit of 8 levels (matching the
  milestone-128 `INCLUDE_DEPTH_LIMIT` convention).
- **FR-022**: System MUST detect nested archives by ZIP central-directory entries with `.jar`, `.war`,
  or `.ear` suffixes — extension-based, not magic-byte sniffing (per the existing JAR reader's
  convention). `.zip` entries MUST NOT be descended into; the false-positive risk on maven-assembly-plugin
  distribution archives (e.g. `<project>-bin.zip`), locale bundles, and sample data outweighs the marginal
  coverage gain (resolved 2026-06-18 clarification).
- **FR-023**: System MUST extract each nested archive's `META-INF/maven/<group>/<artifact>/pom.properties`
  AND `META-INF/maven/<group>/<artifact>/pom.xml` and emit a `pkg:maven/<group>/<artifact>@<version>`
  component for each, applying the existing milestone-009 SPDX license parser to nested `MANIFEST.MF`
  files.
- **FR-024**: System MUST cycle-detect via a SHA-256 visited-set keyed on the bytes of each nested archive,
  breaking before the depth limit fires.
- **FR-025**: System MUST enforce a per-nested-archive decompressed-size cap of 1 GB to mitigate zip-bomb
  attacks. Archives exceeding the cap MUST be skipped with a single `warn`-level log line.
- **FR-026**: When the same maven coordinate is detected at both a top-level JAR AND inside a nested JAR
  in the same scan, the existing milestone-105 dedup pipeline MUST merge them; the
  `mikebom:source-mechanism` annotation MUST distinguish (`maven-jar` vs `maven-jar-nested`).

#### Catalog / parity bookkeeping

- **FR-027**: Any new annotation key MUST be catalogued in `docs/reference/sbom-format-mapping.md` with a
  full Principle V audit narrative (the milestone-128 convention).
- **FR-028**: Any new annotation key MUST be registered as a `ParityExtractor` slice entry with matching
  `cdx_anno!` / `spdx23_anno!` / `spdx3_anno!` macros, emitting `SymmetricEqual` across all three formats.

### Key Entities

- **`.deps.json` document**: A JSON sidecar describing a .NET application's dependency graph.
  Top-level keys: `runtimeTarget`, `compilationOptions`, `targets`, `libraries`. The `libraries` map's
  values carry `type` (`"package"` for NuGet, `"project"` for first-party), `serviceable`, `sha512`, `path`.
- **Managed PE assembly**: A `.dll` file with both a PE COFF header AND a CLR runtime header. Carries
  metadata tables including `AssemblyName`, `AssemblyVersion`, `AssemblyCulture`, `PublicKeyToken`.
- **`cargo-auditable` `.dep-v0` payload**: A deflate-compressed JSON document embedded as an ELF section.
  Schema: `{packages: [{name, version, source, kind, dependencies, root}]}` per the upstream
  `rust-secure-code/cargo-auditable` crate documentation.
- **Nested archive**: A JAR / WAR / EAR / ZIP file embedded as a ZIP central-directory entry inside another
  archive. Recursion depth bounded by the milestone-128 include-depth convention.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For the audit image `767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner:latest`,
  mikebom's NuGet component count MUST equal syft's within 5% (target: ≥1,415 vs syft's 1,489).
- **SC-002**: For the same audit image, mikebom's Cargo component count MUST equal syft's within 5%
  (target: ≥937 vs syft's 986).
- **SC-003**: For the same audit image, mikebom's Maven component count MUST equal syft's within 10%
  (target: ≥335 vs syft's 372 — slightly looser bound because Maven nested-JAR enumeration
  fidelity varies between scanners).
- **SC-004**: For a Microsoft-published `.NET runtime` image (e.g. `mcr.microsoft.com/dotnet/runtime:8.0-alpine`),
  every `*.deps.json` file in the image MUST be discovered and parsed; zero `mikebom:parse-failure`
  annotations on well-formed inputs.
- **SC-005**: For an image carrying `cargo auditable`-built tools, every binary with a `.dep-v0` section
  MUST contribute its crate graph to the SBOM; zero `mikebom:parse-failure` annotations on well-formed
  inputs.
- **SC-006**: For a Spring Boot uber JAR, every nested JAR's `pom.properties` MUST surface as a
  `pkg:maven/...` component; zero phantom components (orphan `pom.properties` without matching JAR).
- **SC-007**: The overall sbom-comparison weighted score for the audit image MUST exceed syft's by at
  least 1.0 weighted point (the milestone-128 audit had mikebom at 3.3, syft at 2.6 — milestone 129 should
  preserve the quality lead while closing the completeness gap to 4/5).
- **SC-008**: For any image where the new readers find no applicable inputs (e.g. a pure-Go image with no
  `.dll` / `.dep-v0` / nested JAR), the emitted SBOM is byte-identical to the pre-milestone output across
  the 33 committed alpha.48 goldens.
- **SC-009**: Per-binary parse latency for the cargo-auditable reader MUST be under 100ms for a typical
  Rust binary (`uv`, ~50 MB). Total scan time growth on the audit image MUST be under 30% relative to
  alpha.48.

## Assumptions

- The image carries `.deps.json` files in the standard dotnet-publish / dotnet-runtime layout. Custom
  build-from-source dotnet apps that strip `.deps.json` are out of scope (rare; users should run on
  the unstripped build artifact).
- `cargo-auditable` is the upstream-blessed mechanism for embedded Rust dependency metadata; we do NOT
  speculatively support alternative formats (e.g. `cargo-bom`, `rust-audit-info-json` sidecars). The
  existing source-tier Cargo reader already handles `Cargo.toml` and `Cargo.lock`; this milestone covers
  the binary side only.
- Maven nested-JAR recursion is bounded at 8 levels deep. Empirically, the deepest real-world nesting
  observed in enterprise fat JARs is 4 (EAR > WAR > JAR > nested-test-JAR). The 8-level bound preserves
  the milestone-128 convention and tolerates pathological inputs.
- The PE/CLR managed-assembly reader operates on .NET Standard 2.0+ assembly metadata. .NET Framework 1.x
  assemblies (last released ~2002) are out of scope.
- The audit image (`remediation-planner:latest`) is representative of the polyglot dev-tooling deployment
  pattern. Single-ecosystem images (e.g. pure-Go, pure-Python) are tested separately to confirm
  byte-identity preservation per SC-008.
- The existing `--exclude-path` flag (milestone 113) gives operators a graceful escape hatch when nested-JAR
  recursion is undesirable (e.g. a CI scan against a build directory that happens to contain a fixture fat
  JAR they don't want walked).
- `cargo-auditable` binaries on platforms other than Linux (macOS Mach-O, Windows PE) are out of scope for
  this milestone — `--image` scans always target a Linux rootfs (the existing assumption documented at
  `--image-platform`'s help text). Cross-platform Rust binaries in source trees scanned via `--path` are
  still discoverable, but most don't carry `.dep-v0` outside Linux container images.

## Out of Scope

- File-type component inventory (the 27,004 `syft:file` entries in the audit). mikebom's design choice
  is to emit `library` and `application` components only — file inventory belongs in a separate `--manifest`
  output if ever needed.
- Reading WIX MSI installers (used by some Windows .NET runtime installers, but irrelevant on Linux
  containers).
- Parsing Java EAR-file deployment descriptors (`application.xml`) for module-level metadata. Out of scope
  per FR-021 (which limits scope to nested archive enumeration, not deployment-descriptor parsing).
- Adding source-tier coverage for `.NET Framework` packages-config (`packages.config` files). milestone 106
  already handles `Directory.Packages.props` + `packages.lock.json`; `packages.config` is legacy
  (.NET Framework 4.x and earlier) and out of scope.
- WebAssembly binaries (`.wasm`) that may carry their own dependency metadata. Out of scope per FR-014
  (ELF-only).
- Reading native (non-managed) `.dll` files for embedded version resources (`VERSIONINFO` PE resource
  blocks). The managed-assembly reader operates only on CLR-tagged DLLs.

## Dependencies

- The existing milestone-105 dedup pipeline with `SourceMechanism` enum + `mikebom:also-detected-via`
  collision handling — extended with three new variants.
- The existing milestone-097 `mikebom:cpe-candidates` annotation channel — reused for the new NuGet
  components per FR-013.
- The existing milestone-114 `safe_walk` helper — reused for the rootfs file enumeration in all three
  readers per FR-005.
- The existing milestone-113 `--exclude-path` flag — honored by all three readers per FR-005.
- The existing parity catalog C-row system in `docs/reference/sbom-format-mapping.md` — new annotation
  keys catalogued there per FR-027.
- The existing `object` crate (workspace dep) for ELF section reading (`cargo-auditable`) and PE COFF +
  CLR header parsing (`.NET assembly`).
- The existing `zip` crate (workspace dep) for nested JAR recursion.
- The existing `serde_json` crate for both `.deps.json` and `cargo-auditable` JSON payloads.
- The existing `flate2` crate (workspace dep) for deflate-decompressing `.dep-v0` ELF section content.
