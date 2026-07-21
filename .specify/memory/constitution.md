<!--
  ============================================================
  SYNC IMPACT REPORT
  ============================================================
  Version change: 1.5.0 → 2.0.0
  Bump rationale: MAJOR — project rename mikebom → Waybill
  (milestone 214). The constitution's project-name identity
  and every Principle heading refer to the project by name.
  Per the constitution's own Amendment procedure at the bottom
  of this file — "MAJOR: Principle removed, redefined, or made
  incompatible with prior interpretation." — renaming the
  project the constitution governs qualifies as redefinition.
  Every prior reference to the "mikebom Constitution" is now
  an artifact of the pre-rename name.

  All Principles' NORMATIVE CONTENT is unchanged — this is
  purely an identity update. No principle added, removed, or
  reinterpreted.

  Modified sections:
    - Constitution title: `# mikebom Constitution` → `# Waybill
      Constitution` (case-preserving prose pass earlier converted
      the lowercase form; this bump also capitalizes the title
      for proper-noun consistency).
    - Preamble (added): one-line heritage note preserving the
      naming history.
    - Every Principle body paragraph: `mikebom` → `waybill` in
      identifiers (`mikebom-cli`, `mikebom-common`, `mikebom-ebpf`,
      `mikebom_common::`, `mikebom trace`, `mikebom scan`, etc.);
      `Mikebom` → `Waybill` in prose where the project is referred
      to by name.
    - Historical SYNC IMPACT REPORT text (kept below in the log):
      the case-preserving prose sweep rewrote `mikebom:*` → `waybill:*`
      in the historical bump-descriptions. Some may argue that
      historical descriptions should preserve pre-rename terminology
      for accuracy; the decision here was to prioritize consistency
      in the constitution's living-document identity. Historical
      accuracy is preserved via `git blame` + spec docs at
      specs/001-*..213-*.
    - Version field: 1.5.0 → 2.0.0.
    - Last Amended field: 2026-06-20 → 2026-07-21.

  Added sections: heritage preamble sentence under the title.
  Removed sections: none.

  Templates requiring updates:
    - .specify/templates/plan-template.md          ✅ no update needed
    - .specify/templates/spec-template.md          ✅ no update needed
    - .specify/templates/tasks-template.md         ✅ no update needed
    - .specify/templates/agent-file-template.md    ✅ no update needed
    - .specify/templates/checklist-template.md     ✅ no update needed
    - CLAUDE.md                                    ✅ project name updated
                                                    via prose pass
    - README.md                                    ✅ project name +
                                                    heritage sentence
                                                    updated via prose pass

  Follow-up TODOs: none. m214 rename is the sole bump content.

  ============================================================
  PRIOR SYNC IMPACT HISTORY (preserved below verbatim)
  ============================================================

  Version change: 1.4.0 → 1.5.0
  Bump rationale: MINOR — new Strict Boundary §5 codifying
  "file-tier emission MUST NOT introduce duplicate components
  in default mode; the `--file-inventory=full` flag is an
  explicit override; full-mode SBOMs MUST carry a document-
  level `waybill:file-inventory-mode` annotation so consumers
  can detect the override at parse time". Principle VIII
  (Completeness) clarification: "unattributed content — files
  surviving all package-DB, binary-tier, and fingerprint
  readers — counts toward Completeness when surfaced as
  file-tier components per the orphan-fallback contract."
  Prompted by milestone 133 (File-tier component emission) —
  the Completeness 1★ vs 5★ gap surfaced during milestone
  132's audit-baseline measurement; milestone 133 is the
  structural response.

  Modified sections:
    - Principle VIII (Completeness): new clarification para
      on unattributed-content surfacing.
    - Strict Boundaries: new §5 covering the no-duplicate-
      in-default-mode rule + the full-mode override marker
      requirement.

  Added sections: Strict Boundary §5
  Removed sections: none

  Previous SYNC IMPACT history:
    - 1.3.1 → 1.4.0: MINOR — Principle V (Specification Compliance)
      gains a new normative bullet codifying "standards-native
      fields take precedence over `waybill:`-prefixed properties".
      Every spec proposing a new `waybill:*` property, annotation,
      or relationship type MUST first audit the target formats for
      an existing native construct carrying the same semantic, and
      reviewers MUST reject specs that don't. Prompted by milestone
      052 (lifecycle-dep-scope), where the alpha.9
      `waybill:dev-dependency` annotation was found to reinvent
      CDX `scope` + SPDX 2.3 `DEV/BUILD/TEST_DEPENDENCY_OF` + SPDX
      3 `LifecycleScopeType` — all three formats had the native
      field, waybill had silently used a custom property.
    - 1.3.0 → 1.3.1: PATCH — pre-PR Verification table updated to
      reflect the post-milestone-016 zero-warnings baseline. The
      clippy invocation now carries `-- -D warnings`; the passing
      condition becomes "Zero errors and zero warnings." A new
      paragraph immediately after the table clarifies the
      deliberate divergence with the Build & Test Commands
      quick-reference at line 346 (`--all-targets --all-features`
      for thorough local linting vs. `--workspace --all-targets`
      matching CI exactly) so future contributors don't mistake
      the two for redundant copies.
    - 1.2.1 → 1.3.0: MINOR — Principle V (Specification Compliance)
      materially expanded. The SPDX bullet now permits both SPDX 2.3
      and SPDX 3.x (instead of pinning SPDX 3.1, which is currently
      rc1). Adds a new normative requirement: experimental / opt-in
      SPDX 3 emitters MUST be visibly labeled in CLI help, output
      filename, and document creator metadata. Prompted by milestone
      010 (SPDX Output Support).
    - 1.2.0 → 1.2.1: PATCH — codified pre-PR verification.
    - 1.1.0 → 1.2.0: MINOR — new principle XII (External Data
      Source Enrichment); principle II + strict boundary #1
      amended to distinguish discovery from enrichment.

  Templates requiring updates:
    - .specify/templates/plan-template.md        ✅ no update needed
    - .specify/templates/spec-template.md         ✅ no update needed
    - .specify/templates/tasks-template.md        ✅ no update needed
    - .specify/templates/agent-file-template.md   ✅ no update needed
    - .specify/templates/checklist-template.md    ✅ no update needed
    - .specify/templates/commands/               ✅ directory empty

  Follow-up TODOs: none
  ============================================================
-->

# Waybill Constitution

> **Waybill was previously known as Mikebom.** Historical spec docs at `specs/001-*/`..`specs/213-*/` retain the original `mikebom` terminology as authorship artifacts; that pre-rename vocabulary in past artifacts is preserved by convention, but every functional identifier in current source + emitted output uses the new `waybill` name (m214 rename, v0.1.0-alpha.66+).

## Core Principles

### I. Pure Rust, Zero C

All code — kernel-space eBPF programs and user-space application
alike — MUST be written exclusively in Rust. The `aya` framework
provides the eBPF toolchain. No C source files, no `libbpf`
bindings, and no C compiler toolchains are permitted in the build
pipeline.

**Rationale**: A single-language stack eliminates FFI bugs,
guarantees memory safety across the entire call graph, and
removes the C toolchain as a supply-chain attack surface —
critical for a tool whose purpose is supply-chain integrity.

### II. eBPF-Only Observation

All dependency **discovery** MUST occur through eBPF tracing
of live build processes. Network interception uses `uprobes`
attached to TLS libraries (OpenSSL, GoTLS) to capture
plaintext before encryption. File operations are traced via
kernel probes. No MITM proxy, no certificate injection, and
no static manifest/lockfile parsing are permitted **as a
dependency source**.

External data sources (lockfiles, databases, APIs) MAY be
used to **enrich** already-discovered dependencies — for
example, adding dependency-tree relationships, license data,
or vulnerability context — per Principle XII. A component
that appears only in an external source but was NOT observed
in the eBPF trace MUST NOT be added to the SBOM.

**Rationale**: Observing the actual build eliminates the gap
between what a manifest declares and what a build actually
fetches. Enrichment from external sources adds value without
compromising the trace-first trust model, provided the
distinction between "observed" and "enriched" is maintained.

### III. Fail Closed

If the eBPF trace fails to attach, loses events, or observes
zero dependency activity, waybill MUST report the failure
transparently and exit with a non-zero status. The tool MUST
NOT fall back to static analysis, lockfile parsing, or any
heuristic gap-filling.

**Rationale**: An SBOM that silently omits dependencies is
worse than no SBOM. Failing closed forces operators to
investigate and fix tracing problems rather than ship
incomplete attestations.

### IV. Type-Driven Correctness

Domain values — cryptographic hashes, Package URLs (PURLs),
SPDX license expressions, CycloneDX component identifiers —
MUST be represented as dedicated newtype structs or enums.
Raw `String` types MUST NOT be passed across function
boundaries for these values. Production code MUST NOT call
`.unwrap()`; use `anyhow` for application errors and
`thiserror` for library error definitions.

**Rationale**: The Rust type system can enforce specification
formats at compile time. A `Purl(String)` wrapper prevents a
hash from being accidentally used where a PURL is expected,
eliminating an entire class of serialization bugs.

### V. Specification Compliance

Generated SBOMs MUST strictly conform to:

- **CISA 2025 Minimum Elements** — all required fields
  populated, including "Tool Name" as `waybill` and
  "Generation Context" reflecting active build-time trace.
- **CycloneDX 1.6** — valid JSON or XML serialization via
  `cyclonedx-bom` or the `sbom-rs` ecosystem.
- **SPDX 2.3 and SPDX 3.x** — when SPDX output is requested.
  Output MUST conform to the SPDX 2.3 JSON schema for the
  stable `spdx-2.3-json` format, and to the targeted SPDX
  3.x JSON schema (currently 3.0.1; subsequent 3.x minors
  may be adopted in follow-up milestones) for any SPDX 3
  emitter. Experimental or opt-in SPDX 3 emitters MUST be
  visibly labeled as such — in CLI `--help` text, in the
  output filename, and in the produced document's
  creator/tool metadata — so that consumers cannot mistake
  them for production-grade output.
- **PURL Specification** — every Package URL emitted MUST
  conform to the PURL spec. Invalid PURLs MUST NOT appear
  in output.
- **Standards-native fields take precedence over `waybill:`-
  prefixed properties.** Before introducing any new
  `waybill:*` property, annotation, or relationship type,
  every spec MUST audit each target format for an existing
  native construct that carries the same semantic. If one
  exists, waybill MUST use the native construct as the
  primary signal; a `waybill:*` property is permitted ONLY
  to carry finer-grained information the standard does not
  express, or to bridge a parity gap when one format has the
  native field but another doesn't (in which case the
  parity-bridging `waybill:*` annotation MUST be documented
  in `docs/reference/sbom-format-mapping.md` with a
  justification clause naming the missing native field).
  Spec authors MUST cite the audit result in the spec's
  Functional Requirements; reviewers MUST reject specs that
  introduce a `waybill:*` field without it.

Conformance applies to the SBOM envelope and to every
sub-element within it. Non-compliant output at any level is
a blocking bug.

**Rationale**: waybill exists to produce legally and
technically defensible SBOMs. A spec-conformant document
containing malformed PURLs is still non-compliant.
Sub-element validity is as critical as envelope validity.
SPDX 2.3 remains the dominant deployed SPDX version across
the SBOM consumer ecosystem — federal procurement pipelines,
sbomqs, syft/grype/trivy interop, and the LF SPDX tools
validator all expect 2.3 today. SPDX 3.x (currently 3.0.1
stable, 3.1-rc1 in flight) is the forward path. Permitting
both lets waybill serve current adopters without locking
out future ones; the experimental-labeling requirement
preserves consumer trust during the transition.

The standards-native-precedence requirement keeps waybill
output interoperable with every SBOM-aware tool, not just
waybill-aware ones — and prevents the catalog from
accumulating `waybill:*` annotations that reinvent
constructs the format already provides. Milestone 049's
`waybill:dev-dependency` annotation (later removed by
milestone 052 in favor of CDX `scope`, SPDX 2.3
`DEV/BUILD/TEST_DEPENDENCY_OF`, and SPDX 3
`LifecycleScopeType`) is the canonical motivating case.

### VI. Three-Crate Architecture

The Cargo workspace MUST contain exactly three crates:

- `waybill-ebpf/` — `no_std` eBPF programs for the kernel.
- `waybill-common/` — shared struct definitions (ring buffer
  event payloads) used by both kernel and user space.
- `waybill-cli/` — user-space application: eBPF loader,
  event processor, API client, SBOM serializer.

Additional crates require explicit justification and a
constitution amendment.

**Rationale**: The `aya` framework requires this separation
between `no_std` kernel code and `std` user code. A shared
crate prevents struct definition drift. Keeping it to three
crates enforces simplicity and prevents premature
modularization.

### VII. Test Isolation

Unit tests MUST cover all PURL parsing, `deps.dev` API
response handling, and CycloneDX/SPDX serialization logic.
These tests MUST run without elevated privileges in standard
CI environments using mock eBPF event generators.

Integration tests that load eBPF programs MUST be gated
behind `root` or `CAP_BPF` privilege checks and MUST be
isolated from unit test suites so that `cargo test` succeeds
in unprivileged environments.

**Rationale**: eBPF requires kernel privileges that most CI
runners lack. Separating privilege-dependent tests from pure
logic tests ensures the fast feedback loop remains usable
while still exercising the full stack when privileges are
available.

### VIII. Completeness

waybill MUST minimize false negatives — dependencies that
were actually fetched during a build but are absent from the
generated SBOM. Every network request and file-read event
observed by the eBPF trace MUST be processed and represented
in the output unless explicitly filtered by a user-specified
exclusion rule.

**Unattributed content also counts toward Completeness**
(added in 1.5.0 per milestone 133). Files surviving every
package-DB reader, every binary-tier reader, and every
fingerprint matcher MUST be surfaced as file-tier components
in the emitted SBOM under the orphan-fallback contract
(`--file-inventory=orphan`, the post-milestone-133 default).
The content-shape allowlist documented in
`docs/reference/component-tiers.md` keeps the orphan output
signal-dense; the file-tier components themselves carry no
PURL and identify content by SHA-256 + observed paths. An
operator who explicitly opts out via `--file-inventory=off`
accepts the resulting false-negative surface.

When completeness cannot be guaranteed (e.g., ring buffer
overflow, partial trace window), the tool MUST signal the
gap per Principle X (Transparency).

**Rationale**: An SBOM that omits real dependencies creates a
false sense of security. Consumers making vulnerability or
license decisions based on an incomplete SBOM inherit
unquantified risk. Unattributed content (custom binaries,
vendored libraries with no manifest, embedded archives) is
the long-tail completeness gap that milestone 132's
audit-baseline measurement surfaced; milestone 133's
file-tier emission closes it without inventing a PURL where
none exists.

### IX. Accuracy

waybill MUST minimize false positives — components listed in
the SBOM that were not actually used by the traced build.
PURL resolution against `deps.dev` or `PurlDB` MUST be
validated before inclusion: ambiguous or low-confidence
matches MUST be flagged rather than silently included as
definitive.

**Rationale**: An SBOM bloated with phantom dependencies
erodes consumer trust, triggers spurious vulnerability
alerts, and increases audit burden. Accuracy preserves the
signal-to-noise ratio that makes SBOMs actionable.

### X. Transparency

When waybill cannot guarantee completeness (Principle VIII)
or accuracy (Principle IX), it MUST include structured
metadata in the SBOM output that informs the consumer of
the limitation. Examples:

- Ring buffer overflow detected → metadata indicating
  potential event loss during a time window.
- PURL resolved via heuristic rather than exact hash match
  → confidence annotation on the affected component.
- Build not directly traced (future inference mode) →
  generation context MUST state that data is inferred, not
  observed.

Transparency metadata MUST use spec-native mechanisms
(e.g., CycloneDX `confidence`, `evidence`, or `property`
fields) rather than ad-hoc extensions where possible.

**Rationale**: Consumers cannot act on data they cannot
assess. Transparent metadata allows downstream tooling and
human reviewers to make informed risk decisions rather than
treating all SBOM entries as equally authoritative.

### XI. Enrichment

waybill SHOULD enrich SBOM output with supplementary data
beyond the minimum dependency graph when the data is
available from upstream sources and can be attached without
violating accuracy (Principle IX). Enrichment targets
include:

- **License data** — resolved from `deps.dev`, registry
  metadata, or package-embedded license files.
- **VEX (Vulnerability Exploitability eXchange)** — when
  vulnerability context is available for a component.
- **Supplier and author metadata** — when provided by the
  package registry.
- **Hash digests** — multiple algorithms (SHA-256, SHA-512)
  for content verification.

Enrichment MUST NOT delay SBOM generation to the point of
failure. If an enrichment source is unavailable, the SBOM
MUST still be emitted with the enrichment fields omitted
and a transparency annotation (Principle X) noting the gap.

**Rationale**: A bare dependency list satisfies minimum
compliance but leaves consumers to independently research
licenses, vulnerabilities, and provenance. Enrichment
collapses that effort into the SBOM itself, increasing its
utility as a single source of truth.

### XII. External Data Source Enrichment

External data sources — including lockfiles, package
registries, hash-to-package databases, and vulnerability
databases — MAY be used to **enrich** eBPF-traced
dependencies with supplementary data. Permitted enrichment
includes:

- **Dependency relationships** — lockfiles (Cargo.lock,
  package-lock.json, go.sum, etc.) MAY be read to add
  dependency-tree edges (e.g., `DEPENDS_ON` relationships)
  between components that were observed in the eBPF trace.
- **Package identity** — hash-to-PURL databases (deps.dev,
  PurlDB) MAY be queried to resolve content hashes to
  package identifiers.
- **Metadata** — license data, supplier info, vulnerability
  context, and provenance data MAY be fetched from any
  available source.

The following constraints apply:

1. External sources MUST NOT introduce new components. A
   package that appears in a lockfile but was NOT observed
   in the eBPF trace MUST NOT be added to the SBOM.
2. Data from external sources MUST be annotated with its
   provenance (e.g., "relationship from Cargo.lock",
   "license from deps.dev") per Principle X (Transparency).
3. External source unavailability MUST NOT prevent SBOM
   generation. The tool MUST degrade gracefully with
   transparency annotations noting missing enrichment.
4. The eBPF trace remains the authoritative source for
   dependency discovery. External sources provide context,
   not authority.

**Rationale**: The eBPF trace tells us *what was fetched*.
Lockfiles and databases tell us *how those fetches relate to
each other* and *what we know about them*. Combining both
produces SBOMs with the dependency trees that downstream
tools expect, without compromising the trace-first trust
model. This closes the dependency-tree gap with tools like
syft and trivy while maintaining waybill's core advantage
of build-time observation.

## Strict Boundaries

These constraints are non-negotiable and MUST NOT be
circumvented by feature flags, configuration options, or
optional modes:

1. **No lockfile-based dependency discovery.** Lockfiles and
   manifests MUST NOT be used as a source of dependency
   discovery. If the eBPF trace produces no data, the tool
   fails closed (Principle III). Lockfiles MAY be read for
   enrichment purposes only (dependency relationships,
   metadata) per Principle XII — but MUST NOT introduce
   components not observed in the trace.

2. **No MITM proxy.** All network observation MUST remain in
   eBPF `uprobes`. Certificate injection, proxy servers, and
   traffic interception outside eBPF are forbidden.

3. **No C code.** Not in the main codebase, not in build
   scripts, not in vendored dependencies. The `aya` crate
   provides all kernel compatibility (Principle I).

4. **No `.unwrap()` in production.** Test code may use
   `.unwrap()` for brevity; production code MUST use proper
   error propagation (Principle IV).

5. **No file-tier duplicates in default mode.** File-tier
   emission (milestone 133) MUST NOT introduce duplicate
   components in the default `--file-inventory=orphan` mode.
   The FR-011 hybrid dedupe (path coverage from package-tier
   components' `evidence.occurrences[].location` field OR hash
   coverage from binary-tier components' `hashes[]` SHA-256
   entries) MUST suppress every file already claimed by a
   package or binary reader. The `--file-inventory=full` flag
   is an explicit override that bypasses the dedupe; full-mode
   SBOMs MUST carry a document-level
   `waybill:file-inventory-mode = "full"` annotation
   (CycloneDX `metadata.properties[]`, SPDX 2.3 / SPDX 3
   document-scope `Annotation`) so consumers can detect the
   override at parse time and filter the file-tier set when
   the duplication is unwanted. Added 1.5.0 per milestone
   133.

## Development Workflow

### Build & Test Commands

| Action | Command |
|--------|---------|
| Build eBPF kernel program | `cargo xtask ebpf` |
| Build user-space application | `cargo build --release` |
| Lint | `cargo clippy --all-targets --all-features -- -D warnings` |
| Format check | `cargo fmt -- --check` |
| Unit tests | `cargo test --workspace` |
| Run (requires root) | `sudo RUST_LOG=info target/release/waybill scan --target-pid <PID>` |

### Pre-PR Verification (MANDATORY)

Before opening or updating ANY pull request, the author MUST run both
of the following commands locally and confirm each passes clean — not
one, not a subset, BOTH:

| Step | Command | Passing condition |
|------|---------|-------------------|
| 1 | `cargo +stable clippy --workspace --all-targets -- -D warnings` | Zero errors and zero warnings |
| 2 | `cargo +stable test --workspace` | Every suite reports `ok. N passed; 0 failed` |

These are the exact commands CI executes (`.github/workflows/ci.yml`).
Note that the Build & Test Commands quick-reference at line 346 above
documents a related-but-not-identical clippy invocation
(`cargo clippy --all-targets --all-features -- -D warnings`) intended
for thorough local linting that exercises feature-gated code. Both
must pass for a PR to merge cleanly; the flag-set difference
(`--all-features` vs `--workspace`) is intentional. Do not
"harmonize" the two commands without updating both this table and the
quick-reference together.
`cargo test -p <crate>` alone is INSUFFICIENT because it skips clippy
and skips cross-crate targets. Specifically, the `clippy::unwrap_used`
deny at the `waybill-cli` crate root (Principle IV) is enforced by
clippy's `--all-targets` inside `#[cfg(test)]` modules too; any test
module using `.unwrap()` MUST be guarded with
`#[cfg_attr(test, allow(clippy::unwrap_used))]` on the `mod tests`
item, matching the convention used throughout `waybill-cli/src/trace/`.

A PR that has not passed both commands locally MUST NOT be opened or
pushed for review. A passing per-crate `cargo test` is not evidence of
CI-readiness and MUST NOT be cited as such in the PR description.

### Async Runtime

The `tokio` async runtime MUST be used for:

- Reading from the eBPF ring buffer (`BPF_MAP_TYPE_RINGBUF`).
- Querying `deps.dev` and `PurlDB` APIs for PURL resolution.
- Concurrent event processing.

### eBPF Specifics

- Attach programs to `cgroup` v2 for process isolation.
- Use `BPF_MAP_TYPE_BLOOM_FILTER` for in-kernel event
  deduplication.
- Use `BPF_MAP_TYPE_RINGBUF` (not perf buffer) for
  kernel-to-user data delivery.

## Governance

This constitution is the authoritative source of
non-negotiable project constraints. It supersedes informal
conventions, PR comments, and ad-hoc decisions.

**Amendment procedure**:

1. Propose the change in a dedicated PR with a clear
   rationale.
2. Update this document and increment the version per
   semantic versioning:
   - MAJOR: Principle removed, redefined, or made
     incompatible with prior interpretation.
   - MINOR: New principle or section added, or existing
     guidance materially expanded.
   - PATCH: Wording clarification, typo fix, or
     non-semantic refinement.
3. All active plans and specs MUST be reviewed for
   consistency with the amended constitution before merge.

**Compliance**: Every PR and code review MUST verify that
changes do not violate any principle. Violations require
either a code fix or a constitution amendment — never silent
deviation.

**Version**: 2.0.0 | **Ratified**: 2026-04-15 | **Last Amended**: 2026-07-21
