# Ecosystems

Per-ecosystem coverage for all nine ecosystems mikebom supports. Use this
page to answer *"does mikebom see my packages the way I expect"* before
diving into the [architecture docs](architecture/overview.md).

## Coverage matrix

| Ecosystem | Detection source | Dep-graph source | Hash source | Enrichment (deps.dev / CD) | Status |
|---|---|---|---|---|---|
| [apk](#apk) | `/lib/apk/db/installed` | DB (direct `D:` only) | — | — / — | Implemented |
| [cargo](#cargo) | `Cargo.lock` v3/v4 | Lockfile (full tree) | Lockfile `checksum` | ✓ / ✓ | Implemented |
| [deb](#deb) | `/var/lib/dpkg/status` + `.list` files | DB (`Depends:`) | Per-file SHA-256 (deep hash) or `.md5sums` fallback | — / Planned | Implemented |
| [gem](#gem) | `Gemfile.lock` + `specifications/*.gemspec` | Lockfile indent-6 | — | — / ✓ | Implemented |
| [golang](#golang) | `go.mod` / `go.sum` + module cache; `runtime/debug.BuildInfo` for binaries | Cache walker (source); **none** (binaries) | `go.sum` H1 (Merkle trie, not CDX) | ✓ / ✓ | Implemented |
| [maven](#maven) | Project `pom.xml` + JAR `META-INF/maven` + `~/.m2` + deps.dev fallback + Gradle `gradle.lockfile` / `buildscript-gradle.lockfile` | Layered: local → JAR → `~/.m2` BFS → parent POM chain → deps.dev. Gradle lockfile = flat | JAR sidecar `.sha512` > `.sha256` > `.sha1` | ✓ / ✓ | Implemented |
| [npm](#npm) | `package-lock.json` v2/v3, `pnpm-lock.yaml`, `bun.lock`, `yarn.lock` (v1 + Berry), `node_modules/` | Lockfile (full tree) | Lockfile `integrity` (package-lock / pnpm-lock only) | ✓ / ✓ | Implemented |
| [nuget](#nuget) | `*.csproj` / `*.vbproj` / `*.fsproj` + `packages.lock.json` + `Directory.Packages.props` | Lockfile (full tree) when present; otherwise direct-deps only | — | ✓ / — | Implemented |
| [pip](#pip) | venv `dist-info/METADATA` + Poetry/Pipfile + `uv.lock` + `requirements.txt` | Lockfile (Poetry / Pipfile / uv), flat (venv) | `--hash=alg:hex` flags | ✓ / ✓ | Implemented |
| [rpm](#rpm) | `/var/lib/rpm/rpmdb.sqlite` (pure-Rust reader) | DB (`REQUIRES`) | — (rpmdb has none) | — / — | Implemented (BDB format detected, not parsed) |
| [yocto](#yocto) | opkg `/var/lib/opkg/status` + `build/tmp/deploy/images/*/*.manifest` + `meta-*/recipes-*/<name>/<name>_<version>.bb` | Flat (per-stanza / per-line / per-recipe) | — | — / — | Implemented |

"Enrichment" columns mark whether deps.dev version info and ClearlyDefined
concluded licenses apply to the ecosystem. Both honour the global
`--offline` flag.

---

## apk

**Module:** `mikebom-cli/src/scan_fs/package_db/apk.rs`

**Detection:** stanza parser over `/lib/apk/db/installed`. Reads `P:`
(name), `V:` (version), `A:` (arch), `D:` (direct dependencies).

**PURL format:** `pkg:apk/alpine/<name>@<version>?arch=<arch>&distro=alpine-<VERSION_ID>`
(e.g., `distro=alpine-3.19`). Same `<namespace>-<VERSION_ID>` shape as
deb and rpm.

**Evidence:** `PackageDatabase` / `manifest-analysis` at confidence 0.85.

**Dep graph:** direct dependencies only. apk's installed DB doesn't encode
transitive graph — it records only what each package declares.

**Hashes:** none. apk's installed DB doesn't carry per-package content
hashes mikebom can use.

**Enrichment:**
- deps.dev: skipped (not in deps.dev's supported ecosystems).
- ClearlyDefined: skipped (not curated).

**Known limitations:**
- apk's DB doesn't carry copyright pointers like dpkg does, so apk
  components ship with empty `licenses[]`.

---

## cargo

**Module:** `mikebom-cli/src/scan_fs/package_db/cargo.rs`

**Detection:** `Cargo.lock` v3 and v4 parser. v1/v2 are refused (they
pre-date the reproducible-lockfile guarantee).

**PURL format:** `pkg:cargo/<name>@<version>`. No namespace (crates.io is
flat).

**Evidence:** `PackageDatabase` for lockfile entries; `FilePathPattern`
for `.crate` files in `~/.cargo/registry/cache`.

**Dep graph:** full tree. Cargo.lock's `[[package]].dependencies` array
encodes every edge.

**Hashes:** `Cargo.lock` `[[package]].checksum` (SHA-256) flows through to
CycloneDX `components[].hashes[]`.

**Enrichment:**
- deps.dev: fetches declared licenses and VCS URLs. Without deps.dev,
  cargo license coverage drops to zero (crates.io doesn't publish licenses
  into `Cargo.lock`, only into `Cargo.toml`).
- ClearlyDefined: concluded licenses from CD's cratesio provider.

**Source-type markers:**
- `workspace` — workspace-local crates (no `source`).
- `git`, `path`, `url` — non-registry sources.
- `(none)` — normal registry crates.

---

## deb

**Module:** `mikebom-cli/src/scan_fs/package_db/dpkg.rs`, with DEP-5
copyright parsing in `scan_fs/package_db/copyright.rs` and per-file deep
hashing in `scan_fs/package_db/file_hashes.rs`.

**Detection:** stanza parser over `/var/lib/dpkg/status`, plus per-package
`/var/lib/dpkg/info/<pkg>.list` manifests for deep-hash occurrences.

**PURL format:** `pkg:deb/debian/<name>@<version>?arch=<arch>&distro=<namespace>-<ver>`
(e.g., `distro=debian-12`, `distro=ubuntu-24.04`, `distro=kali-rolling`).

Canonicalization (strict — reference-implementation-conformant):

- `+` in name and version → `%2B`.
- `:` in version (epoch separator) → literal, inside `@<version>`, not as
  a qualifier.
- `~` in version → literal.
- `distro=<namespace>-<VERSION_ID>` is the canonical form across deb, rpm,
  and apk — one shape so downstream consumers don't need per-ecosystem
  branching. Namespace is the debian/ubuntu/kali/etc. slug; `VERSION_ID`
  is the numeric or codename value from `/etc/os-release`.
- Auto-detected from `<rootfs>/etc/os-release` (`ID` + `VERSION_ID`);
  overridable via `--deb-codename <value>` which stamps the full
  qualifier value verbatim.

See [purls-and-cpes.md](architecture/purls-and-cpes.md) for the full
rationale.

**Evidence:** `PackageDatabase` / `manifest-analysis` at confidence 0.85.

**Dep graph:** full tree from dpkg `Depends:` fields. `Provides:` and
virtual packages are not currently modeled (dangling edges to virtual
packages are dropped by the resolve-stage guard rail).

**Hashes:**
- **Deep hash mode (default):** every file listed in the package's
  `.list` manifest is stream-hashed (SHA-256). Results emit as
  `evidence.occurrences[]` with per-file SHA-256 + dpkg MD5 cross-reference
  in `additionalContext`.
- **`--no-deep-hash`:** SHA-256 of the dpkg `.md5sums` file itself as a
  per-package fingerprint. Microseconds per package; component-level
  identity only; no per-file occurrences.
- Component `hashes[]` is populated in both modes (deep hash yields a
  per-component Merkle root over the listed files; fast mode yields the
  `.md5sums` hash).

**Licenses:** DEP-5 `/usr/share/doc/<pkg>/copyright` parsing, plus
standalone `License:` stanzas, modern `SPDX-License-Identifier:` tag, and a
multi-line recogniser for canonical FSF license-grant prose (catches
`debian-archive-keyring`, `libcrypt1`, `libsemanage2`, `libgcc-s1`, GCC
base libs that ship license grants verbatim).

**Enrichment:**
- deps.dev: skipped (not in deps.dev's supported ecosystems).
- ClearlyDefined: **Planned (next priority).** CD's `deb` type curates
  licenses from Debian's upstream copyright-file server and would fill the
  gap for images that strip `/usr/share/doc/<pkg>/copyright`. See
  [design-notes deferred item 18](design-notes.md#deferred-sbomqs-score-lift).

---

## gem

**Module:** `mikebom-cli/src/scan_fs/package_db/gem.rs`

**Detection:** `Gemfile.lock` indent-structure parser + walker over
`specifications/*.gemspec` files. The gemspec walker catches Ruby stdlib
and default gems that are invisible to `Gemfile.lock`.

**PURL format:** `pkg:gem/<name>@<version>`.

**Evidence:** `PackageDatabase` / `manifest-analysis` for lockfile entries;
gemspec-sourced entries also use `PackageDatabase`.

**Dep graph:** full tree. `Gemfile.lock`'s indent-6 lines encode per-gem
edges; gemspecs themselves carry no dep edges.

**Hashes:** none currently. Bundler 2.5+ emits `CHECKSUMS` sections in
`Gemfile.lock`; the parser for them is tracked as deferred
work — see the sbomqs-score-lift items in
[`design-notes.md`](design-notes.md) (Deferred #17).

**Enrichment:**
- deps.dev: skipped (not in deps.dev's supported ecosystems).
- ClearlyDefined: fetches concluded licenses from CD's rubygems provider.

**Known limitations:**
- Only `--include-dev` gates gems under `test` scope in the declaration
  tree; bundler's full scope semantics (`:development`, `:production`,
  grouped) aren't modeled.
- Interpolated gemspec versions (`"#{FOO_VERSION}"`) produce garbage
  strings — downstream PURL construction rejects them. Theoretical edge
  case; in practice gemspec versions are always literal strings.

---

## golang

**Modules:** `mikebom-cli/src/scan_fs/package_db/golang.rs` (source scans),
`mikebom-cli/src/scan_fs/package_db/go_binary.rs` (binary scans).

### Source scans

**Detection:** `go.mod` + `go.sum` + walker over
`$GOMODCACHE/cache/download/<escaped-module-path>/@v/<version>.mod` files.
Module paths with capital letters escape as `!x` for the cache lookup
(e.g. `Microsoft/go-winio` → `!microsoft/go-winio`).

**PURL format:** `pkg:golang/<module-prefix>/<final-segment>@v<version>`.

**Dep graph:** full tree when the module cache is warm (the walker
traverses `@v/*.mod` files to discover transitive edges). When the cache
is cold, edges are populated for root → direct deps only.

**Hashes:** `go.sum` H1 hashes are Merkle-trie roots, not file SHA-256s,
so they don't fit CDX's hash-algorithm enum. Component-level `hashes[]` is
empty today; see
[design-notes sbomqs deferred item 17](design-notes.md#deferred-sbomqs-score-lift)
for the plan.

### Binary scans

**Detection:** `runtime/debug.BuildInfo` inline-format decoder. Works for
Go 1.18+ binaries. Pre-1.18 binaries are flagged with
`mikebom:buildinfo-status = unsupported` and emit a file-level component
only.

**PURL format:** same as source scans.

**Dep graph:** **none.** `runtime/debug.BuildInfo` encodes the module
list but not module-to-module relationships.

**Hashes:** the binary itself gets hashed (`ResolutionTechnique::FilePathPattern`
at 0.70 confidence with file-level evidence); individual modules don't.

**VCS metadata (milestone 025):** when the binary was built with
`-buildvcs=true` (the Go default since 1.18), three additional
annotations attach to the main-module entry: `mikebom:go-vcs-revision`
(commit SHA from `vcs.revision`), `mikebom:go-vcs-time` (RFC 3339
build timestamp from `vcs.time`), and `mikebom:go-vcs-modified`
(dirty-tree boolean from `vcs.modified`, preserved as the literal
`"true"` / `"false"` string per Go's wire format). Surfaced via the
milestone 023 `extra_annotations` bag — same data `go version -m
<binary>` shows. Dep entries don't carry VCS metadata; that's a
main-module concern. Binaries built with `-buildvcs=false` or outside
a VCS worktree emit no `mikebom:go-vcs-*` annotations.

**Known limitations:**
- Stripped binaries where BuildInfo extraction fails get
  `mikebom:buildinfo-status = missing` and emit only as a file-level
  component with hash-only PURL.
- Scratch / distroless images with a single Go binary produce a flat
  component list. That's the accurate answer — the binary doesn't know the
  graph.
- Private module proxies and `vendor/` directory extraction are out of
  scope today.

**Enrichment:**
- deps.dev: fetches licenses and VCS URLs using the full module path
  (`github.com/sirupsen/logrus`), not the short name.
- ClearlyDefined: concluded licenses via CD's `golang` / `github`
  provider.

---

## maven

**Module:** `mikebom-cli/src/scan_fs/package_db/maven.rs`

Maven is the most complex ecosystem. Transitive versions can live in
parent POMs' `<dependencyManagement>` or be supplied by BOM imports. See
[design-notes §Dep-graph resolution strategy (Maven)](design-notes.md#dep-graph-resolution-strategy-maven)
for the full six-layer strategy.

**Detection (layered):**
1. Scanned project's `pom.xml` (direct deps).
2. JAR-embedded `META-INF/maven/<g>/<a>/{pom.xml, pom.properties}`
   (identity + edges for deployed containers; fat/shaded JARs yield one
   `EmbeddedMavenMeta` per vendored artifact).
3. `~/.m2/repository/` cache walker (BFS over cached `.pom` files).
4. Parent-POM chain (`build_effective_pom`) with
   `<properties>` + `<dependencyManagement>` inheritance + BOM-import
   flattening.
5. deps.dev `:dependencies` endpoint (online fallback for shaded-transitive
   and cold-cache gaps).
6. Empty edges (graceful degradation).

**PURL format:** `pkg:maven/<groupId>/<artifactId>@<version>`. Reverse-DNS
groupId is part of the identity.

**Dep graph:** deps.dev is **authoritative for edge topology** but never
for versions — local `.m2` always wins on the version dimension. See the
[deps.dev policy](design-notes.md#depsdev-policy-critical).

**Hashes:** JAR sidecar `.sha512` > `.sha256` > `.sha1` (Maven Central
mostly ships SHA-1; sbomqs penalizes for `comp_with_strong_checksums`).
Direct-JAR SHA-256 computation when the cache has the JAR but no sidecar
is deferred.

**Enrichment:**
- deps.dev: license + VCS + `:dependencies` graph. Package name is
  `groupId:artifactId` (raw artifactId alone isn't unique).
- ClearlyDefined: concluded licenses via CD's `mavencentral` provider.

**Source-type markers:**
- `workspace` — scanned project's pom.xml.
- `analyzed` — JAR walker's `META-INF/maven` pom.properties.
- `transitive` — BFS-discovered via local cache / JAR walk.
- `declared-not-cached` — deps.dev says it's a declared dep but not
  present locally at any version.

**Shade-plugin fat-jars (feature 009):**
When a JAR contains `META-INF/DEPENDENCIES` (the Apache
`maven-dependency-plugin`'s declared-transitive manifest), mikebom
parses it into ancestor coords and emits one nested component per
ancestor under the enclosing JAR's primary coord, tagged
`mikebom:shade-relocation = true`. Emission is gated on
**bytecode-presence verification**: an ancestor is retained only when a
`.class` entry in the JAR matches either its original group path
(UNSHADED) or a shade-relocated path containing the ancestor's
distinctive artifact-id leaf (SHADED, generic leaves like `io`, `api`,
`util`, `core` excluded). The UNSHADED check is suppressed when
ancestor and primary share a reactor group namespace, since sibling
reactor artifacts cannot be distinguished from the primary's own
classes under the shared namespace. Full rules in
[`specs/009-maven-shade-deps/spec.md`](../specs/009-maven-shade-deps/spec.md)
FR-002b.

**Known limitations:**
- `<exclusions>` not parsed. If a project excludes a transitive via
  `<exclusions>`, mikebom still emits the excluded coord.
- Version ranges (`[1.0,2.0)`) not resolved.
- `<profiles>` ignored — profile-conditional deps never emit.
- Plugin-section deps (`<build><plugins>`) ignored — not runtime deps.
- POM-less JARs (older Gradle outputs, OSGi bundles) can't be inspected
  via `META-INF/maven/` — coord + deps invisible.

### Gradle dependency-locking (milestone 106)

**Module:** `mikebom-cli/src/scan_fs/package_db/gradle/`

**Detection:** either `gradle.lockfile` (runtime classpath) or
`buildscript-gradle.lockfile` (build-script / plugin classpath) found
anywhere in the scan tree (max_depth=6 walker). Both files share a
line-format parser.

**Format:** `<group>:<name>:<version>=<configuration1>,<configuration2>,...`.
Header lines (`#`-prefixed) and the `empty=<configs>` marker are
skipped. Malformed entries warn-and-continue (FR-015).

**PURL format:** `pkg:maven/<group>/<name>@<version>` — same scheme as
Maven, so downstream deps.dev enrichment applies without changes.

**Lifecycle scope:** filename-driven. `buildscript-gradle.lockfile`
emits `LifecycleScope::Build` (→ CDX `scope: "excluded"`, SPDX 2.3
`BUILD_DEPENDENCY_OF`, SPDX 3 `lifecycleScope: "build"`).
`gradle.lockfile` carries no scope (runtime default).

**Annotations:** `mikebom:gradle-configurations` carries the raw
comma-joined configuration list (informational; downstream filterable
by `compileClasspath` / `testRuntimeClasspath` / etc.).

**Dep graph:** flat. Gradle lockfiles don't encode parent → child
edges; each row is an already-resolved coord.

---

## npm

**Module:** `mikebom-cli/src/scan_fs/package_db/npm.rs`

**Detection:** `package-lock.json` v2/v3, `pnpm-lock.yaml`, or flat walk
of `node_modules/` as tertiary fallback. `package-lock.json` v1 is
**refused** — its format doesn't give enough info for reproducible
dependency graphs.

**PURL format:**
- Unscoped: `pkg:npm/<name>@<version>`.
- Scoped: `pkg:npm/<@scope>/<name>@<version>` (e.g. `pkg:npm/@angular/core@17.0.0`).

**Evidence:** `PackageDatabase` / `manifest-analysis` at 0.85.

**Dep graph:** full tree from `package-lock.json` `packages` entries.

**Hashes:** `package-lock.json` `integrity` field (SRI format). Supports
sha256, sha384, sha512; flows through to CycloneDX `components[].hashes[]`.

**Enrichment:**
- deps.dev: licenses + VCS. Package name is `@org/name` for scoped.
- ClearlyDefined: concluded licenses. Namespace for scoped packages
  strips the leading `@` (`@angular` → `angular`).

**npm internals filtering (scope-by-mode, always on):**
- In `--image` scans, components discovered inside npm's own bundled tree
  (`**/node_modules/npm/node_modules/**`) are marked
  `mikebom:npm-role = internal` and retained — the image contains
  npm's own install, so those bytes are legitimately present.
- In `--path` scans, internals are filtered out before resolution on
  the assumption that a path-mode scan targets the application's
  `node_modules/`, not a tool cache.
- This is not user-gated — there is no flag to toggle it. See
  feature 005 (`specs/005-purl-and-scope-alignment/`) for rationale.

### Bun lockfile (milestone 106)

**Module:** `mikebom-cli/src/scan_fs/package_db/npm/bun_lock.rs`

**Detection:** `bun.lock` (Bun's JSONC lockfile format) at any
project root in the scan tree. Bun-only projects (no
`package-lock.json` / `pnpm-lock.yaml`) are picked up via the
`has_npm_signal` marker.

**Format:** JSONC (JSON with comments) — the `// bun: lockfileVersion: 1`
header comment is stripped before `serde_json::from_str` via the shared
`npm/jsonc.rs` helper. Parses `lockfileVersion`, `workspaces`,
`packages`, and `overrides` keys; unknown keys are silently ignored.

**Workspace support:** when `workspaces` declares members, mikebom
emits a synthetic workspace-root component (PURL: `pkg:generic/<name>`,
`mikebom:component-role: "workspace-root"`) plus a `main-module`
component per member. Intra-workspace edges are harvested when a
member's `dependencies` declares `workspace:*` source-specs.

**Overrides:** when `overrides` is present, the overridden version
wins at registry-emission time; the un-overridden version is NOT
emitted as a separate component.

**PURL format:** `pkg:npm/<name>@<version>` — scoped names
URL-encode the `@` (`@scope/name` → `pkg:npm/%40scope/name@version`).

### Yarn lockfile (milestone 106)

**Module:** `mikebom-cli/src/scan_fs/package_db/npm/yarn_lock.rs`

**Detection:** `yarn.lock` at any project root in the scan tree.
Yarn-only projects are picked up via the `has_npm_signal` marker.

**Format auto-detection:** both Yarn lockfile formats are supported,
sniffed from file content:

- **v1 (Yarn Classic)** — text-based, indent-2 / indent-4 structure.
  Top-level entries are `"<descriptor>":` lines like `"foo@^1.0.0"`
  (or comma-joined alias lists like
  `"foo@^1.0.0", "foo@^1.1.0":`). Each body declares
  `version "..."`, optional `resolved "..."`, optional
  `integrity ...`, and an optional `dependencies:` sub-block.
- **Berry (Yarn 2+)** — YAML-shaped, parsed via `serde_yaml`. Has a
  `__metadata:` block at the top (the format-detection sentinel).
  Descriptors carry an `npm:` protocol prefix
  (`"foo@npm:^1.0.0"`); per-entry block uses YAML mappings.

**PURL format:** `pkg:npm/<name>@<version>` — same scheme as
package-lock / pnpm-lock / bun.lock, including scoped-name
URL-encoding.

**Dep graph:** each entry's `dependencies:` map populates
`PackageDbEntry.depends`. The scan orchestrator drops edges whose
target isn't present in the same scan (same pattern as
package-lock).

**Hashes:** not currently surfaced into `components[].hashes[]`.
v1's `integrity ...` line and Berry's `checksum:` field are
present in the source but not threaded through to `PackageDbEntry.hashes`
yet — tracked as a follow-up.

**Out of scope (milestone 106):**
- Yarn 2+ workspaces protocol entries (workspace synthesis
  mirroring the bun_lock shape).
- `resolutions:` overrides (rare in practice; future milestone if
  there's demand).

---

## pip

**Module:** `mikebom-cli/src/scan_fs/package_db/pip.rs`

**Detection:** three parallel paths:
1. Installed venvs: walk `<venv>/lib/python*/site-packages/*.dist-info/METADATA`.
2. Lockfiles: Poetry `pyproject.toml` + `poetry.lock`, Pipfile +
   `Pipfile.lock`.
3. Flat declarations: `requirements.txt`. Captures `--hash=alg:hex` flags
   per requirement.

**PURL format:** `pkg:pypi/<name>@<version>`. Name is PEP 503–normalized
(lowercase, runs of non-alphanum collapsed to `-`).

**Evidence:** `PackageDatabase` / `manifest-analysis` at 0.85 for venv
`METADATA` and lockfiles; `FilePathPattern` at 0.70 for loose `.whl` files.

**Dep graph:**
- Poetry / Pipfile: full tree.
- Venv: flat (venv `Requires-Dist:` lines are captured but not
  transitively expanded; venv installs are "deployed" tier evidence).
- requirements.txt: flat.

**Hashes:** `requirements.txt --hash=alg:hex` flags become
`PackageDbEntry.hashes` → `components[].hashes[]`. Multiple hashes per
requirement are supported. Other sources (venv METADATA, Poetry, Pipfile)
don't carry per-component hashes yet.

**Enrichment:**
- deps.dev: licenses + VCS.
- ClearlyDefined: concluded licenses via CD's `pypi` provider.

### uv lockfile (milestone 106)

**Module:** `mikebom-cli/src/scan_fs/package_db/pip/uv_lock.rs`

**Detection:** `uv.lock` (TOML) at any project root in the scan tree.
Sibling to the existing Poetry / Pipfile readers; uv-only projects
are picked up via the `has_python_project_marker` walker.

**Format:** TOML `[[package]]` array. Each entry carries `name`,
`version`, and an optional `[[package.dependencies]]` sub-array
giving the resolved dep graph. Workspace projects additionally
declare members under `[tool.uv.workspace]` in the root
`pyproject.toml`.

**Workspace support:** mikebom emits a synthetic workspace-root
component (PURL: `pkg:generic/<name>`,
`mikebom:component-role: "workspace-root"`) plus a `main-module`
per member. Intra-workspace dep edges are surfaced automatically
when a member's `[[package.dependencies]]` names a sibling member.

**PURL format:** `pkg:pypi/<name>@<version>` — PEP 503-normalized
name (lowercase, runs of non-alphanum collapsed to `-`).

**Dep graph:** full tree from `[[package.dependencies]]`.

---

## nuget

**Module:** `mikebom-cli/src/scan_fs/package_db/nuget/`

**Detection:** walks the scan tree for `.csproj` / `.vbproj` /
`.fsproj` files (max_depth=8). For each project file, applies a
four-step version-resolution ladder.

**Version-resolution ladder (FR-007 + FR-008):**
1. `packages.lock.json` adjacent to the project (`dependencies.<framework>.<name>.resolved`
   across all frameworks). Pinned version wins over a `.csproj` range
   like `[1.2.3, )`.
2. Inline `Version=` attribute on the `<PackageReference>`.
3. CPM (`<PackageVersion Include="..." Version="..."/>` in the
   closest ancestor `Directory.Packages.props`, walking up bounded
   by `scan_root`).
4. `unresolved` sentinel + `tracing::warn!` if nothing resolves.

**PURL format:** `pkg:nuget/<name>@<version>` — names case-preserved
from the source (NuGet is case-insensitive on the registry but
mikebom records what the source says; dedup handles cross-source
collation).

**Lifecycle scope:** driven by `PrivateAssets`, `IncludeAssets`,
`ExcludeAssets` attributes. `PrivateAssets="All"`, a positive
`IncludeAssets` list lacking `runtime`, and `ExcludeAssets=runtime`
all map to `LifecycleScope::Build` → CDX `scope: "excluded"`,
SPDX 2.3 `BUILD_DEPENDENCY_OF`, SPDX 3 `lifecycleScope: "build"`.
Matching is case-insensitive; both `,` and `;` separators are
recognized per MSBuild conventions.

**Transitive emission:** packages.lock.json entries tagged
`"type": "Transitive"` that don't appear in any `.csproj` are
emitted with `mikebom:source-type: "transitive"`.

**Dependency edges:** each lockfile entry's `dependencies` map
populates `PackageDbEntry.depends`. The standard scan orchestrator
drops edges whose target isn't present in the same scan.

**Source-files merging:** when multiple files contribute to the
same canonical PURL (e.g. `.csproj` + `Directory.Packages.props`
for CPM, or `.csproj` + `packages.lock.json` for direct deps), the
file paths merge into a single comma-joined `mikebom:source-files`
annotation. `BTreeSet<PathBuf>` keeps ordering deterministic.

**Enrichment:**
- deps.dev: licenses + VCS via deps.dev's nuget system.
- ClearlyDefined: not yet wired.

**Out of scope (milestone 106):**
- Project references (`"type": "Project"` in
  `packages.lock.json`) — intra-solution links. Future milestone
  can promote these to workspace-member style.
- `Directory.Build.props` `<PackageVersion>` entries (some repos
  use this file for the same purpose).

---

## rpm

**Modules:** `mikebom-cli/src/scan_fs/package_db/rpm.rs`,
`mikebom-cli/src/scan_fs/package_db/rpmdb_sqlite/`

**Detection:** pure-Rust SQLite reader over
`/var/lib/rpm/rpmdb.sqlite`. No C dependency on librpm (per the project
constitution: no C deps in production).

**PURL format:** `pkg:rpm/<vendor>/<name>@<version>-<release>?arch=<arch>&distro=<vendor>-<ver>`.

Canonicalization:

- Vendor is the distro slug (`redhat`, `rocky`, `fedora`, `suse`,
  `opensuse`, `amzn`).
- `epoch=0` omitted (RPM treats absent and 0 equivalently; `rpm -qa`
  default display omits). See the
  [RPM canonicalization note in design-notes](design-notes.md#purl-canonicalization).

**Evidence:** `PackageDatabase` / `manifest-analysis` at 0.85, with
`mikebom:evidence-kind = rpmdb-sqlite`.

**Dep graph:** full tree from rpmdb `REQUIRES` tags.

**Hashes:** **none.** rpmdb doesn't record per-package content hashes
mikebom can use. This is why rpm scans score 6.1/10 on sbomqs (Integrity
0/10) — the ecosystem itself doesn't provide the data.

**Enrichment:**
- deps.dev: skipped (not in deps.dev's supported ecosystems).
- ClearlyDefined: skipped (CD's rpm coverage is thin).

**Known limitations:**
- **Berkeley DB rpmdb** (`/var/lib/rpm/Packages`, pre-RHEL 8) is
  **detected but not parsed.** Diagnostic logged, zero rpm components
  emitted. The `--include-legacy-rpmdb` flag (or
  `MIKEBOM_INCLUDE_LEGACY_RPMDB=1`) threads through to
  `rpmdb_bdb::read`, which is a stub pending the concrete Hash/BTree
  page parser (milestone 004 US4 tasks T061–T065). Until those land,
  flipping the flag changes nothing about scan output.
- **rpmdb.sqlite size cap** is 200 MB (defense-in-depth; real rpmdbs are
  ~5 MB).
- **Pure-Rust SQLite reader** handles leaf-table + interior-table pages
  only. Overflow pages are refused. RHEL rpmdbs don't use overflow pages
  in practice.

---

## yocto

**Module:** `mikebom-cli/src/scan_fs/package_db/opkg.rs`
+ `mikebom-cli/src/scan_fs/package_db/yocto/{context,manifest,recipe}.rs`

Yocto / OpenEmbedded coverage (milestone 107). Three complementary
readers cover the embedded-Linux scan shapes that mikebom previously
emitted empty SBOMs for: device rootfs scans, build-directory scans,
SDK sysroot scans, and layer-tree scans. Together they close the
largest C/C++ source coverage gap that was deferred from milestone 105
(US7).

### Reader 1: opkg installed-DB (`opkg.rs`)

**Detection:** stanza parser over `/var/lib/opkg/status` (byte-identical
RFC-822 control-file syntax to dpkg; shares the
`package_db/control_file.rs` helper). Plus per-package
`/usr/lib/opkg/info/<pkg>.list` files for binary-walker claim
collection (prevents duplicate `pkg:generic/<basename>` emissions for
files already owned by an opkg package).

**Triggers on:** Yocto-built device rootfs, OpenSTLinux SDK sysroots,
Poky reference images, Wolfi-/Chainguard-derived images, every
OE-based distribution that doesn't explicitly opt into rpm or dpkg.

**PURL:** `pkg:opkg/<name>@<version>?arch=<arch>` — segments
percent-encoded per the package-url spec. Architecture passes through
verbatim from the stanza (`cortexa7t2hf-neon-vfpv4` survives intact).

**Lifecycle scope (FR-005a two-signal sysroot detection):** the reader
calls `yocto::context::detect_scan_context(rootfs)` once per scan and
tags every emitted entry accordingly:

- **Primary signal**: an `environment-setup-*` file anywhere from the
  scan target up to 2 ancestors above (Yocto's SDK installer always
  writes one alongside the sysroot).
- **Secondary signal**: `/usr/include/` present AND `/etc/init.d/`
  absent within the scan target.
- Sysroot context (either signal fires) → every entry tagged
  `LifecycleScope::Build` → emits CDX `scope: "excluded"` / SPDX
  `BUILD_DEPENDENCY_OF`. Ambiguity (primary fires AND `/etc/init.d/`
  is actively present) records a `mikebom:scan-ambiguity` annotation
  on the SBOM metadata but still applies build-scope (primary wins).

**Per-stanza FR-006 override:** `nativesdk-` prefixed packages OR
packages whose `Architecture:` field matches a known host-arch
literal (`x86_64` / `i686` / `aarch64` / `arm64`) are ALWAYS tagged
build, regardless of the context-level result. Catches nativesdk
packages that ship inside an otherwise-runtime rootfs.

### Reader 2: Yocto image manifest (`yocto/manifest.rs`)

**Detection:** walks `build/tmp/deploy/images/<machine>/*.manifest`
(one level under `images/`, non-recursive). Each line: `<name> <arch>
<version>` whitespace-separated. Format is stable since Yocto 2.0
(2015) and produced by every BitBake image build.

**PURL:** `pkg:opkg/<name>@<version>?arch=<arch>` — same ecosystem
as the installed-DB reader. Cross-source dedup collapses identical
coords via the milestone-105 pipeline (FR-010 precedence:
`OpkgInstalled` > `YoctoImageManifest`, so when both readers fire on
the same scan, installed-DB wins and the manifest's source-mechanism
appears in `mikebom:also-detected-via`).

**Lifecycle scope:** runtime by default. Per-line FR-006 override
applies the same nativesdk/host-arch checks as the opkg reader.

**Annotation:** `mikebom:image-name = <manifest-filename-stem>` so
downstream consumers can group components by image variant when
multiple manifests exist alongside each other.

### Reader 3: BitBake recipe walker (`yocto/recipe.rs`)

**Detection:** walks the scan tree (max_depth=8) for `.bb` files.
`.bbappend` and `.bbclass` files are silently ignored. Filename-only
parse via the regex
`^(?P<name>[a-zA-Z0-9_\-\+\.]+)_(?P<version>[a-zA-Z0-9_\-\+\.\~]+)\.bb$`.
Recipe BODY is NOT parsed in this milestone (FR-007 explicit scope
boundary — BitBake variable expansion is out of scope).

**Triggers on:** Yocto layer repositories (`meta-<vendor>/` directory
trees) checked out in isolation, BEFORE any build runs. Useful for
supply-chain pre-screening of vendor layers before adoption.

**PURL:** `pkg:bitbake/<name>@<version>?layer=<layer-name>` — distinct
ecosystem from `pkg:opkg/` because recipes are declarations, not
installed packages. Cross-tier emissions (installed-DB + recipe-tier
naming the same logical package) keep BOTH components because the
PURL ecosystem differs; consumers can filter by ecosystem.

**Layer-root detection:** walks UP from each recipe's directory
looking for the enclosing `meta-<name>/` directory. Fallback when no
`meta-*/` ancestor exists: returns the path component immediately
above the first `recipes-*/` directory.

**Skip-with-warn cases:**

- Filenames containing unexpanded `${` (e.g., `${PN}_${PV}.bb`) →
  silently skipped (FR-008). Downstream consumers who care about
  which recipes were skipped can grep the scan logs.
- `.bb` files with no `_<version>` segment → emitted with
  `version: "unknown"` + `mikebom:version-status: "missing"`
  annotation.

### Out of scope (this milestone)

- BitBake variable expansion in `.bb` recipe bodies. Recipe-tier
  emission is filename-only.
- `bitbake -e` introspection / `bitbake-layers` subprocess calls.
  Filesystem-only per FR-011.
- Dependency edges between recipes (`DEPENDS`, `RDEPENDS_${PN}`).
  Recipe-tier emission is identity-only.
- `Directory.Build.props`-style overlay handling.
- Yocto-specific license-name translation. License fields flow
  verbatim through the existing SPDX-expression pipeline.

### Enrichment

- deps.dev: skipped (not in deps.dev's supported ecosystems).
- ClearlyDefined: skipped (not curated).

Licenses on opkg-installed components come from the `License:`
stanza field directly when present. The Yocto image-manifest
format doesn't carry licenses, so those entries ship with empty
`licenses[]`.

---

## Further reading

- [Scanning architecture](architecture/scanning.md) — how the scan layer
  dispatches to each of these modules.
- [PURLs and CPEs](architecture/purls-and-cpes.md) — the canonicalization
  rules and CPE candidate strategy.
- [Enrichment](architecture/enrichment.md) — deps.dev + ClearlyDefined
  wiring.
- [design-notes.md](design-notes.md) — dated changelog, sharp edges, the
  deferred backlog including per-ecosystem ClearlyDefined expansions and
  sbomqs score-lift items.

## Binary analysis — symbol-fingerprint corpus (milestone 099 + 108)

mikebom's binary scanner identifies statically-linked C libraries from
their exported-symbol fingerprints (ELF `.dynsym` + Mach-O `LC_SYMTAB`
externals — PE deferred). The bundled fallback corpus ships 7 libraries
(openssl, zlib, libcurl, sqlite, pcre, pcre2, gnutls) and stays at
that size as a stability floor; the source-of-truth corpus lives in
the sibling repo
[`kusari-sandbox/mikebom-fingerprints`](https://github.com/kusari-sandbox/mikebom-fingerprints)
and grows independently of mikebom releases.

Operators opt into the external corpus per scan via
`--fingerprints-corpus` (or `MIKEBOM_FINGERPRINTS_CORPUS=1`):

```bash
mikebom sbom scan --image ghcr.io/myorg/my-app:v1 --fingerprints-corpus
```

The cache-first / fetch-on-miss flow, the
`mikebom:fingerprint-corpus-sha` provenance annotation, the
`mikebom fingerprints fetch/cache-clear/list` subcommands, and the
4-step consumer lookup recipe are documented end-to-end in:

- [`docs/reference/identifiers.md` §11](reference/identifiers.md#section-11--milestone-108-external-corpus-provenance-mikebomfingerprint-corpus-sha)
  — annotation value space + per-format carriers + lookup recipe.
- [`specs/108-fingerprint-corpus/quickstart.md`](../specs/108-fingerprint-corpus/quickstart.md)
  — operator + air-gapped + hermetic-build scenarios.
- [`kusari-sandbox/mikebom-cmake-demo`](https://github.com/kusari-sandbox/mikebom-cmake-demo)
  — runnable cmake + ninja demo that exercises both the source-tree
  reader AND the fingerprint matcher end-to-end.

### Milestone 109 — cross-tier PURL attribution for cmake projects

When mikebom scans a cmake project root with `--fingerprints-corpus`
(alpha.45+), fingerprint matches in built binaries are attributed to
the source-tier PURL the cmake reader emitted from
`FetchContent_Declare` (`pkg:github/madler/zlib@v1.3.1`) instead of
the milestone-108 generic shadow (`pkg:generic/zlib`). The mechanism:

1. Walk the scan root for cmake project build dirs (`CMakeCache.txt`
   + `_deps/` co-presence at depth ≤6).
2. For each cmake `FetchContent_Declare` source declaration that
   produced a `_deps/<name>-build/` directory, register an
   attribution observation.
3. When a fingerprint match's library name (case-insensitive)
   resolves against the registry AND the matched binary lives under
   the cmake project's build dir, rewrite the match's PURL to the
   source-tier value.
4. The dedup pipeline then merges the source-tier + binary-tier
   components by shared PURL into ONE final component carrying both
   sources' evidence (`mikebom:source-mechanism = cmake-fetchcontent-git`
   AND `mikebom:fingerprint-corpus-sha = <sha>` AND
   `mikebom:fingerprint-symbols-matched = "10/10"`).

Scope: `FetchContent_Declare` only (git + url forms). `ExternalProject_Add`,
Bazel, Meson, and hand-written Makefiles are out of scope this
milestone but the architecture accommodates them as follow-on
observers feeding the same registry. Operators scanning a SINGLE
binary (no source tree) or running without `--fingerprints-corpus`
see milestone-108 behavior unchanged.

Full design + contracts in [`specs/109-binary-source-purl-binding/`](../specs/109-binary-source-purl-binding/).
