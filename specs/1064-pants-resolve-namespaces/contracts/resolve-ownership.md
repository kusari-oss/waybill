# Contract: `waybill:resolve-ownership` (C161), version 2

Document-scope. Carried as a CycloneDX `metadata.properties[]` string (JSON text)
and as a structured value in the SPDX 2.3 and SPDX 3 annotation envelopes, with
the same value in all three formats (parity row C161, `SymmetricEqual`).

## Shape

```json
{
  "declared":   ["<namespace>:<name>", …],
  "discovered": ["<namespace>:<name>", …],
  "weak_classification": <non-negative integer>,
  "unanchored_lockfiles": <non-negative integer>
}
```

- `<namespace>` ∈ {`python`, `jvm`}.
- Both lists are lexically sorted over the full qualified string and deduplicated.
- `unanchored_lockfiles == len(discovered)`.
- Keys and their order are unchanged from version 1 (#911).

## Presence

| repository | statement |
|---|---|
| no Pants lockfile in either namespace | **absent** (byte-identical to before) |
| Pants lockfiles in one or both namespaces | present, covering every namespace that has lockfiles |

## Change from version 1

| | v1 (m868 / #911) | v2 (m1064) |
|---|---|---|
| producers | Pex reader only | Pex and coursier readers |
| names | bare (`python-default`) | qualified (`python:python-default`) |
| JVM-only repository | absent | present |
| unconfigured default resolve | stem name (`default`), discovered | `python:python-default` / `jvm:jvm-default`, declared |

A v1 reader comparing names breaks loudly on the `:`. A reader of the counts is
unaffected except where JVM resolves now add to them.

## Examples (expected after this feature)

`pants-example-jvm`:
```json
{"declared":["jvm:jvm-default"],"discovered":[],"unanchored_lockfiles":0,"weak_classification":1}
```

`pants-clojure-polyglot`:
```json
{"declared":["jvm:java17","jvm:java21","python:pants-2.30","python:pants-2.31"],"discovered":[],"unanchored_lockfiles":0,"weak_classification":4}
```

Fixture `pants_namespace_collision`:
```json
{"declared":["jvm:default","python:default","python:lint"],"discovered":[],"unanchored_lockfiles":0,"weak_classification":3}
```
These examples are predictions from the rules above, not measurements, and are
confirmed by the tests and goldens this feature adds.
