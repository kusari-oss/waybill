# Cross-tier SBOM binding — external verifier author guide

**Audience**: maintainers of external verifier / auditor tools that
consume waybill-emitted CycloneDX 1.6, SPDX 2.3, or SPDX 3.0.1 SBOMs
and want to verify that an image-tier (or build-tier) component
traces back to a known source-tier SBOM. Covers the binding-hash
algorithm, per-format carrier shapes, the OpenVEX 0.2.0
`Product.identifiers` per-instance extension, and VEX propagation
modes — everything an external implementer needs to write a
compatible verifier from this document alone.

**Status**: written 2026-05-05 against waybill v0.1.0-alpha.15
(milestone 072). Reflects the post-072 emit + verify contract.

**Companion documents**:

- `docs/reference/conformance-harness-guide.md` — milestone 071
  per-format envelope-decode rules + the 7 inherent format-spec
  asymmetries. Read this first if you're new to waybill's emission
  model.
- `docs/reference/binding-fixtures/` — the published reference
  fixture set (SC-004). Three fixture pairs (`cargo-verified`,
  `golang-verified`, `maven-weak`) with canonical input triples +
  expected SHA-256 hex outputs. **Validate your verifier
  implementation against these fixtures before relying on it in
  production.**
- `specs/072-cross-tier-sbom-binding/contracts/` — the source
  contracts this guide externalizes. The contracts are the
  authoritative spec; this guide is the externalized presentation.

---

## Section 1 — The binding-hash-v1 algorithm

The cross-tier identity contract is a per-component hash over a
canonical envelope of three input sides drawn from the source-tier
project tree:

| Side | Type | Contents |
|---|---|---|
| `vcs` | `string` (commit identifier — typically a 40-char SHA-1 hex, but tolerant of any string) or `null` | Source-control commit identity. |
| `lockfile` | `string` (lowercase hex SHA-256) or `null` | Per-ecosystem lockfile bytes. |
| `manifest` | `string` (lowercase hex SHA-256) or `null` | Top-level project manifest bytes (as on disk). |

The triple is wrapped in a fixed-shape JSON envelope, serialized
canonically, and SHA-256'd. The output is 64 lowercase hex chars
(no `sha256:` prefix).

### 1.1 Canonical envelope (`algo: "v1"`)

```json
{
  "algo": "v1",
  "lockfile": "<sha256-or-null>",
  "manifest": "<sha256-or-null>",
  "vcs": "<commit-or-null>"
}
```

**Fixed-shape rules** (all are mandatory; deviations break
determinism):

- Exactly four keys: `algo`, `lockfile`, `manifest`, `vcs`. No more, no fewer.
- Keys appear in lexicographic order: `algo`, `lockfile`, `manifest`, `vcs`.
- `algo` value is the literal string `"v1"` for this contract.
- `lockfile`, `manifest`, `vcs` values are either a JSON string
  OR JSON `null`. **NOT empty string. NOT missing key.** An absent
  input is `null`, not omitted.
- No whitespace between tokens. Equivalent to `serde_json::to_string`
  (Rust's compact form) over a sorted-key map.

### 1.2 Hash computation

```text
binding_hash = sha256(utf8(canonical_envelope_string))
```

Output is the lowercase hex representation of the SHA-256 digest,
64 characters, no prefix.

### 1.3 Worked examples — all three strength outcomes

#### `verified` (all three sides populated)

Input:

```text
vcs:      "deadbeef0123456789abcdef0123456789abcdef"
lockfile: "4c975d294781b5e5f49b946bc5f94da8638b4c60f1c1f3a8c35fa9534744712e"
manifest: "8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da"
```

Canonical envelope:

```text
{"algo":"v1","lockfile":"4c975d294781b5e5f49b946bc5f94da8638b4c60f1c1f3a8c35fa9534744712e","manifest":"8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da","vcs":"deadbeef0123456789abcdef0123456789abcdef"}
```

SHA-256:

```text
745289decaf84d67e5cc9b333b435e8cc341ac19f7ab16673f05133d459a6111
```

Strength: `verified` (3 sides populated; all match source-tier
recomputation).

This vector is pinned at `docs/reference/binding-fixtures/cargo-verified/`
and `docs/reference/binding-fixtures/golang-verified/`; it is also
the `pinned_vec_all_three_sides` test in
`waybill-cli/src/binding/hash.rs::tests`.

#### `weak` (two sides populated — Maven case, no lockfile)

Input:

```text
vcs:      "deadbeef0123456789abcdef0123456789abcdef"
lockfile: null   (Maven has no canonical lockfile)
manifest: "8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da"
```

Canonical envelope:

```text
{"algo":"v1","lockfile":null,"manifest":"8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da","vcs":"deadbeef0123456789abcdef0123456789abcdef"}
```

SHA-256:

```text
59eca409058785ed39170de1fca456872afef52a1af7d1070719a6e36f672c35
```

Strength: `weak` (2 of 3 sides populated; both match).

This vector is pinned at `docs/reference/binding-fixtures/maven-weak/`.

#### `unknown` (fewer than two sides populated, OR any side fails verification)

When `populated_count < 2`, waybill does NOT emit a hash. The
binding annotation carries `hash: null` and a structured `reason`
naming why the trace failed (e.g., `no-evidence`,
`base-layer-system-package`).

When 2 or 3 sides are populated AND their values match the
recomputed source-tier values, strength is `weak` or `verified`.
When ANY populated side mismatches, the verifier OVERRIDES the
emitter's strength label to `unknown` with `reason:
"verification-failed"` regardless of what the emitter claimed.

### 1.4 Determinism contract

For byte-identical `(vcs, lockfile, manifest)` inputs, two distinct
waybill invocations on potentially different machines / different
alpha versions MUST produce byte-identical binding hashes.
Determinism MUST hold across waybill alpha versions for the same
`algo` value.

Specifically:

- Hash algorithm: SHA-256 (RFC 6234). Any conforming implementation
  works.
- Hex encoding: lowercase, no separators, no prefix.
- JSON serializer: any RFC 8259-conformant compact serializer that
  produces sorted-key objects. The reference Rust implementation
  uses `serde_json::to_string` over a `BTreeMap`.

A change to canonicalization, input encoding, or hash algorithm
requires a versioned binding scheme (V1 → V2) per Section 7.

---

## Section 2 — Per-ecosystem input table

Per-ecosystem rules for extracting the `(vcs, lockfile, manifest)`
triple. Mismatch with waybill's emit-side mapping → false-negative
verification.

| Ecosystem | `vcs` source | `lockfile` (SHA-256 input) | `manifest` (SHA-256 input) | Max strength |
|---|---|---|---|---|
| **golang** | Go BuildInfo `vcs.revision` (binary-tier); `git rev-parse HEAD` from source-tree's git checkout (source-tier) | `go.sum` bytes | `go.mod` bytes | `verified` |
| **cargo** | `cargo-auditable` embedded VCS metadata (binary-tier); `git rev-parse HEAD` from project root (source-tier) | top-level `Cargo.lock` bytes | top-level `Cargo.toml` bytes (workspace root, NOT individual crate manifests) | `verified` |
| **npm** | `git rev-parse HEAD` (no widespread binary-embed convention) | `package-lock.json` bytes; fallback to `yarn.lock` then `pnpm-lock.yaml` if the canonical lockfile is absent | top-level `package.json` bytes | `verified` |
| **pip** | `git rev-parse HEAD` | `poetry.lock` (Poetry projects); fallback to `pdm.lock` (PDM); fallback to a SHA-256 of the concatenated `--hash=` lines from `requirements*.txt` (PEP 503, alphabetically sorted) | top-level `pyproject.toml` bytes | `verified` |
| **gem** | `git rev-parse HEAD` | `Gemfile.lock` bytes | top-level `*.gemspec` bytes (project's own gemspec, NOT vendored gemspecs) | `verified` |
| **maven** | `git rev-parse HEAD` (future: `<scm>` block in `pom.xml`) | NOT POPULATED — Maven has no canonical lockfile in waybill's milestone 070 emission pattern | top-level `pom.xml` bytes (resolved form after parent inheritance + property substitution per milestone 070) | `weak` (capped — no lockfile) |

**Lockfile + manifest canonicalization rule**: SHA-256 the **raw
bytes as on disk**. waybill does NOT re-serialize / re-format
before hashing — bytes-on-disk is the contract. Rationale: avoids
subtle whitespace + parser-version drift between waybill and
external verifiers; the manifest's canonical form is "what the
maintainer committed."

**Maven exception**: `pom.xml` is hashed AFTER parent inheritance +
property substitution (per milestone 070). This is a known wart —
the SHA-256 input is the *resolved* form, not the on-disk form, so
external verifiers MUST run the same maven resolution before
hashing. Future work may publish the resolved-pom bytes alongside
the SBOM as a sidecar so verifiers don't need a maven resolver.

**Per-ecosystem strength derivation** (Section 1.3 covered the
algorithm; this table summarizes the per-ecosystem ceiling):

- All ecosystems EXCEPT maven can reach `verified` when the source
  tree is in a git checkout AND the lockfile + manifest are both
  present.
- `weak` is the typical state when scanning outside a git checkout
  (no VCS) or in maven (no lockfile).
- `unknown` indicates < 2 sides populated, OR any populated side
  fails to match the source-tier recomputation.

---

## Section 3 — Per-format carrier shapes

Each output format carries the binding metadata in its own
idiomatic mechanism. Per Constitution Principle V, **standards-native
cross-document references are emitted alongside** the per-component
binding annotation; only the per-component hash + strength label
(which has no native equivalent) lives in the
`waybill:source-document-binding` annotation.

### 3.1 CycloneDX 1.6

#### Standards-native cross-document reference (sibling)

`metadata.component.externalReferences[]` with `type: "bom"`:

```json
{
  "metadata": {
    "component": {
      "type": "container",
      "name": "demo-cargo-image",
      "version": "0.1.0",
      "externalReferences": [
        {
          "type": "bom",
          "url": "https://example.org/sbom/foo-source.cdx.json",
          "comment": "source-tier SBOM that produced this build/deployment",
          "hashes": [
            {
              "alg": "SHA-256",
              "content": "<sha256-of-source-sbom-canonical-bytes>"
            }
          ]
        }
      ]
    }
  }
}
```

`externalReferences[].type: "bom"` is the CDX 1.6 native cross-document
reference type — exactly the semantic waybill needs.

#### Per-component `waybill:source-document-binding` annotation

`components[].properties[]` entry where `name ==
"waybill:source-document-binding"` and `value` is the JSON-encoded
`SourceDocumentBinding` (a string, single-line, no whitespace
beyond what `serde_json::to_string` produces):

```json
{
  "components": [{
    "type": "library",
    "name": "demo-cargo-project",
    "version": "0.1.0",
    "purl": "pkg:cargo/demo-cargo-project@0.1.0",
    "bom-ref": "pkg:cargo/demo-cargo-project@0.1.0?bomref=image-instance-1",
    "properties": [
      { "name": "waybill:sbom-tier", "value": "deployed" },
      {
        "name": "waybill:source-document-binding",
        "value": "{\"algo\":\"v1\",\"hash\":\"745289decaf84d67e5cc9b333b435e8cc341ac19f7ab16673f05133d459a6111\",\"source_doc_id\":{\"sha256\":\"<sha256-of-source-sbom>\"},\"strength\":\"verified\"}"
      }
    ]
  }]
}
```

**Key carrier rules**:

- `properties[].value` is a string per CDX 1.6 schema. The string
  contains a JSON-encoded `SourceDocumentBinding` object.
- The annotation is emitted only on components carrying
  `waybill:sbom-tier: build` or `deployed`. Source-tier
  (`waybill:sbom-tier: source`) components do NOT carry it (they
  ARE the binding target, not the bound entity).
- The component's `bom-ref` is the per-instance identifier (FR-008
  per-instance VEX uses this — see Section 4).

### 3.2 SPDX 2.3

#### Standards-native cross-document reference (siblings)

Document-level `externalDocumentRefs[]`:

```json
{
  "externalDocumentRefs": [
    {
      "externalDocumentId": "DocumentRef-source-sbom",
      "spdxDocument": "https://example.org/sbom/foo-source.spdx.json",
      "checksum": {
        "algorithm": "SHA256",
        "checksumValue": "<sha256-of-source-sbom-canonical-bytes>"
      }
    }
  ]
}
```

Plus a `BUILT_FROM` relationship binding the image-tier root
component to the source-tier main-module:

```json
{
  "relationships": [
    {
      "spdxElementId": "SPDXRef-image-root",
      "relatedSpdxElement": "DocumentRef-source-sbom:SPDXRef-source-main-module",
      "relationshipType": "BUILT_FROM"
    }
  ]
}
```

`BUILT_FROM` (SPDX 2.3 §11.1) is the native binary-from-source
relationship. waybill emits this for every image/build → source
binding.

#### Per-component `waybill:source-document-binding` annotation

`Package.annotations[]` entry wrapped in the existing
`MikebomAnnotationCommentV1` envelope (`schema:
"waybill-annotation/v1"`, `field`,
`value`):

```json
{
  "packages": [{
    "name": "demo-cargo-project",
    "SPDXID": "SPDXRef-foo-binary",
    "annotations": [{
      "annotator": "Tool: waybill-0.1.0-alpha.15",
      "annotationDate": "2026-05-05T12:00:00Z",
      "annotationType": "OTHER",
      "comment": "{\"schema\":\"waybill-annotation/v1\",\"field\":\"waybill:source-document-binding\",\"value\":{\"algo\":\"v1\",\"hash\":\"745289decaf84d67e5cc9b333b435e8cc341ac19f7ab16673f05133d459a6111\",\"source_doc_id\":{\"sha256\":\"<sha256-of-source-sbom>\"},\"strength\":\"verified\"}}"
    }]
  }]
}
```

**Decode rules** (mirroring `conformance-harness-guide.md` §1.2):

1. For each `Package`, walk `annotations[]`.
2. Parse `comment` as JSON (it's a JSON-encoded string).
3. Verify `parsed.schema == "waybill-annotation/v1"`.
4. Match `parsed.field` against `"waybill:source-document-binding"`.
5. Extract `parsed.value` — a real JSON object (NOT a JSON-string).

The envelope's `value` is the per-component `SourceDocumentBinding`
shape (per Section 3.5).

### 3.3 SPDX 3.0.1

#### Standards-native cross-document reference (siblings)

Document-level `import[]` on the `SpdxDocument` element:

```json
{
  "type": "SpdxDocument",
  "spdxId": "https://example.org/spdx/image-doc",
  "import": [
    {
      "type": "ExternalMap",
      "externalSpdxId": "https://example.org/sbom/foo-source.spdx3.json",
      "verifiedUsing": [
        {
          "type": "Hash",
          "algorithm": "sha256",
          "hashValue": "<sha256-of-source-sbom-canonical-bytes>"
        }
      ]
    }
  ]
}
```

Plus a `Relationship` graph element with `relationshipType:
built_from` (lowercase per SPDX 3 convention):

```json
{
  "type": "Relationship",
  "spdxId": "https://example.org/spdx/rel-built-from-1",
  "from": "https://example.org/spdx/image-root",
  "to": ["https://example.org/spdx/source-main-module"],
  "relationshipType": "built_from"
}
```

#### Per-component `waybill:source-document-binding` annotation

A graph-element `Annotation` whose `subject` is the Package's IRI
and whose `statement` carries the same `MikebomAnnotationCommentV1`
envelope as SPDX 2.3:

```json
{
  "type": "Annotation",
  "subject": "https://example.org/spdx/foo-binary",
  "statement": "{\"schema\":\"waybill-annotation/v1\",\"field\":\"waybill:source-document-binding\",\"value\":{\"algo\":\"v1\",\"hash\":\"745289decaf84d67e5cc9b333b435e8cc341ac19f7ab16673f05133d459a6111\",\"source_doc_id\":{\"sha256\":\"<sha256-of-source-sbom>\"},\"strength\":\"verified\"}}"
}
```

**Decode rules**:

1. Walk `@graph[]` for elements with `type == "Annotation"`.
2. Filter to those whose `subject` matches a `software_Package`
   element's `spdxId`.
3. Parse `statement` as the same JSON envelope; extract `value`.

### 3.4 Choosing between native fields and `waybill:*` annotations

Per Constitution Principle V (named pattern: native-first,
`waybill:*` supplementary):

| Datum | Native carrier | `waybill:*` annotation |
|---|---|---|
| Source SBOM document identity (SHA-256, IRI) | YES — CDX `externalReferences[type:bom]`, SPDX 2.3 `externalDocumentRefs[]`, SPDX 3 `ExternalMap` | NO. The `source_doc_id` field inside the per-component annotation duplicates this for self-containment, but the document-level native field is the authoritative source. |
| Build/source provenance edge | YES — SPDX 2.3 `BUILT_FROM` relationship, SPDX 3 `relationshipType: built_from`. CDX has no native per-edge "built-from" type, so the document-level `externalReferences[type:bom]` carries the cross-document signal alone. | NO. |
| **Per-component binding hash + strength** | NO — no format has a native per-component "this binary was produced by inputs X, Y, Z with confidence W" construct. | YES (`waybill:source-document-binding`). This is the exclusive carrier. |

A correct verifier reads the document-level native fields to
locate the source SBOM, then walks the per-component `waybill:*`
annotations to recompute and compare hashes.

### 3.5 The `SourceDocumentBinding` shape

The JSON object inside the CDX `properties[].value` string OR the
SPDX envelope's `value` field:

```json
{
  "algo": "v1",
  "hash": "<lowercase-hex-sha256-or-null>",
  "source_doc_id": {
    "sha256": "<lowercase-hex-sha256>",
    "iri": "<optional-url-or-null>"
  },
  "strength": "verified" | "weak" | "unknown",
  "reason": "<optional-string>"
}
```

**Field rules**:

- `algo` — always `"v1"` for milestone 072 emission. Verifiers MUST
  reject unknown algo values as `unknown` strength with `reason:
  "unsupported-algo"` (forward-compat).
- `hash` — `null` when `strength == "unknown"` AND no recompute is
  possible (e.g., `reason: "no-evidence"`). Otherwise the
  64-char lowercase hex from Section 1.
- `source_doc_id.sha256` — required; SHA-256 of the canonical
  source SBOM bytes. Verifier-computable.
- `source_doc_id.iri` — optional URI / file path / urn:uuid:...
  for human-readable cross-reference. May be a local file path
  during local CI runs.
- `strength` — `verified` / `weak` / `unknown` enum, snake_case.
- `reason` — optional structured rationale string. Required when
  `strength == "unknown"` per FR-003 (transparency). See Section
  3.6 for the documented reason vocabulary.
- Any unknown extra fields MUST be tolerated by readers (forward
  compat).

### 3.6 Common `reason` values

The contract is open-ended (any string is allowed), but waybill
emits the following documented values:

| Reason | Meaning |
|---|---|
| `no-evidence` | Fewer than 2 of (vcs, lockfile, manifest) populated. |
| `base-layer-system-package` | Component came from an OS package manager (deb/apk/rpm); no source SBOM expected. |
| `sideloaded-binary` | Binary in image with no traceable build path (vendored, sideloaded). |
| `source-not-found-in-bind-target` | `--bind-to-source <path>` was supplied but path didn't contain a matching component. |
| `source-tier-binding-evidence-missing` | Source SBOM exists with the matching component, but the source-tier component carries no binding annotation. |
| `verification-failed` | Hash recomputed but didn't match the asserted hash. |
| `algo-version-unsupported` / `unsupported-algo` | Source SBOM's binding used a future algo version this verifier can't recompute. |

Verifier implementations SHOULD round-trip unknown `reason` values
unchanged (don't normalize); operators reading the audit trail
benefit from seeing the original string.

---

## Section 4 — OpenVEX `Product.identifiers` per-instance extension

Per FR-008, waybill extends per-product VEX statements to carry
**per-instance** identifiers (CDX `bom-ref` / SPDX `SPDXID`)
without forking the OpenVEX schema. This solves the worked-example
case where the same PURL appears in an image via two independent
component instances (one bound, one unbound) — without per-instance
identifiers, a propagated `not_affected` source-tier VEX would
quietly mask the unbound instance's potential affectedness.

### 4.1 Wire shape

OpenVEX 0.2.0's `Product` schema includes an open-ended
`identifiers: { [identifier_type]: string }` field. waybill
populates it with a documented set of identifier-type keys:

| Key | Value | When emitted |
|---|---|---|
| `purl` | The component's PURL string | Always — same as the legacy `@id` field, included for completeness. |
| `cyclonedx-bom-ref` | The CDX `bom-ref` value | When the OpenVEX sidecar accompanies a CDX SBOM. |
| `spdx-spdxid` | The SPDX `SPDXID` value | When the OpenVEX sidecar accompanies a SPDX 2.3 / SPDX 3 SBOM. |

Worked example:

```json
{
  "@context": "https://openvex.dev/ns/v0.2.0",
  "@id": "https://example.org/openvex/foo-vex",
  "author": "ci@example.org",
  "timestamp": "2026-05-05T12:00:00Z",
  "version": 1,
  "statements": [{
    "vulnerability": { "name": "CVE-2024-12345" },
    "products": [
      {
        "@id": "pkg:golang/golang.org/x/net@v0.28.0",
        "identifiers": {
          "purl": "pkg:golang/golang.org/x/net@v0.28.0",
          "cyclonedx-bom-ref": "pkg:golang/golang.org/x/net@v0.28.0?bomref=image-instance-3",
          "spdx-spdxid": "SPDXRef-image-instance-3-net"
        },
        "subcomponents": []
      }
    ],
    "status": "not_affected",
    "justification": "vulnerable_code_not_present"
  }]
}
```

### 4.2 Pre-072 consumer fallback (no breakage)

Pre-072 OpenVEX consumers (older `vexctl`, ad-hoc consumer
scripts) match products by `Product.@id` (the PURL string) and
ignore the `identifiers` map. This is supported and documented:

- `Product.@id` continues to carry the PURL string.
- Pre-072 consumers see VEX statements at PURL granularity;
  multiple per-instance statements with the same PURL collapse to
  a per-PURL view (the consumer effectively gets the aggregation
  rule defined in Section 5.1).
- No pre-072 consumer breaks; the `identifiers` map is purely
  additive metadata.

### 4.3 Post-072 per-instance application

Post-072 consumers that recognize `identifiers` can apply VEX
statements at instance granularity:

```python
for product in statement["products"]:
    bom_ref = product.get("identifiers", {}).get("cyclonedx-bom-ref")
    spdxid = product.get("identifiers", {}).get("spdx-spdxid")
    purl = product.get("identifiers", {}).get("purl") or product.get("@id")

    if bom_ref:
        # CDX-paired SBOM — apply to specific instance.
        apply_to_instance_by_bom_ref(target_sbom, bom_ref, statement)
    elif spdxid:
        # SPDX-paired SBOM — apply to specific instance.
        apply_to_instance_by_spdxid(target_sbom, spdxid, statement)
    else:
        # Pre-072 fallback — apply to all instances of this PURL.
        apply_to_all_instances_by_purl(target_sbom, purl, statement)
```

### 4.4 Stability commitment

- The identifier-type keys (`purl`, `cyclonedx-bom-ref`,
  `spdx-spdxid`) are stable across waybill alpha versions
  post-072.
- Future identifier types (e.g., `sigstore-rekor-uuid`) MAY be
  added; readers MUST tolerate unknown keys (already required by
  OpenVEX 0.2.0's open-dictionary semantic).

---

## Section 5 — VEX propagation modes + the C-3 aggregation rule

The `waybill sbom enrich --vex-propagation-mode {permissive,
caveated, strict}` flag controls how source-tier VEX statements
are propagated to image-tier components when their bindings have
varying strength.

| Mode | Behavior |
|---|---|
| `permissive` | Pre-072 behavior — propagate by PURL match without binding check. Use for back-compat when a downstream tool is broken by post-072 caveats. |
| `caveated` (default in milestone 072) | Propagate but tag binding-unverified statements with a structured `waybill:vex-binding-status: unverified` caveat. Operators reading the SBOM see exactly which propagated statements lack a verified binding. |
| `strict` | Refuse propagation when binding strength is not `verified`. The refused (vulnerability, instance) pair is NOT written to the target's `vulnerabilities[]` array; instead a refusal-rationale annotation is added under a top-level `waybill:vex-propagation-refusals` property. The command exits non-zero per VR-006 so CI pipelines can gate. |

### 5.1 The `affected ⊕ unbound-and-not-explicitly-vexed = affected` rule

When a per-PURL aggregation is needed (pre-072 consumers, or
post-072 consumers reading the aggregate VEX state across all
instances of a PURL), the rollup follows:

```text
aggregate_state(purl) =
    affected            if any instance is `affected`
  | affected            if any instance has no explicit VEX statement
                          AND has binding.strength != "verified"
                          (i.e., "could be affected" defaults to "affected"
                           until proven otherwise)
  | not_affected        if every instance is explicitly `not_affected`
                          AND all bindings are `verified`
  | under_investigation if any instance is `under_investigation`
                          (and no `affected` overrides)
  | fixed               if every instance is explicitly `fixed`
```

The headline expression: **`affected ⊕ unbound-and-not-explicitly-vexed = affected`**.

This rule is the user's worked-example resolution — a verified
`not_affected` on instance A doesn't mask an unbound instance B's
potential affectedness. Pre-072 per-PURL consumers compute this
aggregate and get the safe-by-default answer.

### 5.2 The `waybill:vex-binding-status` caveat shape

When `caveated` mode propagates onto a non-`verified` instance,
the OpenVEX statement carries a sibling `waybill:vex-binding-status`
annotation (open-ended per OpenVEX 0.2.0's "additional fields
tolerated" posture):

```json
{
  "vulnerability": { "name": "CVE-2024-12345" },
  "products": [{
    "@id": "pkg:golang/golang.org/x/net@v0.28.0",
    "identifiers": {
      "purl": "pkg:golang/golang.org/x/net@v0.28.0",
      "cyclonedx-bom-ref": "pkg:golang/golang.org/x/net@v0.28.0?bomref=image-instance-3"
    }
  }],
  "status": "not_affected",
  "justification": "vulnerable_code_not_present",
  "waybill:vex-binding-status": {
    "status": "unverified",
    "reason": "binding-strength-weak: lockfile + manifest match but no VCS commit recorded in source-tier scan",
    "source_statement_provenance": {
      "source_sbom_sha256": "e5f6...",
      "propagated_by": "waybill-0.1.0-alpha.15 sbom enrich --vex-propagation-mode caveated"
    }
  }
}
```

When the binding IS `verified`, the `waybill:vex-binding-status`
field is omitted entirely (clean post-072 output).

The caveat ALSO appears on the CDX `vulnerabilities[].affects[]`
entry as a sibling field, so consumers reading the CDX SBOM (not
the OpenVEX sidecar) see the same signal.

### 5.3 Strict-mode refusal annotations

When `--vex-propagation-mode strict` refuses a propagation, the
target SBOM's top-level `metadata.properties[]` array gains a
`waybill:vex-propagation-refusals` entry whose value is a
JSON-encoded array of per-refusal records:

```json
[
  {
    "vulnerability": "CVE-2024-12345",
    "purl": "pkg:golang/golang.org/x/net@v0.28.0",
    "bom_ref": "image-instance-3",
    "binding_strength": "weak",
    "reason": "strict mode refused propagation: binding strength was 'weak', not 'verified'"
  }
]
```

The command exits non-zero (VR-006). The SBOM is still written so
operators can audit the refusal rationale.

---

## Section 6 — Python verifier reference implementation

A standalone Python verifier that reads two CDX 1.6 SBOMs (image +
source) and recomputes per-component binding hashes against the
contract above. Uses only standard-library Python (`hashlib`,
`json`, `pathlib`) — no third-party deps. Validate your own
implementation against the published reference fixtures at
`docs/reference/binding-fixtures/` before relying on it.

```python
#!/usr/bin/env python3
"""waybill cross-tier binding verifier — Python reference.

Validates that an image-tier CDX 1.6 SBOM's per-component
`waybill:source-document-binding` annotations recompute correctly
against a source-tier CDX 1.6 SBOM. Mirrors the algorithm at
`docs/reference/cross-tier-binding.md` Section 1.
"""
import hashlib
import json
import sys
from pathlib import Path
from typing import Optional


def canonical_envelope(vcs: Optional[str],
                       lockfile: Optional[str],
                       manifest: Optional[str]) -> bytes:
    """Build the binding-hash-v1 canonical envelope per Section 1.1.

    Keys appear in lex order; absent inputs are JSON null (NOT empty
    string, NOT missing key). No whitespace. Equivalent to
    `json.dumps(..., separators=(',', ':'), sort_keys=True)`.
    """
    obj = {
        "algo": "v1",
        "lockfile": lockfile,
        "manifest": manifest,
        "vcs": vcs,
    }
    return json.dumps(obj, separators=(",", ":"), sort_keys=True).encode("utf-8")


def compute_binding_hash(vcs: Optional[str],
                         lockfile: Optional[str],
                         manifest: Optional[str]) -> str:
    """SHA-256 hex (lowercase, 64 chars) per Section 1.2."""
    return hashlib.sha256(canonical_envelope(vcs, lockfile, manifest)).hexdigest()


def find_binding_property(component: dict) -> Optional[dict]:
    """Decode the per-component `waybill:source-document-binding`
    annotation per Section 3.1. Returns None when absent."""
    for prop in component.get("properties", []):
        if prop.get("name") == "waybill:source-document-binding":
            value = prop.get("value")
            if not isinstance(value, str):
                return None
            try:
                return json.loads(value)
            except json.JSONDecodeError:
                return None
    return None


def index_components_by_purl(sbom: dict) -> dict:
    """Build a PURL -> component map from a CDX SBOM, recursing
    into nested `components[]` (CDX permits nesting)."""
    out = {}

    def walk(node):
        for c in node.get("components", []):
            purl = c.get("purl")
            if purl:
                out.setdefault(purl, []).append(c)
            walk(c)
    walk(sbom)
    return out


def verify_one(image_comp: dict, source_comp: Optional[dict]) -> dict:
    """Verify one image-tier component against its source-tier
    counterpart. Returns a dict with strength + reason fields."""
    image_binding = find_binding_property(image_comp)
    if image_binding is None:
        return {
            "purl": image_comp.get("purl"),
            "strength": "unknown",
            "reason": "no-binding-annotation",
        }

    asserted_hash = image_binding.get("hash")
    if asserted_hash is None:
        return {
            "purl": image_comp.get("purl"),
            "strength": image_binding.get("strength", "unknown"),
            "reason": image_binding.get("reason", "no-asserted-hash"),
        }

    if source_comp is None:
        return {
            "purl": image_comp.get("purl"),
            "strength": "unknown",
            "asserted_hash": asserted_hash,
            "reason": "source-tier-binding-evidence-missing",
        }

    source_binding = find_binding_property(source_comp)
    if source_binding is None or source_binding.get("hash") is None:
        return {
            "purl": image_comp.get("purl"),
            "strength": "unknown",
            "asserted_hash": asserted_hash,
            "reason": "source-tier-binding-evidence-missing",
        }

    recomputed_hash = source_binding["hash"]
    if asserted_hash != recomputed_hash:
        return {
            "purl": image_comp.get("purl"),
            "strength": "unknown",
            "asserted_hash": asserted_hash,
            "recomputed_hash": recomputed_hash,
            "reason": "verification-failed",
        }

    return {
        "purl": image_comp.get("purl"),
        "strength": image_binding.get("strength", "verified"),
        "binding_hash": asserted_hash,
        "reason": image_binding.get("reason"),
    }


def verify(image_sbom_path: Path, source_sbom_path: Path) -> dict:
    image = json.loads(image_sbom_path.read_text())
    source = json.loads(source_sbom_path.read_text())

    image_comps = index_components_by_purl(image)
    source_comps = index_components_by_purl(source)

    rows = []
    for purl, instances in sorted(image_comps.items()):
        for inst in instances:
            source_match = source_comps.get(purl, [None])[0]
            rows.append(verify_one(inst, source_match))

    summary = {
        "components_checked": len(rows),
        "verified": sum(1 for r in rows if r["strength"] == "verified"),
        "weak": sum(1 for r in rows if r["strength"] == "weak"),
        "unknown": sum(1 for r in rows if r["strength"] == "unknown"),
        "verification_failures": sum(
            1 for r in rows if r.get("reason") == "verification-failed"
        ),
    }
    return {"summary": summary, "rows": rows}


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: verify-binding.py <image-sbom> <source-sbom>",
              file=sys.stderr)
        sys.exit(2)
    report = verify(Path(sys.argv[1]), Path(sys.argv[2]))
    print(json.dumps(report, indent=2))
    # Exit non-zero on any verification failure (per FR-005 / VR-005).
    sys.exit(0 if report["summary"]["verification_failures"] == 0 else 1)
```

**Validation steps before relying on this verifier**:

1. Run it against the three reference fixtures at
   `docs/reference/binding-fixtures/`. Each MUST produce
   `verification_failures: 0` and the documented `strength` per
   the fixture's `EXPECTED.md`.
2. Confirm `compute_binding_hash` against the worked vectors in
   Section 1.3. The cargo-verified vector should produce
   `745289decaf84d67e5cc9b333b435e8cc341ac19f7ab16673f05133d459a6111`.
3. Spot-check on a few real waybill-emitted SBOMs (run `waybill
   sbom scan --image <ref> --bind-to-source <source-sbom>` and
   feed the result into your verifier).

**Extending to SPDX 2.3 + SPDX 3**: replace `index_components_by_purl`
with format-specific walkers per Section 3, and adjust
`find_binding_property` to decode the `MikebomAnnotationCommentV1`
envelope instead of the CDX property string. The
`compute_binding_hash` core is format-agnostic.

---

## Section 7 — Stability commitment + algo-version policy

### 7.1 V1 stability

Once milestone 072 ships:

- The annotation key `waybill:source-document-binding` is stable.
- The JSON shape (`{algo, hash, source_doc_id, strength, reason}`)
  is stable for `algo: "v1"`.
- The canonical envelope shape (Section 1.1) is stable for `algo:
  "v1"`.
- The OpenVEX `Product.identifiers` keys (`purl`,
  `cyclonedx-bom-ref`, `spdx-spdxid`) are stable.
- New optional fields MAY be added in future milestones
  (`skip_serializing_if`-gated on the emit side; readers SHOULD
  tolerate unknown fields).

### 7.2 Algorithm versioning

Future versions (V2, V3, ...) MUST:

- Bump the `algo` value in the envelope to `"v2"` etc.
- Be specified in a separate contract document
  (`binding-hash-v2.md`).
- Be emitted in parallel with V1 for at least one waybill milestone
  (so consumers have a deprecation window).
- Treat unknown `algo` values from external sources as "cannot
  verify" (`strength: "unknown"`, `reason: "unsupported-algo"`)
  rather than failing.

### 7.3 Verifier-author requirements for forward-compat

A correctly-implemented verifier MUST:

- Tolerate unknown `algo` values by reporting `unknown` strength
  (NOT crashing or throwing).
- Tolerate unknown `reason` values by passing them through
  unchanged.
- Tolerate unknown extra fields in the `SourceDocumentBinding`
  shape (forward compat).
- Tolerate unknown identifier-type keys in OpenVEX
  `Product.identifiers` (already required by OpenVEX 0.2.0's
  open-dictionary semantic).

If a verifier's strictness model demands rejecting unknown algo
values as a hard error, that's a verifier-policy choice — but the
contract requires the *capability* to tolerate them; the policy
sits above the contract.

---

## Section 8 — Published reference fixture set

Per SC-004, an external auditor MUST be able to write a
working verifier from this document alone and validate ≥95% of
bindings against the published reference fixture set.

The fixture set lives at `docs/reference/binding-fixtures/` and
contains three pinned fixture pairs covering the three strength
outcomes:

| Fixture | Strength | Notes |
|---|---|---|
| `cargo-verified/` | `verified` | All three input sides populated (vcs + lockfile + manifest). Pinned input substrate matches the `pinned_vec_all_three_sides` test in `waybill-cli/src/binding/hash.rs::tests`. |
| `golang-verified/` | `verified` | Same canonical input substrate as cargo-verified — the binding hash is ecosystem-agnostic at the algorithm level; only the per-ecosystem extraction sites differ per Section 2. |
| `maven-weak/` | `weak` | Maven case — no canonical lockfile, so `lockfile: null` in the envelope. Hash differs from the verified vector (different envelope bytes). |

Each fixture directory contains:

- `source.cdx.json` — the source-tier SBOM with the expected
  `waybill:source-document-binding` annotation pre-pinned on the
  main-module component.
- `image.cdx.json` — the matching image-tier SBOM whose binding
  asserts the same hash. Running `waybill sbom verify-binding
  --image-sbom image.cdx.json --source-sbom source.cdx.json`
  against any alpha.15+ build MUST produce a clean verify
  (exit 0).
- `EXPECTED.md` — the canonical input triple `(vcs, lockfile,
  manifest)` + the expected SHA-256 hex output.

The pinned hex values match the `pinned_vec_*` unit tests in
`waybill-cli/src/binding/hash.rs::tests` — single source of
truth. Future v2-bumps add fixtures under
`docs/reference/binding-fixtures-v2/` in parallel with v1.

### 8.1 Recommended verifier acceptance test

```bash
# Validate your verifier implementation against the published
# reference fixture set. Replace `your-verifier` with your
# binary's path.

for fixture in cargo-verified golang-verified maven-weak; do
    echo "=== $fixture ==="
    your-verifier \
        docs/reference/binding-fixtures/$fixture/image.cdx.json \
        docs/reference/binding-fixtures/$fixture/source.cdx.json
done
```

Expected outcome (pinned): `cargo-verified` and `golang-verified`
report `verified` with zero failures; `maven-weak` reports `weak`
with zero failures.

---

## Section 9 — Where to read the canonical specs

If anything in this guide is unclear or appears inconsistent with
the source, the source wins — please file an issue against the
waybill repo with the specific ambiguity.

- **Source contract** (algorithm details, pinned vectors):
  `specs/072-cross-tier-sbom-binding/contracts/binding-hash-v1.md`.
- **Carrier shapes contract**:
  `specs/072-cross-tier-sbom-binding/contracts/source-document-binding-annotation.md`.
- **OpenVEX identifiers contract**:
  `specs/072-cross-tier-sbom-binding/contracts/openvex-instance-identifiers.md`.
- **Reference fixtures** (SC-004): `docs/reference/binding-fixtures/`.
- **Spec + acceptance criteria**: `specs/072-cross-tier-sbom-binding/spec.md`.
- **Reference Rust implementation** (entry points):
  - `waybill::binding::compute_binding_hash` —
    `waybill-cli/src/binding/hash.rs`.
  - `waybill::binding::extract_source_inputs` —
    `waybill-cli/src/binding/source_inputs.rs`.
  - `waybill::binding::verify_binding` —
    `waybill-cli/src/binding/verify.rs`.
- **CLI commands**:
  - `waybill sbom verify-binding --image-sbom <p> --source-sbom <p>` —
    consumer-side recompute + report (exits non-zero on failure).
  - `waybill sbom trace-binding --component-purl <purl>
    --image-sbom <p> --candidate-sources-dir <d>` — operator
    triage (always exits 0; informational).
  - `waybill sbom scan --image <ref> --bind-to-source <source-sbom>` —
    image-tier scan with binding-emission opt-in.
  - `waybill sbom enrich --vex-overrides <vex.json>
    --vex-propagation-mode {permissive,caveated,strict}` —
    binding-aware VEX propagation.
- **Companion guide**:
  `docs/reference/conformance-harness-guide.md` — milestone 071
  per-format envelope-decode rules.

---

## See also

- [Identifiers](identifiers.md) — the four-layer identity model. Sibling
  concern: identifiers carry stable identity; bindings carry per-component
  cross-tier provenance.
- [SBOM types](sbom-types.md) — CISA SBOM Type signaling and the
  `--sbom-type` flag. Bindings are commonly inspected alongside SBOM-type
  filtering.
- [Conformance harness guide](conformance-harness-guide.md) — per-format
  envelope-decode rules for new waybill emission consumers.
