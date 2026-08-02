# Feature Specification: Pants coursier JVM lockfile reader

**Feature Branch**: `224-pants-coursier-jvm`
**Created**: 2026-08-01
**Status**: Draft
**Input**: User description: "pants-coursier-jvm-reader"

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Scan a Pants JVM repo and get Maven components in the SBOM (Priority: P1)

A platform-security operator runs `waybill sbom scan` against a source
tree that uses the Pants build system for its JVM targets (Java,
Scala, Kotlin). Today the resulting SBOM either lists zero JVM
components (if the repo has no `pom.xml`) or an incomplete set (if
the repo mixes Pants coursier-managed deps with a stray `pom.xml`
elsewhere). Pants stores its resolved JVM dep graph in coursier
lockfile format — a TOML file at `3rdparty/jvm/<resolve-name>.lock`
(default convention) or a `pants.toml`-configured path. Waybill's
existing Maven reader handles `pom.xml`, embedded `META-INF/maven/`,
`~/.m2/`, Gradle lockfiles, and deps.dev fallback — but NOT coursier
lockfile format. The operator wants the JVM packages Pants resolved
to appear in the SBOM with the same fidelity as any Maven-managed
repo: standard `pkg:maven/<group>/<artifact>@<version>` PURLs, sha256
artifact hashes, and a dependency graph.

**Why this priority**: This is the entire reason the feature exists.
Without it, Pants JVM repos are invisible to waybill's Maven coverage,
mirroring the pre-m223 Python gap. Pants JVM adopters overlap
strongly with Pants Python adopters (Toolchain, IBM, several
enterprise monorepos), so shipping this closes the "we can't scan
our JVM code either" gap for the same user base m223 opened Python
coverage for.

**Independent Test**: Point `waybill sbom scan` at a Pants JVM
project fixture containing a valid coursier `.lock` file with 3
locked JVM distributions. Assert the emitted CDX contains one
`pkg:maven/<group>/<artifact>@<version>` component per locked
distribution, each with the sha256 hash the lockfile recorded.

**Acceptance Scenarios**:

1. **Given** a Pants repo with `3rdparty/jvm/default.lock` containing 10 locked JVM distributions, **When** the operator runs `waybill sbom scan --path <repo>`, **Then** the emitted SBOM contains 10 JVM components (one per locked distribution) with `pkg:maven/<group>/<artifact>@<version>` PURLs and the sha256 fingerprints recorded by the lockfile.
2. **Given** the same repo, **When** the SBOM is emitted, **Then** each locked component's `dependsOn` graph reflects the entry's `dependencies[]` array (when the lockfile records inter-package dependencies).
3. **Given** a Pants repo where the coursier lockfile is at a non-default path (declared in `pants.toml` under `[jvm].default_resolve` + `[jvm.resolves]` table), **When** the operator runs the scan, **Then** waybill discovers the lockfile at the configured path.
4. **Given** a Pants repo with **multiple named resolves** (default + `scalatest` + `junit`, each with its own lockfile), **When** the scan runs, **Then** every resolve is scanned. Default-resolve components tag `lifecycle_scope=Runtime`; components from resolves named after known JVM dev tools (see FR-008 allowlist) tag `lifecycle_scope=Development`. Every component carries a `waybill:pants-resolve=<name>` annotation.

---

### User Story 2 — Correct duplicate handling when a Pants repo also has a stray `pom.xml` (Priority: P2)

Some Pants JVM repos also carry `pom.xml` files — for legacy tooling
compatibility, IDE integration, or during migration from Maven to
Pants. If waybill's existing Maven reader finds the `pom.xml` and
waybill's new pants-coursier reader finds the coursier lockfile, the
same JVM packages get discovered twice. The operator wants the SBOM
to list each package exactly once, without random selection about
"which reader wins."

**Why this priority**: Broken deduplication is a very visible
correctness problem — operators comparing waybill output to their
own dependency inventory notice extra components immediately, and it
undermines trust in the whole SBOM. Same rationale as m223 US2.

**Independent Test**: Craft a fixture with both a coursier lockfile
and a `pom.xml` naming overlapping Maven coordinates. Assert the
SBOM lists each Maven coordinate exactly once, with the more
authoritative source (the lockfile — has sha256 fingerprints) winning.

**Acceptance Scenarios**:

1. **Given** a Pants repo with `3rdparty/jvm/default.lock` AND a `pom.xml` both declaring `dev.waybill.fixture:shared:1.0.0`, **When** the scan runs, **Then** the SBOM contains exactly ONE `pkg:maven/dev.waybill.fixture/shared@1.0.0` component, sourced from the lockfile (which carries the sha256 hash the pom does not).
2. **Given** the same repo, **When** the SBOM is emitted, **Then** the component's `waybill:source-files` annotation records that both the lockfile and the `pom.xml` were observed, so operators can audit the dedup decision.

---

### User Story 3 — `pants.toml` `[jvm.resolves]` table discovery (Priority: P3)

Pants JVM supports arbitrary lockfile paths via configuration. Rather
than hardcoding `3rdparty/jvm/*.lock`, waybill reads `pants.toml` to
learn which paths + resolve names the operator's Pants config
declares, and scans those exact paths. Matters because:
- Some repos put lockfiles at `build-support/jvm/coursier.lock` or similar
- Multi-resolve repos declare per-resolve paths explicitly in the
  `[jvm.resolves]` table
- Missing this config causes false-negative "we didn't find any JVM
  packages" for repos with a valid but non-standard layout

**Why this priority**: Nice-to-have for the config-driven-layouts
long-tail; the default `3rdparty/jvm/*.lock` glob covers the majority
of Pants JVM repos out of the box. Deferrable if the P1 scope ships
with just the default glob.

**Independent Test**: Create a fixture with `pants.toml` declaring a
`[jvm.resolves]` table pointing at `build-support/jvm/prod.lock` and
NO file at `3rdparty/jvm/`. Assert the scan finds the packages in
`build-support/jvm/prod.lock`.

**Acceptance Scenarios**:

1. **Given** `pants.toml` declares `[jvm.resolves] prod = "build-support/jvm/prod.lock"`, **When** the scan runs, **Then** waybill discovers packages in `build-support/jvm/prod.lock` and tags each with `waybill:pants-resolve=prod`.
2. **Given** `pants.toml` is missing, invalid, or has no `[jvm.resolves]` table, **When** the scan runs, **Then** waybill falls back to the default `3rdparty/jvm/*.lock` glob and continues normally (no hard failure).

---

### Edge Cases

- **Empty or partially-generated lockfile**: `pants generate-lockfiles` was interrupted, leaving a file with the metadata comment block but zero `[[entries]]` blocks. Waybill emits an INFO diagnostic and produces zero components from that lockfile (no components ≠ scan failure).
- **Lockfile with entries that lack `[entries.file_digest]`**: uncommon but possible with `url=not_provided` markers (see the metadata comment's `generated_with_requirements` shape). Waybill emits the component with an empty `hashes[]` array and continues.
- **Corrupted / non-TOML-parseable lockfile bytes**: waybill emits a WARN, skips that lockfile, and continues scanning (no scan-abort).
- **Lockfile with unknown metadata `version`**: the header comment carries `"version": 1` today; if a future Pants ships `"version": 2` with an incompatible schema, waybill emits a WARN naming the unsupported version and skips that lockfile.
- **Classifier / packaging variants**: some Maven artifacts have non-default classifiers (`sources`, `javadoc`, `linux-x86_64`, etc.). Waybill records them in the PURL as `?classifier=<c>&type=<packaging>` per purl-spec's `maven` type rules.
- **A `pants.toml` exists but declares no JVM backend**: waybill emits an INFO log noting "Pants config detected, no JVM resolves configured", scans zero JVM lockfiles, exits cleanly. Downstream Python (m223) + Go readers still run normally.
- **Pants is present but no lockfile exists yet** (fresh checkout before `pants generate-lockfiles` has run): waybill emits an INFO log and produces no JVM components from Pants (the existing Maven reader may still find `pom.xml` or Gradle files if present).
- **Standalone coursier lockfile** (produced by direct `coursier resolve` CLI, no Pants involvement): out of scope for v1. Detection uses the `# --- BEGIN PANTS LOCKFILE METADATA` header as a discriminator; non-Pants coursier lockfiles skipped with an INFO log per FR-011.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: waybill MUST discover Pants coursier lockfiles by default at the glob `3rdparty/jvm/*.lock` relative to the scan root. Additional paths from `pants.toml` are covered by FR-004.
- **FR-002**: For each locked JVM distribution in a coursier lockfile, waybill MUST emit exactly one SBOM component with:
  - purl of form `pkg:maven/<group>/<artifact>@<version>` per purl-spec's maven type rules (group in the namespace segment, dot-separated; artifact in the name segment; version verbatim). Optional `?classifier=<c>&type=<packaging>` qualifiers when the entry's `[entries.coord]` carries non-default `classifier` / `packaging` values.
  - the sha256 fingerprint recorded in `[entries.file_digest].fingerprint` (if present) in the component's `hashes[]` per CDX and equivalent SPDX slots.
  - `sbom_tier: "source"` (lockfile-derived, matching how m223 tags Pex-derived components).
- **FR-003**: When the lockfile entry records inter-package dependencies (`dependencies[]` array on `[[entries]]`), waybill MUST emit those as SBOM `dependsOn` relationships. Coordinate strings in `dependencies[]` follow the shape `"group:artifact:version[,url=X,jar=Y]"` — waybill MUST extract just the coordinate triple for edge resolution (the `url=`/`jar=` metadata is discarded).
- **FR-004**: waybill MUST read `pants.toml` (if present at the scan root) and honor `[jvm].default_resolve` + the `[jvm.resolves]` table (mapping resolve names to lockfile paths). Missing `pants.toml`, absent JVM sections, or malformed TOML MUST fall back to the FR-001 default glob without failing.
- **FR-005**: When both a coursier lockfile and a competing JVM source (`pom.xml`, `build.gradle`, `gradle.lockfile`, `META-INF/maven/`, `~/.m2/`) declare the same PURL, waybill MUST emit exactly ONE component. The lockfile source wins because it carries authoritative sha256 hashes; the losing source is recorded via the existing `waybill:source-files` annotation channel (m191 reconciler behavior, verified in m223 US2).
- **FR-006**: waybill MUST NOT abort the scan if a coursier lockfile is corrupt, unparseable, or has an unsupported schema version. It MUST emit a WARN diagnostic naming the file + reason, skip that specific lockfile, and continue processing the rest of the repo.
- **FR-007**: The default emit path (no coursier lockfiles present in the scanned repo) MUST be byte-identical to today's goldens — the new reader activates only when a coursier lockfile is discovered. The existing Maven reader's output MUST be unchanged for repos that only have `pom.xml` / Gradle.
- **FR-008**: Multi-resolve handling: waybill MUST scan every discovered coursier lockfile. Components from the `default` resolve MUST tag `lifecycle_scope=Runtime`; components from resolves whose name matches a known-dev-tool allowlist MUST tag `lifecycle_scope=Development`. Initial JVM dev-tool allowlist: `scalatest`, `junit`, `testng`, `mockito`, `assertj`, `hamcrest`, `scalafmt`, `scalastyle`, `scalafix`, `checkstyle`, `spotbugs`, `pmd`, `errorprone`, `jacoco`, `dokka`, `ktlint`, `detekt`, plus generics (`lint`, `test`, `dev`, `ci`, `check`, `tools`, `docs`). Unknown resolve names default to `Runtime` (safe default). Every emitted component MUST carry a `waybill:pants-resolve=<name>` annotation identifying the source lockfile (reuses the m223-shipped C143 catalog row — same annotation key, cross-backend semantics).
- **FR-009**: Entries with non-standard artifact sources (private Maven repositories, git-URL substrates, direct download URLs) MUST still emit `pkg:maven/<group>/<artifact>@<version>` PURLs — Maven coordinates are the identity, not the fetch URL, and vuln scanners pivot on the coordinate. If the lockfile records a non-default `url` in the `[entries.coord]` block, waybill MUST emit a `waybill:source-url` annotation carrying that URL (reuses the m223-shipped C144 catalog row).
- **FR-010**: waybill MUST emit an INFO log line at scan-end reporting the number of coursier lockfiles discovered + the number of JVM components extracted. Structured field names MUST match the m223 shape (`lockfiles_discovered`, `lockfiles_parsed_ok`, `lockfiles_skipped_corrupt`, `components_emitted`) for grep consistency across Pants backends. Log module path: `waybill::scan_fs::package_db::pants_jvm`.
- **FR-011**: Waybill MUST discriminate Pants-generated coursier lockfiles from standalone coursier lockfiles via presence of the `# --- BEGIN PANTS LOCKFILE METADATA` header comment. Non-Pants coursier lockfiles at the same path MUST be skipped with an INFO log naming the file + the "no Pants metadata header" reason. Prevents false-positive discovery of unrelated coursier lockfiles that operators may have in the repo.

### Key Entities

- **Coursier lockfile (Pants-generated)**: A TOML file at `3rdparty/jvm/*.lock` (default) or a path declared in `pants.toml`'s `[jvm.resolves]` table. Records the resolved JVM dependency graph for one named resolve: locked distribution coordinates, sha256 fingerprints, artifact byte lengths, per-entry direct + transitive dependency edges. Header carries a TOML-commented metadata block with `version: 1` + `generated_with_requirements`. Multiple lockfiles can coexist in one repo (one per resolve — `default`, `scalatest`, `junit`, etc.).
- **Pants config file (`pants.toml`)**: TOML file at the repo root. Waybill's JVM discovery reads only `[jvm].default_resolve` (default resolve name) + the `[jvm.resolves]` table (map of resolve name → lockfile path). Other sections ignored. Same fail-open posture as m223's Python-side handling.
- **Named JVM resolve**: A logical grouping in Pants of "these JVM targets share this lockfile." Every Pants JVM repo has at least a `default` resolve; test-only resolves (`junit`, `scalatest`) and code-quality resolves (`scalafmt`, `ktlint`) are common. Each resolve maps to exactly one lockfile.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For a Pants JVM fixture with N locked distributions, waybill emits N Maven components in the CDX and N packages in the SPDX 2.3 output (100% coverage of the lockfile's contents, zero fabricated components). Matches m223 SC-001 in shape.
- **SC-002**: For a fixture where both a coursier lockfile and a `pom.xml` declare overlapping Maven coordinates, the SBOM lists each coordinate exactly once (zero duplicates), with the lockfile as the recorded source.
- **SC-003**: On a repo that does NOT use Pants JVM (no `3rdparty/jvm/*.lock`, no `pants.toml` `[jvm.*]`), scan runtime and SBOM output are byte-identical to pre-feature-224 goldens (feature adds zero cost when unused).
- **SC-004**: For a Pants fixture with a lockfile carrying `dependencies[]` edges, the SBOM's `dependsOn` graph matches the lockfile's declared graph (spot-check: the top 3 dependency edges observable in the lockfile appear identically in the SBOM).
- **SC-005**: For a repo with a corrupted coursier lockfile, the scan exits cleanly (exit code 0), emits a WARN naming the file, and still produces components from any other coursier lockfiles or JVM manifests in the same repo (fail-open on per-file corruption, no scan-abort).
- **SC-006** (post-ship acceptance, not a merge gate): Waybill's coursier-lockfile-derived component count for a real-world Pants JVM repo (e.g., a fork of Pants's `example-jvm`) is within ±5% of what Syft (if it grows pex-lockfile-style coursier support) reports on the same tree. If no comparator supports coursier lockfiles at ship time, waybill's absolute count is the ground truth. Verified manually within one week of feature release; findings recorded as a note in `docs/audits/`.

## Assumptions

- **Coursier lockfile format**: the TOML shape produced by `pants generate-lockfiles` for Pants ≥ 2.13 (when JVM backend became stable). Header carries `# --- BEGIN PANTS LOCKFILE METADATA` block with embedded JSON declaring `version: 1`. Empirically verified 2026-08-01 against `github.com/pantsbuild/example-jvm@main`.
- **Pants version target**: Pants 2.x JVM backend. Pants 1.x has no JVM backend and is out of scope.
- **PURL construction**: standard `pkg:maven/<group>/<artifact>@<version>` per purl-spec, matching what waybill's existing Maven reader emits. Classifier + packaging emitted as PURL qualifiers when non-default. Reuses waybill's existing PURL construction helper (or extracts to a shared helper if one doesn't already exist for Maven — mirrors m223's pip-normalizer reuse).
- **`pants.toml` parse depth**: waybill parses only `[jvm].default_resolve` + the `[jvm.resolves]` table. Full Pants JVM config schema is NOT parsed — that would couple us to Pants config schema evolution. Mirrors m223's minimal-parse posture.
- **License data**: coursier lockfiles do NOT carry license strings (empirically confirmed against example-jvm). Licenses come from downstream enrichment (deps.dev / ClearlyDefined) if enabled. Matches m223 posture.
- **No Pants binary invocation**: waybill parses lockfile bytes on disk; does not shell out to `pants` or `coursier` for any part of this feature. Matches the "no build-tool subprocess for read-time discovery" pattern used by every existing waybill reader.
- **Multi-tier scope**: this feature is source-tier only (lockfile bytes = source of truth). Design-tier `BUILD` file parsing and deployed-tier runtime JVM inspection are separate potential follow-ups, out of scope.
- **Test fixture policy**: fixtures use synthetic Maven coordinates (`dev.waybill.fixture:*`) — never real Central coordinates, per memory `feedback_fixture_synthetic_package_names`.
- **Interoperability with existing Maven reader**: coexistence via the m191 reconciler's PURL-level dedup path. FR-005 covers the specific dedup rule; the reconciler infrastructure is already in place (validated in m223 US2).
- **Fingerprinting**: this feature does NOT extend waybill's fingerprint corpus. Coursier lockfiles are metadata, not binaries.
- **eBPF integration**: this feature is user-space only; `waybill-ebpf` is untouched.
- **Constitution Principle I**: no new Cargo dependencies expected — coursier lockfiles are TOML, parseable by existing `toml = "0.8"` (workspace dep already used by m223, cargo, pip readers). No new C-native transitives.
- **Reuse of m223 infrastructure**: the `waybill:pants-resolve` (C143) + `waybill:source-url` (C144) catalog rows + extractors shipped with m223 are reused as-is. Zero new parity-catalog additions expected. **This is the single biggest LOC savings vs m223** — where m223 spent ~100 LOC on parity work, m224 spends 0.
- **Coursier vs Pants scope**: this feature reads Pants-generated coursier lockfiles specifically (identifiable by the `# --- BEGIN PANTS LOCKFILE METADATA` header per FR-011). Standalone coursier lockfiles produced by direct `coursier` CLI usage (without Pants) are a follow-up decision — the format differs slightly (no Pants metadata header), and standalone coursier is a much smaller usage segment. Deferred until operator demand emerges.
