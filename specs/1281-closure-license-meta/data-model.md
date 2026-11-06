# Data model: licenses for nix closure components

## Where the silence lives today

`waybill-cli/src/scan_fs/package_db/nix/closure/emit.rs:180`:

```rust
licenses: vec![],
concluded_licenses: vec![],
```

Every closure component is constructed with an empty license set. The field
exists and the emitters already carry it to the native `licenses[]` slot of all
three formats; nothing populates it for this tier. That is the whole of the
defect — no new field, no new emission path.

## Entities

### `LicenseRow` (new, in-process)

One row returned by the evaluation. Defined in
[contracts/license-lookup.md](./contracts/license-lookup.md).

| Field | Type | Role |
| --- | --- | --- |
| `drv` | `String` | store-path basename; **the join key** (FR-002) |
| `out` | `String` | output store path; named `out`, never `outPath` (R3) |
| `attr` | `String` | attribute the row was found under; diagnostics only |
| `root` | `String` | root id the row came from; diagnostics only |
| `licenses` | `Vec<String>` | as nixpkgs states them, pre-canonicalisation |

Lifetime: built from the evaluation's JSON, consumed by the join, dropped. No
persistence.

### `PackageDbEntry` (existing, changed)

| Field | Type | Change |
| --- | --- | --- |
| `licenses` | `Vec<SpdxExpression>` | populated from the joined row where one exists |
| `concluded_licenses` | `Vec<SpdxExpression>` | unchanged — this source is *declared*, not concluded |

A nixpkgs `meta.license` is what the packager declares, not a conclusion drawn
from scanning the artifact. It belongs in `licenses`, and putting it in
`concluded_licenses` would overstate the evidence.

### `LicenseCoverage` (new, document-scope)

| Field | Type | Role |
| --- | --- | --- |
| `wanted` | `usize` | closure components eligible for a license |
| `received` | `usize` | components the join actually licensed |
| `source` | `String` | the pinned revision the licenses came from |
| `degraded` | `Option<DegradationReason>` | why, when the lookup could not run |

This is the Principle X carrier. Without it a closure whose members genuinely
declare no licenses is indistinguishable from a lookup that silently did
nothing — and coverage can never reach 100% (R5), so the distinction is
permanent rather than transitional.

## Rules

1. **Join by store path, never by name.** `autoconf269` has
   `pname = autoconf`; a name join attaches 2.73's license to 2.69 (R5).
2. **A row found for a store path already carrying licenses does not
   overwrite.** Milestone 1273's Haskell pass licenses some members already;
   FR-004 governs, and the existing value wins.
3. **Several declared licenses are all carried** (FR-005), in the order
   nixpkgs states them, deduplicated.
4. **A non-SPDX nixpkgs vocabulary value is preserved as stated and marked**,
   never rewritten into a plausible-looking SPDX identifier (US2). Milestone
   1273's wording for this is the precedent: a field that invents its contents
   stops being evidence.
5. **Degradation is not failure.** Any `DegradationReason` leaves the document
   emittable with its components (FR-006).

## State transitions

None. The lookup is a single pass over an immutable closure, run once per
scan, after `closure_identity::reclassify` — identities and relationship
endpoints move together in that step, so anything keyed on component identity
must run downstream of it.
