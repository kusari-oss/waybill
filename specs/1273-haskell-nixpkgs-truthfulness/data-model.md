# Data model: Haskell/nixpkgs resolution correctness

**Milestone**: 1273 | **Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

All state is in-process per scan, as with every reader since milestone 002.
Nothing persists except the per-revision nixpkgs cache, whose on-disk format
is **unchanged** by this milestone.

The design principle running through this document: **add fields to the
types that already carry these facts; do not introduce a parallel
representation.** R1 measured that an hpack `when:` block *is* a Cabal
conditional, so the hpack reader lowers into `CabalManifest` and every
downstream pass runs unchanged. A second representation is what would let
one project get two answers.

---

## 1. New types

### 1.1 `HpackManifest` (new, `scan_fs/package_db/haskell/hpack.rs`)

The parsed `package.yaml`, as `serde_yaml` sees it. Deliberately a *wire*
type: it exists to be lowered into `CabalManifest` and then dropped.

```rust
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HpackManifest {
    name: Option<String>,
    version: Option<String>,
    license: Option<String>,
    /// Top-level list. R1: reaches EVERY stanza kind.
    dependencies: Vec<String>,
    /// Declared flags and their defaults. Consumed by the lowering so the
    /// `.cabal` reader's existing flag-default evaluation can see them.
    flags: BTreeMap<String, HpackFlag>,
    /// Top-level `when:`; applies to every stanza.
    when: Vec<HpackWhen>,
    library: Option<HpackComponent>,
    internal_libraries: BTreeMap<String, HpackComponent>,
    executables: BTreeMap<String, HpackComponent>,
    /// hpack also accepts the singular `executable:`.
    executable: Option<HpackComponent>,
    tests: BTreeMap<String, HpackComponent>,
    benchmarks: BTreeMap<String, HpackComponent>,
    /// FR-001b: present-and-non-null means the dependency set is knowably
    /// incomplete. The VALUE is never resolved (FR-001c).
    defaults: Option<serde_yaml::Value>,
}
```

`!include` cannot be modelled as a field: it is a YAML tag rather than a
mapping key. Detection is therefore a scan of the document for the tag,
recorded on `HpackReadOutcome` below — not a `#[serde]` attribute. R1
measured that `!include` resolves from a sibling file on disk, unlike
`defaults:`; FR-001b announces both, so this milestone only needs to
*detect* it.

**Measured in T004, and the earlier inference was wrong.** This section
previously recorded as inference that `serde_yaml` rejects an unresolved
`!include` tag as a deserialization error, which would have made detection a
scan of the file text. Two unit tests
(`haskell/hpack.rs::include_tests`) measured it:

1. `serde_yaml` 0.9 parses the tag into a **`Value::Tagged`** —
   `TaggedValue { tag: !include, value: String("common.yaml") }`. It is not
   an error at the `Value` level, so detection reads the **parsed document**,
   and a text scan is unnecessary.
2. Deserializing the same document straight into a typed field **fails**,
   with `invalid type: string "common.yaml", expected a sequence` — an error
   that says nothing about `!include`.

The second result sets the parse order, and is why guessing would have been
costly rather than merely untidy. Taking the typed path first reports an
`!include` manifest as **unparseable** (FR-003) when it is **partial**
(FR-001b) — two different requirements, two different signals, and an
operator told to fix a file that is not broken.

So the reader: parses the text to `serde_yaml::Value` once, walks it for
`Value::Tagged` nodes to detect unmodelled constructs, and deserializes
that same `Value` into `HpackManifest` afterwards. One parse, one cheap
walk.

```rust
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HpackFlag {
    description: Option<String>,
    manual: bool,
    default: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct HpackComponent {
    dependencies: Vec<String>,
    /// R1/A6: measured incapable of changing the dependency set. Read for
    /// evidence and source paths only.
    source_dirs: OneOrMany<String>,
    when: Vec<HpackWhen>,
}

#[derive(Debug, Deserialize)]
struct HpackWhen {
    /// A YAML scalar: the literal `true`/`false`, or a Cabal condition
    /// expression such as `os(darwin)` or `flag(fast)`.
    condition: serde_yaml::Value,
    #[serde(default)]
    dependencies: Vec<String>,
}
```

`source_dirs`, `dependencies` and `when` each accept a scalar or a
sequence in real manifests, so each needs an untagged either-or
deserializer. `OneOrMany<T>` is a small local helper, not a new crate.

### 1.2 `HpackReadOutcome` (new)

What the reader did, so FR-003 and FR-005 have something to report. This is
the carrier that gets the unread-manifest fact from the reader to the
completeness pass — R6 established that the completeness pass cannot infer
it, because at that point the *reason* the component set is empty is out of
scope.

```rust
pub(crate) struct HpackReadOutcome {
    /// Every `package.yaml` the walker dispatched to this reader.
    pub(crate) seen: Vec<PathBuf>,
    /// Parsed and lowered successfully.
    pub(crate) read: Vec<PathBuf>,
    /// FR-003: present, dispatched, and NOT read. Path + why.
    pub(crate) unreadable: Vec<(PathBuf, UnreadableReason)>,
    /// FR-001b: read, but a construct outside the modelled subset could
    /// change the dependency set. Path + the construct's name.
    pub(crate) partial: Vec<(PathBuf, UnmodelledConstruct)>,
    /// FR-002: a sibling hpack-generated `*.cabal` suppressed because the
    /// manifest is the source of truth.
    pub(crate) suppressed_cabal: Vec<PathBuf>,
}
```

### 1.3 `UnreadableReason` / `UnmodelledConstruct` (new enums)

Typed, per Principle IV; serialized only at the emission boundary.

```rust
pub(crate) enum UnreadableReason {
    /// `serde_yaml` refused the document.
    MalformedYaml(String),
    /// Read as YAML but not as a manifest (e.g. no `name:`).
    NotAManifest(String),
    /// Could not be opened.
    Unreadable(String),
}

pub(crate) enum UnmodelledConstruct {
    /// FR-001c: resolving it is a network fetch. R1/A7 measured that hpack
    /// itself refuses the whole manifest when it cannot fetch.
    Defaults,
    /// R1/A7 measured this resolves from a sibling file on disk, so it is
    /// a cheaper later win than `Defaults`. Announced, not resolved.
    Include,
}
```

A `when:` whose condition cannot be evaluated is **not** in this enum. R1
measured that hpack does not evaluate those either — it lowers them to a
Cabal `if`, and waybill's existing flag-default machinery handles exactly
that. An undecidable `when:` yields a *conditional stanza*, which is a
complete answer about a conditional dependency, not a gap. (The spec's
edge-case list raises this; the measurement is what resolves it.)

---

## 2. Changed existing types

### 2.1 `CabalManifest` — one field

```rust
 struct CabalManifest {
     name: Option<String>,
     …
+    /// Which file this manifest was read FROM, so FR-002's choice is
+    /// recordable and a suppressed `*.cabal` is distinguishable from one
+    /// that was read.
+    origin: ManifestOrigin,   // Cabal | Hpack
 }
```

No other field changes. The lowering populates `stanzas`,
`default_buildable`, `license` and `skipped_entries` through the same
shapes the `.cabal` parser produces, which is what makes every downstream
pass apply unchanged.

### 2.2 `PackageSetEntry` — one field (US3)

```rust
 pub(crate) struct PackageSetEntry {
     pub(crate) version: String,
     pub(crate) runtime_relations: Vec<String>,
     pub(crate) source_hash: Option<String>,
+    /// FR-010. `None` ONLY when the declaration asserts the license is
+    /// not known (FR-011a) — never when the field was simply not parsed,
+    /// because Probe B measured that every derivation block declares one
+    /// and a `None` from a parse miss would be indistinguishable from a
+    /// genuine absence.
+    pub(crate) license: Option<DeclaredLicense>,
 }
```

### 2.3 `DeclaredLicense` (new, US3)

The three measured families (Probe B), kept apart so FR-011's
"uncanonicalised" fact is a type state rather than a convention.

```rust
pub(crate) enum DeclaredLicense {
    /// Canonicalised. 3,962 SPDX-id declarations + the 48 bare strings
    /// that parse + the 14,481 attribute references that map.
    Canonical(SpdxExpression),
    /// FR-011: preserved as declared, recorded as uncanonicalised. The
    /// measured population is 586 unversioned strings (`"GPL"`, `"LGPL"`,
    /// `"AGPL"`) plus `publicDomain` (313), which has no clean SPDX id.
    Uncanonical(String),
}
```

`"unknown"` (360) produces **neither**: FR-011a makes it `None` on the
entry. It is not a license, and publishing the string would defeat a
consumer filtering for components with no license information.

### 2.4 `EnrichmentSummary` / `ClosureSummary` — the withdrawal account

FR-007 and FR-008 need the document to distinguish three states that are
one state today. The carrier is the existing document-scope value, extended
additively:

```rust
 pub(crate) struct EnrichmentSummary {
     …
+    /// FR-008. `None` = the reconciliation did not run. `Some` with an
+    /// empty `withdrawn` = it ran and withdrew nothing. These are the
+    /// same output today, which is the condition under which a feature
+    /// that CANNOT fire looks like a feature with nothing to do.
+    pub(crate) optional_reconciliation: Option<OptionalReconciliation>,
 }

 pub(crate) struct OptionalReconciliation {
     /// Marks under review: the components carrying
     /// `cabal-flag-disabled-stanza`.
     pub(crate) reviewed: usize,
     /// FR-007: each withdrawal and the non-seed parent that justified it.
     pub(crate) withdrawn: BTreeMap<String, String>,
     /// FR-009: declined, so the mark and its derivation both stand.
     pub(crate) declined: usize,
 }
```

`BTreeMap` for `withdrawn`, not `HashMap`: the value reaches the document
and a golden has to be able to pin the string. This mirrors
`ClosureSummary`'s own reasoning, recorded in its doc comment.

### 2.5 `ReasonCode` — one variant (FR-005)

```rust
 pub enum ReasonCode {
     …
+    /// FR-005. A manifest was dispatched to a reader and not read, so the
+    /// component set's emptiness is a reader outcome rather than a
+    /// property of the project.
+    ManifestUnread { ecosystem: &'static str, count: usize },
 }
```

A new `ReasonCode` variant renders into the **existing**
`waybill:graph-completeness` value through `as_str`, so it adds a reason
code and **not** a catalog row. R6: that is the point — FR-019 warns that a
documentation row ahead of emission code breaks
`parity::extractors::tests::every_catalog_row_has_an_extractor` by design.

**No new catalog row is required by this milestone.** Every signal lands in
`waybill:graph-completeness` (C-row already extracted),
`waybill:unresolved-reason` (C151), or the nixpkgs document-scope records
(C173/C175, already extracted).

---

## 3. The withdrawal, as data (FR-006)

R3's decision stated as a transformation. Today:

```
in_closure  :=  { e.from } ∪ { e.to }   for every edge e
withdraw c  ⟸  c.marked ∧ c.name ∈ in_closure
```

A marked component is a closure **seed**, so the walk expands it and it
appears as `e.from` of its own out-edges. `c.name ∈ in_closure` is therefore
true by construction for every marked component, and the test can only
confirm the marks it was handed.

After:

```
marked      :=  { c.name : c.marked }
seed'       :=  declared \ marked
closure'    :=  walk(seed', lookup)            // one extra BFS, no new I/O
withdraw c  ⟸  c.marked ∧ c.name ∈ closure'.members
justify c   :=  any parent in closure'.reached_from[c.name]
```

Why re-seeding rather than a cheaper local test — both alternatives were
considered and rejected in R3:

- **`e.to` only.** Still unsound: a marked component reached by another
  marked component is a `to`, so a pair of marks confirms itself.
- **`reached_from` with one parent outside `marked`.** Correct for a
  directly-marked component, wrong one level down. Probe D measured 8
  non-boot components reachable from the root *only* through the marked
  pair; each has a parent that is not itself marked, so this test withdraws
  nothing for the pair and still publishes all 8 as runtime-reachable. The
  error fans out, so the test must be global.

`closure'` is computed from the same in-memory package set, so the cost is
one BFS and no fetch. It is also what #1281 proposed independently.

---

## 4. What does NOT change

Named because each is a constraint a plausible implementation would breach.

| invariant | why |
|---|---|
| the per-revision cache's on-disk format | milestone 926's layout; this milestone parses one more field out of a file it already reads |
| `stanza_lifecycle_scope` | `TestSuite`/`Benchmark` → `Development`. R1/A1 measured that top-level `dependencies` reaches test and benchmark stanzas, so hpack test deps flow through this unchanged — do not special-case them |
| `DeclaredDep.range` / `all_ranges` | R1/A4: hpack rewrites ranges (`>= 4 && < 5` → `==4.*`); waybill records what the manifest declares. Do not reproduce the normalisation |
| the C151 reason strings | FR-004a changes the hpack *warning*, not these. #956 chose them per toolchain and that reasoning is untouched |
| every other ecosystem's dev/optional axis | FR-015, SC-007. R7 names the live hazard: milestone 1272 measured that folding an extra section into a runtime seed silently reassigns Optional → Development. The re-seeded closure must remove the marked names **and change nothing else about the seed** |
| `--offline` | FR-001c, SC-010. No construct in the modelled subset reaches the network, and `defaults:` is announced rather than resolved |
