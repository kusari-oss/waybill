# Contract: classifying and identifying a pname-less closure member

## Surface

A new module `closure/member.rs`, consumed by `emit.rs` and `graph.rs`.

```rust
pub(crate) enum MemberRole { Source, Package, Scaffolding, Fragment }

impl MemberRole {
    pub(crate) fn wire(self) -> &'static str;
    /// FR-013 — only these two may be dependency-edge endpoints.
    pub(crate) fn is_edge_endpoint(self) -> bool;
}

pub(crate) enum IdentityBasis { Content(String), Recipe(String) }

/// Total over pname-less members: every one receives exactly one role.
pub(crate) fn classify(drv: &RawDerivation) -> MemberRole;

/// `drv_key` is the closure map key, whose leading hash is nix's
/// content-address of the build recipe.
pub(crate) fn identity(drv: &RawDerivation, drv_key: &str) -> IdentityBasis;
```

## Obligations

**On the classifier**

* Total — never returns an error and never declines (FR-005). Every input gets
  a role; the four counts must sum to the member count.
* Reads only the derivation record: no network, no second evaluation, no
  maintained list of names (FR-006).
* Evidence order is fixed: content hash first, then `env.text`, then a
  version-shaped name, then fragment. A member bearing several signals takes
  the strongest.
* Reads the content hash from `outputs[<name>].hash` **and**
  `env.outputHash`. The first is the structured form and carries 549 of them;
  the second is the older spelling and carries 139. Reading only the latter
  undercounts the population fourfold (research, measurement README).

**On identity**

* The hash occupies the PURL **name segment**. A qualifier does not satisfy
  FR-002.
* The prefix length must be justified by measurement, not convention: the
  probe asserts zero collisions over full hashes, and a truncation is a
  different claim that must be re-measured.
* The component's `name` field keeps the derivation's own name, unmodified
  (FR-016).

**On emission**

* The gate being relaxed is `emit::emits_as_component`'s C182 role filter, not
  a `pname` test: `RawDerivation::pname()` falls back to the derivation's own
  `name` and returns `Some` for all 1,145 (research R8a). Both callers of that
  predicate must be relaxed — `components()` and `emitted_drv_keys()`.
* The merge map must be keyed by the identity, not by `(name, version)`. These
  members declare no version and keep their name unmodified, so the existing
  key merges all 56 members named `source` into one (research R8c).
* A member this feature adds must reach the orphan classifier **unexplained**,
  so that the FR-014 reason is the one this feature emits. Carrying
  `waybill:closure-role == "unreferenced"` makes it explained before any reason
  is assigned (research R8b).

**On the edge projection**

* `graph::stops` widens to `pname`-bearing OR `is_edge_endpoint`. Note that
  `stops` reads `declared_pname()` while `emitted_drv_keys` reads `pname()` and
  additionally applies the C182 role filter: these are already two rules, and
  the subset assertion is what holds them together, not a claim that they are
  one expression.
* The set passed as `project`'s `emitted` argument must be exactly the set
  satisfying that predicate — derived from the one rule, not a second copy of
  it (research R1).
* No edge may name a component absent from the document (FR-013).

## Non-obligations

* The classifier makes no claim that a role is stable across nixpkgs
  revisions. It describes what a derivation looks like in **this** closure.
* The role is not a security or trust statement.
* Licensing of these members is out of scope; milestone 1281 covers the named
  population.
