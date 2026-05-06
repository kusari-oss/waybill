# Identifiers — external SBOM consumer guide

**Audience**: maintainers of external SBOM consumer / verifier tools
that read mikebom-emitted CycloneDX 1.6, SPDX 2.3, or SPDX 3.0.1
SBOMs and want to extract the document-level identifiers attached at
scan time (`repo:`, `git:`, `image:`, `attestation:`, plus arbitrary
operator-defined opaque schemes). Covers the wire format, per-format
carrier shapes, auto-detection paths, the determinism contract, and
runnable `jq` decode recipes — everything an external implementer
needs to write a working extractor from this document alone.

**Status**: written 2026-05-05 against mikebom v0.1.0-alpha.16
(milestone 073), updated for milestone 074 (build-tier identifier
auto-detection, this milestone). Reflects the post-073 dedicated-flag
CLI surface and the milestone-074 build-tier auto-detection
symmetry with source-tier.

**Naming note**: this document was originally drafted as
"source-identifiers" — anchoring on the most common case (source
repos). The same mechanism handles image / attestation /
user-defined identifiers, so the name was generalized to "identifier"
before the milestone shipped. SPDX 3 already uses
`Element.externalIdentifier[]` for the same concept. Milestone-072's
`SourceDocumentBinding` is a DIFFERENT, sibling concept (binding
back to a source-tier SBOM document) and intentionally retains its
name — see `cross-tier-binding.md`.

**Companion documents**:

- `docs/reference/cross-tier-binding.md` — milestone-072 cross-tier
  binding guide (binding hash, per-component verifier flow, VEX
  propagation modes). Identifiers and source-document
  bindings are sibling concerns: identifiers carry stable identity;
  bindings carry per-component cross-tier provenance.
- `docs/reference/conformance-harness-guide.md` — milestone-071
  per-format envelope-decode rules. Read first if you're new to
  mikebom's emission model.
- `specs/073-source-identifiers/contracts/` — the source contracts
  this guide externalizes. The contracts are authoritative; this
  guide is the operator-facing presentation.

---

## Section 1 — Wire format & CLI surface

An identifier is a `(scheme, value)` pair. mikebom emits identifiers
into per-format carriers (Section 2 / 3) where the canonical
representation is structured (e.g., CDX `externalReferences[]` rows,
SPDX 3 `externalIdentifier[]` entries) — there is no single
on-the-wire string format. CLI input uses dedicated flags per
built-in scheme + a generic `--id` flag for user-defined schemes.

### 1.1 CLI surface

| Flag | Built-in scheme | Notes |
|---|---|---|
| `--repo <url>` | `repo:` | Source repository identity (URL or git-style ssh URL). Manual override; auto-detection from `.git/origin` runs by default on `--path` scans and is overridden by this flag per FR-006. |
| `--git-ref <revision>` | `git:` (with `--repo`) | Pairs with `--repo`; emits `git:<repo>#<revision>` and supersedes the bare `repo:` identifier. Cannot be supplied without `--repo` (clap-enforced). |
| `--image-id <ref>` | `image:` | Image identity in the form `[registry/]name[:tag][@sha256:digest]`. Manual override on `--image <PATH>` mode (where auto-detection from the resolved image reference also fires). Named `--image-id` to avoid colliding with the existing `--image <PATH>` scan-input flag. |
| `--attestation <iri>` | `attestation:` | In-toto attestation IRI. Manual only; no auto-detection. |
| `--id <scheme>=<value>` (repeatable) | n/a — user-defined namespaces ONLY | `<scheme>` matches regex `^[a-z][a-z0-9_-]*$` (FR-004); `<value>` is everything after the first `=`. Built-in scheme names (`repo`, `git`, `image`, `attestation`) are REJECTED here at clap parse time with a message pointing at the dedicated flag. |

The same flag set applies to `mikebom sbom scan --path`,
`mikebom sbom scan --image`, and `mikebom trace run`.

### 1.2 Worked examples

```bash
# Source-tier scan with explicit repo override (auto-detect would
# normally produce a repo: identifier from the .git/origin remote;
# this flag wins per FR-006).
mikebom sbom scan --path . --repo git@github.com:acme/foo.git

# Source-tier with git-ref → git: identifier (NOT a repo: identifier).
mikebom sbom scan --path . \
    --repo https://github.com/acme/foo.git \
    --git-ref abc1234567890

# User-defined identifiers ride the mikebom:identifiers annotation.
mikebom sbom scan --path . \
    --id acme_corp_id=svc-alpha-123 \
    --id internal_ticket=PROJ-456

# Image-tier with manual override + user-defined identifier.
mikebom sbom scan --image foo.tar \
    --image-id docker.io/acme/foo:v1@sha256:abc... \
    --id acme_corp_id=svc-alpha-123

# Build-tier (trace) — milestone 074: when invoked in a git
# checkout, `mikebom trace run` auto-detects `repo:` from the same
# git remote source-tier picks up, AND additionally a commit-anchored
# `git:<repo-url>#<sha>` identifier from `git rev-parse HEAD`. No
# flags required — symmetric with source-tier.
mikebom trace run -- ./build.sh

# Build-tier with manual override — manual flags win, same as
# source-tier (FR-004).
mikebom trace run --repo git@github.com:acme/override.git -- ./build.sh
```

### 1.3 Migration from the pre-073 `--with-source` flag

The original milestone-073 implementation shipped a single
`--with-source <scheme>:<value>` flag. Before milestone 073 was
merged, the CLI was refactored to dedicated flags per built-in scheme
+ a generic `--id` for user-defined schemes. The reasons:

1. **`<scheme>:<value>` was visually ambiguous when values contained
   colons** — `repo:git@github.com:foo/bar.git`,
   `image:foo:v1@sha256:abc...`. First-`:`-split was mechanically
   correct but operator-hostile.
2. **Dedicated flags are self-documenting** — `mikebom sbom scan
   --help` shows the 4 built-in schemes by name without the operator
   needing to read prose.

There is no compatibility shim — `--with-source` is gone. Pipelines
that used it must update to the dedicated flags before upgrading
past alpha.15.

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
`mikebom:identifiers` annotation. A `tracing::warn!` log
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

## Section 3 — User-defined schemes (`mikebom:identifiers`)

Schemes matching the FR-004 regex but NOT in the built-in registry
(`acme_corp_id:`, `internal_ticket:`, etc.) are treated as
user-defined. They have no native carrier on CDX or SPDX 2.3 — the
specs don't accept arbitrary operator-defined opaque namespaces. Per
Constitution Principle V's documented-exception path, user-defined
identifiers ride a single document-level `mikebom:identifiers`
annotation wrapped in milestone-071's `MikebomAnnotationCommentV1`
envelope.

### 3.1 Justification clause (Principle V exception)

The `mikebom:identifiers` annotation is the documented Principle V
exception: no standards-native CDX or SPDX 2.3 carrier accepts
arbitrary opaque-namespace identifiers, so user-defined schemes need
a `mikebom:*` carve-out. SPDX 3's open-typed
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
  "field": "mikebom:identifiers",
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
        "name": "mikebom:identifiers",
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
      "comment": "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:identifiers\",\"value\":[{\"scheme\":\"acme_corp_id\",\"value\":\"abc123\"}]}"
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

When auto-detection AND a manual identifier flag both produce an
identifier:

- **Same `(scheme, value)`** → deduplicated. Manual entry inherits
  the auto-detected entry's position in the emitted Vec (front-of-
  list); auto-detected `source_label` is replaced. An `info`-level
  log notes the dedup.
- **Same scheme, different value** → manual override wins.
  Auto-detected entry is dropped (collapsed away); manual entry
  follows in supply order (NOT promoted to front-of-list per the
  FR-006 override-position rule). Both URLs logged at info level.
- **Different scheme** → no override. Both identifiers emit.

The dedicated `--repo` / `--image-id` flags participate in this
logic the same way the old `--with-source` flag did — the
resolution pipeline operates on `(scheme, value)` pairs after the
flag-translation step.

### 4.4 Build-tier auto-detection (milestone 074)

Build-tier scans (`mikebom trace run`) auto-detect symmetrically with
source-tier. When the invocation cwd is a git checkout, mikebom
emits up to TWO auto-detected identifiers per invocation:

1. A `repo:` identifier — same 3-step git-remote fallback algorithm
   as §4.1 (origin → upstream → first-listed).
2. A `git:<repo-url>#<sha>` identifier — uses the SAME remote URL
   selected for `repo:`, plus the full 40-character SHA returned by
   `git rev-parse HEAD` in the invocation cwd.

The `git:` identifier is build-tier-specific. Source-tier `--path`
scans don't naturally carry a commit anchor (a working tree may have
uncommitted changes), but build-tier scans almost always correspond
to a specific HEAD commit — that's the deciding piece of metadata for
correlating "this image was built from commit X."

The `source_label` strings are distinguishable from source-tier:

```text
# Source-tier (§4.1)
"auto-detected from git remote `origin`"

# Build-tier (milestone 074, this section)
"auto-detected from build-tier git remote `origin`"
"auto-detected from build-tier `git rev-parse HEAD`"
```

The `build-tier` substring lets a consumer scanning a flat list of
identifiers (e.g., `jq` over emitted `externalReferences[]`) tell
which tier the entry came from without having to consult the
surrounding `mikebom:sbom-tier` annotation.

When the invocation cwd is not a git checkout, OR has no remotes
configured, OR `git rev-parse HEAD` fails (e.g., a freshly initialized
repo with no commits yet), the affected identifier(s) are silently
skipped with `tracing::info!` log lines. The scan does not fail.
This mirrors the source-tier soft-fail behavior (§4.1).

Manual flags override per the same FR-006 rules in §4.3 — passing
`--repo` overrides only the auto-detected `repo:` (the auto-detected
`git:` still emits with the original auto-detected URL); passing
`--git-ref` overrides only `git:` (the auto-detected `repo:` is
unaffected). Passing `--repo <url> --git-ref <rev>` together emits
a single `git:<url>#<rev>` identifier and overrides both auto-detected
entries' schemes.

### 4.5 Cross-tier correlation recipe (milestone 074)

The headline use case milestone 074 unlocks: an operator (or external
tool) holding all three tier SBOMs — source from `mikebom sbom scan
--path`, build from `mikebom trace run`, image from
`mikebom sbom scan --image` — can correlate them by reading
identifier fields directly. No mikebom-side resolver, no index, no
external registry call.

```bash
# Three scans, all from the same git checkout at the same commit.
cd ~/projects/my-rust-app

mikebom sbom scan --path . --format cyclonedx-json \
    --output cyclonedx-json=/tmp/source.cdx.json
mikebom trace run -- ./build.sh
mikebom sbom scan --image my-app:v1 --format cyclonedx-json \
    --output cyclonedx-json=/tmp/image.cdx.json

# Source `repo:` value
jq -r '.metadata.component.externalReferences[]
       | select(.type == "vcs") | .url' /tmp/source.cdx.json
# git@github.com:acme/my-rust-app.git

# Build `repo:` matches source byte-for-byte (SC-002).
jq -r '.metadata.component.externalReferences[]
       | select(.type == "vcs") | .url' /tmp/build.cdx.json | head -1
# git@github.com:acme/my-rust-app.git

# Build `git:` carries the commit-of-record SHA.
jq -r '.metadata.component.externalReferences[]
       | select(.type == "vcs") | .url' /tmp/build.cdx.json | tail -1
# git@github.com:acme/my-rust-app.git#abc1234567890abcdef1234567890abcdef1234

git -C ~/projects/my-rust-app rev-parse HEAD
# abc1234567890abcdef1234567890abcdef1234
```

The cross-tier story: the image was built from the build-tier SBOM
whose `git:` identifier records commit `abc1234...`, and that commit
lives in the same `repo:` whose source SBOM the operator has on file.
Three SBOMs, three identifier slots, one consistent correlation.
Future milestones may automate the correlation; this milestone lays
the foundation by making every tier emit its identifiers
automatically.

### 4.6 Credential sanitization (milestone 075)

When the discovered remote URL carries RFC 3986 userinfo
(`<user>[:<password>]@host` — common in CI runners using GitHub App
tokens or HTTPS deploy tokens, e.g., `https://x-access-token:ghs_AAA
@github.com/foo.git`), mikebom strips the userinfo before the URL
becomes an identifier value. SBOMs are typically published artifacts
(release attachments, OCI registry referrers, signed attestations);
the strip prevents accidental token disclosure.

Sanitization fires on auto-detected URLs in BOTH tiers:

- Source-tier `repo:` identifier (§4.1).
- Build-tier `repo:` and `git:` identifiers (§4.4). For `git:`, the
  URL portion is sanitized BEFORE the `#<sha>` is appended, so a
  credentialed `https://USER:TOKEN@github.com/foo.git` at HEAD
  `abc1234...` emits as `git:https://github.com/foo.git#abc1234...`.

When sanitization fires, the `source_label` is augmented with the
suffix ` (credentials stripped)`:

```text
"auto-detected from git remote `origin` (credentials stripped)"
"auto-detected from build-tier git remote `origin` (credentials stripped)"
"auto-detected from build-tier `git rev-parse HEAD` (credentials stripped)"
```

An info-level log line is emitted per sanitized identifier with the
userinfo replaced by the literal `<userinfo redacted>` placeholder
so operators can audit the action without the actual credential
appearing in the log.

#### What gets stripped vs not

| Input URL | Auto-detect default | With `--keep-credentials-in-identifiers` |
|---|---|---|
| `https://USER:TOKEN@github.com/foo.git` | `https://github.com/foo.git` | unchanged (verbatim) |
| `https://TOKEN@github.com/foo.git` | `https://github.com/foo.git` | unchanged (verbatim) |
| `https://github.com/foo.git` (no userinfo) | unchanged | unchanged |
| `git@github.com:foo/bar.git` (SSH form) | unchanged (no userinfo per RFC 3986) | unchanged |
| `git://github.com/foo.git` | unchanged (no userinfo present) | unchanged |
| `git://USER:TOKEN@github.com/foo.git` | `git://github.com/foo.git` | unchanged (verbatim) |

SSH-form URLs (`git@host:path` — the SCP-like syntax) carry no
userinfo by construction; the `git@` is a fixed SSH username, not a
credential. mikebom passes them through unchanged in both modes.

#### Manual flags emit verbatim

Manual `--repo`, `--git-ref`, `--image-id`, `--attestation`, and
`--id <scheme>=<value>` flag values are NOT sanitized — operators
who explicitly type credentialed URLs are responsible for their
choice (FR-004). The boundary is "auto-detected = sanitized;
manual = verbatim", and it applies regardless of the
`--keep-credentials-in-identifiers` flag value.

#### Opt-out flag

Operators on private/internal-network setups where the credentials
are deliberately non-sensitive (e.g., a public read-only deploy
token, an internal-network-only HTTPS token with no value outside
the corporate VPN) can preserve userinfo via:

```bash
mikebom sbom scan --path . --keep-credentials-in-identifiers
mikebom trace run --keep-credentials-in-identifiers -- ./build.sh
```

When set, the flag suppresses sanitization for all auto-detected
identifiers in the scan. mikebom emits one info-level log line
acknowledging the suppression so the audit trail records the
operator's choice. The `(credentials stripped)` suffix does NOT
appear on `source_label`s in this mode.

#### Cross-tier picture

The cross-tier correlation recipe in §4.5 stays valid post-075: the
sanitized URL is the canonical correlation key. A downstream tool
matching SBOMs by `repo:` value gets identical canonical forms
across tiers regardless of which side originally observed
credentials. See `specs/075-strip-id-credentials/quickstart.md`
Recipe 5 for a worked end-to-end example.

---

## Section 5 — Determinism contract

Per FR-009: byte-identical scan inputs produce byte-identical
identifier carrier output across runs. Implementation rules:

1. **Built-in identifier order**: auto-detected entries first (in
   detection order), then manual flag entries in supply order
   (`--repo` / `--git-ref` first, then `--image-id`, then
   `--attestation`, then each `--id` in invocation order). The CDX
   `externalReferences[]`, SPDX 2.3 main-module
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
4. **User-defined annotation order**: the `mikebom:identifiers`
   `value` array is sorted lexicographically by `(scheme, value)`
   before serialization (annotations have unordered semantics; lex
   sort gives a stable serialization).
5. **Empty user-defined set**: the `mikebom:identifiers`
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
                   | select(.name == "mikebom:identifiers")
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
                   | select(.field == "mikebom:identifiers")
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
        if p.get("name") == "mikebom:identifiers":
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
        if envelope.get("field") == "mikebom:identifiers":
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
- The dedicated CLI flags (`--repo`, `--git-ref`, `--image-id`,
  `--attestation`, `--id`) are stable. New built-in schemes added
  in future milestones will receive their own dedicated flag and
  WILL be added to the `--id` rejection list at the same time.
  User-defined schemes that collide with future built-ins migrate
  at the registration milestone (operators are warned).
- The `image:` canonical Q3 shape is stable. Future image-reference
  conventions (e.g., OCI 1.x vs 2.x) accommodate via the validator's
  permissive regex; the emit-side keeps the documented shape.
- The `mikebom:identifiers` envelope shape (JSON array of
  `{scheme, value}` objects) is stable for `schema: "mikebom-
  annotation/v1"`. Future fields are skip_serializing_if-gated; new
  envelope versions bump the `schema` value.
- The C47 parity-catalog row directionality is `SymmetricEqual`.
  Future user-defined schemes don't change the directionality.

---

## Section 8 — Milestone 074 status

Milestone 074 (this milestone) closes the symmetry gap between
source-tier, image-tier, and build-tier auto-detection. Build-tier
scans now auto-detect `repo:` and `git:` identifiers from the
invocation cwd's git state — see §4.4 above and §4.5's cross-tier
correlation recipe.

A future milestone will automate the cross-tier correlation itself
(via a local SBOM index, OCI referrers, or external-registry
resolvers — exact mechanism is yet to be scoped). The cross-tier
identifier byte-equality this milestone guarantees is the deciding
substrate that future milestone consumes.

---

## See also

- [Cross-tier binding (milestone 072)](cross-tier-binding.md) — the
  per-component cross-tier identity / verifier flow. Identifiers
  and source-document bindings are sibling concerns.
- [Conformance harness guide (milestone 071)](conformance-harness-guide.md)
  — per-format envelope-decode rules and the 7 inherent format-spec
  asymmetries. Background reading for new mikebom emission
  consumers.
- [Cross-format SBOM mapping](sbom-format-mapping.md) — the
  authoritative catalog of every cross-format datum mikebom emits.
  Search `C47` for the `mikebom:identifiers` row.
