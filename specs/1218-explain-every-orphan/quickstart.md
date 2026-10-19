# Quickstart: verifying this feature

**Feature**: `1218-explain-every-orphan` | **Date**: 2026-10-19

Everything here runs offline. No corpus lane, no docker, no network — the
committed goldens are what the lane compares against, so a conclusion drawn
from them is the conclusion the lane would reach.

## 1. See the defect before you start

```sh
python3 -I specs/1218-explain-every-orphan/measurements/i7-populations.py .
```

Expect `total unexplained: 34`, with `Fail` on five targets:
`go-opentelemetry` 1, `haskell-language-server` 2,
`haskell-security-advisories` 3, `maven-guice` 5, `python-flask` 23.

If that number is not 34, the goldens have moved since 2026-10-19 and the
spec's counts need re-deriving before anything else — which is what FR-019
exists for. A changed number is a finding, not a nuisance.

## 2. The one-line answer to "did it work"

```sh
python3 -I specs/1218-explain-every-orphan/measurements/i7-populations.py . | tail -1
```

Target: `total unexplained: 16`. Not 0 — sixteen components are deliberately
left failing, thirteen for #1234 and three for #1235.

**A result of 0 is a failure, not a success.** It means an arm matched a
population it must not, and the two cells that report real reader defects
have been closed. SC-004 is the named check; this is its cheap form.

## 3. Confirm the sixteen are the *right* sixteen

```sh
python3 -I specs/1218-explain-every-orphan/measurements/i7-populations.py . \
  | grep -A20 'python-flask\|maven-guice'
```

Every remaining `python-flask` offender must have
`src=["examples/celery/requirements.txt"]`, and every remaining
`maven-guice` offender must have a `pom.xml` source. If a `uv.lock` or a
`WORKSPACE` component is still listed, this feature missed one of its own
mechanisms. If an `examples/celery` or `pom.xml` component is *gone*, it
took one of #1234's or #1235's.

## 4. Per mechanism

Each is independently checkable, in the user-story order.

### US1 — mechanism B, `dead-lockfile-entry` (10, python-flask)

```sh
python3 -I - <<'PY'
import json
cdx = json.load(open("waybill-cli/tests/fixtures/public_corpus/python-flask/cdx.json"))
for c in cdx["components"]:
    if "alabaster" in (c.get("purl") or ""):
        p = {x["name"]: x["value"] for x in (c.get("properties") or [])}
        print(c["bom-ref"], "->", p.get("waybill:orphan-reason", "<none>"))
PY
```

Expect `alabaster@0.7.16 -> dead-lockfile-entry` and
`alabaster@1.0.0 -> <none>` — the reachable copy carries nothing.

Then the arm-4 trap, which is the single most important check in this
feature:

```sh
python3 -I - <<'PY'
import json
cdx = json.load(open("waybill-cli/tests/fixtures/public_corpus/maven-guice/cdx.json"))
for c in cdx["components"]:
    if (c.get("purl") or "").endswith(("guice@5.1.0", "guice-testlib@5.1.0",
                                       "guice-servlet@7.0.1-SNAPSHOT")):
        p = {x["name"]: x["value"] for x in (c.get("properties") or [])}
        print(c["bom-ref"], "->", p.get("waybill:orphan-reason", "<none>"))
PY
```

All three MUST print `<none>`. They have reachable same-name siblings and
will satisfy a naive "has a sibling" predicate; if any prints
`dead-lockfile-entry`, #1235's cell has been silently closed.

### US2 — mechanisms C and D (3 + 3)

```sh
for t in maven-guice go-opentelemetry haskell-security-advisories; do
  echo "== $t"
  python3 -I - "$t" <<'PY'
import json, sys
cdx = json.load(open(f"waybill-cli/tests/fixtures/public_corpus/{sys.argv[1]}/cdx.json"))
for c in cdx["components"]:
    p = {x["name"]: x["value"] for x in (c.get("properties") or [])}
    r = p.get("waybill:orphan-reason")
    if r in ("foreign-build-system", "unreferenced-lock-node"):
        print(" ", c["bom-ref"], r, "| mech:", p.get("waybill:source-mechanism", "<none>"))
PY
done
```

Expect two `pkg:bazel/*` and one `pkg:pypi/codespell` as
`foreign-build-system`, three `pkg:github/*` as `unreferenced-lock-node`,
and — per FR-002a — a non-`<none>` `source-mechanism` on all three
`foreign-build-system` components, `codespell` included. That last column is
SC-017; it reads 2 of 3 before this feature.

### US3 — mechanism E (2, haskell-language-server)

```sh
python3 -I - <<'PY'
import json
cdx = json.load(open("waybill-cli/tests/fixtures/public_corpus/haskell-language-server/cdx.json"))
for c in cdx["components"]:
    p = {x["name"]: x["value"] for x in (c.get("properties") or [])}
    if p.get("waybill:orphan-reason") == "build-toolchain":
        print(c["bom-ref"], "| tier:", p.get("waybill:sbom-tier"))
PY
```

Expect exactly `ghc-9.6.5@unspecified` and `stackage-lts-23.28@unspecified`,
both `design`.

Then the arm-7 trap, which fails invisibly and so needs its own check:

```sh
python3 -I - <<'PY'
import json
cdx = json.load(open("waybill-cli/tests/fixtures/public_corpus/haskell-language-server/cdx.json"))
for c in cdx["components"]:
    if (c.get("purl") or "") in ("pkg:pypi/docutils", "pkg:pypi/myst-parser",
                                 "pkg:pypi/sphinx", "pkg:pypi/sphinx-rtd-theme"):
        p = {x["name"]: x["value"] for x in (c.get("properties") or [])}
        print(c["bom-ref"],
              "| orphan-reason:", p.get("waybill:orphan-reason", "<none>"),
              "| unresolved:", "yes" if p.get("waybill:unresolved-reason") else "no")
PY
```

All four MUST print `orphan-reason: <none>` and `unresolved: yes`. They are
`design`-tier and unreachable, like mechanism E's two, but they are
documentation dependencies of the `docs` sibling member and are already
explained by C151 — so this target holds **six** unreachable `design`-tier
components, not two, and only two of them are toolchain pins.

This is the check that cannot be replaced by a count. Those four are not I7
offenders, so if the arm fires on them the probe still reports 16, I7 still
passes on 15 of 17, and every one of SC-001 through SC-004 is still green
while the document claims Sphinx is build tooling. SC-018 exists for exactly
this, and research R6 records how the earlier reasoning missed it.

## 5. The invariants that are not about counts

```sh
# SC-009 — this feature changes annotations only.
# Component counts, edge counts and the reachable set must be identical.
git stash && python3 -I specs/1218-explain-every-orphan/measurements/i7-populations.py . > /tmp/before.txt
git stash pop && python3 -I specs/1218-explain-every-orphan/measurements/i7-populations.py . > /tmp/after.txt
# then diff the structural numbers, not the offender lists
```

```sh
# SC-014 — is_explained must still have exactly five arms.
# Scoped to the function: a file-wide grep returns 9, because the test
# module calls cdx_prop too. Verify what is claimed, not a proxy for it.
awk '/^fn is_explained/,/^}/' \
  waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs \
  | grep -c 'cdx_prop(component'
```

Expect **5**. A sixth means a new annotation was introduced and FR-001 was
missed — the whole point of extending C45 rather than adding a name.

```sh
# SC-007 — zero new catalog rows, zero new extractors.
git diff --stat docs/reference/sbom-format-mapping.md     # row text only
git diff --stat waybill-cli/src/parity/extractors/        # expect: empty
```

```sh
# SC-006 — exactly two surviving I7 declarations.
grep -c 'invariant = "I7"' waybill-cli/tests/corpus_harness_195/invariant-declarations.toml
```

Expect **2**, with counts 13 and 3, citing #1234 and #1235.

## 6. The gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` argument — the pin in `rust-toolchain.toml` is what makes
the gate match CI (#1189), and since v4.0.1 the constitution's own table
says so. Read `GATE_RESULT=pass` and the per-target
`test result: ok. N passed; 0 failed` lines; the absence of the word FAILED
is not evidence, because a build that never compiled also contains no
failures.

## 7. The trap to expect while implementing

Predicates read `c.evidence.source_file_paths`, which is **raw**. Every path
in this feature's docs is the *emitted* spelling, relativized at emit time
by `sbom_path::emitted_source_files`. A predicate that matches
`"examples/celery/requirements.txt"` literally will match nothing.

Match on `Path::file_name`, or route through `emitted_source_files` first.
The unit test that catches this asserts an absolute and a relative input
classify identically — see research R2, and the m226 Pants Go reader note
for the last time this bit someone here.
