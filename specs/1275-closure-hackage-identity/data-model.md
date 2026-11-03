# Data model: closure members that are known Hackage packages

**Milestone**: 1275 | **Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

All state is in-process per scan. The durable output is the emitted document.

---

## 1. Nothing new is parsed

Every input already exists in memory or in the per-revision cache:

| input | where it comes from | since |
|---|---|---|
| `PackageSetEntry.version` | `hackage-packages.nix`, cached per revision | m926 |
| `PackageSetEntry.source_hash` | same | m926 |
| `PackageSetEntry.license` | same | m1273 |
| `PackageSetEntry.runtime_relations` | same | m985 |
| `Derivation.env` (whole map) | `nix derivation show`, already parsed | m1035 |
| boot-library identification | nulls in the revision's configuration | m926 |

No struct gains a field for storage. The pass is a transformation over
`Vec<ResolvedComponent>`.

## 2. The reclassification decision

A pure function of two inputs, with an explicit outcome per component so that
FR-007's reason is a value rather than a reconstruction:

```rust
/// Why a closure member was or was not given a Hackage identity.
///
/// An exhaustive enum rather than `Option<Purl>` plus a comment: FR-007
/// requires every unreclassified member to say which of these it was, and a
/// `_ => None` arm is how a later variant silently loses its reason (#1218,
/// #892).
enum HackageIdentity {
    /// FR-001 — the default attribute's version equals the closure's, and the
    /// derivation was built by a GHC. Both signals, per FR-005.
    Confirmed { attribute: String },
    /// FR-004 — the name is in the set under a different version. The
    /// collision population: `bash 5.3p9` against Hackage's `bash 0.1.8`.
    VersionDisagrees { set_version: String },
    /// The exact pair matched but the derivation was not built by a GHC, so
    /// the two signals disagree. Measured 0 of 60 — kept because a variant
    /// that cannot occur today is cheaper than discovering it occurred.
    SignalsDisagree { set_version: String },
    /// The pinned set has no attribute of that name at all. 190 of 276.
    NotInPackageSet,
    /// nix recorded no version, so no pair could be formed. 14 of 276.
    /// Distinct from a failed lookup: the lookup was impossible.
    NoVersionRecorded,
    /// FR-009 — the set itself was unavailable. Every member lands here, and
    /// the document says the pass did not run.
    PackageSetUnavailable,
}
```

Only `Confirmed` changes a component. Every other variant leaves it exactly as
it is today and supplies FR-007's reason.

## 3. What `Confirmed` changes, and what it must not

| field | change | requirement |
|---|---|---|
| `purl` | `pkg:generic/x@v` → `pkg:hackage/x@v` | FR-001 |
| `licenses` | set from the declaration | FR-002 |
| `hashes` | set from `source_hash` | FR-003 |
| relationships | gain declared out-edges | FR-012 |
| `name`, `version` | **unchanged** | FR-011 |
| role annotation | **unchanged** | FR-011, SC-006 |

`version` is untouched by definition: reclassification happens only when the
closure's version and the set's already agree, so there is nothing to rewrite.
That is what keeps this feature clear of #1216's defect class, where a dedup
preferred one identity and discarded a real version.

## 4. The PURL rewrite carries its edges

Changing a PURL orphans every relationship holding the old one. #980 measured
the failure: CycloneDX keeps an edge pointing at the old PURL, SPDX drops the
relationship outright, and **neither format errors**.

So reclassification produces a rename map and applies it in the same step,
through the existing `apply_renames` helper:

```
old PURL -> new PURL          applied to every Relationship::from/::to
pkg:generic/aeson@2.2.4.1  ->  pkg:hackage/aeson@2.2.4.1
```

Not a follow-up pass. A document with orphaned edges validates, so an ordering
mistake here is invisible to every schema gate.

## 5. Edges resolve by name, not by the set's version

The package set declares dependencies as attribute names. The target's version
in the **document** is whatever nix shipped, which differs from the set's for
**165 of 278** measured targets — the closure carries the version GHC actually
used.

```
declared: aeson -> text            (an attribute name)
document: text 2.0.2               (what GHC shipped)
set:      text 2.1.2               (what the set declares)
edge target: the component present -> text 2.0.2
```

Binding to the set's version would point 59% of edges at components that do
not exist. Binding by name is therefore the rule, and FR-012c covers the
remainder: **13 of 278** targets are absent from the document entirely, and
all 13 are GHC boot libraries (`directory` ×11, `ghc-heap`, `pretty`).

## 6. A dangling target becomes a compiler-supplied component

Not dropped, not dangling. The target is emitted as a versionless component
carrying the reason that the compiler supplies it — the shape the design tier
already uses for boot libraries, which m1273's US4 test asserts:

```
pkg:hackage/directory          (no version — GHC ships it)
  waybill:unresolved-reason = <the compiler-supplied reason>
```

Reuses m926's `boot_libraries` identification rather than a new list, so the
set of boot libraries stays derived from the revision's own configuration.

## 7. Where the pass runs

**After** `merge_closure_components` (`cli/scan_cmd.rs:4729`), before emission.

Not before, and not inside the existing enrichment. R1: milestone 1035
measured that moving the merge earlier downgraded 41 PURLs from `pkg:hackage`
to `pkg:generic`. R2: `is_enrichable` only accepts versionless components, so
the existing pass would skip closure members regardless of order.

## 8. What does NOT change

| invariant | why |
|---|---|
| which derivations a closure contains, and their roles | FR-011, SC-006. Store references still drive roles — the m1066 decision |
| any component's `name` or `version` | FR-011. Reclassification requires agreement, so there is nothing to rewrite |
| the 65 already-resolved components | FR-015. They are produced by a pass this feature does not touch |
| every other ecosystem | SC-007 |
| the CLI surface, dependencies, network | FR-010, R9 |
| CPE | out of scope. Zero of the 65 resolved components carry one either (#1245) |
