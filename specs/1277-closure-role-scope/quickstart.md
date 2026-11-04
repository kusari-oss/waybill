# Quickstart — closure-role-derived scope

**Milestone**: 1277 · **Issue**: #1342

## Reproduce the defect

```sh
cargo build -p waybill --bin waybill
python3 -I specs/1277-closure-role-scope/measurements/roles-and-scopes.py
```

That reads the committed corpus goldens — no scan, no network, no `nix` —
and prints the role distribution plus every component that already carries a
scope. Expected before the feature:

```
### nix-closure-moat: 387 components, 362 with a closure-role
    artifact-input     231   pkg:hackage of those: 131
    both                34   pkg:hackage of those: 2
    build-tooling       80   pkg:hackage of those: 1
    unreferenced        17   pkg:hackage of those: 0
    role+scope pairs present today: {('artifact-input', 'excluded'): 9, ('both', 'excluded'): 1}
```

**97 of 362** members are in a role that does not ship and carry no scope.

## Four things to hold on to

1. **#1342's premise is wrong about ten components.** It says no closure
   member carries a native `scope`. Ten do — nine `artifact-input`, one
   `both`, all test frameworks, scoped by milestone 1269's propagation
   through the *cabal* graph. FR-002 exists because of them.

2. **`artifact-input` does not mean "in the artifact".** `ARTIFACT_FIELDS`
   is `buildInputs`, `propagatedBuildInputs`, `depsHostHost`
   (`closure/classify.rs:13`) and nixpkgs' Haskell builder routes test
   dependencies through `buildInputs`. `hspec` is `artifact-input` and does
   not ship. This feature may only *add* a non-runtime scope.

3. **The obvious one-line fix is measured wrong.** Setting
   `lifecycle_scope = Build` gets all three native carriers for free, but
   the CDX emission is gated on `--include-dev` (`builder.rs:1400`) so the
   feature would not fire by default, and `--exclude-scope build` would then
   **drop** the 97 components — including the `unreferenced` ones m1035
   emitted precisely because `jq` and `lua` there carried 6 of 18 CVEs.
   Follow milestone 112 instead (research R1–R2).

4. **The exemption's population is zero, so the corpus cannot test it.**
   One `pkg:hackage` component sits in a tooling role — `happy@2.1.7` — and
   it is not compiler-supplied (`happy` appears only as an alias in the
   pinned configurations, never as `happy = null`). A corpus assertion would
   pass whether FR-005 works or not. Milestone 1275's F7 is the precedent;
   SC-007 demands a fixture built to make the rule load-bearing.

## Running the suites

```sh
cargo test -p waybill --test closure_role_scope
cargo test -p waybill --bin waybill closure_role_scope     # the unit tests
cargo test -p waybill --test nix_config_closure            # must stay green
cargo test -p waybill --test scope_exemption_record        # m1276, must stay green
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` argument (#1189).

**`scan_fs` and `resolve` unit tests live in the BIN target, not the lib.**
`cargo test -p waybill --lib <filter>` reports `0 passed; … N filtered out`
for a test that exists and runs perfectly well under `--bin waybill`. The
filtered-out count looks like a filter typo rather than the wrong target.

## The committed probe

`#[ignore]`d and env-gated, because it reads a real closure, which no
ordinary test may do (Principle VII). It is an instrument, not a behaviour
test: FR-004a requires the field-list coverage be *measured*, and a key
found outside both lists is a finding to scope rather than a test failure.

```sh
# FR-004a — do the classifier's 7 field names cover what real derivations carry?
WAYBILL_T_CLOSURE_FIELDS=/path/to/closure.json \
  cargo test -p waybill --bin waybill m1277_field_coverage -- --ignored --nocapture
```

Producing the input needs a host `nix`:

```sh
(cd <project> && nix --extra-experimental-features 'nix-command flakes' \
    --option allow-import-from-derivation false \
    derivation show -r '.#default' > closure.json)
```

**Do not grep the in-tree fixtures for this.** They carry only the seven
keys the code already knows, because they were written from those lists — so
the answer they give is the answer they were built from.

## Goldens

One corpus target moves: `nix-closure-moat`. Refresh through CI only —
`gh workflow run "Public corpus regression" -f branch=<branch> -f regen_goldens=true`
— never locally (rule zero). Audit the artifact by enumerating the files per
target (#1286) and by the files written rather than any SHA the run reports
(#1287); and note that in regen mode the summary's "Drifted" heading lists
failed tests, not changed goldens (#1345).
