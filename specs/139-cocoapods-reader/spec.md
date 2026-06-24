# Feature Specification: CocoaPods ecosystem reader

**Feature Branch**: `139-cocoapods-reader`
**Created**: 2026-06-23
**Status**: Draft
**Input**: User description: "424"

## Background

CocoaPods remains the dominant iOS dependency manager. Despite Apple's push toward SwiftPM (which mikebom supports via milestone 122), CocoaPods still ships in the majority of production iOS apps — particularly those with C/Objective-C bridging requirements, native module dependencies (Firebase, GoogleMaps, ReactNative-managed pods, FBSDK), or pre-2018 codebases that haven't migrated. Many iOS projects use BOTH CocoaPods and SwiftPM simultaneously: SwiftPM for pure-Swift libraries, CocoaPods for everything with a C dependency.

mikebom currently emits **zero** CocoaPods-managed components when scanning an iOS source tree. Every pod pulled from the CocoaPods central spec repo (cdn.cocoapods.org) — including the typically-large Firebase/GoogleSignIn/AFNetworking transitive graphs — is invisible to the scan. This is the iOS-side mirror of the Dart/Flutter gap closed by milestone 137.

The CocoaPods ecosystem has four discrimination surfaces a reader must handle:

- **Trunk-hosted pods** (the common case): from the central spec repo `cdn.cocoapods.org`. PURL: `pkg:cocoapods/<pod>@<version>`.
- **Subspec deps**: a pod's `subspecs` (`Firebase/Core`, `Firebase/Auth`, `Firebase/Messaging`) — Podfile.lock records them as `PodName/Subspec` in the `PODS:` section. These resolve to the parent pod's version but represent distinct dep entries.
- **Git/HTTP source pods**: directly-pinned git URLs (`pod 'MyFork', :git => 'https://github.com/foo/bar.git', :commit => 'abc...'`). Identity is the resolved git SHA. Less common than Packagist-style trunk pods but real in monorepo and fork scenarios.
- **Local path pods**: development pods (`pod 'LocalLib', :path => '../my-lib'`) typically used for monorepos and in-flight library development. Identity is the path; no central-repo version.

Additionally CocoaPods' lockfile carries a **SPEC CHECKSUMS** section — SHA-1 hashes of the resolved podspec for every entry. Surfacing these as `mikebom:podspec-checksum` evidence (or via the standards-native `hashes[]` array) is the symmetric pattern to milestone 138's SHA-1 emission via `dist.shasum`.

This feature closes the iOS gap so an operator scanning any iOS project gets a complete SBOM with every CocoaPods-managed dep represented, subspecs distinguished from parent pods, and the right provenance distinction surfaced.

## Clarifications

### Session 2026-06-23

- Q: When `Podfile.lock` is present but `Podfile` is absent (lockfile-only commit OR container/source-archive scan), how should the main-module name be derived given that the `target '<name>' do` block is the only direct source per FR-012? → A: Fall back to the parent-directory basename (`pkg:cocoapods/<dir-basename>@0.0.0-unknown`). Preserves dep-edge attribution from a project anchor without inventing a generic placeholder; mirrors milestone-138's Q3 posture of keeping deps emitting while adapting the main-module identity source. The reader prefers Podfile-derived target name when available; falls back to dir-basename only when Podfile is absent OR Podfile lacks any parseable target block.
- Q: For git-source pods, should the reader parse BOTH `EXTERNAL SOURCES:` (operator-declared url + ref) AND `CHECKOUT OPTIONS:` (the resolved 40-char SHA written by `pod install`) to populate `mikebom:vcs-ref` evidence? → A: Parse both sections. `CHECKOUT OPTIONS:` IS the authoritative resolved-SHA source for git-source pods; without it the common `:git => + :branch =>` declaration pattern would lose identity precision. `EXTERNAL SOURCES:` provides the git URL + the operator-declared ref (`:branch`/`:tag`/`:commit`); `CHECKOUT OPTIONS:` provides the resolved 40-char commit SHA. Both flow into the emitted `PackageDbEntry`: the URL goes into the PURL's `?vcs_url=git+<url>` qualifier; the resolved SHA goes into `mikebom:vcs-ref`. Matches the milestone-138 Composer precedent (`source.reference` parsing for the resolved SHA).
- Q: When only `Pods/Manifest.lock` is present (no sibling `Podfile.lock`), what `mikebom:sbom-tier` should the derived components carry? → A: `deployed`-tier. Manifest.lock IS the post-`pod install` verification copy written by the installer (it lives under `Pods/`); when it's the only lockfile available the operator is scanning a deployed artifact (built container layer, pre-built source archive), not a development source tree. Mirrors milestone-138's `vendor/composer/installed.json` → `deployed` mapping. Components also carry `mikebom:evidence-kind = "cocoapods-manifest-lock"` per FR-011 so consumers can distinguish the install-time evidence path from the standard `Podfile.lock` (`source`-tier, `evidence-kind = "cocoapods-podfile-lock"`) path.

#### Phase 0 research corrections (post-clarification)

Plan-phase research against the [purl-spec `cocoapods-definition.md`](https://github.com/package-url/purl-spec/blob/main/types-doc/cocoapods-definition.md) and the canonical [cocoapods-core `lockfile.rb`](https://github.com/CocoaPods/Core/blob/master/lib/cocoapods-core/lockfile.rb) surfaced three corrections to initial spec guesses. These are CORRECTIONS to align with the authority, not scope changes:

- **Subspec encoding uses the PURL `#subpath` (NOT a `?subspec=` qualifier)**. Per purl-spec: *"The purl subpath is used to represent a pods subspec (if present)."* Canonical examples: `pkg:cocoapods/ShareKit@2.0#Twitter`, `pkg:cocoapods/GoogleUtilities@7.5.2#NSData+zlib`. So `Firebase/Core 10.20.0` emits as `pkg:cocoapods/Firebase@10.20.0#Core`, NOT `?subspec=Core`. Multi-level subspecs (`Firebase/Database/Realtime`) use raw `/` between subpath segments per the purl-spec base rules: `pkg:cocoapods/Firebase@10.20.0#Database/Realtime` (no URL-encoding of the segment separator). This OVERRIDES the spec's earlier Assumption that said "via `?subspec=`".
- **Default repository URL is `https://cdn.cocoapods.org/`** (NOT `trunk.cocoapods.org` — trunk is the publish-API host; CDN is the spec-resolution host that purl-spec records as canonical).
- **`SPEC CHECKSUMS:` is keyed by ROOT pod only**, not per-subspec. When emitting a subspec component (e.g., `Firebase/Core`), the SHA-1 lookup MUST use the root pod name (`Firebase`); all subspecs of the same root share the same checksum (it's a SHA-1 of the parent podspec file). FR-008 reflects this below.
- **syft/trivy divergence note**: both syft and trivy fold subspec into the name (`pkg:cocoapods/Firebase/Database@1.0.0`) instead of using purl-spec subpath. Per Principle V (standards-native first), mikebom emits the spec-conformant subpath form. Emission of a syft/trivy-compatibility `mikebom:also-known-as` annotation is deferred to v1.1.

FR-003 + FR-008 + US2 acceptance scenario 1 + SC-009 + Assumptions are updated below to reflect these corrections.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator scans an iOS app with CocoaPods deps (Priority: P1) 🎯 MVP

An iOS developer runs `mikebom sbom scan --path .` on their iOS app source tree containing a `Podfile.lock` (the standard committed lockfile). They receive an SBOM containing one component per pod pinned in the `PODS:` section. Each component carries a `pkg:cocoapods/<pod>@<version>` PURL identity. When the project has subspecs (Firebase/Core + Firebase/Auth), both surface as distinct components with subspec-qualified PURLs.

**Why this priority**: The headline use case. Every iOS app with CocoaPods commits a `Podfile.lock`; without this, the entire feature has no operator value.

**Independent Test** (SC-001): Synthetic fixture with `Podfile` declaring 3 direct deps (`AFNetworking`, `SDWebImage`, `Firebase/Core`) + `Podfile.lock` pinning those plus their transitive deps (5 total — one of which is a subspec). Run `mikebom sbom scan --path <tmp>`. Assert exactly 5 `pkg:cocoapods/*` components emit with correct names + versions, and the subspec entry uses the subspec-qualified PURL form.

**Acceptance Scenarios**:

1. **Given** an iOS app with `Podfile.lock` pinning `AFNetworking 4.0.1`, `SDWebImage 5.18.10`, `Firebase/Core 10.20.0`, **When** the operator runs `mikebom sbom scan --path <project>`, **Then** the emitted SBOM contains components for each pinned pod with PURL `pkg:cocoapods/<pod>@<version>`.
2. **Given** the same project, **When** the operator inspects the emitted SBOM, **Then** transitive pods pinned in `Podfile.lock` (e.g., `FirebaseCore`, `FirebaseInstallations`, `GoogleUtilities/Environment`) also appear as components — the lockfile is the authoritative dep set, not just `DEPENDENCIES:`.
3. **Given** a source tree WITHOUT `Podfile.lock` or `Podfile`, **When** the operator scans, **Then** no CocoaPods components or annotations appear AND no warning fires (clean no-op).
4. **Given** a project containing `Pods/Manifest.lock` (the post-`pod install` verification manifest), **When** the operator scans alongside `Podfile.lock`, **Then** Manifest.lock content is NOT separately emitted — it's a verification artifact, semantically identical to `Podfile.lock` at any successful install time.

---

### User Story 2 — Operator distinguishes trunk vs git / path / subspec pods (Priority: P2)

The operator's iOS app's `Podfile.lock` mixes trunk-hosted pods (the default), a `:git` dep pinning a fork, a `:path` development pod for a shared monorepo library, and the standard subspec graph (Firebase/Core + Firebase/Auth + Firebase/Messaging). The SBOM must distinguish these sources so downstream supply-chain risk tooling can correctly classify each (subspecs have semantically different risk than parent pods; git deps + path deps have meaningfully different provenance than trunk deps).

**Why this priority**: Important for supply-chain risk assessment but the headline value (US1) ships independently.

**Independent Test** (SC-002): Synthetic fixture with one each of trunk / git / path dep + one subspec graph (Firebase/Core + Firebase/Auth) in `Podfile.lock`. Scan. Assert correct PURL shape and `mikebom:source-type` annotation per FR-003.

**Acceptance Scenarios**:

1. **Given** a `Podfile.lock` entry in the `PODS:` section listed as `Firebase/Core (10.20.0)`, **When** the operator scans, **Then** the emitted component carries PURL `pkg:cocoapods/Firebase@10.20.0#Core` with `mikebom:source-type = "cocoapods-trunk"` evidence; downstream filtering on the subpath surfaces subspec-only views (the `#subpath` form is the purl-spec-canonical encoding per Phase 0 research).
2. **Given** a `Podfile.lock` carrying an `EXTERNAL SOURCES:` block with a `:git => 'https://github.com/foo/bar.git', :commit => 'abc...'` entry, **When** the operator scans, **Then** the emitted PURL embeds the resolved commit per the purl-spec git-source convention (`?vcs_url=git+https://github.com/foo/bar.git`) AND carries `mikebom:source-type = "cocoapods-git"` evidence; the resolved SHA appears as `mikebom:vcs-ref` annotation.
3. **Given** a `Podfile.lock` with a `:path => '../shared-lib'` development pod, **When** the operator scans, **Then** that pod emits with `mikebom:source-type = "cocoapods-path"` evidence and a `pkg:generic/<pod>@<version>` placeholder PURL (path-deps have no trunk-addressable identity).

---

### User Story 3 — Operator scans an iOS project WITHOUT a committed lockfile (Priority: P3)

Some iOS library projects (especially those distributed as both pods and SwiftPM packages) deliberately do NOT commit `Podfile.lock` — only `Podfile` with version constraints. Scanning such a project should produce SOME inventory rather than empty output, marked as `design`-tier to reflect the lower fidelity.

**Why this priority**: Important for library-publisher workflows but not blocking. Most iOS projects DO commit `Podfile.lock`; the design-tier path is a smaller user base than US1.

**Independent Test** (SC-003): Synthetic fixture with `Podfile` declaring 2 direct deps but NO `Podfile.lock`. Scan. Assert 2 components emit with `mikebom:sbom-tier = "design"` annotation, each carrying the declared constraint string as `mikebom:requirement-range` evidence.

**Acceptance Scenarios**:

1. **Given** an iOS library project with `Podfile` only (no `Podfile.lock`), **When** the operator scans, **Then** components emit for declared direct pods with `mikebom:sbom-tier = "design"` and the original constraint string preserved.
2. **Given** the same project, **When** the operator inspects the emitted SBOM, **Then** NO transitive pods appear (lockfile is required for transitive resolution; design-tier captures only what's explicitly declared in `Podfile`).
3. **Given** a `Podfile` declaring `pod 'AFNetworking'` with no version constraint, **When** the operator scans in design-tier mode, **Then** the emitted component uses `pkg:cocoapods/AFNetworking@unspecified` as the PURL placeholder (PURL spec requires a version segment; design-tier with no constraint can't synthesize one).

---

### Edge Cases

- **Mixed trunk/git/path/subspec in one project**: a Firebase-using iOS app commonly has all four surfaces in one `Podfile.lock`. Each MUST surface with the correct `source-type` evidence; subspecs MUST NOT silently collapse into their parent pod.
- **Private CocoaPods spec repo (`source 'https://github.com/acme/specs.git'` at Podfile top)**: some organizations run an internal pod mirror. The `Podfile.lock` doesn't record the source URL per-pod (unlike Packagist's `dist.url`); the `source` directive lives only in `Podfile`. v1 emits trunk PURLs for all centrally-resolved pods; private-spec-repo distinction is deferred (operator can read `Podfile`'s `source` block separately).
- **`SPEC CHECKSUMS:` section**: a Podfile.lock section that maps `<pod-name>: <sha1-hex>` for every pinned pod. v1 surfaces these as SHA-1 hashes on `PackageDbEntry.hashes` (per the milestone-138 FR-013 precedent — Composer's `dist.shasum`).
- **`PODFILE CHECKSUM:` field**: SHA-1 of the operator's `Podfile` text. Informational only; not consumed v1.
- **`COCOAPODS:` version trailer**: records the CocoaPods version that produced the lockfile. Informational only; not consumed v1.
- **Pre-CocoaPods-1.0 lockfile format**: lacks the `SPEC CHECKSUMS:` section. The PODS shape itself is stable across 1.0 → current, so v1 emits components from such lockfiles WITHOUT SHA-1 hashes (graceful degradation per Principle VIII Completeness) and logs an info-level `tracing` message noting the missing SPEC CHECKSUMS section. Pre-1.0 lockfile-specific features (different `hash` field name, missing `aliases:` block) are deferred — modern CocoaPods 1.0+ (released 2016) is universal in 2026 production iOS development and the 1.0+ schema is the only one tested.
- **Malformed `Podfile.lock`**: skip-the-file with `tracing::warn!`; when sibling `Podfile` exists, fall back to design-tier emission from the manifest per FR-005.
- **`Pods/Manifest.lock` vs `Podfile.lock` divergence**: `Manifest.lock` is the post-`pod install` verification copy; CocoaPods aborts subsequent builds if they diverge. v1 reads `Podfile.lock` exclusively and ignores `Manifest.lock` to avoid double-counting; the two are semantically identical at any successful build state. (Operators who need to verify the divergence can use `pod install` itself.)
- **Multi-target Podfiles**: a `Podfile` with multiple `target 'AppName' do ... end` blocks declaring different pod sets per target. The `Podfile.lock`'s `PODS:` section is the union of all targets' resolutions; v1 emits each pinned pod once regardless of which target depends on it. Per-target dep attribution is deferred to v1.1.
- **No `name:` analog**: unlike Composer's `composer.json::name` or Dart's `pubspec.yaml::name`, iOS projects don't carry a Pod-level project name. Per FR-012 + Q1 clarification, v1 derives the main-module name via cascade: (1) first `target '<name>' do` block in `Podfile`; (2) parent-directory basename as fallback when no Podfile exists OR no target block parseable.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect CocoaPods projects by the presence of `Podfile.lock` OR `Podfile` files anywhere under the scan root. Either triggers reader activation.
- **FR-002**: System MUST parse `Podfile.lock` (YAML format, CocoaPods 1.0+ schema) and extract from the top-level keys: `PODS:` array (each entry is a `pod-name (version)` string OR a `{pod-name (version): [transitive-deps]}` map), `DEPENDENCIES:` array (declared direct deps), `EXTERNAL SOURCES:` map (per-pod source overrides for git/path entries — declared url + ref via `:git`/`:path`/`:branch`/`:tag`/`:commit` sub-keys), `CHECKOUT OPTIONS:` map (per-pod RESOLVED git SHAs written by `pod install` — the authoritative source for git-source `mikebom:vcs-ref` evidence per Q2 clarification), `SPEC CHECKSUMS:` map (pod-name → SHA-1 hex), `PODFILE CHECKSUM:` scalar, `COCOAPODS:` version string.
- **FR-003**: System MUST emit one component per parsed `PODS:` entry with PURL according to the source discriminator (shapes per the [purl-spec `cocoapods` definition](https://github.com/package-url/purl-spec/blob/main/types-doc/cocoapods-definition.md)):
  - **trunk** (default — no matching `EXTERNAL SOURCES:` entry): `pkg:cocoapods/<pod>@<version>`. When the PODS entry is a subspec (`PodName/Subspec`), use `pkg:cocoapods/<parent-pod>@<version>#<subspec-path>` per the Phase 0 research correction — purl-spec encodes subspec via the PURL `#subpath` mechanism (NOT a `?subspec=` qualifier). Multi-level subspecs preserve `/` between subpath segments per purl-spec base rules: `Firebase/Database/Realtime 10.20.0` → `pkg:cocoapods/Firebase@10.20.0#Database/Realtime`.
  - **git** (matching `EXTERNAL SOURCES:` entry with `:git => <url>`): `pkg:cocoapods/<pod>@<version>?vcs_url=git+<git-url>` per purl-spec git-source cross-type convention. Per Q2 clarification, the URL comes from `EXTERNAL SOURCES:` and the RESOLVED git SHA comes from the lockfile's `CHECKOUT OPTIONS:` section (the authoritative resolved-ref source — operator-declared `:branch`/`:tag` doesn't pin a SHA; `pod install` resolves the branch/tag to a 40-char commit and records it in CHECKOUT OPTIONS). The resolved SHA is preserved as `mikebom:vcs-ref` evidence annotation; the operator-declared ref (`:branch`/`:tag`/`:commit` from EXTERNAL SOURCES) is preserved as `mikebom:vcs-declared-ref` annotation when distinct from the resolved SHA.
  - **path** (matching `EXTERNAL SOURCES:` entry with `:path => <path>`): `pkg:generic/<pod>@<version>` placeholder + `mikebom:source-type = "cocoapods-path"` evidence — path-deps have no trunk-addressable identity, so the `pkg:generic/` placeholder + annotation surface the discriminator while preserving a usable bom-ref for dep-graph wiring.
- **FR-004**: System MUST emit dependency edges from each project's main-module (per FR-012) to each direct dep declared in the lockfile's `DEPENDENCIES:` array. Transitive components (PODS entries not in DEPENDENCIES) surface as standalone components but their inter-package dependency edges (the per-PODS-entry transitive sub-arrays) are deferred to v1.1 — scope-aligned with the milestone-064 / 066 / 068 / 069 / 070 / 137 / 138 v1 convention.
- **FR-005**: When `Podfile.lock` is absent but `Podfile` is present, system MUST emit components for direct deps declared via `pod` lines in the `Podfile`, with `mikebom:sbom-tier = "design"` annotation and the original version constraint string as `mikebom:requirement-range` evidence (when present). When no version constraint is declared, use `unspecified` as the PURL version placeholder. No transitive deps emit in this design-tier mode.
- **FR-006**: System MUST treat a source tree containing no `Podfile.lock` AND no `Podfile` as a clean no-op — no components emitted, no warnings logged. Existing scans on non-iOS / non-CocoaPods projects MUST stay byte-identical pre/post this feature.
- **FR-007**: System MUST tolerate per-file parse errors (malformed YAML, missing required sections, encoding issues) without aborting the whole scan — log a structured warning naming the affected file path and continue. When `Podfile.lock` is malformed AND a sibling `Podfile` exists, fall back to design-tier emission from the manifest per FR-005.
- **FR-008**: System MUST preserve content-addressable hashes when present in the lockfile's `SPEC CHECKSUMS:` section: each pod's SHA-1 hex flows into `PackageDbEntry.hashes` as a `ContentHash::with_algorithm(HashAlgorithm::Sha1, hex)` entry. Per Phase 0 research correction, `SPEC CHECKSUMS:` is keyed by ROOT pod name only — when emitting a subspec component (e.g., `Firebase/Core`), the SHA-1 lookup MUST use the root pod name (`Firebase`). All subspecs of the same root share the same checksum (it's a SHA-1 of the parent podspec file, not per-subspec). CocoaPods is one of the few common-ecosystem readers (alongside Composer per milestone 138) to natively expose SHA-1 hashes for every dep.
- **FR-009**: System MUST NOT make any network calls during the scan — `Podfile.lock` is fully self-contained. Resolving a pod's trunk metadata via remote query is out of scope.
- **FR-010**: System MUST handle multi-target Podfiles (a single `Podfile.lock` resolving multiple `target 'AppName' do ... end` blocks) by emitting each pinned pod once regardless of which target depends on it. The `PODS:` section is already de-duplicated across targets by CocoaPods itself. Per-target dep attribution is deferred to v1.1.
- **FR-011**: System MUST NOT separately emit components from `Pods/Manifest.lock` when `Podfile.lock` is present in the same project (manifest.lock is CocoaPods' post-install verification copy; emitting it separately would double-count every pod). When ONLY `Manifest.lock` is present (no sibling `Podfile.lock` — typically a built container layer or pre-built source archive that shipped the installed `Pods/` directory without the developer-committed lockfile), parse it identically to `Podfile.lock` and emit components with `mikebom:evidence-kind = "cocoapods-manifest-lock"` (vs `"cocoapods-podfile-lock"` for the standard case) AND `mikebom:sbom-tier = "deployed"` (vs `"source"` for the standard case — per Q3 clarification, Manifest.lock IS the post-install evidence so its provenance is deployed-equivalent to milestone-138's `vendor/composer/installed.json`). Consumers can distinguish source-vs-deployed provenance via the standard `sbom-tier` property filter.
- **FR-012**: For each iOS project root (parent directory of `Podfile.lock` or `Podfile`), system MUST emit one **main-module component** with PURL `pkg:cocoapods/<app-name>@0.0.0-unknown`. The `<app-name>` derivation cascade per Q1 clarification: (1) the first `target '<name>' do` block in `Podfile` when a Podfile exists and a target block is parseable; (2) the project root's parent-directory basename when no Podfile exists OR Podfile lacks any parseable target block (lockfile-only commits + container/source-archive scans). The component MUST carry `mikebom:component-role = "main-module"` and `mikebom:sbom-tier = "source"` annotations. Dep edges MUST flow from this main-module to each entry in the lockfile's `DEPENDENCIES:` array (or to each `pod` line in `Podfile` in design-tier mode). When the parent-directory basename is empty or unusable (e.g., scan root is `/` itself), skip main-module emission with `tracing::warn!`; lockfile deps still emit per FR-002.

### Key Entities

- **Podfile.lock**: YAML lockfile pinning each direct + transitive pod of an iOS project to a specific version. Top-level structure: `PODS:` (array of pod-spec entries), `DEPENDENCIES:` (array of direct dep names), `EXTERNAL SOURCES:` (per-pod source overrides), `SPEC REPOS:` (which spec repo resolved each pod — typically `trunk` or a private repo URL), `SPEC CHECKSUMS:` (pod-name → SHA-1 hex), `PODFILE CHECKSUM:` (SHA-1 of Podfile text), `COCOAPODS:` (CocoaPods version).
- **Podfile**: Declared dep manifest. Ruby DSL but parsed line-by-line for `pod '<name>' [, '<version>'] [, :git => ...] [, :path => ...]` directives + `target '<name>' do` blocks (for main-module name derivation). Lower fidelity than lockfile.
- **Pods/Manifest.lock**: Post-`pod install` verification copy of `Podfile.lock`. Semantically identical at any successful install time. v1 prefers `Podfile.lock` when both present; reads `Manifest.lock` only as a deployed-tier fallback when Podfile.lock is absent (FR-011).
- **PODS entry**: A single line in the `PODS:` section. Can be a bare string (`AFNetworking (4.0.1)`) or a map (`AFNetworking (4.0.1): [AFNetworking/NSURLSession (= 4.0.1), AFNetworking/Reachability (= 4.0.1)]`). The map form carries the pod's transitive dep names + version constraints.
- **Subspec**: A nested `<pod>/<subspec>` (e.g., `Firebase/Core`, `AFNetworking/NSURLSession`). Surfaces as `pkg:cocoapods/<parent>@<version>?subspec=<subspec-name>` per purl-spec.
- **EXTERNAL SOURCES entry**: A pod whose source is overridden from the spec repo (git, path, podspec URL). Map shape: `{<pod-name>: {<source-type>: <value>, ...}}` where `<source-type>` is `:git` / `:path` / `:podspec` and value is the URL/path/SHA.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of a synthetic iOS project with `Podfile` (3 direct deps) + `Podfile.lock` (those 3 plus 2 transitives = 5 total in `PODS:`) produces a CDX SBOM whose CocoaPods component count matches the PODS count exactly (5) plus 1 main-module (= 6 total CocoaPods-derived components); direct-dep edges target real bom-refs.
- **SC-002**: A scan of a fixture mixing one trunk pod, one git-source pod, one path pod, and one subspec graph (parent + 1 subspec) produces correct PURLs for each per FR-003: trunk as `pkg:cocoapods/<pod>@<version>`, subspec as `pkg:cocoapods/<parent>@<version>#<sub>` (PURL subpath form per purl-spec), git as `pkg:cocoapods/<pod>@<version>?vcs_url=git+<url>`, path as `pkg:generic/<pod>@<version>` (placeholder). Each carries the correct `mikebom:source-type` evidence (`cocoapods-trunk` / `cocoapods-git` / `cocoapods-path`).
- **SC-003**: A scan of an iOS library project with `Podfile` only (no `Podfile.lock`) produces components for declared direct pods with `mikebom:sbom-tier = "design"` annotation and the declared constraint string preserved as `mikebom:requirement-range` evidence.
- **SC-004**: A source tree containing no iOS / CocoaPods files produces an SBOM byte-identical (modulo timestamps + serial numbers) to a pre-feature baseline scan. (No-op preservation invariant — protects every non-iOS scan.)
- **SC-005**: A scan completes successfully (exit code 0, valid SBOM) on a fixture where one `Podfile.lock` has corrupted YAML alongside three valid iOS project subdirectories. The output contains components from the three valid projects plus a warning naming the corrupted lockfile path; the corrupted project falls back to design-tier emission from its sibling `Podfile`.
- **SC-006**: An external SBOM consumer reading the emitted CDX JSON can enumerate every CocoaPods-managed pod via the standard `components[]` array filtered on `purl =~ "^pkg:cocoapods/"`. No iOS-specific consumer code is required.
- **SC-007**: A scan of a fixture with `SPEC CHECKSUMS:` populated produces CDX `hashes[]` entries with `alg = SHA-1` for every pinned pod (FR-008).
- **SC-008**: A scan of a project whose `Podfile` declares `target 'MyApp' do ... end` produces a main-module component with PURL `pkg:cocoapods/MyApp@0.0.0-unknown` carrying `mikebom:component-role = "main-module"` annotation; the SBOM's `dependencies[]` block contains an entry for the main-module's bom-ref with `dependsOn` targeting every direct-dep's bom-ref.
- **SC-009**: A scan of a fixture with subspec deps (`Firebase/Core`, `Firebase/Auth`) produces two distinct components — `pkg:cocoapods/Firebase@10.20.0#Core` and `pkg:cocoapods/Firebase@10.20.0#Auth` — NOT collapsed into a single `pkg:cocoapods/Firebase@10.20.0` entry. The PURL `#subpath` is the purl-spec-canonical encoding per Phase 0 research. Operators can filter by subpath suffix to see subspec-level inventory. Both subspecs share the same SHA-1 from `SPEC CHECKSUMS:` under key `Firebase` (root-keyed per FR-008).

## Assumptions

- **Modern CocoaPods 1.0+ format only**: pre-1.0 lockfiles (pre-2016) are out of scope. Modern CocoaPods is universal in 2026 production iOS development.
- **`Podfile.lock` is the authoritative source when present**: the reader prefers lockfile over `Podfile` (design-tier fallback). When both `Podfile.lock` and `Pods/Manifest.lock` are present, lockfile wins (Manifest.lock is the post-install verification copy).
- **No live `pod` invocation**: the reader parses on-disk metadata directly. It does NOT shell out to `pod install` or `pod outdated` — the `pod` binary (Ruby gem) isn't guaranteed to exist on the scan host (mikebom is host-portable; scanned target may be an iOS source tree on macOS scanned from a Linux container).
- **The `cocoapods` PURL type IS purl-spec-blessed**: the [purl-spec PURL-TYPES.rst](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst) defines the `cocoapods` type explicitly with `<pod>` namespace-less form. mikebom emits per the spec — no informal-type follow-up needed.
- **Existing milestone-138 language-reader pattern is the template**: the reader will share architectural shape with `composer.rs` (closest siblings — both parse lockfiles in source-tree-walked project directories with subspec / source-discriminator handling). YAML parsing via `serde_yaml` matches milestone 137's Dart precedent.
- **YAML parsing**: `serde_yaml = "0.9"` is already a workspace dep (per `dart.rs` + `npm/yarn_lock.rs` + `npm/pnpm_lock.rs`); zero new Cargo deps required.
- **Subspec PURL encoding**: per purl-spec `cocoapods-definition.md` (confirmed via Phase 0 research), subspecs encode via the PURL `#subpath` mechanism (NOT a `?subspec=` qualifier). `Firebase/Core` → `pkg:cocoapods/Firebase@10.20.0#Core`. Multi-level subspecs preserve `/` between subpath segments per purl-spec base rules: `Firebase/Database/Realtime` → `pkg:cocoapods/Firebase@10.20.0#Database/Realtime` (no URL-encoding of the segment separator; only RFC-3986-unsafe characters within segments are percent-encoded).
- **Git-source PURL convention**: per purl-spec, git-sourced pods use the `vcs_url=git+<url>` qualifier; the resolved Git SHA from `EXTERNAL SOURCES:` is preserved separately as `mikebom:vcs-ref` evidence rather than embedded in the version segment (lockfile carries the upstream-recorded `version` field even for git sources).
- **Path-deps use `pkg:generic/` placeholders**: these don't have trunk provenance so emitting under `pkg:cocoapods/` would be wrong. The placeholder + `mikebom:source-type = "cocoapods-path"` evidence properly signals their non-trunk nature.
- **`mikebom:source-type` value set**: uses the `cocoapods-` prefix (`cocoapods-trunk` / `cocoapods-git` / `cocoapods-path` / `cocoapods-main-module`) to avoid collision with cargo's existing C1 values and other readers' prefixed values. Per the established milestone-122 `kmp-` + milestone-137 `pub-` + milestone-138 `composer-` precedent.

## Out of Scope

- **Live invocation of `pod` or any Ruby/CocoaPods toolchain binary**: read-only metadata parse only.
- **Pre-CocoaPods-1.0 lockfile-specific features** (different `hash` field name, missing `aliases:` block): deferred indefinitely (exceptionally rare in 2026). Per spec Edge Cases, lockfiles lacking the `SPEC CHECKSUMS:` section still emit components — just without SHA-1 hashes — per Principle VIII graceful-degradation.
- **Private CocoaPods spec repo provenance**: when an organization uses `source 'https://github.com/acme/private-specs.git'` at the top of `Podfile`, v1 still emits trunk-shaped PURLs for centrally-resolved pods. Adding a `repository_url=` qualifier for private spec repos requires cross-referencing `Podfile` (Ruby DSL) and is deferred — practically, the `EXTERNAL SOURCES:` section already handles per-pod overrides; private-spec-repo discrimination is a future enhancement.
- **`Podfile` Ruby DSL evaluation**: line-by-line regex extraction of `pod '<name>'` and `target '<name>' do` only. We do NOT execute Ruby code (matches the gem reader's posture from milestone 069 — Ruby `.gemspec` files are parsed lenient regex-only, not evaluated).
- **Constraint-resolution simulation**: when only `Podfile` is present (no lockfile), we emit at design-tier with raw constraints preserved. We do NOT resolve constraints (`'~> 4.0'`, `'>= 5.0'`, etc.) into pinned versions.
- **`Pods/<pod-name>/` directory walking**: the installed-pod source trees under `Pods/` are post-`pod install` evidence. v1 emits at source-tier from `Podfile.lock`; deployed-tier emission from per-pod podspec parsing is deferred (parallels milestone-138's `vendor/composer/installed.json` deployed-tier path but is NOT in scope for v1).
- **Per-target dep attribution**: multi-target Podfiles emit each pinned pod once regardless of which target depends on it. Per-target attribution (which iOS target / extension / unit-test bundle depends on which pod) is deferred to v1.1.
- **License extraction**: `Podfile.lock` does NOT carry license information — license lives in each pod's own `<PodName>.podspec` shipped under `Pods/<PodName>/<PodName>.podspec`. Same shape as milestone-135 / 136 / 137 / 138 deferrals — out of scope for v1; tracked as cross-reader follow-up.
- **Transitive dep edges from individual PODS-entry sub-arrays**: v1 emits main-module → direct deps only; transitive components surface but their inter-edges are deferred to v1.1.

## Dependencies and Constraints

- **Builds on milestone 002** (initial language-reader architecture — cargo, npm, pip, etc.).
- **Builds on milestone 137** (Dart pub reader — `serde_yaml` precedent + prefixed `mikebom:source-type` convention).
- **Builds on milestone 138** (Composer reader — most recent main-module-per-manifest precedent + SHA-1 hash emission via FR-013).
- **Reuses the existing source-tree walker** (`scan_fs::walk::safe_walk`) — no new walker logic.
- **Does NOT touch existing language readers** — CocoaPods support is strictly additive.
- **Does NOT introduce new external dependencies** — `serde_yaml` is already a direct workspace dep.

## Related

- Closes: #424 (Add CocoaPods ecosystem support (Podfile.lock))
- Adjacent: #422 (Elixir/Mix reader — another lockfile language ecosystem)
- Foundational reference: milestone 122 (SwiftPM `Package.resolved` reader — sibling iOS ecosystem; CocoaPods + SwiftPM frequently coexist), milestone 137 (Dart pub — `serde_yaml` + prefixed source-type precedent), milestone 138 (PHP/Composer — main-module + SHA-1 hash precedent)
