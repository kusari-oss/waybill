# Contract: reaching the declarations

## Invocation

One evaluation for the whole closure, never one per member. A single Nix
expression lists every distinct `pname` and is evaluated once — measured at
0.6–1.1 s for 376 names across four package sets. Per-member invocation would
be 376 process spawns and is not an option.

The expression is evaluated against **the project's own pinned nixpkgs**,
reached through the flake's inputs, not against a registry or channel nixpkgs.
A declaration from a different revision describes a different package set and
would be wrong about this build.

## Guards, and why each exists

Both were found by measurement, and each one silently destroys the result when
absent:

- **`or null` on the attribute lookup.** A missing attribute is not a `throw`
  and escapes `tryEval`. Without it the whole evaluation dies on the first
  name that is not a top-level attribute — measured, `ChasingBottoms`.
- **`deepSeq` inside `tryEval`.** `tryEval` returns a lazy value, so a throw
  escapes at serialisation time, outside the guard. Without it the evaluation
  dies on the first unfree or broken package.

The same two guards appear in `measurements/known-vulnerabilities.sh` with the
same comments. They are not defensive style; each was a failed run.

## Path comparison

Both sides MUST be normalised through
`closure::derivation::store_basename` before comparison.

The closure JSON stores output paths **without** the `/nix/store/` prefix;
evaluation returns them **with** it. Comparing raw forms yields zero matches,
which presents as "the mechanism does not work" rather than "the strings are
formatted differently" (research R4).

## Resolution order

`top-level → haskellPackages → python3Packages → perlPackages`, first
*confirmed* hit wins. A hit that fails the path check does **not** stop the
search — a name collision in one set must not mask a real match in the next.

## Degradation

Reuses milestone 1034's `DegradationReason` vocabulary unchanged. The
declaration pass degrades independently of the closure query: a closure that
resolved successfully still emits its components when this pass fails, with
the reason recorded (FR-019). No new reason variants unless a measured failure
mode fits none of the existing eight.
