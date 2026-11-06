# Quickstart: the pname-less closure members

## See the gap

```sh
cd specs/1282-closure-nameless-components/measurements
python3 -I classify-nameless.py <closure.json> --identity
```

Produce the input with `nix derivation show --recursive <flakeref>`. On the
pinned target it reports 1,145 members in four classes, and the identity
comparison that forced the design.

## Baselines to compare against

Read from the committed golden, not from memory — several figures carried
through earlier sessions were wrong:

```sh
python3 -I -c "
import json
d=json.load(open('waybill-cli/tests/fixtures/public_corpus/nix-closure-moat/cdx.json'))
p=[x for x in d['metadata']['properties'] if x['name']=='waybill:nix-closure'][0]
print(p['value'])"
```

Today: **381 components**, **3,459 edges**, census totalling **1,749**.

## Expected after implementation

* Component count rises to roughly **1,479** (381 + ~1,098).
* Every added component carries exactly one of four roles, and the four counts
  sum to the number added.
* Filtering to `source` and `package` yields **585** components and no
  scaffolding or fragments.
* The 56 members named `source` yield 56 distinct components whose identifiers
  differ **without reference to any qualifier**.
* The milestone-1277 census is byte-identical — it counts derivations.
* The edge count **changes**, and the document carries the new figure.

## Two things that will look like bugs and are not

**The edge count drops.** Stopping at sources and packages replaces a wide
transitive fan-out with a direct edge. A derivation-level proxy measured
12,013 → 7,755. If the component-level figure *rises*, that is the surprise
worth investigating, not a fall.

**513 components are orphans.** That is by construction: FR-013 keeps
scaffolding and fragments out of the graph. Each carries a reason saying so.
An orphan *without* a reason is the defect.

## What a passing test must not be confused with

A classifier that assigns every member the same role still produces four
counts that sum correctly, and still emits 1,145 components. Assert the
**distribution** — 549/83/267/246 on the pinned closure — not just the total,
and assert that disabling the emission makes a test fail.
