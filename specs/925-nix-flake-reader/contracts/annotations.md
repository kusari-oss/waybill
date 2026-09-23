# Contract: annotations introduced

Two facts in a `flake.lock` have no native representation in any of the three
emitted formats. Both are recorded in the Principle V audit in the spec.

## A-1 — NAR hash

**Why not native**: every format's checksum field means a hash over the
component's bytes and expects hex. A `narHash` is SRI-encoded base64 over a NAR
serialization of a directory tree. The mapping would be wrong in semantics and
in encoding, and a consumer that verified it would be misled by a value that
looks correct.

**Shape**: the SRI string verbatim, including its `sha256-` prefix. Preserving
the prefix keeps the value self-describing and re-verifiable by Nix tooling; a
stripped or re-encoded value would not be.

## A-2 — Original (pre-resolution) reference

**Why not native**: the formats model a resolved dependency. The distinction
between "what the author asked for" and "what it resolved to" is the same
distinction waybill already draws for declared ranges versus locked versions,
and it has no native slot for a source reference.

**Shape**: the original reference in a form that makes the moving part visible —
a branch or tag name where one was used.

**Emitted only when it differs from the locked reference** (FR-006).

## Registration obligation

Each annotation requires a row in the format-mapping catalogue and a matching
extractor for CycloneDX, SPDX 2.3 and SPDX 3. The parity suite asserts this by
construction: a row without extractors fails
`every_catalog_row_has_an_extractor`, and an annotation without a row sits
outside the parity guarantee, letting the three formats drift.

Both ship in the same change as the reader (FR-009b).
