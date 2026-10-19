# I7's populations, measured

**Taken**: 2026-10-19, against `main` at `1311224e`
**Probe**: [`i7-populations.py`](./i7-populations.py) — offline, reads only
the committed goldens
**Subject**: `waybill-cli/tests/fixtures/public_corpus/<target>/cdx.json`,
all seventeen targets

## Why this exists

#1214's population table was drawn per target. Two of its five rows
describe the wrong mechanism, and a fix written from them would have
changed the bazel reader for three components the bazel reader never
emitted. The counts were never wrong — 1, 2, 3, 5, 23 = 34 at the filing
commit `dce37304` and identical here, ten days and several merges later.
**A right count can carry a wrong cause**, which is the whole reason this
file exists.

## Method

The probe reimplements
`waybill-cli/tests/corpus_harness_195/invariants/checks/orphan_explained.rs`
rather than approximating it: same subject resolution
(`metadata.component.bom-ref`), same adjacency (`dependencies[].dependsOn`),
same BFS, and the same **five** explaining markers.

Five, not the four #1214 names — C197
(`waybill:version-absence = by-construction`, milestone 1156) was added
after the issue was filed. Its doc comment claims it moves no cell today
because all seven Pants resolve anchors are reachable. Measurement
confirms it: no offender in any target carries it.

Reading the predicate out of the harness rather than restating it is the
point. A probe that guesses the marker set measures a different invariant
than the lane enforces.

## The verdict per target

```
target                       verdict      count  populations
go-cobra                     Pass             0  -
go-opentelemetry             Fail             1  pypi=1
haskell-aeson                Pass             0  -
haskell-language-server      Fail             2  generic=2
haskell-security-advisories  Fail             3  github=3
image-postgres16             Pass             0  -
maven-guice                  Fail             5  bazel=2, maven=3
nix-closure-moat             Pass             0  -
npm-express                  Pass             0  -
pants-clojure-polyglot       Pass             0  -
pants-example-django         Pass             0  -
pants-example-golang         Pass             0  -
pants-example-javascript     Pass             0  -
pants-example-jvm            Pass             0  -
pants-example-python         Pass             0  -
python-flask                 Fail            23  pypi=23
rust-ripgrep                 Pass             0  -

total unexplained: 34
```

## Every offender, by its own provenance annotations

| target | `waybill:source-files` | `waybill:workspace-member` | n |
|---|---|---|---|
| `python-flask` | `examples/celery/requirements.txt` | `examples/celery` | 13 |
| `python-flask` | `uv.lock` | `.` | 10 |
| `maven-guice` | `WORKSPACE` | `.` | 2 |
| `maven-guice` | `examples/guice-demo/pom.xml` | `examples/guice-demo` | 2 |
| `maven-guice` | `extensions/struts2/pom.xml` | `extensions/struts2` | 1 |
| `go-opentelemetry` | `requirements.txt` | `.` | 1 |
| `haskell-security-advisories` | `flake.lock` | `.` | 3 |
| `haskell-language-server` | *(empty)* | *(none)* | 2 |

This grouping is what disproved #1214's two rows. It does **not** yet give
the mechanism — provenance correlates with cause but is not it, which the
next two sections show.

## A1 — `requirements.txt` components carry no out-edges (13)

Provenance said "declared by a sibling member". The edge structure says
something more specific and more serious.

`examples/celery` contributes **21** components, of which 13 dangle and 8
are reachable. The member's own main module is reachable and wired:

```
pkg:pypi/flask-example-celery@1.0.0  -> out-edges: 2
     pkg:pypi/celery@5.2.7
     pkg:pypi/flask@2.3.2

pkg:pypi/celery@5.2.7                -> out-edges: 0
```

and then it stops. Of the 21 components sourced from
`examples/celery/requirements.txt`, **zero have any out-edge**. The eight
reachable ones are reachable only by coincidence, through the subject's own
`uv.lock` closure.

So A1 is not a provenance fact about example directories. It is that the
`requirements.txt` path emits components without edges, and consequently
the transitive closure of anything it declares dangles. `celery`'s own
dependencies — `amqp`, `billiard`, `kombu`, `vine`, `async-timeout`,
`prompt-toolkit`, `redis`, `pytz`, `six`, `wcwidth`, `click-didyoumean`,
`click-plugins`, `click-repl` — are exactly the thirteen.

**The edges exist in the lockfile. The emitter did not write them.**
Explaining these with a marker would record a waybill defect as a property
of the flask repository.

### A reachable component can share the shape

```
click@8.1.3   reachable=True    examples/celery/requirements.txt  examples/celery
click@8.1.8   reachable=False   uv.lock                           .
click@8.2.1   reachable=False   uv.lock                           .
```

`click@8.1.3` is declared by the same sibling member as the thirteen and is
reachable. Any rule keyed on provenance alone would mark it, which is why
the spec separates "describes provenance" from "changes reachability"
(FR-009).

## A2 — the maven dual-key insert emits both coordinates and wires one (3)

`examples/guice-demo/pom.xml` declares `guice` and `guice-testlib`. Two
components exist for each coordinate — the literal version from the pom and
the workspace-resolved one — and the member's edges point at the resolved:

```
pkg:maven/com.example/guice-demo@1.0-SNAPSHOT        -> out-edges: 3
     pkg:maven/com.google.inject.extensions/guice-testlib@7.0.0
     pkg:maven/com.google.inject/guice@7.0.0
     pkg:maven/junit/junit@4.13.2

pkg:maven/com.google.inject.extensions/guice-struts2@7.0.1-SNAPSHOT -> out-edges: 3
     pkg:maven/com.google.inject.extensions/guice-servlet@7.0.0
     pkg:maven/jakarta.servlet/jakarta.servlet-api@5.0.0
     pkg:maven/org.apache.struts/struts2-core@2.5.30
```

The three that dangle are the declared coordinates:

```
pkg:maven/com.google.inject/guice@5.1.0                 <- examples/guice-demo/pom.xml
pkg:maven/com.google.inject.extensions/guice-testlib@5.1.0 <- examples/guice-demo/pom.xml
pkg:maven/com.google.inject.extensions/guice-servlet@7.0.1-SNAPSHOT <- extensions/struts2/pom.xml
```

Each carries `waybill:source-type = workspace`, so the reader knew it was
an intra-workspace reference. All five members' other declared deps
(`junit@4.13.2`, `struts2-core@2.5.30`) are reachable, so this is not a
member-wiring gap — it is one coordinate producing two components with only
one connected.

`com.google.inject/guice@5.1.0` is the project's own artifact. A consumer
looking up guice in this document finds two versions and no statement that
the older is an unresolved pom literal. This is the shape of #1216's
family, and "explaining" it would close the invariant that reports it.

## The remaining four are genuine vocabulary gaps

| mechanism | n | evidence |
|---|---|---|
| **B** — alternate resolution in the subject's own lockfile | 10 | `source-files = uv.lock`, `workspace-member = ["."]`; one copy of each duplicated name reachable, the other not |
| **C** — foreign build system's root manifest | 3 | `WORKSPACE` (bazel, 2) and root `requirements.txt` in a Go repo (pypi, 1); both `workspace-member = ["."]`, both a build system the subject does not use |
| **D** — lockfile node nothing else references | 3 | `flake.lock`; the m925 reader wires edges only between lockfile nodes |
| **E** — toolchain pin | 2 | `pkg:generic/ghc-9.6.5@unspecified`, `pkg:generic/stackage-lts-23.28@unspecified`; `source-files = [""]` and no `workspace-member` |

None of these four has an edge that a source of truth states, so a marker
is the truthful answer rather than the cheap one.

### Duplicate identities are wider than #1214's one example

The issue illustrates the ambiguity with `alabaster`. Nine names in
`python-flask` appear more than once:

```
alabaster 2   blinker 2   click 3   flask 2   itsdangerous 2
jinja2 2      markupsafe 2   sphinx 3   werkzeug 2
```

```
alabaster@0.7.16  reachable=False   uv.lock
alabaster@1.0.0   reachable=True    uv.lock
sphinx@7.4.7      reachable=False   uv.lock
sphinx@8.1.3      reachable=False   uv.lock
sphinx@8.2.3      reachable=True    uv.lock
```

`alabaster` is an instance, not the instance.

## Which readers the corpus can hold to account

#1214's second root cause names three readers missing
`waybill:unresolved-reason`. Measured across all seventeen goldens, the
only `waybill:source-mechanism` value present anywhere is:

```
bazel-http-archive    2
```

and no component is sourced from a cmake manifest. So of the three readers,
**bazel and nix are observable by the corpus and cmake is not**. Extending
the marker to `cmake.rs` would be unverifiable by any lane — recorded in
the spec as FR-019 rather than done.

```
$ grep -c 'unresolved-reason' \
    waybill-cli/src/scan_fs/package_db/{bazel,cmake}.rs \
    waybill-cli/src/scan_fs/package_db/nix/{mod,lockfile}.rs
waybill-cli/src/scan_fs/package_db/bazel.rs:0
waybill-cli/src/scan_fs/package_db/cmake.rs:0
waybill-cli/src/scan_fs/package_db/nix/lockfile.rs:0
waybill-cli/src/scan_fs/package_db/nix/mod.rs:0
```

## Re-running this

```sh
python3 -I specs/1218-explain-every-orphan/measurements/i7-populations.py .
```

No network, no docker, no corpus lane. Re-run it after any golden refresh,
and as the feature's closing step (FR-022) — the numbers above are
reproducible, not quoted, and a refresh that moves them is a finding rather
than a nuisance.
