# Contract: the projection (FR-010)

## The rule

Walk `inputs.drvs`. At each dependency:

- it declares a `pname` → **stop**. If it is an emitted component, emit the
  edge; otherwise emit nothing.
- it declares no `pname` → **pass through** to its own `inputs.drvs`.

Anchor the walk additionally on the derivation nothing consumes, and attach
that one's projected edges to the **document root component** (FR-004).

## Why this rule and not another

A source tarball is not a dependency; it is how a dependency arrived. Passing
through it preserves the real edge. A package *is* a dependency, so passing
through it would assert `A depends on C` where nix said `A → B → C` — a
transitive-closure artefact rather than a stated fact, and Principle IX is why
that is not acceptable even though it reaches zero orphans.

## Properties

| # | Property | Why it is stated |
|---|---|---|
| P1 | A member reached only through a non-package derivation is connected. | The feature. Measured: without it, 381 of 381 components stay orphaned. |
| P2 | A member reached only through a **package** that is not emitted stays an orphan, with the FR-011 reason. | The deliberate residue — 5 on linux, 16 on darwin, one uniform cause. |
| P3 | No edge is emitted that nix did not state at some hop. | Principle IX. Rules out the full-transitive projection. |
| P4 | The root's edges attach to the document root component, with no identity matching. | A match would cross a PURL-type boundary; a wrong one reparents the whole graph silently (R5). |
| P5 | Two scans of one closure produce byte-identical edge sets. | `BTreeSet` over `.drv` paths, converted to PURLs once. Asserted by a test. |
| P6 | The C182 role census is unchanged. | Roles read declared attributes, edges read `inputs.drvs`. The separation is the invariant; SC-005 is its assertion. |
| P7 | A cycle terminates. | FR-006. Asserted against a fixture nix could not produce. |
| P8 | An `inputs.drvs` entry naming a derivation outside the closure is dropped. | FR-005. nix emits these. |

## Measured, both systems

| projection | edges (linux) | orphans | edges (darwin) | orphans |
|---|---|---|---|---|
| direct only | 3,088 | 381 of 381 | 3,091 | 388 of 388 |
| direct + root anchor | 3,105 | 7 | 3,107 | 20 |
| transitive through anything | 16,187 | 0 | 21,237 | 0 |
| **this rule** | **3,429** | **5** | **3,429** | **16** |

The identical 3,429 is a genuine coincidence, checked: the distinct source
counts (378 against 388) and maximum out-degrees (41 against 47) differ, so the
walk is reading its input.

## The test that must exist

The obvious test asserts an edge exists between two components and passes while
the feature is half-broken, because P2 and P4 are separate claims. Three
assertions are required:

1. a member reached only through a non-package derivation **is** connected;
2. a member reached only through an unemitted **package** is **not**, and
   carries the reason; and
3. the root's edges land on the document root component.

Each needs the control the closure suites already carry: that the fixture holds
the derivations it claims and that something was connected. A projection
producing nothing satisfies every "is not wrongly connected" assertion
vacuously.
