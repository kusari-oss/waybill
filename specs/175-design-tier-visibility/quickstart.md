# Quickstart: m175 Manual Verification

**Feature**: 175-design-tier-visibility
**Date**: 2026-07-09

Three verification paths — one per user story — plus a bonus for the KEEP-NATIVE-FIRST tag discoverability.

## Path A — US1: reading-guide operator walk

**Setup**: an operator new to mikebom is handed an SBOM emitted from a `requirements.txt`-only Python project.

```bash
# Synthesize the fixture:
mkdir -p /tmp/m175-us1-fixture
cat > /tmp/m175-us1-fixture/requirements.txt <<'EOF'
requests>=2.31.0
click>=8.1.7
pyyaml>=6.0
EOF

mikebom --offline sbom scan --path /tmp/m175-us1-fixture \
    --format cyclonedx-json \
    --output /tmp/m175-us1.cdx.json \
    --no-deep-hash 2> /tmp/m175-us1.stderr
```

**Path A.1 — Recognize a design-tier component**:

```bash
jq '.components[] | select(.version == "") | {purl, tier: (.properties[]? | select(.name == "mikebom:sbom-tier") | .value)}' /tmp/m175-us1.cdx.json | head -20
```

**Expected**: JSON objects with `.tier == "design"` and empty `.version` field.

**Path A.2 — Count design-tier components (SC-002 recipe)**:

```bash
jq '[.components[]?.version | select(. == "")] | length' /tmp/m175-us1.cdx.json
```

**Expected**: positive integer (3 in this fixture — matches the 3 requirements.txt entries).

**Path A.3 — Name a remediation action**:

The operator opens `docs/reference/reading-a-mikebom-sbom.md#design-tier-components` (the new subsection introduced by m175 T005) and finds the per-ecosystem remediation table. For pip, at least one of these actions is named: `uv lock`, `poetry lock`, `pip-compile`, or `python -m venv .venv && .venv/bin/pip install -r requirements.txt`.

**SC-001 gate**: an operator new to mikebom can complete A.1 + A.2 + A.3 within 5 minutes of reading only the new subsection.

## Path B — US2: advisory log firing behavior

**Setup**: same `requirements.txt` fixture from Path A, plus a fully-lockfile-resolved fixture for the negative case.

**Path B.1 — Fires exactly once on design-tier scan (SC-002)**:

```bash
grep -cF 'design-tier components detected: ' /tmp/m175-us1.stderr
```

**Expected**: `1`.

**Path B.2 — Body contents (SC-002 detailed check)**:

```bash
grep -F 'design-tier components detected: ' /tmp/m175-us1.stderr
```

**Expected**: one line containing the exact count (`3`), a remediation keyword (`lockfile` OR `venv`), and the docs cross-reference (`docs/reference/reading-a-mikebom-sbom.md`).

**Path B.3 — Silent on zero design-tier (SC-003)**:

```bash
# Synthesize a fully-lockfile-resolved fixture (npm with package-lock.json):
mkdir -p /tmp/m175-us2-clean
cat > /tmp/m175-us2-clean/package.json <<'EOF'
{"name": "clean-npm", "version": "0.1.0", "dependencies": {"lodash": "^4.17.21"}}
EOF
cat > /tmp/m175-us2-clean/package-lock.json <<'EOF'
{"name": "clean-npm", "version": "0.1.0", "lockfileVersion": 3, "requires": true,
 "packages": {"": {"name": "clean-npm", "version": "0.1.0", "dependencies": {"lodash": "^4.17.21"}},
              "node_modules/lodash": {"version": "4.17.21", "resolved": "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz"}}}
EOF

mikebom --offline sbom scan --path /tmp/m175-us2-clean \
    --format cyclonedx-json \
    --output /tmp/m175-us2-clean.cdx.json \
    --no-deep-hash 2> /tmp/m175-us2-clean.stderr

grep -cF 'design-tier components detected: ' /tmp/m175-us2-clean.stderr
```

**Expected**: `0` (no design-tier components — advisory correctly suppressed).

**Path B.4 — Suppression env-var honored (SC-004)**:

```bash
MIKEBOM_NO_DESIGN_TIER_ADVISORY=1 mikebom --offline sbom scan --path /tmp/m175-us1-fixture \
    --format cyclonedx-json \
    --output /tmp/m175-us1-suppressed.cdx.json \
    --no-deep-hash 2> /tmp/m175-us1-suppressed.stderr

grep -cF 'design-tier components detected: ' /tmp/m175-us1-suppressed.stderr
```

**Expected**: `0` (env-var suppressed the advisory despite design-tier count > 0).

**Path B.5 — Fires under `--offline` (SC-005)**:

The Path A + Path B.1 scans already use `--offline`. The advisory count is `1`, verifying FR-002's offline-orthogonality.

## Path C — US3: KEEP-NATIVE-FIRST tag discoverability

**Setup**: any checkout of the mikebom repo post-m175.

```bash
grep -n KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md
```

**Expected**: exactly one match — the m175 row. `wc -l` on the grep output returns `1`.

**Path C.1 — Row contents check**:

```bash
grep -B0 -A0 KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md | head -1
```

**Expected**: a row that names `mikebom:design-tier-count` as the rejected alternative, cites the native carriers (empty `version`, `metadata.lifecycles[design]`), and includes the phrase "Constitution Principle V" or equivalent audit citation.

**SC-007 gate**: the grep returns exactly one match. The tag polarity is now prior-art for future Principle V audits.

## Path D — SC-006 byte-identity gate

**Setup**: any pre-175 golden fixture that produces design-tier components (e.g., the `pip` golden fixture in `mikebom-cli/tests/fixtures/golden/cyclonedx/pip.cdx.json`).

```bash
# Emit a fresh SBOM against the pip fixture:
mikebom --offline sbom scan --path mikebom-cli/tests/fixtures/pip \
    --format cyclonedx-json \
    --output /tmp/m175-pip-fresh.cdx.json \
    --no-deep-hash

# Compare against the golden (assumes goldens are UN-touched by m175):
diff mikebom-cli/tests/fixtures/golden/cyclonedx/pip.cdx.json /tmp/m175-pip-fresh.cdx.json
```

**Expected**: **zero diff** (byte-identical). Only the stderr differs (advisory line addition on the fresh scan; the golden fixture never captured stderr).

## Bonus Path — Full m175 test suite

```bash
cargo +stable test -p mikebom --test design_tier_advisory
```

**Expected**: `5 passed; 0 failed` — one integration test per US2 acceptance scenario + SC-005 offline verification.

```bash
./scripts/pre-pr.sh
```

**Expected**: `>>> all pre-PR checks passed.` No golden regeneration needed; all existing goldens stay byte-identical.

## Full success criteria table

| SC | Verification | Expected |
|---|---|---|
| SC-001 | Path A.1 + A.2 + A.3 walk-through | operator completes in <5 minutes |
| SC-002 | Path B.1 `grep -cF` on design-tier stderr | `1` |
| SC-003 | Path B.3 `grep -cF` on clean-scan stderr | `0` |
| SC-004 | Path B.4 `grep -cF` with env-var set | `0` |
| SC-005 | Path A scan uses `--offline`; Path B.1 grep result | `1` |
| SC-006 | Path D `diff` on any pre-existing golden | empty |
| SC-007 | Path C `grep -n KEEP-NATIVE-FIRST` | `1` line |
| SC-008 | Manual audit — operator follows Path A.3 remediation, re-scans, counts design-tier | drops to 0 |
