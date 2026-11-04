# Contract: disclosing a partially-read closure

Two surfaces: the **fact** goes in each format's native carrier, the **detail**
in a new annotation. Native-first is Principle V, and research R1 verified the
carriers against the vendored schemas rather than from memory.

## Native carriers

| Format | Carrier | Verified |
|---|---|---|
| CycloneDX 1.6 | `compositions[].aggregate` — `aggregateType` enum includes `incomplete`: *"Additional relationships exist and may include constituent components, services, or dependencies."* | `cyclonedx-1.6.json`, `definitions.aggregateType.enum` |
| SPDX 3.0.1 | `Relationship.completeness` — enum `["complete","incomplete","noAssertion"]` | `spdx-3.0.1.json`, `prop_Relationship_completeness` |
| SPDX 2.3 | **none.** `completeness` does not occur in the schema. | counted in `spdx-2.3.json` |

### The claim being corrected

The committed `nix-closure-moat` golden carries:

```json
{"aggregate": "complete", "dependencies": ["nix-closure-moat@d069055"]}
```

A native assertion that the root's dependency relationships are complete, in a
document whose closure skipped 554 declarations. **A document with an
unconsulted declaration must not carry this record.**

### How the demotion is routed

Through `dependency_claims`, which CycloneDX and SPDX 3 have shared since
m1069 so the two cannot drift. The partial read is an input to it. Both claims
it produces are affected and they are different claims:

- the **ecosystem** claim over nix-derived components, which already demotes
  on `degraded_ecosystems`;
- the **root** dependency claim, which the helper derives from trace integrity
  and which is the record measured as false today.

A second, parallel demotion path is forbidden: it would reintroduce exactly
the cross-format drift m1069 removed.

## The annotation

Document-scope, new catalog row (C204 — next free; the number is confirmed
when the row is written, not reserved by the spec). A **JSON object encoded as
a string**, following C200 `waybill:hpack-read-outcome`.

```json
{
  "format-version": 4,
  "fields-unconsulted": ["depsBuildTarget", "depsTargetTarget"],
  "declarations-unconsulted": 554,
  "derivations-affected": 421
}
```

- Keys sort, so the value is byte-stable.
- `fields-unconsulted` is sorted and deduplicated. Its membership is an
  **output** of this milestone — it is the first measurement of #1346's
  coverage gap — so no expected value is quoted here or in the spec. The
  values shown are illustrative of shape only.
- `format-version` is an integer, recorded whether or not anything was
  unconsulted.

### Emission gating

| State | Native | Annotation |
|---|---|---|
| `--nix-closure` not given | untouched | absent |
| Given, no closure recorded | untouched | absent — **C190 already covers this**, and a zero-valued disclosure would claim a closure was read where none was |
| Given, closure fully read | `complete` as today | absent |
| Given, closure partially read | demoted | present |

Absence therefore means "nothing to disclose", never "not checked" — which is
the ambiguity this whole milestone is about. The guard of US4 is what makes
absence trustworthy: it fires on a declaration the reader cannot interpret, so
an unread field cannot masquerade as an absent one.

## Parity

All three formats carry the annotation with **identical payload**, checked by
the existing cross-format machinery — three extractors, one per format,
landing in the same change as the row. A documented row that nothing emits
fails the catalog's own completeness test, so documentation and emission are
not separable here.

**SPDX 2.3 adds nothing to the payload to compensate for its missing native
field.** Its annotation says what the other two say, and the fact of
incompleteness is carried there by the annotation's *presence*. The
alternative — a format-specific extra field — would make the payloads unequal
and force the row to be `PresenceOnly`, giving up the equal-content check for
a redundancy. Equal payload, format-specific meaning of presence, is the
weaker-looking option that is actually checkable.

**Principle V audit, per row:**

| Claim | Native carrier | Disposition |
|---|---|---|
| the graph is incomplete | CDX `aggregate`, SPDX 3 `completeness` | **USE NATIVE.** Both. |
| the graph is incomplete (SPDX 2.3) | none in the schema | **KEEP-NO-NATIVE.** Carried by the annotation's presence, not by an extra field, so the payload stays equal across formats. |
| *which* fields went unconsulted | none in any format | **KEEP-NO-NATIVE.** |
| how many declarations and derivations | none in any format | **KEEP-NO-NATIVE.** |
| which output shape it was read from | none in any format | **KEEP-NO-NATIVE.** |

The detail rows are the same category as m1277's C203: provenance about how a
conclusion was reached, which no SBOM format has a vocabulary for.
