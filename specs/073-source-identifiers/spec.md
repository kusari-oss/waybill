# Feature Specification: Identifiers — built-in + user-defined

> **Post-implementation rename note (2026-05-03, before merge)**: the original draft of this spec called the feature "source identifiers" and proposed a single repeatable `--with-source <scheme>:<value>` CLI flag. After implementation review, two changes were made:
>
> 1. **Naming**: "source" anchored on the most-common case (source repos) but the same mechanism handles image / attestation / user-defined identifiers. SPDX 3 already calls these `Element.externalIdentifier[]`. The name was generalized to "identifier" / `mikebom:identifiers` / `docs/reference/identifiers.md`. Milestone-072's `SourceDocumentBinding` is a DIFFERENT concept (binding back to a source-tier SBOM document) and intentionally retains its name.
> 2. **CLI surface**: `--with-source <scheme>:<value>` was visually ambiguous when values contained colons (URL ssh forms, image `@sha256:` digests). Replaced with dedicated flags per built-in scheme + a generic `--id` for user-defined schemes:
>    - `--repo <url>` → `repo:` identifier
>    - `--git-ref <revision>` (with `--repo`) → `git:<repo>#<revision>` identifier (supersedes the bare `repo:`)
>    - `--image-id <ref>` → `image:` identifier (named `--image-id` to avoid colliding with the existing `--image <PATH>` scan-input flag)
>    - `--attestation <iri>` → `attestation:` identifier
>    - `--id <scheme>=<value>` (repeatable) → user-defined-namespace identifier (built-in scheme names are clap-rejected here with a message pointing at the dedicated flag)
>
> All of `--with-source` is gone — there is no compatibility shim. The annotation key changed from `mikebom:source-identifiers` to `mikebom:identifiers`. The internal Rust field `ScanArtifacts::source_identifiers` was renamed to `ScanArtifacts::identifiers`. The doc was renamed from `docs/reference/source-identifiers.md` to `docs/reference/identifiers.md`. Test files in `mikebom-cli/tests/` were renamed from `source_identifiers_*.rs` to `identifiers_*.rs`. The directory `specs/073-source-identifiers/` retains its name (it's a milestone-tracking artifact). Throughout the rest of this spec, treat "source identifier" mentions as referring to "identifier"; references to `--with-source` are a historical artifact of the original draft and have been superseded by the dedicated-flag CLI surface.

# Feature Specification: Source identifiers — built-in + user-defined

**Feature Branch**: `073-source-identifiers`
**Created**: 2026-05-05
**Status**: Draft
**Input**: User description: when scanning a source repo (or a build, or an image), the operator wants to attach stable, human-meaningful identifiers to the emitted SBOM so that downstream consumers — including milestone-072's cross-tier binding mechanisms and the future multi-source `--bind-to-source` flag — can refer to the SBOM by identity, not just by file path or content hash. Mikebom should auto-detect a built-in identifier when scanning a git checkout (`repo:git@github.com:foo/bar.git` from the origin remote) and accept manual `--with-source <scheme>:<value>` flags for both built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) and user-defined opaque schemes (`acme_corp_id:abc123`). Built-in schemes are validated; user-defined schemes are passed through verbatim. Standards-native carriers per format (CDX `metadata.component.externalReferences[]`, SPDX 2.3 `Package.externalRefs[PERSISTENT-ID]`, SPDX 3 `Element.externalIdentifier[]`) carry the identifiers where they fit; a `mikebom:source-identifiers` document-level annotation handles user-defined namespaces with no native fit. The same flag works on source-tier (`mikebom sbom scan --path`), image-tier (`mikebom sbom scan --image`), and build-tier (`mikebom trace`) scans.

## Clarifications

### Session 2026-05-05

- Q: When auto-detecting the `repo:` identifier from a git checkout, which remote does mikebom select if `origin` is absent? → A: Three-step fallback — try `origin` first; fall back to `upstream` (the conventional fork-parent name); fall back to first-listed remote per `git remote` output (alphabetical, since `git remote` lists alphabetically). The chosen remote name is logged in the identifier's comment field for audit.
- Q: Which SPDX 2.3 carrier holds the source identifier? → A: Both — `Package.externalRefs[referenceCategory: PERSISTENT-ID]` on the document's main-module Package (typed primary, what schema-aware consumers like Trivy / syft / sbomqs decode) AND a redundant `Tool: mikebom-<version> ... source: <identifier>` text line under `creationInfo.creators` (free-form fallback for consumers that don't walk to the main-module). The main-module is the SPDX 2.3 closest analog to "this SBOM describes this thing" — guaranteed populated for every supported ecosystem post-milestone 053–070.
- Q: What is the canonical shape of the auto-detected `image:` identifier? → A: Full form `image:<registry>/<name>:<tag>@sha256:<digest>`. All four components emitted when present: registry (provenance pin), name (logical asset), tag (human-meaningful version label), digest (cryptographic content identity, unforgeable). When the image is loaded from a tarball without a registry context, the registry portion is omitted: `image:<name>@sha256:<digest>`. When digest is unavailable (rare — pre-distribution-spec images), the digest portion is omitted: `image:<registry>/<name>:<tag>`. Operators reading the SBOM see the maximally-informative form their input supports.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator scanning a git source tree gets the repo identifier auto-detected (Priority: P1) 🎯 MVP

A developer runs `mikebom sbom scan --path ./my-project` on a project that's a git checkout. Without any extra flags, the emitted SBOM should carry a stable identifier derived from the git origin remote (e.g., `repo:git@github.com:acme/my-project.git`). Downstream tools and operators can then refer to "the SBOM for `repo:git@github.com:acme/my-project.git`" without needing to know the file path on disk.

**Why this priority**: This is the zero-config win. The vast majority of real-world source-tier scans run inside CI or a developer machine on a git checkout — auto-detection means every existing pipeline gets a meaningful identifier with zero workflow change. Without it, the manual `--with-source` flag is the only path and adoption stalls.

**Independent Test**: `cd` into a git checkout with `git remote get-url origin` returning `git@github.com:foo/bar.git`. Run `mikebom sbom scan --path .` against any project type (cargo / go / npm / etc.). Inspect the emitted CDX SBOM. The `metadata.component.externalReferences[]` array MUST contain an entry of `type: "vcs"` with `url: "git@github.com:foo/bar.git"` and a comment naming the auto-detection source (`comment: "auto-detected from git remote origin"`). Same content equivalent in SPDX 2.3 `Package.externalRefs[]` and SPDX 3 `Element.externalIdentifier[]` per Constitution Principle V.

**Acceptance Scenarios**:

1. **Given** a git checkout with `git remote origin` set, **When** mikebom scans the project root, **Then** the emitted SBOM document-level metadata carries an identifier of scheme `repo:` with the origin URL as its value, attached to the format-native cross-document/identifier slot.
2. **Given** a directory that is NOT a git checkout (no `.git/` dir), **When** mikebom scans, **Then** no `repo:` identifier is auto-detected and no error is raised — the SBOM emits without the identifier (no spurious failure).
3. **Given** a git checkout with no `origin` remote configured, **When** mikebom scans, **Then** auto-detection logs a `tracing::info!` "no git origin remote, source identifier auto-detection skipped" and the SBOM emits without the identifier.
4. **Given** a git checkout with multiple remotes configured (e.g., `origin` + `upstream` for a fork), **When** mikebom scans, **Then** the three-step fallback applies (per the 2026-05-05 clarification): `origin` is preferred; if absent, `upstream` is used (the conventional fork-parent name); if neither exists, first-listed remote per `git remote` output (alphabetical) is used. The chosen remote name is recorded in the comment field for transparency.

---

### User Story 2 — Operator manually attaches built-in or user-defined identifiers via `--with-source` (Priority: P1)

A developer running mikebom in a CI pipeline that doesn't have a normal git checkout (e.g., shallow clone, source artifact extracted from a tarball, or a non-git VCS like Mercurial) needs to specify the source identifier explicitly. Or they want to attach an additional identifier that's not auto-detected — a corporate asset ID like `acme_corp_id:svc-alpha-123` or an in-toto attestation IRI like `attestation:https://example.org/att/build-42`. They invoke `mikebom sbom scan --path . --with-source repo:git@github.com:acme/foo.git --with-source acme_corp_id:svc-alpha-123`. Both identifiers land in the emitted SBOM.

**Why this priority**: Same priority as US1 because this is the manual escape hatch — without it, scans outside git checkouts (or scans needing custom identifiers) have no way to attach identity. US1 + US2 together cover both the zero-config and the explicit-config paths that operators encounter in real pipelines.

**Independent Test**: Run `mikebom sbom scan --path . --with-source repo:git@github.com:acme/foo.git --with-source acme_corp_id:svc-alpha-123` (the directory is NOT a git checkout, so auto-detection finds nothing). Inspect the emitted SBOM. Both identifiers MUST appear: the `repo:` identifier in the standards-native VCS-reference slot per format; the `acme_corp_id:` identifier under `mikebom:source-identifiers` because the user-defined scheme has no native carrier per Constitution Principle V's audit rule. Both are document-level. Order is preserved.

**Acceptance Scenarios**:

1. **Given** any source-tier scan invocation with one or more `--with-source <scheme>:<value>` flags, **When** the scan completes, **Then** every supplied identifier appears in the emitted SBOM at document level. Built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) ride standards-native carriers; user-defined schemes ride a `mikebom:source-identifiers` annotation.
2. **Given** a `--with-source` flag with a built-in scheme whose value fails validation (e.g., `--with-source repo:not-a-valid-git-url`), **When** the scan runs, **Then** mikebom logs a `tracing::warn!` naming the validation failure but emits the value verbatim under `mikebom:source-identifiers` (graceful degradation — emit-as-opaque rather than fail the scan).
3. **Given** both auto-detection from a git checkout AND a manual `--with-source repo:<different-url>`, **When** the scan runs, **Then** the manual override wins; the auto-detected identifier is NOT emitted; the manual one IS emitted; an info-level log records which path was used and which was overridden.
4. **Given** multiple `--with-source` flags with the same scheme but different values (e.g., two different `acme_corp_id:` values), **When** the scan runs, **Then** both are emitted (they're distinct identifiers in the same namespace) — no deduplication, no error.

---

### User Story 3 — Same identifier mechanism works on build-tier and image-tier scans (Priority: P2)

The `--with-source` flag and the auto-detection logic apply uniformly to `mikebom trace` (build-tier eBPF scans) and `mikebom sbom scan --image` (image-tier scans), not just source-tier. A build-tier SBOM benefits from the same identifier scheme so downstream consumers can refer to "the build SBOM for `repo:git@github.com:acme/foo.git` at commit X." Image-tier scans gain identifiers like `image:docker.io/foo/bar@sha256:...` that auto-detect from the image reference itself.

**Why this priority**: Lower than US1/US2 because the source-tier path is by far the most common scan invocation. Build-tier and image-tier identifier support is a natural extension once the mechanism exists, but the foundational value is on the source-tier side. US3 ensures the design composes uniformly across tiers — no special-casing.

**Independent Test**: Run `mikebom sbom scan --image docker.io/foo/bar:v1` against a published image. Without any extra flags, the emitted SBOM document-level metadata MUST contain an `image:` identifier auto-detected from the image reference + digest (e.g., `image:docker.io/foo/bar@sha256:abc...`). The `--with-source` flag accepts the same shape on this command. Run `mikebom trace --with-source repo:git@github.com:acme/foo.git -- ./build.sh` (eBPF tracing). The build-tier SBOM emits with the manual identifier attached.

**Acceptance Scenarios**:

1. **Given** an image-tier scan against a registry image, **When** the scan runs, **Then** an `image:` identifier of the form `image:<registry>/<name>:<tag>@sha256:<digest>` (per the 2026-05-05 clarification) is auto-detected from the resolved image reference + digest and emitted at document level. Components missing in the input (no digest, tarball-only, etc.) are omitted; the maximally-informative form supported by the input is emitted.
2. **Given** a build-tier scan with `--with-source repo:...` supplied, **When** the trace completes, **Then** the build-tier SBOM emits with the manual identifier in the same standards-native slot used by source-tier scans.
3. **Given** any tier scan, **When** identifiers are emitted, **Then** the per-format carrier semantics are uniform: CDX uses `metadata.component.externalReferences[]`, SPDX 2.3 uses `Package.externalRefs[PERSISTENT-ID]` (or `creationInfo` for doc-level), SPDX 3 uses `Element.externalIdentifier[]`. No tier-specific carrier divergence.

---

### User Story 4 — Future cross-tier binding can resolve sources by identifier (Priority: P3)

This milestone does NOT implement the resolution path — milestone 074 (multi-source `--bind-to-source`) does. But this milestone's emission MUST produce identifiers in a shape that 074's resolution layer can consume. The contract: every identifier emitted at document level is parseable, schema-stable, and survives JSON canonicalization. A future `mikebom sbom scan --image foo:v1 --bind-to-source repo:git@github.com:acme/svc-a.git` invocation can find the source SBOM by identifier match.

**Why this priority**: Forward-looking design constraint, not a delivered behavior. Listed here so the spec's emission shape is intentional rather than accidental. 074 will exercise this path; 073 just lays the foundation.

**Independent Test**: After this milestone ships, write a small JSON-walker (any language) that loads an emitted source-tier SBOM, extracts every document-level identifier of any scheme, and returns them as a list of `(scheme, value)` pairs. The walker's output MUST be deterministic across runs of the same scan. The walker MUST work without knowledge of the SBOM's filesystem path — only the SBOM bytes.

**Acceptance Scenarios**:

1. **Given** an emitted source-tier SBOM in any format, **When** an external tool walks the document-level metadata extracting identifiers, **Then** the tool returns a stable list of `(scheme, value)` pairs that includes every `--with-source` value plus any auto-detected identifier.
2. **Given** the same source SBOM emitted twice from byte-identical inputs, **When** both emissions are JSON-canonicalized and walked, **Then** the extracted identifier lists are equal (deterministic emission).

---

### Edge Cases

- **Empty `--with-source` value (`--with-source repo:`)**: malformed flag — clap rejects at parse time with a clear error before any scan work begins.
- **Whitespace-only or non-printable scheme prefix**: rejected with a parse-time error. Schemes MUST be `[a-z][a-z0-9_-]*` to maintain consistency with built-in scheme conventions.
- **A built-in scheme used with a value that LOOKS valid but isn't (e.g., `repo:https://malformed`)**: validation runs, fails, mikebom emits a warning, identifier passes through as opaque under `mikebom:source-identifiers`. Operator-explicit > silent rejection.
- **Identifier value containing the literal `:` character (e.g., a URL with a port)**: the value is everything after the FIRST `:`. `--with-source repo:git@github.com:8080/foo.git` parses as scheme `repo`, value `git@github.com:8080/foo.git`. No further splitting.
- **Duplicate identifier (same scheme + same value supplied twice)**: deduplicated. One emit per unique `(scheme, value)` pair.
- **Identifier supplied on a tier where no native carrier exists for that scheme** (e.g., `attestation:` on a SPDX 2.3 source-tier scan): falls back to `mikebom:source-identifiers` annotation per Constitution Principle V's documented-asymmetry path.
- **Auto-detection on a directory that's a git submodule (not a top-level checkout)**: `git remote get-url origin` may return the submodule's parent URL or the submodule's own URL depending on git config. Mikebom uses whatever `git remote get-url origin` returns from the scan-root directory; submodule-specific behavior is out of scope. Documented in the published guide.
- **The user-defined scheme conflicts with a future built-in scheme (e.g., today `acme_corp_id:` is opaque, tomorrow milestone 080 adds a built-in `acme_corp_id:` validator)**: built-in schemes win. Pre-080 SBOMs carrying `acme_corp_id:` under `mikebom:source-identifiers` continue to work; post-080 emissions move the same scheme to its native carrier. Operators are warned at the scheme-registration milestone.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `mikebom sbom scan --path` MUST attempt to auto-detect a `repo:` identifier when the scan root is inside a git checkout. Remote selection follows a three-step fallback (per the 2026-05-05 clarification): (1) `git remote get-url origin`; if that returns no result, (2) `git remote get-url upstream`; if neither exists, (3) first-listed remote per `git remote` output (alphabetical). The chosen remote name MUST be recorded in the standards-native carrier's comment / detail field so operators can audit which remote was selected. Auto-detection failure (no git, no remotes, command error) MUST be a `tracing::info!` log line naming the failure reason — NOT a scan failure.
- **FR-002**: A new `--with-source <scheme>:<value>` flag MUST be available on `mikebom sbom scan` (path and image modes) and `mikebom trace`. The flag MUST be repeatable — passing it N times attaches N identifiers in the order supplied.
- **FR-003**: Built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) MUST be value-validated at scan time. Validation failures emit a `tracing::warn!` and the identifier passes through as opaque under the `mikebom:source-identifiers` annotation. Validation passes route the identifier into the standards-native carrier per format.
- **FR-004**: Schemes MUST match the regex `^[a-z][a-z0-9_-]*$` at the scheme-prefix layer (lowercase ASCII letter start; lowercase letters, digits, underscores, hyphens after). Non-conforming schemes are rejected at clap parse time.
- **FR-005**: Standards-native carriers per format (per Constitution Principle V): CDX `metadata.component.externalReferences[]` with `type` matching scheme (`vcs` for `repo:`/`git:`, `distribution` for `image:`, `attestation` for `attestation:`); SPDX 2.3 carries built-in identifiers in **both** `Package.externalRefs[referenceCategory: PERSISTENT-ID]` on the document's main-module Package (typed primary — schema-aware consumers like Trivy / syft / sbomqs decode this) AND a redundant `Tool: mikebom-<version> ... source: <identifier>` text line under `creationInfo.creators` (free-form fallback for consumers that don't walk to the main-module — per the 2026-05-05 clarification). SPDX 3 carries every identifier in `Element.externalIdentifier[]` (the SPDX 3 multi-identifier model is the perfect fit). User-defined schemes that have no native fit ride a document-level `mikebom:source-identifiers` annotation per Constitution Principle V's documented-asymmetry path.
- **FR-006**: Manual `--with-source` flags MUST override auto-detected identifiers when scheme + auto-detected value differ. The override is logged (`tracing::info!` naming both values). When manual + auto-detected agree, only one identifier is emitted (deduplicated). **Override position rule (per analyze F1 fix):** when a manual entry's `(scheme, value)` matches an auto-detected entry's, the manual entry inherits the auto-detected entry's position in the emitted Vec (front-of-list); the auto-detected entry is dropped without shifting subsequent entries. This preserves byte-identity of the emitted carrier arrays under override semantics. When the manual `(scheme, value)` does NOT match (true override of a different value), the dropped auto-detected entry's position is collapsed away and remaining manual entries follow in their original supply order; the manual override does NOT migrate to the front.
- **FR-007**: The auto-detected `repo:` identifier MUST capture which remote name was consulted (e.g., `origin`, `upstream`) in the comment / detail field of the standards-native carrier, for transparency.
- **FR-008**: The same identifier-emission mechanism MUST apply to source-tier (`scan --path`), image-tier (`scan --image`), and build-tier (`trace`) scans. Image-tier scans MUST auto-detect an `image:` identifier from the resolved image reference + digest. Per the 2026-05-05 clarification, the canonical shape is `image:<registry>/<name>:<tag>@sha256:<digest>` (all four components when available). Tarball-loaded images omit the registry portion (`image:<name>@sha256:<digest>`); pre-distribution-spec images without published digest omit the digest (`image:<registry>/<name>:<tag>`). Build-tier scans accept manual `--with-source` flags with no auto-detection (build context is opaque to mikebom-trace's eBPF observability).
- **FR-009**: Identifier emission MUST be deterministic — byte-identical inputs produce byte-identical identifier output across runs. The emit-side ordering rule (per analyze F1 + FR-006 override rule): (1) auto-detected entries appear FIRST in the emitted Vec, in detection order; (2) manual `--with-source` entries follow, in supply order; (3) when a manual entry deduplicates against an auto-detected entry on `(scheme, value)`, the manual entry inherits the auto-detected position (no shift); (4) when manual entries collide with each other on `(scheme, value)`, the first-supplied wins.
- **FR-010**: A new published reference at `docs/reference/source-identifiers.md` MUST document the built-in schemes, the user-defined-scheme passthrough rules, the per-format carriers, and the determinism contract — same standard as milestone-072's `cross-tier-binding.md` guide. External tools writing SBOM consumers can decode mikebom-emitted identifiers from this doc alone.
- **FR-011**: The cross-format-parity test suite (milestone 071's `holistic_parity.rs`) MUST pass with the new `mikebom:source-identifiers` annotation registered as a parity catalog row. Directionality `SymmetricEqual` (the JSON-encoded payload is byte-identical across CDX / SPDX 2.3 / SPDX 3 envelopes after canonicalization).
- **FR-012**: The byte-identity goldens for source-tier scans run inside git checkouts (the existing fixture set already includes git-tracked directories) MUST be regenerated to incorporate the new auto-detected `repo:` identifier. The existing milestone-072 binding annotations on image-tier goldens are unaffected. No source-tier-emit shape change beyond the additive identifier slot.

### Key Entities

- **Source identifier**: A `(scheme, value)` pair attached at document level to an emitted SBOM. The scheme classifies the identifier kind; the value is the identifier proper.
- **Built-in scheme**: One of `repo:`, `git:`, `image:`, `attestation:` — recognized + validated by mikebom; mapped to a standards-native carrier per format.
- **User-defined scheme**: Any scheme not in the built-in set. Treated as opaque — no validation, no native-carrier mapping. Rides a `mikebom:source-identifiers` document-level annotation.
- **Auto-detected identifier**: An identifier mikebom inferred without an explicit `--with-source` flag. Currently only `repo:` (from git origin remote) and `image:` (from image reference + digest). Always emitted with a comment naming the detection source.
- **Manual identifier**: An identifier supplied via `--with-source <scheme>:<value>`. Overrides auto-detected when the scheme matches.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of source-tier scans run inside a git checkout with an `origin` remote auto-detect a `repo:` identifier without any operator action. Operators don't need to remember a flag for the common case.
- **SC-002**: An external tool walking an emitted SBOM with no knowledge of the original scan's filesystem path can extract every attached identifier as `(scheme, value)` pairs from any of the three formats. The extraction logic is the same regardless of which format the SBOM is in.
- **SC-003**: Operators with non-git source trees (tarballs, Mercurial, shallow clones) can attach a stable `repo:` or other built-in identifier via a single `--with-source <scheme>:<value>` flag. No SBOM post-processing required.
- **SC-004**: User-defined identifier schemes (e.g., `acme_corp_id:abc123`) round-trip through emission + extraction without modification. Operators can attach corporate metadata that survives format conversion.
- **SC-005**: When milestone 074 (multi-source `--bind-to-source`) ships, the resolution layer can consume identifier-keyed input (e.g., `--bind-to-source repo:git@github.com:acme/foo.git`) and resolve to the matching source SBOM file path via the document-level identifier emitted by this milestone. No additional emission-side work needed in 074.
- **SC-006**: The published `docs/reference/source-identifiers.md` reference enables an external SBOM consumer to write a working identifier extractor in any language using only standard JSON tooling and the documented per-format carrier table. Same SC-004-style test as milestone 072.
- **SC-007**: Existing alpha.15 byte-identity goldens for non-git fixtures remain byte-identical (no auto-detection fires when no git remote is present). Alpha.15 goldens for git-tracked fixtures get one additive identifier entry per the FR-012 regen.

## Assumptions

- The four built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) cover the majority of operator needs. Additional schemes (`oci:`, `purl:`, `cve-feed:`, etc.) can be added in future milestones without breaking the user-defined-passthrough mechanism.
- Auto-detection is "best-effort, never failing." If git is missing, the remote isn't configured, or the command errors out, the scan emits without the auto-detected identifier and logs the reason. No scan failure.
- The standards-native carriers — CDX `externalReferences[]`, SPDX 2.3 `Package.externalRefs[]` + `creationInfo`, SPDX 3 `Element.externalIdentifier[]` — are sufficient for the four built-in schemes. SPDX 3's native multi-identifier model is the cleanest fit; CDX and SPDX 2.3 use the more constrained types per FR-005's mapping.
- The `:` separator between scheme and value is the FIRST `:` only. Values may contain additional `:` characters (e.g., URLs with ports, in-toto attestation IRIs). This matches PURL semantics and most URI conventions.
- User-defined schemes are NOT validated at all — mikebom passes them through verbatim. Operators are responsible for picking schemes that don't collide with future built-in schemes.
- Determinism is contractual (FR-009). Future canonicalization changes that break determinism are a versioned migration, not a silent change.
- This milestone does NOT implement the resolution path (identifier → source SBOM file path lookup). That's milestone 074's scope — this milestone just guarantees that 074 can do the lookup against emitted identifiers.
- The `mikebom:source-identifiers` annotation reuses the milestone-071 cross-format-parity infrastructure — adding a catalog row + parity-aware emission, no new mechanism.

## Out of Scope

- **Identifier-keyed `--bind-to-source` resolution** (the operator passes `--bind-to-source repo:git@...` and mikebom finds the source SBOM file). That's milestone 074.
- **A registry / lookup service** mapping identifiers to SBOM file paths or content. Local filesystem scan only when 074 lands; no networked registry, no fetch.
- **Identifier signing or cryptographic verification** of the identifier itself. Identifiers are operator-attested provenance metadata; cryptographic provenance is the in-toto attestation layer's job.
- **Auto-detection beyond git origin remote**: Mercurial, Subversion, fossil, etc. are out of scope for v1. Operators in those VCSes use manual `--with-source`.
- **Identifier mutation post-emission** (e.g., a `mikebom sbom edit-identifier` subcommand). Identifiers are immutable post-emit; operators wanting different identifiers re-run the scan with different flags.
- **Format-specific identifier shapes that don't fit the four built-in schemes** (e.g., a CycloneDX-1.6-only identifier). The four schemes were chosen for cross-format compatibility; format-specific extensions are out of scope.
- **Backfilling identifiers onto pre-073 SBOMs**. New scans on alpha.16+ get identifiers natively; alpha.15 SBOMs stay as-emitted.
- **Validation of the user-defined scheme prefix** beyond the FR-004 regex. No "registered" / "reserved" prefix list — operators pick whatever fits their org's namespace conventions.
