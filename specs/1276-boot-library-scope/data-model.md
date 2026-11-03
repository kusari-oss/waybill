# Data model: a package the compiler ships is never scoped as absent

**Milestone**: 1276 | **Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

All state is in-process per scan. The durable output is the emitted document.

---

## 1. Nothing new is parsed

| input | where it comes from | since |
|---|---|---|
| the nulled-name set (the exempt class) | `configuration-ghc-<series>.nix`, cached per revision | m926 |
| which series are candidates | `candidate_series`, textual scan of `flake.nix` | m926 |
| the declared adjacency | `Relationship` list, already built | m1209 |
| the immunity concept | `runtime_reachable` in the propagation | m1209/m1269 |
| "the compiler supplies this" per component | C170, already emitted | m926 |

The feature reads one set that is already computed and discarded (R5) and adds
one condition to a predicate that already exists.

## 2. The exempt class

```
exempt = the nulled names of the series the flake names,
         or, when it names none, their union across candidate series
```

Both halves are already produced by `boot_set` / `candidate_series` (R2), so
this is a definition rather than a mechanism. Measured at revision
`a799d3e3`: 41 names for `9.10.x` alone, 50 for the union, 32 nulled in every
series. The gap between 41 and 50 is the measured cost of the fallback and the
figure SC-006a distinguishes a correct implementation by.

**Membership is per revision by construction.** A package the compiler ships
at one revision and not another is exempt only where that revision says so,
which is what FR-004 forbids a maintained list from approximating.

## 3. What the exemption changes, and what it must not

| field | change | requirement |
|---|---|---|
| `lifecycle_scope` on an exempt component | a non-runtime value is **not assigned** | FR-001, FR-002 |
| `lifecycle_scope` on any other component | **unchanged** | FR-003 |
| `name`, `version`, `purl`, `hashes`, `licenses` | **unchanged** | FR-012 |
| the C170 reason | **unchanged** — already present, and the exemption's basis | FR-006 |
| edges | **unchanged** — the graph is not rewritten, only the scope | FR-003 |

**Not assigned, rather than assigned and then removed.** The component is
immune: the propagation should never reach it, the way it never reaches a
runtime-reachable component. A repair after the fact would leave the document
and the invariant disagreeing about one component (R1).

An exempt component is left with **no scope**, not an explicit `required`. The
schema states an unspecified scope SHOULD be assumed `required`, so the two
are equivalent to a conforming consumer, and 68 of the 78 boot-library
components already carry no scope — removing the wrong value makes the 10
consistent with the 68 rather than changing 68 that are already correct.

## 4. The two invariants partition the class

This is the part Phase 0 found and the design turns on.

| invariant | before | after |
|---|---|---|
| **I3** — reachable only through non-runtime edges ⟹ carries a non-runtime marker | holds, including for the 10 | **amended**: the same exempt class is excluded from its population |
| **FR-002a's new invariant** — compiler-supplied ⟹ carries no non-runtime scope | does not exist | added; I3's complement |

Together they say: every component reachable only through non-runtime edges
carries the marker **unless** the compiler supplies it, and no component the
compiler supplies carries it at all. Every component satisfies exactly one of
the two, which is the property I3 already documents for its boundary with I7 —
and which it records getting wrong once, when "two instances of one defect
class landed under the wrong number".

**Both must read the exempt class from one source.** Two rows disagreeing
about which components are exempt is worse than either rule being absent,
because each would then be locally satisfiable while the pair is not.

## 5. Where the set comes from at the point it is needed

`enrich` destructures `BootSet { boot, … }` at `mod.rs:941`, classifies with
it, and drops it. The propagation runs later, at `scan_cmd.rs:4851`, and the
defect occurs **without** `--nix-closure` — so milestone 1275's boot-set load,
which lives inside the `--nix-closure` arm, is not reachable here.

```
enrich ──BootSet.boot──> EnrichmentSummary.boot_libraries ──> propagate_non_runtime_scope
```

One field, one assignment, the same shape milestone 1275 used for
`package_set` and for the same reason: the value is already in hand and
re-deriving it would mean re-reading the configurations.

When `enrich` did not run or degraded, the field is `None`, the exempt set is
empty, and nothing is exempted (FR-005). That is the fail-closed direction:
the dangerous default is treating a component as immune without evidence.

## 6. What the document gains

| carrier | what it says | status |
|---|---|---|
| C170, per component | the compiler supplies this package | **existing**, reused (FR-006) |
| a new document-scope row | how many components the rule exempted, including zero | **new** (FR-007), audited in R4 |

The count is at document scope because its reader is comparing two documents
of one project across waybill versions, and needs to see that a rule changed
rather than the project. `PropagationSummary` already holds the figure; it is
logged and never emitted, which is the gap C174 exists to close for the
neighbouring pass.

## 7. What does NOT change

| invariant | why |
|---|---|
| 569 of the corpus's 579 non-runtime scopes | FR-003. Only the 10 move |
| every other ecosystem's output | SC-004; OS-package and file-tier components carry no scope at all |
| the dependency graph | the scope is corrected, the edges are not touched |
| the meaning of `required` / `optional` / `excluded` | FR-012. What changes is which components carry them |
| C170, C42 and C62's definitions | C42's and C62's *values* change for 10 components; their meanings do not |
