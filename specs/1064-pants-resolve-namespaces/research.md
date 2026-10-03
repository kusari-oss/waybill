# Research: Pants resolves owned and named across both namespaces

Sources: `main` @ 29f90262 (after #1103, #1104), committed public-corpus
goldens, crate fixtures, and Pants `release_2.31.0` source read through the
GitHub API on 2026-10-03.

## R1 — Owning-component identity (FR-006)

**Decision**: `pkg:generic/<resolve-name>?pants-namespace=<python|jvm>` for every
owning component, Python and JVM, whether or not a name collides (spec
clarification, 2026-10-03).

**Rationale**: Unique across namespaces without changing the name segment, so
`purl.name()` still returns the resolve name. That matters because split
filenames and m922's naming root are derived from it, so they are unaffected.

**Consequences found in code**:
- `generate/split.rs:340` finds a resolve's anchor by
  `purl.name() == resolve && ecosystem == "generic"`. In a collision repository
  that matches **both** anchors. It must also match the namespace, read from the
  component's `waybill:pants-resolve-namespace` annotation, never parsed back
  out of the PURL.
- CycloneDX `bom-ref` is the PURL string and SPDX IDs are hashed from it, so
  every Python anchor's identifiers change. This is expected (FR-015).
- The #925 layer-1 invariant `resolve-anchor-reaches-pants` matches
  `p == "pkg:generic/pants-2.31"` and must match the qualified form.

**Alternatives considered**: path form (`pkg:generic/pants-resolve/<ns>/<name>`)
and qualify-on-collision only. Both were rejected in the clarification. The
community question is tracked on #1106.

## R2 — Pants built-in default resolves (FR-007)

**Measured** (Pants 2.31 source):

| language | option | default |
|---|---|---|
| JVM | `[jvm].resolves` (`jvm/subsystems.py:66-72`) | `{"jvm-default": "3rdparty/jvm/default.lock"}`, `default_resolve = "jvm-default"` |
| Python | `[python].resolves` (`backend/python/subsystems/setup.py:187-237`) | `{"python-default": "3rdparty/python/default.lock"}`, `default_resolve = "python-default"` |

`pants-example-jvm` configures no `[jvm.resolves]` and refers to its resolve as
`jvm-default` (`[scala.version_for_resolve] jvm-default = "2.13.8"`). waybill
names it `default` (file stem).

**Decision**: the built-in default applies when **`pants.toml` exists** and the
language's `resolves` table is **absent**. The lockfile at the default path is
then named `jvm-default` / `python-default` and counted as declared.

**Rationale**: Pants applies its default only inside a Pants repository, and
`pants.toml` is what makes one. Requiring it keeps crate fixtures that carry
lockfiles without a `pants.toml` (e.g. `pants_pex/multi_resolve`) byte-identical,
and they are not Pants repositories by Pants's own definition. An explicitly
configured table, even one that omits the default path, keeps today's behaviour
(a stem-named lockfile is discovered and unanchored, m868 FR-003).

**Alternatives considered**: applying the default without `pants.toml` would
rename every fixture resolve, and Pants would not do it either. Inferring the
default from `default_resolve` alone was also rejected: `default_resolve` names
which resolve targets use, not where its lockfile is.

## R3 — JVM anchors and their edges (FR-004, FR-005)

**Measured**: Pants coursier lockfiles carry the declared top-level requirements
in the metadata header. `pants_jvm/lockfile.rs` already parses it into
`PantsMetadata::generated_with_requirements` (line 158) and then discards it
(`let _ = …`, line 226). Entry form:
`"com.google.guava:guava:31.0.1-jre,url=not_provided,jar=not_provided"`.

**Decision**: a JVM owning component per declared JVM resolve, built like the
Python one (`pants/lockfile.rs::resolve_component_entry`):
`component-kind = lockfile-resolve`, membership `[<name>]`, namespace `jvm`,
`depends_ecosystem = "maven"`, and `depends` = `group:artifact` from each
requirement (text before the first `,`, first two `:`-separated fields).
Edges then resolve within the resolve through the scoped `group:artifact` key
#1103 added.

**Alternatives considered**: deriving top-levels as "entries nothing else in the
lock depends on". Rejected for the same reason m868 rejected it for Python
(`resolve_component_entry` doc): it describes the graph's shape, not the
declaration.

## R4 — One repository-wide statement (FR-001–FR-003, FR-011)

**Today**: `pants::read_with_summary` returns `Option<PantsResolveSummary>`. The
JVM reader returns entries only. The summary flows through
`package_db/mod.rs:1811` → `diagnostics.pants_resolve_summary` → emitters.

**Decision**: the JVM reader also returns an optional summary of the same
shape. The two merge at the diagnostics site into one value whose name lists
are `<namespace>:<name>`, lexically sorted, with counts summed. It is present if
either reader found a lockfile and absent if neither did (FR-014 byte-identity).

**Wire form** (contract `contracts/resolve-ownership.md`): same four keys as
today, with namespace-qualified names, e.g.
`{"declared":["jvm:java17","jvm:java21","python:pants-2.30","python:pants-2.31"],"discovered":[],"unanchored_lockfiles":0,"weak_classification":…}`.
The `<namespace>:<name>` form is C163's existing per-document form.

**Alternatives considered**: a per-namespace nested object. Rejected because it
breaks every reader of the existing four keys, where qualified strings break
only name comparisons, and C163 already established the string form.

## R5 — JVM tool lockfiles (FR-009)

**Measured**: Pants 2.31 JVM tools subclass `JvmToolBase`
(`jvm/resolve/jvm_tool.py:32`), whose lockfile option is
`[<scope>].lockfile` (line 67), a path, not `install_from_resolve`. The 17
scopes are: `junit`, `scalatest`, `scalafmt`, `scalafix`, `ktlint`,
`google-java-format`, `scalapb`, `scrooge`, `openapi-generator`, `java-avro`,
`protobuf-java-grpc`, `jar_tool`, `jarjar`, `strip-jar`, `java-parser`,
`scala-parser`, `kotlin-parser`. The value `<default>` means a lockfile built
into Pants, so there is no file in the repository.

**Decision**: any `pants.toml` table other than `[jvm]` and `[python]` with a
string `lockfile` value that resolves to a discovered JVM lockfile path declares
that lockfile as a tool resolve:
- named after the table (the tool's scope, which is what
  `pants generate-lockfiles --resolve=<scope>` calls it);
- counted as declared and anchored;
- classified `Development` with source `Declared`, the same outcome Python gives
  a tool-declared resolve (`pants/resolve_classifier.rs:64-75`).

This matches by path, so no scope list is hard-coded (the 17 are recorded here
as the measurement, not as an allowlist). `<default>` and paths that don't exist
are ignored.

**Alternatives considered**: a hard-coded scope list would go stale as Pants
adds tools.

## R6 — Root edges and format agreement (FR-013)

**Measured**: the CycloneDX and SPDX 2.3 root fallback already exempts owning
components (`cyclonedx/dependencies.rs:85-110`,
`graph_completeness/mod.rs:215-235` key on `component-kind =
lockfile-resolve`). In `pants-clojure-polyglot` the root reaches the two Python
anchors in all three formats (that agreement is the "2" in 6 / 6 / 2), and the
four JVM top-levels only through the fallback.

**Decision**: no emitter change. JVM anchors join the existing anchor path, so
JVM top-levels become reachable through anchors in every format and the
fallback no longer applies to them. **Verify** that the clojure target reaches
4 / 4 / 4 and leaves `KNOWN_SPDX3_ROOT_EDGE_DIVERGENCE` (SC-002). If the
root→anchor edge proves to be Python-specific, that finding goes back into this
plan before implementation proceeds.

**Finding at T043 (first CI corpus run, 2026-10-03)**: the "no emitter
change" decision holds for every root edge an owning component creates (FR-013).
The clojure target measured 4 / 4 / 4 in the emitted documents, and the
collision fixture 3 / 3 / 3. But R6 measured the CycloneDX and SPDX 2.3
fallbacks only. The SPDX 3 fallback (`v3_document.rs`, the issue-#236 block
gated on `synth_has_outgoing`) counts root → owning-component edges when it
decides whether the root already has edges; the other two formats ignore them.
So once a repository has an owning component, SPDX 3 stops attaching components
that nothing else reaches.

`pants-example-jvm` shows it: root out-edges went from 4 / 4 / 4 (before: three
JVM top-levels plus the file-tier `get-pants.sh`, all through the fallback) to
2 / 2 / 1. `jvm-default` is reached in all three formats; `get-pants.sh` is
reached only in CycloneDX and SPDX 2.3. This is the #1022 mechanism, and
`pants-example-python` already sits at 2 / 2 / 1 for the same reason. Per
FR-013 and the out-of-scope note, the fallback is not changed here.
`pants-example-jvm` joins `KNOWN_SPDX3_ROOT_EDGE_DIVERGENCE`, and the
mechanism is recorded on #1022. For a file-tier component the direction is
not in doubt, because it has no other parent: SPDX 3 is dropping a real edge.

## R7 — Blast radius

- **Public corpus**: `pants-example-python`, `pants-example-django`,
  `pants-example-jvm` and `pants-clojure-polyglot` change, via anchor identity,
  C161 qualification, JVM anchors, and `jvm-default` naming for `example-jvm`.
  The other 13 targets have no Pants lockfiles and must stay byte-identical
  (SC-004). Regenerate twice in CI, `xtask corpus-diff`, and attribute every
  category (`docs/development/refreshing-corpus-goldens.md`).
- **In-repo goldens**: none of the six golden-writing suites (`cdx_regression`,
  `spdx_regression`, `spdx3_regression`, `oci_pull_backward_compat`,
  `optional_dep_classification`, `pkg_alias_binding_us1`) mentions `pants` or
  `3rdparty` (grep, 0 matches each). They are expected to stay unchanged, and
  the regeneration sweep confirms it.
- **Crate tests that will change**: `pants_namespace_split.rs` (anchor lookup,
  filenames unchanged per R1), `pants_resolve_membership.rs`,
  `pants_pex_reader.rs` and `pants_coursier_jvm_reader.rs` (C161 values),
  and the #925 layer-1 assertion.
- **Parity**: C161's catalogue row is unchanged; the value carries the same
  string in all formats (FR-012).
