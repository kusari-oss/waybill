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

### 2.1 Curation rubric — which signals get depth coverage

mikebom emits 100+ `mikebom:*` annotation keys (the full set lives in [Appendix A](#appendix-a--annotation-key-index)). Most are consumer-facing, but only a focused subset benefits from the full per-signal rendering treatment §3 uses — others are best served by a one-line appendix entry that points at the catalog row for the wire shape. This section documents the **decision rubric** the doc applies to make that call, so future signal additions stay principled rather than reflecting whatever the author happened to remember.

**The rubric**: 5 yes/no criteria. A signal warrants depth coverage in §3 if **at least 3 of the 5** evaluate to YES; otherwise it stays in Appendix A only.

**C1 — Drives a consumer policy decision.** YES if a documented consumer workflow (CVE filtering, license auditing, build-provenance verification, completeness audit, supplement-conflict resolution) explicitly reads the signal to decide whether to alert, suppress, gate, or escalate. NO if the signal is purely informational (e.g., binary forensics data like Mach-O load commands) or is consumed only by mikebom's own internal pipeline (e.g., dedup co-ownership evidence). Audit method: search this doc for an explicit "use it to do X" or "as a filter for Y" clause referencing the signal — if no such clause is plausibly authorable, C1 is NO.

**C2 — Cross-ecosystem reach OR ecosystem-essential.** YES if either (a) the signal is emitted by ≥2 ecosystem readers, OR (b) the signal is emitted by exactly one ecosystem AND is essential to the default consumer workflow for that ecosystem (not just an opt-in / advanced-feature flag). The "essential" exception covers cases like [`mikebom:not-linked`](#mikebom-not-linked) (Go-only; essential for Go CVE matching because Go's `runtime/debug.BuildInfo` is the only mechanism for proving linker DCE) and [`mikebom:peer-edge-targets`](#mikebom-peer-edge-targets) (npm-only; essential for npm SCA closure because peer-dep semantics are npm-unique). It does NOT cover advanced-feature / opt-in signals like `mikebom:kmp-source-set` (Kotlin Multiplatform; Android-only Kotlin projects don't use it) or `mikebom:shade-relocation` (Maven shade plugin; opt-in build-tool feature).

**C3 — Audit-significant.** YES if the signal affects the consumer's trust in the SBOM itself — it answers "should I trust this component's identification?" or "did mikebom miss anything?" or "did the operator override scanner-derived facts?". Drives auditor / reviewer workflows. NO if the signal is runtime-decision-oriented only (e.g., `mikebom:lifecycle-scope = "test"` filters CVE alerts but doesn't change the auditor's view of the SBOM's trustworthiness — it's a consumer-policy signal under C1, not an audit-trust signal under C3). A signal CAN satisfy both C1 and C3.

**C4 — Composes with another signal.** YES if the signal forms a meaningful tuple / trio with related signals consumers query together. Examples: the trust trio (`mikebom:source-type` + `mikebom:evidence-kind` + `mikebom:confidence`); the completeness pair (`mikebom:graph-completeness` + `mikebom:graph-completeness-reason`); the collision pair (`mikebom:duplicate-purl-divergent` + `mikebom:purl-collisions-detected`); the unresolved-deps pair (`mikebom:depends-unresolved` + `mikebom:rdepends-unresolved`). NO if the signal is standalone — consumers query it on its own, not in combination with another `mikebom:*` key.

**C5 — Wire shape requires documentation beyond the catalog row.** YES if the signal carries one or more of: (a) structured JSON-encoded data (object or array of records); (b) a closed enum value space (≥2 distinct named values) that benefits from explicit listing; (c) a two-state or three-state interpretation rule that affects consumer behavior (e.g., "absent means either X or Y; consumers MUST disambiguate via Z"); (d) per-format placement variance that benefits from worked jq examples. NO if the signal is a bare opaque hex string, a single boolean with no two-state interpretation rule, a single numeric with no scaling guidance, or an open-enum free-form string with no documented vocabulary.

**How to apply this to a new signal**:

1. **Score** the new `mikebom:KEY` against each of C1–C5 (5 yes/no answers).
2. **Sum** the YES count.
3. **Verdict**:
   - YES count ≥ 3 → **DEPTH coverage**: add a new subsection in the appropriate §3 cluster per the per-signal rendering shape used throughout §3 (What it is / Where it lives (per-format) / What to do with it / Milestone / Catalog link / jq recipe + Expected output). Add an Appendix A entry with `(see §3.X for depth coverage)`. Add an Appendix B entry with the originating milestone.
   - YES count < 3 → **APPENDIX coverage**: add an Appendix A entry only (one-line description + catalog C-row link).

Edge cases: if the rubric yields exactly N=3 on a marginal signal, prefer DEPTH — the cost of a slightly-too-prominent depth section is lower than the cost of a consumer never discovering an actionable signal. If unsure on C2's "essential vs niche" judgment, look at the worked-example table below for precedent.

#### Worked-example table — every depth-covered signal scored

The 18 entries §3 currently depth-covers (covering 21 unique catalog keys — 3 paired entries collapse two keys each: duplicate-purl-divergent + purl-collisions-detected, graph-completeness + …-reason, depends-unresolved + …-rdepends-unresolved). Each scores ≥3 on the rubric:

| Signal | Cluster | C1 | C2 | C3 | C4 | C5 | YES | Verdict |
|--------|---------|:--:|:--:|:--:|:--:|:--:|:---:|:-------:|
| `mikebom:lifecycle-scope` | 3.1 | Y | Y | Y | N | Y | 4 | DEPTH ✓ |
| `mikebom:layer-digest` | 3.1 | Y | Y | Y | N | Y | 4 | DEPTH ✓ |
| `mikebom:duplicate-purl-divergent` + `…-purl-collisions-detected` | 3.1 | Y | Y | Y | Y (paired) | Y | 5 | DEPTH ✓ |
| `mikebom:linkage-kind` (new in 151) | 3.1 | Y | Y | Y | N | Y | 4 | DEPTH ✓ |
| `mikebom:not-linked` (new in 151) | 3.1 | Y | Y (Go-essential) | Y | N | Y | 4 | DEPTH ✓ |
| `mikebom:license-concluded-source` | 3.2 | Y | Y | Y | N | Y | 4 | DEPTH ✓ |
| `mikebom:component-tier` (file value) | 3.2 | Y | Y | N | N | Y | 3 | DEPTH ✓ |
| `mikebom:demoted-from-main-module` | 3.2 | Y | Y | Y | N | Y | 4 | DEPTH ✓ |
| `mikebom:source-type` | 3.3 | Y | Y | Y | Y (trust trio) | Y | 5 | DEPTH ✓ |
| `mikebom:evidence-kind` (new in 151) | 3.3 | Y | Y | Y | Y (trust trio) | Y | 5 | DEPTH ✓ |
| `mikebom:confidence` (new in 151) | 3.3 | Y | Y | Y | Y (trust trio) | N | 4 | DEPTH ✓ |
| `mikebom:generation-context` | 3.3 | Y | Y | Y | N | Y | 4 | DEPTH ✓ |
| `mikebom:source-document-binding` | 3.3 | Y | Y | Y | Y | Y | 5 | DEPTH ✓ |
| `mikebom:file-inventory-mode` | 3.4 | Y | Y | Y | N | Y | 4 | DEPTH ✓ |
| `mikebom:graph-completeness` + `…-reason` | 3.4 | Y | Y | Y | Y (paired) | Y | 5 | DEPTH ✓ |
| `mikebom:go-transitive-fallback-count` (new in 172) | 3.4 | Y | Y (Go-essential; degraded-scan signal) | Y | Y (companion to C110) | Y | 5 | DEPTH ✓ |
| `mikebom:go-cache-warming-mode` + `…-failed` (new in 173) | 3.4 | Y | Y (Go-essential; monorepo ergonomics) | Y | Y (paired) | Y | 5 | DEPTH ✓ |
| `mikebom:peer-edge-targets` | 3.4 | Y | Y (npm-essential) | Y | N | Y | 4 | DEPTH ✓ |
| `mikebom:depends-unresolved` + `…-rdepends-unresolved` (new in 151) | 3.4 | Y | Y (Yocto-essential; reserved key) | Y | Y (paired) | Y | 5 | DEPTH ✓ |
| `mikebom:assertion-conflict` (new in 151) | 3.4 | Y | Y | Y | N | Y | 4 | DEPTH ✓ |

#### Counter-example table — representative appendix-only signals

7 signals correctly excluded from depth coverage. Each scores <3 on the rubric:

| Signal | C1 | C2 | C3 | C4 | C5 | YES | Verdict |
|--------|:--:|:--:|:--:|:--:|:--:|:---:|:-------:|
| `mikebom:macho-build-tools` (Mach-O load-command details) | N | N | N | N | N | 0 | APPENDIX ✓ |
| `mikebom:pe-machine` (PE architecture enum) | N | N | N | N | N | 0 | APPENDIX ✓ |
| `mikebom:elf-build-id` (opaque hex) | N | N | N | N | N | 0 | APPENDIX ✓ |
| `mikebom:yocto-layer-version-missing` (Yocto-specific transparency) | N | N | Y | N | N | 1 | APPENDIX ✓ |
| `mikebom:shade-relocation` (Maven opt-in build-time feature) | N | N | N | N | Y | 1 | APPENDIX ✓ |
| `mikebom:co-owned-by` (dedup-pipeline internal evidence) | N | N | N | N | N | 0 | APPENDIX ✓ |
| `mikebom:also-detected-via` (dedup-pipeline alternative-source list) | N | N | N | N | Y | 1 | APPENDIX ✓ |

**Validation**: the rubric correctly classifies 25/25 sampled signals (18 depth + 7 appendix-only). If a future signal addition produces a classification the maintainer disagrees with after scoring, the disagreement is either (a) a real edge case warranting a rubric clarification milestone, or (b) a signal-specific judgment call that should be resolved by re-reading the criterion definitions and the worked-example precedent — NOT by author discretion.

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

#### `mikebom:workspace-member` + `mikebom:workspaces-detected`

> **What they are**: paired signals introduced in milestone 176 to close the "which subproject is this dep in?" discoverability gap for monorepo scans. C120 is per-component (which workspace does this component belong to?); C121 is doc-scope (which workspaces does this SBOM enumerate?).
>
> **The problem being solved**: pre-m176, a security team receiving a fresh CVE (say, `pyyaml < 6.0.2`) against a mikebom SBOM of the langflow monorepo (9 pypi + 2 npm workspace members, 3280 total components) had to cross-reference the `mikebom:source-files` annotation manually to answer "which of our subprojects is affected?" That's 3280 jq walks. Post-m176 they get the answer in one jq call — `contains(["src/frontend"])` on the per-component annotation returns exactly the components that workspace declares or locks.
>
> **What C120 tells you** — the workspace(s) a component belongs to. Value is a JSON-encoded array of workspace root-relative paths (forward-slash separator on all platforms per FR-010), alphabetically sorted, deduplicated. Single-workspace components emit a 1-element array (not a bare string) for consumer-parsing uniformity. Cross-workspace shared deps (npm hoisted, Python transitive pinned in multiple lockfiles) emit N-element arrays and match every included workspace's filter via `contains()`. **File-tier (m133) and any other unattributable components do NOT get the annotation** — absence is the wire-visible signal for "no workspace attribution." Consumers wanting to enumerate file-tier components have the existing `mikebom:sbom-tier` / `mikebom:component-tier=file` discriminators; workspace-scoped filters naturally exclude file-tier via absence-selection.
>
> **What C121 tells you** — every workspace enumerated in the scan. Value equals the sorted-deduplicated UNION of every C120 value (the FR-012 cross-annotation invariant, guaranteed by construction). Emitted iff the union is non-empty; absent when zero workspaces detected (matches the C119 no-empty-array precedent). Enables the "how many subprojects does this SBOM cover?" question in one jq call — no `components[]` walk required.
>
> **The advisory log** — when the scan detects N > 1 workspaces AND produced ≥1 component, mikebom emits exactly one INFO-level log line on stderr:
>
> ```
> monorepo shape detected: 3 workspaces (docs, src/frontend, src/lfx). Downstream consumers can filter per-workspace via `mikebom:workspace-member`; see docs/reference/monorepos.md for jq recipes.
> ```
>
> The message is grep-stable: `grep -F 'monorepo shape detected: '` matches whether mikebom's log formatter is plain-text or JSON. Suppressed on single-project scans (N ≤ 1) so non-monorepo scans stay quiet. NOT gated on `--offline` — the remediation (per-workspace jq slicing) is entirely consumer-side.
>
> **Where they live**:
> - **CDX 1.6**: C120 in `components[].properties[]`; C121 in `metadata.properties[]`.
> - **SPDX 2.3**: `MikebomAnnotationCommentV1` envelope on the target Package (C120) or `SpdxDocument` (C121).
> - **SPDX 3**: typed `Annotation` graph element targeting the Package IRI (C120) or `SpdxDocument` root IRI (C121).
>
> **Standards-native audit** (Constitution Principle V): **KEEP-NO-NATIVE**. CDX `component.group` is the closest native field but semantically different — it's the component's *authoring organization/project* (e.g., `com.fasterxml.jackson.core`), NOT the scan target's workspace boundary. SPDX 2.3 has no analogous field; SPDX 3 `Element.namespace` scopes to identity-URI generation, not workspace boundary. Nested CDX composition via `metadata.component.components[]` is structural, not enumerative; loses the "workspaces that exist" vs "workspaces that produce components" distinction. See `sbom-format-mapping.md` C120 + C121 for the full rejected-alternatives audit.
>
> **What to do with them**: filter emitted components by workspace for per-subproject CVE triage (see recipes below) — the P1 use case. Enumerate workspaces via C121 without walking `components[]`. Read the monorepo reading guide at [docs/reference/monorepos.md](monorepos.md) for the full jq recipe set + composition patterns.
>
> **Follow-up milestones**: m177 (structural CDX composition per workspace) and m178 (per-workspace multi-SBOM emission) are candidate follow-ups that will build on C120 as their substrate. Emit-once, reuse many.
>
> **Milestone**: 176 — added.
> **Catalog**: [C120](sbom-format-mapping.md) + [C121](sbom-format-mapping.md)

```jq
# CDX — enumerate every workspace in the SBOM (single call, no components walk):
jq '.metadata.properties[]?
    | select(.name == "mikebom:workspaces-detected")
    | .value | fromjson' your.cdx.json
```

```jq
# CDX — list every PURL declared/locked in a specific workspace:
jq -r '.components[]
       | select((.properties[]?
                 | select(.name == "mikebom:workspace-member")
                 | .value | fromjson
                 | contains(["src/frontend"])))
       | .purl' your.cdx.json
```

```jq
# CDX — per-CVE workspace scoping ("which subprojects does this CVE hit?"):
jq -r '.components[]
       | select(.purl | startswith("pkg:pypi/pyyaml"))
       | .properties[]?
       | select(.name == "mikebom:workspace-member")
       | .value | fromjson | .[]' your.cdx.json
```

```jq
# CDX — verify the FR-012 cross-annotation invariant (C121 == union of C120):
jq '
  [.components[]?.properties[]?
   | select(.name == "mikebom:workspace-member")
   | .value | fromjson | .[]] | unique as $union
  | .metadata.properties[]?
  | select(.name == "mikebom:workspaces-detected")
  | .value | fromjson
  | {union: $union, detected: ., match: (. == $union)}
' your.cdx.json
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

#### `mikebom:linkage-kind`

> **What it is**: the binary-tier linkage mode for a component identified from a binary artifact (ELF / Mach-O / PE / Go binary). Closed enum: `dynamic` (component is loaded at runtime via a shared-library reference — e.g., an ELF `DT_NEEDED` entry pointing at `libssl.so`), `static` (component was linked into the binary at build time — e.g., a Rust crate compiled into the final binary, a Go module embedded via the toolchain), `mixed` (the binary references the component via both mechanisms, typical of binaries that statically link a base then dynamically load plugins). NO native field in any of CDX 1.6 / SPDX 2.3 / SPDX 3.0.1 — parity-bridging annotation per Constitution Principle V.
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry `{name: "mikebom:linkage-kind", value: "<enum>"}` on binary-tier components.
> - **SPDX 2.3**: annotation envelope on the Package.
> - **SPDX 3**: annotation envelope on the `software_Package` element.
> **What to do with it**: filter CVE alerting policies by linkage mode. A CVE in a `dynamic`-linked library only affects the binary at runtime IF the shared library is actually loaded — a strict policy might suppress dynamic-linked CVEs for binaries that ship with their own pinned `.so` files. `static`-linked CVEs are unconditional: the vulnerable code is baked into the binary. `mixed` warrants per-component disambiguation. Pair with `mikebom:not-linked` (Go-specific) for the full binary-tier filtering view.
> **Milestone**: 005-era — added (binary tier readers landed); closed enum stabilized via milestone 104 (binary-role classification).
> **Catalog**: [C12](sbom-format-mapping.md)

```jq
# CDX — list binary-tier components linked statically (most CVE-impactful):
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:linkage-kind" and .value == "static")
    | {name, version, purl}
' your.cdx.json
```

#### `mikebom:not-linked`

> **What it is**: marker on Go source-tier components (from `go.sum`) signaling that mikebom's binary-vs-source comparison proved the Go linker dead-code-eliminated the module from the produced binary's `runtime/debug.BuildInfo`. Emitted only when (a) a Go binary is present in the rootfs AND (b) the binary's BuildInfo does NOT confirm the component as linked. Boolean literal `true` when present. Composes with `mikebom:linkage-kind` for the full binary-tier filtering view.
>
> **Scope**: Go-only emission (milestone 050). This signal is NOT emitted on non-Go components, and a non-Go component lacking this annotation should NOT be interpreted as either present or absent in any binary. The signal applies to Go's specific build-time DCE behavior; equivalents for other ecosystems do not currently exist in mikebom.
>
> **Two-state interpretation rule** (per catalog C41):
> - **Present + `true`** → mikebom PROVED the module is not linked (Go BuildInfo authoritative).
> - **Absent** → either (a) the component IS linked (confirmed via BuildInfo), OR (b) no Go binary was present in the scan to compare against. Consumers MUST disambiguate by checking whether any binary-tier components exist in the SBOM — a SBOM with no `mikebom:component-tier = "binary"` components is in case (b).
>
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry `{name: "mikebom:not-linked", value: "true"}`.
> - **SPDX 2.3**: annotation envelope on the Package with boolean `value: true`.
> - **SPDX 3**: annotation envelope on the `software_Package` element with boolean `value: true`.
> **What to do with it**: suppress CVE alerts for Go modules marked `not-linked = true` when the consumer is running CVE matching against the deployed binary — the vulnerable code is provably not in the binary. Typical false-positive sources this suppresses: build-tag-DCE'd alternatives (e.g., `bytedance/sonic` when gin's fast-path isn't compiled in), test scaffolding modules (`davecgh/go-spew`, `kr/pretty`), older-yaml shims pulled in by transitive `go.mod` requires. Pair with `mikebom:lifecycle-scope` (test-scoped modules can be BOTH `lifecycle-scope = "test"` AND `not-linked = "true"`).
> **Milestone**: 050 — added (Go binary-vs-source comparison G3 redesign).
> **Catalog**: [C41](sbom-format-mapping.md)

```jq
# CDX — list every Go module mikebom proved is not linked into the binary:
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:not-linked" and .value == "true")
    | .purl
' your.cdx.json

# Sanity check: confirm at least one binary-tier component exists in the SBOM
# (otherwise an absent mikebom:not-linked annotation is ambiguous per the
# two-state interpretation rule):
jq '[.components[] | select(.properties[]? | .name == "mikebom:component-tier" and .value == "binary")] | length' your.cdx.json
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

**Trust trio composition.** Three signals in this cluster compose to support threshold-based vulnerability-scanner policies: `mikebom:source-type` (where the evidence came from), `mikebom:evidence-kind` (how it was derived), and `mikebom:confidence` (how strongly mikebom backs the claim). Consumers building risk-weighting filters should consider all three together — `source-type` answers "where", `evidence-kind` answers "how", and `confidence` answers "how strongly". A worked composing jq recipe appears under [`mikebom:source-type`](#mikebom-source-type) below.

#### `mikebom:source-type`

> **What it is**: tags each component with its discovery provenance. Common values include `"trace-observed"` (eBPF-observed during a live build trace), `"declared-not-cached"` (declared in a lockfile but mikebom couldn't verify its presence on disk), `"transitive"` (added via transitive lockfile resolution from an observed component), `"package-database"` (read from a system package DB like dpkg/rpm/apk). Strong-vs-weak provenance markers.
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry.
> - **SPDX 2.3**: annotation envelope on the Package.
> - **SPDX 3**: annotation envelope on the `software_Package`.
> **What to do with it**: trace-observed components have stronger ground truth than enrichment-derived ones. For vulnerability scanning, you may want to weight CVEs against trace-observed components more heavily than declared-not-cached ones. For compliance audits, mark non-trace-observed components for additional review (their presence in the SBOM is from secondary signals, not direct observation).
> **Milestone**: 002 — added; refined across milestones 049–055.
> **Catalog**: [C1](sbom-format-mapping.md)
> **Composes with**: [`mikebom:evidence-kind`](#mikebom-evidence-kind) (how it was derived), [`mikebom:confidence`](#mikebom-confidence) (how strongly).

```jq
# CDX — list components grouped by source-type provenance:
jq '[.components[]
     | {purl, source_type: (.properties[]? | select(.name == "mikebom:source-type") | .value)}]
    | group_by(.source_type)
    | map({source_type: .[0].source_type, count: length, examples: [.[0:3][] | .purl]})
' your.cdx.json
```

**Trust-trio composing recipe** (the workflow that drove this milestone — filter to high-trust components only):

```jq
# CDX — components with all three trust-trio signals present, projected as a tuple:
jq '.components[]
    | {
        purl,
        source_type:   (.properties[]? | select(.name == "mikebom:source-type")   | .value),
        evidence_kind: (.properties[]? | select(.name == "mikebom:evidence-kind") | .value),
        confidence:    (.properties[]? | select(.name == "mikebom:confidence")    | .value)
      }
    | select(.source_type != null)
' your.cdx.json
```

A vulnerability-scanner author can chain `select` calls on the tuple to filter by policy — e.g., `select(.source_type == "trace-observed" and .evidence_kind == "direct-observation")` for the strictest "alert only on directly-observed evidence" view.

#### `mikebom:evidence-kind`

> **What it is**: classification of how mikebom derived this component's identity. Closed enum: `direct-observation` (mikebom's eBPF / filesystem scan directly observed the evidence — e.g., a file with a matching SHA-256, a package-DB record, a `runtime/debug.BuildInfo` entry), `inference` (mikebom derived the identity from a secondary signal — e.g., a lockfile entry parsed without disk-confirming the artifact, a transitive resolution that walks a dep graph), `enrichment` (mikebom looked up the identity from an external source — e.g., deps.dev, PurlDB). Pairs with `mikebom:source-type` and `mikebom:confidence` as the trust trio.
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry `{name: "mikebom:evidence-kind", value: "<enum>"}`.
> - **SPDX 2.3**: annotation envelope on the Package — `{schema: "mikebom-annotation/v1", field: "mikebom:evidence-kind", value: "<enum>"}`.
> - **SPDX 3**: annotation envelope on the `software_Package` element.
> **What to do with it**: use as a filter in threshold-based vulnerability-scanner policies. Common policy: alert on `direct-observation` evidence at full severity; downgrade `inference`-derived CVEs to advisory; treat `enrichment`-only evidence as informational pending operator review. For compliance audits, the value affects which evidence-quality bucket a finding falls into — operator-asserted conclusions stacking on top of `direct-observation` evidence are strongest.
> **Milestone**: 002 — added (foundational discovery / enrichment infrastructure).
> **Catalog**: [C4](sbom-format-mapping.md)
> **Composes with**: [`mikebom:source-type`](#mikebom-source-type) (where), [`mikebom:confidence`](#mikebom-confidence) (how strongly).

```jq
# CDX — list components whose identity came from direct observation:
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:evidence-kind" and .value == "direct-observation")
    | .purl
' your.cdx.json

# SPDX 2.3 — same query via the annotation envelope:
jq -r '.packages[]
       | select(.annotations[]?
                | .comment | fromjson?
                | select(.field == "mikebom:evidence-kind" and .value == "direct-observation"))
       | .name + " " + .versionInfo' your.spdx.json

# SPDX 3 — walk the @graph Annotation elements:
jq -r '.["@graph"][]
       | select(.type == "Annotation"
                and (.statement | fromjson?
                                | .field == "mikebom:evidence-kind"
                                  and .value == "direct-observation"))
       | .subject' your.spdx3.json
```

#### `mikebom:confidence`

> **What it is**: qualitative confidence label for components identified via fuzzy or heuristic matching. Closed enum — currently only the value `"heuristic"` is emitted (set on components resolved through any heuristic path; absent on components with deterministic identity such as direct package-DB reads or content-hash matches). The third member of the trust trio (with `mikebom:source-type` and `mikebom:evidence-kind`).
>
> **Distinct from `mikebom:fingerprint-confidence`** (catalog C59, appendix-only, milestone 110): that separate annotation key carries a numeric quantitative score (`"0.70"` / `"0.85"` / `"0.99"`) specifically for milestone-108 / milestone-110 symbol-fingerprint matches on binary-tier components. Do NOT conflate the two keys — they have different value spaces, different emission gating, and live on different components. This guide depth-covers C16 (`mikebom:confidence`) only; C59 stays in Appendix A pending operator demand.
>
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry `{name: "mikebom:confidence", value: "heuristic"}`.
> - **SPDX 2.3**: annotation envelope on the Package.
> - **SPDX 3**: annotation envelope on the `software_Package` element.
> **What to do with it**: pair with `mikebom:evidence-kind` to identify components where the identity is both heuristic-derived AND from inference / enrichment — the lowest-trust bucket. A policy might alert on these only when the underlying CVE itself has a high CVSS score, or surface them for operator review before downstream automation acts.
> **Milestone**: 002 — added (foundational).
> **Catalog**: [C16](sbom-format-mapping.md)
> **Composes with**: [`mikebom:source-type`](#mikebom-source-type) (where), [`mikebom:evidence-kind`](#mikebom-evidence-kind) (how). For numeric quantitative confidence on fingerprint-matched components, see the separate [`mikebom:fingerprint-confidence`](#appendix-a--annotation-key-index) annotation (catalog C59).

```jq
# CDX — list every component flagged with heuristic confidence:
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:confidence" and .value == "heuristic")
    | {purl, evidence_kind: (.properties[]? | select(.name == "mikebom:evidence-kind") | .value)}
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

> **What they are**: document-scope **universal reachability** signal — `complete` (every component in the SBOM is reachable from a root via `dependsOn` edges), `partial` (at least one component is orphaned), or `unknown` (the classifier couldn't run — e.g., trace-mode SBOM with no root marker). The companion `-reason` annotation enumerates the cause when the value is not `complete`. Applies to every emitted SBOM regardless of ecosystem — the milestone-158 multi-root BFS runs on any resolved graph.
>
> **Rewritten in milestone 170**: pre-m170 this signal wore two hats. Milestone 061 emitted it as Go-scoped ("did every `go.sum` transitive edge resolve?") from catalog row C44; milestone 158 also emitted it as universal ("is every component reachable from a root?") from catalog row C104. Both emissions landed at the same annotation key, producing undefined consumer behavior. Milestone 170 retired C44; C104 is now the sole owner. The Go-specific "did transitive edges resolve?" signal moved to [`mikebom:go-transitive-coverage`](#mikebomgo-transitive-coverage) — its modern canonical home with a richer reason-code vocabulary.
>
> **Where it lives**:
> - **CDX 1.6**: `metadata.properties[]` entries (document-scope). Guaranteed exactly-once emission per SBOM post-m170.
> - **SPDX 2.3**: document-scope annotations on `SpdxDocument`.
> - **SPDX 3**: document-scope Annotation element targeting the `SpdxDocument` root IRI.
>
> **What to do with it**: when an SBOM reports `partial`, at least one component is orphaned in the dep-graph view. Surface the gap in your compliance dashboard or vulnerability scanner; the reason string tells you which reason-code class applies (e.g., `orphaned-components-detected`). Compose with per-component `mikebom:orphan-reason` (C45) to localize the gap.
>
> **Milestone 177 — reachability-consumer contract**: post-m177 mikebom fires the reason code `transitive-edges-unresolvable: <ecosystem-list>` whenever the scan emits ≥1 design-tier or analyzed-tier component that lacks a same-package source-tier-or-higher counterpart (per the m005 traceability ladder: `design` → `source` → `analyzed` → `deployed` → `build`; `source`/`deployed`/`build` are the "safe" tiers). This makes `graph-completeness = "partial"` on scans where the transitive-edge closure is unreliable — e.g., a Python project with only `requirements.txt` (no lockfile), or a scan where all components resolved via hash-match (analyzed-tier) without a matching source-tier peer.
>
> **Reachability consumers** (downstream tools that answer "does this CVE actually reach my application code via the dep graph?") SHOULD machine-check this signal BEFORE running: if the value is `"partial"` AND the reason contains `transitive-edges-unresolvable:`, the affected-ecosystem transitive-edge closure is unwalkable, and reachability analysis in those ecosystems WILL produce silent false negatives (nothing reaches anything because nothing is connected). Reachability tools have three options: (a) **refuse** to run and instruct the operator to remediate the scan input, (b) **run** with results flagged as low-confidence, (c) **filter** to reachability-analyze only the ecosystems NOT named in the reason detail (the partial-reachability pattern for polyglot scans).
>
> **Composes orthogonally with milestone 175** (design-tier component visibility): m175 emits an INFO-level advisory log at scan time (`"design-tier components detected: "`) for the OPERATOR; m177 emits this machine-readable signal in the SBOM for downstream reachability CONSUMERS. Both fire on the same underlying scan-input state but serve different audiences.
>
> **Milestone**: 158 — added (universal reachability). 170 — dedup + docs rewrite. 177 — added `transitive-edges-unresolvable` code to the closed vocabulary (vocabulary extended 8 → 9 codes). See [issue #516](https://github.com/kusari-oss/mikebom/issues/516) for the follow-up investigation into whether pre-m170's Go-scoped signal is reconstructible from remaining signals.
> **Catalog**: [C104](sbom-format-mapping.md) (C44 REMOVED in m170)

```jq
# CDX — check universal graph-completeness for this scan (single value post-m170):
jq '.metadata.properties[]?
    | select(.name == "mikebom:graph-completeness" or .name == "mikebom:graph-completeness-reason")
    | {name, value}' your.cdx.json
```

```jq
# CDX — Milestone 177 reachability-tool pre-flight gate: returns
# `true` when the graph is unreliable for reachability analysis
# (transitive-edge closure unwalkable in ≥1 ecosystem); `false`
# otherwise. Reachability tools SHOULD refuse or downgrade when true.
jq '
    .metadata.properties[]?
    | select(.name == "mikebom:graph-completeness-reason")
    | .value
    | contains("transitive-edges-unresolvable")
' your.cdx.json
```

```jq
# CDX — Milestone 177: extract the affected-ecosystem list so a
# reachability tool can filter to safe ecosystems (partial-reachability
# pattern for polyglot scans). Returns array of ecosystem names.
jq '
    .metadata.properties[]?
    | select(.name == "mikebom:graph-completeness-reason")
    | .value
    | capture("transitive-edges-unresolvable: (?<eco>[^;]+)")
    | .eco
    | split(", ")
' your.cdx.json
```

#### `mikebom:go-transitive-coverage` + `mikebom:go-transitive-coverage-reason`

> **What they are**: document-scope Go-specific transitive-edge coverage signal — `complete` (every observed Go module resolved via the milestone-055/091 ladder: local cache → GOPROXY fetch → `go mod graph` → `go.sum` fallback), `partial` (at least one module ended `Unresolved`), or `unknown` (the classifier couldn't measure — e.g., `--offline` + empty cache + `GOPROXY=off`, or `go mod graph` degraded). The companion `-reason` annotation enumerates the reason-code class(es). Emitted only when the scan includes ≥1 Go component; absent otherwise.
>
> **Where it lives**:
> - **CDX 1.6**: `metadata.properties[]` entries (document-scope).
> - **SPDX 2.3**: document-scope annotations on `SpdxDocument`.
> - **SPDX 3**: document-scope Annotation elements targeting the `SpdxDocument` root IRI.
>
> **What to do with it**: this is the modern home for the "did Go transitive edges resolve?" question that pre-m170 lived under [`mikebom:graph-completeness`](#mikebomgraph-completeness-mikebomgraph-completeness-reason)'s C44 emission. Consumers building Go-vulnerability policies pair it with universal graph-completeness: `unknown` on Go-transitive + `complete` on universal means "we couldn't verify Go's transitive edges, but the graph we DID emit is fully reachable"; `partial` on Go-transitive + `partial` on universal indicates a Go-specific gap that also produced orphans in the overall graph. The reason-code vocabulary includes `offline-mode`, `proxy-fetch-degraded`, `goproxy-off-in-chain`, `go-mod-graph-degraded`, `module-cache-empty-and-no-proxy`.
>
> **Milestone**: 160 — added (closes #494).
> **Catalog**: [C110](sbom-format-mapping.md) + [C111](sbom-format-mapping.md)

```jq
# CDX — check Go-transitive coverage for this scan:
jq '.metadata.properties[]?
    | select(.name == "mikebom:go-transitive-coverage" or .name == "mikebom:go-transitive-coverage-reason")
    | {name, value}' your.cdx.json
```

#### `mikebom:go-transitive-fallback-count`

> **What it is**: document-scope non-negative integer counting Go modules whose FINAL resolution step was the `go.sum` flat fallback — the last rung of the Go transitive-resolution ladder. When this count is > 0, the graph shape for that many modules is a *flat* root→transitive edge rather than the true parent-child topology the resolver would have produced from steps 1–3. Companion to [`mikebom:go-transitive-coverage`](#mikebomgo-transitive-coverage-mikebomgo-transitive-coverage-reason) (C110): C110 gives you the aggregate verdict (`complete` / `partial` / `unknown`); C117 tells you HOW MANY modules degraded — the difference between "unknown with 3 fallbacks" and "unknown with 300 fallbacks" is the difference between a minor cache miss and a fully offline scan.
>
> **The 5-step resolution ladder** (milestone 055 + 091 + 160):
>
> 1. **`go mod graph`** — invoke the host `go` toolchain to enumerate the true transitive dependency graph.
> 2. **`$GOMODCACHE` walk** — parse `.mod` files from the local Go module cache (typically `~/go/pkg/mod`) to build transitive requires.
> 3. **`$GOPROXY` fetch** — HTTP-fetch `.mod` files from the configured Go proxy (`https://proxy.golang.org` by default) to fill gaps steps 1–2 missed.
> 4. *(retired numbering — no step 4 in the modern ladder)*
> 5. **`go.sum` flat fallback** — when steps 1–3 all failed for a module (offline mode, empty cache, `GOPROXY=off`, `go mod graph` degraded), attach modules from `go.sum` as flat root→transitive edges. **Every module attached via step 5 loses its parent-child topology**: mikebom knows the module *exists* and hashes to a specific version, but not who required it or who it required in turn.
> 6. **Unresolved** — the module appears somewhere in the scan but no resolution step landed a result. Zero edges emitted; the per-component `mikebom:go-transitive-unresolved-reason` annotation names the class.
>
> **Emission rules (per Q1 clarification, milestone 172)**:
> - Annotation ABSENT → no Go components in the scan (annotation is Go-gated identically to C110).
> - Annotation `= "0"` → Go was scanned AND every module resolved cleanly via steps 1–3 (or step 6 unresolved; step 6 does NOT increment this counter — only step 5 does).
> - Annotation `= "N > 0"` → Go was scanned AND N modules landed on step 5. The graph topology for those N modules is degraded.
>
> The explicit `"0"` on healthy scans is deliberate: consumers filtering "scans with fallback contamination > threshold" can rely on presence-with-value rather than presence-absence-plus-Go-detection-heuristics.
>
> **Where it lives**:
> - **CDX 1.6**: `metadata.properties[]` entry (document-scope).
> - **SPDX 2.3**: document-scope annotation on `SpdxDocument` in the `MikebomAnnotationCommentV1` envelope.
> - **SPDX 3**: document-scope Annotation element targeting the `SpdxDocument` root IRI.
>
> **What to do with it**: use as a CI signal to detect degraded Go scans. In a well-maintained CI environment (populated `$GOMODCACHE`, working `$GOPROXY`), the count should be `"0"` for every scan. A non-zero count means EITHER (a) the CI runner lost network / lost cache / degraded proxy access, OR (b) the target repo pins modules whose upstream is no longer resolvable. Both are actionable operator signals. Cross-reference with [`mikebom:go-transitive-source`](#mikebomgo-transitive-source-mikebomgo-transitive-unresolved-reason) (C108) on individual components to enumerate WHICH modules degraded.
>
> **SC-005 invariant**: the doc-scope value equals the count of components tagged `mikebom:go-transitive-source == "go-sum-fallback"`. Both derive from the same underlying `ResolutionStep::GoSumFallback` classifier. If they diverge in an SBOM, that's an emission bug — please file an issue.
>
> **Milestone**: 172 — added.
> **Catalog**: [C117](sbom-format-mapping.md)

```jq
# CDX — check Go step-5 fallback count for this scan:
jq '.metadata.properties[]?
    | select(.name == "mikebom:go-transitive-fallback-count")
    | .value | tonumber' your.cdx.json
```

```jq
# CDX — SC-005 invariant check: does the doc count equal the per-component count?
jq '{
  doc_count:  (.metadata.properties[]? | select(.name == "mikebom:go-transitive-fallback-count") | .value | tonumber),
  per_component_count: [.components[]?.properties[]? | select(.name == "mikebom:go-transitive-source" and .value == "go-sum-fallback")] | length
} | .match = (.doc_count == .per_component_count)' your.cdx.json
```

#### `mikebom:go-cache-warming-mode` + `mikebom:go-cache-warming-failed`

> **What they are**: paired document-scope signals introduced in milestone 173 to close the "why did my Go graph degrade?" diagnostic loop that milestone 172's `mikebom:go-transitive-fallback-count` (C117) opened. The pair captures the operator's chosen cache-warming mode + any per-workspace failures encountered while warming.
>
> **The problem being solved**: pre-m173, an operator seeing `mikebom:go-transitive-fallback-count: "73"` in their SBOM knew *that* something degraded but not *what to do about it*. The m055/m091 5-step ladder needs a warm module cache OR reachable `$GOPROXY` to succeed at steps 1–3; without both, it falls through to step-5's flat go.sum fallback (loses parent-child topology). The operator-side fix is to prime the cache before scanning — via `go mod download` per workspace, or by shipping mikebom with a flag that does it for them.
>
> **The m173 flag pair**:
> - `--warm-go-cache=<off|per-workspace>` — default `off`. When set to `per-workspace`, mikebom invokes `go mod download` in every discovered Go workspace BEFORE the transitive resolver runs. Step 1 (`go mod graph`) then succeeds against a hot cache and emits true parent-child topology.
> - `--warm-go-cache-concurrency=<N>` — default `4`. `1` = sequential; `0` = auto (`min(cpus, 8)`); values above `32` are clamped with a warn log. Matches the m055/m091 `fetch_concurrency = 16` posture — monorepos are the motivating use case and sequential warming defeats the ergonomics purpose.
>
> **What C118 tells you** — the effective warming mode:
> - `"off"` — default. No warming was performed. C117 reflects the operator's env verbatim.
> - `"per-workspace"` — operator opted in via `--warm-go-cache=per-workspace`. The warmer ran.
> - `"offline-inhibited"` — operator requested `per-workspace` AND `--offline`. Warming was suppressed per FR-003 (offline mode is authoritative; warming would need network). C118 surfaces the operator's request for auditability.
>
> **What C119 tells you** — per-workspace failure records. Emitted iff at least one workspace failed warming. Absent on clean scans (no empty-array emission — byte-identity gate per FR-007). Value is a JSON-encoded array of `{reason, workspace}` records, sorted alphabetically by workspace. Reason class is a closed 6-value enum: `go-binary-absent`, `spawn-failed`, `timeout`, `subcommand-failed`, `parse-error`, `budget-exhausted`. Every failure ALSO fires a real-time `tracing::warn!` line on stderr with the same workspace + reason — operators grepping tool output during CI see the failures as they happen; the C119 annotation is the post-scan audit trail.
>
> **The (C118, C117) tuple** post-m173 is fully self-describing:
> - `(off, 0)` — healthy scan without warming
> - `(off, N>0)` — degraded scan without warming; the m173 advisory log fires exactly once suggesting `--warm-go-cache=per-workspace`
> - `(per-workspace, 0)` — warming succeeded; graph topology is authoritative
> - `(per-workspace, N>0)` — warming ran but N modules still fell to step-5; consult C119 for per-workspace failure details and cross-reference `$GOPROXY` coverage
> - `(offline-inhibited, N>0)` — warming would have run but `--offline` overrode it; C117 reflects offline-mode degradation
>
> **The advisory log** — when your env is degraded AND you haven't set `--warm-go-cache` explicitly, mikebom emits exactly one INFO-level log line on stderr:
>
> ```
> mikebom:go-transitive-fallback-count > 0 detected. Prime the cache with --warm-go-cache=per-workspace or 'go mod download' per workspace before scanning.
> ```
>
> The message is grep-stable: `grep -F 'Prime the cache with --warm-go-cache=per-workspace'` matches whether mikebom's log formatter is plain-text or JSON. Suppressed when: (1) scan has no Go components, (2) `--offline` is set, (3) operator explicitly set `--warm-go-cache=<any>` (including `off`), or (4) C117 is `"0"`.
>
> **Where they live**:
> - **CDX 1.6**: `metadata.properties[]` entries (document-scope). C118 is unconditional on Go presence; C119 is conditional on any-workspace-failed.
> - **SPDX 2.3**: document-scope annotations on `SpdxDocument` in the `MikebomAnnotationCommentV1` envelope.
> - **SPDX 3**: document-scope Annotation elements targeting the `SpdxDocument` root IRI.
>
> **What to do with them**: in CI, wire `mikebom:go-cache-warming-failed` to your monitoring — its presence is an actionable signal that specific workspaces couldn't warm. Common causes and fixes:
> - `go-binary-absent` — install the Go toolchain in the CI image.
> - `subcommand-failed` — typically an unreachable required module; check `$GOPROXY` coverage or add a `GOPRIVATE`/`GONOSUMCHECK` for internal modules.
> - `timeout` — the workspace's module closure is too large for the 60-second per-workspace budget; either use a warmer proxy, run `go mod download` outside mikebom, or (future milestone) raise the timeout via a knob.
> - `budget-exhausted` — the monorepo has more workspaces than fit in the 300-second overall budget at the chosen concurrency; raise `--warm-go-cache-concurrency` or split the scan.
>
> **Milestone**: 173 — added.
> **Catalog**: [C118](sbom-format-mapping.md) + [C119](sbom-format-mapping.md)

```jq
# CDX — get the effective cache-warming mode:
jq '.metadata.properties[]?
    | select(.name == "mikebom:go-cache-warming-mode")
    | .value' your.cdx.json
```

```jq
# CDX — list failed workspaces:
jq '.metadata.properties[]?
    | select(.name == "mikebom:go-cache-warming-failed")
    | .value
    | fromjson
    | map({workspace, reason})' your.cdx.json
```

```jq
# CDX — full m160+m172+m173 Go-signals dashboard:
jq '{
  coverage:         (.metadata.properties[]? | select(.name == "mikebom:go-transitive-coverage") | .value // "no-go-signal"),
  fallback_count:   (.metadata.properties[]? | select(.name == "mikebom:go-transitive-fallback-count") | .value // "not-emitted"),
  warming_mode:     (.metadata.properties[]? | select(.name == "mikebom:go-cache-warming-mode") | .value // "not-emitted"),
  warming_failures: ([.metadata.properties[]? | select(.name == "mikebom:go-cache-warming-failed") | .value | fromjson | .[]?] // [])
}' your.cdx.json
```

#### Design-tier components

> **What they are**: components emitted from a *constraint-only* manifest — a Python `requirements.txt` line like `pyyaml>=6.0` (no lockfile), a Ruby `Gemfile` line naming a gem without a `Gemfile.lock`, an npm root `package.json` without `package-lock.json`, a `Cargo.toml` without `Cargo.lock`, and equivalents. mikebom's ecosystem readers tag these with `sbom_tier = "design"` (native `mikebom:sbom-tier` per-component annotation) and emit them with an **empty `version` field** — because the operator's manifest DECLARED the dependency but no lockfile or install evidence pins the resolved version. Empty version is Constitution Principle IX-honest behavior: mikebom refuses to fabricate a resolved version when the scan input doesn't carry one.
>
> **The problem this signal is solving**: two empirical audits ([2026-07 langflow](audits/2026-07-05-kubernetes-argocd.md), [2026-07 test-tensorflow-models](audits/2026-07-06-tauri-airflow.md)) produced the same operator confusion — SBOMs contained dozens of components with empty version strings, and consumers couldn't tell if it was a mikebom bug or intentional. It's intentional. This subsection + the milestone-175 advisory log make it discoverable.
>
> **The traceability ladder** (mikebom's `sbom_tier` closed enum, ascending in evidence strength):
> - `"design"` — unlocked manifest declaration (this subsection's topic).
> - `"source"` — lockfile entry (`.lock` / `.freeze` / `Gemfile.lock` / etc.).
> - `"analyzed"` — artifact file on disk with a SHA-256 hash.
> - `"deployed"` — installed package DB / installed venv / installed rpm/deb/apk.
> - `"build"` — eBPF build-time trace evidence.
>
> Higher tiers imply stronger provenance. Operators MAY threshold their CI on minimum tier.
>
> **How to recognize a design-tier component in an emitted SBOM**:
>
> - **CDX 1.6**: `component.version = ""` + `evidence.identity[].confidence < 1.0` + technique `"manifest-analysis"` + `properties[]` contains `{name: "mikebom:sbom-tier", value: "design"}`. Doc-scope: `metadata.lifecycles[]` contains `{"phase": "design"}` when the SBOM has ≥1 design-tier component (m047/m081 native aggregate).
> - **SPDX 2.3**: `Package.versionInfo = ""` + `annotations[]` includes the `MikebomAnnotationCommentV1` envelope with `{field: "mikebom:sbom-tier", value: "design"}`. Doc-scope: `SpdxDocument.annotations[]` may carry parity-bridge annotations depending on config.
> - **SPDX 3.0.1**: `software_Package.packageVersion = ""` + typed `Annotation` element with `mikebom:sbom-tier = "design"` targeting the Package IRI.
>
> **The advisory log** — when the scan detects ≥1 design-tier component AND the scan produced at least one component AND `MIKEBOM_NO_DESIGN_TIER_ADVISORY` is unset, mikebom emits exactly one INFO-level log line on stderr containing the stable substring `"design-tier components detected: "`. Fires under `--offline` (remediation is offline-capable). Suppressible via the env var for CI pipelines that intentionally scan constraint-only projects.
>
> **Operator remediation** — the SBOM is correct; the *scan input* is what could improve. Two operator actions lift design-tier components to a higher-provenance tier:
>
> | Ecosystem | Lift to source-tier (generate a lockfile) | Lift to deployed-tier (install into isolated env) |
> |---|---|---|
> | pip | `uv lock` / `poetry lock` / `pip-compile requirements.in` — produces `uv.lock` / `poetry.lock` / `requirements.txt` (with pins) | `python -m venv .venv && .venv/bin/pip install -r requirements.txt` — then rescan; mikebom's pip reader detects the installed venv |
> | npm | `npm install` — writes `package-lock.json` | `npm install` also installs into `node_modules/`; the walker picks up installed packages |
> | Cargo | `cargo generate-lockfile` — writes `Cargo.lock` | `cargo build` — populates `target/` + registry cache; rescan sees the resolved crates |
> | Ruby | `bundle lock` OR `bundle install --deployment` — writes `Gemfile.lock` | `bundle install --path vendor/bundle` — installs into a per-project vendor dir; walker picks it up |
> | Composer (PHP) | `composer install --no-dev` — writes `composer.lock` | Same command; also installs into `vendor/` |
> | Cocoapods | `pod install` — writes `Podfile.lock` | Same command; also installs into `Pods/` |
> | Mix (Elixir) | `mix deps.get` — writes `mix.lock` | Also fetches into `deps/` |
> | Rebar3 (Erlang) | `rebar3 get-deps` — writes `rebar.lock` | Also fetches into `_build/` |
>
> **The `pip install` warning** — NEVER recommend `pip install <manifest>` without a virtualenv; it pollutes the operator's system Python. Every remediation above assumes venv isolation OR an alternative that doesn't touch system Python.
>
> **Milestone**: 175 — added.
> **Catalog**: [KEEP-NATIVE-FIRST audit row](sbom-format-mapping.md). See also [component-tiers.md](component-tiers.md).

```jq
# Count design-tier components (the exact number the advisory log reports):
jq '[.components[]?.version | select(. == "")] | length' your.cdx.json
```

```jq
# List design-tier PURLs — one per line:
jq -r '.components[] | select(.version == "") | .purl' your.cdx.json
```

```jq
# Verify CDX's native doc-scope aggregate — returns 1 when the SBOM has
# any design-tier component; 0 when none:
jq '[.metadata.lifecycles[]? | select(.phase == "design")] | length' your.cdx.json
```

```jq
# Mixed-tier breakdown — histogram of sbom_tier values across components:
jq '[.components[]?.properties[]? | select(.name == "mikebom:sbom-tier") | .value]
    | group_by(.)
    | map({tier: .[0], count: length})' your.cdx.json
```

```bash
# CI threshold-check (informational only, matching the m175 advisory tone):
DESIGN=$(jq '[.components[]?.version | select(. == "")] | length' scan.cdx.json)
if [ "$DESIGN" -gt 0 ]; then
  echo "::warning::mikebom found $DESIGN design-tier components; consider generating a lockfile"
fi
```

**Suppressing the advisory log in CI**: set `MIKEBOM_NO_DESIGN_TIER_ADVISORY=1` (or `true`, case-insensitive) in the environment before running mikebom. Matches the milestone-110 `MIKEBOM_NO_DEPRECATION_NOTICE=1` env-var convention. The suppression is diagnostic-only; the emitted SBOM bytes are unchanged.

#### `mikebom:peer-edge-targets`

> **What it is**: alphabetically-sorted array of PURL strings naming the peer-driven `dependsOn` edges from a given npm component. npm `peerDependencies` are install-time conventional (npm 7+ auto-installs them) but semantically declarative — different from regular `dependencies`. mikebom emits peer-edges as standard `dependsOn` (matching the npm install reality) AND tags the source component with this annotation so consumers can distinguish install-driven edges from functional-dep edges. Emitted only on npm components with ≥1 resolved peer-driven edge.
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry — annotation VALUE is a JSON-encoded array of PURL strings.
> - **SPDX 2.3**: annotation envelope on the Package — VALUE is a native JSON array (per the milestone-145 envelope-shape).
> - **SPDX 3**: annotation envelope on the `software_Package` — VALUE is a native JSON array.
> **What to do with it**: vulnerability scanners that want the install-only edge view (matching pre-milestone-147 mikebom behavior) can subtract this set from each component's `dependsOn`. License auditors who care about the functional-dep distinction can flag peer-edges separately.
>
> **Milestone 178 — SPDX 2.3 native primary signal**: post-m178, peer-driven edges in SPDX 2.3 output carry `relationshipType: "PROVIDED_DEPENDENCY_OF"` (reversed direction — reads as "B is a provided dependency of A" when A declares B as a peer) under the default `--spdx2-relationship-compat=full`. Consumers walking SPDX 2.3 typed relationship types now see the peer distinction natively without needing to parse this annotation. Under `--spdx2-relationship-compat=basic` (the m228 escape hatch for downstream tooling with basic-vocabulary relationship-type support), peer edges collapse to `DEPENDS_ON` natural-direction (pre-178 behavior preserved). The annotation itself remains present in **both** compat modes with byte-identical value — it's the finer-grained "which specific peer targets are declared" supplement (Principle V's "carry information the standard doesn't natively express" carve-out — the native relationship type says "this edge is peer" but not "which specific targets").
>
> **CDX 1.6 and SPDX 3.0.1 unchanged**: neither format has a native peer construct. CDX 1.6 `dependencies[].dependsOn[]` has NO per-element metadata slot. SPDX 3.0.1's `LifecycleScopeType` enum values `{development, build, test, runtime, design, other}` have NO `peer` value. The annotation remains the primary peer signal in both formats — parity-bridge for the missing native construct. m178 is the canonical **Principle V pattern for the "native construct exists in ONE format but not others" case**: elevate the format that has the construct; keep the annotation as parity-bridge for the others.
>
> **Milestone**: 147 — added (closes Trivy-comparison orphan gap on the looker-frontend lockfile: 5 orphans → 0). 178 — SPDX 2.3 native-first migration via `PROVIDED_DEPENDENCY_OF` relationship type.
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

```jq
# SPDX 2.3 — extract every peer edge natively via PROVIDED_DEPENDENCY_OF
# (post-m178, under default --spdx2-relationship-compat=full). Reads as
# "provided-package → consumer-package" pairs due to the reversed
# direction convention (m228).
jq '.relationships[]
    | select(.relationshipType == "PROVIDED_DEPENDENCY_OF")
    | { provided: .spdxElementId, consumer: .relatedSpdxElement }' your.spdx.json
```

#### `mikebom:optional-derivation`

> **What it is**: derivation-source string on a component classified as `LifecycleScope::Optional`. Value is drawn from an open enum naming which ecosystem-reader mechanism populated the classification:
> - `cargo-optional-true` — Cargo `[dependencies]` entry with `optional = true`
> - `npm-optional-dependencies` — shared across npm-family JavaScript ecosystems by design; the same value covers:
>   - **npm** `optionalDependencies` (`package-lock.json` v2/v3 per-entry `optional: true` flag) — m180
>   - **pnpm** `optionalDependencies` (`pnpm-lock.yaml` v9 per-package `optional: true` marker) — m180
>   - **yarn v1** parent-entry `optionalDependencies:` sub-block in `yarn.lock` — m181
>   - **yarn Berry (v2+)** `dependenciesMeta.<name>.optional = true` in `package.json` (out-of-band from `yarn.lock`) — m181
>   - **bun** — deferred to m182
> See the m179 ecosystem-survey research artifact at `specs/179-spdx23-transitive-devscope/research.md` for the per-lockfile mapping. **Peer-optional guard** (FR-005): when a dep is BOTH `peerDependencies.<name>` AND `peerDependenciesMeta.<name>.optional = true`, m178's `PROVIDED_DEPENDENCY_OF` classification wins — the optional emission is short-circuited at reader time so the dep emits as peer (not optional). npm + pnpm short-circuit via per-entry lockfile `peer: true` flags (m180); yarn short-circuits via a `package.json` cross-reference because yarn.lock doesn't carry `peer: true` (m181 — yarn's Plug'n'Play resolver moves peer metadata into `.pnp.cjs` or the source manifest).
> - `pip-optional-dependencies` — Python pip-family family, shared across three sources by design; the same value covers:
>   - **poetry.lock** per-package `optional = true` (`category = "main"` or `groups = ["main"]`; dev-groups classify as `Development` and do NOT emit this annotation per m183 Decision 2) — m183
>   - **pyproject.toml** `[project.optional-dependencies].<extra>` arrays (PEP 621) — m183
>   - **uv.lock** `[[package.optional-dependencies]].<extra>` sub-tables (uv 0.5+) — m183
>
> When both a `poetry.lock` (or `uv.lock`) AND a `pyproject.toml` are present in the same project root, the lockfile classification takes precedence for any component appearing in both (m183 Decision 3 lockfile-precedence). `setup.py`'s `extras_require` and `requirements.txt` are OUT OF SCOPE — no first-class optional-deps syntax exists in `requirements.txt`, and mikebom does not shell out to a Python interpreter to evaluate `setup.py`.
> - `maven-optional-element` — Maven `<dependency>` with `<optional>true</optional>` in `pom.xml` (POM 4.0.0 spec: transitive-exposure control — the enclosing artifact uses the dep at compile time but does NOT expose it to consumers). Extracted per-`<dependency>` at parse time; scope-derived classifications (`<scope>test</scope>` → `Development`; `<scope>provided</scope>` → `Build`) win over Optional per m184 Decision 2, so a test-scope or provided-scope dep with `<optional>true</optional>` does NOT emit this annotation. `<dependencyManagement>` entries are ignored (m184 classifies only real `<dependencies>` blocks). Inherited-`<optional>` via parent POM `<dependencyManagement>` is deferred to a follow-up milestone. — m184
> - `gradle-compile-only` — Gradle `compileOnly` deps detected via lockfile shape inference: entries appearing on any `*compileClasspath` configuration (main, `testCompileClasspath`, custom source sets like `debugCompileClasspath` for Android or `<name>CompileClasspath` for Kotlin/user-declared sets) AND absent from any `*runtimeClasspath` configuration. `buildscript-gradle.lockfile` entries with the same shape stay classified as `Build` per Decision 2 buildscript-wins. The pre-existing `mikebom:gradle-configurations` annotation is PRESERVED alongside — operators can audit the raw configs list that produced the classification. — m184
> - `erlang-optional-applications` — Erlang `.app.src` `optional_applications` list (m185+)
>
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry — value is the derivation string.
> - **SPDX 2.3**: annotation envelope on the Package.
> - **SPDX 3**: annotation envelope on the `software_Package`.
>
> **Milestone 179 — SPDX 2.3 native primary signal**: post-m179, optional-declared deps in SPDX 2.3 output carry `relationshipType: "OPTIONAL_DEPENDENCY_OF"` (reversed direction — reads as "B is an optional dependency of A" when A declares B as optional per its manifest) under the default `--spdx2-relationship-compat=full`. Consumers walking SPDX 2.3 typed relationship types can filter every "not-in-production" component in one query — closing the pico filter-parity gap where CDX `scope: "excluded"` used to catch 23 components but SPDX 2.3 only caught 13 via `TEST_DEPENDENCY_OF`. Under `--spdx2-relationship-compat=basic` (m228 escape hatch), optional edges collapse to natural-direction `DEPENDS_ON`. The annotation itself remains present in **both** compat modes with byte-identical value — it's the "which mechanism populated the classification" supplement (Principle V's "carry information the standard doesn't natively express" carve-out).
>
> **Milestone 179 also closes the m112 Go transitive gap**: components with `mikebom:build-inclusion = "not-needed"` (from `go mod why -m`) whose `lifecycle_scope` was `None` now emit as `TEST_DEPENDENCY_OF` in SPDX 2.3 under Full mode. This is a semantic overloading (some m112 not-needed components are declared-but-truly-unused, not test-only) accepted as a pragmatic first delivery — a follow-up milestone MAY refine the dispatch once m112 carries granular reason codes.
>
> **CDX 1.6 unchanged**: CDX `component.scope = "excluded"` is the native primary on the CDX side (automatic via `LifecycleScope::is_non_runtime()`); the annotation is the derivation-source supplement.
>
> **SPDX 3.0.1 unchanged**: SPDX 3's `LifecycleScopeType` enum has no `optional` value at spec 3.0.1 (verified via the m078 conformance harness). SPDX 3 emits the annotation only — parity-bridge for the missing native construct, matching the m147/m178 pattern for CDX peer-edges.
>
> **What to do with it**: filter not-in-production components identically across CDX and SPDX 2.3 formats:

```jq
# CDX — filter every not-in-production component
jq '[.components[] | select(.scope == "excluded") | .purl] | sort | unique' your.cdx.json

# SPDX 2.3 — same PURL set via native typed dep-scope verbs
# (works under default --spdx2-relationship-compat=full)
jq '
  ( [ .packages[] | { key: .SPDXID, value: (.externalRefs[]? | select(.referenceType == "purl") | .referenceLocator) } ] | from_entries ) as $purl_by_ref |
  [ .relationships[]
    | select(.relationshipType | test("^(TEST|DEV|BUILD|OPTIONAL)_DEPENDENCY_OF$"))
    | $purl_by_ref[.spdxElementId]
  ] | sort | unique
' your.spdx.json
```

> Both recipes MUST return the same sorted PURL set (contract: `specs/179-spdx23-transitive-devscope/contracts/pico-filter-parity.md` — SC-001 + SC-002 gate).

> **Milestone**: 179 — introduced `LifecycleScope::Optional` variant + `OPTIONAL_DEPENDENCY_OF` SPDX 2.3 native signal + this annotation + Go m112 NotNeeded fallthrough → `TEST_DEPENDENCY_OF`. Cargo reader wires the annotation in m179 (US3); npm + pnpm in m180; yarn v1 + Berry in m181; pip / poetry / uv in m183; Maven + Gradle in m184; Erlang deferred to m185+.
> **Catalog**: [C122](sbom-format-mapping.md)

#### `mikebom:depends-unresolved` + `mikebom:rdepends-unresolved`

> **What they are**: paired closure-gap markers naming components that mikebom KNOWS were declared as dependencies but couldn't pin to concrete components in the scan output. `mikebom:depends-unresolved` covers build-time / runtime DEPENDS-style declarations; `mikebom:rdepends-unresolved` covers the runtime-only RDEPENDS variant. The VALUE is a JSON-encoded array of unresolved dep names. Per Constitution Principle X (Transparency), the SBOM emits these markers rather than silently dropping the declarations — auditors can see exactly which deps mikebom failed to resolve.
>
> **Reserved-key framing**: the annotation key namespace is reserved for cross-ecosystem use. **Currently emitted only by the Yocto recipe reader** (milestone 128 FR-009). If you scan a non-Yocto source tree and these annotations are absent, that does NOT mean every dep was resolved — it means no reader on the scan path emits these signals yet. Treat absence as "no closure-gap data available for this ecosystem" rather than "no closure gaps." When other ecosystem readers adopt the key in future milestones, the wire shape (JSON-encoded array of names) and consumer-interpretation rule (each entry is a declared-but-unresolved dep) stay stable.
>
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry with `value` as a JSON-encoded string carrying an array of dep names.
> - **SPDX 2.3**: annotation envelope on the Package with `value` as a native JSON array.
> - **SPDX 3**: annotation envelope on the `software_Package` element with `value` as a native JSON array.
> **What to do with it**: in compliance audit dashboards, surface each entry as a known closure gap — auditors validate the original declaration source (the upstream recipe / manifest) and decide whether the unresolved dep is an in-scope risk (the dep should have been resolved; investigate why) or out-of-scope (the dep doesn't ship in this artifact). Distinct from a component being absent from the SBOM entirely: absence is opaque ("mikebom doesn't know about this dep"); `mikebom:depends-unresolved` is transparent ("mikebom knows about this declared dep but couldn't resolve it"). The pair semantically composes — when querying for closure gaps, walk both keys together.
> **Milestone**: 128 (Yocto recipe enrichment FR-009).
> **Catalog**: [C77](sbom-format-mapping.md) + [C78](sbom-format-mapping.md)

```jq
# CDX — list every component with unresolved declared deps (closure gaps):
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:depends-unresolved")
    | {
        purl,
        depends_unresolved:  (.properties[]? | select(.name == "mikebom:depends-unresolved")  | .value | fromjson),
        rdepends_unresolved: (.properties[]? | select(.name == "mikebom:rdepends-unresolved") | .value | fromjson? // [])
      }
' your.cdx.json
```

#### `mikebom:assertion-conflict`

> **What it is**: structured audit signal flagging components where the operator's supplement file declared a value (via `--supplement-cdx <path>`) that contradicted what mikebom's scanner observed. The VALUE is a JSON-encoded array of conflict records, each shaped `{field, scanner_value, supplement_value, winner, justification}`. The `winner` is a closed enum: `scanner` (mikebom kept its observation; supplement was rejected) or `supplement` (operator's declaration won; mikebom's observation was overridden). The `justification` is a closed enum naming the conflict-resolution rule that fired: `bytes-evident-detection-preserved` (mikebom's observation was derived from binary evidence and trumps operator-asserted metadata) or `developer-metadata-override` (the field is metadata-only and the operator's declaration wins by policy).
>
> **Where it lives**:
> - **CDX 1.6**: `components[].properties[]` entry with `value` as a JSON-encoded array string.
> - **SPDX 2.3**: annotation envelope on the Package with `value` as a native JSON array of records.
> - **SPDX 3**: annotation envelope on the `software_Package` element with `value` as a native JSON array of records.
> **What to do with it**: in compliance audit workflows, walk every component carrying this annotation. For each record:
> - `winner = "scanner"` records are informational — they document that the operator's supplement file declared X but the scanner observed Y; the scanner kept its observation because the field is bytes-derived. Auditors typically don't need to act, but the record exists for transparency.
> - `winner = "supplement"` records are audit-significant — the operator's declaration overrode the scanner's observation. Auditors should validate the supplement-declared value against external evidence (operator's policy, upstream registry metadata, license verbatim text) before accepting the override.
>
> The hard / soft partition (scanner wins on bytes-derived facts; developer wins on metadata) is mechanically derived from the field — consumers can re-validate the partition by re-running the logic.
> **Milestone**: 119 (supplement-CDX merge FR-008 / FR-009).
> **Catalog**: [C67](sbom-format-mapping.md)

```jq
# CDX — surface every supplement-override (winner = "supplement") for auditor review:
jq '.components[]
    | select(.properties[]?
             | .name == "mikebom:assertion-conflict")
    | {
        purl,
        conflicts: (.properties[]
                    | select(.name == "mikebom:assertion-conflict")
                    | .value | fromjson)
      }
    | .conflicts[]
    | select(.winner == "supplement")
    | {purl: .purl, field: .field, justification: .justification}
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
- **`mikebom:assertion-conflict`** — milestone-119 supplement-CDX merge: structured records flagging where the operator's supplement-declared values contradicted scanner observations (closed `winner` enum `scanner`/`supplement`, closed `justification` enum). See [§3.4](#34-transparency--completeness-gaps) for depth coverage. ([C67](sbom-format-mapping.md))
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
- **`mikebom:component-role`** — milestone-127 role tag for root-selection + downstream filtering. Open-enum string; common values: `build-tool` / `language-runtime` / `main-module` / `workspace-root` / `saas-service`. Emitted to wire output for most values; the `main-module` value is stripped on a per-component basis when the milestone-077 root override fires with the milestone-149 drop or demote path. ([C40](sbom-format-mapping.md))
- **`mikebom:component-tier`** — package-tier vs binary-tier vs file-tier classification. See [§3.2](#32-compliance-auditing) for depth coverage. ([C91](sbom-format-mapping.md))
- **`mikebom:confidence`** — qualitative confidence label for components identified via heuristic matching (closed enum — currently only `"heuristic"`). For numeric quantitative confidence on fingerprint-matched components see [`mikebom:fingerprint-confidence`](#appendix-a--annotation-key-index) (C59) — distinct key. See [§3.3](#33-build-provenance) for depth coverage. ([C16](sbom-format-mapping.md))
- **`mikebom:cpe-candidates`** — when a component has multiple unresolved CPE candidates, the full set rides this annotation (resolved candidates ride native `externalRefs[cpe23Type]`). ([C19](sbom-format-mapping.md))
- **`mikebom:demoted-from-main-module`** — milestone-149 marker for library entries preserved from the manifest-derived main-module after `--root-name` override + `--preserve-manifest-main-module` opt-in. See [§3.2](#32-compliance-auditing) for depth coverage. ([C102](sbom-format-mapping.md))
- **`mikebom:depends-unresolved`** — for components with declared deps mikebom couldn't resolve, lists the unresolved dep names (currently emitted only by the Yocto recipe reader; key namespace reserved for cross-ecosystem use). See [§3.4](#34-transparency--completeness-gaps) for depth coverage. ([C77](sbom-format-mapping.md))
- **`mikebom:deps-dev-match`** — deps.dev enrichment match record (system + name + version triple). ([C3](sbom-format-mapping.md))
- **`mikebom:detected-cargo-auditable`** — boolean signaling the binary embeds cargo-auditable JSON manifest data. ([C36](sbom-format-mapping.md))
- **`mikebom:detected-go`** — Go-binary detection metadata (Go module path + version extracted from `runtime/debug.BuildInfo`). ([C14](sbom-format-mapping.md))
- **`mikebom:download-url`** — recipe-declared download URL (Yocto / Bitbake `SRC_URI` variant). ([C53](sbom-format-mapping.md))
- **`mikebom:duplicate-purl-divergent`** — milestone-134: per-component flag for divergent same-PURL collisions. See [§3.1](#31-vulnerability-scanning) for depth coverage. ([C99](sbom-format-mapping.md))
- **`mikebom:elf-build-id`** — ELF binary's GNU build-ID hex string (from `.note.gnu.build-id` section). ([C24](sbom-format-mapping.md))
- **`mikebom:elf-compiler-stamps`** — compiler-version strings extracted from ELF `.comment` / `.note.gnu` sections. ([C49](sbom-format-mapping.md))
- **`mikebom:elf-debuglink`** — ELF `.gnu_debuglink` reference (path to debug-symbol companion file). ([C26](sbom-format-mapping.md))
- **`mikebom:elf-runpath`** — ELF `DT_RPATH` / `DT_RUNPATH` colon-separated runpath entries. ([C25](sbom-format-mapping.md))
- **`mikebom:evidence-kind`** — classification of evidence quality (`direct-observation` / `inference` / `enrichment`). See [§3.3](#33-build-provenance) for depth coverage. ([C4](sbom-format-mapping.md))
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
- **`mikebom:graph-completeness`** — milestone-158 document-scope signal for universal graph reachability (`complete` / `partial` / `unknown`). Rewritten in milestone 170 (was Go-scoped pre-m170; C44's Go-scoped emission retired, moved to `mikebom:go-transitive-coverage` — see [§3.4](#34-transparency--completeness-gaps) for depth coverage). ([C104](sbom-format-mapping.md); C44 REMOVED in m170)
- **`mikebom:graph-completeness-reason`** — milestone-158 companion enumerating the reason class for non-`complete` graph-completeness (e.g., `orphaned-components-detected: N component(s) not reachable from root`). ([C105](sbom-format-mapping.md))
- **`mikebom:go-transitive-coverage`** — milestone-160 document-scope Go-transitive edge-resolution signal (`complete` / `partial` / `unknown`). Modern home for the Go-specific completeness question that pre-m170 lived at C44. See [§3.4](#34-transparency--completeness-gaps) for depth coverage. ([C110](sbom-format-mapping.md))
- **`mikebom:go-transitive-coverage-reason`** — milestone-160 companion enumerating the reason class for non-`complete` Go-transitive coverage (e.g., `offline-mode`, `proxy-fetch-degraded`, `goproxy-off-in-chain`, `go-mod-graph-degraded`, `module-cache-empty-and-no-proxy`). ([C111](sbom-format-mapping.md))
- **`mikebom:go-transitive-fallback-count`** — milestone-172 document-scope non-negative integer counting Go modules whose FINAL resolution step was the `go.sum` flat fallback (step 5 of the 5-step ladder). Companion to C110: gives the count that C110's verdict aggregates. See [§3.4](#34-transparency--completeness-gaps) for depth coverage. ([C117](sbom-format-mapping.md))
- **`mikebom:go-cache-warming-mode`** — milestone-173 document-scope closed-enum string (`"off"` / `"per-workspace"` / `"offline-inhibited"`) reflecting the effective `--warm-go-cache` mode during the scan. See [§3.4](#34-transparency--completeness-gaps) for depth coverage. ([C118](sbom-format-mapping.md))
- **`mikebom:go-cache-warming-failed`** — milestone-173 document-scope JSON-encoded array of per-workspace warming failure records (`{reason, workspace}`). Companion to C118. Emitted iff at least one workspace failed. ([C119](sbom-format-mapping.md))
- **`mikebom:identifiers`** — milestone-073 user-defined identifier envelope (`<scheme>:<value>` records beyond the built-in `repo:` / `git:` / `image:` / `attestation:` schemes). See [identifiers.md](identifiers.md). ([C47](sbom-format-mapping.md))
- **`mikebom:kmp-source-set`** — milestone-122 Kotlin Multiplatform source-set names that declared each dep. ([C68](sbom-format-mapping.md))
- **`mikebom:layer-digest`** — milestone-133 OCI layer-digest attribution. See [§3.1](#31-vulnerability-scanning) for depth coverage. ([C88](sbom-format-mapping.md))
- **`mikebom:license-concluded-source`** — milestone-132 provenance marker for license conclusions. See [§3.2](#32-compliance-auditing) for depth coverage. ([C98](sbom-format-mapping.md))
- **`mikebom:lifecycle-scope`** — milestone-052 finer-grained dev/build/test/runtime scope distinction. See [§3.1](#31-vulnerability-scanning) for depth coverage. ([C42](sbom-format-mapping.md))
- **`mikebom:lifecycle-scope-derivation`** — milestone-062 reason-class for why a particular lifecycle-scope was chosen for a component. ([C62](sbom-format-mapping.md))
- **`mikebom:linkage-kind`** — binary linkage classification (closed enum: `dynamic` / `static` / `mixed`). See [§3.1](#31-vulnerability-scanning) for depth coverage. ([C12](sbom-format-mapping.md))
- **`mikebom:macho-build-tools`** — Mach-O `LC_BUILD_VERSION` build-tools entries (clang/swift version + similar). ([C51](sbom-format-mapping.md))
- **`mikebom:macho-build-version`** — Mach-O `LC_BUILD_VERSION` SDK + min-OS values. ([C50](sbom-format-mapping.md))
- **`mikebom:macho-codesign-flags`** — Mach-O code-signature flags bitmask (hex). ([C38](sbom-format-mapping.md))
- **`mikebom:macho-codesign-identifier`** — Mach-O code-signature identifier string. ([C37](sbom-format-mapping.md))
- **`mikebom:macho-codesign-team-id`** — Mach-O code-signature team identifier (Apple developer team). ([C39](sbom-format-mapping.md))
- **`mikebom:macho-min-os`** — Mach-O `LC_VERSION_MIN_*` minimum-OS version. ([C32](sbom-format-mapping.md))
- **`mikebom:macho-rpath`** — Mach-O `LC_RPATH` runpath entries. ([C31](sbom-format-mapping.md))
- **`mikebom:macho-uuid`** — Mach-O `LC_UUID` binary identifier. ([C30](sbom-format-mapping.md))
- **`mikebom:not-linked`** — milestone-050 marker for Go components present in source but never linked into the binary output (two-state: present `true` = proven not-linked; absent = either confirmed-linked OR no-binary-present). See [§3.1](#31-vulnerability-scanning) for depth coverage. ([C41](sbom-format-mapping.md))
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
- **`mikebom:rdepends-unresolved`** — for components with declared runtime-deps mikebom couldn't resolve, lists the unresolved names (paired with [`mikebom:depends-unresolved`](#appendix-a--annotation-key-index); same Yocto-only emission + reserved-key framing). See [§3.4](#34-transparency--completeness-gaps) for depth coverage. ([C78](sbom-format-mapping.md))
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
| `mikebom:evidence-kind` | 002-era | added (foundational discovery / enrichment infrastructure); depth coverage added in 151 |
| `mikebom:confidence` | 002-era | added (foundational; carries qualitative `"heuristic"` value — distinct from milestone-110's numeric `mikebom:fingerprint-confidence`); depth coverage added in 151 |
| `mikebom:linkage-kind` | 005-era | added (binary tier readers landed); enum stabilized in milestone 104 (binary-role classification); depth coverage added in 151 |
| `mikebom:not-linked` | 050 | added (Go binary-vs-source comparison G3 redesign); depth coverage added in 151 |
| `mikebom:assertion-conflict` | 119 | added (supplement-CDX merge FR-008 / FR-009); depth coverage added in 151 |
| `mikebom:depends-unresolved` + `mikebom:rdepends-unresolved` | 128 | added (Yocto recipe enrichment FR-009 — currently Yocto-only emission, key namespace reserved); depth coverage added in 151 |
| `mikebom:generation-context` | 002 | added |
| `mikebom:generation-context` | 005, 047 | refined |
| `mikebom:source-document-binding` | 072 | added |
| `mikebom:file-inventory-mode` | 133 (US4) | added; codified in Constitution Strict Boundary §5 |
| `mikebom:graph-completeness` | 061 | added as Go-scoped signal (closes #119); 170 rewrote as universal reachability (C104), retired C44 Go-scoped emission |
| `mikebom:graph-completeness-reason` | 061 | added as Go-scoped reason (closes #119); 170 rewrote as universal reachability reason (C105) |
| `mikebom:go-transitive-coverage` | 160 | added (closes #494) — modern home for Go-scoped transitive-edge coverage; replaces the pre-m170 C44 emission |
| `mikebom:go-transitive-coverage-reason` | 160 | added (closes #494) — companion for C110 |
| `mikebom:go-transitive-fallback-count` | 172 | added — doc-scope count of Go modules that landed on step-5 go.sum flat fallback; companion to C110 giving the numeric count under the aggregate verdict |
| `mikebom:go-cache-warming-mode` | 173 | added — doc-scope closed-enum ({off, per-workspace, offline-inhibited}) surfacing the operator's chosen cache-warming mode; companion to C117 tuple `(C118, C117)` is fully self-describing |
| `mikebom:go-cache-warming-failed` | 173 | added — doc-scope JSON-encoded array of per-workspace warming failure records; emitted iff at least one workspace failed warming |
| `mikebom:peer-edge-targets` | 147 | added (closes Trivy-comparison orphan gap on the looker-frontend npm lockfile) |

mikebom releases follow the `v*-alpha.*` tag sequence. As of milestone 149, the released version is `v0.1.0-alpha.52`. To determine signal availability for a specific binary version, cross-reference the [CHANGELOG](../../CHANGELOG.md) for the release-to-milestone mapping.
