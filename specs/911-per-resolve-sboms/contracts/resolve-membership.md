# Contract: Resolve membership and per-resolve partitioning

**Feature**: `911-per-resolve-sboms` (#902 items 1, 3, 4)
**Consumers**: any tool partitioning a waybill SBOM by Pants resolve.

This is a **consumer-visible wire contract**. C-1 changes the value of an
annotation that already ships, in a way an existing reader will mis-parse
rather than reject.

---

## C-1. Membership is a lex-sorted JSON array — always

`waybill:pants-resolve` (catalogue row C143) carries every resolve that pins
the component.

```
before:  "app"
after:   ["app"]            one resolve
after:   ["app","tools"]    two resolves
```

- Lexically sorted. Not incidental: it is what makes two scans of one
  repository byte-identical (FR-003).
- Never empty, no duplicates.
- **The array form is used even for a single resolve** (FR-006a). A shape that
  varies with cardinality forces every consumer to write two paths, and the
  branch exercised least is the shared-package case this feature exists to fix.

### C-1a. Migration

An existing reader expecting a bare string will receive an array and, in most
JSON tooling, **carry on with a wrong value rather than fail**. For a
downstream security tool partitioning on this field, a silent mis-parse is the
worse failure. FR-006b therefore requires this be called out in release notes
as a consumer-visible change, not left to be discovered.

The key is **not** retired and no parallel key is added: one key, one grammar,
one place to look.

---

## C-2. Membership survives deduplication by union

When components describing one package merge, the result names every resolve
either side named.

- Union applies to **membership only**, not to annotations generally. The
  existing first-wins rule stays for single-valued evidence, where the
  higher-confidence side should win and a union would produce a value true of
  neither side.
- The union is order-independent: the merged value does not depend on which
  component won the merge, nor on read order.

Satisfies FR-002, FR-003. The defect this replaces silently dropped at least
1,147 membership claims on the reported monorepo, emptying four resolves
entirely.

---

## C-3. Emission decodes identically across formats

CycloneDX, SPDX 2.3 and SPDX 3 carry the same array, in the same order, for
the same package (FR-006) — but not in the same bytes, and they cannot:

```
CDX    "waybill:pants-resolve" = "[\"app\",\"tools\"]"     (JSON-in-string)
SPDX23 {"field":"waybill:pants-resolve","value":["app","tools"]}   (real array)
SPDX3  {"field":"waybill:pants-resolve","value":["app","tools"]}   (real array)
```

CycloneDX spec'es `properties[].value` as a string, so an array can only be
carried encoded. This is not new: `waybill:source-files` and
`waybill:file-paths` have always been carried this way. A consumer decodes per
format; the **decoded value** is what this contract fixes. Three readers write this annotation — Pants Pex,
Pants coursier/JVM, and uv-as-a-Pants-backend — and all three use the array
form. A reader left on the scalar would reintroduce exactly the cross-reader
inconsistency #901 was about.

---

## C-4. The document names which resolves were declared **[NEW]**

`waybill:resolve-ownership` (C161, document scope) today carries counts:

```
weak-classification=1;unanchored-lockfiles=0
```

It becomes a **JSON object** naming the resolves in each category, so a
consumer can tell declared from discovered **from the document alone**
(FR-007, FR-008):

```json
{"declared":["app","tools"],"discovered":["scratch"],"weak_classification":1}
```

The grammar change is deliberate. Nesting a list inside `key=value;key=value`
needs a second delimiter level and breaks on a resolve name containing the
separator; JSON is what every other plural value here uses. An existing reader
of the count form breaks **loudly** — the value is no longer `k=v` at all —
which is the right failure mode for a format change a consumer must notice.

- The declared and discovered lists together account for every resolve named
  on any component (SC-006). A resolve in membership but in neither list is a
  defect, not a third category.
- A discovered resolve is named but **not anchored** (FR-009). Naming is
  information; it is not an ownership claim the repository never made.

Changing this value changes a documented grammar. The catalogue row and all
three extractors move in the same change, or the parity gate fails.

---

## C-5. `--split=resolve` partitions by membership **[NEW]**

One document per resolve that contains at least one package.

- **Selection is a membership filter, not a graph walk.** This departs from
  how `--split` has worked since m215, which does BFS from a main-module seed.
  The reason is that a discovered resolve has no seed: no anchor component
  exists for it, so nothing can be constructed to start from. One filter path
  works for declared and discovered alike; two strategies that must agree
  would disagree rarely and data-dependently, which is worse.
- A component in several resolves resolves each bare dependency name in
  **each** of them, emitting one edge per resolve that resolves it (FR-011b).
  Resolves pin independently, so `shared` denotes `shared@1.0.0` in `app` and
  `shared@2.0.0` in `tools`, and a component in both depends on both. Taking
  one match would assert a dependency the requirer does not uniquely have and
  drop one it does — for a consumer matching advisories, the dropped edge is a
  vulnerability that goes unattributed.
- Those edges separate by resolve without being tagged: **an edge belongs to
  resolve R when both endpoints name R**. The membership filter in C-5 already
  produces exactly this, so FR-011c needs no new mechanism.
- A package in several resolves appears in several documents, carrying its
  **full** membership in each (FR-011a) — so a reader triaging one resolve's
  SBOM can see the same fix lands in another.
- Consequently a per-resolve document may name resolves whose packages it does
  not contain. This is correct, not a dangling reference; validation over
  split output must accept it.

### C-5a. Each document's root

Not inherited from the existing split. A sub-SBOM's `metadata.component` is
chosen at emit time by m127's root-selector, which looks for the single
`component-role = "main-module"` component — and a resolve projection has
none, so without intervention every sub-SBOM names the repository instead of
itself. That is the m215 failure in a new place: 23 of 25 sub-SBOMs there
named the repository.

**Declared resolve**: the anchor is promoted to a main-module *within the
projection*, so the document names its own resolve. Any other main-module the
filter carried in is demoted, for the same reason m215 demotes siblings —
more than one candidate leaves the ladder ambiguous.

**Discovered resolve** — *amended during implementation*: the document names
the **repository**, not the resolve, and nothing is synthesised.

This contract originally called for a synthesised root here. Two findings
changed it:

1. **The manifest already answers the question.** Every entry carries
   `root_purl` (`pkg:generic/default`, `pkg:generic/lint`) and the filename
   carries the resolve slug, so a consumer maps each document to its resolve
   without opening it.
2. **Synthesising a component is a stronger claim than FR-009 permits.**
   m868 declined to emit an anchor for a convention-named resolve because a
   filename stem is not a declaration of ownership. Creating that component
   inside split output smuggles it into a different file. Naming a resolve in
   an annotation is information; inventing a component that owns its packages
   is the assertion FR-009 refuses.

The cost is that a discovered-resolve document is not self-describing when
taken out of the manifest's context. A doc-scope "this document is resolve X"
annotation would close that without inventing anything — information rather
than ownership, the same reasoning that makes FR-007 acceptable. Filed as a
follow-up rather than folded in here, because it is new scope.

### C-5b. When a partition is not meaningful

No resolves, or none containing packages: the operator is told what happened
and gets the single-document fallback (FR-012). Silence plus an empty
directory is not an acceptable outcome.

---

## C-6. Preserved

- Which packages are discovered, and which resolves get anchors.
- The dedup grouping key.
- m868's refusal to anchor glob-discovered lockfiles. This feature makes the
  distinction visible; it does not soften it.
- `--split=workspace` and `--split=directory` behaviour, byte-for-byte.

---

## Verification

| Contract | How verified |
|---|---|
| C-1 | a fixture with one- and two-resolve packages; assert array form in both cases |
| C-1a | assert the previous scalar form appears nowhere in emitted output |
| C-2 | two resolves pinning one package at one version → both named after dedup |
| C-2 (order) | perturb component read order; membership byte-identical |
| C-3 | same package, all three formats, same array and order |
| C-4 | one declared-resolve and one convention-only fixture; lists distinguishable, and together complete |
| C-5 | multi-resolve fixture → one document per resolve; shared package in each |
| C-5a | assert each sub-document's root, including for a discovered resolve |
| C-5b | zero-resolve fixture → stated outcome, not an empty directory |
| FR-011b | a component in two resolves, dependency pinned differently in each → both edges present in the unsplit document |
| FR-011c | the same fixture split → each document carries exactly one of those edges |
| C-6 | existing `--split` goldens unchanged |
