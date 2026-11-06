# Phase 0 research: licenses for nix closure components

Every number here was executed against the pinned `nix-closure-moat` closure
(1,749 derivations, 604 of them `pname`-bearing, system `x86_64-linux`) and
nixpkgs `a799d3e3`, the revision that target's `flake.lock` pins. Probes are in
`measurements/`.

## R1 — the package name scopes the scan; the store path is the join

sbomnix's `package_meta.nix` takes `lookupKeys` (package names) and
`inputRoots`, and ends at:

```nix
rowsForLookup = lookup: concatMap (root: rowsForLookupInRoot root lookup) roots;
derivations   = concatMap rowsForLookup lookupKeys;
```

The scan is scoped to the names asked for, across a set of roots; each row
carries `drvPath`/`outPath`, and the join back to the closure is by store path.
**The name is a hint that bounds the scan, never the join key.** This matters
because it avoids enumerating all of nixpkgs — see R4 for what enumeration
costs — while still satisfying FR-002's exact-store-path requirement.

**Decision**: adopt the same split. Names scope; store paths join.

## R2 — `tryEval` is not a sufficient guard

`builtins.tryEval` catches `throw` and `assert`. It does **not** catch a
missing attribute or a type error, and both occur in this population. Three
failures were observed in sequence, each escaping the guard before it:

| Shape | Error | Why `tryEval` misses it |
| --- | --- | --- |
| `p.drvPath` on a set without it | `attribute 'drvPath' missing` | not a throw |
| a `null` alias in `pkgs` | `expected a set but found null` | type error, not a throw |
| a throw forced during JSON output | varies | `tryEval` forces only to WHNF |

The working guard set, in order:

1. `builtins.hasAttr name root.set`, ahead of any access.
2. `builtins.isAttrs p && p ? drvPath` — structural, ahead of any forcing.
3. `builtins.tryEval (builtins.deepSeq v v)` — `deepSeq` **inside** the
   `tryEval`, so a throw in a lazy field surfaces while the guard is still in
   scope instead of during serialisation.

Milestone 1050 recorded two `tryEval` traps; this is a third, and the rule
generalises: a guard that catches one error class is not a guard.

A fourth hazard was hypothesised and **disproved**: that a package name
containing a dot (`M0.hex2` is in this closure) would be split into an
attribute path. It is not — dynamic access does not split on dots, so
`s.${n}` and `builtins.getAttr n s` are equivalent for every name:

```
s = { "a.b" = "DYNAMIC-OK"; a = { b = "PATH"; }; }; n = "a.b";
s ? ${n}  => true          s.${n}             => "DYNAMIC-OK"
hasAttr n s => true        builtins.getAttr n s => "DYNAMIC-OK"
```

This matters beyond style: `is_safe_attribute_name` admits `.`, and the
existing `versions_expr` uses `hp.${n}`. Had the hypothesis held, that would
have been a latent defect in shipped code. It does not, and there is nothing
to fix there.

## R3 — an attribute named `outPath` is string-coerced

An attrset carrying a key named `outPath` is coerced to a string by `nix eval
--json`, silently replacing the row. Name the field something else (`out`).

## R4 — name-scoped vs enumeration, measured

Identical guards, identical roots, identical join (exact store-path basename),
3 runs each, warm cache:

| Shape | Wall (median) | Rows | Named nodes joined | Of the 96-component gap |
| --- | --- | --- | --- | --- |
| Name-scoped (478 names) | **1.05 s** | 747 | 228 / 604 (37.7%) | 67 / 96 (70%) |
| Full enumeration (3 roots) | **165.64 s** | 38,683 | 261 / 604 (43.2%) | 82 / 96 (85%) |

Enumeration costs **158×** the wall time for **+5.5 pp** of named nodes and
**+15 pp** of the gap. Zero store paths carried conflicting licenses across
roots in either shape, so the join is unambiguous where it lands.

**Decision**: name-scoped is the default. Enumeration is worth exposing as an
opt-in for a compliance reviewer who wants the last 15 pp and can pay three
minutes for it; it is not worth putting in the default scan path.

**Caveat on the gap column**: those two figures join by name+version parsed out
of the derivation basename, not by the store path FR-002 mandates. They measure
whether the license data is *reachable* in each shape, which is what the choice
between the shapes turns on. They are an upper bound on the exact-join yield,
and the task that implements the real join must re-measure it.

## R5 — the cap is `pname` ≠ attribute name

The closure contains `autoconf-2.69`. nixpkgs exposes it as the attribute
`autoconf269`, whose `pname` is still `autoconf`:

```
attr=autoconf     pname=autoconf  drv=31ziv3nb…-autoconf-2.73.drv
attr=autoconf269  pname=autoconf  drv=0kkywv8w…-autoconf-2.69.drv
closure has:                          04pp7161…-autoconf-2.69.drv
```

A lookup keyed on `pname` finds `autoconf` 2.73 and misses 2.69 entirely. This
is the structural reason the name-scoped shape caps at 37.7% rather than
approaching the enumeration figure, and it is exactly the case R1's
name-is-a-hint design anticipates.

Note the third line: even the *correct* attribute's hash differs from the
closure's. A `drvPath` encodes the whole transitive input closure, so the same
package at the same version built in a different context is a different store
path. This is a property of the join, not a defect, and it is why FR-002's
exactness is the right call — but it also means coverage can never reach 100%,
and no success criterion should imply otherwise.

## R6 — roots must follow the target, not a fixed list

The moat's flake builds from `pkgs.haskell.packages.ghc94/96/910`, **not**
`pkgs.haskellPackages`. Its `flake.nix` takes plain upstream nixpkgs at the
locked revision with no overlays, so evaluating upstream is sound; but the
package *set* within it is target-specific.

Measured here, adding the three `ghc*` roots produced 467 further rows and
**zero** additional joins — at this revision `haskellPackages` already resolves
to the same derivations, and root-iteration order credits it first. That is a
property of this target, not a general one. Treat the root list as an input to
be discovered per target, with `pkgs` plus the default Haskell set as the
floor.

## R7 — the 96-component gap is confirmed

The spec's headline figure reproduces: joining waybill's emitted document to
sbomnix's by `(name, version)`, **96** components are licensed by sbomnix and
unlicensed by waybill. The figure is stable under a name-only join (also 96).

An intermediate count of 58 obtained during this research was wrong: it indexed
components into a dict keyed by `(name, version)`, which silently kept only the
last component per key and dropped licensed duplicates on the sbomnix side,
which emits 1,274 components with many names repeated across store paths. Group
into lists before asking whether any carries a license.

Also confirmed, and already stated correctly in the spec: waybill's license
*rate* (33.2%) is higher than sbomnix's (23.6%). sbomnix leads on absolute
licensed count (301 vs 137) because it emits roughly three times the
components, not because it licenses a larger share of them.

## R8 — the name-scoped evaluation already exists in the tree

`waybill-cli/src/scan_fs/package_db/nix/eval/invoke.rs` already builds a
name-scoped nixpkgs evaluation, for versions, behind the `Evaluator` trait:

```nix
let
  pkgs = (builtins.getFlake "github:NixOS/nixpkgs/{revision}").legacyPackages.{system};
  hp = pkgs.haskellPackages;
  ver = n: let r = builtins.tryEval (if hp ? ${n} then (hp.${n}.version or null) else null);
           in if r.success then r.value else null;
in builtins.listToAttrs (map (n: { name = n; value = ver n; }) [ … ])
```

R1's design is therefore already the house pattern, which settles the shape
question. Two differences decide that this is a **new trait method** rather
than a widening of `evaluate_versions`:

* **Return key.** `evaluate_versions` returns `BTreeMap<name, Option<version>>`,
  keyed by name. FR-002 requires the join to be by store path, so the license
  method must return `drvPath`/`outPath` per row. Re-keying the existing method
  would change its contract for its current caller.
* **Roots.** `versions_expr` reads one root, `pkgs.haskellPackages`. The
  license population needs `pkgs` as well — 104 of the 228 measured joins come
  from `pkgs` and would be lost to a Haskell-only root (R6).

Everything protective around it is reused verbatim and must be: the
`is_safe_attribute_name` filter, `is_valid_revision`, `resolving_argv`,
`argv_is_safe`, the `--accept-flake-config` prohibition and `run_bounded`'s
budget. That scaffolding exists because a scanned flake can otherwise choose
nix's evaluation settings, and the comment block at `invoke.rs` records that
this is measured behaviour, not a hypothetical. A second evaluation that
rebuilt its own argv would be a second place for that to be lost.
