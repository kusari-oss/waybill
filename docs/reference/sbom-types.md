# SBOM Types — operator-facing reference

**Audience**: operators consuming mikebom-emitted SBOMs who need to
classify a document by **CISA SBOM Type** (Design / Source / Build
/ Analyzed / Deployed / Runtime) for downstream policy, compliance
dashboards, or vulnerability-scanner pipelines that filter by SBOM
type. Covers per-format field positions, copy-pasteable `jq`
recipes, the four-column equivalence reference, mixed-type SBOM
presentation, edge cases, and operator self-assertion via the new
`--sbom-type` flag.

**Status**: written 2026-05-06 against mikebom v0.1.0-alpha.21
(milestone 081 — SBOM-type signaling clarity).

**Companion documents**:

- `docs/reference/identifiers.md` — document-level identifiers
  (`repo:`, `git:`, `image:`, `attestation:`, user-defined). Read
  if you also need scan-time identity metadata.
- `docs/reference/sbom-format-mapping.md` — the per-format
  audit-record source of truth (Section I lists the milestone 081
  audit conclusion: CDX clean / SPDX 2.3 escape clause / SPDX 3
  native field promotion).
- `specs/081-sbom-type-clarity/` — the design specs this doc
  externalizes. Spec is authoritative; this document is the
  operator-facing presentation.

---

## Overview — the CISA SBOM Types framework

The CISA Software Supply Chain Working Group defined six canonical
SBOM Types in April 2023
([cisa.gov, sbom-types-document-508c.pdf](https://www.cisa.gov/sites/default/files/2023-04/sbom-types-document-508c.pdf)).
Downstream tooling (vulnerability scanners, regulatory pipelines,
CISA-aligned compliance dashboards) increasingly classifies SBOMs
by these types so consumers can apply the correct policy.

| CISA Type | One-line definition |
|---|---|
| **Design** | Components an architect/developer designed in (intended dependency set, pre-implementation). |
| **Source** | Components present in the source tree (manifests, lockfiles, source-control snapshots). |
| **Build** | Components observed during the build process (build-tool output, build-graph trace). |
| **Analyzed** | Components reverse-derived from a build artifact via static or dynamic analysis. |
| **Deployed** | Components in a packaged, deployed system (installed-package databases, container layers). |
| **Runtime** | Components observed by instrumenting the running system (loaded into memory, externally called). |

mikebom's `mikebom:sbom-tier` per-component vocabulary maps 1:1
to the CISA SBOM Types (lowercase short-names: `design`, `source`,
`build`, `analyzed`, `deployed`, `runtime`).

---

## Per-format field-position table

The same conceptual SBOM-type signal lives at three different
field positions across the three formats mikebom emits.

| Format | Field path | Vocabulary | Notes |
|---|---|---|---|
| **CDX 1.6** | `metadata.lifecycles[].phase` | CDX 1.6 phase enum (`design`/`pre-build`/`build`/`post-build`/`operations`) | Standards-native enum. Aggregated lex-sorted from per-component tier values. |
| **SPDX 2.3** | `creationInfo.comment` (parse-and-translate) | Free-text "Observed lifecycle phases: <CDX-phase>, ..." | SPDX 2.3 has NO native single-document SBOM-type enum. The Principle V escape clause applies — mikebom carries the same aggregated phase set as free-text in `creationInfo.comment`. Per the milestone 081 audit, no native promotion is possible. |
| **SPDX 3** | `software_Sbom.software_sbomType[]` | SPDX 3 enum (`design`/`source`/`build`/`analyzed`/`deployed`/`runtime`) | **Standards-native** per milestone 081. The `software_Sbom` element lives in `@graph` alongside the `SpdxDocument` element (the `software_Sbom` element exists ONLY when the scan produced at least one mappable lifecycle tier). The `comment` field on `SpdxDocument` continues to carry the free-text aggregation as a backwards-compatible signal. |

---

## Per-format `jq` recipes

Copy-pasteable extractors for an mikebom-emitted document on local
disk. Output values map to CISA SBOM Types via the equivalence
table below.

### Recipe 1.1 — CDX 1.6

```bash
jq '.metadata.lifecycles[]?.phase' out.cdx.json
# Sample output for a Cargo source-tier scan:
# "pre-build"
#
# Multi-element output for a polyglot scan with both source-tier
# manifests and build-tier artifacts:
# "build"
# "pre-build"
```

Each emitted value is a CDX 1.6 phase string. Map via the four-column
equivalence table to the CISA SBOM Type (`pre-build` ↔ Source).

### Recipe 1.2 — SPDX 2.3

```bash
jq -r '.creationInfo.comment' out.spdx.json
# Sample output:
# "Scope: manifest (declared transitives included). Observed lifecycle phases: pre-build. Per-component scope detail in mikebom:sbom-tier annotations."
```

Parse the suffix after `Observed lifecycle phases:`. Map each CDX
phase via the equivalence table.

### Recipe 1.3 — SPDX 3 (post-milestone 081)

```bash
jq '.["@graph"][] | select(.type == "software_Sbom") | .software_sbomType' out.spdx3.json
# Sample output for a Cargo source-tier scan:
# [
#   "source"
# ]
#
# Multi-element output for a polyglot mixed-tier scan:
# [
#   "build",
#   "source"
# ]
```

Each emitted value is a SPDX 3 `software_SbomType` short-name —
direct mapping to the CISA SBOM Type without translation.

The SPDX 3 path is the cleanest of the three: native enum, no
string parsing, no label translation.

---

## Four-column equivalence reference

The single source of truth for translating any of the format-native
vocabularies to/from the CISA SBOM Types canonical names. Validated
against each spec's published vocabulary at milestone 081 audit
time (research §2 in `specs/081-sbom-type-clarity/research.md`).

| CISA SBOM Type | mikebom tier (`mikebom:sbom-tier`) | CDX 1.6 phase (`metadata.lifecycles[].phase`) | SPDX 3 SbomType (`software_Sbom.software_sbomType[]`) |
|---|---|---|---|
| **Design** | `design` | `design` | `design` |
| **Source** | `source` | `pre-build` | `source` |
| **Build** | `build` | `build` | `build` |
| **Analyzed** | `analyzed` | `post-build` | `analyzed` |
| **Deployed** | `deployed` | `operations` | `deployed` |
| **Runtime** | `runtime` *(operator-asserted only — see below)* | `operations` *(closest CDX equivalent — CDX has no `runtime` phase)* | `runtime` |

**Naming-case note**: CISA's document uses Title Case
(`Source`, `Build`); mikebom's emission uses lowercase
(`source`, `build`) to match the per-component
`mikebom:sbom-tier` vocabulary AND the SPDX 3 schema enum
(`prop_software_Sbom_software_sbomType`). Operators normalizing
back to Title Case for downstream-pipeline display do so at the
consumer side.

**CDX-vs-CISA naming**: CDX uses `pre-build` / `post-build` /
`operations` for what CISA calls Source / Analyzed / Deployed.
mikebom's `mikebom:sbom-tier` matches CISA exactly; CDX
`metadata.lifecycles[].phase` matches the CDX spec enum. The
equivalence table is the bridge.

**Out-of-scope CDX phases**: CDX 1.6 defines two additional
phases (`discovery`, `decommission`) that don't map to the CISA
framework. mikebom doesn't emit them. Consumers seeing these
values in non-mikebom CDX SBOMs should consult the CDX spec for
their semantics.

---

## Mixed-type SBOM presentation

A polyglot scan may produce some components tagged `source` (e.g.,
manifest entries from a `Cargo.lock`) AND others tagged `build`
(e.g., artifacts in a build cache). mikebom's `metadata.lifecycles[]`
(CDX) and `software_sbomType[]` (SPDX 3) aggregate ALL observed
tiers — the multi-element array is intentional.

```bash
jq '.metadata.lifecycles[]?.phase' out.cdx.json
# "build"
# "pre-build"

jq '.["@graph"][] | select(.type == "software_Sbom") | .software_sbomType' out.spdx3.json
# [
#   "build",
#   "source"
# ]
```

This is a **Mixed-type SBOM**: the document spans multiple CISA
types simultaneously. mikebom does NOT invent a "dominant tier"
heuristic to collapse this into a single type — the per-component
data lineage is the source of truth.

If your downstream pipeline requires a single SBOM type
(regulatory dashboards expecting one CISA type per document,
compliance tools that hard-fail on multi-type lifecycles), use
`--sbom-type <type>` (next section) to assert the operator-facing
primary type. The per-component data lineage stays accurate.

Per the 2026-05-07 Q1 clarification on milestone 081's spec:
operators wanting single-type assertion are pointed at
`--sbom-type` as the documented escape hatch; mikebom does NOT
infer a single SBOM type from a multi-tier aggregation.

---

## Operator self-assertion via `--sbom-type`

When your pipeline knows the SBOM should be classified as a single
CISA type regardless of mikebom's per-component auto-detection
(e.g., a CI/CD pipeline producing a Build SBOM where you want
downstream policy tools to classify the entire document as Build,
not as Mixed), pass `--sbom-type <type>`:

```bash
mikebom sbom scan --path . --sbom-type build \
    --format cyclonedx-json --output cyclonedx-json=out.cdx.json \
    --format spdx-2.3-json --output spdx-2.3-json=out.spdx.json \
    --format spdx-3-json --output spdx-3-json=out.spdx3.json
```

After the override:

```bash
jq '.metadata.lifecycles' out.cdx.json
# [{"phase": "build"}]   ← single element

jq -r '.creationInfo.comment' out.spdx.json
# "Scope: ... Observed lifecycle phases: build. ..."

jq '.["@graph"][] | select(.type == "software_Sbom") | .software_sbomType' out.spdx3.json
# ["build"]   ← single element
```

**Important — document-level only**: the `--sbom-type` override
applies at the document level. Per-component
`mikebom:sbom-tier` annotations preserve their auto-detected
values:

```bash
jq '.["@graph"][] | select(.type == "software_Package") | {name, annotations}' out.spdx3.json
# Per-component "mikebom:sbom-tier" annotations may still show
# "source" / "build" / "analyzed" — whatever mikebom auto-detected
# from the actual data lineage.
```

The operator's `--sbom-type build` assertion is a CLAIM about the
document's primary type for downstream-consumer classification;
it does NOT rewrite per-component data. The per-component lineage
stays accurate so downstream consumers that DO want the
fine-grained breakdown can still read it.

**Vocabulary**: exactly `{design, source, build, analyzed,
deployed, runtime}` — case-sensitive. Mismatched-case forms
(`Build`, `BUILD`) and unknown values fail at CLI parse time
with:

```text
--sbom-type 'X' is not a valid CISA SBOM type; valid values are design/source/build/analyzed/deployed/runtime
```

Available on both `mikebom sbom scan` and `mikebom trace run`.

---

## Edge cases

### Empty SBOM (no components emitted)

mikebom may produce an SBOM with zero components (e.g., scan of an
empty directory, or a manifest tree with no resolvable
dependencies). When NO component carries a recognized
`mikebom:sbom-tier` value:

- **CDX**: `metadata.lifecycles[]` is OMITTED entirely (matches the
  milestone-047 `metadata_omits_lifecycles_when_no_tiers_present`
  behavior at `cyclonedx/metadata.rs`).
- **SPDX 2.3**: `creationInfo.comment` shows
  `"Observed lifecycle phases: no lifecycle phases observed."` —
  the comment field is always present so downstream parsers can
  rely on its existence.
- **SPDX 3**: the `software_Sbom` element is OMITTED entirely from
  `@graph` (matches CDX). The `SpdxDocument` element's `comment`
  field still surfaces the free-text "no lifecycle phases observed"
  signal.

Operators interpreting absence-of-signal MUST distinguish "no
components carry tiers" from "this is a mikebom-emitted but
intentionally type-untagged SBOM" — both produce identical wire
shapes. The recommended distinguishing signal is the presence of
`@graph[].type == "software_Package"` entries (zero packages →
truly empty scan; non-zero packages but missing
`software_Sbom` → packages all lack tier annotations, which is
abnormal and should be filed as a bug).

### mikebom version mismatch

Lifecycle aggregation was introduced in milestone 047 (mikebom
v0.1.0-alpha.6). SBOMs emitted by **pre-047** mikebom versions
have NO `metadata.lifecycles[]` (CDX) and NO mention of
"Observed lifecycle phases:" in `creationInfo.comment` (SPDX 2.3 /
SPDX 3 `comment`). Detect by absence of these fields.

The SPDX 3 native `software_Sbom.software_sbomType[]` field was
introduced in **milestone 081** (mikebom v0.1.0-alpha.22). SBOMs
emitted by mikebom alpha.6 through alpha.21 have the
milestone-047 `comment` aggregation but NOT the new
`software_Sbom` element. Detect by absence of
`@graph[].type == "software_Sbom"`. Pre-alpha.22 SPDX 3 SBOMs
require the `creationInfo.comment` parse-and-translate path
(SPDX 2.3 Recipe 1.2) for SBOM-type identification.

### Per-component tier vs document-level type

Per-component `mikebom:sbom-tier` annotations describe **the
data lineage** for individual components (where mikebom learned
about each component). The document-level signal
(`metadata.lifecycles[]` / `software_sbomType[]`) describes **the
SBOM as a whole**.

These are NOT redundant: a polyglot scan with component A from
source manifest + component B from build artifact has
per-component tiers `["source", "build"]` AND document-level
lifecycles `["build", "source"]` (lex-sorted) — the document is a
"Mixed-type SBOM" spanning both. Per the milestone 081 Q1
clarification, mikebom presents this transparently rather than
collapsing to a single "dominant" type.

If the operator passes `--sbom-type build` on a mixed-tier scan,
the document-level signal collapses to `["build"]` (single-element
override) BUT per-component tiers preserve their auto-detected
`["source", "build"]` values. The operator's assertion is a
classification CLAIM; the per-component data is the actual
lineage record.

---

## Discoverability + cross-references

This document is reachable from:

- `README.md` — top-level project README, "What mikebom emits"
  section.
- `docs/reference/identifiers.md` — Section "SBOM types and
  lifecycle phases" cross-references this document.
- `docs/reference/sbom-format-mapping.md` — Section I audit-record
  appendix lists the milestone-081 entry pointing at this
  document.

For deeper milestone-design context:

- `specs/081-sbom-type-clarity/spec.md` — milestone spec.
- `specs/081-sbom-type-clarity/research.md` — Phase 0 audit (§1
  per-format field audit; §2 four-column equivalence table; §3
  `runtime` tier auto-detection deferral rationale; §4 override
  semantics).
- `specs/081-sbom-type-clarity/contracts/sbom-type-signaling.md` —
  the wire-format + CLI contract.
- `specs/047-document-level-scope-flag/` — the underlying lifecycle
  aggregation infrastructure milestone 081 builds on.
