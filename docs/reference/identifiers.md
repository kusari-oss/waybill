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
| `repo:` | Source repository identity | URL or git-style ssh URL | `vcs` | `PERSISTENT-ID` | `other` (with `comment: "original-scheme: repo"`) |
| `git:` | Repo + commit/ref-anchored identity | URL with optional `#<commit-or-ref>` fragment | `vcs` | `PERSISTENT-ID` | `other` (with `comment: "original-scheme: git"`) — value matching `^[0-9a-f]{40}$` SHA-1 maps to `gitoid` instead, no `comment` |
| `image:` | Image identity | `[registry/]name[:tag][@sha256:digest]` | `distribution` | `PERSISTENT-ID` | `other` (with `comment: "original-scheme: image"`) |
| `attestation:` | In-toto attestation IRI | URL/IRI | `attestation` | `PERSISTENT-ID` | `other` (with `comment: "original-scheme: attestation"`) |
| `subject:` | Build-tier content-hash subject | `<algo>:<hex>` | `attestation` | `PERSISTENT-ID` | `other` (with `comment: "original-scheme: subject"`) |

The SPDX 3 column reflects the milestone-079 mapping. SPDX 3
defines `Core/externalIdentifierType` as a controlled vocabulary
(`[other, cve, swhid, securityOther, cpe23, packageUrl, gitoid,
cpe22, urlScheme, email, swid]`) — none of mikebom's built-in
scheme names are in that set, so they map to `other` with the
original scheme preserved in the `comment` field. See §6.3.2 below
for the wire-mapping reference and §6.3.1 for milestone-078's
SPDX 3 CreationInfo fix that landed in the same conformance
campaign.

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

### 6.3.1 SPDX 3.0.1 — CreationInfo wire-shape note (milestone 078)

mikebom v0.1.0-alpha.16 through v0.1.0-alpha.18 (milestone-073 →
milestone-077) emitted SPDX 3 `CreationInfo.createdBy[]` referencing
the `Tool` element directly, which trips the SPDX 3 SHACL constraint
`Core/createdBy` (range = `Core/Agent`; `Tool` is a sibling class,
not a subclass). Post-milestone-078 emission corrects this:

- `CreationInfo.createdBy[0]` now references an `Organization`
  element with `name: "mikebom contributors"` (Agent subclass —
  satisfies SHACL).
- `CreationInfo.createdUsing[0]` is a NEW field referencing the
  same Tool element pre-fix emission carried in `createdBy`. The
  Tool element's identity (spdxId, name) is unchanged.
- `SpdxDocument.dataLicense` now resolves (within `@graph`) to a
  `simplelicensing_LicenseExpression` element (with field
  `simplelicensing_licenseExpression: "CC0-1.0"`) at IRI
  `https://spdx.org/licenses/CC0-1.0`, satisfying the SHACL
  constraint `Core/dataLicense` (range = `SimpleLicensing/
  AnyLicenseInfo`).

External consumers walking SPDX 3 can extract the publisher from
`@graph[type=Organization].name` and the producing tool from
`@graph[type=Tool].name`. Operators wishing to verify the SBOM
passes external SPDX 3 validators can install
`scripts/install-spdx3-validate.sh` (the JPEWdev `spdx3-validate`
Python tool, version-pinned per milestone-078 research) and run:

```bash
.venv/spdx3-validate/bin/spdx3-validate -j out.spdx3.json
```

The underlying SHACL validation rules trace back to
[the official SPDX 3 JSON-LD validation reference](https://github.com/spdx/spdx-3-model/blob/develop/serialization/jsonld/validation.md);
operators with bespoke conformance harnesses should use those rules
as the source of truth.

### 6.3.2 SPDX 3.0.1 — externalIdentifierType wire-mapping (milestone 079)

mikebom v0.1.0-alpha.16 through v0.1.0-alpha.19 (milestones 073/074/076/078)
emitted internal scheme names (`image`, `repo`, `git`, `subject`,
`attestation`) directly into SPDX 3's
`Core/externalIdentifierType` field. SPDX 3 defines that field as a
SHACL-enumerated controlled vocabulary with exactly 11 allowed
values: `[other, cve, swhid, securityOther, cpe23, packageUrl,
gitoid, cpe22, urlScheme, email, swid]`. None of mikebom's
internal scheme names are in that set, so JPEWdev's
`spdx3-validate` flagged every such SBOM as non-conformant
(GitHub issue [#154](https://github.com/kusari-oss/mikebom/issues/154)).

Milestone 079 corrects the emission per the table below. This
corrects the alpha.16-alpha.19 emission shape (closes GitHub issue
#154). Runs side-by-side with milestone 078's CreationInfo fix
(§6.3.1) — both ship together as the SPDX 3 conformance pass.

| Input scheme | Input value shape | Output `externalIdentifierType` | Output `comment` |
|---|---|---|---|
| `image` | any | `other` | `"original-scheme: image"` |
| `repo` | any | `other` | `"original-scheme: repo"` |
| `git` | matches `^[0-9a-f]{40}$` (SHA-1) | `gitoid` | (omitted) |
| `git` | does NOT match the regex | `other` | `"original-scheme: git"` |
| `subject` | any | `other` | `"original-scheme: subject"` |
| `attestation` | any | `other` | `"original-scheme: attestation"` |
| `<vocab>` (e.g., `cve`, `cpe23`, `gitoid`, etc.) | any | the vocab value verbatim | (omitted) |
| `<non-vocab user-defined>` (e.g., `jira`, `internal-ticket`) | any | `other` | `"original-scheme: <name>"` |

**Recovery**: operators wanting to filter by the original mikebom
scheme can read the `comment` field's `original-scheme: ` prefix:

```bash
jq '
[."@graph"[]?
  | select(.type == "SpdxDocument")
  | .externalIdentifier[]?
  | select(.comment != null and (.comment | startswith("original-scheme: ")))
  | {scheme: (.comment | sub("^original-scheme: "; "")),
     value: .identifier,
     vocab_type: .externalIdentifierType}]
' out.spdx3.json
```

CDX 1.6 + SPDX 2.3 emission paths use independent vocabularies
(CDX `externalReferences[].type`, SPDX 2.3 `externalRefs[].
referenceCategory`/`referenceType`) and are not affected by this
milestone — those formats continue to emit the internal scheme
names as before.

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

Milestone 074 closes the symmetry gap between source-tier, image-tier,
and build-tier auto-detection. Build-tier scans now auto-detect
`repo:` and `git:` identifiers from the invocation cwd's git state —
see §4.4 above and §4.5's cross-tier correlation recipe.

A future milestone will automate the cross-tier correlation itself
(via a local SBOM index, OCI referrers, or external-registry
resolvers — exact mechanism is yet to be scoped). The cross-tier
identifier byte-equality this milestone guarantees is the deciding
substrate that future milestone consumes.

---

## Section 9 — Milestone 076: subject identifier scheme + per-component identifiers

Milestone 076 closes the cross-tier content-addressable correlation
chain by adding two operator-visible features:

1. **`subject:` document-level identifier scheme** (fifth built-in).
2. **Per-component user-defined identifiers** via a new
   `--component-id <PURL>=<scheme>:<value>` flag.

### 9.1 `subject:` identifier scheme

`subject:<algo>:<hex>` declares "this SBOM describes the artifact
with the given content hash." Allowed algos: `sha256` (64 lowercase
hex chars), `sha512` (128 lowercase hex chars). Other algos and
mixed/uppercase hex soft-fail to `IdentifierKind::UserDefined` per
FR-005.

**Auto-detection** (build-tier only): on `mikebom trace run`, the
trace's in-toto attestation envelope captures `subject[]` entries
with digest maps. mikebom emits one `subject:sha256:<hex>` identifier
per subject that has a sha256 digest in its map, in input order.
Subjects without sha256 are skipped with `tracing::info!`. Multi-digest
subjects (sha256 AND sha512) auto-emit sha256 only — the 2026-05-06
clarification. Operators who need other algos pass
`--subject-hash sha512:<hex>` manually.

**Manual** (any tier): `--subject-hash <ALGO>:<HEX>` is repeatable on
both `mikebom sbom scan` and `mikebom trace run`. Manual values
augment auto-detected entries (deduplicated by `(scheme, value)` per
milestone 073's resolution pipeline).

**Per-format wire mapping**:

| Format | Carrier | Shape |
|---|---|---|
| CDX 1.6 | `metadata.component.externalReferences[]` | `{type:"attestation", url:"sha256:<hex>", comment:"..."}` |
| SPDX 2.3 | `Package.externalRefs[]` on main-module + `creationInfo.creators[]` redundant text | `{referenceCategory:"PERSISTENT-ID", referenceType:"subject", referenceLocator:"sha256:<hex>"}` |
| SPDX 3 | `SpdxDocument.externalIdentifier[]` | `{type:"ExternalIdentifier", externalIdentifierType:"subject", identifier:"sha256:<hex>"}` |

CDX reuses the `attestation` enum value — coexists with milestone-073
attestation IRIs in the same array, distinguishable by `url` shape
(digest vs IRI). The SPDX 2.3 main-module gate is the same one
milestone 073 introduced; subject identifiers without a main-module
package still appear in `creationInfo.creators[]` for fixture-agnostic
discovery.

### 9.2 Cross-tier digest handshake

External SBOM-store consumers can correlate components across SBOMs
purely by string match:

```text
image-SBOM.components[].hashes[].sha256 == X
    →   build-SBOM with subject:sha256:X identifier
        →   that build SBOM's git: identifier
            →   matching source SBOM
```

No mikebom-side resolver. The `subject:` value's hex portion equals
the digest portion of an `image:` value when they refer to the same
artifact (FR-014).

### 9.3 Per-component user-defined identifiers (`--component-id`)

Attach an operator-defined identifier to a specific component:

```bash
mikebom sbom scan --path . \
    --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-shared-lib-v2" \
    --output out.cdx.json
```

The flag is repeatable. Built-in scheme names (`repo`, `git`,
`image`, `attestation`, `subject`) are rejected at clap parse time
per FR-009 — those slots are reserved for document-level use. PURL
matching is byte-equality only (no glob, no version-range). Selectors
matching multiple components attach the identifier to ALL matches
(FR-011); selectors matching zero components emit a `tracing::warn!`
and the scan continues (FR-010).

**Per-format wire mapping**:

| Format | Carrier | Shape |
|---|---|---|
| CDX 1.6 | `components[].properties[]` | `{name:"<scheme>", value:"<value>"}` |
| SPDX 2.3 | `Package.externalRefs[]` | `{referenceCategory:"PERSISTENT-ID", referenceType:"<scheme>", referenceLocator:"<value>"}` |
| SPDX 3 | `Element.externalIdentifier[]` | `{type:"ExternalIdentifier", externalIdentifierType:"<scheme>", identifier:"<value>"}` |

Pre-existing per-component entries (`mikebom:not-linked`,
`mikebom:shade-relocation`, the SPDX 2.3 `purl` externalRef, etc.)
preserve their original positions; new `--component-id` entries
append after, lex-sorted by `(scheme, value)` per research §6.

### 9.4 jq decode recipes

Extract `subject:` from a CDX build SBOM:

```bash
jq '.metadata.component.externalReferences[]
    | select(.type == "attestation")
    | select(.url | startswith("sha256:") or startswith("sha512:"))
    | .url' out.cdx.json
```

Extract per-component user-defined identifiers from a CDX SBOM:

```bash
jq '.components[]
    | {purl: .purl, ids: [.properties[]?
        | select(.name | test("^[a-z][a-z0-9_-]*$"))
        | select(.name | startswith("mikebom:") | not)
        | {(.name): .value}]}' out.cdx.json
```

Same against SPDX 2.3:

```bash
jq '.packages[]
    | {purl: (.externalRefs[] | select(.referenceType == "purl") | .referenceLocator),
       ids: [.externalRefs[]
        | select(.referenceCategory == "PERSISTENT-ID")
        | {(.referenceType): .referenceLocator}]}' out.spdx.json
```

### 9.5 Backward compatibility

All milestone-073/074/075 byte-identity goldens stay byte-identical:
no fixture passes `--subject-hash` or `--component-id` today. New
fixtures that exercise the new paths gain additive entries — the
expected golden regen for this milestone. See quickstart.md for
operator recipes covering all four user stories.

---

## Section 10 — Milestone 077: Root component override (`--root-name` / `--root-version`)

Milestone 077 adds two new CLI flags that override the auto-derived
root component identity in the emitted SBOM. Before 077, source-tier
scans of arbitrary directories produced names like
`filesystem-scan@0.0.0` (basename of `--path` + hardcoded `0.0.0`)
that didn't reflect the operator-meaningful project identity. The
new flags close that gap.

### 10.1 The two flags

```text
--root-name <NAME>      Override metadata.component.name.
--root-version <VERSION> Override metadata.component.version.
```

Both flags accept any non-empty UTF-8 except whitespace, control
characters, `?`, and `#` (the URL-syntax-breaking subset). Both
flags are independent — operators can override one without the
other. When neither is passed, behavior is byte-identical to
alpha.17. URL-encoding is applied automatically at PURL emission
time per RFC 3986 §2.3, so npm-scoped names like `@acme/widget-svc`
are accepted at parse and percent-encoded into the PURL `name`
segment (`%40acme%2Fwidget-svc`).

Operators can stack the override with milestone-072–076 identifier
flags freely — they're orthogonal slots:

```bash
mikebom sbom scan --path . \
    --root-name widget-svc --root-version 1.2.3 \
    --repo git@github.com:acme/widget-svc.git \
    --subject-hash sha256:abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890ab \
    --component-id "pkg:cargo/serde@1.0.0=kusari-id:asset-foo" \
    --output out.cdx.json
```

### 10.2 Operator recipes

**Recipe A: source-tier override on an arbitrary directory.** The
headline use case the milestone exists for.

```bash
mikebom sbom scan --path /opt/builds/abc123-snapshot \
    --root-name widget-svc --root-version 1.2.3 \
    --output widget-svc.cdx.json

jq '.metadata.component | {name, version, "bom-ref", purl, cpe}' widget-svc.cdx.json
# {
#   "name": "widget-svc",
#   "version": "1.2.3",
#   "bom-ref": "widget-svc@1.2.3",
#   "purl": "pkg:generic/widget-svc@1.2.3",
#   "cpe": "cpe:2.3:a:mikebom:widget-svc:1.2.3:*:*:*:*:*:*:*"
# }
```

**Recipe B: override on a manifest-driven Cargo project (clean
replacement).** When the override is set on a Cargo project, the
manifest-derived main-module identity is dropped entirely from the
emitted SBOM (it doesn't appear in `metadata.component`, and it
doesn't appear in `components[]` as a demoted library entry).

```bash
cd ~/projects/foo-internal-cargo  # has [package].name = "foo-internal"
mikebom sbom scan --path . \
    --root-name widget-svc --root-version 1.2.3 \
    --output out.cdx.json

# The manifest main-module is gone:
jq '.components[] | select(.purl == "pkg:cargo/foo-internal@0.5.1")' out.cdx.json
# (no output)

# metadata.component carries the operator identity:
jq '.metadata.component.name' out.cdx.json
# "widget-svc"
```

To preserve the manifest-derived identity as a regular library
entry alongside the override, track **GitHub issue #151** (the
"demote to library" follow-up). Today's MVP uses clean replacement.

**Recipe C: override on image-tier scan.** Useful when the image
SBOM should identify as the deployed service name rather than the
image basename. The auto-detected `image:` identifier (milestone
073) is unaffected — it rides the orthogonal `externalReferences[]`
slot.

```bash
mikebom sbom scan --image acme/internal-bld:v1 \
    --root-name widget-svc-image --root-version 1.2.3 \
    --output image.cdx.json
```

### 10.3 Per-format wire mapping

| Field | CDX 1.6 | SPDX 2.3 | SPDX 3.0.1 |
|-------|---------|----------|------------|
| Name | `metadata.component.name` | Synthesized root `Package.name` | Synthesized root element `name` |
| Version | `metadata.component.version` | Synthesized root `Package.versionInfo` | Synthesized root element `software_packageVersion` |
| `bom-ref`/SPDXID | `metadata.component.bom-ref = <name>@<version>` | `Package.SPDXID` (hash-derived from override) | Element `spdxId` (hash-derived from override) |
| PURL | `metadata.component.purl = pkg:generic/<percent-encoded(name)>@<percent-encoded(version)>` | `Package.externalRefs[purl]` | Element `software_packageUrl` |
| CPE | `metadata.component.cpe` | `Package.externalRefs[cpe23Type]` | Element `externalIdentifier[cpe23]` |

When the override is active, manifest-derived main-module components
(identified by `properties[mikebom:component-role=main-module]`) are
filtered OUT of:
- CDX `components[]`
- SPDX 2.3 `packages[]`
- SPDX 3 `@graph[type=software_Package]` elements

per the 2026-05-06 clean-replacement clarification. See GitHub
issue #151 for the demote-to-library follow-up tracking.

### 10.4 Validation rules

`--root-name` and `--root-version` reject the following at CLI
parse (before any scan work happens):
- Empty strings
- ASCII whitespace
- ASCII control characters (`\x00`–`\x1F`, `\x7F`)
- `?` and `#` (the URL-syntax-breaking subset)

Error messages identify the offending character and its position so
operators can pinpoint the violation:

```bash
mikebom sbom scan --path . --root-name "my widget svc"
# error: invalid value 'my widget svc' for '--root-name <NAME>':
# --root-name contains whitespace at position 2 (character: ' ');
# whitespace is not allowed
```

### 10.5 Backward compatibility

All milestone-073/074/075/076 byte-identity goldens stay
byte-identical: no existing fixture passes `--root-name` or
`--root-version`, and the no-flag emission path is unchanged.
The new helper `percent_encode_purl_name` is invoked only on the
override-active path; non-override PURL emission continues to use
the existing `encode_purl_segment` (CDX) and `url_friendly` (SPDX 3)
helpers verbatim per research §1.

### 10.6 Interaction with milestone 080's `--scan-target-name`

Milestone 080 adds a sibling flag, `--scan-target-name <name>`, that
overrides the **document-level** name field — distinct from
`--root-name` which targets the root component / Package's name.
Per milestone 080 research §5, the precedence rules are
**format-specific**:

| Format | Field | Wins on conflict | Notes |
|--------|-------|------------------|-------|
| CDX 1.6 | `metadata.component.name` | `--root-name` (the root component IS the document subject in CDX) | A stderr warning fires when both flags are set; `--scan-target-name` is silently ignored on this slot. |
| SPDX 2.3 | top-level `name` (document) AND root `Package.name` | Both honored independently | `--scan-target-name` sets the document-level `name`; `--root-name` sets the root Package's `name`. SPDX 2.3 is the only format with two distinct slots. |
| SPDX 3 | `SpdxDocument.name` AND root `software_Package.name` | Both honored independently | Same independence as SPDX 2.3 — `--scan-target-name` targets the SBOM-level name; `--root-name` targets the root-element name. |

Operators who want both an SBOM-document name AND a root-component
name on SPDX 2.3 / SPDX 3 SBOMs should pass both flags. On CDX
emissions, only `--root-name` is observable on the
`metadata.component.name` slot — the audit trail of the conflict is
recorded as a stderr warning at scan time.

See `specs/080-user-sbom-metadata/quickstart.md` for the full set of
milestone-080 flag recipes.

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

### SBOM types and lifecycle phases

mikebom signals the **SBOM type** (CISA Design / Source / Build /
Analyzed / Deployed / Runtime) via three different format-native
fields: CDX `metadata.lifecycles[].phase`, SPDX 2.3
`creationInfo.comment` (parse-and-translate), and SPDX 3
`software_Sbom.software_sbomType[]` (added in milestone 081). See
[SBOM types — operator-facing reference](sbom-types.md) for the
full per-format `jq` recipes, the four-column equivalence table
(CISA ↔ mikebom tier ↔ CDX phase ↔ SPDX 3 SbomType), the
mixed-type-SBOM presentation rule, and the `--sbom-type <type>`
operator-assert flag for downstream pipelines that require a
single-type document-level claim.
