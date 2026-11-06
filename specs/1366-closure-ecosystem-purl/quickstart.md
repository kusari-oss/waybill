# Quickstart: verifying the ecosystem identity pass

**Feature**: `1366-closure-ecosystem-purl` | **Issue**: #1365

Read this before writing a test for the feature. The main hazard is that the
obvious verification paths **cannot witness it** (research R7), so a green
suite is weak evidence unless it includes the controls below.

---

## The trap, stated first

The one corpus target that runs a closure — `nix-closure-moat` — is invoked
with `--no-deps-dev --no-clearly-defined` (`corpus_harness_195/harness.rs:218`).
Under this design that suppresses confirmation. So:

> **A corpus run can never confirm a single candidate.** Its goldens will show
> `unchecked: 75, reason: "disabled"` and nothing else, forever. A green corpus
> lane is **not** evidence that US1 works.

This is `feedback_a_passing_test_may_exercise_nothing` with the cause known in
advance. Every claim about confirmation needs one of the two controls below.

---

## Control 1 — the stubbed deps.dev test (gives US1 teeth)

`wiremock = "0.6"` is already a dev-dependency (`waybill-cli/Cargo.toml:236`)
and `DepsDevClient::with_base_url` (`deps_dev_client.rs:137`) already exists to
point a client at it.

```text
1. Stand up a wiremock server that answers
     GET /v3alpha/systems/pypi/packages/urllib3/versions/2.6.3  → 200 + VersionInfo
     GET /v3alpha/systems/pypi/packages/librt/versions/1.0      → 404
2. Build DepsDevSource::new(client.with_base_url(&server.uri()), false)
3. Feed a synthetic closure fixture through the pass.
4. Assert: urllib3 → pkg:pypi/urllib3@2.6.3 + C207 present
           librt   → pkg:generic/librt@1.0   + C207 absent
```

The 404 arm is not optional padding: it is the only place FR-003's
"decline, not failure" distinction is exercised, and it is the behaviour US2
is built on.

**Fixture naming**: synthetic only — `waybill-fixture-*`, never a real
coordinate (`feedback_fixture_synthetic_package_names`; it tripped the advisory
scan in PR #285 and #640). Use real names **only** in the stub's URL arms,
where they are inputs to a mock rather than published fixture content — or
prefer a synthetic name there too and keep the real-name assertion in the
measurement (Control 2).

### Mutation check — required

A test that cannot fail proves nothing. Before claiming the suite covers this:

```sh
# Make confirmation always decline, then run the suite.
# At least the US1 assertions MUST go red.
```

Record which tests failed and how many. SC-007 is this check, promoted to a
success criterion.

---

## Control 2 — the measurement run (gives SC-001 its number)

SC-001 requires a measured count and explicitly forbids a predicted one. The
only place to get it is a real scan with deps.dev enabled:

```sh
# aarch64-darwin, pinned nixpkgs a799d3e3 — the closure every other
# figure in this spec was measured on.
waybill sbom scan --path <moat-clone> \
  --nix-closure --nix-closure-attr default \
  --format cyclonedx-json --output cyclonedx-json=/tmp/moat.cdx.json
```

Then read the answer out of the document, not out of a model of it:

```sh
jq '[.components[] | select(.purl | startswith("pkg:pypi/"))] | length' /tmp/moat.cdx.json
jq -r '.metadata.properties[] | select(.name=="waybill:closure-ecosystem-identity-outcome") | .value' /tmp/moat.cdx.json
```

Commit both the commands and their output under
`specs/1366-closure-ecosystem-purl/measurements/`, as milestones 1276 and 1282
did. **Quote the figure from this run and no other.**

> **Figures are per-closure and per-system.** These counts are aarch64-darwin;
> the corpus goldens pin x86_64-linux, which is a *different closure*. Quoting
> one against the other is wrong in membership — an error the closure
> milestones have made repeatedly.

---

## Control 3 — the regression guards

These are cheap and catch the two ways this feature can do harm.

**FR-005 — the 539 members must not move.** They are the majority of the
population and `pkg:generic` is *correct* for them.

```sh
jq -S '[.components[]
        | select(any(.properties[]?; .name=="waybill:closure-member-role"))
        | {name, version, purl}] | sort' before.cdx.json > /tmp/m-before.json
# ...same for after; diff must be empty.
```

**R11 — the edges must still resolve.** `Relationship.from`/`.to` are PURL
strings, so a mis-ordered pass orphans every corrected component.

```sh
# Every dependency endpoint must name a component that exists.
jq '[.dependencies[].dependsOn[]?] - [.components[].purl] | length' after.cdx.json
# → 0
```

Also compare the orphan count before and after: it must not rise. A rise means
the pass ran outside the window research R11 specifies.

---

## Reading what the closure actually contains

Do not model the closure — ask the reader. `waybill debug closure-dump` exists
for exactly this and calls the production functions
(`classify`, `member::classify`, `emit::emits_as_component`):

```sh
nix derivation show --recursive <flakeref>#default > /tmp/closure.json
waybill debug closure-dump --closure-json /tmp/closure.json --summary-only
```

And for the candidate population specifically:

```sh
python3 -I specs/1276-boot-library-scope/measurements/pypi-identity-probe.py /tmp/closure.json
```

Its docstring carries the two gotchas that produced wrong numbers first:
`pname` lives in **two** carriers (`env` and top-level `structuredAttrs` — 66
of 1,275 moat derivations use only the latter), and output paths are bare
basenames while `src` carries a full `/nix/store/` path.

---

## Running the gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` argument (#1189). Assert on the positive signal and on
`GATE_RESULT=pass` in the log — never on a pipeline's exit status, and never on
the absence of the word FAILED.

**Watch the bin/lib split.** `scan_fs` lives in the **bin** target and `parity`
in the **lib**. `cargo test --lib <filter>` over a bin-target test reports
`0 tests; 218 filtered out`, which is indistinguishable from a pass. This bit
milestone 1282 directly.

---

## Golden refresh

The moat goldens change once, by the addition of C208. Refresh them **only**
through the `public-corpus.yml` workflow dispatch.
`WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is never set locally — all 109 corpus
tests are `#[ignore]`d locally, so a local run neither validates nor regenerates
them (`feedback_green_local_gate_excludes_corpus_goldens`).
