# Quickstart

How to see the defect, and how to know when it is fixed. ~10 minutes.

## Prerequisites

- a `waybill` release binary built from the commit under test — **rebuild
  it**; a stale binary is how an earlier spec in this repo came to describe a
  product state that no longer existed
- `git`, `jq`, `python3`

## 1. Reproduce

```bash
W=$(mktemp -d); cd "$W"
git init -q backend.ai && cd backend.ai
git remote add origin https://github.com/lablup/backend.ai
git fetch -q --depth 1 origin 809fcd394dd8e39456986dd742e7d51c6aedd647
git checkout -q FETCH_HEAD && cd ..
# ~200 MB clone

# --root-name matters: it is what the quality harness passes. Without it the
# main module becomes the root and the primary-dependency fallback fires,
# fabricating ~157 edges and hiding the defect completely.
waybill --offline sbom scan --path "$W/backend.ai" \
  --format cyclonedx-json --output cyclonedx-json="$W/out.json" \
  --root-name pants-backend-ai --root-version 809fcd3
```

Walk the graph from the root:

```bash
python3 - <<'PY'
import json, collections, sys
d=json.load(open(sys.argv[1]))
deps={e["ref"]:e.get("dependsOn",[]) for e in d["dependencies"]}
root=d["metadata"]["component"]["bom-ref"]
seen={root:0}; q=collections.deque([root])
while q:
    n=q.popleft()
    for c in deps.get(n,[]):
        if c not in seen: seen[c]=seen[n]+1; q.append(c)
print(f"reachable {len(seen)-1} of {len(d['components'])}, max depth {max(seen.values())}")
print(f"total edges {sum(len(v) for v in deps.values())}")
PY
```

Before the fix: **reachable 1 of 331, max depth 1, total edges 761.**

760 of those edges are a real pypi graph. The consumer walking from the root
sees none of it.

## 2. Confirm the main module is not the problem

```bash
head -6 "$W/backend.ai/pyproject.toml"
```

`dynamic = ["version"]` and **no `dependencies` key**. The root genuinely
declares nothing, so the main module having no edges is correct. This is not
the `python-ansible` defect (dynamic dependencies unhandled) and not the
`gradle-bitwarden-android` one (requirer/dependency ecosystem mismatch) —
check both before assuming, since all three look identical from the shape
alone.

## 3. See what ownership information already exists

```bash
jq -r '[.components[].properties[]? | select(.name=="waybill:pants-resolve") | .value]
       | group_by(.) | map("\(length)  \(.[0])") | .[]' "$W/out.json" | sort -rn
```

Eight named resolves, already attributed per package. The grouping this
feature needs is already in the document; what is missing is anything that
*owns* it.

## 4. What "fixed" looks like

| | before | after |
|---|---|---|
| reachable from root | 1 of 331 | **the resolves' contents** |
| max depth | 1 | **> 1** |
| reports itself flat | yes | **no** |
| orphaned-components-detected | 271 | **falls, and agrees with the graph** |
| `coverage-py` lifecycle | runtime | **build-time** |
| `setuptools` lifecycle | runtime | **build-time** |
| package components | 331 | **331 + one per declared resolve, no new packages** |
| a corpus target with no resolve | — | **byte-identical** |

The last two rows are the ones that catch a fix which went too far.

## 5. The control

```bash
# Any corpus target with no lockfile resolve must not move at all.
jq -r '[.dependencies[].dependsOn//[]|length]|add' <any committed corpus cdx.json>
```

Byte-identical before and after. Seventeen of the eighteen corpus targets
have no resolves, so this is most of the corpus and it is the cheapest check
that the feature is scoped where it claims to be.

## Traps

- **A stale binary.** Rebuild before measuring.
- **Omitting `--root-name`.** Without it the fallback fires and reproduces the
  old authored figure of 918 edges, which looks healthy and is fabricated.
- **Reading the completeness annotation as proof.** It consumes the same
  edges this feature adds, so it will agree with itself. Assert reachability
  against the emitted graph (contract A-8).
- **Assuming the corpus bound is the target.** `pants-backend-ai`'s current
  `edges 826..1010` was authored against fabricated fallback edges. Re-author
  it after this lands, from a CI measurement, not toward the old number.
- **Trusting the name allowlist.** It already misclassifies `coverage-py`
  here — the allowlist has `coverage` and `coveragepy`. If a classification
  looks right, check whether a declaration or a near-miss produced it.
