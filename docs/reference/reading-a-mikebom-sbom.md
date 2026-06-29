# Reading a mikebom SBOM

**Audience**: anyone reading a mikebom-emitted CycloneDX 1.6 / SPDX 2.3 / SPDX 3.0.1 document — compliance engineers, vulnerability-scanner authors, SBOM-diff tool maintainers, auditors walking output for the first time.

**What this doc is**: a consumer-facing tour of the signals mikebom makes available beyond the CDX/SPDX spec baseline — what they mean, where to find them per format, what to do with them, plus a complete index of every `mikebom:*` annotation key.

**What this doc is NOT**: a complete wire-shape catalog (see [SBOM format mapping](sbom-format-mapping.md) for the 100+ row catalog that the emitters honor — that document is the authoritative wire-shape contract, structured for code-review depth). This guide is the onboarding surface; the catalog is the contract.

---

## Table of contents

1. [Positioning — how mikebom uses spec-native fields](#1-positioning--how-mikebom-uses-spec-native-fields)
2. [How to read this doc](#2-how-to-read-this-doc)
3. [Signals mikebom makes available — by use case](#3-signals-mikebom-makes-available--by-use-case)
   - 3.1 [Vulnerability scanning](#31-vulnerability-scanning)
   - 3.2 [Compliance auditing](#32-compliance-auditing)
   - 3.3 [Build provenance](#33-build-provenance)
   - 3.4 [Transparency / completeness gaps](#34-transparency--completeness-gaps)
4. [The `mikebom-annotation/v1` envelope](#4-the-mikebom-annotationv1-envelope)
5. [Cross-format reading patterns](#5-cross-format-reading-patterns)
6. [Stability](#6-stability)
7. [For tool authors](#7-for-tool-authors)
8. [Cross-references](#8-cross-references)
- [Appendix A — Annotation key index](#appendix-a--annotation-key-index)
- [Appendix B — Milestone-citation map](#appendix-b--milestone-citation-map)

---

## 1. Positioning — how mikebom uses spec-native fields

mikebom strictly conforms to **CycloneDX 1.6**, **SPDX 2.3**, and **SPDX 3.0.1**. Most data in a mikebom-emitted SBOM lives in spec-native fields — PURL, name, version, supplier, licenses, hashes, CPEs, dependency edges. If you've parsed CDX or SPDX before, the bulk of a mikebom SBOM looks familiar.

A small fraction of the data — currently 102 distinct keys at the time of writing — rides on `mikebom:*`-prefixed annotations. These are **parity-bridging annotations** introduced per [Constitution Principle V](../../.specify/memory/constitution.md): every spec proposing a new `mikebom:*` field must first audit the target formats for an existing native construct carrying the same semantic. Annotations are permitted only when (a) no native field exists across all three formats, or (b) one format has the native field but the others don't (parity-gap carve-out). Every `mikebom:*` annotation has a documented audit trail in the [SBOM format mapping](sbom-format-mapping.md) catalog naming the rejected native-field alternatives.

The job of this doc is to tell consumers what each parity-bridging annotation means and how to use it. The framing is consumer-centric ("here's what mikebom emits and how to use it"), not competitive — we don't name specific competing SBOM tools or characterize their omissions. Consumers reading this guide already have an SBOM in hand; the question is "what's in it that I should care about?"

---

## 2. How to read this doc

**Quick navigation by goal**:

- Building a vulnerability scanner or fortifying CVE alerting → [§3.1](#31-vulnerability-scanning)
- License audit or compliance workflow → [§3.2](#32-compliance-auditing)
- Verifying build-time provenance (vs lockfile-derived enrichment) → [§3.3](#33-build-provenance)
- Evaluating completeness or detecting gaps → [§3.4](#34-transparency--completeness-gaps)
- Parsing the annotation envelope as a tool author → [§4](#4-the-mikebom-annotationv1-envelope) + [§7](#7-for-tool-authors)
- Looking up an unfamiliar `mikebom:*` key seen in an SBOM → [Appendix A](#appendix-a--annotation-key-index)
- Mapping a mikebom binary version to which signals are available → [Appendix B](#appendix-b--milestone-citation-map)

**Full wire-shape detail**: every per-signal section links to the corresponding row in [`sbom-format-mapping.md`](sbom-format-mapping.md). That catalog is the contract; this guide is the orientation.

---

## 3. Signals mikebom makes available — by use case

This section covers 12 signals depth-covered across 4 thematic clusters. Each signal follows the same rendering shape: plain-language description, per-format wire location, recommended consumer action, milestone introduced/stabilized, link to the catalog row, and a worked `jq` recipe.

### 3.1 Vulnerability scanning

Vulnerability scanners need to (a) suppress dev/test/build-only deps from production CVE alerting, (b) attribute findings to OCI layers for forensics and remediation prioritization, and (c) flag when the same package identity actually carries divergent content (so identical CVEs don't silently merge across distinct binaries). mikebom emits all three signals.

#### `mikebom:lifecycle-scope`

> **What it is**: the finer-grained dev / build / test / runtime distinction beyond CDX 1.6's 3-value `scope` enum. CDX 1.6's `component.scope` only carries `required`/`optional`/`excluded`; mikebom emits `excluded` for non-runtime deps and this annotation carries the finer split. SPDX 2.3 has native typed relationships (`DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF`) used as the primary signal; this annotation also rides on the target Package so consumers walking only `DEPENDS_ON` still see the distinction. SPDX 3 carries the value natively in `LifecycleScopedRelationship.scope` AND omits the parity-bridging annotation (the native field is sufficient there).
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry `{name: "mikebom:lifecycle-scope", value: "<scope>"}` on the target component. Native `component.scope: "excluded"` set alongside.
> - **SPDX 2.3**: annotation envelope on the target Package — `{schema: "mikebom-annotation/v1", field: "mikebom:lifecycle-scope", value: "<scope>"}`. Plus native typed relationship `DEV_DEPENDENCY_OF` / `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF` reversed-direction edge under `--spdx2-relationship-compat=full` (the default).
> - **SPDX 3**: NOT EMITTED as a per-package annotation. The value lives natively on the dep edge as `LifecycleScopedRelationship.scope` (`development` / `build` / `test`). Runtime edges omit `scope`.
> **What to do with it**: filter your dep-graph walk to suppress non-runtime components from production-only CVE alerting. Common policy: alert on `runtime` (or absent scope, which means runtime) deps only; report dev/test/build hits as informational. Default mikebom scans INCLUDE dev/build/test-scoped components in `components[]`; the operator can drop them at scan time via `--exclude-scope dev,build,test` to produce the strict "what shipped to production" view.
> **Milestone**: 052 — added; 228 — extended to SPDX 2.3 as parity-bridge.
> **Catalog**: [C42](sbom-format-mapping.md#section-c--mikebom-specific-data-preserved-via-fallback)

```jq
# CDX 1.6 — list dev-scoped components by PURL:
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:lifecycle-scope"
             and .value == "development")
    | .purl' your.cdx.json

# SPDX 2.3 — same query via the annotation envelope:
jq -r '.packages[]
       | select(.annotations[]?
                | .comment | fromjson?
                | select(.field == "mikebom:lifecycle-scope" and .value == "development"))
       | .name + " " + .versionInfo' your.spdx.json

# SPDX 3 — read the relationship-scope natively:
jq -r '.["@graph"][]
       | select(.type == "Relationship"
                and .relationshipType == "dependsOn"
                and .scope == "development")
       | .to[]' your.spdx3.json
```

#### `mikebom:layer-digest`

> **What it is**: the OCI layer's compressed-blob `sha256:<hex>` digest (matches the `LayerDigest` semantic from OCI's image manifest — NOT the uncompressed `DiffID`). Tells you which layer in an OCI image first introduced the file backing this component. When a path is written by multiple layers, the LAST writer in the manifest's `Layers[]` array wins (OCI overlay semantics).
> **Where it lives**: emitted ONLY for `--image` scans on every component whose source path resolves to an OCI layer.
> - **CDX 1.6**: `components[].properties[]` entry `{name: "mikebom:layer-digest", value: "sha256:<hex>"}`.
> - **SPDX 2.3**: annotation envelope on the Package.
> - **SPDX 3**: annotation envelope on the `software_Package` element.
> **What to do with it**: when a CVE fires for a component, look up its `mikebom:layer-digest` to find which layer to rebuild / replace / patch. Useful for differential rebuilds (only re-pull the affected layer), for forensics ("which build step introduced this vulnerable dep?"), and for layered remediation policies (e.g., "block deployment if a CVE-vulnerable component lives in the base layer").
> **Milestone**: 133 (US2.2) — added.
> **Catalog**: [C88](sbom-format-mapping.md#section-c--mikebom-specific-data-preserved-via-fallback)

```jq
# CDX — find every component that came from a specific layer digest:
jq --arg layer "sha256:abc123..." '
  .components[]
  | select(.properties[]?
           | .name == "mikebom:layer-digest" and .value == $layer)
  | {purl, name, version}
' your-image.cdx.json
```

#### `mikebom:duplicate-purl-divergent` + `mikebom:purl-collisions-detected`

> **What they are**: when two Cargo `(name, version)` coords collide in a scan but their declared dep sets or hashes diverge, mikebom flags both the per-component (C99) and document-scope (C100) signals. Without these, a vulnerability scanner could silently merge two distinct binaries under one CVE assessment — masking that the actual code differs. The per-component annotation carries a `DivergenceRecord` envelope `{v, purl, reason, paths[], dep_sets_by_path?, hashes_by_path?}` where `reason` is `deps-differ` / `hashes-differ` / `both`. The document-scope summary aggregates every detected collision in one `jq`-queryable surface.
> **Where it lives**:
> - **CDX 1.6**: per-component `components[].properties[]` (C99) + document-scope `metadata.properties[]` (C100).
> - **SPDX 2.3**: per-Package annotation envelope (C99) + document-scope annotation on `SpdxDocument` (C100).
> - **SPDX 3**: per-`software_Package` annotation (C99) + document-scope annotation (C100).
> **What to do with it**: when a CVE fires for `(name, version)`, check whether the component carries `mikebom:duplicate-purl-divergent` — if so, the CVE may not uniformly apply to all instances. Each `paths[]` entry shows where each diverging variant was found. For triage automation: when present, do NOT auto-suppress the CVE for any path; require per-path human review.
> **Milestone**: 134 — added (Cargo only this milestone; detection logic structured for ecosystem-agnostic reuse).
> **Catalog**: [C99](sbom-format-mapping.md) + [C100](sbom-format-mapping.md)

```jq
# CDX — list every divergent collision detected in this scan:
jq '.metadata.properties[]?
    | select(.name == "mikebom:purl-collisions-detected")
    | .value
    | fromjson
    | .collisions[]' your.cdx.json
```

---

### 3.2 Compliance auditing

Compliance auditors need to (a) distinguish operator-asserted license conclusions from external-enrichment-derived ones, (b) account for unattributed content via file-tier components that fill the orphan-coverage gap, and (c) verify that operator overrides don't lose manifest-derived provenance.

#### `mikebom:license-concluded-source`

> **What it is**: identifies WHERE a component's `licenseConcluded` value came from. Initial value: `"operator-asserted"` — set when the operator passed `--conclude-licenses`, formally asserting they have reviewed the declared licenses and accept them as the analyst-verified conclusion. Future values may include `"clearly-defined"`, `"deps-dev"`, etc. for parallel external-enrichment provenance. Emitted ONLY on components whose `licenseConcluded` was populated by this mechanism; pre-existing `NOASSERTION` conclusions stay unannotated.
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry `{name: "mikebom:license-concluded-source", value: "operator-asserted"}`.
> - **SPDX 2.3**: annotation envelope on the Package.
> - **SPDX 3**: annotation envelope on the `software_Package`.
> **What to do with it**: distinguish license conclusions that carry a human-review claim (operator-asserted) from external-enrichment-derived ones. CDX 1.6's `component.licenses[].acknowledgement` enum is `"declared"` / `"concluded"` — it identifies the LICENSE TYPE, not the SOURCE of the conclusion. Use this annotation to filter your compliance dashboard: operator-asserted conclusions carry stronger provenance than auto-derived ones.
> **Milestone**: 132 — added (issue #363).
> **Catalog**: [C98](sbom-format-mapping.md)

```jq
# CDX — find every component with an operator-asserted license conclusion:
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:license-concluded-source")
    | {purl, licenseConcluded: (.licenses[]? | .license.id // .expression)}
' your.cdx.json
```

#### `mikebom:component-tier` (specifically `"file"`)

> **What it is**: marks components emitted by the file-tier walker — files surviving every package-DB reader + every binary-tier reader + every fingerprint matcher. File-tier components carry NO PURL; they identify content by SHA-256 + observed paths instead. This closes the unattributed-content gap that lockfile / package-DB-only scanners miss (Constitution VIII Completeness — vendored libraries with no manifest, custom binaries, embedded archives, etc.). Emitted by default under `--file-inventory=orphan` mode (the post-milestone-133 default); the override mode `--file-inventory=full` adds duplicate file-tier components to package/binary-attributed paths.
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry `{name: "mikebom:component-tier", value: "file"}`. The component itself uses `type: "file"`, omits `purl`, and carries paths via `mikebom:file-paths` (C92).
> - **SPDX 2.3**: annotation envelope on the Package (`filesAnalyzed: false`, no `externalRefs[purl]`).
> - **SPDX 3**: annotation envelope on a `software_File` graph-element (the tier-shape change is the element type swap).
> **What to do with it**: when a compliance auditor walks the dep tree for license attribution, file-tier components represent content with no manifest-declared license. Treat them as "needs human review" or "license: unknown" rather than skipping. Pair with `mikebom:file-paths` to see where each piece of unattributed content was found.
> **Milestone**: 133 (US1.B) — added; 133 (US1.C) — default flip to `orphan` mode.
> **Catalog**: [C91](sbom-format-mapping.md) (+ [C92](sbom-format-mapping.md) for the paths companion). See also [component-tiers.md](component-tiers.md) for the full tier model.

```jq
# CDX — list every file-tier component + its observed paths:
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:component-tier" and .value == "file")
    | {
        name,
        sha256: (.hashes[]? | select(.alg == "SHA-256") | .content),
        paths: (.properties[]? | select(.name == "mikebom:file-paths") | .value | fromjson)
      }
' your.cdx.json
```

#### `mikebom:demoted-from-main-module`

> **What it is**: marks a library-typed component in `components[]` as the manifest-derived main-module that was preserved (rather than dropped per the milestone-077 clean-replacement default) when the operator passed `--root-name` / `--root-version` / `--root-purl` together with the milestone-149 `--preserve-manifest-main-module` opt-in flag. The demoted entry retains its original ecosystem-derived PURL + name + version + license + hashes; the only differences vs the pre-override main-module are: (a) `component.type` changes from `application` → `library`; (b) this annotation is added; (c) the entry appears in `components[]` instead of at `metadata.component`.
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry `{name: "mikebom:demoted-from-main-module", value: "true"}`.
> - **SPDX 2.3**: annotation envelope on the Package.
> - **SPDX 3**: annotation envelope on the `software_Package`, BUT the subject IRI routes to the synth-root rather than the demoted entry's own IRI (per `package_iri_by_purl` aliasing serving milestone-084 relationship re-anchoring). The annotation VALUE is byte-identical across all three formats; only the SPDX 3 SUBJECT differs. Consumers querying by annotation `field` key find the annotation regardless.
> **What to do with it**: for compliance auditors verifying that operator overrides don't lose manifest provenance — surface demoted components separately in your audit dashboard. The demoted entry has its own ecosystem-derived PURL (e.g., `pkg:cargo/foo-internal@0.5.1`) usable for vulnerability lookups; the operator-override root carries the deployment-meaningful identity. Treat them as a paired set.
> **Milestone**: 149 — added (closes issue #151).
> **Catalog**: [C102](sbom-format-mapping.md)

```jq
# CDX — find demoted components + their original manifest identity:
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:demoted-from-main-module" and .value == "true")
    | {purl, name, version}
' your.cdx.json
```

---

### 3.3 Build provenance

Consumers verifying build-time provenance need to (a) distinguish components observed during an actual build vs lockfile-derived enrichment, (b) understand the doc-level generation mode (eBPF trace vs source-tree scan vs image scan), and (c) follow cross-tier bindings from a build SBOM back to its source SBOM.

#### `mikebom:source-type`

> **What it is**: tags each component with its discovery provenance. Common values include `"trace-observed"` (eBPF-observed during a live build trace), `"declared-not-cached"` (declared in a lockfile but mikebom couldn't verify its presence on disk), `"transitive"` (added via transitive lockfile resolution from an observed component), `"package-database"` (read from a system package DB like dpkg/rpm/apk). Strong-vs-weak provenance markers.
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry.
> - **SPDX 2.3**: annotation envelope on the Package.
> - **SPDX 3**: annotation envelope on the `software_Package`.
> **What to do with it**: trace-observed components have stronger ground truth than enrichment-derived ones. For vulnerability scanning, you may want to weight CVEs against trace-observed components more heavily than declared-not-cached ones. For compliance audits, mark non-trace-observed components for additional review (their presence in the SBOM is from secondary signals, not direct observation).
> **Milestone**: 002 — added; refined across milestones 049–055.
> **Catalog**: [C1](sbom-format-mapping.md)

```jq
# CDX — list components grouped by source-type provenance:
jq '[.components[]
     | {purl, source_type: (.properties[]? | select(.name == "mikebom:source-type") | .value)}]
    | group_by(.source_type)
    | map({source_type: .[0].source_type, count: length, examples: [.[0:3][] | .purl]})
' your.cdx.json
```

#### `mikebom:generation-context`

> **What it is**: document-level signal carrying the generation mode metadata for the entire scan — e.g., source-tree scan vs image scan vs build trace. Useful for downstream tooling that processes SBOMs differently per generation context.
> **Where it lives**:
> - **CDX 1.6**: `metadata.properties[]` entry (document-scope).
> - **SPDX 2.3**: document-scope annotation on `SpdxDocument` via `creationInfo.comment` aggregation.
> - **SPDX 3**: document-scope annotation on the `SpdxDocument` element.
> **What to do with it**: parse before walking components — different generation contexts imply different trust models. Build-trace SBOMs have strong observational provenance; static-scan SBOMs have lockfile-derived provenance; image-scan SBOMs have file-system-extracted provenance.
> **Milestone**: 002 — added; refined across milestones 005, 047.
> **Catalog**: search for "generation-context" in [sbom-format-mapping.md](sbom-format-mapping.md)

```jq
# CDX — read the doc-level generation context:
jq '.metadata.properties[]?
    | select(.name == "mikebom:generation-context")
    | .value' your.cdx.json
```

#### `mikebom:source-document-binding`

> **What it is**: cross-tier binding from a build-tier or analyzed-tier SBOM back to its source-tier SBOM via a content hash + optional IRI. Built using `--bind-to-source <path>` at scan time; verified with `mikebom sbom verify-binding`. Enables source ↔ build ↔ deploy correlation across the artifact lifecycle.
> **Where it lives**: rides on spec-native carriers when available — CDX `metadata.component.externalReferences[type:bom]`, SPDX 2.3 `externalDocumentRefs` + `BUILT_FROM` relationship, SPDX 3 `import[]` ExternalMap + `Relationship[built_from]`. The annotation envelope carries the binding hash + identity metadata.
> **What to do with it**: when auditing a deployment, follow the binding back to the source SBOM to verify what code was actually compiled. Useful for supply-chain attestations + SLSA-style provenance correlation.
> **Milestone**: 072 — added.
> **Catalog**: search for "source-document-binding" in [sbom-format-mapping.md](sbom-format-mapping.md). Full design in [cross-tier-binding.md](cross-tier-binding.md).

```jq
# CDX — extract the source-document binding hash:
jq '.metadata.component.externalReferences[]?
    | select(.type == "bom")
    | .url' your-build.cdx.json
```

---

### 3.4 Transparency / completeness gaps

Consumers evaluating SBOM completeness need explicit signals when mikebom couldn't fully resolve the dep graph or when a non-default emission mode is in effect. mikebom is "fail loud" per Constitution Principle X (Transparency) — gaps are surfaced as annotations rather than silently omitted.

#### `mikebom:file-inventory-mode`

> **What it is**: document-scope marker emitted ONLY when the operator passed `--file-inventory=full` (the override mode that bypasses the milestone-133 hybrid dedup, producing file-tier components that may duplicate package/binary-attributed paths). The default `orphan` mode and the byte-identity-preserving `off` mode do NOT emit this marker. Per Constitution Strict Boundary §5, full-mode SBOMs MUST carry this marker so consumers can detect the override at parse time and filter the file-tier set when the duplication is unwanted.
> **Where it lives**:
> - **CDX 1.6**: `metadata.properties[]` entry (document-scope).
> - **SPDX 2.3**: document-scope annotation on `SpdxDocument`.
> - **SPDX 3**: document-scope annotation on the `SpdxDocument` element.
> **What to do with it**: when present, your dep-graph walks will see duplicated entries (the file-tier component AND the package-tier component covering the same file). Either deduplicate by content hash on your side, or filter out file-tier entries (`mikebom:component-tier = "file"`) before counting components.
> **Milestone**: 133 (US4) — added; codified in Constitution Strict Boundary §5.
> **Catalog**: [C97](sbom-format-mapping.md). See also [component-tiers.md](component-tiers.md).

```jq
# CDX — detect full-mode emission:
jq '.metadata.properties[]?
    | select(.name == "mikebom:file-inventory-mode")
    | .value' your.cdx.json
# Output: "full" if the marker is present; null/empty otherwise.
```

#### `mikebom:graph-completeness` + `mikebom:graph-completeness-reason`

> **What they are**: document-scope Go-graph-completeness signal — `Complete` (all observed Go components have full dep edges) or `Partial` (some edges couldn't be resolved). The companion `-reason` annotation enumerates the specific cause classes. Currently Go-only (the only ecosystem where mikebom can detect partial graph state explicitly); other ecosystems either complete or fail closed.
> **Where it lives**:
> - **CDX 1.6**: `metadata.properties[]` entries (document-scope).
> - **SPDX 2.3**: document-scope annotations on `SpdxDocument`.
> - **SPDX 3**: document-scope annotations on the `SpdxDocument` element.
> **What to do with it**: when a Go-bearing SBOM reports `Partial`, surface the gap in your compliance dashboard or vulnerability scanner — the dep-graph view is incomplete. The reason string tells you whether the gap is recoverable (e.g., re-scan with `go.sum` present) or structural (e.g., cgo external refs).
> **Milestone**: 061 — added (closes #119).
> **Catalog**: search for "graph-completeness" in [sbom-format-mapping.md](sbom-format-mapping.md)

```jq
# CDX — check Go-graph completeness for this scan:
jq '.metadata.properties[]?
    | select(.name == "mikebom:graph-completeness" or .name == "mikebom:graph-completeness-reason")
    | {name, value}' your.cdx.json
```

#### `mikebom:peer-edge-targets`

> **What it is**: alphabetically-sorted array of PURL strings naming the peer-driven `dependsOn` edges from a given npm component. npm `peerDependencies` are install-time conventional (npm 7+ auto-installs them) but semantically declarative — different from regular `dependencies`. mikebom emits peer-edges as standard `dependsOn` (matching the npm install reality) AND tags the source component with this annotation so consumers can distinguish install-driven edges from functional-dep edges. Emitted only on npm components with ≥1 resolved peer-driven edge.
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry — annotation VALUE is a JSON-encoded array of PURL strings.
> - **SPDX 2.3**: annotation envelope on the Package — VALUE is a native JSON array (per the milestone-145 envelope-shape).
> - **SPDX 3**: annotation envelope on the `software_Package` — VALUE is a native JSON array.
> **What to do with it**: vulnerability scanners that want the install-only edge view (matching pre-milestone-147 mikebom behavior) can subtract this set from each component's `dependsOn`. License auditors who care about the functional-dep distinction can flag peer-edges separately.
> **Milestone**: 147 — added (closes Trivy-comparison orphan gap on the looker-frontend lockfile: 5 orphans → 0).
> **Catalog**: [C101](sbom-format-mapping.md)

```jq
# CDX — extract peer-edge targets for a specific component:
jq --arg purl "pkg:npm/@react-native-async-storage/async-storage@1.24.0" '
  .components[]
  | select(.purl == $purl)
  | .properties[]?
  | select(.name == "mikebom:peer-edge-targets")
  | .value
  | fromjson
' your.cdx.json
```

---

## 4. The `mikebom-annotation/v1` envelope

SPDX 2.3 and SPDX 3 don't have a native flat-string property carrier the way CycloneDX does (CDX's `properties[].{name, value}` accepts arbitrary string-typed values directly). Instead, mikebom uses a JSON envelope wrapped inside the format's native `Annotation` element. The envelope shape:

```json
{
  "schema": "mikebom-annotation/v1",
  "field": "<mikebom:* annotation key>",
  "value": <payload — string OR JSON value, depending on the key>
}
```

**Per-format examples** for a single signal (`mikebom:lifecycle-scope` = `"development"`):

| Format | Carrier | Wire shape |
|---|---|---|
| CDX 1.6 | `components[i].properties[]` flat string | `{ "name": "mikebom:lifecycle-scope", "value": "development" }` |
| SPDX 2.3 | `packages[i].annotations[].comment` envelope-as-string | `"comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:lifecycle-scope\",\"value\":\"development\"}"` |
| SPDX 3 | `Annotation.statement` envelope-as-string | `"statement": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:lifecycle-scope\",\"value\":\"development\"}"` |

The envelope schema string `mikebom-annotation/v1` is the stability anchor — future envelope evolutions would bump the version (`mikebom-annotation/v2`) and provide a migration window. Consumers can safely match on `schema == "mikebom-annotation/v1"` to identify mikebom-emitted annotations.

**Canonical Rust source references**:
- Encoder: [`mikebom-cli/src/generate/spdx/annotations.rs`](../../mikebom-cli/src/generate/spdx/annotations.rs) — defines `ENVELOPE_SCHEMA_V1` constant + `MikebomAnnotationCommentV1` struct (lines ~31–67).
- Decoder: [`mikebom-cli/src/parity/extractors/common.rs`](../../mikebom-cli/src/parity/extractors/common.rs) — verifies envelope `schema` + extracts `field` + `value` payload (line ~185).

For SPDX 3 annotations: the `statement` field carries the envelope JSON-as-string. The `subject` field points at the SPDX 3 IRI of the element the annotation applies to — for most signals this is the relevant `software_Package` element; for document-scope signals it's the `SpdxDocument` element. **SPDX 3 subject-routing quirk**: a few annotations (notably `mikebom:demoted-from-main-module`) route to the synth-root IRI rather than the demoted entry's own IRI due to `package_iri_by_purl` aliasing serving milestone-084 relationship re-anchoring. The annotation VALUE remains byte-identical across formats; only the SPDX 3 SUBJECT differs. Consumers querying by annotation `field` key find the annotation regardless. See [C102](sbom-format-mapping.md) for the documented divergence.

---

## 5. Cross-format reading patterns

The same `mikebom:*` signal lives in different per-format carriers. Here's a representative cross-format lookup table for 4 depth-covered signals:

| Signal | CDX 1.6 | SPDX 2.3 | SPDX 3 |
|---|---|---|---|
| `mikebom:lifecycle-scope` | per-component `properties[]` flat string + native `component.scope: "excluded"` for non-runtime | per-Package annotation envelope + native typed relationships (`DEV/BUILD/TEST_DEPENDENCY_OF`) | NOT emitted as annotation — native `LifecycleScopedRelationship.scope` on the dep edge |
| `mikebom:layer-digest` | per-component `properties[]` flat string | per-Package annotation envelope | per-`software_Package` annotation envelope |
| `mikebom:source-type` | per-component `properties[]` flat string | per-Package annotation envelope | per-`software_Package` annotation envelope |
| `mikebom:demoted-from-main-module` | per-component `properties[]` flat string | per-Package annotation envelope | per-`software_Package` annotation envelope, BUT subject routes to synth-root IRI (see [§4 subject-routing note](#4-the-mikebom-annotationv1-envelope)) |

For the full per-row wire-shape detail across all 100+ catalog rows, see [`sbom-format-mapping.md`](sbom-format-mapping.md). That doc names the exact JSON Pointer paths, annotation subject rules, and per-format omissions for every signal.

**Common pattern**: when a signal has a spec-native field in one or more formats AND only some formats need the parity-bridge, mikebom emits the native field where available + the annotation only where the native field is absent. This is Constitution Principle V in action — native first, annotation as a parity-bridge. SPDX 3's `LifecycleScopedRelationship.scope` is a representative example (SPDX 3 omits the parity-bridge annotation because the native field is sufficient there).

---

## 6. Stability

**Catalog row numbers are durable identifiers**. Every `mikebom:*` annotation has a row in [`sbom-format-mapping.md`](sbom-format-mapping.md) numbered `C<N>`. The row number is the stable identifier — annotation key names and wire-shapes may evolve through documented migrations, but the row number identifies the signal's continuity across versions.

**The `mikebom-annotation/v1` envelope shape is stable**. Future envelope evolutions would bump the version string (`mikebom-annotation/v2`) and provide a parallel-emission migration window so consumers can support both versions during the transition.

**Opt-in / experimental flags affect which signals are emitted**:

| Flag | Effect on emission |
|---|---|
| `--exclude-scope <dev,build,test>` | Default mikebom scans INCLUDE dev/build/test-scoped components (and emit `mikebom:lifecycle-scope` annotations for non-runtime entries). This flag DROPS the listed scopes from `components[]` — operator-side opt-out for production-only views. |
| `--file-inventory=full` | Bypasses milestone-133 hybrid dedup; emits `mikebom:file-inventory-mode = "full"` as a transparency marker. Default `orphan` mode does not emit this marker. |
| `--file-inventory=off` | Disables file-tier emission entirely. `mikebom:component-tier = "file"` components do not appear. |
| `--preserve-manifest-main-module` | Enables `mikebom:demoted-from-main-module` annotations when used with `--root-name` / `--root-version` / `--root-purl` overrides. |
| `--include-declared-deps` | Affects whether `declared-not-cached` `mikebom:source-type` components appear. |
| `--no-deep-hash` | Skips per-file SHA-256 emission for binary-tier components. Affects whether content-hash signals are present. |
| `--conclude-licenses` | Emits `mikebom:license-concluded-source = "operator-asserted"` on affected components. |

**Versioning**: mikebom releases follow the `v*-alpha.*` tag sequence (currently in alpha; on `v0.1.0-alpha.52` as of milestone 149). Map binary version → signal availability via the milestone citations in [Appendix B](#appendix-b--milestone-citation-map).

**Consumer-side compatibility**: consumers parsing a mikebom SBOM SHOULD treat unknown `mikebom:*` annotation keys as ignorable rather than as errors. The catalog is the canonical source-of-truth for which keys exist at any given mikebom version; consumers encountering an unknown key in real SBOM data should consult [Appendix A](#appendix-a--annotation-key-index) (this guide's snapshot) or the catalog directly.

---

## 7. For tool authors

If you're building a tool that consumes mikebom SBOMs (vulnerability scanner, SBOM-diff tool, compliance dashboard, license auditor, custom remediation engine), this section is a quick integration checklist.

**1. Decide your envelope-parse approach**. Per [§4](#4-the-mikebom-annotationv1-envelope), SPDX 2.3 + SPDX 3 ride the `mikebom-annotation/v1` envelope as a JSON-encoded string inside the format's native `Annotation`. You can either:
- Match on the envelope `schema` field (`"mikebom-annotation/v1"`) to identify mikebom-emitted annotations.
- Match directly on the envelope `field` field (e.g., `"mikebom:lifecycle-scope"`) to filter for a specific signal.

CDX 1.6 has no envelope — the value is a flat string on `properties[].value`. Same `field` semantic; different carrier.

**2. Use the catalog row numbers as the stability anchor**. Per [§6](#6-stability), `C<N>` row numbers in [`sbom-format-mapping.md`](sbom-format-mapping.md) are durable. If you depend on a specific annotation's semantic, reference its C-row number in your tool's docs so future mikebom version migrations remain trackable.

**3. Suggested integration patterns**:

| Goal | Pattern |
|---|---|
| Suppress dev-only deps from production CVE alerting | Filter components by `mikebom:lifecycle-scope` (CDX `properties[]` / SPDX 2.3+3 envelope) OR walk SPDX 2.3 typed `DEV_DEPENDENCY_OF` relationships OR walk SPDX 3 `LifecycleScopedRelationship.scope`. |
| Attribute findings to OCI layers | Walk `mikebom:layer-digest` on every component; pair with the image manifest's `Layers[]` index to find the introducing layer. |
| Flag divergent same-PURL collisions | Walk document-scope `mikebom:purl-collisions-detected` once; per-collision details on each affected component's `mikebom:duplicate-purl-divergent`. |
| Recover orphan / unattributed content | Walk components with `mikebom:component-tier == "file"`; treat as identity-by-SHA-256 (no PURL); pair with `mikebom:file-paths` for path coverage. |
| Detect full-mode duplicate file-tier coverage | Check document-scope `mikebom:file-inventory-mode == "full"`; if present, deduplicate file-tier vs package-tier components by content hash. |
| Filter npm peer-edges separately | Walk `mikebom:peer-edge-targets` to extract install-driven edges; subtract from `dependsOn` for the functional-dep view. |
| Verify build-time vs lockfile-derived provenance | Walk `mikebom:source-type`; weight `trace-observed` higher than `declared-not-cached` for CVE risk scoring. |
| Cross-tier source ↔ build correlation | Follow `mikebom:source-document-binding` from build SBOM back to source SBOM; verify with `mikebom sbom verify-binding`. |

**4. Report unexpected behavior**: if you encounter a signal that's not in the catalog, an annotation that doesn't match the documented envelope shape, or a behavior that contradicts this guide — file an issue at <https://github.com/kusari-oss/mikebom/issues> with the mikebom binary version + a minimal repro SBOM.

---

## 8. Cross-references

| Doc | Purpose |
|---|---|
| [SBOM format mapping](sbom-format-mapping.md) | **The catalog.** Authoritative wire-shape contract — every emitted data element has a row naming its CDX 1.6 / SPDX 2.3 / SPDX 3 location. Code-review depth. |
| [Identifiers](identifiers.md) | The four-layer identity model (`repo:` / `git:` / `image:` / `attestation:` / user-defined identifiers) — covers `mikebom:identifiers`, the `--repo` / `--git-ref` / `--image` / `--attestation` / `--id` CLI surface, and per-flag identity behavior. |
| [SBOM types](sbom-types.md) | CISA SBOM Type signaling (Design / Source / Build / Analyzed / Deployed / Runtime), the four-column equivalence table, and the `--sbom-type` flag. Per-format mapping for `metadata.lifecycles[]` / `creationInfo.comment` / `software_Sbom.software_sbomType[]`. |
| [Component tiers](component-tiers.md) | The package-tier vs binary-tier vs file-tier model — content-shape allowlist, full-mode override, behavior across `--file-inventory` modes. |
| [Cross-tier binding](cross-tier-binding.md) | The `--bind-to-source` flag, `mikebom sbom verify-binding` CLI, the binding-hash-v1 algorithm, and source ↔ build ↔ deploy correlation patterns. |
| [Conformance harness guide](conformance-harness-guide.md) | Per-format envelope-decode rules + the 7 inherent format-spec asymmetries (the structural reasons certain cross-format comparisons require canonicalization). |
| [Ecosystems](../ecosystems.md) | Per-ecosystem coverage matrix for all supported ecosystems — what mikebom emits for each ecosystem, source-format detection rules, transitive-dep resolution patterns. |
| [Changelog](../../CHANGELOG.md) | Milestone-by-milestone release history. Each released mikebom version maps to a milestone range; cross-reference with [Appendix B](#appendix-b--milestone-citation-map) to determine signal availability per binary version. |

---

## Appendix A — Annotation key index

Snapshot at milestone-150 publication time (98 unique keys across the catalog). Future annotations land in [`sbom-format-mapping.md`](sbom-format-mapping.md) only; this index is best-effort current. When you encounter an unfamiliar `mikebom:*` key not listed here, search the catalog directly — its row contains the full wire-shape contract.

Alphabetical order. Each entry links to the FIRST catalog row that mentions the key (some keys appear on multiple rows — e.g., per-component + document-scope variants — follow the linked row for cross-references to siblings).

- **`mikebom:also-detected-via`** — when the deduplicator merges entries from multiple readers, lists the alternative reader sources for the surviving component. ([C56](sbom-format-mapping.md))
- **`mikebom:assembly-version-informational`** — for .NET assemblies, the raw `AssemblyInformationalVersion` value extracted from the binary. ([catalog](sbom-format-mapping.md))
- **`mikebom:assembly-version-informational-stripped`** — for .NET assemblies, the `AssemblyInformationalVersion` stripped of its build-metadata SemVer suffix (everything after `+`) for cross-tool comparison. ([C87](sbom-format-mapping.md))
- **`mikebom:assertion-conflict`** — milestone-119 supplement-CDX merge: signals when a supplement file's assertion conflicts with auto-detected scan data. ([C67](sbom-format-mapping.md))
- **`mikebom:bazel-archive-name`** — for Bazel-emitted components, the archive filename used in the BUILD/WORKSPACE rule. ([C54](sbom-format-mapping.md))
- **`mikebom:bbappend-applied`** — Yocto: signals that a `.bbappend` recipe overlay was applied during build. ([C76](sbom-format-mapping.md))
- **`mikebom:binary-class`** — binary-tier classification (`elf-executable` / `elf-shared-library` / `pe-executable` / `mach-o-binary` / etc.). ([C10](sbom-format-mapping.md))
- **`mikebom:binary-packed`** — boolean signaling the binary file was detected as packed/compressed (UPX, similar). ([C15](sbom-format-mapping.md))
- **`mikebom:binary-stripped`** — boolean signaling the binary has had its debug symbols stripped. ([C11](sbom-format-mapping.md))
- **`mikebom:build-inclusion`** — milestone-112 Go build-inclusion classification (`tracked` / `not-tracked` / `unknown`). ([C60](sbom-format-mapping.md))
- **`mikebom:build-inclusion-derivation`** — milestone-112 derivation reason for the build-inclusion classification above. ([C61](sbom-format-mapping.md))
- **`mikebom:build-reference`** — links a build-tier component to its build-system reference (Bazel target, Maven coord, etc.). ([C57](sbom-format-mapping.md))
- **`mikebom:buildinfo-status`** — Debian buildinfo-file presence + verification status (`present` / `absent` / `partial`). ([C13](sbom-format-mapping.md))
- **`mikebom:co-owned-by`** — multi-source component co-ownership marker (e.g., Maven coord owned by both `groupA:artifact` and `groupB:artifact`). ([C7](sbom-format-mapping.md))
- **`mikebom:component-role`** — milestone-127 role tag (`main-module` / `service` / etc.) for root-selection logic. Internal — filtered before emission. ([C40](sbom-format-mapping.md))
- **`mikebom:component-tier`** — package-tier vs binary-tier vs file-tier classification. See [§3.2](#32-compliance-auditing) for depth coverage. ([C91](sbom-format-mapping.md))
- **`mikebom:confidence`** — resolution-confidence score (0.0–1.0) for components identified via fuzzy matching. ([C16](sbom-format-mapping.md))
- **`mikebom:cpe-candidates`** — when a component has multiple unresolved CPE candidates, the full set rides this annotation (resolved candidates ride native `externalRefs[cpe23Type]`). ([C19](sbom-format-mapping.md))
- **`mikebom:demoted-from-main-module`** — milestone-149 marker for library entries preserved from the manifest-derived main-module after `--root-name` override + `--preserve-manifest-main-module` opt-in. See [§3.2](#32-compliance-auditing) for depth coverage. ([C102](sbom-format-mapping.md))
- **`mikebom:depends-unresolved`** — for components with declared deps mikebom couldn't resolve, lists the unresolved dep names. ([C77](sbom-format-mapping.md))
- **`mikebom:deps-dev-match`** — deps.dev enrichment match record (system + name + version triple). ([C3](sbom-format-mapping.md))
- **`mikebom:detected-cargo-auditable`** — boolean signaling the binary embeds cargo-auditable JSON manifest data. ([C36](sbom-format-mapping.md))
- **`mikebom:detected-go`** — Go-binary detection metadata (Go module path + version extracted from `runtime/debug.BuildInfo`). ([C14](sbom-format-mapping.md))
- **`mikebom:download-url`** — recipe-declared download URL (Yocto / Bitbake `SRC_URI` variant). ([C53](sbom-format-mapping.md))
- **`mikebom:duplicate-purl-divergent`** — milestone-134: per-component flag for divergent same-PURL collisions. See [§3.1](#31-vulnerability-scanning) for depth coverage. ([C99](sbom-format-mapping.md))
- **`mikebom:elf-build-id`** — ELF binary's GNU build-ID hex string (from `.note.gnu.build-id` section). ([C24](sbom-format-mapping.md))
- **`mikebom:elf-compiler-stamps`** — compiler-version strings extracted from ELF `.comment` / `.note.gnu` sections. ([C49](sbom-format-mapping.md))
- **`mikebom:elf-debuglink`** — ELF `.gnu_debuglink` reference (path to debug-symbol companion file). ([C26](sbom-format-mapping.md))
- **`mikebom:elf-runpath`** — ELF `DT_RPATH` / `DT_RUNPATH` colon-separated runpath entries. ([C25](sbom-format-mapping.md))
- **`mikebom:evidence-kind`** — classification of evidence quality (`direct-observation` / `inference` / `enrichment`). ([C4](sbom-format-mapping.md))
- **`mikebom:exclude-path`** — milestone-113 transparency annotation listing exclude-path patterns that suppressed file emission. ([C63](sbom-format-mapping.md))
- **`mikebom:file-inventory-mode`** — milestone-133 marker emitted only when `--file-inventory=full` (override mode). See [§3.4](#34-transparency--completeness-gaps) for depth coverage. ([C97](sbom-format-mapping.md))
- **`mikebom:file-inventory-skipped-oversize`** — milestone-133 transparency counter for files skipped during the file-tier walk due to size cap. ([C93](sbom-format-mapping.md))
- **`mikebom:file-inventory-skipped-special-files`** — milestone-133 transparency counter for special-file skips during the file-tier walk. ([C94](sbom-format-mapping.md))
- **`mikebom:file-inventory-unreadable`** — milestone-133 transparency counter for unreadable-file skips during the file-tier walk. ([C95](sbom-format-mapping.md))
- **`mikebom:file-paths`** — milestone-133 file-tier component's path coverage list (every rootfs-relative path where the SHA-256 content was observed). ([C92](sbom-format-mapping.md))
- **`mikebom:file-paths-truncated`** — boolean signaling the `mikebom:file-paths` array was truncated to the cap; some paths were dropped. ([C96](sbom-format-mapping.md))
- **`mikebom:fingerprint-confidence`** — milestone-108 fingerprint-matcher confidence score (0.0–1.0). ([C59](sbom-format-mapping.md))
- **`mikebom:fingerprint-corpus-sha`** — milestone-108 corpus identity SHA used for fingerprint matching (provenance). ([C58](sbom-format-mapping.md))
- **`mikebom:generation-context`** — document-scope generation-mode metadata. See [§3.3](#33-build-provenance) for depth coverage. ([C21](sbom-format-mapping.md))
- **`mikebom:go-vcs-modified`** — Go binary's `vcs.modified` flag from `runtime/debug.BuildInfo`. ([C29](sbom-format-mapping.md))
- **`mikebom:go-vcs-revision`** — Go binary's `vcs.revision` (commit SHA) from `runtime/debug.BuildInfo`. ([C27](sbom-format-mapping.md))
- **`mikebom:go-vcs-time`** — Go binary's `vcs.time` (commit timestamp) from `runtime/debug.BuildInfo`. ([C28](sbom-format-mapping.md))
- **`mikebom:graph-completeness`** — milestone-061 document-scope signal for Go-graph completeness (`Complete` / `Partial`). See [§3.4](#34-transparency--completeness-gaps) for depth coverage. ([C44](sbom-format-mapping.md))
- **`mikebom:graph-completeness-reason`** — milestone-061 companion enumerating the reason class for `Partial` Go-graph completeness. ([C44](sbom-format-mapping.md))
- **`mikebom:identifiers`** — milestone-073 user-defined identifier envelope (`<scheme>:<value>` records beyond the built-in `repo:` / `git:` / `image:` / `attestation:` schemes). See [identifiers.md](identifiers.md). ([C47](sbom-format-mapping.md))
- **`mikebom:kmp-source-set`** — milestone-122 Kotlin Multiplatform source-set names that declared each dep. ([C68](sbom-format-mapping.md))
- **`mikebom:layer-digest`** — milestone-133 OCI layer-digest attribution. See [§3.1](#31-vulnerability-scanning) for depth coverage. ([C88](sbom-format-mapping.md))
- **`mikebom:license-concluded-source`** — milestone-132 provenance marker for license conclusions. See [§3.2](#32-compliance-auditing) for depth coverage. ([C98](sbom-format-mapping.md))
- **`mikebom:lifecycle-scope`** — milestone-052 finer-grained dev/build/test/runtime scope distinction. See [§3.1](#31-vulnerability-scanning) for depth coverage. ([C42](sbom-format-mapping.md))
- **`mikebom:lifecycle-scope-derivation`** — milestone-062 reason-class for why a particular lifecycle-scope was chosen for a component. ([C62](sbom-format-mapping.md))
- **`mikebom:linkage-kind`** — binary linkage classification (`statically-linked` / `dynamically-linked` / `cgo-import` / etc.). ([C12](sbom-format-mapping.md))
- **`mikebom:macho-build-tools`** — Mach-O `LC_BUILD_VERSION` build-tools entries (clang/swift version + similar). ([C51](sbom-format-mapping.md))
- **`mikebom:macho-build-version`** — Mach-O `LC_BUILD_VERSION` SDK + min-OS values. ([C50](sbom-format-mapping.md))
- **`mikebom:macho-codesign-flags`** — Mach-O code-signature flags bitmask (hex). ([C38](sbom-format-mapping.md))
- **`mikebom:macho-codesign-identifier`** — Mach-O code-signature identifier string. ([C37](sbom-format-mapping.md))
- **`mikebom:macho-codesign-team-id`** — Mach-O code-signature team identifier (Apple developer team). ([C39](sbom-format-mapping.md))
- **`mikebom:macho-min-os`** — Mach-O `LC_VERSION_MIN_*` minimum-OS version. ([C32](sbom-format-mapping.md))
- **`mikebom:macho-rpath`** — Mach-O `LC_RPATH` runpath entries. ([C31](sbom-format-mapping.md))
- **`mikebom:macho-uuid`** — Mach-O `LC_UUID` binary identifier. ([C30](sbom-format-mapping.md))
- **`mikebom:not-linked`** — milestone-050 marker for Go components present in source but never linked into the binary output. ([C41](sbom-format-mapping.md))
- **`mikebom:npm-role`** — npm-specific component role (e.g., `workspace-root` / `peer-dep`). ([C9](sbom-format-mapping.md))
- **`mikebom:orphan-reason`** — when a component appears as a graph orphan (no inbound edges), enumerates the reason class. ([C45](sbom-format-mapping.md))
- **`mikebom:os-release-missing-fields`** — document-scope transparency: lists `/etc/os-release` fields missing during distro detection. ([C22](sbom-format-mapping.md))
- **`mikebom:pe-linker-version`** — PE binary's linker-version major/minor (from `IMAGE_OPTIONAL_HEADER`). ([C52](sbom-format-mapping.md))
- **`mikebom:pe-machine`** — PE binary's machine architecture (x86 / x64 / ARM64). ([C34](sbom-format-mapping.md))
- **`mikebom:pe-pdb-id`** — PE binary's PDB GUID + age (for symbol-file correlation). ([C33](sbom-format-mapping.md))
- **`mikebom:pe-subsystem`** — PE binary's subsystem value (CLI / GUI / driver). ([C35](sbom-format-mapping.md))
- **`mikebom:peer-edge-targets`** — milestone-147 npm peer-edge PURL list for install-vs-functional filtering. See [§3.4](#34-transparency--completeness-gaps) for depth coverage. ([C101](sbom-format-mapping.md))
- **`mikebom:produces-binaries`** — milestone-116 marker for source components that produce binary outputs (cross-tier binding helper). ([C64](sbom-format-mapping.md))
- **`mikebom:purl-collisions-detected`** — milestone-134 document-scope summary of divergent same-PURL collisions. See [§3.1](#31-vulnerability-scanning) for depth coverage. ([C100](sbom-format-mapping.md))
- **`mikebom:raw-version`** — pre-canonicalization raw version string when canonicalization altered the value. ([C17](sbom-format-mapping.md))
- **`mikebom:rdepends-unresolved`** — for components with declared runtime-deps mikebom couldn't resolve, lists the unresolved names. ([C78](sbom-format-mapping.md))
- **`mikebom:requirement-range`** — the declared version-range constraint (e.g., `^1.2.0` / `>=2.0`) when known. ([C20](sbom-format-mapping.md))
- **`mikebom:resolver-step`** — milestone-091 Go resolver: enumerates which resolution-ladder step produced a transitive component (e.g., `go-mod-graph` / `go-sum-fallback`). ([C48](sbom-format-mapping.md))
- **`mikebom:root-selection-heuristic`** — milestone-127 document-scope: which heuristic selected the root component (per the [SBOM root-selection ladder](component-tiers.md)). ([C69](sbom-format-mapping.md))
- **`mikebom:sbom-tier`** — per-component tier marker (`source` / `build` / `analyzed` / `deployed` — orthogonal to component-tier). ([C5](sbom-format-mapping.md))
- **`mikebom:shade-relocation`** — Maven shade-plugin relocation rule that was applied to the vendored coord. ([C8](sbom-format-mapping.md))
- **`mikebom:source-connection-ids`** — comma-separated list of eBPF source-connection identifiers that observed this component. ([C2](sbom-format-mapping.md))
- **`mikebom:source-document-binding`** — milestone-072 cross-tier binding envelope. See [§3.3](#33-build-provenance) for depth coverage. ([C46](sbom-format-mapping.md))
- **`mikebom:source-files`** — milestone-145 JSON-array of source file paths backing this component (canonicalized via `c.evidence.source_file_paths`). ([C18](sbom-format-mapping.md))
- **`mikebom:source-mechanism`** — for cross-tier-attribution components, the mechanism used to bind source ↔ build (e.g., `cmake-fetchcontent-git` / `binary-symbol-fingerprint`). ([C55](sbom-format-mapping.md))
- **`mikebom:source-tier`** — companion to `mikebom:sbom-tier` identifying the source-tier when relevant. ([C65](sbom-format-mapping.md))
- **`mikebom:source-type`** — discovery-provenance tag. See [§3.3](#33-build-provenance) for depth coverage. ([C1](sbom-format-mapping.md))
- **`mikebom:src-uri`** — Yocto / Bitbake recipe `SRC_URI` value carrying the upstream source URL(s). ([C71](sbom-format-mapping.md))
- **`mikebom:src-uri-local-only`** — boolean signaling all `SRC_URI` entries were `file://` (no external upstream). ([C82](sbom-format-mapping.md))
- **`mikebom:srcrev`** — Yocto recipe `SRCREV` value (commit SHA pin for source). ([C70](sbom-format-mapping.md))
- **`mikebom:srcrev-by-machine`** — Yocto multi-machine builds: per-machine `SRCREV` override map. ([C72](sbom-format-mapping.md))
- **`mikebom:supplement-cdx`** — milestone-119 supplement-CDX merge marker (which CDX entries came from supplement files vs scan output). ([C66](sbom-format-mapping.md))
- **`mikebom:trace-integrity-*`** — milestone-002 family of eBPF trace-integrity counters + dropped-event diagnostics. ([C23](sbom-format-mapping.md))
- **`mikebom:yocto-class-extend`** — Yocto `BBCLASSEXTEND` flavor list (e.g., `["native", "nativesdk"]`). ([C83](sbom-format-mapping.md))
- **`mikebom:yocto-description`** — Yocto recipe `DESCRIPTION` when distinct from `SUMMARY` (added value vs the native `description` field). ([C81](sbom-format-mapping.md))
- **`mikebom:yocto-layer`** — Yocto layer name owning the recipe (provenance for multi-layer builds). ([C73](sbom-format-mapping.md))
- **`mikebom:yocto-layer-series`** — Yocto layer release series (e.g., `scarthgap` / `kirkstone`). ([C75](sbom-format-mapping.md))
- **`mikebom:yocto-layer-version`** — Yocto layer's `LAYERVERSION` declaration. ([C74](sbom-format-mapping.md))
- **`mikebom:yocto-layer-version-missing`** — Yocto transparency marker: signals that a layer's `LAYERVERSION` declaration was absent / unparseable. ([catalog](sbom-format-mapping.md))
- **`mikebom:yocto-license-closed`** — boolean signaling the Yocto recipe declared `LICENSE_FLAGS = "commercial"` (CLOSED license). ([C80](sbom-format-mapping.md))
- **`mikebom:yocto-overrides-merged`** — boolean signaling ≥1 override-syntax merge fired during Yocto recipe parsing (FR-016 union-merge approximation). ([C84](sbom-format-mapping.md))
- **`mikebom:yocto-recipe-name`** — Yocto recipe filename-derived name (emitted when host-typed PURL fires per FR-002a). ([C85](sbom-format-mapping.md))
- **`mikebom:yocto-recipe-version`** — Yocto recipe filename-derived version (companion to recipe-name). ([C86](sbom-format-mapping.md))
- **`mikebom:yocto-unexpanded-vars`** — Yocto: transparency annotation listing variables that couldn't be expanded during recipe parsing. ([C79](sbom-format-mapping.md))
- **`mikebom:vendored`** — marker for components detected as vendored into the source tree (e.g., `vendor/` for Go, Cargo workspace `vendor/`). ([catalog](sbom-format-mapping.md))

---

## Appendix B — Milestone-citation map

Each depth-covered signal cites the milestone that introduced or stabilized it. Consumers comparing a mikebom binary version against signal availability can use this table to determine whether a specific signal is available in their pinned binary.

| Signal | Milestone | Verb |
|---|---|---|
| `mikebom:lifecycle-scope` | 052 | added (Constitution V redesign: replaces an alpha-era custom annotation with native CDX `scope` + finer-grained annotation) |
| `mikebom:lifecycle-scope` | 228 (issue) | extended to SPDX 2.3 as parity-bridge so `DEPENDS_ON`-walkers can recover the dev/build/test distinction |
| `mikebom:layer-digest` | 133 (US2.2) | added |
| `mikebom:duplicate-purl-divergent` | 134 | added (Cargo only this milestone; ecosystem-agnostic detection logic) |
| `mikebom:purl-collisions-detected` | 134 | added (document-scope companion of `mikebom:duplicate-purl-divergent`) |
| `mikebom:license-concluded-source` | 132 | added (issue #363) |
| `mikebom:component-tier` (`file` value) | 133 (US1.B) | added; default flip to `orphan` mode in 133 (US1.C) |
| `mikebom:demoted-from-main-module` | 149 | added (closes issue #151) |
| `mikebom:source-type` | 002 | added |
| `mikebom:source-type` | 049, 050, 052, 055 | refined / extended across multiple milestones |
| `mikebom:generation-context` | 002 | added |
| `mikebom:generation-context` | 005, 047 | refined |
| `mikebom:source-document-binding` | 072 | added |
| `mikebom:file-inventory-mode` | 133 (US4) | added; codified in Constitution Strict Boundary §5 |
| `mikebom:graph-completeness` | 061 | added (closes #119) |
| `mikebom:graph-completeness-reason` | 061 | added |
| `mikebom:peer-edge-targets` | 147 | added (closes Trivy-comparison orphan gap on the looker-frontend npm lockfile) |

mikebom releases follow the `v*-alpha.*` tag sequence. As of milestone 149, the released version is `v0.1.0-alpha.52`. To determine signal availability for a specific binary version, cross-reference the [CHANGELOG](../../CHANGELOG.md) for the release-to-milestone mapping.
