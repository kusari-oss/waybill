# Quickstart: verifying the closure license lookup

## Reproduce the gap this milestone closes

```sh
# waybill's own document for the pinned target, and sbomnix's for comparison.
# 96 components are licensed by sbomnix and silent in waybill (R7).
python3 - <<'PY'
import json
def lic(c): return bool(c.get('licenses') or [])
w=json.load(open('waybill.cdx.json'))['components']
s=json.load(open('sbomnix.cdx.json'))['components']
wk={};sk={}
for c in w: wk.setdefault((c.get('name'),c.get('version')),[]).append(c)
for c in s: sk.setdefault((c.get('name'),c.get('version')),[]).append(c)
gap=[k for k in set(wk)&set(sk)
     if not any(lic(x) for x in wk[k]) and any(lic(x) for x in sk[k])]
print('gap:', len(gap))
PY
```

Group into lists before asking whether any component carries a license. A dict
keyed by `(name, version)` keeps only the last component per key and drops
licensed duplicates on the sbomnix side, which emits many repeated names across
distinct store paths — that mistake reports 58 instead of 96 (R7).

## Run the probes

```sh
cd specs/1281-closure-license-meta/measurements
nix eval --json --option allow-import-from-derivation false \
  --file name-scoped-probe.nix > rows.json      # ~1 s
nix eval --json --option allow-import-from-derivation false \
  --file enumerate-probe.nix  > rows-all.json   # ~166 s
```

`name-scoped-probe.nix` reads `names.json` from the working directory — the
distinct `pname` values of the closure under test.

## Expected after implementation

* Closure components carry licenses in the **native** `licenses[]` slot of all
  three formats, not in a `waybill:` property.
* A document-scope annotation states how many components wanted a license and
  how many received one. An incomplete join is legible rather than silent —
  coverage cannot reach 100%, because a `drvPath` encodes the whole transitive
  input closure and the same package built in a different context is a
  different store path (R5).
* With the host's `nix` absent or the budget exhausted, the document is still
  emitted with its components, no licenses from this source, and a recorded
  reason.

## What a passing run must not be confused with

An empty license set is a legitimate outcome for a closure whose members
genuinely declare none, and it looks identical to a lookup that never ran. The
coverage annotation is what distinguishes them, so assert on it — a positive
signal — rather than on the absence of an error.
