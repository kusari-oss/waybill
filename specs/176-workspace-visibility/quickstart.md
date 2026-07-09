# Quickstart: m176 Manual Verification

**Feature**: 176-workspace-visibility
**Date**: 2026-07-08

Three verification paths — one per user story — plus a bonus reproducing the langflow audit.

## Path A — US1: CVE triage via per-workspace filter

**Setup**: any monorepo with at least 2 workspaces. The langflow test fixture (10 workspace members) or a synthesized 2-workspace fixture work.

```bash
git clone --depth 1 https://github.com/kusari-sandbox/test-langflow /tmp/test-langflow

mikebom --offline sbom scan --path /tmp/test-langflow \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/langflow.cdx.json \
    --no-deep-hash
```

**Path A.1 — enumerate workspaces**:

```bash
jq '.metadata.properties[]?
    | select(.name == "mikebom:workspaces-detected")
    | .value | fromjson' /tmp/langflow.cdx.json
```

**Expected**: JSON array of ≥10 workspace paths (langflow has 9 pypi + 2 npm workspace members plus root).

**Path A.2 — filter components by workspace**:

```bash
jq '[.components[]
     | select((.properties[]? | select(.name == "mikebom:workspace-member") | .value | fromjson | contains(["src/frontend"])))
     | .purl] | length' /tmp/langflow.cdx.json
```

**Expected**: positive integer matching npm-frontend component count for the `src/frontend` workspace.

**Path A.3 — CVE scoping**:

```bash
# Suppose a CVE affects pyyaml. Which workspaces are impacted?
jq -r '.components[]
       | select(.purl | startswith("pkg:pypi/pyyaml"))
       | .properties[]?
       | select(.name == "mikebom:workspace-member")
       | .value | fromjson | .[]' /tmp/langflow.cdx.json
```

**Expected**: list of workspace paths where pyyaml is declared/locked.

## Path B — US2: advisory log fires exactly once on monorepo

**Setup**: same langflow scan, but capture stderr.

```bash
mikebom --offline sbom scan --path /tmp/test-langflow \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/langflow.cdx.json \
    --no-deep-hash 2> /tmp/langflow.stderr

grep -cF "monorepo shape detected: " /tmp/langflow.stderr
```

**Expected**: `1`.

**Path B.2 — verify substring stability**:

```bash
grep -F "monorepo shape detected: " /tmp/langflow.stderr | head -1
```

**Expected**: one line containing the workspace count + comma-separated workspace paths + the `docs/reference/monorepos.md` cross-reference.

**Path B.3 — advisory suppressed on single-project scan**:

```bash
# Scan a simple single-workspace repo (e.g., the mikebom repo itself
# has 3 workspace members but scan sees them as a Cargo workspace, not
# multiple boundaries — verify against actual behavior).
mkdir -p /tmp/single-py-project/src && echo "print('hi')" > /tmp/single-py-project/src/main.py
cat > /tmp/single-py-project/pyproject.toml <<'EOF'
[project]
name = "demo"
version = "0.1.0"
requires-python = ">=3.10"
EOF

mikebom --offline sbom scan --path /tmp/single-py-project \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/single.cdx.json \
    --no-deep-hash 2> /tmp/single.stderr

grep -cF "monorepo shape detected: " /tmp/single.stderr
```

**Expected**: `0` (single workspace → no advisory).

## Path C — US3: doc-scope aggregate matches per-component union

**Setup**: any scan output from Path A.

```bash
# Cross-check the C121 doc-scope invariant vs the union of C120 values.
jq '
  [.components[]?.properties[]?
   | select(.name == "mikebom:workspace-member")
   | .value | fromjson | .[]] | unique as $union
  | .metadata.properties[]?
  | select(.name == "mikebom:workspaces-detected")
  | .value | fromjson
  | {union: $union, detected: ., match: (. == $union)}
' /tmp/langflow.cdx.json
```

**Expected**: `"match": true`.

## Bonus Path — Reproduce langflow audit + verify m176 fix

**Baseline** (pre-176 — from the earlier audit): 3280 total components, 10-way ambiguous root selection, no per-workspace visibility. Consumer had to walk `mikebom:source-files` values by hand to derive workspace membership.

**Post-176**:
1. Every workspace-attributable component has `mikebom:workspace-member`.
2. Doc-scope `mikebom:workspaces-detected` enumerates all 10+ workspaces.
3. Advisory log fires once with the workspace count.
4. `metadata.component` root selection UNCHANGED — the m127 heuristic still picks (arguably-wrong) `langflow-base@0.10.2`, but downstream consumers can now use C120 to slice correctly regardless of that choice.

## Full success criteria table

| SC | Verification | Expected |
|---|---|---|
| SC-001 | Path A.2 jq filter | positive integer matching workspace-scoped component count |
| SC-002 | Path B `grep -c` on stderr | `1` |
| SC-003 | Path B.3 `grep -c` on single-project scan | `0` |
| SC-004 | Post-golden-regen: `jq -S 'del(...C120...) \| del(...C121...)'` on pre vs post | empty diff |
| SC-005 | Path B.2 substring match on langflow | line contains all 10 workspace paths |
| SC-006 | tf-models scan: `jq '[.components[]?.properties[]? \| select(.name == "mikebom:workspace-member") \| .value \| fromjson \| .[]] \| unique'` | array containing at least 3 workspace paths |
| SC-007 | Path C `.match` field | `true` |
| SC-008 | Post-golden-regen jq diff on non-monorepo goldens | zero byte delta outside C120/C121 |
