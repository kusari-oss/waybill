# Quickstart: 1280-nix-closure-edges

## See the defect

```sh
python3 - <<'EOF'
import json, collections
g = json.load(open('waybill-cli/tests/fixtures/public_corpus/nix-closure-moat/cdx.json'))
props = lambda c: {p['name']: p['value'] for p in c.get('properties', [])}
closure = {c['purl'] for c in g['components'] if 'waybill:closure-role' in props(c)}
targets = {d for e in g.get('dependencies', []) for d in e.get('dependsOn', [])}
sources = {e['ref'] for e in g.get('dependencies', []) if e.get('dependsOn')}
print(f"closure components:              {len(closure)}")
print(f"  ...that any edge points AT:    {len(closure & targets)}")
print(f"  ...that have outgoing edges:   {len(closure & sources)}")
dp = {p['name']: p['value'] for p in g['metadata']['properties']}
print(f"graph-completeness: {dp['waybill:graph-completeness']}")
print(f"reason: {dp['waybill:graph-completeness-reason']}")
EOF
```

Expect zero and zero: **381 components that nothing points at and that point at
nothing**, and a document that says `partial` with
`orphaned-components-detected: 352`.

## Reproduce the baseline

Needs a closure JSON. **Capture it once and read it from a file.** Running a
real `nix` against a flake in place can re-lock it: on 2026-11-04 that rewrote
a committed `flake.lock` and broke 11 tests in an unrelated file. When the
subject is a corpus clone, copy it first and verify the lock afterwards.

```sh
SRC=~/.cache/waybill/corpus/53983fe1021353b3/d06905558ba68b49b17f36c875e97f7234a9c19b/repo
cp -R "$SRC" moat-proj
md5 -q "$SRC/flake.lock" moat-proj/flake.lock   # must print the same hash twice

nix derivation show -r --option allow-import-from-derivation false \
    "$PWD/moat-proj#packages.x86_64-linux.default"  > closure-linux.json
nix derivation show -r --option allow-import-from-derivation false \
    "$PWD/moat-proj#packages.aarch64-darwin.default" > closure-darwin.json

python3 -I specs/1280-nix-closure-edges/measurements/input-graph-probe.py closure-linux.json
```

Expect on linux: 10,342 `inputs.drvs` edges against the declared view's 3,386;
1,748 of 1,749 derivations explained; one root, `moat-0.1`.

**Capture both systems.** They are different closures — 1,749 derivations
against 1,275 — and they populate different attributes. A figure from one
quoted against the other is wrong in membership, not magnitude.

## Verify an implementation

1. **Local gate**, the whole thing, **no `+toolchain`** (#1189):

   ```sh
   ./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
   ```

   Assert on `GATE_RESULT`, the expect line, and the per-target
   `test result: ok. N passed; 0 failed` lines. The absence of FAILED is not
   evidence.

2. **The corpus is not in that gate, and a local run is worse than none.**
   The target prints `skipping nix-closure-moat: WAYBILL_RUN_PUBLIC_CORPUS not
   set` and reports `ok` **in 0.00s** — a pass that checked nothing. Refresh
   through `public-corpus.yml` with `regen_goldens=true`, install, then dispatch
   again with `regen_goldens=false` and confirm the log's regen count is **0**,
   so the run is known to have compared rather than rewritten.

3. **Merge main into the branch before dispatching a refresh.** A branch cut
   before `48a5115e` cannot print the `corpus-regen-golden: … CHANGED`
   verdicts, so the run cannot tell you which goldens moved — it will report
   `success` and be useless.

4. **Prove the guard can fail.** Stop reading `inputs.drvs` and confirm a test
   goes red (SC-009). A graph source no test is sensitive to is not evidence
   that it is read.

## Traps

- **Build the edges AFTER the m1275 identity pass.** It rewrites 104 of 381
  component PURLs from `pkg:generic/…` to `pkg:hackage/…`, and
  `Relationship` is keyed on PURL strings. Edges built in the emitter dangle
  for 27% of the graph — and **no fixture catches it**, because the fixtures
  stub the package set out as empty. Research R1.
- **Do not read `inputs.srcs`.** A `src` is a store path, not a derivation, so
  it names no node in this graph. Only `inputs.drvs` keys are edges.
- **Anchor on the document root, not on the closure root's identity.**
  `moat-0.1` is already `pkg:hackage/moat@0.1` from the manifest reader; the
  closure would spell it `pkg:generic/moat@0.1`. A match across that boundary
  that goes wrong reparents the whole graph silently.
- **Five orphans left is correct.** SC-002 is 352 → 5, not → 0. If an
  implementation reaches zero, check whether it started passing through
  packages: that is the rejected projection, and it asserts edges nix never
  stated.
- **The role census must not move.** 268 / 121 / 45 / 1,315 at derivation
  level, 237 / 92 / 41 / 11 at component level. If it moves, `inputs.drvs` has
  leaked into classification, which FR-002 forbids.
- **m1277's 103 `BUILD_TOOL_OF` edges must stay 103.** They point member → root
  and cannot make anything reachable; that is why they have prevented none of
  the 352 orphans. Do not "fix" their direction here.
- **Check for a cycle guard even though nix cannot make one.** FR-006 asks for
  a visited set, and the fixture that tests it has to be hand-written.
- **The counts are dated and system-labelled.** SC-001, SC-004, SC-005 and
  SC-009 are the invariants; everything numbered is x86_64-linux as of
  2026-11-05.
