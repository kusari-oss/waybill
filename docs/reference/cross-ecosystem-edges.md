# Cross-ecosystem dep-name edges (milestone 218 / waybill#633)

Consumer-facing guide to the `--experimental-cross-ecosystem-edges`
CLI flag and the three annotations it enables:
`waybill:cross-ecosystem-inference`,
`waybill:cross-ecosystem-inference-ambiguous`, and
`waybill:cross-ecosystem-inference-unresolved`.

Filed against issue [waybill#633](https://github.com/kusari-oss/waybill/issues/633).

## 1. What the flag does

Waybill's graph-dep-name resolver at
`scan_fs/mod.rs:794` normally keys on `(source_ecosystem, name)`. For
`pkg:gem/` main-modules asking for `"fastlane"`, that maps to
`pkg:gem/fastlane@2.220.0`. But m216 (Gemfile-only Ruby apps) emits
main-modules as `pkg:generic/<slug>@<version>` — a purl-spec-blessed
identity for source-tree applications with no upstream registry
identity. When the resolver looks up `("generic", "fastlane")` in
`name_to_purl`, no match — the `fastlane` gem was indexed under
`("gem", "fastlane")`. **The edge silently drops.**

With `--experimental-cross-ecosystem-edges` enabled, the resolver
falls back to iterating every non-generic ecosystem in the index
whenever a `pkg:generic/`-source lookup misses. Every recovered edge
carries a per-edge `waybill:cross-ecosystem-inference` annotation
naming the ecosystem transition and the reader path that produced
the source.

Root cause detail is in the [m218 spec](../../specs/218-cross-ecosystem-edges/spec.md)
and [issue #633 body](https://github.com/kusari-oss/waybill/issues/633).

## 2. When to enable it

**Today**: enable it for scans of Gemfile-only Ruby applications
(the m216 case). Without the flag, the pkg:generic/ main-module's
outgoing DEPENDS_ON edges to the DEPENDENCIES-declared gems are
dropped in SPDX 2.3 / SPDX 3 output. (CDX shows these edges via an
unrelated CDX-emit-time synth-fallback in `dependencies.rs`, so CDX
consumers may not notice the SPDX-side gap.)

**Automatically applies to future m216-alike readers**: pip apps
declared via `pyproject.toml` with no `[project.name]`; npm CLI
tools declared via `bin.<name>` scripts with no published package;
cargo binary-only crates with no `[package]` version; Go binary
modules where `go install` on an unpublished repo produces a real
binary. None of these readers exist yet, but the resolver fix is
ecosystem-agnostic per FR-009 — every future m216-alike inherits
the fix without further code changes.

**When NOT to enable**: default off. If you've been consuming
post-m216 waybill output and coded around the missing-edges shape,
you can defer opting in until you're ready. The "experimental"
prefix conveys that annotation shapes may still evolve before flag
graduation.

## 3. Interpreting the three annotations

### 3a. `waybill:cross-ecosystem-inference` (per-edge, C137)

Emitted on every DEPENDS_ON edge produced by the flag's
cross-ecosystem fallback. Absent on same-ecosystem edges (per FR-006).

**Per-format landing slots**:

| Format    | Landing slot                                                                                      |
|-----------|---------------------------------------------------------------------------------------------------|
| CycloneDX | `dependencies[i].properties[]` on the source-Component's dependency entry                          |
| SPDX 2.3  | `Package.annotations[]` on the source Package (`MikebomAnnotationCommentV1` envelope)              |
| SPDX 3    | `Annotation` element on the source Package IRI (parity-safe alternative to per-Relationship IRI) |

**Payload** (canonical JSON, fields alphabetic):

```json
{
  "from_eco": "generic",
  "lookup_via": "gemfile-lock-dependencies",
  "target_purl": "pkg:gem/fastlane@2.220.0",
  "to_eco": "gem"
}
```

- `from_eco`: PURL type of the source main-module's ecosystem.
  Always `"generic"` at v1 (only ecosystem that triggers FR-001).
- `to_eco`: PURL type of the target ecosystem. Values in practice
  today: `"gem"`, `"pypi"`, `"npm"`, `"cargo"`, `"golang"`, etc.
- `target_purl`: full PURL of the target component. Disambiguates
  which edge this annotation applies to (CDX has no
  per-target-within-dependsOn slot; SPDX 2.3 has no relationship-
  level annotation slot).
- `lookup_via`: stable machine-readable identifier of the reader
  path that produced the source main-module. Today only
  `"gemfile-lock-dependencies"` (m216); future m216-alike readers
  register their own identifiers.

**Consumer action**: for trust-scoring / VEX reachability / Guac
ingest, these edges are waybill-inferred (not lockfile-verbatim).
Downgrade edge-confidence accordingly, or treat as authoritative if
the source PURL is `pkg:generic/*` and consumer already trusts
waybill's m216-alike readers.

### 3b. `waybill:cross-ecosystem-inference-ambiguous` (per-edge, C138)

Emitted when the FR-003 tie-break rule (prefer non-generic
main-module ecosystem present in scan) does NOT narrow the match to
exactly one target ecosystem. Every affected edge ALSO carries C137.

**Payload** (extends C137 with an `alternates[]` field):

```json
{
  "alternates": [
    {"target_purl": "pkg:npm/json@1.0.0", "to_eco": "npm"},
    {"target_purl": "pkg:pypi/json@0.1.1", "to_eco": "pypi"}
  ],
  "from_eco": "generic",
  "lookup_via": "gemfile-lock-dependencies",
  "target_purl": "pkg:gem/json@2.7.1",
  "to_eco": "gem"
}
```

`alternates[]` is sorted lex by `target_purl` and excludes self
(the current edge's own `{target_purl, to_eco}` is never in
`alternates`).

**Consumer action**: waybill emitted N edges, one per matching
ecosystem, because it couldn't determine which target ecosystem the
source really meant. If your consumer needs a single answer, pick a
tie-break rule appropriate for your domain (e.g., prefer edges
whose `to_eco` matches a main-module ecosystem YOU control) and
drop the others. If your consumer accepts fanout, keep all.

### 3c. `waybill:cross-ecosystem-inference-unresolved` (doc-scope, C139)

Emitted at document scope iff at least one `pkg:generic/`-source
`depends[]` entry matched NO component in ANY ecosystem's resolver
index. Silent when empty per FR-011.

**Per-format landing slots**:

| Format    | Landing slot                                                                     |
|-----------|-----------------------------------------------------------------------------------|
| CycloneDX | `metadata.properties[]` on the document root                                     |
| SPDX 2.3  | Document-level `Annotation` on `SPDXRef-DOCUMENT`                                |
| SPDX 3    | Document-scope `Annotation` element on the SpdxDocument root IRI                 |

**Payload** (canonical JSON array, sorted lex):

```json
[
  {"source_purl": "pkg:generic/my-app@0.0.0-unknown", "unresolved_name": "nonexistent-gem"},
  {"source_purl": "pkg:generic/other-app@0.0.0-unknown", "unresolved_name": "missing-plugin"}
]
```

**Consumer action**: an unresolved name typically indicates one of:
- The offending dep is git-sourced or path-sourced (Gemfile
  references gems by URL / local path — the m216 gem reader
  doesn't index those).
- A lockfile parser bug in waybill.
- A missing reader for an ecosystem present in the scan.

For SBOM quality dashboards, count these annotations as a
completeness signal (fewer = healthier).

## 4. Decision tree for consumers

```
For each DEPENDS_ON edge (source → target):
  ├─ Does source PURL start with pkg:generic/?
  │  ├─ NO: same-ecosystem edge. No C137/C138. Trust as lockfile-verbatim.
  │  └─ YES: check for C137 annotation on this edge.
  │     ├─ NO C137: edge is either (a) a m216-emitter-time synth (pre-m218) or
  │     │            (b) an internal same-ecosystem edge between pkg:generic/
  │     │            components (rare). Trust with caution.
  │     └─ HAS C137: edge is waybill-inferred cross-ecosystem.
  │        ├─ Check for C138 alongside.
  │        │  ├─ NO C138: single-winner resolution (either fast-path or
  │        │  │           sibling-eco tie-break narrowed to one). Trust
  │        │  │           at waybill-inferred confidence.
  │        │  └─ HAS C138: ambiguous. Waybill emitted N edges for the same
  │        │              source-name; consumer decides which to trust.
  │        │              Read `alternates[]` for the other candidates.
  │        └─ Continue graph traversal.
  └─ Continue.

After processing all edges, check document scope for C139:
  ├─ ABSENT: every cross-eco lookup resolved. Healthy scan.
  └─ PRESENT: enumerate `{source_purl, unresolved_name}` records for
     completeness dashboards or lockfile-parser bug triage.
```

## 5. Experimental status disclaimer

The `--experimental-cross-ecosystem-edges` flag is opt-in for the
m218 release. The "experimental" prefix conveys that the annotation
shapes MAY evolve before flag graduation:

- **v1 annotation payload contract** (current): `{from_eco,
  lookup_via, target_purl, to_eco}` for C137; extended with
  `alternates[]` for C138; `[{source_purl, unresolved_name}]` for
  C139. Fields declared alphabetic, canonical JSON via
  `serde_json::to_string(&struct)`.
- **Future flag graduation** (out of scope for m218) will:
  - Drop the "experimental" prefix (rename to
    `--cross-ecosystem-edges` or flip to default-on).
  - Call out any breaking payload changes in the release notes.
  - Continue to honor the env-var alias
    `WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES` as a compat shim.

If your consumer pins on the annotation payload shape today,
watch the [m218 tracking issue](https://github.com/kusari-oss/waybill/issues/633)
for graduation announcements.

## 6. Worked example

**Scan invocation** (fastlane fixture, Gemfile.lock with 27
DEPENDENCIES entries):

```sh
WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES=1 \
  waybill sbom scan \
    --path ./fastlane \
    --format cyclonedx-json \
    --output fastlane.cdx.json
```

**INFO log emitted** (per FR-013):

```
INFO waybill::scan_fs: cross-ecosystem edges resolved=27 ambiguous=0 unresolved=0
```

**Extract C137 payloads via jq**:

```sh
jq '.dependencies[]?.properties[]? |
      select(.name == "waybill:cross-ecosystem-inference") |
      .value | fromjson' \
   fastlane.cdx.json | head -20
```

Output (first three of 27):

```json
{"from_eco":"generic","lookup_via":"gemfile-lock-dependencies","target_purl":"pkg:gem/climate_control@0.2.0","to_eco":"gem"}
{"from_eco":"generic","lookup_via":"gemfile-lock-dependencies","target_purl":"pkg:gem/coveralls@0.8.23","to_eco":"gem"}
{"from_eco":"generic","lookup_via":"gemfile-lock-dependencies","target_purl":"pkg:gem/danger-junit@1.0.2","to_eco":"gem"}
```

**Extract unresolved names (SPDX 2.3)**:

```sh
jq '.annotations[]? |
      .comment | fromjson |
      select(.field == "waybill:cross-ecosystem-inference-unresolved") |
      .value | fromjson' \
   fastlane.spdx.json
```

For this fixture: no output (every DEPENDENCIES gem resolved cleanly).

**Consumer render** (Python, trust-scoring adapter):

```python
import json

with open("fastlane.cdx.json") as f:
    doc = json.load(f)

for dep in doc.get("dependencies", []):
    source = dep["ref"]
    for prop in dep.get("properties", []):
        if prop["name"] == "waybill:cross-ecosystem-inference":
            payload = json.loads(prop["value"])
            print(
                f"[inferred cross-eco edge] "
                f"{source} → {payload['target_purl']} "
                f"({payload['from_eco']} → {payload['to_eco']} "
                f"via {payload['lookup_via']})"
            )
```

## References

- Spec: `specs/218-cross-ecosystem-edges/spec.md`
- Plan: `specs/218-cross-ecosystem-edges/plan.md`
- Payload contract: `specs/218-cross-ecosystem-edges/contracts/annotation-payloads.md`
- Tie-break rule: `specs/218-cross-ecosystem-edges/contracts/tie-break-rule.md`
- Parity catalog rows: `docs/reference/sbom-format-mapping.md` (C137, C138, C139)
- Issue: [waybill#633](https://github.com/kusari-oss/waybill/issues/633)
