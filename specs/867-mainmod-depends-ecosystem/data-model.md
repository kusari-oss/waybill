# Phase 1 — Data Model

Only what changes. Existing types are described far enough to say what
happens to them.

---

## `PackageDbEntry.depends_ecosystem` (new field)

The carrier for FR-001a: the ecosystem the names in `depends` belong to, as
asserted by the reader that read them.

| Property | Value |
|---|---|
| Shape | optional ecosystem identifier |
| `None` | resolution behaves exactly as today — requirer's PURL type is used |
| `Some(e)` | `e` is the only ecosystem searched, and drives normalisation |
| Set by | the reader, at the point it parses the manifest |
| Lifetime | per scan, alongside the rest of the entry |

**Why optional rather than defaulted.** A default would make every reader
adopt at once, which is exactly what the clarified scope rejected. `None` is
not "unknown, go figure it out" — FR-001a forbids inferring it. It means
"this reader has not adopted", and that is what makes SC-004a provable by
construction rather than by testing every reader.

**Invariant**: `depends_ecosystem` describes `depends`. An entry with an
empty `depends` and a `Some` value is harmless but meaningless; an entry with
a non-empty `depends` whose names are not from that ecosystem is a reader
defect, and per the spec's accepted edge case is fixed at the reader rather
than compensated for during resolution.

---

## Resolution index (existing — `name_to_purl`)

`HashMap<(ecosystem, normalized_name), purl>`, built once per scan at
`scan_fs/mod.rs:590`.

**No shape change.** One identity per name per ecosystem, collisions collapsed
at build time. This is the pre-existing behaviour the clarification session
confirmed is unchanged here, recorded so it is not mistaken for something
this feature introduces.

What changes is only which key is looked up.

---

## Edge resolution (existing — `scan_fs/mod.rs:952`)

| | Today | After |
|---|---|---|
| Lookup ecosystem | requirer's PURL type, always | `depends_ecosystem` when set, else requirer's PURL type |
| Name normalisation | under the requirer's PURL type | under the same ecosystem used for lookup |
| Lookups per dep | exactly one | exactly one — unchanged |
| Miss disposition | silently dropped | dropped, counted, and localised on the requirer |

The normalisation row is a correctness change riding along with the lookup
change and must not be separated from it: normalising a gem name under
generic rules is the same category of mistake as searching the generic
ecosystem for it.

---

## Unresolved-declaration reporting

Two halves, mirroring the aggregate/localise pairing the catalogue already
documents between C104 and C45.

### Document-scope count (new)

| Property | Value |
|---|---|
| Scope | document |
| Meaning | how many declared dependencies resolved to nothing |
| Presence | **always**, including when zero (FR-005a) |
| Zero | every declared dependency resolved |
| Absent | must not happen — absence is indistinguishable from "no declarations", which FR-005a exists to prevent |

### Per-component detail (existing — C115)

`waybill:unresolved-declared-dep`, today scoped to npm workspace peers.
Broadened to any requirer with unresolved declared names. Value shape,
envelope, and the KEEP-NO-NATIVE Principle V audit are inherited unchanged;
only the set of components eligible to carry it widens.

**Catalogue obligation**: the new document-scope row needs a matching entry in
`parity/extractors/mod.rs::EXTRACTORS` in the same change, or
`every_catalog_row_has_an_extractor` and `holistic_parity` both fail. C115's
scope broadening reuses its existing extractor and adds no row.

---

## Validation rules

| Rule | Source | Enforced at |
|---|---|---|
| `None` ⇒ byte-identical output | FR-001a | index/lookup; asserted by SC-004a |
| Only the recorded ecosystem is searched | FR-002 | lookup |
| Cross-ecosystem candidates never considered | FR-006 | lookup, by omission |
| No edge and no fabricated component on a miss | FR-004 | lookup |
| Count present even at zero | FR-005a | emission |
| No self-edge | FR-010 | existing guard, unchanged |
| Opt-in inference unchanged | FR-008 | untouched path; asserted anyway |
