# Quickstart — verifying #919

Every command below runs against fixtures in this repository. Each maps to a
success criterion, and each states what the answer is **today** so a reader
can tell a real pass from a vacuous one.

## 0. See the defect first

```sh
waybill sbom scan --path waybill-cli/tests/fixtures/pants_namespace_collision \
  --split=resolve --output-dir /tmp/before --offline
```

Today: one `default.*` document holding a Maven jar and a PyPI wheel.

```sh
jq -r '.components[].purl' /tmp/before/default.generic.cdx.json
# pkg:maven/dev.waybill.fixture/waybill-fixture-jvmside@1.0.0
# pkg:pypi/waybill-fixture-pyside@1.0.0
```

Two unrelated resolves in one document that claims to be a resolve.

## 1. SC-001 / SC-002 — two documents, disjoint components

After: two documents, each with only its own namespace's package.

```sh
ls /tmp/after | grep default
# jvm-default.generic.cdx.json
# python-default.generic.cdx.json
```

The qualification appears **only** because these names collide.

## 2. SC-003 — each states one resolve

```sh
jq -r '.metadata.properties[] | select(.name=="waybill:document-resolve") | .value' \
  /tmp/after/python-default.generic.cdx.json
# ["python:default"]
```

Today the single merged document correctly states **both**, per milestone
912's C-6. Two documents each stating one is how that plurality stops arising.

## 3. SC-004 — the collision alone still splits (US3)

Against a fixture with **only** the colliding pair and no third resolve:

```sh
waybill sbom scan --path <collision-only fixture> --split=resolve \
  --output-dir /tmp/us3 --offline 2>&1 | grep -c "no partitionable"
# 0   — and two documents in /tmp/us3
```

Today: `detected=1`, the fallback fires, and **no split is produced at all**.
This is the simplest real-world reproduction and the one most likely to exist.

## 4. SC-005 — nothing moves where there was no collision

```sh
for fx in pants_resolve_edges pants_discovered_resolves pants_pex \
          pants_coursier_jvm/multi_resolve; do
  # split before and after; diff the trees
done
```

Byte-identical, including filenames. The fix must be invisible where the
defect was absent.

## 5. SC-006 / C-1 — partitioning an unsplit document works

```sh
jq -r '.components[] | select(.properties) |
  [(.properties[]|select(.name=="waybill:pants-resolve-namespace")|.value),
   (.properties[]|select(.name=="waybill:pants-resolve")|.value)] | @tsv' \
  /tmp/unsplit.cdx.json
# jvm      ["default"]
# python   ["default"]
```

Two partitions from one document. Today both rows read `["default"]` with
nothing beside them.

## 6. C-2 — the namespace is not the ecosystem

On `pants-example-python`, whose `python-default` resolve contains
`pkg:generic/*` members (measured, research R2):

```sh
# every member, generic ones included, must read `python`
```

If a `pkg:generic/*` component reads anything else — or nothing — the
namespace is being inferred rather than recorded.

## 7. C-3 — plurality is refused, not guessed

Unit-level: a component carrying two namespaces yields "cannot answer" plus a
warning. It must never silently pick one; that would file a component into the
wrong resolve's document, which is this milestone's own defect one layer down.

## 8. C-7 — goldens move only by the annotation

Three corpus targets carry membership and will move:
`pants-example-django`, `pants-example-jvm`, `pants-example-python`.
`-golang` and `-javascript` carry none and must not move.

Read the diff — the corpus lane emits a readable masked one and ships the
masked `.actual` as of #921 — and confirm the only change is the added
annotation.

## What "done" looks like

- Two documents where there was one, with disjoint components.
- A collision-only repository splits instead of silently not splitting.
- Every non-colliding repository byte-identical.
- One new annotation on every component that has membership, and on nothing else.
- Two corpus targets provably untouched.
