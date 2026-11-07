# Quickstart: verifying the build-graph admission signal

**Feature**: `1372-unreferenced-pname-components` | **Issue**: #1366

Read this before writing a test. The trap here is the opposite of milestone
1367's: that feature's effect was invisible (zero CPEs looked like no mapped
software), and this one's is **so large that a wrong implementation still
produces a plausible diff**.

---

## The trap: 641 relabellings make almost any diff look right

The honest summary of this change is *100 new components and 641 role
corrections*. A diff that size has cover for a lot of errors:

- Admitting a derivation **no** `inputs.drvs` names (over-admission) adds a
  component to a set that gained 45 anyway.
- Relabelling a derivation a declared attribute **does** name (FR-003a
  violation, SC-004a) changes a role in a document where 641 roles changed.
- Losing the closure root's `unreferenced` looks like one more correction.

So the controls here are not about proving something happened. They are about
proving the *right* things happened and nothing else did.

---

## Control 1 — the three exact counts

Every assertion below is a number measured on the pinned closure before
implementation. A run that produces different ones is a finding.

```sh
waybill sbom scan --path <moat clone at d069055> \
  --nix-closure --nix-closure-attr default \
  --format cyclonedx-json --output cyclonedx-json=/tmp/after.cdx.json

# roles, after
jq -r '[.components[] | .properties[]? | select(.name=="waybill:closure-role") | .value]
       | group_by(.) | map({(.[0]): length}) | add' /tmp/after.cdx.json
```

| | expected |
| --- | --- |
| components still `unreferenced` | **1** (`moat 0.1`, the closure root) |
| newly emitted | **100** derivations, ~45 distinct identities |
| relabelled, already emitting | **641** |
| C210 `admitted + relabelled` | **741** |

**The root is the control that matters most.** It is the only derivation
nothing consumes. If it stops being `unreferenced`, the signal is admitting on
no evidence and every other number is suspect.

---

## Control 2 — SC-004a, the one a large diff hides

FR-003a says the graph signal applies **only** where no declared attribute
names the derivation. Nothing in the 641-row diff would reveal a violation.

```text
For every component whose role changed:
    assert NO declared dependency attribute (ARTIFACT_FIELDS ∪ TOOLING_FIELDS)
    of any derivation names its output path.
```

This needs a fixture, not the corpus: construct a derivation named by **both**
`buildInputs` and some other derivation's `inputs.drvs`, and assert it keeps
`artifact-input` rather than becoming `build-tooling`. On the pinned closure
every such derivation already has a non-`unreferenced` role, so the real
closure cannot exercise the precedence rule.

---

## Control 3 — emission must not become role-dependent

The 603 milestone-1282 members emit through `declared_pname().is_none()`,
*not* through their role (research R4). Their role changes; their emission
must not.

```sh
# member count must be identical before and after
jq '[.components[] | select(any(.properties[]?; .name=="waybill:closure-member-role"))] | length' \
   /tmp/before.cdx.json /tmp/after.cdx.json
```

If this moves, something made emission depend on the role — which would be a
regression in milestone 1282's Gap A, reached from the opposite direction.

---

## Control 4 — the mutation check (SC-007)

```text
1. Make the graph signal a no-op                → the 100 must vanish; US1 tests red.
2. Apply the signal to ALL arms, not just
   (false,false)                                → SC-004a's fixture test must go red.
3. Drop the self-reference guard (V1)           → needs a fixture; the corpus has none.
4. Count a referrer that is itself not emitted
   as not counting (V2)                         → some of the 100 must vanish.
```

Mutation 2 is the one most likely to stay green, because the corpus cannot
produce its input. If it does stay green, Control 2's fixture is missing or
wrong.

---

## What to measure, beyond pass/fail

Three numbers belong in `measurements/README.md` because later work will be
diagnosed against them:

1. **Lifecycle scope changes.** Milestone 1277 derives scope from the role, so
   741 role changes move scope too — but not one-for-one, because scope has
   its own exemptions. Record both numbers; a reader who assumes they match
   will misread the diff.
2. **Edge delta.** Admitting mid-walk derivations converts pass-through edges
   into direct ones, so the count can move either way (research R7). **Issue
   #1367's edge undershoot must not later be diagnosed against a number this
   feature moved** — record the before and after.
3. **The residual sbomnix name gap.** It is 32 today and all 23 names the
   comparison document lists are in this population. Re-run
   `compare-sbomnix.sh` and record the new figure; near-zero is the
   expectation, and substantially above zero is a finding.

---

## Goldens

The inverse of milestone 1367. There, an unconditional record touched every
golden by four lines; here **one corpus target changes by a lot and nothing
else changes at all**:

| | effect |
| --- | --- |
| local `*_regression` and the other three golden writers | **unchanged** — none scans a closure |
| corpus `nix-closure-moat` | large — 100 components, 641 roles, edges, C210, C211 |
| the other 16 corpus targets | **unchanged** |

If a local golden changes, C210 or C211 is being emitted unconditionally —
check the gate before regenerating.

Refresh only through the `public-corpus.yml` dispatch; a `regen_goldens=true`
run proves nothing by passing, because it regenerates the goldens it then
compares against. Account for every differing file from the artifact, and keep
`image-postgres16` out of this feature's diff (#1372).

---

## Running the gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` (#1189). Assert on the positive signal and `GATE_RESULT=pass`
in `target/gate/prepr.log`.

**Target split**: `scan_fs` is in the **bin** target, `parity` in the **lib**.
`cargo test --bins every_catalog_row_has_an_extractor` reports `0 passed; N
filtered out`, indistinguishable from a pass.

**Never pipe a check through `head`** — it truncates the evidence a
completeness claim rests on, and it wedges cargo mid-run.
