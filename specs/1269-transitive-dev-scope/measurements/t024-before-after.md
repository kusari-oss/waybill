# T024 — before/after on real documents

**Date**: 2026-10-25 | **Binaries**: `HEAD~1` (00b2f37c) vs `HEAD` (b5471684), both debug
**Invocation**: the corpus harness's own — `--offline sbom scan --path <cache_dir> --format cyclonedx-json,spdx-2.3-json,spdx-3-json --root-name <target> --root-version <short pin>`

Measured by emitting each target twice from the *same* cached input,
once with each binary, rather than diffing against committed goldens.
Goldens were generated in CI; three of these targets scan differently
on this host (see "Not comparable locally"), so a golden diff would
have measured the environment.

## Comparable, same input both sides

| target | components | marked | I3 violations | C199 `transitive` |
|---|---|---|---|---|
| `python-flask` | 108 → 108 | 22 → **77** | 51 → **0** | 0 → **55** |
| `rust-ripgrep` | 68 → 68 | 3 → 3 | 2 → **2** | 0 → **0** |
| `npm-express` | 370 → 370 | 303 → 303 | 0 → 0 | 0 → 0 |

- **`python-flask` is fixed.** 51 violations to zero, 55 components
  newly carrying the inferred-scope marker, component count unchanged.
- **`npm-express` is unchanged in every column**, which is FR-013 /
  SC-004 measured rather than argued. Its local component count is 370
  against the golden's 45 because this host's cached repo has
  `node_modules/` installed; that difference is present on *both* sides
  of the comparison and is not what was being measured.
- **`rust-ripgrep` is NOT fixed.** See below.

## Why `rust-ripgrep` did not move

Diagnosed by instrumenting the pass, not inferred:

```
immune=7  seeded_from=3  seeds=1  ecosystems_with_root={"cargo"}  assigned=0
phase B (optional) reached 11 components
skip pkg:cargo/ryu@1.0.18  why=declared  scope_is=runtime
skip pkg:cargo/itoa@1.0.11 why=declared  scope_is=runtime
```

Both violators are reached by Phase B and are not immune. They are
skipped because the cargo reader has already assigned them
`lifecycle_scope = Some(Runtime)` — and their only inbound edge is
`pkg:cargo/serde_json@1.0.128 -> …` with `data_type
package-database-depends`, from the same reader, which marked
`serde_json` **non-runtime**.

So the reader asserts both "`ryu` is a runtime component" and "the only
thing depending on `ryu` is not runtime". FR-003 forbids this pass from
overriding either assertion, so it correctly declines, and I3 keeps
flagging the consequence.

This is the mirror image of the case the spec already deferred — "a
declared non-runtime component that is itself runtime-reachable", three
of `pydantic`'s 76. Same class: a reader/declaration conflict, not a
graph question.

**The emitted document cannot distinguish the two populations.**
`waybill:lifecycle-scope` is emitted only for non-runtime scopes, so an
explicitly-Runtime component and an unscoped one look identical in
CycloneDX. All 101 violators across the five declared targets read as
"no scope property" in their goldens; only the in-memory state
separates them, which is why this was invisible until the pass ran.

## Not comparable locally

| target | local components | golden components |
|---|---|---|
| `haskell-security-advisories` | 62 | 251 |
| `haskell-language-server` | not run | 399 |
| `nix-closure-moat` | not run (needs `nix`) | — |

The local scan of `haskell-security-advisories` produces a quarter of
the golden's components, so its local "0 violations, 0 propagated" says
nothing about the fix — the graph it measured is not the graph CI
measures. Whether the haskell and nix violators are unscoped (fixable,
like flask) or explicitly Runtime (not fixable, like ripgrep) is
**unknown until the corpus lane runs**.

## Consequence for SC-002

SC-002 requires all five I3 declarations deleted. `rust-ripgrep`'s
cannot be, as the feature is specified. Three others are unmeasured.
Resolving this is a decision about FR-003, not an implementation
detail — recorded here rather than worked around.

## Resolution (2026-10-25)

`rust-ripgrep` is split out as **#1272** — "cargo reader: transitives of
a non-activated optional dependency are scoped Runtime". FR-003 stands
unchanged; the pass correctly declines, and the fix belongs in the
reader, which already has the data (milestone 205's `activated_names`)
and does not use it when building `prod_set`.

Consequences recorded in the spec rather than carried implicitly:

- **SC-001** gains a second named exception, population (b): components
  carrying a reader-assigned `Runtime`. It names `pkg:cargo/ryu@1.0.18`
  and `pkg:cargo/itoa@1.0.11` by coordinate, so the carve-out cannot
  quietly widen.
- **SC-002** and **FR-014** drop from five declarations to four.
  `rust-ripgrep`'s is retained, its `reason` rewritten to the measured
  cause and its `issue` re-pointed to #1272 — in this change, not a
  follow-up.
- The first spec assumption's escape clause ("if implementation finds
  any of them to be a different defect, that one is split out rather
  than silently carried") is what fired here, and now records the
  outcome.

The three unmeasurable-on-this-host targets (`haskell-language-server`,
`haskell-security-advisories`, `nix-closure-moat`) remain unmeasured
until the corpus lane runs under T037. If any of them turns out to be
ripgrep-shaped rather than flask-shaped, it joins #1272's population
and its declaration is re-pointed the same way — which is why T038
checks each cell individually rather than checking that the lane is
green.
