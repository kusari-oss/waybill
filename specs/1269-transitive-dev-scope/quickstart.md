# Quickstart: verifying transitive scope propagation

**Feature**: `1269-transitive-dev-scope` | **Date**: 2026-10-25

Five checks, cheapest first. Each one can fail for a different reason,
which is the point — a green run of only the first proves nothing.

---

## 1. Reproduce the defect before fixing it (2 min, no build)

The committed goldens already contain it. This script is the detector
reduced to its essentials, so you can see the population move:

```sh
python3 -I - <<'PY'
import json
from collections import deque
for t in ['python-flask','rust-ripgrep','haskell-language-server','npm-express']:
    d=json.load(open(f'waybill-cli/tests/fixtures/public_corpus/{t}/cdx.json'))
    ref=lambda c: c.get('bom-ref') or c.get('purl')
    marked={ref(c) for c in d.get('components',[]) if c.get('scope')=='excluded'}
    adj={x['ref']:x.get('dependsOn') or [] for x in d.get('dependencies',[]) or []}
    r=ref(d['metadata']['component'])
    def bfs(skip):
        seen={r}; q=deque([r])
        while q:
            for n in adj.get(q.popleft(),[]):
                if skip and n in marked: continue
                if n not in seen: seen.add(n); q.append(n)
        return seen
    rt, allr = bfs(True), bfs(False)
    print(t, "violations:", len([x for x in allr if x not in rt and x not in marked]))
PY
```

Expected **before**: `python-flask 51`, `rust-ripgrep 2`,
`haskell-language-server 26`, `npm-express 0`.
Expected **after**: all four `0`.

`npm-express` reading `0` on both sides is the control. If it reads `0`
because the file has no `excluded` component at all, the control is
vacuous — it has 16, and 29 runtime-reachable components. Check that
before trusting it.

---

## 2. The positive control, which no corpus target can give you (SC-003)

A corpus target cannot prove a negative. Build the mixed-reachability
case as a unit fixture:

```
root ──depends──▶ shared
root ──depends──▶ devtool   (lifecycle_scope = Development, declared)
devtool ──depends──▶ shared
```

`shared` MUST keep `lifecycle_scope: None`: it is runtime-reachable, so
FR-002 immunizes it, whatever else reaches it.

**Then mutate your own fix**: delete the immunity check from Phase B and
re-run. If `shared` still comes out unscoped, the test is not exercising
the guard and SC-003 is not met. A pass that over-propagates satisfies
every count in check 1 while demoting components that ship, and that
failure is invisible in orphan counts.

---

## 3. Byte-identity where nothing should change (FR-013, SC-004)

```sh
cargo test -p waybill --test corpus_goldens -- --ignored npm-express
```

Any `npm-express` golden that moves is a bug in the pass, not churn to
regenerate. Same for `go-cobra`, `go-opentelemetry`, `maven-guice` and
the pants targets unless the per-ecosystem record (FR-015) says
otherwise and says why.

---

## 4. Parity for the new annotation (FR-009)

```sh
cargo test -p waybill --bin waybill parity::extractors::tests::every_catalog_row_has_an_extractor
cargo test -p waybill --bin waybill holistic_parity
```

Both fail if the C62 enum entry lands in
`docs/reference/sbom-format-mapping.md` without its extractor. Add the
row and the extractor in one commit.

---

## 5. The full gate, then the corpus lane

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` argument — the pin in `rust-toolchain.toml` is what
makes the gate match CI (#1189).

The corpus goldens are `#[ignore]`d locally and the PR lane skips them,
so a green gate says nothing about them. Dispatch the lane on the
branch:

```sh
gh workflow run public-corpus.yml -f branch=1269-transitive-dev-scope
```

`-f branch=`, never `--ref` (#1223). **Never generate corpus goldens
locally** — `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is the flag not to
set on your machine.

---

## Deleting the I3 declarations (FR-014)

When the five entries come out of
`waybill-cli/tests/corpus_harness_195/invariant-declarations.toml`, they
come out in the same commit as the fix. The lane fails on a stale
declaration by design, so a fixed defect cannot keep its exemption —
and a declaration left behind is the lane telling you the fix did not
reach that target.

Targets: `python-flask`, `haskell-language-server`,
`haskell-security-advisories`, `nix-closure-moat`, `rust-ripgrep`, all
`kind = "unmarked-non-runtime-reachable"`, all `issue = 1209`.

---

## What a complete verification looks like

| claim | evidence | not evidence |
|---|---|---|
| dev transitives are marked | check 1 reads 0 on four targets; five declarations deleted and the lane green | the pass compiled |
| runtime deps are not demoted | check 2, with its mutation | orphan counts unchanged |
| already-correct output unchanged | check 3 byte-identical | "npm wasn't in the failing set" |
| the distinction is on the wire | check 4 in all three formats | the CDX property exists |
| the fix reached ecosystem X | a corpus target or a named fixture for X | X not appearing in the I3 declarations |
