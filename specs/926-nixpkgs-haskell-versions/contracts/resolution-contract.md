# Contract — nixpkgs Haskell version resolution (#947)

The observable surface this feature adds. Anything here is something a second
component (an emitter, a parity extractor, a downstream consumer) can see, so
it changes only by amending this file first.

---

## C1 — Trigger

Resolution runs **iff both** hold:

1. `flake.lock` names an input resolvable to an exact revision whose shape is
   nixpkgs-like (m925 `OriginalPinState::Exact`), **and**
2. the Haskell reader produced at least one declared dependency.

Neither condition met → **no retrieval, no annotation, no output change**
(SC-009, FR-012).

`--offline` suppresses retrieval unconditionally (FR-009). The dedicated
opt-out flag suppresses this feature only, leaving other network enrichment
active (FR-015a).

---

## C2 — Per-dependency outcome

Every declared Haskell dependency ends in exactly one state:

| state | version | source hash | reason annotation |
|---|---|---|---|
| resolved | exact | hex SHA-256, native field | absent |
| unresolved | **absent** | **absent** | present, one closed-set value |

There is no third state and no partial fill (FR-005). A dependency carrying a
version but no hash is permitted only when the package set records no hash for
it; a dependency carrying a hash but no version is a contract violation.

**Closed reason set**: `compiler-supplied`, `absent-from-package-set`,
`source-unreachable`, `no-exact-revision`, `offline`.

---

## C3 — Source hash is a native field, not an annotation

A resolved dependency's source hash is emitted as SHA-256 in each format's
**native** checksum carrier:

- CycloneDX — `components[].hashes[]`, `{"alg": "SHA-256", "content": "<hex>"}`
- SPDX 2.3 — `packages[].checksums[]`, `{"algorithm": "SHA256", "checksumValue": "<hex>"}`
- SPDX 3 — the established `software_ContentIdentifier` shape

The value is lower-case hex, converted from the package set's Nix base32.

**This is verified, not assumed** (research R2): the decoded digest equals the
SHA-256 of the Hackage source tarball byte-for-byte. A `waybill:` annotation
for this datum would violate Principle V.

**Contrast with C165** (`waybill:nix-nar-hash`): that value is SRI base64 over
a NAR serialization of a directory tree, hashes no file's bytes, and therefore
had no native carrier. Same ecosystem, opposite outcome. Do not generalise
from one to the other.

---

## C4 — Provenance annotation

Every resolved dependency records that its version came from a nixpkgs package
set and which revision produced it. Distinguishable from a version taken from
a `cabal.project.freeze` or `stack.yaml.lock` (FR-007, US3).

A consumer comparing two waybill documents must never have to treat two bare
version strings of unequal provenance as equivalent.

---

## C5 — Candidate-compiler disclosure

When more than one compiler package set is a candidate, every dependency left
unresolved as `compiler-supplied` records the candidates that were considered
(FR-014b), and whether they came from the flake or from the
all-series-at-revision fallback.

Consumers can therefore tell "this is a boot library for the one compiler in
use" from "this might be a boot library depending on which of several
compilers was used".

---

## C6 — Precedence over local lockfiles

A version established by a project-local lockfile or freeze file wins over a
nixpkgs-resolved version (FR-013). When both exist and disagree, the local
value is emitted and the disagreement is recorded. Neither value is silently
discarded.

---

## C7 — Degradation

Any of {unreachable, refused, unauthorized, timed out, layout absent or
unparseable} produces:

- every affected dependency unresolved with reason `source-unreachable`,
- a document-scope degradation record,
- component and relationship counts **identical** to a scan before this
  feature (SC-006),
- no prompt for credentials (FR-018),
- completion within the FR-019 bound (SC-008).

---

## C8 — Determinism and caching

Two scans of the same repository at the same pinned revision produce
byte-identical documents (FR-011, SC-004). A revision already retrieved is not
retrieved again (FR-010, SC-005). The cache is keyed by the exact revision and
needs no expiry, the revision being immutable.

---

## C9 — Components are enriched, never introduced

Resolution attaches versions and hashes to components the Haskell reader
already emitted. It does **not** add components, and it does not walk the
package set's derivation dependency graph to discover new ones (FR-001a).

This is Constitution Principle XII constraint 1 — "External sources MUST NOT
introduce new components" — and is the reason the transitive closure is #962
rather than part of this feature.
