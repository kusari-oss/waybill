# Quickstart: verifying the CPE coverage pass

**Feature**: `1367-closure-component-cpes` | **Issue**: #1371

Read this before writing a test. The traps here are different from milestone
1366's, and one of them is the reason this defect survived since milestone 097.

---

## The trap that created this bug

**Zero CPEs and "no mapped software" are the same document.** The synthesis
pass has never reached a closure component, and nothing failed, warned or
logged. A scan looked exactly as it would if the closure simply held nothing
the table covers.

That is `feedback_zero_results_can_mean_broken` in its purest form, and it is
why FR-008 exists: after this feature, those two states produce different
documents.

**So the first test to write is not "openssl has a CPE".** It is a control
proving the component set under test actually contains mappable software —
otherwise every assertion below passes vacuously on an empty population.

```rust
// CONTROL, before any CPE assertion.
assert!(components.iter().any(|c| c.name == "openssl" && !c.version.is_empty()),
        "the fixture must contain mappable software, or nothing below is evidence");
```

---

## Control 1 — the six, end to end

The measured targets on the pinned closure:

| name | version |
| --- | --- |
| `openssl` | 3.6.2 |
| `curl` | 8.20.0 |
| `zlib` | 1.3.2 |
| `sqlite` | 3.51.2 |
| `pcre2` | 10.46 |
| `llvm` | 21.1.8 |

```sh
waybill sbom scan --path <moat clone at d069055> \
  --nix-closure --nix-closure-attr default \
  --format cyclonedx-json --output cyclonedx-json=/tmp/moat.cdx.json

jq -r '.components[] | select(.cpe) | "\(.name)\t\(.cpe)"' /tmp/moat.cdx.json
jq '[.components[] | select(.cpe)] | length' /tmp/moat.cdx.json
```

`--no-deps-dev` is **not** needed here and should not be passed: unlike
milestone 1366, this pass makes no network call, so the hermetic flag buys
nothing and would suppress the milestone-1366 identity pass that research R7
says changes which arm 74 components take.

---

## Control 2 — the negative populations

Three assertions that must hold, each over a population large enough to matter:

```sh
# FR-002 — no closure member carries a CPE (539 on the pinned closure)
jq '[.components[]
     | select(any(.properties[]?; .name=="waybill:closure-member-role"))
     | select(.cpe)] | length' /tmp/moat.cdx.json          # → 0

# FR-003 — no CPE carries a non-release version
jq -r '.components[] | select(.cpe) | .version' /tmp/moat.cdx.json \
  | grep -vE '^[0-9]+(\.[0-9]+)*[a-z]?[0-9]*$' || echo "all release-shaped"

# FR-004 — operator-supplied CPEs survive
# (scan the same project twice, once with --supplement-cdx supplying a CPE
#  for a component the table would also map, and diff that component)
```

The FR-004 check needs a **constructed** case: it is the one requirement whose
violation the pinned closure cannot demonstrate, because no component there
both receives a supplement and matches the table. A synthetic fixture is the
only way to make that test fail when the guard is removed.

---

## Control 3 — the mutation check (SC-007)

A test that cannot fail proves nothing. Before claiming coverage:

```text
1. Make the new pass a no-op.            → the six-component test must go red.
2. Make the pass fill unconditionally
   instead of only-if-empty.             → the FR-004 supplement test must go red.
3. Remove the release-shape guard.       → the FR-003 test must go red.
```

Record which tests failed and how many. If mutation 2 leaves everything green,
the supplement case is not actually covered — and that is the mutation most
likely to pass unnoticed, because it changes nothing on the pinned closure.

---

## What the number will be, and why not to predict it

SC-001 says **at least 6**. Research R7 predicts the real total is higher,
because the 74 components milestone 1366 re-identified to `pkg:pypi` currently
hold empty `cpes` and will take the `"pypi"` arm once this pass runs after the
identity pass.

**That is a prediction from reading the code, not a measurement.** Record the
measured total in `measurements/README.md` with the command that produced it,
and quote that figure and no other.

> **Figures are per-closure and per-system.** The counts above are
> aarch64-darwin at a pinned nixpkgs revision; the corpus goldens pin
> x86_64-linux, which is a *different closure*. Milestone 1366 measured 74
> candidates on one and 75 on the other for exactly this reason.

---

## Golden impact

Unlike milestone 1366, this pass runs on **every scan**, not only closure
scans. So the golden expectation is not "only the closure target changes" — it
is "only targets containing mappable software change", and which targets those
are is a **measurement**.

Diff the regenerated goldens file by file and account for every differing
target before committing any of them. `image-postgres16` currently drifts for
an unrelated reason (issue #1372) and its regenerated goldens must **not** be
swept into this feature's diff.

Refresh only through the `public-corpus.yml` workflow dispatch.
`WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is never set locally, and a
`regen_goldens=true` run proves nothing by passing — it regenerates the
goldens it then compares against.

---

## Running the gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` (#1189). Assert on the positive signal and `GATE_RESULT=pass`
in `target/gate/prepr.log` — never on a pipeline's exit status, and never on
the absence of the word FAILED.

**Watch the target split.** `scan_fs` and `cli` live in the **bin** target;
`parity` lives in the **lib**. `cargo test --bins every_catalog_row_has_an_extractor`
reports `0 passed; N filtered out`, which is indistinguishable from a pass;
`--lib` runs it. The inverse bit milestone 1282.

**And never pipe cargo through `head`.** It closes the pipe mid-run and wedges
the process at whatever count the filter stopped at — which looks exactly like
a hung test suite, and cost two detours in the milestone-1366 session.
