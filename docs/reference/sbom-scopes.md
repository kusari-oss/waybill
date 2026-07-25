# What kind of SBOM does Waybill emit?

A common question when comparing Waybill's component count to
trivy's, syft's, or another scanner's: **are we counting the
same thing?** Often the answer is no — and the gap is a scope
choice, not a bug. Waybill self-describes its scope on every
output so consumers can answer the question by reading the SBOM
rather than reverse-engineering it from the component list.

## Two axes

Waybill uses two orthogonal scope axes:

**1. Document-level scope mode** — the answer to "what set of
things is this SBOM trying to describe?"

| Mode | Meaning | When |
|---|---|---|
| **Artifact** (default for `--image`) | On-disk components only — every emitted component has its bytes physically present in the scanned tree or image. CDX phase aggregation typically shows `operations` (deployed runtime) plus build-time tiers from installed packages. | Scanning a container image or a built artifact — answers "what's actually here right now?" |
| **Manifest** (default for `--path`) | On-disk components plus declared-but-not-on-disk transitives (lockfile-pinned but absent from local caches, deps.dev-resolved, Maven cache-miss BFS). | Scanning a source tree — answers "what would this build pull in?" |

The mode is controlled by `--include-declared-deps` (auto-on for
`--path`, auto-off for `--image`; explicit override available).

**2. Per-component lifecycle tier** — the answer to "where in
the build/deploy lifecycle was this component observed?"
Annotated as `waybill:sbom-tier` on every component, with five
values:

| Tier | Meaning |
|---|---|
| `design` | Declared but not pinned (e.g., `>=1.0` ranges in `requirements.txt`). |
| `source` | Lockfile-pinned, byte-resolvable (e.g., `Cargo.lock`, `package-lock.json`). |
| `build` | Captured during a live build event via eBPF tracing. |
| `deployed` | Installed in the runtime image — dpkg, apk, rpm, populated `node_modules`, populated venv `dist-info`. |
| `analyzed` | Artifact file on disk, identified by filename + content hash. |

## How Waybill self-describes scope in each format

Waybill ships scope information through native fields in every
output format, not as `waybill:`-prefixed extensions, so any
spec-compliant SBOM reader picks it up:

| Format | Document-level scope hint | Per-component tier |
|---|---|---|
| **CycloneDX 1.6** | `metadata.lifecycles[]` (aggregated from per-component tiers, deduplicated, sorted) + `compositions[].aggregate` | `properties[].name = "waybill:sbom-tier"` |
| **SPDX 2.3** | `creationInfo.comment` (free-text scope summary) | `packages[].annotations[]` with `waybill:sbom-tier` |
| **SPDX 3.0.1** | `SpdxDocument.comment` (free-text scope summary) + `software_Sbom.software_sbomType[]` (native enum) | top-level `annotations[]` with `waybill:sbom-tier` |

For the operator-facing **SBOM type** classification (CISA Design /
Source / Build / Analyzed / Deployed / Runtime), per-format `jq`
recipes, the four-column equivalence table, and the
`--sbom-type <type>` operator-assert flag, see
[SBOM types](sbom-types.md).

## Industry / consumer terminology bridge

When operators compare Waybill's count to other scanners, the
delta usually traces back to a different scope choice rather
than a real coverage gap. As a rule of thumb:

- Waybill's `--image` output ≈ NTIA "deployed" SBOM. CDX phase
  `operations` dominates. Tighter than tools that walk a build
  cache (e.g. trivy's `~/.m2/`) but more accurate for "what's
  actually running in this image."
- Waybill's `--path` output ≈ NTIA "build" SBOM. CDX phases
  `pre-build` (lockfile entries) and `build` (eBPF-traced
  events, when applicable) dominate. Closer to a manifest
  view; useful for license compliance and full transitive
  coverage.

For the deeper rationale on why Waybill takes this stance — and
why class-presence verification deliberately prunes Maven shade-
relocation ancestors that *aren't actually in the JAR* — see
[design notes](../design-notes.md)'s "Scope: artifact vs
manifest SBOM" section.

## SBOM interpretation

- [SBOM format mapping](sbom-format-mapping.md)
  — the parity catalog: every `waybill:*` annotation with its
  per-format landing slot + KEEP-NO-NATIVE audit against
  standards-native constructs (per Constitution Principle V).
- [Cross-ecosystem edges](cross-ecosystem-edges.md)
  — consumer guide to the `--experimental-cross-ecosystem-edges`
  flag (m218 / waybill#633) and the three cross-ecosystem-inference
  annotations (`waybill:cross-ecosystem-inference`,
  `-ambiguous`, `-unresolved`).
- [Split modes](split-modes.md)
  — consumer + contributor guide to `--split[=<mode>]` (m219): the
  new `--split=directory` grouping mode + the additive-optional
  `split-manifest.json` `members[]` field + the extensibility
  contract for future grouping strategies.
