# Changelog

All notable changes to mikebom are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
adheres to [Semantic Versioning](https://semver.org/) once it exits
`0.1.x` alpha.

## [Unreleased]

### Issue #235 — Registry credential extension for in-cluster operation

Extends the OCI registry credential resolution at `scan_fs/oci_pull/auth.rs` so mikebom can pull from private registries when running in environments without a Docker config file at the conventional `~/.docker/config.json` path — e.g. inside a Kubernetes CronJob pod where credentials arrive via `imagePullSecrets`-derived volume mounts or environment variables.

#### Added

- **`--registry-credentials-dir <PATH>` CLI flag** on `mikebom sbom scan`. Probes K8s secret-mount filenames in order: `config.json` (plain Docker), `.dockerconfigjson` (K8s `kubernetes.io/dockerconfigjson` secret type), `.dockercfg` (legacy K8s `kubernetes.io/dockercfg` secret type). First readable + parseable file wins; standard Docker config shape applies.
- **Env-var credential sources**:
  - **Per-registry**: `MIKEBOM_REGISTRY_<HOST>_USERNAME` + `MIKEBOM_REGISTRY_<HOST>_PASSWORD`, where `<HOST>` is the registry hostname uppercased with `[^A-Z0-9]` replaced by `_` (e.g. `ghcr.io` → `MIKEBOM_REGISTRY_GHCR_IO_USERNAME`; `my-ecr.amazonaws.com` → `MIKEBOM_REGISTRY_MY_ECR_AMAZONAWS_COM_USERNAME`).
  - **Generic**: `MIKEBOM_REGISTRY_USERNAME` + `MIKEBOM_REGISTRY_PASSWORD` — applies to every registry as a catch-all (useful when a cluster scan only ever hits one registry).
- **`resolve_credentials_layered`** entry point in `scan_fs/oci_pull/auth.rs` with documented precedence: env vars → `--registry-credentials-dir` → default Docker config. Existing `resolve_credentials` is unchanged — the new function wraps it.
- **8 new unit tests** in `auth.rs::tests` covering env-var resolution (per-registry, generic, partial-pair fallback, precedence), credentials-directory probing (config.json first, `.dockerconfigjson` fallback, malformed-skip-and-retry, empty-dir-returns-None). Environment-mutating tests serialize on a `Mutex<()>` matching the convention from `cache.rs` and `attestation/signer.rs`.
- **`docs/user-guide/cli-reference.md`** documents the new flag inline + the full 4-layer credential-resolution priority chain.

#### Security

- The `Credential` type's redacting `Debug` impl (`username` / `secret` → `<redacted>`) is preserved; the new env-var path doesn't introduce any logging that could leak credentials. Partial env-var configurations (USERNAME without PASSWORD or vice versa) are treated as no-credentials rather than synthesizing half-complete credentials. The `--registry-auth <registry>=<user>:<password>` CLI flag from Mario's spec is **deliberately not implemented** in this PR because credentials on the command line are visible to other processes via `/proc/<pid>/cmdline` and end up in shell history; env vars + secret-mount cover production cases cleanly.

#### Compatibility

- No behavior change for existing users. When neither `--registry-credentials-dir` nor any `MIKEBOM_REGISTRY_*` env var is set, the resolver falls through to the default Docker config path with the existing precedence (`credHelpers` > `credsStore` > `auths.<reg>.auth` > `auths.<reg>.identitytoken`).

## [0.1.0-alpha.35] — 2026-05-25

This release closes the C/C++ binary-SBOM defect cluster the reporter surfaced after alpha.34. Four PRs ship together: three bug fixes addressing edge-orphan and root-identity defects in the synthesized-root code paths (#237 / #239) and the cross-format scope-edge parity (#238); plus milestone 104 fixing the binary-role typing inversion that was the root of the reporter's "feels off" observation about `/bin/ls`.

### Milestone 104 — Binary role classification (Application vs Library)

Every binary discovered by the file-level binary reader (`mikebom-cli/src/scan_fs/binary/`) was historically emitted with CycloneDX `type: "library"` regardless of whether the file was an executable or a shared library. SBOM consumers reading the `type` field on `/bin/ls` saw `library` — the inverse of reality: `ls` is an application, not something other components link into. This milestone classifies every binary-reader-discovered component into one of four roles (`Application`, `SharedLibrary`, `Object`, `Other`) by reading the file's format header and maps the role to the format-native component-type field in each of CycloneDX, SPDX 2.3, and SPDX 3.

**Classification rules** (per the [binary-role cross-format contract](./specs/104-binary-role-classification/contracts/binary-role-cross-format-mapping.md)):

- **Mach-O**: `MH_EXECUTE` → Application; `MH_DYLIB` → SharedLibrary; `MH_OBJECT` → Object; `MH_BUNDLE` / `MH_KEXT_BUNDLE` / `MH_CORE` → Other.
- **ELF**: `ET_EXEC` → Application; `ET_DYN` with `PT_INTERP` program-header → Application (PIE executables, the modern Linux default); `ET_DYN` without `PT_INTERP` → SharedLibrary; `ET_REL` → Object; `ET_CORE` → Other.
- **PE**: `IMAGE_FILE_DLL` characteristic bit unset → Application; set → SharedLibrary; `IMAGE_FILE_SYSTEM` → Other.
- **Universal/fat Mach-O**: classification taken from the first slice's filetype (FR-006), matching the existing milestone-030 convention for identity metadata extraction.

**Format mapping**:

| Role          | CycloneDX 1.6 `Component.type` | SPDX 2.3 `Package.primaryPackagePurpose` | SPDX 3.0.1 `software_Package.software_primaryPurpose` |
|---------------|---------------------------------|------------------------------------------|------------------------------------------------------|
| Application   | `application`                   | `APPLICATION`                            | `application`                                         |
| SharedLibrary | `library`                       | `LIBRARY`                                | `library`                                             |
| Object        | `file`                          | `FILE`                                   | `file`                                                |
| Other         | `library` (historic default)    | _omitted_                                | _omitted_                                             |

Per Constitution Principle V, all three target formats have purpose-built native fields for component typing — the role rides exclusively through those standards-native slots. **No `mikebom:binary-role` annotation is introduced**; the existing `mikebom:binary-class` annotation (carrying format `elf`/`macho`/`pe`) is preserved as a distinct signal.

#### Added

- **`BinaryRole` enum** in `mikebom_common::resolution` with four variants (`Application` / `SharedLibrary` / `Object` / `Other`). Serde-renames as `snake_case` for stable wire-format serialization.
- **`binary_role: Option<BinaryRole>` field** on `ResolvedComponent` and `PackageDbEntry`. `Some(_)` for components from the binary reader; `None` for manifest- and lockfile-driven readers.
- **`mikebom-cli/src/scan_fs/binary/role.rs` module** containing the four-way classifier, the ELF `PT_INTERP`-based PIE disambiguation helper, and 9 unit tests covering each role variant per format.
- **CDX emission**: `binary_role_to_cdx_type` helper in `generate/cyclonedx/builder.rs` replaces the hardcoded `"type": "library"` literal.
- **SPDX 2.3 emission**: `primary_package_purpose` derivation in `generate/spdx/packages.rs` extended to honor `binary_role` ahead of the existing main-module-tagged → APPLICATION fallback. `Library` and `File` variants of `SpdxPrimaryPackagePurpose` lose their `#[allow(dead_code)]` attributes (first real uses).
- **SPDX 3 emission**: `software_primaryPurpose` derivation in `generate/spdx/v3_packages.rs` mirrors the SPDX 2.3 logic.
- **Parity catalog row A13** in `parity/catalog.rs` (auto-loaded from the new `sbom-format-mapping.md` row) plus per-format extractors `cdx_binary_role` / `spdx23_binary_role` / `spdx3_binary_role`, scoped to binary-reader-emitted components only (detected via the existing `mikebom:binary-class` property/annotation). Marked `SymmetricEqual` so all three formats must agree component-by-component (FR-008).
- **Tracing audit logs** at scan time for the ambiguous-fallback cases (FR-004): ELF ET_DYN PIE classification and Mach-O `Unknown` (bundle / kext) fallback. Lets operators audit unexpected role classifications without source-level debugging.
- **Two new integration tests** in `mikebom-cli/tests/`: `binary_role_parity.rs` (5 tests covering CDX, SPDX 2.3, SPDX 3 per-format role typing + cross-format agreement) and `binary_role_disambiguation.rs` (3 tests covering ELF PIE vs shared-library disambiguation, fat Mach-O first-slice classification per FR-006, and the MH_BUNDLE → Other → CDX library fallback).
- **`docs/reference/sbom-format-mapping.md`** new row A13 documenting the role-mapping table and pointing at the milestone's contract file. Constitution Principle V documentation requirement satisfied.

#### Changed

- The CycloneDX `Component.type` field for binary-reader-emitted components is now role-aware. Pre-milestone-104 every binary-reader component emitted `type: "library"`; post-fix executables emit `type: "application"`, shared libraries continue to emit `type: "library"`, object files emit `type: "file"`, and the `Other` bucket preserves the historic `"library"` default to avoid breaking consumers reading components the spec can't classify further.
- The SPDX 2.3 `primaryPackagePurpose` and SPDX 3 `software_primaryPurpose` fields are now populated for binary-reader-emitted Packages (pre-milestone-104 both were omitted unless the component carried the main-module annotation).
- No goldens regenerated. Per R4, no existing ecosystem fixture (cargo, gem, golang, maven, npm, pip, rpm, deb, apk, bazel, cmake) exercises the binary reader path — they're all manifest-driven, so the existing byte-identity goldens are unchanged.

### Issue #236 — image-scan SPDX root no longer orphaned + cross-format root PURL parity

Fixes two related defects in mikebom's emission of a *synthesized* root for scans that have no natural single root (container images, OS-package-only scans, `requirements.txt`- / `Gemfile`-only Python and Ruby projects, and any other case where milestone-053-style main-module annotation isn't set).

**The orphaned-root bug.** Before the fix, when the SPDX 2.3 / SPDX 3 emitters synthesized a placeholder root Package for the scan subject (via `synthesize_root` / `pick_root_iri`), the placeholder had no outgoing `DEPENDS_ON` / `dependsOn` edges. The synthetic root was only the target of the document-level `DESCRIBES` relationship; every top-level package the scan discovered was a graph-top with no incoming dependency edge. A consumer walking the dependency graph from the declared root saw zero direct deps. CDX has not had this problem since milestone 084 (closed via the primary-dependency fallback in `cyclonedx/dependencies.rs:74-99`, which synthesizes `metadata.component.bom-ref → <every component that nothing else depends on>` when the bom-ref has no outgoing edges). This fix mirrors that fallback into both SPDX emitters: when a root is synthesized, mikebom now emits one `DEPENDS_ON` / `dependsOn` edge from the synthesized root SPDXID/IRI to every graph-root component, preserving cross-format parity.

**The root-PURL divergence.** Before the fix, the synthesized-root PURL differed across the three formats for the same scan target. For `postgres:16`, CDX produced `pkg:generic/postgres:16@0.0.0` (via `encode_purl_segment`, which preserves the colon literal — matching the Debian / dpkg convention), SPDX 2.3 produced `pkg:generic/postgres_16@0.0.0` (via `sanitize_for_coord`, which collapses non-alphanumeric to `_`), and SPDX 3 produced `pkg:generic/postgres-16@0.0.0` (via `url_friendly`, which collapses non-alphanumeric to `-`). Three different root identities for the same image. This fix switches both SPDX synthesize-root paths to use `encode_purl_segment` for the PURL (matching CDX), while keeping the format-specific sanitizers for the CPE field (CPE has its own grammar rules). Post-fix, all three formats emit the identical root PURL `pkg:generic/postgres:16@0.0.0`.

**Verified.** Reconfirmed on `alpine:3`: CDX, SPDX 2.3, and SPDX 3 now all show `pkg:generic/alpine:3@0.0.0` as the root identity with 8 outgoing edges each (one per top-level apk package). Both bugs gone.

#### Added

- **Synthetic-root → graph-root `DEPENDS_ON` edges in SPDX 2.3** (`generate/spdx/document.rs:465-507` post `build_relationships` call). Fires only when `synthesize_root` runs; emits edges in deterministic PURL-lex order. Graph roots are defined as the same set CDX uses: components with `parent_purl: None` that aren't the target of any other dep edge.
- **Synthetic-root → graph-root `dependsOn` `Relationship` elements in SPDX 3** (`generate/spdx/v3_document.rs:609-647`, between containment and license relationships, before the final sort). Same graph-root definition as SPDX 2.3.
- **Three new unit tests in `generate/spdx/document.rs`** covering the synthesized-root PURL form (colon literal preserved), the synth-root-has-outgoing-edges invariant, and the "already-depended-on components are excluded from the fallback" subtlety that mirrors CDX's `cyclonedx/dependencies.rs:91-95` filter.

#### Changed

- **SPDX 2.3 synthesized-root PURL** switched from `sanitize_for_coord` (collapses `:` → `_`, lowercases) to `encode_purl_segment` (preserves the colon literal). CPE field keeps `sanitize_for_coord` — different grammar rules.
- **SPDX 3 synthesized-root PURL** switched from `url_friendly` (collapses `:` → `-`) to `encode_purl_segment`. CPE field keeps `url_friendly`.
- **SPDX 2.3 + SPDX 3 byte-identity goldens regenerated for `apk`, `bazel`, `cargo`, `cmake`, `deb`, `gem`, `pip`** (14 files, +270 lines, -0 lines — purely additive). Each diff is a constant set of synthetic-root → graph-root `DEPENDS_ON` edges. The other 8 SPDX 2.3 / SPDX 3 goldens (`golang`, `maven`, `npm`, `rpm`) are byte-identical to alpha.34 because their fixtures all have a main-module annotation and never hit the `synthesize_root` branch.
- **Parity-extractor coverage extended** (`parity/extractors/common.rs` + `parity/extractors/cdx.rs`). The pre-fix extractors deliberately skipped synthetic roots from the dep-edge buckets because those roots had no edges to walk; post-fix they're load-bearing edge sources, so the new helper `walk_cdx_components_main_module_and_synth_subject` (used by `cdx_dependency_edges`) and a synth-root inclusion in `spdx_relationship_edges` close that gap. The component-count extractors (A1–A12) still exclude synthetic roots — those rows count real components, not placeholders.
- **`transitive_parity_gem` baseline bumped 196 → 217** (fastlane has no top-level `.gemspec`, so synthesize_root fires).
- **`transitive_parity_pip_plain` baseline bumped 0 → 13** (the synthetic 13-pkg requirements.txt fixture). FR-008's "all 3 tools agree on zero" invariant moves to a soft warning — mikebom's CDX has been emitting these edges since milestone 084, and post-fix SPDX agrees with mikebom's own CDX. The `cross_tool_parity_check` continues to surface the divergence-from-Trivy/Syft as informational output.
- **`transitive_parity_pip_poetry` baseline bumped 62 → 88** (same root cause: poetry fixture lacks the main-module annotation, so synthesize_root fires).

### Issue #228 — SPDX 2.3 cross-format parity for scoped deps

Adds a `mikebom:lifecycle-scope` annotation to every scoped Package in SPDX 2.3 output and introduces a new `--spdx2-relationship-compat {full|basic}` CLI flag that selects the SPDX 2.3 relationship-type vocabulary the emitter uses for scoped dependency edges (dev / build / test).

**Background.** Milestone 052/part-2 removed the legacy `mikebom:dev-dependency` annotation from SPDX 2.3 emission on the grounds that the spec-native typed reversed-direction relationship variants — `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` — already carried the same signal. That decision is still defensible per Constitution Principle V, but it leaves SPDX 2.3 consumers that walk only `DEPENDS_ON` (the Trivy / Syft convention, which covers most deployed SBOM-consumer tooling — verified by source-code review of `aquasecurity/trivy` `spdxRelationshipType()` and `anchore/syft` `lookupRelationship()`) unable to see scope-on-edge. Issue #228's reporter ran exactly that walk against `go.mod` projects with test deps (e.g. `testify`) and observed the SPDX-vs-CDX edge-count delta.

**Both modes are spec-conformant, but they are not equivalent.** The SPDX 2.3 spec defines `DEV_DEPENDENCY_OF`, `BUILD_DEPENDENCY_OF`, and `TEST_DEPENDENCY_OF` for exactly the purpose of expressing dev/build/test scope on a dependency edge — the spec's intent is that the most specific applicable field should be used. Constitution Principle X (Transparency) further requires mikebom to default to the spec-native mechanism that preserves the most consumer-actionable signal. **We want more transparency in SBOM output, not less.** Default `full` honors both: it emits the typed scoped variants the spec built for this purpose, and a consumer that implements the full SPDX 2.3 relationshipType enum sees the dev/build/test distinction directly on every scoped edge.

The new flag exists because some downstream consumers — notably Trivy and Syft, plus tooling built on top of them — only implement the basic relationship vocabulary (`DEPENDS_ON` / `CONTAINS` / `DESCRIBES`) and silently ignore the typed scoped variants the spec also defines. The default doesn't change mikebom's existing behavior: typed variants stay the SPDX 2.3 default emission (`--spdx2-relationship-compat=full`). What does change is that the target Package now ALSO carries a `mikebom:lifecycle-scope: "development" | "build" | "test"` annotation, so consumers walking a flat `DEPENDS_ON` view can still recover the dev/build/test distinction by inspecting the Package itself. The annotation is the same one CDX has carried since milestone 052/part-2 (parity-catalog row C42) — issue #228 simply extends it to SPDX 2.3 under Constitution Principle V's documented parity-gap carve-out. SPDX 3 is unaffected — it carries scope natively via `LifecycleScopedRelationship.scope` and intentionally does not emit the annotation.

`--spdx2-relationship-compat=basic` is an explicit downshift, not an equivalent alternative. When set, every dep — runtime, dev, build, test — emits as a natural-direction `DEPENDS_ON` edge, and scope info lives entirely on the Package annotation. Operators who pick `basic` accept information loss on the edge in exchange for compatibility with tooling that doesn't read the typed variants. That choice is fine when targeting Trivy / Syft / similar downstreams; it is the wrong default for SBOMs that should be maximally informative.

**Why this matters to SBOM consumers.** Knowing whether a component is dev-only, build-only, or test-only — vs. a deployed-runtime dependency — is consumer-critical signal. It is the difference between an actionable CVE on a shipped artifact and one that doesn't affect production. Tooling that can't distinguish these scopes will over-report risk against test deps (`testify`, `junit`, `criterion`, `mocha`) and under-report against deployed runtime deps. Defaulting to the spec-rich `full` mode AND emitting the Package annotation guarantees the signal is recoverable from the document in either compat mode.

#### Added

- **`--spdx2-relationship-compat <PROFILE>`** CLI flag on `mikebom sbom scan`. Accepts `full` (default — current behavior, spec-native typed variants) or `basic` (every dep as natural-direction `DEPENDS_ON`, for downstream tooling that only implements the basic SPDX 2.3 relationship vocabulary). Only affects the `spdx-2.3-json` format; CDX and SPDX 3 emission are unaffected.
- **`mikebom:lifecycle-scope` Package annotation in SPDX 2.3 output**, emitted for every non-runtime scoped Package regardless of compat mode. The annotation field name and `development` / `build` / `test` value set match the existing CDX `mikebom:lifecycle-scope` property (parity-catalog row C42).
- **Five new unit tests** covering the new code paths: `basic_compat_collapses_dev_to_depends_on` and `full_is_the_default_relationship_compat` in `generate/spdx/relationships.rs`; `lifecycle_scope_annotation_emitted_for_test_scope`, `lifecycle_scope_annotation_emitted_for_dev_and_build`, and `lifecycle_scope_annotation_omitted_for_runtime_and_none` in `generate/spdx/annotations.rs`.
- **`docs/reference/sbom-format-mapping.md`** rows B2 and C42 extended with the SPDX 2.3 emission story, the compat flag, and the consumer-importance rationale per Constitution Principle V's documentation requirement.
- **`docs/user-guide/cli-reference.md`** new `--spdx2-relationship-compat` section with examples.

#### Changed

- **SPDX 2.3 byte-identity goldens regenerated for `maven` and `bazel`** — the only two ecosystem fixtures in the suite that exercise non-runtime scopes (maven `<scope>test</scope>` on junit; Bazel `dev_dependency` rules). Diff is purely additive: one new `mikebom:lifecycle-scope` annotation per scoped Package, no edge-type changes (default mode unchanged). The other 9 SPDX 2.3 goldens (apk, cargo, cmake, deb, gem, golang, npm, pip, rpm) are byte-identical to alpha.34.
- **`spdx3_annotation_fidelity.rs::collect_spdx23`** updated to skip the `mikebom:lifecycle-scope` field when computing cross-format-fidelity diffs — the annotation is SPDX-2.3-only by design (SPDX 3 carries the same signal natively), so the fidelity test would otherwise flag an intentional asymmetry as drift.

### Milestone 073 — identifiers (built-in + user-defined)

Adds dedicated identifier flags (`--repo`, `--git-ref`, `--image-id`, `--attestation`, `--id <scheme>=<value>`) to `mikebom sbom scan` (path + image modes) and `mikebom trace run`, plus auto-detected identifiers on `--path` scans (from the git origin remote, with `upstream` + first-listed fallbacks per the 3-step Q1 algorithm) and `--image` scans (from the resolved image reference + manifest digest, in the `image:<registry>/<name>:<tag>@sha256:<digest>` canonical Q3 shape). Built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) ride per-format standards-native carriers (CDX `metadata.component.externalReferences[]`, SPDX 2.3 dual-carrier on main-module `Package.externalRefs[PERSISTENT-ID]` + `creationInfo.creators` redundant text line, SPDX 3 `Element.externalIdentifier[]`); user-defined schemes (e.g., `acme_corp_id:`, `internal_ticket:`) ride a `mikebom:identifiers` document-level annotation per Constitution Principle V's documented-exception path. Sets up milestone 074's `--bind-to-source <identifier>` resolution path with no additional emission-side work needed at that point.

The original draft of this milestone called the feature "source identifiers" and shipped a single `--with-source <scheme>:<value>` flag. Both the name and the CLI surface were refactored before the milestone shipped: "source" anchored on the most-common case (source repos) but the same mechanism handles image / attestation / user-defined identifiers — the SPDX 3 spec already calls these `Element.externalIdentifier[]`, so the term was generalized to "identifier". Separately, the `<scheme>:<value>` syntax was visually ambiguous when values contained colons (URL ssh forms, image `@sha256:` digests) and operator-hostile despite being mechanically correct; dedicated flags per built-in scheme are self-documenting and unambiguous. Milestone-072's `SourceDocumentBinding` is a DIFFERENT concept (binding back to a source-tier SBOM document) and intentionally retains its name.

### Added

- **Dedicated identifier flags** on `mikebom sbom scan` (path + image modes) and `mikebom trace run` (FR-002):
  - `--repo <url>` — emits a `repo:` identifier
  - `--git-ref <revision>` (with `--repo`) — upgrades to a `git:<repo>#<revision>` identifier (supersedes the bare `repo:`)
  - `--image-id <ref>` — emits an `image:` identifier (named `--image-id` to avoid colliding with the existing `--image <PATH>` scan-input flag)
  - `--attestation <iri>` — emits an `attestation:` identifier
  - `--id <scheme>=<value>` (repeatable) — user-defined-namespace identifier; built-in scheme names (`repo`/`git`/`image`/`attestation`) are REJECTED at clap parse time with a message pointing the operator at the dedicated flag.

  Scheme regex `^[a-z][a-z0-9_-]*$` (FR-004) is enforced on `--id` schemes. Empty values rejected at clap parse time (`IdentifierError::EmptyValue`); malformed schemes rejected (`IdentifierError::InvalidSchemeName`). Built-in scheme value validation runs and may soft-fail to opaque pass-through under `mikebom:identifiers` (research.md §1 / VR-005) — same behavior as the pre-refactor flag.
- **Auto-detected `repo:` identifier on `--path` scans** (FR-001) — when the scan root is a git checkout, `git remote get-url <name>` runs with a 3-step name fallback per Q1 clarification: `origin` → `upstream` → first-listed-alphabetical. The chosen remote name is recorded in the standards-native carrier's comment field for transparency (FR-007). Auto-detection failure is `tracing::info!` only — never fails the scan.
- **Auto-detected `image:` identifier on `--image` scans** (FR-008) — synthesizes the canonical `image:<registry>/<name>:<tag>@sha256:<digest>` form from the resolved image reference + extracted manifest digest. Components are omitted when not available per the Q3 clarification (tarball mode without registry; pre-distribution-spec without digest).
- **Per-format standards-native carriers** for the 4 built-in schemes (FR-005) per Constitution Principle V — CDX `metadata.component.externalReferences[]` with per-scheme `type` mapping (`vcs` for `repo:`/`git:`, `distribution` for `image:`, `attestation` for `attestation:`); SPDX 2.3 dual-carrier on main-module `Package.externalRefs[PERSISTENT-ID]` (typed primary) + `creationInfo.creators` text lines (`Tool: mikebom-<version> source: <full-identifier>`, free-form fallback) per Q2 clarification; SPDX 3 `Element.externalIdentifier[]` on the `SpdxDocument` element (perfect fit for SPDX 3's open-typed multi-identifier model).
- **`mikebom:identifiers` document-level annotation** (parity-catalog row C47) — user-defined-namespace identifiers ride a single annotation envelope (`MikebomAnnotationCommentV1` reused from milestone 071). The `value` is a JSON array of `{scheme, value}` objects sorted lex by `(scheme, value)` for determinism (FR-009). Emitted ONLY when the user-defined entry set is non-empty (VR-007 — preserves cross-format byte-identity for non-user-defined-namespace scans). SPDX 3 carries user-defined identifiers natively in `Element.externalIdentifier[]`, NOT in this annotation.
- **Override semantics** (FR-006) — manual identifier flags override auto-detected identifiers on `(scheme, value)` match. Override-position rule per analyze F1: when manual deduplicates against auto-detected on `(scheme, value)`, manual inherits the auto-detected position; when manual overrides on different value (true override), auto-detected is dropped and manual follows in supply order (NOT promoted to front).
- **Parity catalog row C47** registered for `mikebom:identifiers` with `Directionality::SymmetricEqual`. The cross-format-parity test suite passes against the new row across CDX / SPDX 2.3 / SPDX 3 — user-defined identifiers byte-equivalent across formats after canonicalization.
- **`docs/reference/identifiers.md`** (FR-010) — published external-consumer guide. Covers wire format, CLI surface (with a migration note from the pre-073 `--with-source` draft), the 4 built-in schemes' per-format carriers, the user-defined-passthrough rule + `mikebom:identifiers` envelope shape, the auto-detection algorithm (3-step git fallback + `image:` synthesis), the determinism contract, runnable `jq` decode recipes per format, a Python reference extractor, the V1 stability commitment, and a forward pointer to milestone 074. SC-006 deliverable: external auditor writes a working extractor from this doc alone.
- **`docs/design-notes.md`** updated with a new "Identifiers (milestone 073)" section pointing operators at the published guide and explaining operator-visible behavior.
- **5 new integration tests** under `mikebom-cli/tests/`: `identifiers_emission.rs` (US1 — auto-detect happy path + 3-step fallback in all 3 formats), `identifiers_manual.rs` (US2 — manual flag emission + override + dedup + parse-time errors + soft-fail-to-opaque + the new built-in-scheme rejection on `--id`), `identifiers_per_tier.rs` (US3 — image-tier auto-detect + cross-tier consistency), `identifiers_determinism.rs` (US4 — deterministic emission + cross-format consistency).

### Migration

- The 27 alpha.15 byte-identity goldens for non-git fixtures stay byte-identical (no auto-detection fires when no git remote is present in fixtures). All `cdx_regression`, `spdx_regression`, and `spdx3_regression` byte-identity tests pass without regen.
- The dedicated identifier flags are opt-in. Operators not passing them see no behavior change beyond the new auto-detection paths (which only fire on git checkouts and `--image` scans).
- The `SpdxExternalRef` struct in `mikebom-cli/src/generate/spdx/packages.rs` adds an optional `comment: Option<String>` field. Existing constructions get `comment: None` and emit identical bytes to alpha.15.
- **Pipelines that prototyped against the original `--with-source` draft must update**: there is no compatibility shim. Replace `--with-source repo:<url>` with `--repo <url>`, `--with-source image:<ref>` with `--image-id <ref>`, `--with-source attestation:<iri>` with `--attestation <iri>`, `--with-source acme_corp_id:abc` with `--id acme_corp_id=abc` (note `=` separator on `--id`, not `:`).

### Out of scope (deferred to milestone 074)

- **Identifier-keyed `--bind-to-source` resolution** — passing `--bind-to-source repo:git@...` and having mikebom find the matching source SBOM file path via a local lookup directory. Milestone 074's scope. This milestone (073) emits identifiers in a shape 074's resolution layer can consume; no emission-side work needed at 074 land time.

## [0.1.0-alpha.15] — 2026-05-05

The **milestone 072 closure release.** Three sequential PRs (#140, #141, #142) shipped end-to-end cross-tier SBOM binding. With this release, an operator can: (a) emit image-tier SBOMs with verifiable cross-tier binding metadata; (b) verify the binary in the image matches the source SBOM via `mikebom sbom verify-binding`; (c) propagate VEX statements safely from source to image, with binding-strength-aware caveats by default; (d) triage which source SBOM (if any) describes a build that produced an image-tier component via `mikebom sbom trace-binding`; (e) hand external auditors `docs/reference/cross-tier-binding.md` and the binding-fixtures/ reference set so they can write their own verifier without mikebom source-code access.

The user's two specific worries are both addressed: "we cannot verify the binary running in the image matches the version that the source or build SBOM is built for" → `verify-binding`; "a vulnerability in the source has a VEX against it but the image actually has the vuln through some other path" → per-instance VEX with `caveated`-mode propagation default.

### Milestone 072 — cross-tier SBOM binding (closed by PR-C)

With PR-C merged, **milestone 072 is fully closed**: US1 (verify image's foo == source's foo) + US2 (VEX propagation respects binding strength) + US3 (operator triage via `trace-binding`) + the published verifier guide. The three-PR sequence (PR-A foundation + US1, PR-B US2, PR-C US3 + docs) implements every requirement FR-001..FR-012, satisfies every success criterion SC-001..SC-008, and lands the SC-004 published reference fixture set + external-verifier guide that an external auditor can write a working verifier from with zero source-code access.

### Added (milestone 072 PR-C — operator triage + published verifier guide: US3)

Third and final PR closing out milestone 072. PR-A delivered the foundation + verification (`mikebom sbom verify-binding`). PR-B delivered VEX propagation respecting binding strength. PR-C delivers **US3** (operator triage) and the **FR-010 published verifier guide**.

User-visible scope this PR:

- **`mikebom sbom trace-binding`** subcommand (FR-006) — operator triage tool answering "which source-tier SBOM (if any) corresponds to this image-tier component?". Args: `--component-purl <purl>`, `--image-sbom <path>`, EITHER `--source-sbom <path>` OR `--candidate-sources-dir <dir>` (mutually exclusive), `--format {table,json}` (default `table`). Reports per-instance binding state for every instance of the supplied PURL in the image SBOM. **Always exits 0** (informational, not validating — contrast with `verify-binding` which exits non-zero on hash mismatch). Mirrors the JSON output shape from `quickstart.md` Recipe 6.
- **`mikebom-cli/tests/binding_trace.rs`** — 3 integration tests covering the Recipe 6 scenarios: (a) component with one bound instance → `verified` with the bound source ID, (b) component with no candidate match → `unknown` with `reason: "source-not-found-in-bind-target"`, (c) two instances (one bound + one unbound) → both returned with their respective binding states.
- **`docs/reference/cross-tier-binding.md`** (FR-010) — comprehensive published guide for external verifier authors. **The SC-004 deliverable**: an external auditor can write a working verifier from this document alone and validate ≥95% of bindings against the published reference fixture set. Sections cover the binding-hash-v1 algorithm with worked examples for all three strength outcomes, per-ecosystem input-table (cargo / npm / pip / gem / maven / golang), per-format carrier shapes (CDX `properties[]`, SPDX 2.3 `MikebomAnnotationCommentV1` envelope, SPDX 3 `Annotation.statement`, plus standards-native `externalReferences` / `externalDocumentRefs` / `built_from` siblings), the OpenVEX `Product.identifiers` per-instance extension contract, the three VEX propagation modes plus the `affected ⊕ unbound-and-not-explicitly-vexed = affected` aggregation rule, a runnable Python verifier reference implementation (~150 lines, standard-library only), the V1 stability commitment + algo-version policy, and a pointer to the published reference fixtures for verifier-author acceptance testing. Mirrors the milestone-071 `conformance-harness-guide.md` structural model.
- **`docs/design-notes.md`** updated with a new "Cross-tier SBOM binding (milestone 072)" section pointing operators at the published guide and explaining the operator-visible behavior — when to use `--bind-to-source`, what `verify-binding` / `trace-binding` answer (validation vs. triage), and the migration path for operators using `--vex-overrides` today.

### Migration

- No SBOM output shape change for callers that don't use the milestone-072 commands. All 27 alpha.14 byte-identity goldens remain byte-identical.
- `trace-binding` is purely additive — a new read-only subcommand that doesn't modify any SBOM.
- The published `cross-tier-binding.md` guide is documentation-only; it has no runtime impact.

### Added (milestone 072 PR-B — VEX propagation respects binding strength: US2)

Second of three sequential PRs implementing milestone 072. PR-A delivered the foundation + verification (`mikebom sbom verify-binding`). PR-B delivers **US2**: cross-tier VEX propagation that respects the binding strength PR-A established. PR-C will add operator triage (`mikebom sbom trace-binding`) + the published verifier guide.

User-visible scope this PR:

- **`mikebom sbom enrich --vex-propagation-mode {permissive,caveated,strict}`** (FR-007) — new flag with default `caveated`. Wires real VEX propagation (replacing the pre-072 no-op `--vex-overrides` stub). Pre-072 callers needing exact prior behavior pass `--vex-propagation-mode permissive`. The flag's help text + the `mikebom sbom enrich --help` output document the breaking-change opt-out.
- **`propagate_vex_with_binding`** engine in `mikebom-cli/src/sbom/mutator.rs` — handles all three modes, applies per-`Directionality` matching (one-to-one when source statement carries `Product.identifiers.cyclonedx-bom-ref` / `spdx-spdxid`; one-to-many broadcast when source is pre-072 PURL-only). Emits per-instance `affects[]` entries on the target CDX `vulnerabilities[]` array.
- **Per-instance VEX emission** (FR-008) — propagated OpenVEX statements populate `OpenVexProduct.identifiers` with the target instance's `cyclonedx-bom-ref` / `spdx-spdxid` keys. Pre-072 OpenVEX consumers see byte-identical wire shape (the field is `skip_serializing_if_empty` from PR-A); post-072 consumers can apply VEX statements at instance granularity.
- **`mikebom:vex-binding-status: unverified` caveat** (FR-009) — when `caveated` mode propagates onto a non-`verified` instance, every `affects[]` entry on the target CDX `vulnerabilities[]` row carries this sibling annotation per `contracts/openvex-instance-identifiers.md` C-5. Operators reading the SBOM see exactly which propagated statements lack verified bindings.
- **Refusal-rationale annotations** — `strict` mode refuses to propagate onto non-`verified` instances. The refused (vulnerability, instance) pairs are recorded under a new `mikebom:vex-propagation-refusals` document-level property carrying a structured per-refusal record (`vulnerability`, `purl`, `bom_ref`, `binding_strength`, `reason`). The command exits non-zero per VR-006 so CI pipelines can gate on strict-mode outcomes; the SBOM is still written so operators can audit the refusal rationale.
- **`mikebom-cli/tests/binding_drift.rs`** — 2 tests: strict-mode refusal on weak binding (exit non-zero, no `vulnerabilities[]` entry, refusal annotation present); strict-mode acceptance on verified binding.
- **`mikebom-cli/tests/vex_per_instance.rs`** — 1 test, the canonical worked-example regression (US2 AS#4 / SC-003): two instances of `pkg:golang/golang.org/x/net@v0.28.0` (verified-bound + unbound), `caveated`-mode propagation correctly produces clean propagation on the bound instance and caveated propagation on the unbound instance.

### Changed — VEX propagation default flips from no-op to `caveated`

- Pre-072: `mikebom sbom enrich --vex-overrides <path>` was a documented no-op — the legacy flag did nothing. Post-072: same flag combination triggers real propagation in `caveated` mode by default. Callers depending on the no-op behavior pass `--vex-propagation-mode permissive` to disable binding-strength-aware caveats. Strictly speaking, this is a behavior change rather than a "breaking change in output that previously existed" — pre-072 there was no propagation output at all. Documented in spec SC-008 + the `--vex-propagation-mode` flag's help text.

### Migration

- No SBOM output shape change for callers that don't pass `--vex-overrides`. The 27 alpha.14 byte-identity goldens remain byte-identical.
- Callers passing `--vex-overrides` previously got nothing in the output; now they get propagated VEX statements with binding-aware caveats by default.

### Out of scope (deferred to PR-C)

- `mikebom sbom trace-binding` operator-triage subcommand (FR-006 / US3) — PR-C.
- `docs/reference/cross-tier-binding.md` published verifier guide (FR-010) — PR-C, lands when the full contract surface is implemented.

### Added (milestone 072 PR-A — cross-tier SBOM binding: foundation + US1)

This is the first of three sequential PRs implementing milestone 072. PR-A delivers the **foundational binding contract + US1** (verify image's foo == source's foo). PR-B will add US2 (`mikebom sbom enrich --vex-propagation-mode` + per-instance OpenVEX). PR-C will add US3 (`mikebom sbom trace-binding`). User-visible scope this PR:

- **`mikebom-cli/src/binding/`** — new module owning the layered binding-hash algorithm (FR-002), per-component `SourceDocumentBinding` annotation shape (FR-001), per-ecosystem source-input extraction (cargo / npm / pip / gem / maven / golang per research.md §1), and consumer-side verification logic (FR-005). Public re-exports include `compute_binding_hash`, `extract_source_inputs_for_component`, `verify_binding`, plus the data types (`BindingHashInputs`, `BindingHash`, `BindingStrength`, `SourceDocumentId`, `SourceDocumentBinding`, `VexPropagationMode`).
- **`mikebom sbom scan --bind-to-source <path>`** flag (FR-011) — image-tier scans loaded with this option resolve the named source-tier SBOM and emit per-component `mikebom:source-document-binding` annotations carrying the layered-hash + `BindingStrength` (verified / weak / unknown) labels. Non-`verified` components carry a structured `reason` per FR-003. `--path` scans (source-tier) warn-and-skip emission to preserve alpha.14 source-tier byte-identity.
- **`mikebom sbom verify-binding`** subcommand (FR-005) — given an image-tier SBOM and a source-tier SBOM, recomputes per-component binding hashes from the source-tier inputs and reports verification pass/fail. `--format {table,json}` (default `table`); exits non-zero on any verification failure per VR-005.
- **Standards-native cross-document references** per Constitution Principle V (FR-004) — CDX `metadata.component.externalReferences[type:bom]`, SPDX 2.3 `externalDocumentRefs[]` + `BUILT_FROM` relationship, SPDX 3 `import[] ExternalMap` + `Relationship[built_from]` graph element. Every cross-tier reference rides through standards-native fields; only the per-component hash + strength label live in the `mikebom:source-document-binding` annotation.
- **`OpenVexProduct.identifiers: BTreeMap<String, String>`** field added at `mikebom-cli/src/generate/openvex/statements.rs:71` per contracts/openvex-instance-identifiers.md C-1. The field is `skip_serializing_if_empty` — pre-072 wire shape preserved for callers that don't populate it. PR-B will populate `cyclonedx-bom-ref` / `spdx-spdxid` keys at propagation time when both source-VEX and target-SBOM are paired.
- **Parity catalog row C46** registered for `mikebom:source-document-binding` with `Directionality::SymmetricEqual` per milestone-071's invariant. The cross-format-parity test suite passes against the new row across all 9 ecosystem fixtures × 3 formats — image-tier emission is symmetric across CDX / SPDX 2.3 / SPDX 3.
- **Reference fixture set** at `docs/reference/binding-fixtures/` (SC-004) — three fixture pairs (`cargo-verified`, `golang-verified`, `maven-weak`) with pinned input triples + expected SHA-256 hex values. External verifiers writing their own implementations use these as the published reference set.
- **Algo v1 contract pinned** by 3 unit-test pinned-vector cases per analyze-C2 / SC-007 — future canonicalization changes break these tests, surfacing version-drift before consumers see it.

### Migration

- No SBOM output shape change for source-tier scans. The 27 alpha.14 byte-identity goldens remain byte-identical.
- Image-tier scans that don't pass `--bind-to-source` are byte-identical to alpha.14.
- `--bind-to-source` is opt-in; absent the flag, image-tier scans emit no binding annotations.

### Out of scope (deferred to PR-B / PR-C)

- VEX propagation logic (FR-007 / `--vex-propagation-mode`) is **PR-B**. The `OpenVexProduct.identifiers` field exists in PR-A but is empty at every emit-site. Pre-072 OpenVEX consumers see byte-identical output.
- Per-instance VEX emission carrier population (FR-008) is **PR-B**.
- `mikebom sbom trace-binding` operator-triage subcommand (FR-006 / US3) is **PR-C**.
- `docs/reference/cross-tier-binding.md` published verifier guide (FR-010) lands when the full contract surface is implemented (PR-C). The contract is fully specified in `specs/072-cross-tier-sbom-binding/contracts/` already and external implementers can follow those today.

## [0.1.0-alpha.14] — 2026-05-04

The **conformance-tooling polish release.** Two user-visible
improvements since alpha.13: granular network-enrichment skip
flags for large-scale users, and a real value-equality upgrade
to the `mikebom sbom parity-check` subcommand backed by a
comprehensive conformance-harness-author guide.

### Added (PR #136 — granular enrichment control flags)

- Three new flags on `mikebom sbom scan` give operators sub-`--offline` control over which post-scan enrichment sources fire: `--no-clearly-defined` (skip ClearlyDefined; deps.dev still active), `--no-deps-dev-graph` (skip deps.dev transitive dep-graph; license enrichment stays active), `--enrich-sources <list>` (allowlist mode overriding the `--no-*` flags). `--offline` retains its all-network-off semantics. Motivation: ClearlyDefined enrichment can dominate wall-clock on 1000+-component scans (~6+ minutes / ~87% of total scan time); these flags give large-scale users a finer-grained escape hatch than `--offline`. Underlying CD performance gap tracked separately as #137.

### Added (milestone 071 — cross-format SBOM annotation parity)

- **Conformance harness author guide** at `docs/reference/conformance-harness-guide.md` — a reference for external SBOM-conformance harness maintainers explaining how mikebom carries `mikebom:*` metadata in each of the three supported formats (CDX 1.6 / SPDX 2.3 / SPDX 3.0.1), the 7 inherent format-spec asymmetries that should NOT be flagged as cross-format-inequivalence findings, and how to wire a harness to read the `MikebomAnnotationCommentV1` envelope correctly. Authored against milestone 071's catalog state.
- **Synthetic drift regression test** at `mikebom-cli/tests/parity_synthetic_drift.rs` — pins the post-071 value-equality semantics by constructing a synthesized SBOM triple where a `SymmetricEqual` row's set CONTENTS differ across formats. Asserts the post-071 logic catches the drift; demonstrates the pre-071 presence-only logic would have silently passed it.
- `ParityExtractor` struct gains a `pub order_sensitive: bool` field at `mikebom-cli/src/parity/extractors/common.rs` for future order-sensitive annotation rows. Default `false` for all 68 currently-catalogued rows; rationale: every currently-named key is an unordered set under the existing `BTreeSet<String>` extractor model.
- `canonicalize_for_compare(value: &Value, order_sensitive: bool) -> String` helper at the same path — sorts object keys lex, sorts arrays lex (default) or preserves order (override), normalizes whitespace via `serde_json::to_string`. Available for future per-row value-payload comparison work.

### Changed (milestone 071 — `mikebom sbom parity-check` upgrade)

- **`mikebom sbom parity-check` now does real value-equality checking** instead of presence-only checking. Pre-071 the subcommand reported `Parity gaps: 0` whenever all three formats had ≥1 entry per universal-parity row, regardless of whether the actual set CONTENTS matched across formats. Post-071 it applies the per-`Directionality` invariants: set equality for `SymmetricEqual`, `cdx_set ⊆ spdx23/3` for `CdxSubsetOfSpdx`, presence-parity for `PresenceOnly`, CDX-non-empty for `CdxOnly`. The same logic the canonical `tests/holistic_parity.rs` integration test uses; the CLI subcommand and the integration test now return the same verdict.
- The presence-only undercounting of unexercised rows is also fixed — universal-parity rows where no format carries data are now correctly counted as "passing by default" rather than "neither passing nor failing." Typical output on a small-fixture cargo-workspace scan goes from `Universal-parity rows: 16 / 67 ✓` (pre-071) to `Universal-parity rows: 67 / 67 ✓` (post-071), reporting the same number of real gaps (zero) but with cleaner accounting.
- **Harness implication**: external conformance harnesses that shell out to `mikebom sbom parity-check` to validate cross-format parity were missing real value-drift bugs pre-071. Upgrade to alpha.14 or later for the rigorous check.

### Migration

- No SBOM output shape change. CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 emissions are byte-identical to alpha.13 (all 27 byte-identity goldens unchanged).
- No new `mikebom:*` annotation keys; no removed keys; no directionality changes in the parity catalog. Milestone 071 is purely an internal-verification + documentation milestone.

### Fixed (CI hardening — realistic-projects lane)

- The online knative-func scan step in the realistic-projects CI lane (`.github/workflows/realistic-projects.yml`) now skips ClearlyDefined + deps.dev-graph enrichment via `--no-clearly-defined --no-deps-dev-graph` (the new #136 flags). The step's purpose is exclusively the Go transitive-edge resolver gate (SC-003 from milestone 055); neither CD nor deps.dev-graph contributes to Go transitive edges. When CD's public API has a slow event (knative-func has 1000+ components and CD is ~87% of online wall-clock per #137), the lane was hitting the 15-minute job timeout. Skipping the irrelevant enrichment passes keeps the assertion focused and the job under budget. Fix-forward: #137 captures the underlying CD-perf work that would let the lane re-enable full enrichment without timing out.

## [0.1.0-alpha.13] — 2026-05-03

The **issue #104 closure release.** Three milestones since alpha.12
ship the final three per-ecosystem main-modules — pip, gem, and
maven — completing the project-self-identity coverage matrix
across every ecosystem mikebom supports.

### Changed (BREAKING — SBOM output shape, milestone 070 — closes #104 in full)

- **Maven project SBOMs now identify the project itself** via a
  synthetic main-module component for every project with a
  top-level `pom.xml`. Pre-070: maven SBOMs had no project-self
  component. Post-070: every Maven project emits a
  `pkg:maven/<groupId>/<artifactId>@<version>` component placed
  in standards-native "BOM subject" slots — CDX
  `metadata.component`, SPDX `primaryPackagePurpose: APPLICATION`
  plus `documentDescribes`, SPDX 3.0.1
  `software_primaryPurpose: application`. Carries
  `mikebom:component-role: main-module` (C40) supplementary
  signal per Constitution Principle V.

- **Multi-module reactor support (FR-002).** When the top-level
  `pom.xml` declares `<modules>`, mikebom emits one main-module
  component per submodule (and per the parent reactor POM
  itself) — leveraging the multi-DESCRIBES super-root
  infrastructure from milestone 064/#127. Each submodule POM is
  resolved through POM inheritance: missing `<groupId>` and
  `<version>` are filled from the `<parent>` block (FR-001
  step 2).

- **Property substitution (FR-012).** GAV coordinates containing
  Maven property references (`${project.version}`, `${revision}`,
  `${parent.groupId}`, custom keys defined in `<properties>`)
  are resolved at parse time. Unresolvable properties fall
  through verbatim with `tracing::warn!`, matching the
  cross-host determinism convention.

- **Install-state path exclusion (FR-003).** `pom.xml` files
  inside `target/`, `.m2/`, `node_modules/`, `vendor/` are NOT
  discovered for main-module emission. Those paths are handled
  by the existing dep-emission walker.

- **Same-PURL collision dedup** with operator-visible
  `tracing::warn!` per the cargo (064) / npm (066) / pip (068) /
  gem (069) Q1 convention.

### Migration (070)

- Consumers reading `metadata.component.purl` for Maven scans
  now receive `pkg:maven/<g>/<a>@<v>` instead of the pre-070
  `pkg:generic/...` placeholder.
- **Per-ecosystem main-module coverage matrix is now COMPLETE**:
  Go ✅ (053), cargo ✅ (064), npm ✅ (066), pip ✅ (068, PEP 621),
  gem ✅ (069, top-level `*.gemspec`), maven ✅ (070, top-level
  `pom.xml` + reactor + inheritance + property substitution).
  **Issue #104 fully closed.**

### Changed (BREAKING — SBOM output shape, milestone 069)

- **Ruby gem project SBOMs now identify the project itself** via a
  synthetic main-module component for every project with a
  top-level `*.gemspec`. Pre-069: gem SBOMs had no project-self
  component. Post-069: every Ruby gem project emits a
  `pkg:gem/<name>@<version>` component placed in standards-native
  "BOM subject" slots — CDX `metadata.component`, SPDX
  `primaryPackagePurpose: APPLICATION` plus `documentDescribes`,
  SPDX 3.0.1 `software_primaryPurpose: application`. Carries
  `mikebom:component-role: main-module` (C40) supplementary
  signal per Constitution Principle V.

- **Skip rule for application-style projects.** Per FR-002, Ruby
  projects with only `Gemfile` + `Gemfile.lock` (no top-level
  `*.gemspec`) emit NO main-module. Application-style projects
  don't have a project-self identity in the gem ecosystem; the
  existing `Gemfile.lock` dep emission is unaffected. This is
  gem-specific because Ruby explicitly distinguishes publishable
  gems (`*.gemspec` declares the gem) from applications
  (Gemfile-based dep management only).

- **Install-state path exclusion (FR-003).** `*.gemspec` files
  inside `vendor/`, `gems/`, `specifications/`, `.bundle/` are
  NOT discovered for main-module emission. Those paths are
  handled by the existing dep-emission walker.

- **Pure-Rust regex parsing.** Mikebom never executes the
  `*.gemspec` as Ruby code (Constitution Principle I). Literal-
  string assignments (`s.name = "foo"`, `s.version = "1.2.3"` —
  with optional `.freeze` chaining) are extracted; constant
  references (`s.version = Foo::VERSION`) and dynamic
  computations fall through to the `0.0.0-unknown` placeholder
  per the cross-host determinism convention from milestones
  053/064/066/068.

- **Same-PURL collision dedup** with operator-visible
  `tracing::warn!` per the cargo (064) / npm (066) / pip (068)
  Q1 convention.

### Migration

- Consumers reading `metadata.component.purl` for Ruby gem scans
  now receive `pkg:gem/<name>@<version>` instead of the pre-069
  `pkg:generic/...` placeholder.
- Per-ecosystem main-module coverage matrix: Go ✅ (053),
  cargo ✅ (064), npm ✅ (066), pip ✅ (068, PEP 621), gem ✅ (069,
  top-level `*.gemspec`); maven tracked in #104 (last remaining
  slice).

### Changed (BREAKING — SBOM output shape, milestone 068)

- **Python project SBOMs now identify the project itself** via a
  synthetic main-module component for every `pyproject.toml`
  containing PEP 621 `[project]` table. Pre-068: Python SBOMs had
  no project-self component. Post-068: every Python project scan
  emits a `pkg:pypi/<name>@<version>` component (with PEP 503
  name normalization — lowercase + underscore→hyphen) placed in
  standards-native "BOM subject" slots — CDX `metadata.component`,
  SPDX `primaryPackagePurpose: APPLICATION` plus
  `documentDescribes`, SPDX 3.0.1
  `software_primaryPurpose: application`. Carries
  `mikebom:component-role: main-module` (C40) supplementary
  signal per Constitution Principle V.

- **Skip rule for `[tool.poetry]`-only manifests.** Per issue
  #104's explicit guidance, Python projects using the pre-PEP-621
  Poetry schema (no `[project]` table) are skipped from main-
  module emission. Existing Poetry lockfile-driven dep emission
  is unaffected. `tracing::info!` notes the skip with a pointer
  to a future Poetry-extension follow-up issue. Manifests with
  BOTH `[project]` AND `[tool.poetry]` (Poetry 1.5+ shim case)
  emit from `[project]` — the standards-native PEP 621 source
  wins.

- **Editable-install merge precedence (FR-011).** When a venv
  `.dist-info` shares the same PURL as the milestone-068 main-
  module emitted from the project's own `pyproject.toml`, the
  augment-existing-entry logic preserves venv evidence
  (`mikebom:sbom-tier: deployed`, hashes from METADATA) while
  layering Phase A's C40 tag + `parent_purl: None` on top. The
  resulting main-module has both signals: project identity
  (Phase A) + installation evidence (venv). This is unique to
  pip — no equivalent in cargo/npm because their installation
  models differ.

- **`dynamic = ["version"]` → `0.0.0-unknown` placeholder.**
  When a PEP 621 manifest defers `version` resolution to
  setuptools-scm or similar, mikebom emits the literal
  `0.0.0-unknown` placeholder rather than shelling out to a
  Python toolchain. Cross-host determinism + zero-dependency
  posture preserved per the convention from milestones
  053/064/066.

- **PEP 508 dep-name extraction.** Direct deps from
  `[project.dependencies]` and `[project.optional-dependencies]`
  emit edges from the main-module via the same
  `name_to_purl`-resolution + dangling-target-drop convention
  as cargo + npm. PEP 508 markers, version specifiers, and
  extras are stripped — only the package name is used for edge
  resolution.

- **Same-PURL collision dedup** with operator-visible
  `tracing::warn!` per the cargo (064) / npm (066) Q1
  convention. Rare given `__pycache__/`, `.venv/`, `site-packages/`
  are excluded from manifest discovery.

### Migration

- Consumers reading `metadata.component.purl` for Python scans
  now receive `pkg:pypi/<pep503-name>@<version>` instead of the
  pre-068 `pkg:generic/...` placeholder.
- Per-ecosystem main-module coverage matrix: Go ✅ (053),
  cargo ✅ (064), npm ✅ (066), pip ✅ (068, PEP 621 only —
  Poetry-only `[tool.poetry]` deferred), maven / gem tracked in
  #104.

### Known gaps (filed for follow-up)

- **#104 Poetry coverage**: `[tool.poetry]`-only manifests
  currently skipped per #104's explicit guidance. If demand
  surfaces, a follow-up issue will extend the reader.
- **#103** — LICENSE detection for the new pip main-module
  (PEP 621 `[project].license` field, classifiers, LICENSE-file
  matching).
- **#125** — divergent-PURL detection extends to pip too.



## [0.1.0-alpha.12] — 2026-05-03

A focused per-ecosystem release expanding the main-module pattern
shipped for Go in alpha.10/053 to two more ecosystems (cargo + npm),
plus closing a pre-existing cargo workspace-root edge-emission gap
and a deps.dev enrichment perf bug surfaced during 064 review.

Three milestones since alpha.11 (~1 day later):

- **064**: cargo source-tree main-module component (closes the
  cargo slice of #104). Each `Cargo.toml` with `[package]` emits
  a `pkg:cargo/<name>@<version>` component in standards-native
  BOM-subject slots. Includes the multi-main-module super-root +
  plural-DESCRIBES wiring (#127) shipped as a side-fix —
  workspace scans correctly surface every member through
  `documentDescribes`/`rootElement`.
- **065**: cargo workspace-root direct-dep edges (closes #126).
  Pre-065 cargo SBOMs had a project-self component but no
  outgoing dep tree because `parse_lockfile` was skipping
  workspace-root entries. Post-065, those entries' `dependencies
  = [...]` declarations are harvested separately and merged into
  the milestone-064 main-module's `depends` field. Completes
  064's FR-007. Bundled with a deps.dev seed-skip for
  empty/`unknown`-version components — surfaced when knative-func
  realistic-project CI hit the 15-minute job timeout from dozens
  of guaranteed-404 maven calls.
- **066**: npm source-tree main-module component (closes the npm
  slice of #104). Each `package.json` with `name` (and not
  `private: true` + no version per #104's guidance) emits a
  `pkg:npm/<name>@<version>` (or `%40scope/name` encoded)
  component. Workspace handling extends the cargo pattern; no
  generator-side changes needed thanks to the C40-tag-driven
  hooks established in 053+064+#127.

### Changed (BREAKING — SBOM output shape, milestone 066)

- **npm SBOMs now identify the project itself** via a synthetic
  main-module component for every `package.json` with `name`.
  Pre-066: npm SBOMs had no project-self component. Post-066:
  every Node.js project scan emits a `pkg:npm/<name>@<version>`
  (or `pkg:npm/%40<scope>/<name>@<version>` for scoped names)
  component placed in standards-native "BOM subject" slots —
  CDX `metadata.component`, SPDX `primaryPackagePurpose:
  APPLICATION` plus `documentDescribes`, SPDX 3.0.1
  `software_primaryPurpose: application`. Carries
  `mikebom:component-role: main-module` (C40) supplementary
  signal per Constitution Principle V.

- **Skip rule for `private: true` + no `version`.** Per issue
  #104's explicit guidance, manifests with `private: true` AND
  no declared `version` are skipped from main-module emission —
  the author has signaled "not a publishable artifact." Common
  pattern: monorepo workspace roots. `private: true` + a
  declared `version` still emits (the flag is a publish guard,
  not an SBOM-presence signal).

- **Workspace handling.** npm 7+ `workspaces: ["packages/*"]`
  arrays are honored: each member `package.json` emits its own
  main-module. Workspace path-deps (`"<member>": "*"`) emit
  member-to-member `dependsOn` edges. `documentDescribes` is
  multi-target (one SPDXID per member, alphabetically sorted)
  via the milestone-064-#127 infrastructure that ships
  unchanged for npm.

- **Scoped name encoding.** `@scope/name` PURLs URL-encode the
  `@` to `%40` per PURL spec (`pkg:npm/%40scope/name@version`),
  reusing the existing `build_npm_purl` helper.

- **Same-PURL collision dedup.** When two-or-more `package.json`
  files yield identical PURLs (rare given `node_modules/` is
  excluded from manifest discovery, but defensive), exactly one
  main-module emits and a `tracing::warn!` lists dropped
  duplicate paths. Same convention as cargo (064) per spec
  Clarifications Q1.

- **No version-inheritance feature, no resolver ladder.** Unlike
  cargo's `version.workspace = true`, npm has no
  workspace-version-inheritance. The resolver collapses to two
  steps: literal version → `0.0.0-unknown` placeholder. The
  placeholder fires when `name` is declared but `version` is
  missing AND `private` isn't `true` (matching cargo's permissive
  behavior per spec Q1).

- **`node_modules/` excluded from manifest discovery.** Deliberate
  ecosystem-specific divergence from cargo's "emit excluded
  crates" rule (064 FR-003). `node_modules/` contains upstream
  dependencies, not project-internal artifacts; emitting
  main-modules for every `node_modules/*/package.json` would
  balloon SBOMs with thousands of FPs.

### Migration

- Consumers reading `metadata.component.purl` for npm scans now
  receive `pkg:npm/<name>@<version>` instead of the pre-066
  `pkg:generic/...` placeholder (or for the
  `tests/fixtures/npm/node-modules-walk` golden, the
  `package-json-only-fixture` synthetic identity).
- Per-ecosystem main-module coverage matrix: Go ✅ (053),
  cargo ✅ (064), npm ✅ (066), pip / maven / gem tracked in #104.

### Changed (BREAKING — SBOM output shape, milestone 064)

- **Cargo SBOMs now identify the project itself** via a synthetic
  main-module component for every `Cargo.toml` with `[package]`.
  Pre-064: cargo SBOMs had no project-self component, so consumers
  could not answer "what is this an SBOM for?" from the bytes alone.
  Post-064: every cargo crate scan emits a `pkg:cargo/<name>@<version>`
  component placed in standards-native "BOM subject" slots — CDX
  `metadata.component` (single-crate) or `components[]` siblings
  (workspace cases); SPDX `primaryPackagePurpose: APPLICATION` plus
  `documentDescribes`; SPDX 3.0.1 `software_primaryPurpose:
  application`. Carries `mikebom:component-role: main-module` (C40)
  as a supplementary signal per Constitution Principle V.

- **Workspace-only `Cargo.toml` files emit no main-module for the
  root.** A `Cargo.toml` declaring only `[workspace]` (no
  `[package]`) is not a publishable crate. Each `[workspace] members
  = [...]` entry emits its own main-module instead. Crates in
  `[workspace].exclude` directories that have their own `[package]`
  ARE emitted (the filesystem walker is authoritative; exclusion is
  a workspace-build concern, not an SBOM-coverage concern).

- **`version.workspace = true` resolution.** Member crates that
  inherit their version from `[workspace.package].version` now
  resolve to the actual workspace-root value in the main-module's
  PURL. Falls back to the literal `0.0.0-unknown` placeholder when
  the workspace root is outside the scan's filesystem boundary —
  same cross-host determinism convention as Go's milestone-053
  `git describe` ladder step 3.

- **Same-PURL collisions dedup with operator-visible warning.**
  When two-or-more `Cargo.toml` files resolve to the same
  `pkg:cargo/<name>@<version>` PURL (vendored copies, `examples/`
  mirrors, `target/package/` extractions), exactly one main-module
  emits (deterministic first-discovered-wins) and a `tracing::warn!`
  lists dropped duplicate paths. Divergent-PURL detection (same
  PURL, different content hashes — a potential supply-chain signal)
  deferred to follow-up issue #125.

- **Generator-side hooks generalized.** The milestone-053 CDX
  `metadata.component` selector and `components[]` exclusion
  predicates are now C40-tag-driven (filter by
  `mikebom:component-role: main-module`) instead of Go-PURL-prefix-
  driven. When the scan contains exactly 1 main-module, it is
  promoted to `metadata.component`; when N>1 (cargo workspaces,
  polyglot scans), all N emit as siblings in `components[]` under
  the existing synthetic super-root.

### Migration

- Consumers reading `metadata.component.purl` for cargo scans now
  receive `pkg:cargo/<crate-name>@<version>` instead of the
  pre-064 `pkg:generic/...` placeholder. Update vuln-intersection
  and licensing tools to recognize the new shape.
- The C40 supplementary annotation continues to emit unchanged on
  Go and now extends to cargo. The `mikebom:component-role:
  main-module` value identifies the project-self component for
  consumers that filter it from licensing-coverage denominators
  (sbomqs convention).
- Per-ecosystem main-module coverage matrix: Go ✅ (053),
  cargo ✅ (064), npm/pip/maven/gem tracked in #104.

### Resolved during alpha.12

- **#127** — multi-main-module workspaces (cargo + polyglot):
  closed by milestone 064 (#128). The synthetic super-root now
  emits plural DESCRIBES via SPDX 2.3 `documentDescribes[]` and
  SPDX 3 `rootElement[]`. Workspace scans correctly surface every
  member.
- **#126** — pre-existing cargo workspace-root edge-emission gap:
  closed by milestone 065 (#129). Workspace-root `[[package]]`
  entries' `dependencies = [...]` declarations are now harvested
  separately and merged into the main-module's `depends`.

### Known gaps (filed for follow-up)

- **#125** — divergent-PURL detection (cargo + npm): when two
  manifests claim the same PURL but have different content hashes
  (potential typosquatting / supply-chain signal), surface a
  stronger SBOM signal beyond the current `tracing::warn!`.
  Applies to the same-PURL dedup paths shipped in milestones 064
  + 066.
- **#103** — LICENSE detection for main-module components.
  Currently the C40 carve-out keeps sbomqs licensing-coverage
  scoring from regressing, but the main-module entries themselves
  have empty `licenses` fields. Real `license`/`license-file`
  reading + SPDX-License-Identifier header scanning + askalono
  content matching tracked here. Applies to all three
  main-module-bearing ecosystems.
- **#104** — remaining ecosystems (pip, maven, gem). The
  C40-tag-driven generator hooks are now ecosystem-agnostic, so
  the per-ecosystem effort is mostly reader-side (a few hundred
  LOC each, similar to milestones 064/066).




## [0.1.0-alpha.11] — 2026-05-02

A focused release on the Go ecosystem covering five milestones shipped
since alpha.10 (~1 day later). Closes the issue #102 residual gap
(transitive edges in offline / empty-cache scans), fixes a graph
topology lie identified during the #113 review, and adds the
Constitution-Principle-X transparency signal so consumers can
interpret partial graphs correctly. Also adds Layer-1 LICENSE
detection for the Go main-module per #103.

### Headline changes

- **Go transitive dependency edges work in offline + empty-cache
  scans.** Pre-055, scanning a Go project with `--offline` and an
  empty `$GOMODCACHE` produced an SBOM whose transitive components
  carried `dependsOn: []` — issue #102's residual gap after
  milestone 053. Post-055, a 4-step resolution ladder
  (`go mod graph` → `$GOMODCACHE` walk → proxy.golang.org fetch →
  graceful no-edges fallthrough) supplies edges from at least one
  source on every typical scan. Honors `$GOPROXY`, `$GOPRIVATE`,
  `$GONOSUMCHECK`. Differentiator vs. peers: Trivy / Syft / cdxgen
  all degrade in the "no `go` toolchain AND no cache" cell;
  mikebom is the only static SBOM tool that produces the full Go
  transitive graph there. (Milestone 055; PR #114.)

- **Go main-module dependency-graph topology no longer lies about
  direct vs indirect.** Pre-059, the synthetic main-module
  emitted ALL `require` lines from `go.mod` as direct edges
  (including those marked `// indirect`). Consumers asking "what
  does this project directly use?" got the wrong answer.
  Post-059, main-module emits ONLY non-`// indirect` requires as
  direct edges; `// indirect` modules are reached transitively via
  the milestone 055 resolver, or become orphans (Trivy-style
  trade-off) when the resolver can't supply transitive edges.
  Closes #113. (Milestone 059; PR #118.)

- **Graph-completeness transparency signal (Principle X).** The
  cost of milestone 059's correctness fix is that orphan
  components appear in offline + empty-cache scans. Pre-061,
  consumers couldn't tell "dead dep" from "mikebom couldn't
  resolve." Post-061, the SBOM signals the limitation natively:
  a document-level `mikebom:graph-completeness` annotation
  (`complete` / `partial`) + a `mikebom:graph-completeness-reason`
  free-text summary, plus per-component `mikebom:orphan-reason`
  on each orphan with the classification (`unresolved-indirect-
  require` / `private-module` / `proxy-fetch-failed`). Closes
  #119. (Milestone 061; PR #121.)

- **Go main-module LICENSE detection (Layer 1).** Closes
  milestone 053 FR-005's deferral. The synthetic main-module
  component now carries the project's own license expression
  populated from `LICENSE` / `LICENSE.md` / `LICENSE.txt` /
  `COPYING` / British `LICENCE` files at the workspace root via
  SPDX-License-Identifier header scan. Default-on; ~30–50% of
  high-profile Go projects ship the SPDX header. Closes #103.
  (Milestone 057; PR #116.)

- **Realistic-project CI gate now asserts Go transitive edges.**
  Milestone 054's `knative/func` regression job gains a second
  scan (without `--offline`) that counts `pkg:golang →
  pkg:golang` `dependsOn` edges and fails the gate if the count
  drops below the floor. Catches future regressions in milestone
  055's resolver. (T036/T037 of milestone 055; PR #115.)

### Breaking changes for SBOM consumers

- **Go main-module's `dependsOn` set shrinks** for every Go
  project that has `// indirect` requires in its `go.mod`. For
  the simple-module test fixture, main-module's outgoing edges
  drop from 10 to 5. SBOM consumers that count main-module's
  direct edges should expect smaller numbers per the new (and
  correct) topology. Indirect components are still present in
  `components[]`; consumers walking the graph beyond direct deps
  should follow `dependsOn` from each direct dep transitively.
  See #118 description for migration notes.

- **Two new CDX `metadata.properties[]` entries appear on every
  Go-touching scan**: `mikebom:graph-completeness` and (when
  partial) `mikebom:graph-completeness-reason`. Equivalents
  appear in SPDX 2.3 + SPDX 3 document-level annotations.
  Cross-format consumers that strict-validate `metadata.properties`
  / `annotations[]` length should expect 1–2 additional entries.
  See `docs/reference/sbom-format-mapping.md` C44.

- **New per-component property `mikebom:orphan-reason`** appears
  on Go components that the resolver couldn't reach. Three-state
  semantics: absent ⇒ component is reachable. Catalog row C45.

### Milestones in this release

- **055**: Go transitive dependency edges, anchored on go.sum
  (4-step ladder). Closes #102's residual gap. PR #114.
- **055 follow-up (T036/T037)**: realistic-project CI gate for
  Go transitive edges. PR #115.
- **057**: Go main-module LICENSE detection (Layer 1). Closes
  #103. PR #116.
- **059**: Go main-module dependency-graph topology fix —
  direct edges only. Closes #113. PR #118.
- **061**: SBOM graph-completeness transparency signal. Closes
  #119. PR #121.

### Follow-ups (open issues)

- **#104**: per-ecosystem main-modules for npm / cargo / maven /
  pip / gem (alpha.11 remains Go-only); when those land they'll
  inherit the Layer-1 LICENSE scanner from milestone 057 and
  the graph-completeness signal pattern from milestone 061.
- **#108**: migrate every filesystem walker to a single shared
  `safe_walk` helper (still deferred from milestone 054).
- **#109**: per-ecosystem expansion of the realistic-project CI
  matrix beyond knative/func.
- **#111**: umbrella transitive-dep correctness audit across
  all ecosystems (only Go addressed in alpha.11).
- **Layer 2** for Go LICENSE detection (content-based matcher
  via askalono or similar, for projects without SPDX headers
  like knative/func itself) — not yet tracked; file if real
  users hit a wall.

## [0.1.0-alpha.10] — 2026-05-02

A larger release covering seven milestones shipped since alpha.9
(~1 day later): three breaking changes to SBOM output shape PLUS
a critical hang fix that SBOM consumers should review before
upgrading.

### Headline changes

- **Filesystem-walker symlink-loop hang fixed.** Pre-054
  `mikebom sbom scan --path <project>` hung at 100% CPU
  indefinitely on any repo containing intentional symlink-loop
  test fixtures (e.g., knative/func @ `knative-v1.22.0` ships
  `pkg/oci/testdata/test-links/linkToRoot -> .` plus parent-loop
  variants). Root cause: `rpm_file::walk_dir` and
  `binary/discover::walk_dir` followed symlinks via
  `path.is_dir()` with no visited-set or depth limit. Per-walker
  hardening now applies a canonicalize-keyed visited-set + max-
  depth backstop to every walker in `mikebom-cli/src/scan_fs/`.
  New `realistic-projects.yml` CI workflow clones knative/func
  per CI run as a regression gate. (Milestone 054; PR #110.)

- **Go source-tree scans now produce a real dependency graph.**
  Pre-053, scanning a freshly-cloned Go project with empty
  `GOMODCACHE` produced an SBOM with **zero `DEPENDS_ON` edges**
  (issue #102). Post-053 it emits a synthetic main-module
  component (CDX `metadata.component`, SPDX
  `primaryPackagePurpose: APPLICATION`) with direct-require edges
  for every `require` in `go.mod`. Closes the parity gap with
  trivy / syft. (Milestone 053; PR #105.)

- **Default scan now emits ALL lifecycle scopes, not just
  Runtime.** Pre-052, the default `mikebom sbom scan` silently
  dropped Development / Build / Test components. Post-052 they
  emit by default with native scope tagging (CDX
  `scope: "excluded"`, SPDX 2.3 `*_DEPENDENCY_OF` typed
  relationships, SPDX 3 `lifecycleScope` on `dependsOn`).
  Consumers wanting the strict pre-052 prod-only view use the
  new `--exclude-scope dev,build,test` flag. (Milestone
  052/part-3; PR #100.)

- **Lifecycle-scope dependency tagging via standards-native
  fields.** The legacy `mikebom:dev-dependency` annotation is
  REMOVED; the dev-vs-build-vs-test distinction now travels via
  each format's standards-defined construct per Constitution
  Principle V (v1.4.0). (Milestone 052/part-2; PR #99.)

### Breaking changes for SBOM consumers

- **CDX `metadata.component.purl` for Go-only scans** shifts
  from synthetic `pkg:generic/<target>@0.0.0` to the real
  `pkg:golang/<module-path>@<version>`.
- **SPDX `documentDescribes` for Go-only scans** targets the
  Go main-module's SPDXID instead of a `SPDXRef-DocumentRoot-*`
  placeholder.
- **CDX `components[]` no longer contains the Go main-module**
  for Go-only scans (it lives in `metadata.component`).
- **`mikebom:dev-dependency` annotation** is gone everywhere —
  consumers filtering on it migrate to CDX
  `components[].scope = "excluded"` / SPDX 2.3
  `*_DEPENDENCY_OF` / SPDX 3 `lifecycleScope` on `dependsOn`.
- **`--include-dev` flag** is now a deprecated parse-and-warn
  no-op shim (will be removed in a future release per #101).
  Use `--exclude-scope dev,build,test` for the strict prod-only
  view.

### Milestones in this release

- **054**: filesystem-walker symlink-loop hang fix +
  realistic-project regression suite (closes #102 — second time;
  PR #110).
- **053**: Go main-module component + direct dependency edges
  (closes #102 — first time; PR #105).
- **052/part-3**: `--exclude-scope` flag + default scope flip +
  `--include-dev` deprecation (PR #100).
- **052/part-2**: native CDX/SPDX 2.3/SPDX 3 lifecycle-scope
  emission + edge rewrite (PR #99).
- **052/part-1**: `LifecycleScope` data model + behavior-
  preserving rename + Constitution Principle V codification
  (PR #98).
- **051**: polyglot dev/test tagging — cargo + gem + maven
  + python + npm dev-dep classification via
  `mikebom:dev-dependency` (legacy, removed in 052/part-2;
  PR #96).
- **050**: `mikebom:not-linked` annotation on Go source-tier
  entries not confirmed by binary BuildInfo + scope hint
  for source-tree-only Go scans (PR #93).

### Follow-ups (open issues)

- **#101**: remove the deprecated `--include-dev` parse-and-warn
  shim once the soak window completes (~3 weeks post-052/part-3).
- **#103**: LICENSE-file detection on the Go main-module
  (currently emits empty `licenses`; C40 role tag preserves
  sbomqs licensing-coverage parity).
- **#104**: per-ecosystem main-modules for npm / cargo / maven /
  pip / gem (Go-only in milestone 053).
- **#108**: migrate every filesystem walker to a single shared
  `safe_walk` helper (milestone 054 kept per-walker patches to
  minimize blast radius before this release).
- **#109**: per-ecosystem expansion of the realistic-project CI
  matrix beyond knative/func (cargo / npm / maven / pip / gem /
  rpm / deb / apk).

## [0.1.0-alpha.9] — 2026-05-01

A small targeted release covering one user-facing fix shipped
since alpha.8 (~1 day later): milestone 049's correction of the
Go source-tree component scope. Resolves an audit-grounded gap
where `mikebom sbom scan --path` on a Go project emitted only
the project's directly-imported modules (collapsing legitimate
transitive prod deps into the dropped-as-test-only bucket).

### Changed

- **Go source-tree scans now emit the full go.sum closure by
  default** (milestone 049). Previously the source-tree filter
  dropped every entry not directly imported by this project's
  non-`_test.go` files, collapsing legitimate transitive prod
  deps (e.g., aws-sdk internals, gin's middleware chain) into
  the test-only bucket. Audit on `apigatewayv2/config` showed
  6 components emitted vs. 55 in trivy / 56 in syft. The new
  default emits every `go.sum` entry as a component (matches
  trivy/syft) and only TAGS the small subset proven test-only
  by source-walking the project's `_test.go` imports. Test-only
  deps carry the existing `mikebom:dev-dependency = true`
  annotation when `--include-dev` is set; default-mode drops
  them (mirrors npm/Poetry/Pipfile semantics). No new flag,
  no new annotation, no new catalog row. CDX + SPDX 2.3 +
  SPDX 3 outputs all carry the new emission via existing
  parity wiring.

  Scope: Go-only. cargo / gem / maven test-tagging extension
  tracked as milestone 050 (see specs/049-go-source-scope/).

## [0.1.0-alpha.8] — 2026-04-30

A small targeted release covering one user-facing feature
shipped since alpha.7 (~1 hour after alpha.7): the
`mikebom:component-role` annotation surfacing
filesystem-position-classified component roles in CDX + SPDX 2.3
+ SPDX 3 outputs. Audit-grounded — addresses 3 false-positive
Maven build-tool JARs surfaced in the alpha.7 polyglot-builder-
image conformance run.

- **Build-tool and language-runtime components are now
  explicitly tagged** in every output format. Maven's own
  internals at `/usr/share/maven/lib/`, JDK system-installed
  JARs at `/usr/lib/jvm/*/lib/`, system Python packages at
  `/usr/lib/python*/site-packages/` and `dist-packages/`, and
  comparable build-tool / language-runtime paths now carry
  `mikebom:component-role = "build-tool"` or
  `mikebom:component-role = "language-runtime"`. Downstream
  consumers (vulnerability scanners, license auditors,
  conformance ground-truths) can filter on the annotation
  without re-implementing the path-heuristic.

### Added

- **`mikebom:component-role` annotation** (048). Components
  whose `evidence.occurrences[]` paths match a curated
  filesystem heuristic now carry a `mikebom:component-role`
  annotation classifying them as `build-tool` (under
  `/usr/share/maven/lib/`, `/usr/share/gradle/lib/`, `/opt/sbt/`)
  or `language-runtime` (under `/usr/lib/jvm/*/lib/`,
  `/usr/lib/node_modules/`, `/usr/lib/python*/site-packages/`,
  `/usr/lib/python*/dist-packages/`). Three-state semantics:
  components without a heuristic match get NO annotation —
  absence does NOT mean "definitely application code", it
  means the heuristic didn't classify. Emitted symmetrically
  across CDX `properties[]`, SPDX 2.3 `packages[].annotations[]`,
  and SPDX 3 top-level `annotations[]` (catalog row C40,
  SymmetricEqual). Lets downstream consumers (vulnerability
  scanners, license auditors, conformance suites) filter
  build-tooling and platform runtime libraries from
  application-deps reporting without mikebom dropping any
  component from the SBOM.

## [0.1.0-alpha.7] — 2026-04-30

A small docs + SPDX-parity release. Two days after alpha.6, with
a focus on closing user-facing gaps surfaced during alpha.6
adoption: SPDX consumers' scope context, README staleness, and
CI flake hardening so the next milestone lands cleanly.

- **SPDX-side document-level scope hint** (047). SPDX 2.3 +
  SPDX 3 outputs now self-describe scope at the document level,
  closing the parity gap with CDX's `metadata.lifecycles[]`.
  Closes the user-reported "is mikebom undercounting?"
  conversational ambiguity by making scope explicit in every
  format.
- **README post-alpha.6 docs refresh** (046). Closes 10 audited
  drift items in user-facing docs: stale version pin,
  `--image-src` flag missing from CLI reference, registry-first
  framing for `--image` (default is now docker-daemon-first),
  internal-milestone-number jargon leaking into user docs,
  `--include-legacy-rpmdb` "deferred" framing for shipped
  behavior.
- **CI test-suite flake hardening** (045 + 044-followon).
  Diagnosed two genuine flake patterns from a 60-run audit:
  macOS-runner perf-test variance (now median-of-5 sampling)
  and a timestamp-race on byte-identity tests (now pinned via
  `MIKEBOM_FIXED_TIMESTAMP` in subprocess-spawning helpers).
  Plus a new gated end-to-end integration test for the
  docker-daemon image source. Test-only — no production
  behavior change.

### Added

- **SPDX-side document-level scope hint** (047). SPDX 2.3
  `creationInfo.comment` and SPDX 3 `SpdxDocument.comment` now
  carry a free-text scope summary naming the scope mode
  (artifact vs manifest, derived from `--include-declared-deps`),
  the observed lifecycle phases (mirroring CDX
  `metadata.lifecycles[]`), and a pointer to the per-component
  `mikebom:sbom-tier` annotation for finer-grained detail. SPDX
  consumers reading metadata-only now get the same scope
  context CDX consumers already had via
  `metadata.lifecycles[]`. CDX output unchanged.
- **README "What kind of SBOM does mikebom emit?" section**
  (047). New top-level section between "Why" and "Install"
  explaining mikebom's two scope axes (document-level
  artifact-vs-manifest mode + per-component lifecycle tier),
  how each format self-describes its scope, and how mikebom's
  default scopes map to industry / NTIA-style terminology — so
  operators comparing component counts to trivy / syft can see
  the question being asked rather than wonder whether mikebom
  is undercounting.
- **End-to-end docker-daemon integration test** (044
  follow-on). Gated on `docker --version` + `docker info`
  succeeding; pulls `alpine:3.19`, runs `mikebom sbom scan
  --image alpine:3.19 --image-src docker`, asserts the SBOM
  was produced via the docker-daemon path and contains ≥5
  components. Skips cleanly on CI lanes without docker
  (macOS-latest).

### Changed

- **README + `docs/user-guide/cli-reference.md` reflect
  post-alpha.6 reality** (046). Status pin updated to alpha.6;
  `--image-src docker,remote` flag documented; `--image`
  description updated to describe docker-daemon-first default;
  `--include-legacy-rpmdb` description rewritten to drop
  "deferred until that code lands" framing for long-shipped
  BDB rpmdb reading; OCI-cache flag rows cross-link to the
  `OCI layer caching` section. Also drops internal milestone
  numbers from user-facing docs (CHANGELOG and design-notes
  retain them as appropriate).

### Fixed

- **macOS perf-test flake** (045). `dual_format_perf` and
  `triple_format_perf` failed intermittently on macos-latest
  CI runners (observed 9.0% / 14.4% / 19.9% reduction vs the
  25% gate, while local distribution sits around 50%). Bumped
  median-of-3 → median-of-5 sampling — cuts the median's
  variance by ≈40% so macOS CPU contention spikes don't push
  the measurement below the gate. CI gate (25%) and spec
  target (30%) unchanged.
- **SPDX byte-identity test flake** (045). Three byte-identity
  tests (`spdx_3_alias_bytes_are_byte_identical_to_stable_
  identifier`, `scenario_7_alias_bytes_are_byte_identical`,
  `scenario_8_mikebom_no_deprecation_notice_env_suppresses_
  stderr_warning`) compared raw bytes across two sequential
  subprocess invocations. When the two invocations straddled
  a second-boundary, `creationInfo.created` diverged at
  second precision, surfacing as a CI flake on unrelated
  branches. Pinned `MIKEBOM_FIXED_TIMESTAMP` in the two
  subprocess-spawning helpers (the env var was added in
  milestone 011 specifically for this case but the helpers
  weren't using it).

## [0.1.0-alpha.6] — 2026-04-29

A small, focused release: makes `mikebom sbom scan --image <ref>`
behave the way users coming from trivy and syft expect, and
unblocks AWS ECR pulls that were previously failing on a Basic
auth challenge.

- **Docker daemon as a default image source.** When `--image
  <ref>` is an OCI reference, mikebom now checks the local docker
  daemon's cache first and falls back to a registry pull only on
  miss. Matches trivy's `--image-src` and syft's auto-detection
  convention. The new `--image-src docker,remote` flag (default
  in that order) controls the resolution sequence; pass
  `--image-src remote` to force a fresh registry fetch.
- **AWS ECR support for the registry path.** The OCI-pull's
  401-retry now handles `Basic` auth challenges in addition to
  `Bearer`, applying cached `~/.docker/config.json` credentials
  directly. ECR's `aws ecr get-login-password | docker login`
  flow now works end-to-end with `--image-src remote`.

Together these resolve the reported case where an ECR image was
already cached locally and `docker login`'d, but mikebom errored
out with `WWW-Authenticate is not a Bearer challenge: Basic ...`.

### Added

- **`Basic` auth challenge support for the OCI registry pull** (044
  commit 2). The 401-retry path now accepts both `Bearer` (existing
  Docker Hub / GHCR / gcr.io flow) and `Basic` (AWS ECR's flavor)
  `WWW-Authenticate` challenges. For `Basic`, mikebom applies the
  cached docker-config credentials directly on the original request
  — no token-realm round-trip. Resolves the previous
  `WWW-Authenticate is not a Bearer challenge: Basic ...` error
  on `mikebom sbom scan --image <ecr-ref> --image-src remote`. The
  `~/.docker/config.json` lookup is unchanged (already supported
  `auths.<host>.auth`, `credHelpers`, `credsStore` since milestone
  034); only the challenge parser was Bearer-only.
- **Local docker daemon as a default image source** (044 commit 1).
  `mikebom sbom scan --image <ref>` now consults the local docker
  daemon before reaching for a registry pull, matching trivy and
  syft conventions. New `--image-src docker,remote` flag controls
  the source-resolution order; default is `docker,remote`. Force a
  fresh registry fetch with `--image-src remote`. Docker source
  shells out to `docker image inspect` + `docker save`, so the
  user's existing `DOCKER_HOST` / contexts are honored. Resolves
  the case where an ECR image is already cached locally but the
  registry pull is failing (e.g. on a Basic-auth challenge).

## [0.1.0-alpha.5] — 2026-04-29

Cuts a new pre-release covering everything merged since
alpha.3 (the alpha.4 tag was a CHANGELOG-less mechanical
bump). Ships milestones 010, 023–030, and 034–042 together.
Highlights:

- **Container per-file evidence trilogy** (037 → 040 → 041):
  deb, apk, and rpm components all carry populated
  `evidence.occurrences[]` blocks now, plus matching
  upstream-cross-ref checksums (`md5` / `sha1` /
  `rpm_filedigest`) in `additionalContext`.
- **Direct OCI registry image scanning** (034 → 036):
  `mikebom sbom scan --image alpine:3.19` now pulls from
  registries directly, including authenticated private pulls
  via the standard Docker keychain, cross-arch selection via
  `--image-platform`, and SHA-256-content-addressed disk
  caching for fast repeat scans.
- **Distroless / chainguard / Bazel-built minimal-image
  coverage** (037 → 038): the per-package
  `/var/lib/dpkg/status.d/` layout and its `.md5sums`
  companion files are now read; deb minimal images go from
  zero components to a full SBOM with per-file evidence.
- **Mach-O binary identity + codesign + Go VCS metadata**
  (024 → 025 → 030): macOS and Apple-platform binaries now
  emit `LC_UUID`, `LC_RPATH`, codesign identifier / flags /
  team-id, and Go-binary VCS commit-SHA + build-time
  metadata.
- **Maven sidecar Debian layout** (042): in addition to
  Fedora's `/usr/share/maven-poms/`, mikebom now reads
  Debian's `/usr/share/maven-repo/` GAV-tree layout, so
  `lib*-java`-installed JARs surface as
  `pkg:maven/<group>/<artifact>@<version>` PURLs.
- **Two cross-ref-symmetry milestones** (040 US2 and 041)
  bring apk and rpm to parity with deb's longstanding `md5`
  cross-ref carrier on per-file occurrences.

Detailed entries below.

### Added
- **Milestone 042 — Post-041 small follow-ons.** Two unrelated
  legacy-deferral items closed:
  - **US1 (housekeeping)**: dropped a stale comment in
    `binary/predicates.rs:88` that named rpm file-list
    extraction from HeaderBlob `BASENAMES` / `DIRNAMES` /
    `DIRINDEXES` as "deferred to a follow-on milestone." That
    work shipped in milestone 040 US3; the comment now
    accurately credits 040 US3 as the authoritative claim
    source and explains the directory-heuristic's role as a
    defense-in-depth fallback for corrupt / partial rpmdb cases.
  - **US2 (Maven sidecar Debian layout)**: extends
    `maven_sidecar.rs` with a parallel `DebianSidecarIndex`
    that walks `/usr/share/maven-repo/` (the GAV-tree layout
    populated by Debian's `maven-repo-helper` during
    `apt-get install lib*-java`). Debian-shaped Java images
    that previously emerged as `pkg:generic/<filename>` PURLs
    now resolve to `pkg:maven/<group>/<artifact>@<version>` —
    matching the milestone-007 Fedora-side coverage.
    Implementation introduces a small `SidecarIndex` trait so
    `resolve_coords` works generically over either layout.
    Fedora wins on basename collision (FR-005). Alpine
    layouts remain out of scope (Alpine ships no documented
    system-wide maven repo convention).
  - 6 new inline tests for the Debian sidecar reader; 27
    byte-identity goldens regen with zero diff (no fixture
    contains `/usr/share/maven-repo/` content).
- **Milestone 041 — Rpm FILEDIGESTS cross-reference.** Closes
  the milestone-040 Q1 deferral. Every populated rpm
  `evidence.occurrences[]` entry's `additionalContext` JSON-
  string now carries `rpm_filedigest` alongside the existing
  `sha256`, in algorithm-prefixed form (e.g.
  `"sha256:abc..."` for modern rpm packages,
  `"md5:def..."` for legacy ones). The algorithm matches the
  package's `FILEDIGESTALGO` value (or defaults to MD5 when
  absent per the rpm spec). Brings rpm to full cross-ref
  symmetry with deb (`md5`, since milestone 037) and apk
  (`sha1`, since milestone 040 US2).
  Verified end-to-end against `fedora:40`: 6938 of 6966
  total file occurrences carry the cross-ref (99.6%; the
  28 remainder are non-regular files whose `FILEDIGESTS`
  entry is empty by rpm-spec convention). Sample value
  `rpm_filedigest = "sha256:7544bd..."` matches the
  mikebom-observed `sha256` for the same file — the
  integrity-check arrow goes both ways. New
  `rpm_file_digest: Option<String>` field on
  `mikebom_common::resolution::FileOccurrence` (additive,
  `#[serde(default, skip_serializing_if = "Option::is_none")]`).
  No new top-level dependencies. 27-fixture goldens regen
  with zero diff. See `specs/041-rpm-filedigests/spec.md`.
- **Milestone 040 — Package-DB follow-ons (trifecta).** Three
  sequenced follow-on items closing coverage and hygiene gaps
  after milestones 037 / 038 / 039:
  - **US1 (housekeeping)**: dropped a stale "deferred to
    milestone 031.y" framing in `oci_pull/mod.rs::host_oci_arch`
    that named `--image-platform` as deferred. The flag shipped
    in milestone 035 (PR #72); the error message now positively
    references it with an example invocation.
  - **US2 (apk SHA-1 cross-ref)**: extends milestone 039's apk
    per-file evidence with the apk-provided SHA-1 from each `Z:`
    line in `/lib/apk/db/installed`. Surfaced as `sha1` in the
    per-occurrence `additionalContext` JSON-string alongside the
    mikebom-computed `sha256`. Mirrors deb's `md5` cross-ref
    contract from milestone 037. New `ApkFileEntry` struct in
    apk.rs, new optional `apk_sha1: Option<String>` field on
    `mikebom_common::resolution::FileOccurrence` (additive,
    `#[serde(default, skip_serializing_if = "Option::is_none")]`).
    Verified end-to-end against `alpine:3.19`.
  - **US3 (rpm per-file deep-hash)**: completes the OS-package
    per-file-evidence trilogy. rpm-based images (fedora,
    almalinux, rocky, centos:stream, redhat/*) now produce
    populated `evidence.occurrences[]` blocks at parity with
    deb (037/038) and apk (039). New
    `rpm::read_file_lists(rootfs)` exposes the per-package
    file-list map decoded from the rpmdb HeaderBlob's
    `BASENAMES` / `DIRNAMES` / `DIRINDEXES` triple via the
    existing `iter_rpmdb` helper; new `hash_rpm_package_files`
    + `hash_rpm_db_only` mirror the apk pattern; new `is_rpm`
    branch in `scan_fs/mod.rs::read_all`. Verified end-to-end:
    `fedora:40` produces 147 components with 6966 total file
    occurrences (was 0). Per the milestone-040 Q1
    clarification, rpm FILEDIGESTS cross-ref is OUT of scope
    and deferred to a separate follow-on milestone — rpm-side
    `additionalContext` carries SHA-256 only.
  - No new top-level Cargo dependencies. 27 byte-identity
    goldens regen with zero diff (the goldens use
    `--no-deep-hash` so they're insulated from the deep-hash
    path by design).
  - See `specs/040-pkg-db-followups/spec.md`.
- **Milestone 039 — Per-file evidence for apk components
  (alpine + chainguard apko + Wolfi).** Closes the asymmetry
  surfaced during milestone 038's recon (#75): apk-based images
  now produce per-file `evidence.occurrences[]` blocks at the
  same quality as deb-based images. Implementation mirrors the
  dpkg deep-hash path: a new `apk::read_file_lists` extracts
  per-package paths from the `F:` (directory) and `R:` (regular
  file) lines that the apk installed-db carries inline; a new
  `hash_apk_package_files` walks those paths, opens each file,
  and SHA-256s the content (same 256 MB cap as the dpkg path).
  A parallel `--no-deep-hash` fast path
  (`hash_apk_db_only`) hashes the package's stanza bytes
  in-place. Verified end-to-end:
  `alpine:3.19` produces 79 file occurrences across 15
  components (was 0); `cgr.dev/chainguard/static:latest`
  produces 1217 occurrences across 3 components (was 0). 27
  byte-identity goldens regen with zero diff (those goldens use
  `--no-deep-hash` so they're insulated from the deep-hash path
  by design). Apk-side `additionalContext` carries SHA-256 only;
  the apk-provided SHA-1 (`Z:` lines) is a future extension. No
  new top-level dependencies. Closes #75. See
  `specs/039-apk-deep-hash/spec.md`.
- **Milestone 038 — Per-file evidence for distroless /
  Bazel-built minimal-image deb scans.** Closes the deferred
  milestone-037 item: distroless deb images
  (`gcr.io/distroless/*`, rules-distroless, similar Bazel-built
  minimal images) now produce populated
  `evidence.occurrences[]` blocks with per-file paths and
  SHA-256 + MD5 hashes — matching the evidence quality
  full-fat-image scans have produced since the early
  milestones. Implementation: extended
  `file_hashes.rs::read_info_file{,_bytes}` lookup chain to
  fall back to `var/lib/dpkg/status.d/<pkg>.<ext>` after the
  legacy `info/` paths, and synthesized the path list from the
  second column of `<pkg>.md5sums` when `<pkg>.list` is absent.
  Stanzas in this layout legitimately omit the `Status:` field
  (no dpkg daemon manages install state in the image), so a
  relaxed parse path was added that treats the stanza file's
  existence as the installation marker; strict filtering is
  preserved for the legacy `status` file source. Verified
  end-to-end: `gcr.io/distroless/static-debian12:latest` now
  produces 4 components with 938 total file occurrences (was
  0). 27 byte-identity goldens regen with zero diff.
  Out-of-scope concurrent finding: apk per-file evidence is
  empty for both `alpine:3.19` and chainguard apko/wolfi
  images — mikebom's `file_hashes.rs` is dpkg-only. Filed as
  follow-on issue
  [#75](https://github.com/kusari-sandbox/mikebom/issues/75)
  for a future milestone. See
  `specs/038-minimal-image-deep-hash/spec.md`.
- **Milestone 037 — distroless / chainguard / Bazel minimal-image
  dpkg coverage.** mikebom now reads per-package metadata from
  `/var/lib/dpkg/status.d/<pkgname>` files in addition to the
  legacy single-file `/var/lib/dpkg/status`. Closes the
  milestone-031-surfaced gap where mikebom reported 0 deb
  components for `gcr.io/distroless/static-debian12:latest` and
  similar minimal images that ship per-package metadata files
  instead of the monolithic dpkg-daemon-managed `status` file.
  Same coverage syft and trivy already provided. Filtering uses
  parse-success-or-skip so companion files (`<pkg>.md5sums`,
  `.conffiles`, etc.) naturally drop out without breaking on
  package names that contain dots (`python3.11`). When both
  layouts are present (defensive — never seen in practice), the
  `status.d/` source wins. No new dependencies, no SBOM-shape
  changes, no parity-catalog impact. Closes #64. See
  `specs/037-dpkg-status-d/spec.md`.
- **Milestone 036 (031.z) — On-disk cache for pulled OCI image blobs.**
  Repeat-scans of the same image now skip the network fetch and
  read from a SHA-256-content-addressed cache on disk, completing
  in seconds rather than tens of seconds for non-trivial images.
  The cache lives at `$MIKEBOM_OCI_CACHE_DIR` →
  `$XDG_CACHE_HOME/mikebom/oci-layers` →
  `$HOME/Library/Caches/mikebom/oci-layers` (macOS) →
  `$HOME/.cache/mikebom/oci-layers` (fallback). Default size cap
  10 GB with mtime-based LRU eviction; configurable via
  `--oci-cache-size <bytes>` or `MIKEBOM_OCI_CACHE_SIZE=<bytes>`.
  Disable with `--no-oci-cache` or `MIKEBOM_OCI_CACHE=0`. Every
  cache read re-verifies SHA-256 against the digest, so silent
  corruption is detected and recovered (drop entry + re-fetch).
  Atomic-rename writes (tempfile + persist) keep concurrent scans
  safe. Best-effort posture: any IO failure (read-only fs, missing
  $HOME) falls through to network-only behavior; scans complete
  either way. Manifests are NOT cached (floating tags like
  `:latest` need to re-fetch). No new dependencies. Closes #68.
  See `specs/036-oci-layer-cache/spec.md` and the new
  ["OCI layer caching"](docs/user-guide/cli-reference.md#oci-layer-caching)
  section in the user guide.
- **Milestone 035 (031.y) — `--image-platform` CLI flag for cross-arch
  image scans.** New `mikebom sbom scan --image <ref>
  --image-platform linux/<arch>[/<variant>]` selects a specific
  platform from a multi-arch image index instead of auto-resolving
  to `linux/<host-arch>`. Common shapes: `linux/amd64`,
  `linux/arm64`, `linux/arm/v7`, `linux/386`, `linux/ppc64le`,
  `linux/s390x`. The variant segment is honoured for indexes that
  carry it (e.g. arm v6 vs v7 vs arm64 v8). Closes the macOS-arm64
  dev / Linux-x86_64 CI workflow gap that previously required
  `docker pull --platform <X> && docker save` to scan a non-host
  image. Registry-only — passing `--image-platform` alongside a
  pre-extracted tarball errors clearly. Non-`linux` OS values
  reject with an explanation that mikebom's package-DB readers are
  linux-rootfs-shaped. No SBOM-shape changes (the byte-identity
  goldens regen produces zero diff). Closes #67. See
  `specs/035-image-platform-flag/spec.md` and the new flag row in
  `docs/user-guide/cli-reference.md`.
- **Milestone 034 (031.x) — Authenticated OCI registry pulls.**
  `mikebom sbom scan --image <ref>` now supports private registries
  via the standard Docker keychain — the same `~/.docker/config.json`
  (or `$DOCKER_CONFIG/config.json`) that `docker pull` uses. All four
  documented credential sources resolve in Docker's documented
  precedence order: per-registry `credHelpers` > registry-wide
  `credsStore` > direct `auths.<reg>.auth` (base64 user:password) >
  `auths.<reg>.identitytoken`. Credential helpers are invoked as
  subprocesses (`docker-credential-<helper> get`) per the published
  protocol — covers ECR (`docker-credential-ecr-login`), Google
  Artifact Registry (`docker-credential-gcloud`), macOS keychain,
  Windows credential store, GNOME Secret Service, and `pass`. When
  credentials resolve, they're sent as Basic auth on the
  bearer-token realm GET; the resulting bearer token authorizes the
  manifest + blob fetches. Anonymous fallback is preserved: no
  config.json + public image works exactly as it did in milestone
  031. Credentials never leak to stdout, stderr, `--verbose` output,
  or `RUST_LOG=debug` traces — `Credential::Debug` redacts both
  fields and the helper subprocess's stderr is dropped to /dev/null.
  No new top-level dependencies; the
  `no_c_dependencies_in_oci_registry_feature_tree` regression test
  still passes. See `specs/034-authenticated-registry-pulls/spec.md`
  and the new ["Authenticating to private registries"](docs/user-guide/cli-reference.md#authenticating-to-private-registries)
  section in the user guide. Closes #66.
- **Milestone 030 — Mach-O codesign metadata.** Every Mach-O scan
  now extracts three identity-flavored signals from the
  `LC_CODE_SIGNATURE` (cmd `0x1D`) SuperBlob's CodeDirectory blob:
  `mikebom:macho-codesign-identifier` (e.g. `com.apple.ls` —
  universal across Apple-signed binaries),
  `mikebom:macho-codesign-flags` (JSON array decoded from
  `CodeDirectory.flags` — `hardened-runtime`, `library-validation`,
  `adhoc`, etc.; unrecognized bits emit as `unknown-0x<hex>`), and
  `mikebom:macho-codesign-team-id` (10-char Apple Team ID for
  developer-signed binaries; absent for Apple-system signatures
  whose `TeamIdentifier=not set` and for ad-hoc signatures). This
  is what `codesign -dvv` reads. Fat / universal binaries report
  from the first slice (matching milestone 024's convention).
  **Sixth amortization-proof consumer of the milestone-023
  `extra_annotations` bag** (after 023/024/025/028/029 — 026 was
  a coverage-breadth milestone that didn't touch the bag). No new
  crate dependencies. CMS PKCS#7 cert-chain decoding (which would
  extract the leaf-cert subject CN, signing time, intermediate
  cert hashes — requires ASN.1 DER parsing) and entitlements XML
  extraction explicitly deferred to a follow-on milestone (likely
  unified with PE Authenticode, which has the same DER-parsing
  requirement). See `specs/030-macho-codesign-metadata/spec.md`
  and catalog rows C37/C38/C39 in
  `docs/reference/sbom-format-mapping.md`.
- **Milestone 029 — cargo-auditable extraction.** Extracts the
  zlib-compressed JSON manifest from Rust binaries' `.dep-v0` linker
  section ([cargo-auditable](https://github.com/rust-secure-code/cargo-auditable)
  format) and surfaces the full build-time crate dependency closure as
  per-crate `pkg:cargo/<name>@<version>` components with
  `evidence-kind = "cargo-auditable"`, `confidence = "high"`,
  `parent_purl` cross-linking back to the file-level binary, and
  index-based `dependencies` resolved into `depends` edges. The binary
  itself gains a `mikebom:detected-cargo-auditable = true` cross-link
  annotation (Rust analog of milestone 005's `mikebom:detected-go =
  true`). Cargo wrappers in Debian Trixie+, Fedora 40+, Alpine Edge,
  and the official Rust container images auto-enable the embedding —
  so most Rust binaries built in those environments now surface their
  full statically-linked crate closure without source access. Cross-
  format: ELF / Mach-O / PE. Optional bag annotations
  `mikebom:cargo-auditable-source` (non-registry sources) and
  `mikebom:cargo-auditable-kind` (non-runtime kinds) preserve
  manifest detail. **Fifth amortization-proof consumer of the
  milestone-023 `extra_annotations` bag** (after 023/024/025/028 —
  026 was a coverage-breadth milestone that didn't touch the bag).
  No new crate dependencies — `flate2` and `serde_json` were already
  in the workspace. See `specs/029-cargo-auditable-extraction/spec.md`
  and catalog row C36 in `docs/reference/sbom-format-mapping.md`.
- **Milestone 026 — curated version-string scanner expansion (easy-4
  cohort).** Extends `version_strings.rs`'s curated scanner from 7 to
  **11 self-identifying native libraries**. Four new detectors with
  clean self-identifying signatures in the binary's read-only string
  region:
  - **GnuTLS** (`GnuTLS X.Y.Z`) — common in curl-with-GnuTLS, wget,
    GnuPG, GNU-stack tools.
  - **LibreSSL** (`LibreSSL X.Y.Z`) — macOS system tools (system curl
    was LibreSSL-backed for years), OpenBSD-derived utilities.
  - **LLVM** (`LLVM version X.Y.Z`) — strict prefix; bare `LLVM ` is
    too noisy (matches `LLVM ERROR:`, `LLVM IR ...` etc.).
  - **OpenJDK** — two-scheme parser handling both modern JEP 322
    (`21.0.1+12`) and legacy Java 8 (`8u362-b09`).

  Each match emits a `pkg:generic/<library>@<version>` component with
  `mikebom:evidence-kind = "embedded-version-string"` and
  `mikebom:confidence = "heuristic"`, flowing through the existing
  `version_match_to_entry` machinery (no downstream wiring change).
  9 new inline tests cover positive + negative cases per library
  plus a `libressl_distinct_from_openssl` cross-validation test.

  Three additional libraries from the original wishlist (glibc, musl,
  V8) are deferred to a 026.x research-and-attempt follow-on because
  they don't have clean self-identifying strings in `string_region` —
  glibc's `GLIBC_X.Y` lives in the `.gnu.version_r` ELF section, musl
  rarely self-identifies in compiled output, and V8's version strings
  are buried in stack-trace formatting code. Tracked via
  `TODO(milestone-026.x)` in `version_strings.rs` and the
  "Deferred backlog" section of `docs/design-notes.md`. See
  `specs/026-version-string-library-expansion/spec.md`.

  Note: this milestone is **not** a `extra_annotations` bag consumer —
  it produces new components rather than annotations on existing
  components. The bag-amortization streak from 023/024/025/028 stays
  at four; 026 is purely scanner coverage breadth.
- **Milestone 028 — PE binary identity.** Every Windows-binary scan
  now surfaces three identity signals via `object` 0.36's typed PE
  accessors: `mikebom:pe-pdb-id` (the `<guid-hex-lowercase>:<age>`
  pair from the CodeView Type-2 record in `IMAGE_DIRECTORY_ENTRY_DEBUG`
  — the canonical PE binary identity used by Microsoft Symbol Server,
  Mozilla / Chromium symbol stores, WinDbg, drmingw; analog of
  Linux's NT_GNU_BUILD_ID and macOS's LC_UUID), `mikebom:pe-machine`
  (lowercase `IMAGE_FILE_HEADER.Machine` — `amd64` / `i386` /
  `arm64` / `armnt` / `ia64` / `riscv32` / `riscv64` / `unknown`),
  and `mikebom:pe-subsystem` (lowercase
  `IMAGE_OPTIONAL_HEADER.Subsystem` — `console` / `windows-gui` /
  `efi-application` / `native` / etc., with `WINDOWS_CUI` rendering
  as `console` per Microsoft toolchain idiom). PE32 vs PE32+
  bit-width is auto-dispatched by reading
  `IMAGE_OPTIONAL_HEADER.Magic` (`0x10B` vs `0x20B`). With ELF (023)
  and Mach-O (024) already shipping, this completes the binary-
  identity trifecta — every compiled binary mikebom scans now
  carries cross-platform identity in the SBOM. Surfaced via the
  milestone-023 generic annotation bag — the **fourth** amortization-
  proof consumer, with zero churn in `package_db/`, `mikebom-common/`,
  `cli/`, `resolve/`, `generate/`, `elf.rs`, or `macho.rs`. See
  `specs/028-pe-binary-identity/spec.md` and catalog rows
  C33/C34/C35 in `docs/reference/sbom-format-mapping.md`.
- **Milestone 024 — Mach-O binary identity.** Every macOS-binary
  scan now surfaces three identity signals from byte-level Mach-O
  load-command parsing: `mikebom:macho-uuid` (16-byte LC_UUID
  hex-encoded lowercase — the macOS analog of NT_GNU_BUILD_ID; used
  by `dwarfdump`, `xcrun symbolicatecrash`, the macOS crash reporter,
  and every `*.dSYM` bundle for symbol matching),
  `mikebom:macho-rpath` (LC_RPATH paths in declaration order, dedup'd
  — `@executable_path` / `@loader_path` / `@rpath` recorded raw,
  runtime-context-dependent expansion deferred to consumers), and
  `mikebom:macho-min-os` (`<platform>:<version>` shape — e.g.
  `macos:14.0`, `ios:17.5` — preferring `LC_BUILD_VERSION`, falling
  back to `LC_VERSION_MIN_MACOSX` / `LC_VERSION_MIN_IPHONEOS` /
  `LC_VERSION_MIN_TVOS` / `LC_VERSION_MIN_WATCHOS`). Fat / universal
  Mach-O binaries report from the FIRST slice's bytes (per-slice
  arch-divergence is uncommon in practice; consumers needing it can
  fall back to `otool -l <slice>`). SC-002 verified on the macOS CI
  lane: `/bin/ls` scan emits a non-empty 32-lowercase-hex
  `mikebom:macho-uuid` and a non-empty `<platform>:<version>`
  `mikebom:macho-min-os` — both universal on every supported macOS
  version. Surfaced via the milestone-023 generic annotation bag,
  with zero PackageDbEntry-init churn (the bag's amortization
  payoff). 3 atomic commits; see `specs/024-macho-binary-identity/spec.md`
  and catalog rows C30/C31/C32 in `docs/reference/sbom-format-mapping.md`.
- **Milestone 025 — Go BuildInfo VCS metadata.** Every Go-binary scan
  now surfaces the source-tree VCS state recorded at build time. The
  main-module entry (`pkg:golang/<module>@<version>`) gains three new
  annotations across CDX / SPDX 2.3 / SPDX 3:
  `mikebom:go-vcs-revision` (commit SHA from `vcs.revision`),
  `mikebom:go-vcs-time` (RFC 3339 commit timestamp from `vcs.time`),
  `mikebom:go-vcs-modified` (dirty-tree boolean from `vcs.modified`,
  preserved as the literal string `"true"` / `"false"` matching Go's
  wire format). The data was already present in BuildInfo's vers_info
  blob; pre-025 the parser read only the first line (Go version) and
  discarded the rest. Dep modules don't carry VCS info — it's a
  main-module concern. Surfaced via the milestone-023 generic
  annotation bag, with zero PackageDbEntry-init churn or generate/
  plumbing changes (the bag's amortization payoff). 4 atomic commits;
  see `specs/025-go-vcs-metadata/spec.md` and catalog rows C27/C28/C29
  in `docs/reference/sbom-format-mapping.md`.
- **Milestone 023 — ELF binary identity + per-component generic
  annotation bag.** Two cohorts in one milestone. (a) ELF identity:
  every Linux-binary scan now surfaces `NT_GNU_BUILD_ID` (the
  canonical Linux binary-identity hash used by `eu-unstrip`,
  `coredumpctl`, `debuginfod`, `*-dbgsym` packaging), `DT_RPATH` /
  `DT_RUNPATH` (embedded library search paths the dynamic loader
  consults — `$ORIGIN` etc. recorded raw), and `.gnu_debuglink`
  (pointer to the stripped-debug sibling file). Three new annotations
  on the file-level binary component: `mikebom:elf-build-id`,
  `mikebom:elf-runpath`, `mikebom:elf-debuglink`. SC-002 is satisfied
  on Linux CI: `/bin/ls` scan emits a non-empty hex build-id (every
  modern distro stamps build-ids by default). (b) Per-component
  annotation bag: `extra_annotations: BTreeMap<String, Value>` on
  `PackageDbEntry` and `ResolvedComponent` provides a generic per-
  component annotation channel that future per-binary-metadata
  milestones (024 Mach-O LC_UUID, 026 version-string library
  expansion, 027 container layer attribution) can populate without
  per-field schema migration. Determinism is preserved by `BTreeMap`
  iteration order. Catalog rows C24/C25/C26.

- **Milestone 010 — SPDX 2.3 output + OpenVEX sidecar + SPDX 3.0.1
  experimental stub.** SPDX 2.3 JSON is now a peer of CycloneDX across
  all 9 supported ecosystems. A single `mikebom sbom scan` invocation
  can emit both formats from one pass over the target; the new
  `--format` flag accepts a comma-separated list and is repeatable,
  and `--output` accepts either a bare path (single-format, legacy)
  or repeated `<fmt>=<path>` (per-format). Every data element that
  CDX emits has a documented target in SPDX — native field where the
  spec has one, `annotations[]` entry with a `mikebom-annotation/v1`
  JSON envelope for the rest; the full map is at
  `docs/reference/sbom-format-mapping.md`. When a scan produces
  advisory data, SPDX 2.3 emission co-emits a companion OpenVEX 0.2.0
  JSON sidecar referenced from the SPDX document via
  `externalDocumentRefs` with a SHA-256 of the sidecar bytes;
  `--output openvex=<path>` retargets it (legal only alongside an
  SPDX format). A third, opt-in format `spdx-3-json-experimental`
  emits a minimal SPDX 3.0.1 JSON-LD document for npm components —
  clearly labeled `[EXPERIMENTAL]` in `--help`, in error messages,
  and in the document's own `CreationInfo.comment`. Typing bare
  `spdx-3-json` offers a did-you-mean hint. No behavior change for
  users who don't request SPDX output: CycloneDX emission is
  byte-identical to the pre-milestone baseline, guarded by pinned
  golden fixtures and a dedicated regression test.
  See `specs/010-spdx-output-support/spec.md` for the full
  requirement list and `docs/reference/sbom-format-mapping.md` for
  the cross-format data-placement contract.
- **Feature 009 refinement — bytecode-presence gating for Maven
  shade-relocation.** Shade-relocation entries are now emitted only when
  an ancestor's bytecode is verifiably present in the enclosing JAR
  (either at its original group path or at a shade-relocated path whose
  leaf matches a distinctive artifact-id fragment). Apache's
  `maven-dependency-plugin` emits `META-INF/DEPENDENCIES` into any JAR
  it is configured on, not only shade fat-jars, so the pre-gating
  emission path reported ancestors as "present in" JARs whose bytecode
  was never relocated there. New unit + integration tests exercise
  every disposition. See `specs/009-maven-shade-deps/spec.md` FR-002b.

### Changed
- **`oci-registry` Cargo feature is now on by default.** Direct
  registry pulls (`mikebom sbom scan --image alpine:3.19`) work
  out of the box on a stock `cargo install mikebom` — matches
  syft / trivy UX without requiring `--features oci-registry`.
  The post-milestone-032 substrate (`oci-spec` types-only +
  workspace `reqwest 0.12` + mikebom-owned thin HTTP client) is
  small enough + durable enough that the milestone-031 default-off
  framing no longer pays for itself. Users embedding mikebom in a
  context that needs a minimal-deps build can opt out via
  `cargo install mikebom --no-default-features`; the local
  `--path <dir>` and `--image <foo.tar>` paths still work in that
  configuration. The dep-audit guardrail
  (`no_c_dependencies_in_oci_registry_feature_tree` regression
  test) continues to enforce zero new C-bound transitive deps in
  the now-default tree.

### Removed
- **`mikebom sbom compare` subcommand** and the `demos/` directory.
  The head-to-head comparison story is now owned by a separate test
  suite outside this repo; keeping the in-tree version invited drift
  between the two. Any workflow that depended on `sbom compare`
  should move to the external suite.

## [0.1.0-alpha.3] — 2026-04-23

### Added
- **Feature 009 US1 — shade-relocation ancestor emission.** When a JAR
  contains `META-INF/DEPENDENCIES`, mikebom emits one nested
  `pkg:maven/...` component per declared ancestor, nested under the
  enclosing JAR's primary coord and tagged with
  `mikebom:shade-relocation = true`. Ancestor licenses are parsed from
  the adjacent `License:` lines. Classifier-bearing coords preserve
  `?classifier=<value>` in the PURL. Self-references are dropped
  (`com.example:outer` cannot shade itself). Commit `cdf29b0`.
- **Feature 008 US3 — Maven target/-dir path heuristic** for
  suppressing `target/`-staged development artifacts from image scans.
  Commit `701ea50` (#14).
- **Feature 008 US2 — cache-ZIP Go component filter.** Emissions from
  Go module-cache ZIPs are cross-checked against the linked binary's
  `runtime/debug.BuildInfo`, suppressing ZIPs that never made it into
  the shipped binary. Commit `db6fbab` (#13).
- **Feature 007 US1 — Fedora sidecar POM reading.** JARs installed by
  `dnf` that have stripped embedded `META-INF/maven/` metadata now
  fall back to `/usr/share/maven-poms/` sidecar POMs (JPP-prefixed
  and plain). Commit `a06b7ff` (#8).
- **Feature 007 US2+US3 — Go test-scope and main-module filters.**
  go.sum and BuildInfo emissions are filtered against non-`_test.go`
  import closure and against the primary module's self-coord. Commit
  `b06eda8` (#10).
- **Feature 007 US4 — Main-Class executable-JAR self-reference
  suppression.** JARs whose `META-INF/MANIFEST.MF` names a `Main-Class`
  no longer re-emit their own primary coord as a generic-binary
  `pkg:generic/...` entry. Commit `89a334f` (#11).
- **Feature 006 US5 — SBOM enrichment (`mikebom sbom enrich`).**
  RFC 6902 JSON Patch applier with per-patch provenance recorded as
  `mikebom:enrichment-patch[N]` properties on the BOM metadata. Replaces
  a previously stubbed bail.
- **Feature 006 US4 — in-toto policy layouts (`mikebom policy init`
  and `mikebom sbom verify --layout`).** Single-step functionary-keyed
  layouts. Multi-step deferred.
- **Feature 006 US3 — real artifact subjects.** Attestation subjects
  are resolved via a 5-stage resolver (operator override → artifact-dir
  walk → suffix match → magic-byte detect for ELF / Mach-O / PE →
  synthetic fallback).
- **Feature 006 US2 — DSSE signing + verification** via `sigstore-rs`
  0.10 (pinned below 0.13 to stay off `aws-lc-rs` per Constitution
  Principle I). `mikebom sbom verify` replaces the never-shipped `sbom
  validate` stub; exit contract: 0 pass / 1 crypto / 2 envelope /
  3 layout.
- **Feature 006 foundation — DSSE verify MVP + witness-v0.1 emission.**
  `mikebom trace run` emits in-toto statements compatible with
  `go-witness` / `sbomit generate`.
- **ClearlyDefined license enrichment.** Post-scan enricher querying
  `api.clearlydefined.io` for `npm`, `cargo`, `gem`, `pypi`, `maven`,
  `golang` components. CD's `licensed.declared` becomes an
  `acknowledgement: "concluded"` license entry. `--offline` disables.
- **Per-ecosystem manifest hashes.** Maven sidecar hashes
  (`.jar.sha512` > `.sha256` > `.sha1`) and PyPI `requirements.txt
  --hash=alg:hex` flags now thread through to `components[].hashes[]`.
- **`metadata.component` carries synthetic `purl` + `cpe`** for sbomqs
  schema validity (`pkg:generic/<name>@<version>` +
  `cpe:2.3:a:mikebom:<name>:<version>:...`).
- **`--include-legacy-rpmdb` flag** (feature 004 US4) enables reading
  legacy Berkeley-DB `/var/lib/rpm/Packages` on pre-RHEL-8 /
  CentOS-7 / Amazon-Linux-2 rootfs. Off by default; also configurable
  via `MIKEBOM_INCLUDE_LEGACY_RPMDB=1`.

### Changed
- **`mikebom trace` reclassified as experimental.** Primary SBOM
  surface is now `mikebom sbom scan`. Trace-mode output format
  (witness-v0.1 + DSSE envelope) remains stable; the capture pipeline
  itself is opt-in, Linux-only (kernel ≥ 5.8), and adds 2–3× wall-clock
  overhead on syscall-heavy builds. Commit `45da74d`.
- **Artifact vs. manifest SBOM scope** is now explicit.
  `sbom scan --image` defaults to artifact scope (on-disk presence
  required). `sbom scan --path` defaults to manifest scope (declared
  deps included). `--include-declared-deps` is the explicit override.
  Gated in three Maven emission paths: deps.dev graph enricher,
  pom.xml direct-dep loop, and the `.m2` BFS cache-miss branch.
- **Dual-identity Maven coords.** JARs at `/usr/share/java/*` owned by
  an OS package-db reader (RPM / dpkg / apk) now emit both identities:
  the `pkg:rpm/...` NEVRA (for distro CVE feeds) and the
  `pkg:maven/<g>/<a>@<v>` GAV (for Maven Central advisories). The
  Maven coord is tagged `mikebom:co-owned-by = rpm` (or equivalent);
  `archive_sha256` is dropped since the archive bytes belong to the
  owning OS component. Pre-fix, the Maven coord was skipped entirely
  under a claim-based heuristic, which cost 53 polyglot GT matches.
- **CycloneDX 1.6 conformance pass.** `evidence.identity` is now an
  array (single-object form deprecated in 1.5→1.6);
  `evidence.identity[].tools` is no longer emitted (the previous
  payload wasn't `tools` by the spec's definition); `mikebom:
  source-connection-ids` + `mikebom:deps-dev-match` now land on the
  component as properties. License shape emits
  `{"license": {"id": "<SPDX-id>"}}` for simple IDs and
  `{"expression": "..."}` for compound expressions.
- **PURL canonicalization.** Qualifiers are now sorted
  lexicographically per purl-spec. `+` is percent-encoded across
  every ecosystem. RPM `epoch=0` is dropped (semantically equivalent
  to no epoch; `rpm -qa` omits it).
- **Compositions emit both `assemblies` and `dependencies`** for each
  `complete` ecosystem record, plus a dep-completeness composition so
  sbomqs's `comp_with_dependencies` credits the primary component.
- **Primary-dependency fallback.** When the scanned project's root
  entry was filtered out (npm `path_key == ""`, cargo `source = None`)
  mikebom now synthesizes edges from the primary metadata.component to
  every orphan root. Without this, sbomqs reported "no dependency
  graph present" even when transitives were populated.
- **OS-release reader** prefers `<rootfs>/etc/os-release`, falls back
  to `<rootfs>/usr/lib/os-release` — fixes Ubuntu images where
  `/etc/os-release` is a relative symlink that dangles after
  tar-extraction.
- **Binary-scanner version-string scanner gated on
  `skip_file_level_and_linkage`** to suppress claimed-binary
  self-identification (curl reporting libcurl from `/usr/bin/curl`).
  Trade-off: static-library version detection inside claimed binaries
  is lost; see `docs/design-notes.md`.

### Fixed
- **Pre-PR verification gate** (Constitution v1.2.1). CI runs
  `cargo +stable clippy --workspace --all-targets` and
  `cargo +stable test --workspace`; skipping either locally before
  opening a PR now yields a reject cycle. Commit `6ec1cf3` (#9).
- **Cross-source deduplication + scan-target filter.** Resolves
  duplicate emissions when the same coord surfaces via multiple
  readers (e.g. Maven JAR walker + `.m2` cache + deps.dev). Commit
  `5c98ed2` (#3).
- **Go `go.sum` vs. BuildInfo divergence.** `go.sum` emissions are
  filtered against the companion binary's BuildInfo so dev-only
  transitives don't surface as runtime components. Commit `5b38b98`
  (#7).
- **Go component name alignment** across the source-tree and binary
  emission paths. Commit `ffa7d9f` (#6).
- **Maven version-aware artifact-presence gate** (M6). Commit
  `b4a9041` (#5).
- **Fat-jar heuristic gated on `co_owned_by.is_none()`** to avoid
  double-reporting. Commit `cb7f14e` (#4).
- **ELF-note ghost emissions.** Previously unconditional — a claimed
  Fedora binary emitted both `pkg:rpm/fedora/<subpackage>` (from
  rpmdb) and a ghost `pkg:rpm/rpm/<source-package>` (from the ELF
  `.note.package` section). Now gated on
  `skip_file_level_and_linkage`; unclaimed binaries respect a
  precedence `note.distro > os-release ID > hardcoded default`.
  Commit `3e5ab91`.
- **Cargo workspace-root false positive.** Commit `3e5ab91`.
- **`declared-not-cached` components dropped from `components[]` by
  default.** They remain in the dependency graph as references but are
  no longer materialized as standalone components. Commit `7688ddb`.
- **sbom-conformance findings + CDX 1.6 evidence serialization.**
  Commit `3cd55e3`.

## [0.1.0-alpha.2] and earlier

Earlier alpha milestones landed as a bootstrap commit
(`b0f31c1 feat: bootstrap mikebom + milestones 001-005`) and ship the
foundational work below. CHANGELOG entries below are a roll-up, not a
per-release breakdown.

### Feature 005 — PURL & scope alignment
- Distro qualifier shape standardized as `distro=<ID>-<VERSION_ID>`
  (matches packageurl-python reference tests); codename-required
  claims dropped from docs + tests.
- npm internals scoping: image scans include
  `node_modules/npm/node_modules/**`; path scans exclude.
  Always-on; not user-gated.
- RPM version-string normalization for canonical round-trip.

### Feature 004 — RPM binary SBOMs
- Standalone `.rpm` file scanning (feature 004 US1/US2).
- Generic binary reader for ELF / Mach-O / PE: linkage
  (`DT_NEEDED`, `LC_LOAD_DYLIB`, PE `IMPORT`) plus embedded
  version-string scanning for a curated 7-library list
  (OpenSSL / BoringSSL / zlib / SQLite / curl / PCRE / PCRE2).
- Legacy Berkeley-DB rpmdb parsing gated behind
  `--include-legacy-rpmdb` (feature 004 US4). Default-off.

### Feature 003 — multi-ecosystem expansion
- Go source + binary readers (`go.mod`, `go.sum`, module cache,
  `runtime/debug.BuildInfo` inline format).
- RPM rpmdb.sqlite pure-Rust reader (page/record/schema).
- Maven pom.xml parser with `<properties>` + `<dependencyManagement>`
  + BOM import resolution (`EffectivePom`, cycle-guarded memo).
- Cargo v3/v4 lockfile parser; v1/v2 refused.
- Gem `Gemfile.lock` indent-6 parser; `specifications/*.gemspec`
  walker catches Ruby stdlib/default gems invisible to Gemfile.lock.

### Feature 002 — Python + npm
- Python venv `dist-info/METADATA` reader; `poetry.lock`,
  `Pipfile.lock`, `requirements.txt` support with dev/prod
  distinction.
- npm `package-lock.json` v2/v3 + `pnpm-lock.yaml` + `node_modules/`
  tree walker. v1 lockfiles refused.

### Feature 001 — build-trace pipeline (experimental)
- eBPF capture of syscall + network events during a build. Requires
  CAP_BPF + CAP_PERFMON and Linux kernel ≥ 5.8. Produces in-toto
  attestations bound to the build event.

---

[Unreleased]: https://github.com/kusari-sandbox/mikebom/compare/v0.1.0-alpha.3...HEAD
[0.1.0-alpha.3]: https://github.com/kusari-sandbox/mikebom/releases/tag/v0.1.0-alpha.3
