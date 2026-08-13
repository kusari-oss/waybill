# Ecosystems

Per-ecosystem coverage for all nine ecosystems Waybill supports. Use this
page to answer *"does Waybill see my packages the way I expect"* before
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
| [pants (Python)](#pants-python) | Pex lockfile at `3rdparty/python/*.lock` (default glob) or `pants.toml`-declared path; multi-resolve support | Lockfile (`requires_dists` PEP 508 → `dependsOn`) | Lockfile per-artifact `sha256` | — / ✓ (via `pkg:pypi/`) | Implemented (milestone 223) |
| [pants (JVM)](#pants-jvm) | Coursier lockfile at `3rdparty/jvm/*.lock` (default glob) or `pants.toml` `[jvm.resolves]`-declared path; multi-resolve support; Pants-header discriminator vs standalone coursier | Lockfile (`dependencies[]` coord strings → `dependsOn`) | Lockfile per-artifact `sha256` | — / ✓ (via `pkg:maven/`) | Implemented (milestone 224) |
| [pants (shell)](#pants-shell) | `BUILD` files declaring `shell_source` / `shell_sources` / `shunit2_test` / `shunit2_tests` targets under scan root; plus `pants.toml` `[shellcheck]` / `[shfmt]` / `[shunit2]` version pins | File-tier (per-`.sh` component with SHA-256) + design-tier (per-pinned-tool `pkg:generic` component) | Per-file SHA-256 (content-addressed) | — / — | Implemented (milestone 225) |
| [pants (Go)](#pants-go) | `BUILD` files declaring `go_binary` / `go_package` / `go_third_party_package` / `go_mod` targets under scan root; plus `pants.toml` `[golang] expected_version` toolchain pin | Enrichment only (attaches `waybill:pants-target` to `pkg:golang/*` components emitted by the existing Go reader) + design-tier `pkg:generic/go@<version>` from `expected_version` | Per-module `sha1` (Go reader's existing go.sum-derived hash — unchanged) | — / — | Implemented (milestone 226) |
| [kotlin](#kotlin) | `build.gradle.kts` (regex) + `gradle/libs.versions.toml` (catalog) + `settings.gradle.kts` (workspace topology) | Manifest (declarations only) | — | ✓ (via `pkg:maven/`) / ✓ | Implemented (milestone 122 US2; design-tier — gated by `--include-declared-deps`) |
| [rpm](#rpm) | `/var/lib/rpm/rpmdb.sqlite` (pure-Rust reader) | DB (`REQUIRES`) | — (rpmdb has none) | — / — | Implemented (BDB format detected, not parsed) |
| [swift](#swift) | `Package.resolved` (SwiftPM v1/v2/v3 schema) | Lockfile (full pin set) | — | — / — | Implemented (milestone 122 US1; `Package.swift` not parsed in v0.1) |
| [yocto](#yocto) | opkg `/var/lib/opkg/status` + `build/tmp/deploy/images/*/*.manifest` + `meta-*/recipes-*/<name>/<name>_<version>.bb` | Flat (per-stanza / per-line / per-recipe) | — | — / — | Implemented |

"Enrichment" columns mark whether deps.dev version info and ClearlyDefined
concluded licenses apply to the ecosystem. Both honour the global
`--offline` flag.

---

## SBOM tiers: source, design, binary

Every component waybill emits carries a **tier** — a per-component classification
indicating the strength of provenance backing the version claim. Understanding
tiers is critical for consumers deciding whether to trust the SBOM for their use
case (compliance, vulnerability scanning, license analysis, etc.).

The three tiers reflect three fundamentally different provenance stories:
**source-tier** components are backed by a lockfile or resolved manifest and
carry an exact version; **design-tier** components are declared in a manifest
but no authoritative resolution is available (usually because no lockfile is
committed); **binary-tier** components are extracted from an actual compiled
artifact (ELF / PE / Mach-O / `.deps.json` sidecar / RPM header). A single scan
can — and often does — produce a mix of all three tiers in one output SBOM.

### 1. Concept — what are source, design, binary tiers

| Tier | Waybill emits when… | PURL shape | `waybill:sbom-tier` value | Detection recipe |
|---|---|---|---|---|
| **source** | A lockfile pins the version, OR the manifest carries an unambiguous version (e.g., `Cargo.toml` `[package] version = "1.2.3"`). | `pkg:<type>/<name>@<version>` | `"source"` | [Recipe 1](#3-detection-recipes-jq-for-cyclonedx--spdx) |
| **design** | A manifest declares the dependency but resolution cannot produce a version (no lockfile, no CPM entry, unresolvable `$()` MSBuild ref, etc.). Waybill emits a **versionless PURL** so downstream vuln scanners don't false-match on an invalid `@unresolved` literal. | `pkg:<type>/<name>` (no `@`) | `"design"` | [Recipe 2](#3-detection-recipes-jq-for-cyclonedx--spdx) |
| **binary** | A compiled artifact is scanned directly — ELF / PE / Mach-O symbol extraction, PE CLR `.deps.json` parsing, `runtime/debug.BuildInfo` from a Go binary, RPM header from a `.rpm` file, etc. | `pkg:<type>/<name>@<version>` (version comes from binary metadata) | `"binary"` | Grep for `"waybill:sbom-tier": "binary"` |
| _file_ | A file is unattributed by any package/binary reader — surfaced under the [file-tier orphan fallback](reference/component-tiers.md). Not covered further in this section; see the [component-tiers reference](reference/component-tiers.md). | No PURL (identified by SHA-256 + path) | `"file"` | See [component-tiers.md](reference/component-tiers.md). |

**Multi-tier scans are the norm, not the exception.** A monorepo with a Cargo
workspace (source-tier — has `Cargo.lock`) AND a Python subproject with only
`pyproject.toml` (design-tier — no `uv.lock` / `poetry.lock`) produces both
tiers in the same output SBOM. The `waybill:sbom-tier` property on each
component tells consumers which is which. See §5 for guidance on when
design-tier is enough and when it isn't.

> **Why `waybill:sbom-tier` is a `waybill:*` property, not a native CDX/SPDX
> field**: neither CycloneDX 1.6, SPDX 2.3, nor SPDX 3.0.1 has a native field
> carrying the "was this version resolved authoritatively vs declared-only"
> semantic. Per constitution Principle V (standards-native fields take
> precedence over `waybill:*` properties), this annotation is a parity-bridge
> introduced only because no native construct exists. See
> [sbom-format-mapping.md](reference/sbom-format-mapping.md) for the catalog
> entry.

### 2. Per-ecosystem design-tier fallback matrix

This matrix covers the 17 ecosystems documented in the coverage matrix above.
For every ecosystem, it answers: does waybill emit design-tier components
when the operator's project lacks a lockfile? What triggers the fallback?
What PURL shape results? Is the `waybill:unresolved-reason` annotation
attached?

| Ecosystem | Design-tier fallback? | Trigger condition | PURL shape | `waybill:unresolved-reason` emitted? |
|---|---|---|---|---|
| [apk](#apk) | No — always source | Installed-DB scan: version is always known. | `pkg:apk/<distro>/<name>@<version>` | N/A |
| [cargo](#cargo) | Yes — automatic | `Cargo.toml` declares a dep with no matching `Cargo.lock` entry, OR version field is empty. | `pkg:cargo/<name>` (versionless) or `pkg:cargo/<name>@<version>` | No (not yet — universal adoption is [follow-up](#follow-up)) |
| [deb](#deb) | No — always source | Installed-DB scan: version is always known. | `pkg:deb/<distro>/<name>@<version>` | N/A |
| [gem](#gem) | Yes — automatic | Gemfile declaration with no matching `Gemfile.lock` entry (design-tier), OR synthetic Ruby built-in gems (allowlist). | `pkg:gem/<name>` (versionless) | No |
| [golang](#golang) | Rare | `go.mod` present but no `go.sum` — resolver falls back to design-tier for the missing modules. Ordinarily go.sum is authoritative. | `pkg:golang/<module>@<version>` (if pseudo-version can be inferred) or versionless | No |
| [maven](#maven) | Yes — automatic | `pom.xml` declares a dep with no `<version>` element (typically inherited-scope declarations that would resolve via `mvn` subprocess — waybill doesn't shell to mvn), OR version contains `${…}` property syntax and the property isn't in the parsed POM chain. | `pkg:maven/<group>/<name>` (versionless) or `@<literal-${…}>` legacy | No |
| [npm](#npm) | Yes — automatic | `package.json` declares a dep but no lockfile (`package-lock.json` / `pnpm-lock.yaml` / `yarn.lock` / `bun.lock`) covers it. | `pkg:npm/<name>@<declared-range>` or versionless | No |
| [nuget](#nuget) | Yes — automatic (post [#653](https://github.com/kusari-oss/waybill/pull/656)) | 4-step resolution ladder exhausted: no `packages.lock.json`, no CPM entry in `Directory.Packages.props`, no inline `Version=`, no matching `<PackageVersion>` in `Directory.Build.props`/`targets`. | `pkg:nuget/<name>` (versionless) | **Yes** — the only reader today emitting `waybill:unresolved-reason` |
| [pip](#pip) | Yes — automatic | `requirements.txt` declares a dep with no matching resolved lockfile (`uv.lock` / `pip-tools` / `poetry.lock`), OR extras cause an unresolved constraint. | `pkg:pypi/<name>` or `pkg:pypi/<name>@<constraint>` | No |
| [pants (Python)](#pants-python) | No — always source (Pex lockfile is the authoritative source) | Every `pkg:pypi/*` entry comes from a Pex lockfile that pins exact versions + sha256. | `pkg:pypi/<name>@<version>` | N/A |
| [pants (JVM)](#pants-jvm) | No — always source (Coursier lockfile is authoritative) | Every `pkg:maven/*` entry comes from a Coursier lockfile. | `pkg:maven/<group>/<name>@<version>` | N/A |
| [pants (shell)](#pants-shell) | Yes — synthetic design-tier | `pants.toml` `[shellcheck]` / `[shfmt]` / `[shunit2]` tool pins emit ONE synthetic design-tier `pkg:generic/<tool>@<version>` per pinned tool. | `pkg:generic/<tool>@<version>` (version present but design-classified) | No |
| [pants (Go)](#pants-go) | Yes — synthetic design-tier | `pants.toml` `[golang] expected_version` emits ONE synthetic design-tier `pkg:generic/go@<version>` (toolchain pin, not a package dep). | `pkg:generic/go@<version>` | No |
| [rpm](#rpm) | No — always source | Installed-DB scan: version + release always known. | `pkg:rpm/<distro>/<name>@<version>-<release>` | N/A |
| [swift](#swift) | No — always source | `Package.resolved` is authoritative. Note: `Package.swift` NOT parsed in v0.1, so projects with only `Package.swift` (no `Package.resolved`) emit no components at all — see [swift limitations](#known-limitations-swift-v01). | `pkg:swift/<host>/<name>@<version>` | N/A |
| [kotlin](#kotlin) | Yes — **opt-in only** | `--include-declared-deps` flag enables Kotlin DSL declaration emission at design-tier. Default: no emission. Rationale: Gradle KTS DSL cannot be fully resolved without a Gradle daemon. | `pkg:maven/<group>/<name>@<constraint>` | No |
| [yocto](#yocto) | Yes — recipe scope always design-tier | `.bb` recipes have no "resolved" state — every recipe-derived component is inherently design-tier. Yocto installed-DB (opkg) is source-tier separately. | `pkg:generic/<recipe>@<pv>` | No |

**Cross-reader consistency gap** (see §4): only NuGet emits the
`waybill:unresolved-reason` annotation today. Other design-tier readers set
`waybill:sbom-tier: "design"` and use a versionless PURL, but don't attach a
human-readable reason. Consumers should treat annotation ABSENCE as "no
reason provided", not "component was resolved".

**Readers without per-ecosystem sections in this doc**: waybill supports
additional ecosystems whose readers exist in source but don't yet have
dedicated sections here — **cocoapods, composer, dart, elixir, erlang,
haskell, helm, scala, ipk**. Each of those readers emits design-tier
fallback per the same convention (versionless PURL + `sbom_tier: "design"`).
Documenting these ecosystems' per-section coverage is a separate follow-up.

### 3. Detection recipes (jq for CycloneDX + SPDX)

The `waybill:sbom-tier` property is emitted on every component that carries
a tier classification. Consumers filter by grepping this property.
All recipes below use jq 1.6+ syntax and have been verified against real
waybill-emitted SBOMs.

**Recipe 1 — Filter to source-tier only (CycloneDX)**:

```bash
jq '[.components[] | select(any(.properties[]?; .name == "waybill:sbom-tier" and .value == "source"))]' <sbom.cdx.json>
```

Returns a JSON array of components whose `waybill:sbom-tier` property is
`"source"`. Verified 2026-08-05 against `orleans.postfix.cdx.json`
(1020 source-tier components returned; 20 design-tier excluded).

**Recipe 2 — Filter to design-tier only (CycloneDX)**:

```bash
jq '[.components[] | select(any(.properties[]?; .name == "waybill:sbom-tier" and .value == "design"))]' <sbom.cdx.json>
```

Returns design-tier components (versionless PURLs). Verified against
`orleans.postfix.cdx.json` (20 design-tier components returned).

**Recipe 3 — Filter to source-tier only (SPDX 2.3)**:

```bash
jq '[.packages[] | select(any(.annotations[]?; .comment | test("\"waybill:sbom-tier\"[^\"]*\"source\"")))]' <sbom.spdx.json>
```

The `waybill:sbom-tier` value is embedded inside a JSON envelope in the
`annotations[].comment` field (waybill's milestone-071 annotation envelope).
The regex matches the specific field:value pair inside that envelope.

**Recipe 4 — Filter to design-tier only (SPDX 2.3)**:

```bash
jq '[.packages[] | select(any(.annotations[]?; .comment | test("\"waybill:sbom-tier\"[^\"]*\"design\"")))]' <sbom.spdx.json>
```

Same pattern as Recipe 3 for the design-tier value.

**Recipe 5 — Extract `waybill:unresolved-reason` per design-tier component (CycloneDX)**:

```bash
jq '.components[] | select(any(.properties[]?; .name == "waybill:sbom-tier" and .value == "design")) | {purl, reason: (.properties[]? | select(.name == "waybill:unresolved-reason") | .value)}' <sbom.cdx.json>
```

Returns one object per design-tier component with its unresolved-reason.
**Note**: only NuGet-emitted design-tier components currently include
this annotation (see §4). Non-NuGet design-tier components return
`{"purl": "…", "reason": null}` from this recipe — treat null as "no
reason provided", not "component was resolved".

**Recipe 6 — Count components per tier**:

CycloneDX:

```bash
jq -r '.components[]? | .properties[]? | select(.name == "waybill:sbom-tier") | .value' <sbom.cdx.json> | sort | uniq -c
```

SPDX 2.3:

```bash
jq -r '.packages[]? | .annotations[]?.comment | capture("\"waybill:sbom-tier\"[^\"]*\"(?<v>[a-z]+)\"") | .v' <sbom.spdx.json> | sort | uniq -c
```

Both return a per-tier count line like:

```text
      20 design
    1020 source
```

The counts sum to the total component count of components that carry
a tier annotation. Components without a `waybill:sbom-tier` (rare — mostly
operator-supplemented components from `--supplement-cdx`) are excluded from
these totals.

### 4. The waybill:unresolved-reason annotation

When waybill emits a design-tier component because its version can't be
resolved, it MAY attach a human-readable string annotation explaining why.
This helps operators quickly identify the missing input (lockfile? CPM
entry? something else?) and remediate.

**Where the annotation appears**:

- **CycloneDX**: as a `properties[]` entry alongside `waybill:sbom-tier`:

  ```json
  {
    "purl": "pkg:nuget/Aspire.Hosting.AppHost",
    "properties": [
      {"name": "waybill:sbom-tier", "value": "design"},
      {"name": "waybill:unresolved-reason", "value": "no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry"}
    ]
  }
  ```

- **SPDX 2.3**: embedded in an `annotations[].comment` field inside the
  milestone-071 annotation envelope (schema `mikebom-annotation/v1`, field
  `waybill:unresolved-reason`, value the reason string).

- **SPDX 3.0.1**: as an `Annotation` element attached to the component.

**Adoption status (as of 2026-08-05)**: **only the NuGet reader** emits this
annotation today. Discovered via `grep -rn '"waybill:unresolved-reason"'
waybill-cli/src/` — the annotation was introduced by [#653](https://github.com/kusari-oss/waybill/pull/656)
as part of the 2026-08-04 NuGet audit follow-up.

**Consumer interpretation guidance**:

- **Present** — the string tells the operator which lockfile or manifest field
  is missing. Downstream tools SHOULD surface the string verbatim to human
  reviewers as remediation guidance.
- **Absent** — treat as "no reason provided", NOT as "component was
  resolved". Consumers MUST rely on `waybill:sbom-tier: "design"` as the
  authoritative tier signal; the reason string is supplementary.

**Cross-reader consistency gap** — the other 17 design-tier-emitting readers
(cargo, gem, maven, npm, pip, kotlin_dsl, yocto, cocoapods, composer, dart,
elixir, erlang, haskell, helm, scala, pants_shell, pants_go) do NOT emit
this annotation. A follow-up issue tracks universalizing it across all
readers so consumers get consistent explanations regardless of ecosystem.

**Concrete value shape** — the one currently-emitted example, from NuGet:

> `no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry`

The string names the specific waybill resolution ladder that ran out. Other
readers adopting this annotation SHOULD use similar
"named-inputs-that-failed" strings so operators can identify the specific
remediation.

### 5. When design-tier is enough vs when it isn't

The single most important consumer-facing question about design-tier SBOMs
is: **can I make decisions from this data?** The answer depends on the
decision.

**Design-tier is enough for**:

- **Compliance attribution** — CISA 2026 Minimum Elements requires component
  identity but doesn't mandate exact-version resolution for every entry.
  Design-tier components with versionless PURLs satisfy component-name
  disclosure obligations.
- **Contract audits** — "does the vendor use component X?" is answerable
  from a design-tier PURL that shows `pkg:cargo/tokio` even without an
  exact version.
- **Declared-inventory manifests** — legal / procurement reviews focused on
  what the developer authored (not what was actually pulled in transitively).
- **Third-party-code disclosure** — enumerating direct dependencies for
  license-attribution notices in shipped software.
- **First-pass architectural review** — understanding what a project _intends_
  to depend on, without needing the build to have run.

**Design-tier is NOT enough for**:

- **Exact-version CVE scanning** — this is the highest-impact caveat. A
  vulnerability scanner running an exact-version CVE match against a
  versionless PURL like `pkg:cargo/serde` produces **silent false negatives**:
  the query returns "no matching CVEs" not because the component is safe,
  but because there's no version to match against. This is not a loud error;
  it's a quiet miss. Consumers running vuln scans MUST filter to source-tier
  only (see [Recipe 1](#3-detection-recipes-jq-for-cyclonedx--spdx)) OR
  upgrade the SBOM to source-tier first (see §7).
- **Transitive-graph analysis** — design-tier reflects declared dependencies
  only, not the full resolved transitive closure. Analysis that depends on
  the full graph (e.g., "does anything I depend on transitively pull in
  log4j?") requires source-tier resolution via a lockfile.
- **Transitive license-conflict analysis** — determining GPL contamination
  through transitive deps requires the resolved graph, not the declared set.
- **Dependency-confusion detection** — this requires knowing exactly which
  package resolved from which registry, which is a source-tier artifact.
- **SLSA level-3+ provenance claims** — SLSA build-integrity requirements
  presume the full resolved dependency graph is captured.

**The interpretive frame** — think of design-tier as **"declared inventory"**
and source-tier as **"resolved inventory"**. Both are legitimate SBOM shapes
for different questions. A design-tier SBOM answers "what did the developer
write into their manifests?"; a source-tier SBOM answers "what did the build
actually pull in?". Consumers acting on the wrong tier for their question
get systematically-wrong answers.

Waybill emits both tiers side-by-side when the input source tree contains
both lockfile-covered and lockfile-less projects — see §1's multi-tier
statement. Downstream filtering to the right tier for the task is the
consumer's responsibility; §3 provides the `jq` recipes.

### 6. Design-tier and the graph-completeness annotation

Waybill emits a document-scope `waybill:graph-completeness` annotation
(milestone 158) that summarizes whether the emitted SBOM has full transitive
coverage. Design-tier components interact with this signal directly: because
design-tier means the resolver ran out of authoritative inputs, transitive
edges below those components are often missing, and the completeness value
degrades to `"partial"`.

**The annotation shape** (CycloneDX `metadata.properties[]`):

```json
{"name": "waybill:graph-completeness", "value": "partial"}
{"name": "waybill:graph-completeness-reason", "value": "orphaned-components-detected: 570 component(s) not reachable from root"}
```

**Two distinct "partial" causes to distinguish**:

1. **Design-tier fallback** — components with `sbom_tier: "design"` exist,
   and their transitive edges are unknown to waybill. This is a
   resolver-input limitation, not a scanner bug.
2. **Unreachable-from-root orphans** — components exist in the SBOM but no
   dependency edge points from a root component to them. This can happen
   when a package-DB reader (e.g., dpkg) discovers system-installed
   packages that no application manifest references.

Both classes surface as `waybill:graph-completeness: partial`, but they
have different reason codes in the `waybill:graph-completeness-reason`
string. Consumers wanting to distinguish them should parse the reason
string OR count design-tier components separately.

**Recipe — extract completeness signal + reason**:

```bash
jq '.metadata.properties[]? | select(.name | startswith("waybill:graph-completeness"))' <sbom.cdx.json>
```

Returns both the completeness classification (`full` / `partial`) and the
reason string. For a design-tier-heavy scan the reason often names
"orphaned-components-detected" (design-tier components are orphans from
the perspective of the transitive graph pass) OR a design-tier-specific
reason code depending on the scan shape.

**Cross-reference**: see [component-tiers.md](reference/component-tiers.md)
for the file-tier orphan-fallback contract (a related but distinct
completeness surface).

### 7. Upgrading design-tier to source-tier

For each ecosystem with a design-tier fallback, there's a specific action the
operator can take to promote the components to source-tier before the next
scan:

- **cargo** — `cargo generate-lockfile` (or `cargo update`) writes `Cargo.lock`.
  Every dep gets a resolved version + checksum on the next waybill scan.
- **gem** — `bundle install` (or `bundle lock --update`) writes `Gemfile.lock`.
- **maven** — There's no in-tree solution for maven without shelling out to
  `mvn dependency:tree` (waybill doesn't shell to mvn today). Operators wanting
  full transitive resolution should either (a) commit a Gradle dependency-lock
  file (`gradle.lockfile` / `buildscript-gradle.lockfile` — waybill parses
  these) or (b) supply externally-known versions via `--supplement-cdx`.
- **npm** — Any of `npm install` / `pnpm install` / `yarn install` / `bun
  install` writes the ecosystem's lockfile.
- **nuget** — Add `<RestorePackagesWithLockFile>true</RestorePackagesWithLockFile>`
  to the `.csproj`, then run `dotnet restore`. Writes `packages.lock.json`
  alongside the project.
- **pip** — `uv lock` (or `pip-compile` / `poetry lock`) writes a resolved
  lockfile waybill's pip reader can consume.
- **kotlin** — `--include-declared-deps` opts into design-tier emission
  (this is the "trigger", not an upgrade). Full source-tier is
  available via milestone-235 `--gradle-resolve` (invokes the Gradle
  wrapper — requires JDK on `$PATH`) OR by committing a
  `gradle.lockfile` (milestone-106 flat-list). External
  supplementation via `--supplement-cdx` remains available.
- **yocto** — Recipe scope is inherently design-tier; source-tier is provided
  separately by the yocto opkg installed-DB reader when a built image is
  available.
- **golang** — Rare in practice (go.sum is usually committed alongside go.mod).
  If missing, run `go mod tidy` locally to regenerate go.sum, then re-scan.
- **swift** — Commit `Package.resolved` (usually generated by `swift package
  resolve` or by Xcode on first build). `Package.swift`-only projects will
  emit nothing (see [swift limitations](#known-limitations-swift-v01)).

**Universal operator-supplied override** — the `--supplement-cdx <file>` flag
(milestone 119) lets the operator overlay externally-known versions onto
components waybill couldn't resolve. Format: a CycloneDX SBOM whose
components carry the missing versions. Waybill merges by PURL name; the
supplement file's versions win for any design-tier components with matching
names.

**Per-ecosystem opt-in resolver flags** — waybill also exposes some ecosystem-
specific network-fetching resolvers as opt-in flags. Notable one:
`--warm-go-cache` (milestone 173) — pre-warms the Go module cache so
transitive resolution succeeds even in an offline environment. Not a
design-tier fix, but a related "how do I get more resolved components?"
lever.

### 8. Contributor guidance (implementing design-tier in a new reader)

Contributors implementing a new ecosystem reader should follow the
cross-reader design-tier convention when resolution can't produce a
version:

**The 4-field convention**:

1. **PURL shape**: emit a **versionless** PURL when the version is empty —
   `pkg:<type>/<name>` (no `@` segment). Consumer vulnerability scanners
   doing exact-version CVE lookups get "no match" instead of a false
   positive on an `@unresolved` literal.
2. **`sbom_tier: Some("design".to_string())`** on the `PackageDbEntry`.
   This is the authoritative tier classifier.
3. **`waybill:unresolved-reason` annotation** — a human-readable string
   explaining WHY the version couldn't be resolved (which lockfile is
   missing, which manifest field was empty, etc.). Currently only NuGet
   emits this; adopting it in your new reader helps close the [cross-
   reader consistency gap](#4-the-waybillunresolved-reason-annotation).
4. **Explicit trigger condition** documented in the reader's module doc-
   comment — spell out the specific "when X and Y and Z, fall through
   to design-tier" logic. Ambiguous fallbacks are hard for consumers
   to interpret and for future maintainers to reason about.

**Precedent readers to copy from** (verified 2026-08-05):

- `waybill-cli/src/scan_fs/package_db/gem.rs:385-402` — `build_gem_purl`
  handles the versionless-when-empty pattern cleanly. Look at how the
  `if version.is_empty()` branch produces `pkg:gem/<name>` vs the else
  branch producing `pkg:gem/<name>@<version>`.
- `waybill-cli/src/scan_fs/package_db/nuget/mod.rs:113` — `read_one_project`
  demonstrates the complete pattern post-[#653](https://github.com/kusari-oss/waybill/pull/656):
  4-step resolution ladder, versionless PURL fallback, `sbom_tier: "design"`
  emission, AND the `waybill:unresolved-reason` annotation.
- `waybill-cli/src/scan_fs/package_db/nuget/mod.rs:435` — `build_nuget_purl`
  is the constructor showing the branching `if version.is_empty()` /
  `format!("pkg:nuget/{}")` vs `format!("pkg:nuget/{}@{}")` shape.

**Anti-pattern to avoid — the `@unresolved` sentinel bug**: prior to
[#653](https://github.com/kusari-oss/waybill/pull/656), the NuGet reader
used a hardcoded `UNRESOLVED_VERSION_SENTINEL = "unresolved"` string,
emitting invalid PURLs like `pkg:nuget/Foo@unresolved`. Downstream SBOM
consumers (Trivy fs-scanning the emitted CDX, DependencyTrack)
**dropped these entries silently** — the string `unresolved` is not a
valid SemVer, so purl-spec-validating tools skip the components. **Do
not invent your own sentinel string.** The design-tier + versionless-PURL
convention above is the correct fallback shape.

**Testing your new reader's design-tier path** — add regression tests
following the pattern in
`waybill-cli/src/scan_fs/package_db/nuget/mod.rs::tests::unresolved_version_emits_design_tier_versionless_purl`
(post-#653): verify the PURL is versionless, verify `sbom_tier` is
`Some("design")`, and verify the annotation shape.

---

## Directory exclusion (--exclude-path)

Every ecosystem reader below honors operator-supplied directory exclusion via
`--exclude-path` (milestone 113, issue #108). The flag is cross-cutting —
literal paths or glob patterns suppress descent into matching directories
across every filesystem walker (Cargo, Maven, npm, pip, gem, Gradle, NuGet,
Yocto recipes, Go source modules, AND binary discovery).

Two entry forms:

- **Literal**: `--exclude-path tests/fixtures` — suppresses descent into
  any directory whose path matches the literal exactly OR starts with the
  literal followed by `/`. Path separators normalize to forward-slash at
  parse time, so backslash-separated literals (`tests\fixtures`) work
  identically.
- **Pattern**: `--exclude-path '**/testdata'` — `globset` semantics; `**`
  spans path separators. Patterns combine by union when the flag is
  repeated.

Set via the CLI flag (repeatable) OR the `WAYBILL_EXCLUDE_PATH` env-var
(platform path-list separator). When at least one entry is in effect,
emitted SBOMs carry a `waybill:exclude-path` transparency annotation
listing every entry, and a scan-end `tracing::info!` line surfaces
`excluded_entries=N excluded_literals=N excluded_patterns=N
suppressed_dirs=N` for operator inspection (milestone 118 / #343).

Built-in skip-list precedence: Waybill-internal skips (`.git`, `target`,
`node_modules`, `.cargo`, `__pycache__`, `.venv`) take precedence; an
operator cannot re-include them via `--exclude-path`. See
[`docs/user-guide/cli-reference.md` § `--exclude-path`](user-guide/cli-reference.md#--exclude-path-path_or_pattern)
for the full troubleshooting matrix + worked examples.

## Operator-supplied supplement (--supplement-cdx)

Across every ecosystem there's a class of dependencies the scanner
cannot observe: SaaS services with no on-disk footprint, vendored
libraries dropped into the source tree without a recognizable
manifest, and metadata gaps (license / supplier / copyright) on
otherwise-known components. `--supplement-cdx <PATH>` (milestone 119,
issue #326) lets the operator hand-author a small CDX 1.6 JSON file
declaring this ground truth; the merge runs once per scan, before
emission, so every output format sees the same combined view.

Three concrete use cases:

- **SaaS dependencies** (Stripe, Twilio, Auth0, …) appear under the
  emitted SBOM's `services[]` section — a CDX-native section the
  scanner never populates from on-disk evidence.
- **Vendored libraries with no manifest** appear as regular components
  tagged `waybill:source-tier = "declared"` so downstream consumers
  can distinguish declared from observed.
- **Metadata gaps** on scanner-discovered components are filled by
  the operator's declared values (licenses, supplier, copyright,
  description, externalReferences). Bytes-derived facts (hashes,
  cpe, version) the scanner read off disk continue to win.

Safety property: the operator **cannot** suppress scanner detection of
bytes-evident content. A supplement asserting "no openssl" against a
fingerprint-detected openssl still produces an SBOM containing the
openssl component; the assertion appears as an annotated conflict for
audit (`waybill:assertion-conflict`).

Provenance: when the flag is in effect, the emitted SBOM carries a
document-scope `waybill:supplement-cdx = "<path>@sha256:<hex>"`
annotation so consumers can verify which supplement file fed the
merge.

Parse / I/O / schema-validation failures fail closed before any
walker begins — no partial SBOM is ever emitted on supplement
failure. See
[`docs/user-guide/cli-reference.md` § `--supplement-cdx`](user-guide/cli-reference.md#--supplement-cdx-path)
for the full file format, hard/soft conflict-split rules, worked
example, and troubleshooting matrix.

---

## apk

**Module:** `waybill-cli/src/scan_fs/package_db/apk.rs`

**Detection:** stanza parser over `/lib/apk/db/installed`. Reads `P:`
(name), `V:` (version), `A:` (arch), `D:` (direct dependencies).

**PURL format:** `pkg:apk/alpine/<name>@<version>?arch=<arch>&distro=alpine-<VERSION_ID>`
(e.g., `distro=alpine-3.19`). Same `<namespace>-<VERSION_ID>` shape as
deb and rpm.

**Evidence:** `PackageDatabase` / `manifest-analysis` at confidence 0.85.

**Dep graph:** direct dependencies only. apk's installed DB doesn't encode
transitive graph — it records only what each package declares.

**Hashes:** none. apk's installed DB doesn't carry per-package content
hashes Waybill can use.

**Enrichment:**
- deps.dev: skipped (not in deps.dev's supported ecosystems).
- ClearlyDefined: skipped (not curated).

**Known limitations:**
- apk's DB doesn't carry copyright pointers like dpkg does, so apk
  components ship with empty `licenses[]`.

---

> **Design-tier fallback**: No — always source (installed-DB scan; version is always known). See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## cargo

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

**Module:** `waybill-cli/src/scan_fs/package_db/cargo.rs`

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

> **Design-tier fallback**: Yes — automatic when a `Cargo.toml` declaration has no matching `Cargo.lock` entry. Versionless `pkg:cargo/<name>` emitted. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix) and [§7 upgrade](#7-upgrading-design-tier-to-source-tier).

---

## deb

**Module:** `waybill-cli/src/scan_fs/package_db/dpkg.rs`, with DEP-5
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

> **Design-tier fallback**: No — always source (installed-DB scan; version is always known). See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## gem

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

**Module:** `waybill-cli/src/scan_fs/package_db/gem.rs`

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
- Bundler's full scope semantics (`:development`, `:production`,
  grouped) aren't modeled. Test-scoped gems carry the milestone-052
  native scope tag (CDX `scope: "excluded"`, SPDX 2.3
  `TEST_DEPENDENCY_OF`, SPDX 3 `LifecycleScopeType: test`); operators
  use `--exclude-scope test` to drop them.
- Interpolated gemspec versions (`"#{FOO_VERSION}"`) produce garbage
  strings — downstream PURL construction rejects them. Theoretical edge
  case; in practice gemspec versions are always literal strings.

---

> **Design-tier fallback**: Yes — automatic when a Gemfile declaration has no matching `Gemfile.lock` entry (via `build_gem_purl` at `gem.rs:385`). Also synthetic Ruby built-in gems (allowlist, milestone 162). Versionless `pkg:gem/<name>` emitted. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## golang

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

**Modules:** `waybill-cli/src/scan_fs/package_db/golang.rs` (source scans),
`waybill-cli/src/scan_fs/package_db/go_binary.rs` (binary scans).

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

**Build-inclusion clarity (milestone 112):** `go.sum` routinely retains
entries for modules outside the final build list (test-only transitives,
pruned graph leftovers). Modules that Waybill could only attach via the
lower-fidelity fallback paths (the milestone-091 `go.sum` flat fallback,
recognizable by `waybill:resolver-step: go-sum-fallback`) get two layers
of treatment:

1. **Always-on marker:** every fallback-attached module carries
   `waybill:build-inclusion: unknown` — an explicit "Waybill cannot
   prove this module participates in the build" signal, instead of
   silently looking like a confirmed dependency. No native scope field
   is set (CDX `scope` absent ≠ `excluded`).
2. **Default-on classification:** when a `go` toolchain is found on
   PATH, Waybill runs `go mod why -m -vendor` against each main module
   (modules batched in chunks of 20, 60-second total budget shared
   across the scan) and upgrades the marker per verdict:
   - outside the build graph → `waybill:build-inclusion: not-needed` +
     CDX `scope: "excluded"` + `waybill:build-inclusion-derivation:
     go-mod-why`;
   - reachable only through `.test` packages → test lifecycle scope +
     `waybill:lifecycle-scope-derivation: go-mod-why`;
   - needed by ANY main module in the scanned tree → no marker (a
     module needed by one of several main modules is never excluded).

   Disable with `--no-go-mod-why` or `WAYBILL_NO_GO_MOD_WHY=1`; see the
   [CLI reference](user-guide/cli-reference.md#--no-go-mod-why).

**Degrade matrix:** classification never fails a scan — every failure
class falls back to the conservative `unknown` marker and logs a
warning, plus a one-line summary
(`go-mod-why classification: analyzed=… skipped=… elapsed_ms=…`):

| Failure | Skip class | Effect |
|---|---|---|
| no `go` on PATH | `no-toolchain` | markers only, no classification |
| flag / env var set | `disabled` | markers only (operator intent) |
| `go list all` preflight fails | `unresolvable-packages` | ALL verdicts for that main module rejected (guards against `go mod why` silently reporting false not-needed) |
| `go mod why` non-zero exit / spawn failure | `subprocess-error` | that chunk's modules stay `unknown` |
| 60 s budget exhausted | `budget-exhausted` | remaining modules stay `unknown`; verdicts already obtained are kept |

With `--offline`, the `go` children run with `GOPROXY=off`,
`GOFLAGS=-mod=mod`, `GOTOOLCHAIN=local` pinned so they can neither hit
the network nor self-upgrade. Catalog rows C60–C62 in the
[SBOM format mapping](reference/sbom-format-mapping.md) define how the
three annotations carry across CDX / SPDX 2.3 / SPDX 3.

### Binary scans

**Detection:** `runtime/debug.BuildInfo` inline-format decoder. Works for
Go 1.18+ binaries. Pre-1.18 binaries are flagged with
`waybill:buildinfo-status = unsupported` and emit a file-level component
only.

**PURL format:** same as source scans.

**Dep graph:** **none.** `runtime/debug.BuildInfo` encodes the module
list but not module-to-module relationships.

**Hashes:** the binary itself gets hashed (`ResolutionTechnique::FilePathPattern`
at 0.70 confidence with file-level evidence); individual modules don't.

**VCS metadata (milestone 025):** when the binary was built with
`-buildvcs=true` (the Go default since 1.18), three additional
annotations attach to the main-module entry: `waybill:go-vcs-revision`
(commit SHA from `vcs.revision`), `waybill:go-vcs-time` (RFC 3339
build timestamp from `vcs.time`), and `waybill:go-vcs-modified`
(dirty-tree boolean from `vcs.modified`, preserved as the literal
`"true"` / `"false"` string per Go's wire format). Surfaced via the
milestone 023 `extra_annotations` bag — same data `go version -m
<binary>` shows. Dep entries don't carry VCS metadata; that's a
main-module concern. Binaries built with `-buildvcs=false` or outside
a VCS worktree emit no `waybill:go-vcs-*` annotations.

**Known limitations:**
- Stripped binaries where BuildInfo extraction fails get
  `waybill:buildinfo-status = missing` and emit only as a file-level
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

### Build the binary for richer per-component classification

A source-only scan (`waybill sbom scan --path .` on a Go project
before `go build`) emits the full `go.sum` closure — every module
the resolver ever fetched, including build-tag alternatives the
linker DCE'd and test scaffolding never linked. With the binary
present, Waybill keeps the same components but annotates each one
the linker didn't embed with `waybill:not-linked = true`, so
consumers get both the broad lockfile view AND a precise
"what shipped" filter on a single SBOM. On `apigatewayv2/config`
(typical service): 65 modules with binary, 24 of them carrying
`waybill:not-linked`; consumers wanting the binary-tight view
filter on the property and see ~41:

```bash
go build .                                    # produces ./apigatewayv2-config
waybill sbom scan --path . --output app.cdx.json
# → 65 components, 24 carrying waybill:not-linked = true
jq '[.components[] | select(.properties[]? | select(.name=="waybill:not-linked") | not)]' app.cdx.json
# → strict "what shipped" view (~41 components, no annotation noise)
```

When no binary is found, Waybill emits a one-line `tracing::info`
hint pointing you at this workflow — no `waybill:not-linked` data
is computed in that case.

---

> **Design-tier fallback**: Rare — go.mod present but no go.sum. Ordinarily go.sum is committed alongside go.mod. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## maven

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

**Module:** `waybill-cli/src/scan_fs/package_db/maven.rs`

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
`maven-dependency-plugin`'s declared-transitive manifest), Waybill
parses it into ancestor coords and emits one nested component per
ancestor under the enclosing JAR's primary coord, tagged
`waybill:shade-relocation = true`. Emission is gated on
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
  `<exclusions>`, Waybill still emits the excluded coord.
- Version ranges (`[1.0,2.0)`) not resolved.
- `<profiles>` ignored — profile-conditional deps never emit.
- Plugin-section deps (`<build><plugins>`) ignored — not runtime deps.
- POM-less JARs (older Gradle outputs, OSGi bundles) can't be inspected
  via `META-INF/maven/` — coord + deps invisible.

### Gradle dependency-locking (milestone 106)

**Module:** `waybill-cli/src/scan_fs/package_db/gradle/`

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

**Annotations:** `waybill:gradle-configurations` carries the raw
comma-joined configuration list (informational; downstream filterable
by `compileClasspath` / `testRuntimeClasspath` / etc.).

**Dep graph:** flat. Gradle lockfiles don't encode parent → child
edges; each row is an already-resolved coord. The transitive-edge
gap is closed by milestone 235's subprocess resolver (below).

### Gradle transitive resolution (milestone 235; US1 subprocess live)

**Module:** `waybill-cli/src/scan_fs/package_db/gradle/` (siblings to
the milestone-106 `lockfile.rs`: `subprocess.rs`, `ladder.rs`,
`tier.rs`, plus stubs for the follow-on US2/US3 tiers).

**Opt-in:** `--gradle-resolve` — see the [CLI reference](user-guide/cli-reference.md#--gradle-resolve).
Absent the flag, Gradle projects fall through to milestone-106
lockfile reading (byte-identical pre-m235 behavior; FR-009
non-regression).

**Ladder** (spec: `specs/235-gradle-transitive-ladder/`):

| Tier | Mechanism | State |
|---|---|---|
| US1 subprocess | `./gradlew :<sub>:dependencies --no-daemon` → ASCII-tree parse | ✅ Live on `main` |
| US2 cache | Walks `~/.gradle/caches/modules-2/` cached POMs | 🟡 Stub; follow-on |
| US3 static | Regex-scoped extract from `build.gradle(.kts)` | 🟡 Partial (direct-dep helper only) |
| US4 annotations | `waybill:gradle-resolution-tier` transparency | 🟡 Follow-on |

**Detection:** any directory containing `build.gradle`,
`build.gradle.kts`, `settings.gradle`, or `settings.gradle.kts`.

**Subprocess flow (US1):**

1. Discover `./gradlew` in the project dir (or `gradlew.bat` on
   Windows). Fall back to `gradle` on `$PATH`.
2. Enumerate subprojects: `./gradlew projects --no-daemon --quiet`
   — parses `+--- Project ':<name>'` / `\--- Project ':<name>'`
   lines. Empty list → single-project build.
3. Per subproject × configuration (default set:
   `runtimeClasspath` + `testRuntimeClasspath` per clarify Q1):
   `./gradlew :<sub>:dependencies --configuration <config>
   --no-daemon --quiet`
4. Parse ASCII tree (`+--- g:a:v`, `\--- g:a:v (*)`, `g:a:v -> resolved`).
   Depth-based parent-child edge reconstruction. `(*)` dedup-marker
   entries record the coord but don't descend; `(c)` constraint
   markers are skipped entirely.
5. Assemble `Vec<PackageDbEntry>` + edges. Configurations map to
   `LifecycleScope`: `runtime*` → `Runtime`, `test*` → `Test`.

**Timeout:** 5 minutes per subprocess call by default; configurable
via `--gradle-timeout-secs`. On timeout: SIGTERM → 2s grace → SIGKILL,
then fall through to the next tier (m106 lockfile if present,
otherwise no components for that project).

**PURL format:** `pkg:maven/<group>/<name>@<version>` — same as m106
and the Maven ecosystem, so downstream deps.dev enrichment applies
unchanged.

**Configurations resolved:** default `runtimeClasspath` +
`testRuntimeClasspath`. Extend via `--gradle-extra-configurations
<name>` (repeatable). Buildscript classpath (Gradle plugins) reached
via `--gradle-resolve-buildscript`.

**Daemon:** default `--no-daemon` — waybill doesn't want to leave a
JVM in the operator's process list after a scan (see clarify Q2).
`--gradle-daemon` opts out for iterative local scanning.

**Constitution:** Principle I preserved. JDK is a runtime prerequisite
of the invoked wrapper, not a compile-time waybill dep — same posture
as m173's `go` shell-out and m053's `git describe` ladder.

---

> **Design-tier fallback**: Yes — automatic when a `pom.xml` declaration has no `<version>` element (typically inherited-scope declarations that would resolve via `mvn` subprocess — waybill doesn't shell to mvn), or when the version contains unresolved `${…}` property syntax. Versionless `pkg:maven/<group>/<name>` emitted. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## npm

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

**Module:** `waybill-cli/src/scan_fs/package_db/npm.rs`

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
  `waybill:npm-role = internal` and retained — the image contains
  npm's own install, so those bytes are legitimately present.
- In `--path` scans, internals are filtered out before resolution on
  the assumption that a path-mode scan targets the application's
  `node_modules/`, not a tool cache.
- This is not user-gated — there is no flag to toggle it. See
  feature 005 (`specs/005-purl-and-scope-alignment/`) for rationale.

### Bun lockfile (milestone 106)

**Module:** `waybill-cli/src/scan_fs/package_db/npm/bun_lock.rs`

**Detection:** `bun.lock` (Bun's JSONC lockfile format) at any
project root in the scan tree. Bun-only projects (no
`package-lock.json` / `pnpm-lock.yaml`) are picked up via the
`has_npm_signal` marker.

**Format:** JSONC (JSON with comments) — the `// bun: lockfileVersion: 1`
header comment is stripped before `serde_json::from_str` via the shared
`npm/jsonc.rs` helper. Parses `lockfileVersion`, `workspaces`,
`packages`, and `overrides` keys; unknown keys are silently ignored.

**Workspace support:** when `workspaces` declares members, Waybill
emits a synthetic workspace-root component (PURL: `pkg:generic/<name>`,
`waybill:component-role: "workspace-root"`) plus a `main-module`
component per member. Intra-workspace edges are harvested when a
member's `dependencies` declares `workspace:*` source-specs.

**Overrides:** when `overrides` is present, the overridden version
wins at registry-emission time; the un-overridden version is NOT
emitted as a separate component.

**PURL format:** `pkg:npm/<name>@<version>` — scoped names
URL-encode the `@` (`@scope/name` → `pkg:npm/%40scope/name@version`).

### Yarn lockfile (milestone 106)

**Module:** `waybill-cli/src/scan_fs/package_db/npm/yarn_lock.rs`

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

> **Design-tier fallback**: Yes — automatic when a `package.json` declaration has no matching lockfile entry (`package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, or `bun.lock`). See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## pip

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

**Module:** `waybill-cli/src/scan_fs/package_db/pip.rs`

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

**Module:** `waybill-cli/src/scan_fs/package_db/pip/uv_lock.rs`

**Detection:** `uv.lock` (TOML) at any project root in the scan tree.
Sibling to the existing Poetry / Pipfile readers; uv-only projects
are picked up via the `has_python_project_marker` walker.

**Format:** TOML `[[package]]` array. Each entry carries `name`,
`version`, and an optional `[[package.dependencies]]` sub-array
giving the resolved dep graph. Workspace projects additionally
declare members under `[tool.uv.workspace]` in the root
`pyproject.toml`.

**Workspace support:** Waybill emits a synthetic workspace-root
component (PURL: `pkg:generic/<name>`,
`waybill:component-role: "workspace-root"`) plus a `main-module`
per member. Intra-workspace dep edges are surfaced automatically
when a member's `[[package.dependencies]]` names a sibling member.

**PURL format:** `pkg:pypi/<name>@<version>` — PEP 503-normalized
name (lowercase, runs of non-alphanum collapsed to `-`).

**Dep graph:** full tree from `[[package.dependencies]]`.

---

> **Design-tier fallback**: Yes — automatic when a `requirements.txt` declaration has no matching resolved lockfile (`uv.lock` / `pip-tools` / `poetry.lock`), or when extras cause an unresolved constraint. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## pants (Python)

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

**Module:** `waybill-cli/src/scan_fs/package_db/pants/`

**Trigger:** any `3rdparty/python/*.lock` file, OR a `pants.toml` at the
scan root declaring `[python].lockfile = "..."`. Missing / malformed
`pants.toml` falls back gracefully to the default glob per FR-004
(milestone 223 US3).

**Reader:** parses [Pex lockfile format](https://pex.readthedocs.io/en/latest/lockfiles.html)
(JSON, Pex 2.x). One `PackageDbEntry` per locked distribution.

**PURL construction:**
- PyPI-hosted artifacts (`artifacts[].url` starts with
  `https://files.pythonhosted.org/`): `pkg:pypi/<name>@<version>` with
  PyPI-normalized name (lowercase + `_` → `-` via shared
  `pip::normalize_pypi_name_for_purl` helper).
- Git URLs (`git+*`), direct download URLs, or `file://` /
  absolute-path artifacts: `pkg:generic/<name>@<version>` plus
  `waybill:source-type` (C1) + `waybill:source-url` (C144) annotations
  identifying the non-PyPI source. Rationale: preserves vuln-scanner
  PURL semantics (they won't pivot to a fake PyPI CVE lookup on a
  git-sourced package).

**Dep graph:** full — `LockedRequirement.requires_dists[]` PEP 508
strings are parsed for project names (strip version specifiers,
extras, markers) and emitted as `dependsOn` edges.

**Multi-resolve support** (milestone 223 US1 / Q1 B): every discovered
lockfile is scanned. Every emitted component carries a
`waybill:pants-resolve` annotation (C143) naming the source resolve
(lockfile filename stem). Resolves whose name matches a dev-tool
allowlist (`mypy`, `pytest`, `black`, `ruff`, `isort`, `flake8`,
`bandit`, `coverage`, `sphinx`, `lint`, `test`, `dev`, `ci`, `check`,
`tools`, and their case-insensitive variants) tag components with
`LifecycleScope::Development`. Unknown resolve names default to
`Runtime` (safe default — matches operator intent for custom resolves
named after production concerns).

**Hashes:** every `Artifact.hash` field with `algorithm == "sha256"`
becomes one `ContentHash` on the component.

**Fail-open:** per-lockfile corruption (invalid JSON, unsupported
`pex_version`, missing `locked_resolves`) logs a WARN naming the
file + reason and skips it — the whole scan never aborts. Absent
`pants.toml`, missing `[python].lockfile` key, or malformed TOML all
fall back to the default glob without failing.

**FR-010 discovery log:** at scan-end, a single INFO log line with
four structured fields:

```text
INFO waybill::scan_fs::package_db::pants: pants-pex reader complete
  lockfiles_discovered=<N>
  lockfiles_parsed_ok=<N>
  lockfiles_skipped_corrupt=<N>
  components_emitted=<N>
```

Emitted only when at least one lockfile was discovered (silent on
non-Pants repos per FR-007 byte-identity guarantee).

**Coexistence with pip reader:** if a Pants repo carries a
`requirements.txt` alongside its Pex lockfile, both readers emit
entries. The m191 reconciler deduplicates at PURL level — the
lockfile-tier entry (with hashes) wins over the manifest-tier entry
(no hashes); `waybill:source-files` records both source paths for
audit. See US2 in the spec for detail.

**GitHub Actions users**: `sigstore/gh-action-sigstore-python` and
similar helper actions are for **signing** (feature 222), not
scanning — the pants reader has nothing to do with OIDC.

**Follow-up ecosystems** (out of scope for milestone 223): coursier
lockfiles at `3rdparty/jvm/*.lockfile` for Pants JVM targets; `BUILD`
file walker for design-tier signals; eBPF trace of `pex` / `pants`
subprocess invocations at build time.

---

> **Design-tier fallback**: No — always source. Pex lockfile pins exact versions + sha256 for every entry. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## nuget

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

**Module:** `waybill-cli/src/scan_fs/package_db/nuget/`

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
Waybill records what the source says; dedup handles cross-source
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
emitted with `waybill:source-type: "transitive"`.

**Dependency edges:** each lockfile entry's `dependencies` map
populates `PackageDbEntry.depends`. The standard scan orchestrator
drops edges whose target isn't present in the same scan.

**Source-files merging:** when multiple files contribute to the
same canonical PURL (e.g. `.csproj` + `Directory.Packages.props`
for CPM, or `.csproj` + `packages.lock.json` for direct deps), the
file paths merge into a single comma-joined `waybill:source-files`
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

> **Design-tier fallback**: Yes — automatic when the 4-step resolution ladder is exhausted (no `packages.lock.json`, no CPM entry, no inline `Version=`, no matching `<PackageVersion>` in `Directory.Build.props`/`targets`). Versionless `pkg:nuget/<name>` emitted. **NuGet is the only reader today that also emits `waybill:unresolved-reason`** — see [§4](#4-the-waybillunresolved-reason-annotation). Post-[#653](https://github.com/kusari-oss/waybill/pull/656). See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## pants (JVM)

**Modules:** `waybill-cli/src/scan_fs/package_db/pants_jvm/`
(`mod.rs`, `lockfile.rs`, `config.rs`, `coordinate.rs`,
`resolve_classifier.rs`).

**Detection:** discovers Pants-generated coursier lockfiles at
`<scan_root>/3rdparty/jvm/*.lock` (default glob) plus every
`[jvm.resolves]` path declared in `<scan_root>/pants.toml`.
Standalone coursier lockfiles (i.e., lockfiles produced by
`coursier resolve` directly, without Pants) are skipped via the
FR-011 discriminator: lockfiles that lack the
`# --- BEGIN PANTS LOCKFILE METADATA` header substring get an
INFO log and are excluded from ingestion. This prevents the
JVM reader from double-counting Maven components already picked
up by the standalone Maven reader.

**PURL construction:** `pkg:maven/<group>/<artifact>@<version>`
with optional `?classifier=<c>&type=<packaging>` qualifiers when
the coursier entry sets `classifier` or a non-default `packaging`
(anything other than `jar` — Maven's default). Segment encoding
matches the standalone Maven reader.

**Dependency edges:** parsed from each entry's `dependencies[]`
coord-strings via `coordinate::parse_coord_string`, which handles
the `"group:artifact:version[,url=X,jar=Y]"` shape by splitting
on the first `,` then `splitn(3, ':')` the triple. Metadata k/v
pairs after the triple are ignored (waybill needs only the
group/artifact for edge resolution). Edges emit as
`"group:artifact"` strings to align with the standalone Maven
reader's dep-edge convention.

**Multi-resolve support:** every `.lock` file discovered gets its
own `waybill:pants-resolve=<name>` annotation. The resolve name
comes from the lockfile filename stem OR (when config wins) the
`[jvm.resolves]` map key. Resolve names matching the JVM
dev-tool allowlist (scalatest, junit, testng, mockito, assertj,
hamcrest, scalafmt, scalastyle, scalafix, checkstyle, spotbugs,
pmd, errorprone, jacoco, dokka, ktlint, detekt, plus generics
lint / test / dev / ci / check / tools / docs) get
`lifecycle_scope = Development`; everything else gets `Runtime`.

**Dedup with the Maven reader:** the m191 PURL-level reconciler
handles it. Coursier lockfile entries carry both
`sbom_tier="source"` AND per-artifact sha256 hashes; standalone
`pom.xml` entries carry `"source"` tier but no hashes.
Hash-bearing entries win by rule (2) of the reconciler — the
lockfile-tier component survives, the pom-tier component is
absorbed into the winner's `waybill:source-files` annotation.

**FR-010 log:** one INFO line at scan end reports
`lockfiles_discovered=N`, `lockfiles_parsed_ok=N`,
`lockfiles_skipped_corrupt=N`,
`lockfiles_skipped_non_pants=N` (NEW vs m223 — tracks the
FR-011 discriminator), `components_emitted=N`. The reader
returns early WITHOUT logging when zero lockfiles are found
(SC-003 byte-identity guarantee).

**Coexistence with the standalone Maven reader (`pom.xml`,
Gradle, `~/.m2/`, embedded `META-INF/maven/`):** both readers run
in every scan. Their outputs merge through the m191 reconciler.
No behavior changes for repos without any coursier lockfiles.

**Follow-ups deferred:**
- Standalone coursier lockfile support (no Pants header) —
  usage segment is small; revisit when operator demand emerges.
- Coursier v2 schema — handled reactively via the `version`
  guard.
- `BUILD`-file walker for `jvm_artifact(...)`, `scala_source(...)`
  — design-tier signal that duplicates the lockfile.

See [`specs/224-pants-coursier-jvm/quickstart.md`](../specs/224-pants-coursier-jvm/quickstart.md)
for a walkthrough.

---

> **Design-tier fallback**: No — always source. Coursier lockfile pins exact versions + sha256 for every entry. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## pants (shell)

**Modules:** `waybill-cli/src/scan_fs/package_db/pants_shell/`
(`mod.rs`, `build_dsl.rs`, `target_resolver.rs`, `config.rs`,
`component_emit.rs`).

**Detection:** discovers every `BUILD` file under the scan root
via `safe_walk` (respects `--exclude-path` + symlink-cycle guard),
extracts `shell_source` / `shell_sources` / `shunit2_test` /
`shunit2_tests` target declarations via a regex-scoped Pants-DSL
parser (Constitution Principle I — no embedded Python
interpreter), resolves each target's `source=` / `sources=[...]`
expression against the BUILD file's own directory, and emits ONE
`pkg:generic/<basename>@<sha256[:12]>` file-tier component per
resolved `.sh` file. Also parses `pants.toml` at the scan root
for `[shellcheck]` / `[shfmt]` / `[shunit2]` `version = "..."`
pins and emits each as a design-tier `pkg:generic/<tool>@<version>`
component.

**Recognized target types:**

- `shell_source(name="X", source="a.sh")` — single file, runtime
- `shell_sources(name="X", sources=["*.sh", ...])` — glob, runtime
- `shunit2_test(name="X", source="a_test.sh")` — single file, dev
- `shunit2_tests(name="X", sources=["*_test.sh"])` — glob, dev

Plugin-registered custom target types are silently ignored.
`shell_command` targets (Pants's arbitrary-command wrapper) are
NOT ingested per FR-012 — they describe actions, not artifacts.

**PURL construction:**
- Scripts: `pkg:generic/<url-encoded-basename>@<sha256[:12]>` —
  content-addressed, readable in a component listing. The full
  sha256 lives in the standard `hashes[]` slot.
- Tool pins: `pkg:generic/<tool>@<version>` — version preserved
  verbatim (leading `v` prefix kept when present).

**Annotations:**
- `waybill:pants-target` (NEW catalog row C145 with this milestone)
  — the Pants target address(es) that own the component. Multiple
  owners (same file resolved by two targets) merge into ONE
  annotation, lex-sorted comma-separated.
- `waybill:source-files` (m080 row) — scan-root-relative file path.
- Tool components: `waybill:source-file = pants.toml` (m080 row) +
  `waybill:sbom-tier = design`.

**Lifecycle-scope classification:** `shunit2_test` /
`shunit2_tests`-owned components tag `Development` (dev-tool
allowlist convention matches m179's `LifecycleScope::Development`
emission for `waybill:lifecycle-scope=development` property).
`shell_source` / `shell_sources`-owned components leave the
scope absent (Runtime = default, elided per m179 convention).
When a file is owned by BOTH a runtime and a dev target, the
merged component tags as Development (dev scope wins — the safer
default for compliance triage).

**FR-010 log:** one INFO line at scan end reports
`build_files_discovered=N`, `build_files_parsed_ok=N`,
`build_files_skipped_corrupt=N`, `shell_targets_found=N`,
`script_components_emitted=N`, `tool_components_emitted=N`. The
reader returns early WITHOUT logging when zero BUILD files are
discovered AND no `pants.toml` is present at the scan root
(SC-003 byte-identity guarantee).

**Coexistence with existing readers:**
- The m133 file-tier walker (orphan-file discovery) sees
  pants-shell-emitted `source_path` values in its dedupe index,
  so no double-emission occurs for scripts already claimed by a
  BUILD file target.
- The `pants` (m223, Python) and `pants_jvm` (m224, JVM) readers
  are independent modules. All three may activate on the same
  scan (repos with Python + JVM + shell all use `pants.toml` for
  different subsystem sections).

**Follow-ups deferred:**
- `shell_command` targets — architectural addition (model
  actions as SBOM subjects).
- Plugin-registered custom shell target types — currently ignored.
- Nested `pants.toml` files under scan root — only root-level
  consulted.
- Pants's embedded shunit2 bundle — only operator-pinned
  `[shunit2] version = "..."` triggers emission.

See [`specs/225-pants-shell-reader/quickstart.md`](../specs/225-pants-shell-reader/quickstart.md)
for a walkthrough.

---

> **Design-tier fallback**: Yes — synthetic. `pants.toml` `[shellcheck]` / `[shfmt]` / `[shunit2]` tool pins emit ONE synthetic `pkg:generic/<tool>@<version>` per pinned tool at design-tier. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## pants (Go)

**Modules:** `waybill-cli/src/scan_fs/package_db/pants_go/`
(`mod.rs`, `build_dsl.rs`, `ownership_index.rs`, `config.rs`,
`enrichment.rs`).

**Detection:** discovers every `BUILD` file under the scan root
via `safe_walk` (respects `--exclude-path` + symlink-cycle
guard), extracts `go_binary` / `go_package` /
`go_third_party_package` / `go_mod` target declarations via a
regex-scoped Pants-DSL parser (reuses the m225 pants_shell
extractor pattern — no embedded Python interpreter per
Constitution Principle I). Also parses `pants.toml`
`[golang] expected_version` when present.

**Enrichment-only architecture** (FR-012 / Principle IX — zero
fabrication): this reader does NOT emit any `pkg:golang/*`
components of its own. It runs a **post-`read_all` enrichment
pass** (at `scan_fs/mod.rs:1001`, after m191 reconciler + before
m148 canonicalization) that iterates every `pkg:golang/*`
component the existing Go reader emitted from `go.sum` entries
and injects a `waybill:pants-target` annotation naming the
Pants target(s) that own the component.

**Recognized target types:**

- `go_mod(name="mod")` — implicit owner of every go.sum entry
  in the BUILD file's directory (deepest-prefix wins for
  multi-`go_mod` Go workspaces)
- `go_third_party_package(name="X", import_path="example.com/foo")`
  — explicit owner of one third-party module
- `go_binary(name="X", main="./cmd/foo")` — owns the
  main-module component when `source_path.parent()` matches
  `<build_dir>/<normalized_main>`
- `go_package(name="X")` — owns the main-module component when
  its `source_path.parent()` starts_with the BUILD file's
  directory

Plugin-registered custom target types are silently ignored.
`go_source` / `go_test` file-level targets are deferred.

**Toolchain-pin emission:** when `pants.toml` `[golang]
expected_version` is set, waybill emits ONE design-tier
`pkg:generic/go@<version>` component with
`waybill:source-file=pants.toml`. Version is preserved
verbatim (waybill does not normalize patch-vs-major.minor).

**Annotations:**
- `waybill:pants-target` (broadened C145 with this milestone —
  same catalog row as m225's shell case, doc-only description
  update; no new row) — comma-sep, lex-sorted list of owning
  Pants target addresses. Multi-owner merge when the same
  component is owned by multiple targets.

**Zero-fabrication invariant:** a `go_third_party_package(import_path=X)`
declaration whose `X` has no matching go.sum entry produces
NO synthetic component. Waybill emits only an INFO
diagnostic naming the orphan import path.

**FR-010 log:** one INFO line at scan end reports
`build_files_discovered=N`, `build_files_parsed_ok=N`,
`build_files_skipped_corrupt=N`, `go_targets_found=N`,
`components_annotated=N`, `toolchain_component_emitted=<0|1>`.
Silent (no log) when no BUILD files AND no `pants.toml` at
scan root (byte-identity guarantee).

**Coexistence with existing readers:**
- The existing Go reader (m053 + m055 + m160 + m161) is
  unchanged. Every `pkg:golang/*` component it emits from
  `go.sum` still emits; pants_go only ADDS annotations to
  existing components.
- m191 reconciler is unaffected — enrichment runs AFTER it on
  the reconciled component set.
- m148 canonicalization is unaffected — it operates on
  `evidence.source_file_paths`, not `extra_annotations`.
- The `pants` (m223 Python), `pants_jvm` (m224 JVM), and
  `pants_shell` (m225 shell) readers are unchanged. All four
  Pants-family readers may activate independently.

**Follow-ups deferred:**
- `go_source` / `go_test` file-level targets
- `min_dot_version` from `pants.toml` `[golang]`
- Plugin-registered custom Go target types
- Nested `pants.toml` files under scan root

See [`specs/226-pants-go-reader/quickstart.md`](../specs/226-pants-go-reader/quickstart.md)
for a walkthrough.

---

> **Design-tier fallback**: Yes — synthetic. `pants.toml` `[golang] expected_version` emits ONE synthetic `pkg:generic/go@<version>` toolchain-pin component at design-tier. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## rpm

**Modules:** `waybill-cli/src/scan_fs/package_db/rpm.rs`,
`waybill-cli/src/scan_fs/package_db/rpmdb_sqlite/`

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
`waybill:evidence-kind = rpmdb-sqlite`.

**Dep graph:** full tree from rpmdb `REQUIRES` tags.

**Hashes:** **none.** rpmdb doesn't record per-package content hashes
Waybill can use. This is why rpm scans score 6.1/10 on sbomqs (Integrity
0/10) — the ecosystem itself doesn't provide the data.

**Enrichment:**
- deps.dev: skipped (not in deps.dev's supported ecosystems).
- ClearlyDefined: skipped (CD's rpm coverage is thin).

**Known limitations:**
- **Berkeley DB rpmdb** (`/var/lib/rpm/Packages`, pre-RHEL 8) is
  **detected but not parsed.** Diagnostic logged, zero rpm components
  emitted. The `--include-legacy-rpmdb` flag (or
  `WAYBILL_INCLUDE_LEGACY_RPMDB=1`) threads through to
  `rpmdb_bdb::read`, which is a stub pending the concrete Hash/BTree
  page parser (milestone 004 US4 tasks T061–T065). Until those land,
  flipping the flag changes nothing about scan output.
- **rpmdb.sqlite size cap** is 200 MB (defense-in-depth; real rpmdbs are
  ~5 MB).
- **Pure-Rust SQLite reader** handles leaf-table + interior-table pages
  only. Overflow pages are refused. RHEL rpmdbs don't use overflow pages
  in practice.

---

> **Design-tier fallback**: No — always source. Installed-DB scan; version + release always known. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## yocto

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

**Module:** `waybill-cli/src/scan_fs/package_db/opkg.rs`
+ `waybill-cli/src/scan_fs/package_db/yocto/{context,manifest,recipe}.rs`

Yocto / OpenEmbedded coverage (milestone 107). Three complementary
readers cover the embedded-Linux scan shapes that Waybill previously
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
  is actively present) records a `waybill:scan-ambiguity` annotation
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
appears in `waybill:also-detected-via`).

**Lifecycle scope:** runtime by default. Per-line FR-006 override
applies the same nativesdk/host-arch checks as the opkg reader.

**Annotation:** `waybill:image-name = <manifest-filename-stem>` so
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
  `version: "unknown"` + `waybill:version-status: "missing"`
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

> **Design-tier fallback**: Yes — recipe scope is inherently design-tier (`.bb` recipes have no "resolved" state). Yocto's opkg installed-DB reader emits source-tier separately when a built image is available. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## kotlin

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

Waybill's Kotlin DSL Gradle reader (milestone 122 US2) regex-extracts
dependency declarations from `build.gradle.kts` files (the Android
Studio / IntelliJ default since 2023) and resolves `libs.<alias>`
references against the workspace's `gradle/libs.versions.toml` version
catalog. PURLs emit as `pkg:maven/<group>/<name>@<version>` per the
existing milestone-106 `pkg:maven/` lane so downstream deps.dev / OSV
enrichment applies without changes.

This reader complements (not replaces) the existing milestone-106
`gradle.lockfile` reader: `gradle.lockfile`-locked components emit at
source-tier (`waybill:sbom-tier = "source"`); `build.gradle.kts`-only-
discovered components emit at design-tier (`waybill:sbom-tier =
"design"`) and are gated by the existing `--include-declared-deps`
flag (auto-on for `--path` scans; opt-in for `--image` scans). When
both readers find the same canonical PURL the milestone-105 dedup
pipeline collapses them with the lockfile-discovered tier winning.

### What gets parsed

Three regex shapes cover the dominant `build.gradle.kts` dep
declaration syntax:

1. **String-literal GAV**: `implementation("com.squareup.okhttp3:okhttp:4.12.0")`
   → `pkg:maven/com.squareup.okhttp3/okhttp@4.12.0`.
2. **Catalog alias**: `implementation(libs.okhttp)` →
   resolved against `gradle/libs.versions.toml` →
   `pkg:maven/com.squareup.okhttp3/okhttp@<version-from-catalog>`.
3. **Named-args GAV**: `implementation(group = "g", name = "n", version = "v")` →
   `pkg:maven/g/n@v`.

What's NOT matched (documented "common surface syntax only" contract):
deps declared via meta-programming (`deps.forEach { implementation(it) }`),
deps declared via custom DSL extensions (`coreDeps()` shorthands),
deps declared via Kotlin reflection. Operators using exotic
declarations get a `tracing::debug!` and a degraded SBOM — adopting a
full Kotlin parser would require either a tree-sitter Kotlin grammar
(C code, Principle I violation) or shelling out to `kotlinc` (JVM
dependency at scan time, Strict Boundary 3 violation), neither of
which Waybill takes on.

### Dep-configuration → lifecycle-scope mapping

| Configuration | `waybill:lifecycle-scope` (CDX `scope`) |
|---|---|
| `implementation`, `api`, `runtimeOnly`, `compileOnly` | (omitted — runtime default) |
| `testImplementation`, `androidTestImplementation`, `testRuntimeOnly`, `testCompileOnly` | `test` (`excluded`) |
| `debugImplementation`, `releaseImplementation` | `development` (`excluded`) |
| `kapt`, `annotationProcessor`, `ksp` | `build` (`excluded`) |

Non-listed configurations capture as runtime (no annotation) with a
`tracing::debug!` line announcing the unrecognized config.

### Multi-module workspaces

When Waybill finds a `settings.gradle.kts` declaring
`rootProject.name = "..."` + `include(":mod1", ":mod2")`, it
synthesizes a workspace-root component:

- PURL: `pkg:generic/<rootProject.name>@0.0.0` (falling back to the
  workspace directory name when `rootProject.name` is absent).
- `waybill:component-role = "workspace-root"`.
- `waybill:source-files = "<path>/settings.gradle.kts"`.
- `waybill:sbom-tier = "source"`.

Only the OUTERMOST `settings.gradle.kts` per scan tree synthesizes a
workspace-root — nested `settings.gradle.kts` files are walked for
sibling `build.gradle.kts` discovery only and DO NOT emit additional
workspace-root components (per the "Two-deep nested Gradle workspaces"
edge case from spec).

### Kotlin Multiplatform (KMP) source-set provenance

When a `build.gradle.kts` declares deps inside a
`kotlin { sourceSets { <name> { dependencies { ... } } } }` block,
Waybill stamps `waybill:kmp-source-set` on the emitted component as a
JSON-encoded array of every source-set name that declared the dep
(lex-sorted, deduped). Multiple source-sets declaring the same
canonical PURL accumulate into one merged array — consumers reading
the SBOM call `JSON.parse(prop.value).includes("commonMain")` etc. to
filter to a specific KMP target.

Per the FR-006 timing contract, the kotlin_dsl reader may emit one
`PackageDbEntry` per `(dep × source-set)` tuple pre-dedup; each
duplicate carries the SAME merged source-set array; the milestone-105
dedup pipeline collapses them into ONE canonical component
post-emission while preserving the merged annotation.

### Known limitations (Kotlin DSL v0.1)

- Meta-programmed deps (`deps.forEach { ... }`, custom DSL extensions,
  reflection-driven declarations) are NOT extracted. Operators using
  these patterns get a degraded SBOM.
- `apply(from = "common.gradle.kts")` indirection is NOT followed in
  v0.1 — only the immediate `build.gradle.kts` is parsed. Convention-
  plugin chains (rare) are deferred to a future phase.
- The catalog reader looks at `gradle/libs.versions.toml` only — other
  catalog paths (multi-catalog setups via `dependencyResolutionManagement`)
  are deferred.
- Cargo workspace + Gradle workspace co-located in one scan tree (rare
  cross-ecosystem polyglot) emit one workspace-root per ecosystem; the
  milestone-105 dedup pipeline handles cross-reader collapse via
  canonical PURL match.

### Per-component annotations (Kotlin)

| Annotation | When emitted |
|---|---|
| `waybill:source-files` | always — path to the `build.gradle.kts` |
| `waybill:sbom-tier = "design"` | every kotlin_dsl-discovered dep |
| `waybill:lifecycle-scope` | per dep-config family (see table above); absent for runtime |
| `waybill:kmp-source-set` | KMP source-set deps only; JSON-encoded array |

Workspace-root entries additionally carry `waybill:component-role =
"workspace-root"`.

---

> **Design-tier fallback**: Yes — **opt-in only**. `--include-declared-deps` flag enables Kotlin DSL declaration emission at design-tier. Default: no emission. Rationale: Gradle KTS DSL cannot be fully resolved without a Gradle daemon. See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

---

## swift

**Path exclusion**: see [Directory exclusion (--exclude-path)](#directory-exclusion---exclude-path).

Waybill's Swift Package Manager (SwiftPM) reader (milestone 122 US1)
parses `Package.resolved` lockfiles at any directory in the scan tree
and emits one component per `pins[]` entry. Schema versions 1, 2, and 3
are dispatched on the lockfile's top-level `version` integer. The
sibling `Package.swift` manifest is detected — it signals that a
directory is a SwiftPM project root — but its content is NEVER parsed
in v0.1 (per spec FR-002 + clarification Q3 of milestone 122).
`Package.swift` is executable Swift code; safe parsing requires either
a Swift parser dependency (Constitution Principle I violation) or
shelling out to the host `swift` toolchain (Strict Boundary 3 +
scan-time-dependency violation). Local-path declarations
(`.package(path: ...)`), target declarations, conditional-platform
deps, and workspace-member emission from `Package.swift` content are
all deferred to a future phase.

PURLs emit as `pkg:swift/<host>/<namespace>/<name>@<version>` per the
[purl-spec swift type](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst#swift):

- HTTPS-form locations with the `.git` suffix have it stripped:
  `https://github.com/apple/swift-log.git` → `pkg:swift/github.com/apple/swift-log@<ver>`.
- SSH-form locations have the user segment dropped and the `.git`
  suffix stripped: `git@gitlab.acme.com:internal/lib.git` →
  `pkg:swift/gitlab.acme.com/internal/lib@<ver>`.
- Deep-namespace URLs (GitLab subgroups: more than one path segment
  between host and the package name) are NOT supported in v0.1 — the
  purl-spec swift type currently allows single-segment namespaces
  only. Affected entries emit a `tracing::warn!` and drop. Operators
  using subgroups should declare their packages via SwiftPM workspace
  layouts instead until the spec extension lands.

Commit-pinned mode (the SwiftPM "branch-tracking" lockfile shape — a
`state.revision` without a `state.version`) emits the component with
the FULL 40-char revision SHA as the PURL version segment AND a
`waybill:source-type = "git"` property AND a redundant
`waybill:source-revision` property for grep convenience. This matches
the existing Go reader's `pkg:golang/...@<sha>` convention and lets
deps.dev / OSV consumers exact-match the commit.

Per-component:

- `waybill:source-files = "<path>/Package.resolved"` — the path of the
  lockfile that emitted this component.
- `waybill:source-type = "git"` — set only on commit-pinned entries
  (no `state.version`).
- `waybill:source-revision = "<40-char sha>"` — set only on
  commit-pinned entries.
- `waybill:sbom-tier = "source"` — every Swift lockfile-discovered
  component.

Parse failures (malformed JSON, missing `pins[]`, unknown schema
version) emit a `tracing::warn!` naming the file path + the specific
failure; Waybill continues the walk on sibling files (per spec
FR-009). Per-entry failures (invalid revision, unparseable URL,
deep-namespace) emit a `tracing::warn!` naming the affected pin's
identity; other entries in the same file still emit.

### Known limitations (Swift v0.1)

- `Package.swift` content is not parsed (see above).
- Deep-namespace URLs (GitLab subgroups) emit a warn-and-drop.
- CocoaPods (`Podfile.lock`) + Carthage (`Cartfile.resolved`) are not
  supported. SwiftPM is the modern Apple-blessed dependency manager;
  CocoaPods adoption is declining. Adding either is a future
  milestone if operator demand surfaces.

---

> **Design-tier fallback**: No — always source. `Package.resolved` is authoritative. **Caveat**: `Package.swift`-only projects (no `Package.resolved`) emit NO components at all — see [Known limitations (Swift v0.1)](#known-limitations-swift-v01). See [SBOM tiers → §2 matrix](#2-per-ecosystem-design-tier-fallback-matrix).

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

Waybill's binary scanner identifies statically-linked C libraries from
their exported-symbol fingerprints (ELF `.dynsym` + Mach-O `LC_SYMTAB`
externals — PE deferred). The bundled fallback corpus ships 7 libraries
(openssl, zlib, libcurl, sqlite, pcre, pcre2, gnutls) and stays at
that size as a stability floor; the source-of-truth corpus lives in
the sibling repo
[`kusari-sandbox/waybill-fingerprints`](https://github.com/kusari-sandbox/waybill-fingerprints)
and grows independently of Waybill releases.

Operators opt into the external corpus per scan via
`--fingerprints-corpus` (or `WAYBILL_FINGERPRINTS_CORPUS=1`):

```bash
waybill sbom scan --image ghcr.io/myorg/my-app:v1 --fingerprints-corpus
```

The cache-first / fetch-on-miss flow, the
`waybill:fingerprint-corpus-sha` provenance annotation, the
`waybill fingerprints fetch/cache-clear/list` subcommands, and the
4-step consumer lookup recipe are documented end-to-end in:

- [`docs/reference/identifiers.md` §11](reference/identifiers.md#section-11--milestone-108-external-corpus-provenance-mikebomfingerprint-corpus-sha)
  — annotation value space + per-format carriers + lookup recipe.
- [`specs/108-fingerprint-corpus/quickstart.md`](../specs/108-fingerprint-corpus/quickstart.md)
  — operator + air-gapped + hermetic-build scenarios.
- [`kusari-sandbox/waybill-cmake-demo`](https://github.com/kusari-sandbox/waybill-cmake-demo)
  — runnable cmake + ninja demo that exercises both the source-tree
  reader AND the fingerprint matcher end-to-end.

### Milestone 109 — cross-tier PURL attribution for cmake projects

When Waybill scans a cmake project root with `--fingerprints-corpus`
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
   sources' evidence (`waybill:source-mechanism = cmake-fetchcontent-git`
   AND `waybill:fingerprint-corpus-sha = <sha>` AND
   `waybill:fingerprint-symbols-matched = "10/10"`).

Scope: `FetchContent_Declare` only (git + url forms). `ExternalProject_Add`,
Bazel, Meson, and hand-written Makefiles are out of scope this
milestone but the architecture accommodates them as follow-on
observers feeding the same registry. Operators scanning a SINGLE
binary (no source tree) or running without `--fingerprints-corpus`
see milestone-108 behavior unchanged.

Full design + contracts in [`specs/109-binary-source-purl-binding/`](../specs/109-binary-source-purl-binding/).
