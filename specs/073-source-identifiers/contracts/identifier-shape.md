# Contract — Identifier wire format

This contract specifies the wire-format of source identifiers as supplied via `--with-source` and as emitted in mikebom SBOMs. External tools (verifiers, harnesses, parsers) implement against this contract.

## C-1 — Wire format

```text
<scheme>:<value>
```

- `<scheme>` matches regex `^[a-z][a-z0-9_-]*$` (FR-004).
- `<value>` is everything after the FIRST `:`. Empty values are rejected.
- The split is on the FIRST `:` only; values may contain additional `:` characters.

**Worked examples**:

| Input | Scheme | Value |
|---|---|---|
| `repo:git@github.com:foo/bar.git` | `repo` | `git@github.com:foo/bar.git` |
| `image:docker.io/foo/bar:v1@sha256:abc...` | `image` | `docker.io/foo/bar:v1@sha256:abc...` |
| `acme_corp_id:abc123` | `acme_corp_id` | `abc123` |
| `attestation:https://example.org/att/build-42` | `attestation` | `https://example.org/att/build-42` |

## C-2 — Built-in scheme registry

Four built-in schemes recognized by mikebom alpha.16+:

| Scheme | Semantic | Value form | CDX `type` | SPDX 2.3 `referenceCategory` | SPDX 3 `Element.externalIdentifier[].type` |
|---|---|---|---|---|---|
| `repo:` | Source repository identity | URL or git-style ssh URL | `vcs` | `PERSISTENT-ID` | `repo` |
| `git:` | Repo + commit/ref-anchored identity | URL with optional `#<commit-or-ref>` fragment | `vcs` | `PERSISTENT-ID` | `git` |
| `image:` | Image identity | `[registry/]name[:tag][@sha256:digest]` per Q3 clarification | `distribution` | `PERSISTENT-ID` | `image` |
| `attestation:` | In-toto attestation IRI | URL/IRI | `attestation` | `PERSISTENT-ID` | `attestation` |

## C-3 — Built-in scheme validators

Validators are best-effort syntactic checks. Failures emit a `tracing::warn!` and downgrade the identifier to `IdentifierKind::UserDefined` (research.md §1 soft-fail).

### `repo:` validator

Regex-style: `^(https?://|ssh://|git@|git://)[^\s]+$` OR `<user>@<host>:<path>` shape (the conventional ssh-pseudo).

Operator-recognizable git URL shapes accepted; we don't normalize.

### `git:` validator

Same as `repo:` validator on the URL portion. Optional `#<fragment>` is preserved verbatim. The fragment SHOULD be a commit SHA / branch / tag identifier but isn't validated.

### `image:` validator

Per the Q3 clarification — full form is `<registry>/<name>:<tag>@sha256:<digest>` with components omittable as documented:

- Full: `image:docker.io/foo/bar:v1@sha256:abc...`
- Tarball-only (no registry): `image:foo/bar@sha256:abc...`
- Pre-distribution-spec (no digest): `image:docker.io/foo/bar:v1`

Validator regex (permissive): `^([a-zA-Z0-9.\-_/]+/)?[a-zA-Z0-9.\-_/]+(:[a-zA-Z0-9.\-_]+)?(@sha256:[a-fA-F0-9]{64})?$`.

### `attestation:` validator

Permissive — any RFC 3986 URI shape accepted. No further structure enforced.

## C-4 — User-defined schemes

Any scheme matching the FR-004 regex but NOT in the built-in registry is treated as user-defined:

- No validation on the value side.
- Emitted via the `mikebom:source-identifiers` document-level annotation (per `source-identifiers-annotation.md` C-1).
- Operators are responsible for picking schemes that don't collide with future built-in schemes (forward-compat note in research.md §7).

## C-5 — Determinism

Per FR-009: byte-identical inputs produce byte-identical identifier output. Implementation rules:

- Auto-detected identifiers appear FIRST in the emitted carrier array.
- Manual `--with-source` identifiers follow in supply order.
- Duplicates by exact `(scheme, value)` are deduplicated; on dedup, the manual entry wins (FR-006), the auto-detected `source_label` is dropped.
- The `mikebom:source-identifiers` annotation's `value` array is sorted lexicographically by `(scheme, value)` before serialization (independent of the carrier ordering — annotations are unordered semantics, sort gives determinism).

## C-6 — Failure semantics

| Failure | Behavior |
|---|---|
| Auto-detection unable to find git remote | `tracing::info!` log; emit no auto-detected identifier; scan continues |
| Manual `--with-source` with malformed scheme prefix | clap parse error; scan exits non-zero before any work |
| Manual `--with-source` with empty value | clap parse error; scan exits non-zero |
| Built-in scheme value-validation fails | `tracing::warn!` log; identifier downgrades to `IdentifierKind::UserDefined` and emits via `mikebom:source-identifiers` annotation; scan continues |
| Multiple `--with-source` flags with same `(scheme, value)` | Deduplicated to one entry; no error |

## C-7 — Stability commitment

- The 4 built-in schemes (`repo:`, `git:`, `image:`, `attestation:`) are stable across mikebom alpha versions post-073.
- The FR-004 scheme regex is stable. Future schemes that don't match the regex (e.g., uppercase) are not allowed without a contract-level change.
- New built-in schemes MAY be added without breaking compat. User-defined schemes that collide with future built-ins migrate at the registration milestone (operators are warned).
- The `image:` canonical shape per Q3 is stable. Future image-reference conventions (e.g., OCI 1.x vs 2.x) accommodate via the validator's permissive regex; emit-side keeps the documented shape.
