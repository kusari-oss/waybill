# Data model: components for the closure members waybill does not name

## The thing that changes

`emit::components` today skips any derivation whose `pname()` is `None`. That
one filter is why 1,145 of 1,749 derivations are invisible. Everything here
exists to replace it with a classification.

## Entities

### `MemberRole` (new)

Which of four kinds a `pname`-less closure member is. An enum, not a string:
the role selects the identity basis and the edge eligibility, so a typo in a
string would silently change both.

| Variant | Wire | Count | Derived from |
| --- | --- | --- | --- |
| `Source` | `source` | 549 | the derivation records a content hash |
| `Package` | `package` | 83 | no content hash, but the name encodes a version |
| `Scaffolding` | `scaffolding` | 267 | no content hash, and `env.text` is present |
| `Fragment` | `fragment` | 246 | none of the above |

Order matters: a derivation with both a content hash and `env.text` is a
`Source`, because the hash is the stronger evidence. The classifier is a
total function — every member receives exactly one role (FR-005), with no
residue, which the probe confirms by the four counts summing to 1,145.

**Not** a C182 `DerivationRole`. That enum classifies *derivations* as
`artifact-input` / `build-tooling` / `both` / `unreferenced` and already covers
this population (research R4). The two vocabularies coexist; neither is
derived from the other.

### `IdentityBasis` (new)

Two variants, because FR-003 and FR-004 prescribe different sources:

| Variant | When | Effect |
| --- | --- | --- |
| `Content(hash)` | the derivation records a content hash | two members with the same name and hash **merge** into one component |
| `Recipe(drv_hash)` | otherwise | every member stays distinct |

A two-variant enum rather than `Option<ContentHash>` so that the merging
behaviour is a property of the type: an `Option` would let a caller forget
that `None` still needs an identity and emit a name-keyed component, which is
the collision FR-002 forbids.

### Emitted component (existing type, new population)

| Field | Value |
| --- | --- |
| `name` | the derivation's own `name`, unmodified (FR-016) |
| `purl` | `pkg:generic/<name>-<hash-prefix>`, hash per `IdentityBasis` |
| `extra_annotations["waybill:closure-member-role"]` | the role's wire string |
| source URL | carried where the derivation records one (FR-008), 138 of 1,145 |
| licenses | none — milestone 1281 covers named members; nixpkgs declares nothing here |

The hash lives in the **name segment**, not a qualifier: PURL equality is how
consumers match components and some tools compare without qualifiers, under
which 56 distinct `source` artifacts would collapse to one (FR-002).

### Document-scope records

| Record | Carries | Changes? |
| --- | --- | --- |
| milestone-1277 C182 census | 1,749 derivations by C182 role | **No** — counts derivations, not components (FR-012) |
| new member-role record | the four counts (FR-009) | New |
| edge count | the post-change figure (FR-018) | **Yes** — reshaped, not merely grown |

## Rules

1. **The classifier reads only the closure.** No network, no second
   evaluation, no maintained name list (FR-006).
2. **Identity is unique under qualifier-ignoring comparison** (FR-002).
   Measured: name alone collides for 188 of 1,145.
3. **Content identity merges; recipe identity does not.** 549 content-hashed
   members become 502 components, and that merge is correct — the same content
   fetched twice is one artifact (FR-003).
4. **Only `Source` and `Package` are edge endpoints** (FR-013). The projection
   stops at them and walks through the other two.
5. **Every member unreachable by an edge carries a reason** (FR-014). On the
   pinned closure that is 513 components — a population, not an exception.
6. **The derivation's name is never rewritten** (FR-016).

## State transitions

None. Classification is a pure function of one derivation record, evaluated
once per member per scan.
