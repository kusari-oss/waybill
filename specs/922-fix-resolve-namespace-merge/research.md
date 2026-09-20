# Phase 0 — Research

**Feature**: `922-fix-resolve-namespace-merge` (#919)

Every empirical claim below was measured against this tree, not carried over
from the issue or the spec. Harnesses and raw output live in
`measurements/`.

---

## R1 — FR-006c: is a component's namespace singular? **Yes, measurably.**

The spec required this be measured rather than assumed, because a plural
answer changes the annotation's shape.

**Measured across every committed corpus golden and every Pants fixture**
(`measurements/singularity.txt`, `measurements/singularity-fixtures.txt`):

| observation | result |
|---|---|
| components with plural membership, whole corpus | **0** |
| components with plural membership, fixtures | **1** — `pkg:pypi/waybill-fixture-common@1.0.0` in `["app","tools"]` |
| that component's two resolves | **both Python** |
| any component in resolves from two namespaces | **none found** |

**Decision**: the namespace is a **scalar** per component.

**Rationale, and it is structural rather than merely observed.** A component
instance comes from exactly one reader, and each reader writes membership only
for resolves in its own namespace. The only way a component could accumulate
two namespaces is deduplication unioning two components that share a PURL and
came from different readers. The Python readers emit `pkg:pypi/*` and
`pkg:generic/*`; the coursier reader emits `pkg:maven/*`. The sets do not
intersect, so that union cannot occur today.

**The residual risk is named rather than dismissed**: it is a property of
which PURL types today's readers happen to emit, not an invariant anything
enforces. A future reader emitting `pkg:generic/*` on the JVM side would make
it reachable. So the accessor must **detect** plurality and refuse to guess,
rather than assume it away — exactly the shape m911 gave `read_single`, which
returns `None` and warns when membership is plural at a site that expects one.

**Alternatives considered**: a plural (array) namespace, rejected as modelling
a state that cannot currently arise and that would reintroduce the pairing
problem the clarification eliminated; and deriving the namespace at read time
from the component's PURL ecosystem, rejected in R2.

## R2 — Ecosystem inference is wrong, and now provably so

Contract C-2 of milestone 912 rejected inferring the namespace from member
PURL ecosystem, on the grounds that it holds only because fixture resolves are
single-ecosystem. **The measurement shows it is worse than that.**

```
pants-example-python    python-default -> ecosystems {generic, pypi}
pants-example-django    python-default -> ecosystems {generic, pypi}
```

A **Python** resolve routinely contains `pkg:generic/*` components — milestone
223 emits them for non-PyPI Pex entries. So ecosystem is not merely an
unreliable proxy for namespace; for a large class of real components it maps
to nothing at all. `generic` is not a namespace signal.

The namespace must come from the producing reader. Confirmed as the right call
and not revisited.

## R3 — The degenerate split (US3) is a second, independent code path

`split.rs:1031`:

```rust
if groups.len() <= 1 {
    tracing::warn!(… detected = groups.len(), mode = "resolve",
        "no partitionable Pants resolves detected — emitting a single SBOM …");
    return Ok(false);
}
```

`groups` is built by the same bare-name keying that causes the merge, so a
repository whose only resolves are a colliding pair counts **one** group and
never splits at all. Reproduced: `detected=1`.

**This is why US3 is a separate story rather than a free consequence.** Fixing
the grouping key fixes the count as a side effect — but only if the count is
taken *after* regrouping. A fix that regroups at emission time while leaving
this check reading a pre-regrouping collection would still swallow the
simplest real-world case, silently, with a warning that reads like correct
behaviour.

## R4 — The existing filename-collision fallback cannot serve here

`filename_for` already de-collides, by appending `sha8_hex(root.source_dir)`.
It is inert for resolve mode:

```rust
let root = root_purl.map(|purl| SubprojectRoot {
    …
    source_dir: std::path::PathBuf::new(),   // empty, for every resolve
});
```

Every resolve projection's synthetic root has the **same empty** source
directory, so every resolve hashes identically and two colliding resolves
would collide again after the "fallback" ran. Reaching for it is the obvious
move and it silently does nothing.

Per the clarification, the slug is namespace-qualified on collision instead,
and FR-002b exists to stop a future reader assuming the generic mechanism
covers this.

## R5 — Catalogue row and blast radius

Highest live row is **C163** (milestone 912). The new per-component namespace
row is **C164**, plus three extractors.

Corpus targets carrying resolve membership, and therefore moving when the
annotation is emitted unconditionally (FR-006d):

```
pants-example-django
pants-example-jvm
pants-example-python
```

**Three, not five.** `pants-example-golang` and `pants-example-javascript`
carry no membership — the Go reader is enrichment-only and the JS one is not a
resolve reader — so they are untouched. The spec's assumption of "every Pants
target" was pessimistic; the measured blast radius is smaller.

Golden movement is expected and is now cheap to review: the corpus lane emits
a readable masked diff and ships the masked `.actual` as of #921, and the
tool-version masking means this refresh will not be confounded by an unrelated
release bump.

## R6 — What milestone 912 leaves in place

- `LanguageNamespace` — the closed `python | jvm` set, already typed.
- `NamespaceIndex` — resolve name → namespaces, **document scope**. Enough to
  identify a document, not to assign a component. It stays; the per-component
  annotation is what this milestone adds.
- C163 `waybill:document-resolve` — states the resolves a document represents,
  plural only on a merged document (its contract C-6). **Once this milestone
  lands the plural case cannot arise**, and C163 needs no change: it will
  simply always state one. Its test asserting the merged-document plural case
  becomes the assertion that two documents each state one.

## R7 — What has NOT been established

The spec's US2 asserts value for a consumer who partitions an unsplit
document's components by resolve themselves. **No such consumer has been
confirmed**, exactly as #914's equivalent question was unconfirmed until
asked. The difference is that here the same annotation is *required anyway* by
US1's grouping fix (FR-005), so US2 costs only the decision to emit what is
already computed — the marginal cost is a catalogue row, not the milestone.

Recorded so the trade is visible rather than implied: if the answer is "no
such consumer", US1 still needs the data and the honest framing becomes
"emitted because it was free and absence would be ambiguous", not "emitted
because consumers asked".
