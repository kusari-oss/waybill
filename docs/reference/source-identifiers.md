# Source identifiers — external SBOM consumer guide

**Audience**: maintainers of external SBOM consumer / verifier tools
that read mikebom-emitted CycloneDX 1.6, SPDX 2.3, or SPDX 3.0.1
SBOMs and want to extract the document-level identifiers attached at
scan time (`repo:`, `git:`, `image:`, `attestation:`, plus arbitrary
operator-defined opaque schemes). Covers the wire format, per-format
carrier shapes, auto-detection paths, the determinism contract, and
runnable `jq` decode recipes — everything an external implementer
needs to write a working extractor from this document alone.

**Status**: written 2026-05-05 against mikebom v0.1.0-alpha.16
(milestone 073). Reflects the alpha.16 source-identifier emission
contract.

**Companion documents**:

- `docs/reference/cross-tier-binding.md` — milestone-072 cross-tier
  binding guide (binding hash, per-component verifier flow, VEX
  propagation modes). Source identifiers and source-document
  bindings are sibling concerns: identifiers carry stable identity;
  bindings carry per-component cross-tier provenance. Milestone 074
  will resolve identifiers to source-SBOM file paths.
- `docs/reference/conformance-harness-guide.md` — milestone-071
  per-format envelope-decode rules. Read first if you're new to
  mikebom's emission model.
- `specs/073-source-identifiers/contracts/` — the source contracts
  this guide externalizes. The contracts are authoritative; this
  guide is the operator-facing presentation.

---

## Section 1 — Wire format

A source identifier is a `(scheme, value)` pair encoded on the
command line as:

```text
<scheme>:<value>
```

- `<scheme>` matches the regex `^[a-z][a-z0-9_-]*$` — lowercase ASCII
  letter start, lowercase letters / digits / underscores / hyphens
  thereafter. Empty schemes, leading digits, uppercase, dots, and
  whitespace are rejected at clap parse time.
- `<value>` is everything after the FIRST `:` character. Values may
  contain additional `:` characters (e.g., `git@github.com:foo/bar.git`
  carries an embedded `:`; `image:docker.io/foo:v1@sha256:abc...`
  carries two). Empty values are rejected at parse time.
- The split is on the FIRST `:` only. Consumers parsing identifiers
  from carrier fields MUST use the same first-`:` split rule.

### 1.1 Worked examples

| Input | Scheme | Value |
|---|---|---|
| `repo:git@github.com:foo/bar.git` | `repo` | `git@github.com:foo/bar.git` |
| `image:docker.io/foo/bar:v1@sha256:abc...` | `image` | `docker.io/foo/bar:v1@sha256:abc...` |
| `acme_corp_id:abc123` | `acme_corp_id` | `abc123` |
| `attestation:https://example.org/att/build-42` | `attestation` | `https://example.org/att/build-42` |

---

## Section 2 — Built-in scheme registry

Four built-in schemes are recognized + value-validated by
mikebom alpha.16+. Built-in identifiers ride per-format
standards-native carriers per Constitution Principle V's
native-first directive.

| Scheme | Semantic | Value form | CDX `externalReferences[].type` | SPDX 2.3 `referenceCategory` | SPDX 3 `Element.externalIdentifier[].externalIdentifierType` |
|---|---|---|---|---|---|
| `repo:` | Source repository identity | URL or git-style ssh URL | `vcs` | `PERSISTENT-ID` | `repo` |
| `git:` | Repo + commit/ref-anchored identity | URL with optional `#<commit-or-ref>` fragment | `vcs` | `PERSISTENT-ID` | `git` |
| `image:` | Image identity | `[registry/]name[:tag][@sha256:digest]` | `distribution` | `PERSISTENT-ID` | `image` |
| `attestation:` | In-toto attestation IRI | URL/IRI | `attestation` | `PERSISTENT-ID` | `attestation` |

### 2.1 Per-scheme validators

Validators are best-effort syntactic checks. A failure does NOT
fail the scan — the identifier soft-fails to `IdentifierKind::User
Defined` (research.md §1) and emits as opaque under the
`mikebom:source-identifiers` annotation. A `tracing::warn!` log
records the validation failure for operator audit.

- **`repo:`** accepts `https://...`, `http://...`, `ssh://...`,
  `git://...`, `git@host:path`, and the general ssh-pseudo
  `<user>@<host>:<path>` shape.
- **`git:`** accepts the same URL shapes as `repo:` plus an optional
  `#<commit-or-ref>` fragment. The fragment SHOULD be a commit SHA /
  branch / tag identifier but isn't validated.
- **`image:`** accepts the canonical Q3 shape:
  `[<registry>/]<name>[:<tag>][@sha256:<digest>]`. Components are
  omittable as documented in §3.2 below.
- **`attestation:`** accepts any RFC 3986 URI shape (any inner
  scheme: `https://...`, `urn:...`, etc.). Whitespace is rejected.

### 2.2 SPDX 2.3 dual-carrier

Per Q2 clarification, SPDX 2.3 uses BOTH a typed primary slot AND a
free-form fallback. Schema-aware consumers (Trivy, syft, sbomqs)
decode the typed primary; consumers that don't walk to the
main-module Package see the free-form text.

- **Typed primary**: main-module Package `externalRefs[]` with
  `referenceCategory: "PERSISTENT-ID"`, `referenceType: <scheme-name>`,
  `referenceLocator: <value>`, optional `comment: <source_label>`.
- **Free-form fallback**: `creationInfo.creators[]` text line
  `"Tool: mikebom-<version> source: <full-identifier>"`. One line per
  built-in identifier.

### 2.3 SPDX 2.3 `referenceType` enum note

SPDX 2.3 spec doesn't enumerate `repo` / `git` / `image` /
`attestation` under the `PERSISTENT-ID` category's `referenceType`
registry. mikebom uses the scheme name as the `referenceType` value
verbatim — consistent with how SPDX 2.3 implementations tolerate
unregistered identifier types under `PERSISTENT-ID` (the category
itself is the typed slot; the `referenceType` value is operator-
defined for non-registered identifiers per the spec's open-
extension posture). External consumers that strictly enforce the
SPDX-registered registry of `referenceType` values may treat these
as `OTHER` — equivalent semantics, different `referenceCategory`.

---

## Section 3 — User-defined schemes (`mikebom:source-identifiers`)

Schemes matching the FR-004 regex but NOT in the built-in registry
(`acme_corp_id:`, `internal_ticket:`, etc.) are treated as
user-defined. They have no native carrier on CDX or SPDX 2.3 — the
specs don't accept arbitrary operator-defined opaque namespaces. Per
Constitution Principle V's documented-exception path, user-defined
identifiers ride a single document-level
`mikebom:source-identifiers` annotation wrapped in milestone-071's
`MikebomAnnotationCommentV1` envelope.

### 3.1 Justification clause (Principle V exception)

The `mikebom:source-identifiers` annotation is the documented
Principle V exception: no standards-native CDX or SPDX 2.3 carrier
accepts arbitrary opaque-namespace identifiers, so user-defined
schemes need a `mikebom:*` carve-out. SPDX 3's open-typed
`Element.externalIdentifier[]` model handles BOTH built-in and
user-defined identifiers natively, so the annotation is intentionally
omitted on the SPDX 3 side.

### 3.2 SPDX 3 native carrier

User-defined identifiers on SPDX 3 ride the SAME native
`Element.externalIdentifier[]` carrier as built-in identifiers.
SPDX 3's open-typed model means consumers can decode either set
uniformly without distinguishing. This is a structural advantage
of SPDX 3 over SPDX 2.3 for opaque-namespace identifiers.

### 3.3 Annotation envelope shape

```json
{
  "schema": "mikebom-annotation/v1",
  "field": "mikebom:source-identifiers",
  "value": [
    { "scheme": "acme_corp_id", "value": "abc123" },
    { "scheme": "internal_ticket", "value": "PROJ-456" }
  ]
}
```

The `value` array is sorted lexicographically by `(scheme, value)`
for determinism (FR-009 / contract C-4). Entries do NOT carry a
`source_label` field — manual flags don't have one and user-defined
auto-detection isn't a concept today.

### 3.4 CDX envelope wrapping

CDX 1.6 `metadata.properties[].value` is a string-typed slot. The
envelope is JSON-encoded into a string:

```json
{
  "metadata": {
    "properties": [
      {
        "name": "mikebom:source-identifiers",
        "value": "[{\"scheme\":\"acme_corp_id\",\"value\":\"abc123\"}]"
      }
    ]
  }
}
```

Consumers parse the `value` string via `JSON.parse(...)` to recover
the array. This shape mirrors the milestone-071 envelope precedent.

### 3.5 SPDX 2.3 envelope wrapping

SPDX 2.3 document-level `annotations[]` use the
`MikebomAnnotationCommentV1` envelope (milestone 071). The envelope
JSON lives inside `comment` as a string:

```json
{
  "annotations": [
    {
      "annotator": "Tool: mikebom-0.1.0-alpha.16",
      "annotationDate": "2026-05-05T12:00:00Z",
      "annotationType": "OTHER",
      "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:source-identifiers\",\"value\":[{\"scheme\":\"acme_corp_id\",\"value\":\"abc123\"}]}"
    }
  ]
}
```

Same parse rule: extract the `comment` string, `JSON.parse(...)`,
walk `value`.

---

## Section 4 — Auto-detection

mikebom auto-detects identifiers in two cases. Auto-detection is
"best-effort, never failing" — when detection can't fire (no git
remote, no resolved image), the scan emits without the auto-detected
identifier and a `tracing::info!` log records why.

### 4.1 `repo:` from `--path` scans (3-step git-remote fallback)

When the scan root is a git checkout (has `.git/` directory),
mikebom runs `git remote get-url <name>` with a 3-step name
fallback per Q1 clarification:

1. **`origin`** — try this first. Most common case.
2. **`upstream`** — fall back when `origin` is absent. Conventional
   fork-parent name.
3. **First-listed** — fall back when neither of the above is
   configured. `git remote` output is parsed alphabetically; the
   first non-`origin`, non-`upstream` remote is selected.

The chosen remote name is recorded in the standards-native carrier's
`comment` / `source_label` field for transparency (FR-007). The
emitted identifier looks like:

```json
{
  "type": "vcs",
  "url": "git@github.com:acme/foo.git",
  "comment": "auto-detected from git remote `origin`"
}
```

When the third-step (first-listed) fallback fires, the comment
suffix `(origin/upstream absent; first-listed)` is appended.

### 4.2 `image:` from `--image` scans (canonical Q3 shape)

Image-tier scans synthesize an `image:<registry>/<name>:<tag>@sha256:<digest>`
identifier from the resolved image reference. Components are
omitted when absent:

| Available components | Emitted shape | Use case |
|---|---|---|
| All four | `image:<registry>/<name>:<tag>@sha256:<digest>` | Registry pull (full canonical form) |
| No registry | `image:<name>@sha256:<digest>` or `image:<name>:<tag>@sha256:<digest>` | Tarball-loaded image without registry context |
| No digest | `image:<registry>/<name>:<tag>` | Pre-distribution-spec images |

The emitted carrier comment is `"auto-detected from resolved image
reference"`.

### 4.3 Manual override semantics (FR-006)

When auto-detection AND a manual `--with-source <scheme>:<value>`
flag both produce an identifier:

- **Same `(scheme, value)`** → deduplicated. Manual entry inherits
  the auto-detected entry's position in the emitted Vec (front-of-
  list); auto-detected `source_label` is replaced. An `info`-level
  log notes the dedup.
- **Same scheme, different value** → manual override wins.
  Auto-detected entry is dropped (collapsed away); manual entry
  follows in supply order (NOT promoted to front-of-list per the
  FR-006 override-position rule). Both URLs logged at info level.
- **Different scheme** → no override. Both identifiers emit.

Build-tier scans (`mikebom trace`) do NOT auto-detect — manual
flags only per FR-008. The build-tier path is opaque to mikebom's
eBPF observability so there's no analog of the `--path` git remote
or `--image` resolved reference auto-detection.

---

## Section 5 — Determinism contract

Per FR-009: byte-identical scan inputs produce byte-identical
identifier carrier output across runs. Implementation rules:

1. **Built-in identifier order**: auto-detected entries first (in
   detection order), then manual `--with-source` entries in supply
   order. The CDX `externalReferences[]`, SPDX 2.3 main-module
   `Package.externalRefs[]`, SPDX 2.3 `creationInfo.creators[]`,
   and SPDX 3 `Element.externalIdentifier[]` arrays all follow this
   order.
2. **Override-position rule**: when a manual entry deduplicates
   against an auto-detected entry on `(scheme, value)`, the manual
   entry inherits the auto-detected position. When manual differs
   in value (true override), auto-detected is dropped and manual
   follows in supply order — NOT promoted.
3. **Dedup**: by exact `(scheme, value)` match. Manual-vs-manual
   collisions resolve to first-supplied wins.
4. **User-defined annotation order**: the `mikebom:source-identifiers`
   `value` array is sorted lexicographically by `(scheme, value)`
   before serialization (annotations have unordered semantics; lex
   sort gives a stable serialization).
5. **Empty user-defined set**: the `mikebom:source-identifiers`
   annotation is OMITTED entirely when no user-defined identifiers
   are present (VR-007). Preserves cross-format byte-identity for
   non-user-defined-namespace scans.

---

## Section 6 — Runnable decode recipes

External consumers can extract identifiers without mikebom source-
code access using standard JSON tooling.

### 6.1 CDX 1.6 — `jq`

```bash
jq '
{
  builtin: ([.metadata.component.externalReferences[]?
              | select(.type == "vcs" or .type == "distribution" or .type == "attestation")
              | {scheme: (if .type == "vcs" then "repo"
                          elif .type == "distribution" then "image"
                          else "attestation" end),
                 value: .url,
                 comment}]),
  user_defined: ([.metadata.properties[]?
                   | select(.name == "mikebom:source-identifiers")
                   | .value | fromjson] | flatten)
}
' /tmp/out.cdx.json
```

### 6.2 SPDX 2.3 — `jq`

```bash
jq '
{
  builtin: ([.packages[]?.externalRefs[]?
              | select(.referenceCategory == "PERSISTENT-ID")
              | {scheme: .referenceType,
                 value: .referenceLocator,
                 comment}]),
  user_defined: ([.annotations[]?
                   | .comment | fromjson?
                   | select(.field == "mikebom:source-identifiers")
                   | .value] | flatten)
}
' /tmp/out.spdx.json
```

### 6.3 SPDX 3.0.1 — `jq`

```bash
jq '
{
  identifiers: ([."@graph"[]?
                  | select(.type == "SpdxDocument")
                  | .externalIdentifier[]?
                  | {scheme: .externalIdentifierType,
                     value: .identifier,
                     comment}])
}
' /tmp/out.spdx3.json
```

SPDX 3's open-typed model carries BOTH built-in and user-defined
identifiers in a single uniform `externalIdentifier[]` array.
External consumers that need to distinguish can filter on
`scheme in ["repo", "git", "image", "attestation"]`.

### 6.4 Python equivalent

```python
import json

def extract_cdx(doc):
    builtin = []
    refs = doc.get("metadata", {}).get("component", {}).get("externalReferences", [])
    for r in refs:
        ty = r.get("type")
        if ty in ("vcs", "distribution", "attestation"):
            scheme = {"vcs": "repo", "distribution": "image",
                      "attestation": "attestation"}[ty]
            builtin.append({"scheme": scheme, "value": r.get("url"),
                            "comment": r.get("comment")})
    user_defined = []
    for p in doc.get("metadata", {}).get("properties", []):
        if p.get("name") == "mikebom:source-identifiers":
            for entry in json.loads(p.get("value", "[]")):
                user_defined.append(entry)
    return {"builtin": builtin, "user_defined": user_defined}

def extract_spdx23(doc):
    builtin = []
    for pkg in doc.get("packages", []):
        for r in pkg.get("externalRefs", []):
            if r.get("referenceCategory") == "PERSISTENT-ID":
                builtin.append({
                    "scheme": r.get("referenceType"),
                    "value": r.get("referenceLocator"),
                    "comment": r.get("comment"),
                })
    user_defined = []
    for a in doc.get("annotations", []):
        try:
            envelope = json.loads(a.get("comment", ""))
        except json.JSONDecodeError:
            continue
        if envelope.get("field") == "mikebom:source-identifiers":
            user_defined.extend(envelope.get("value", []))
    return {"builtin": builtin, "user_defined": user_defined}

def extract_spdx3(doc):
    identifiers = []
    for el in doc.get("@graph", []):
        if el.get("type") != "SpdxDocument":
            continue
        for i in el.get("externalIdentifier", []):
            identifiers.append({
                "scheme": i.get("externalIdentifierType"),
                "value": i.get("identifier"),
                "comment": i.get("comment"),
            })
    return identifiers
```

The same data is extractable from all three formats; the per-format
shape differs but the canonical `(scheme, value)` payload is
preserved.

---

## Section 7 — Stability commitment

- The 4 built-in schemes (`repo:`, `git:`, `image:`, `attestation:`)
  are stable across mikebom alpha versions post-073.
- The FR-004 scheme regex (`^[a-z][a-z0-9_-]*$`) is stable. Future
  schemes that don't match the regex (e.g., uppercase) require a
  contract-level change.
- New built-in schemes MAY be added in future milestones without
  breaking compat. User-defined schemes that collide with future
  built-ins migrate at the registration milestone (operators are
  warned).
- The `image:` canonical Q3 shape is stable. Future image-reference
  conventions (e.g., OCI 1.x vs 2.x) accommodate via the validator's
  permissive regex; the emit-side keeps the documented shape.
- The `mikebom:source-identifiers` envelope shape (JSON array of
  `{scheme, value}` objects) is stable for `schema: "mikebom-
  annotation/v1"`. Future fields are skip_serializing_if-gated; new
  envelope versions bump the `schema` value.
- The C47 parity-catalog row directionality is `SymmetricEqual`.
  Future user-defined schemes don't change the directionality.

---

## Section 8 — Forward pointer to milestone 074

This milestone (073) emits identifiers at scan time. Milestone 074's
`mikebom sbom scan --image foo:v1 --bind-to-source <identifier>`
will resolve identifier-keyed lookups against a local SBOM directory
to find the source SBOM that carries the matching identifier.

The forward-looking contract: every identifier emitted at document
level via this milestone's mechanisms is parseable, deterministic,
and survives JSON canonicalization — the bare-minimum properties a
074-style resolver needs. The `(scheme, value)` extraction recipes
above are exactly what 074's resolution layer will run against
candidate source SBOMs to find a match.

This milestone (073) lays the foundation. Milestone 074 will not
need to change emission-side code — it consumes what's already here.

---

## See also

- [Cross-tier binding (milestone 072)](cross-tier-binding.md) — the
  per-component cross-tier identity / verifier flow. Source
  identifiers and source-document bindings are sibling concerns.
- [Conformance harness guide (milestone 071)](conformance-harness-guide.md)
  — per-format envelope-decode rules and the 7 inherent format-spec
  asymmetries. Background reading for new mikebom emission
  consumers.
- [Cross-format SBOM mapping](sbom-format-mapping.md) — the
  authoritative catalog of every cross-format datum mikebom emits.
  Search `C47` for the `mikebom:source-identifiers` row.
