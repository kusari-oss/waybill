# Contract: the curated table and what it emits

**Feature**: `1375-generic-cpe-table` | **Issue**: #1381

No new annotation rows, no schema change, no CLI flags. 49 components gain a
CPE; the C209 counts move by exactly that number.

---

## 1. Components that gain a CPE

48 names, 49 components (`gettext` appears twice). Examples of the emitted
form:

| component | CPE |
| --- | --- |
| `bash 5.3` | `cpe:2.3:a:gnu:bash:5.3:*:*:*:*:*:*:*` |
| `libxml2 2.14.6` | `cpe:2.3:a:xmlsoft:libxml2:2.14.6:*:*:*:*:*:*:*` |
| `expat 2.7.3` | `cpe:2.3:a:libexpat:expat:2.7.3:*:*:*:*:*:*:*` |
| `less 692` | `cpe:2.3:a:greenwoodsoftware:less:692:…` **and** `cpe:2.3:a:gnu:less:692:…` |

The primary goes to `component.cpe`; additional candidates to the existing
C19 `waybill:cpe-candidates` property, exactly as `curl` already behaves.

## 2. Components that deliberately gain nothing

| name | components | why |
| --- | --- | --- |
| `ninja` | 1 | NVD's 126 entries are three other products |
| `cmake` | 1 | the only vendor is an npm package; `kitware:cmake` = 0 |
| `libxcrypt` | 1 | one entry, attributed to an automotive vendor |

**These are contract, not omission.** A future change that gives any of them
a CPE is a regression, and a test says so.

## 3. `C209 waybill:cpe-coverage` — values move, shape does not

**Before** (aarch64-darwin)

```json
{"schema":"waybill-cpe-coverage/v3","with_candidates":78,
 "addressable_without_cpe":166,"unaddressable":194,
 "excluded_members":539,"excluded_files":0}
```

**After**

```json
{"schema":"waybill-cpe-coverage/v3","with_candidates":127,
 "addressable_without_cpe":166,"unaddressable":145,
 "excluded_members":539,"excluded_files":0}
```

`+49 / −49`, everything else identical, sum still **977**. The schema stays
`/v3`: the shape is unchanged, only the population inside it moves.

> **x86_64-linux is a different closure** and its delta will differ. The
> corpus figures are measured by a task rather than scaled from these.

## 4. What does not change

| | |
| --- | --- |
| C209's schema and field set | **unchanged** |
| any component outside the 48 names | **unchanged** |
| any component that already carried a CPE | **unchanged** (SC-005) |
| `addressable_without_cpe`, `excluded_members`, `excluded_files` | **unchanged** |
| every other ecosystem arm | **unchanged** |
| CLI surface | **no new flags** |

## 5. Blast radius

| | files |
| --- | --- |
| local goldens | those whose fixtures contain an affected name |
| corpus goldens | `nix-closure-moat` certainly; others only if they carry one of the 48 names as `pkg:generic` |

**Unlike the last two milestones this is not a document-wide change.** C209 is
on every document, but its *value* only moves where an affected component
exists. A task measures which goldens actually move rather than predicting
all of them — and a golden moving on a target with none of these names would
be a finding.
