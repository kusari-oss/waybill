# Milestone 868 — measurements

Every figure here is an observation. Where a number is a prediction or a
derivation it says so. Nothing in this file is read back from the
graph-completeness classifier, which consumes the same edges this feature
adds (contract A-8).

## Binaries

| Name | Build | Path |
|---|---|---|
| baseline | merge-base of `868-resolve-ownership`, release | `target/release/waybill-baseline` |
| after | this branch, release | `target/release/waybill` |

Both report `waybill 0.7.0`; they are distinguished only by build, so keep
them side by side rather than re-resolving `waybill` on `$PATH`.

## T002 / T015 / T016 — the measured target

`lablup/backend.ai` @ `809fcd394dd8e39456986dd742e7d51c6aedd647`, re-measured
2026-09-16 with both binaries against the same checkout, same flags
(`--offline --root-name pants-backend-ai --root-version 809fcd3
--no-deep-hash`).

Every row below is walked from the emitted CycloneDX `dependencies[]`, not
read from the graph-completeness annotation (contract A-8). The one row that
quotes the annotation says so.

| | baseline | after |
|---|---:|---:|
| CDX components | 331 | 340 |
| of those, tiered (classifier's universe) | 272 | 281 |
| CDX `dependsOn` edges | 760 | 919 |
| reachable from root **(walked)** | 1 | 247 |
| max depth **(walked)** | 1 | 5 |
| orphans **(walked, tiered universe)** | 271 | **34** |
| orphans *as the classifier reports them* | 271 | *37* |
| `pkg:generic/*` resolve components | none | 9 |

Components rise by exactly 9, the number of resolves `pants.toml` declares;
no package component is created (contract A-6, SC-006).

The 760 edges that existed before were not wrong — they were unreachable.
This feature connects them; it does not manufacture them.

### The walked and reported orphan counts disagree by 3

Contract A-8 exists for exactly this, so it is recorded rather than smoothed
over, and the walked number is the one this milestone claims.

The three are `pkg:pypi/sphinx`, `pkg:pypi/sphinx-autodoc-typehints` and
`pkg:pypi/sphinx-rtd-theme` — versionless design-tier components from
`docs/requirements.txt`. Around 35 resolved packages declare them as extras
dependencies, so the emitted CDX graph reaches all three (e.g.
`pkg:pypi/cryptography@46.0.3 -> pkg:pypi/sphinx`). The classifier's BFS does
not, and counts them orphans.

**m868 did not introduce this.** On the baseline the two views agree exactly
(271 = 271) — but only because a single component was reachable at all, so
the disagreement had nowhere to show. Making the graph reachable is what
exposed it. The divergence is in the conservative direction (the classifier
over-reports incompleteness), which is why it has gone unnoticed.

Out of scope here; filed as #892. Anyone reading the annotation on this
target should expect it to be pessimistic by 3.

## T010 — teeth-check

The point of this section is that the tests fail against the pre-change
binary. Where a test does *not* fail there, it is a regression guard rather
than a defect-catcher, and this file says which is which rather than
implying every assertion bites.

### The fixture must give the root a real edge

The first version of the T009 fixture had **no teeth**, and the teeth-check
is what caught it.

CycloneDX's primary-dependency fallback
(`target_has_no_edges`, `waybill-cli/src/generate/cyclonedx/dependencies.rs:77`)
attaches every un-depended-on component to a root that has no outgoing edges
of its own. On a fixture whose only content is two Pants lockfiles, that
fallback fires and manufactures both the reachability and the depth:

```
baseline, fixture WITHOUT a root-level pyproject.toml:
  components=3 reachable_from_root=3 max_depth=2
```

`t009b_the_document_is_no_longer_flat` asserts `depth >= 2` and therefore
**passed against the defect**. Adding a root `pyproject.toml` with one
dependency gives the root a genuine edge, suppresses the fallback, and
reproduces the real fingerprint the #870 canary reports (`depth=1`,
`flat=true`):

```
baseline, fixture WITH a root-level pyproject.toml:
  components=4 reachable_from_root=1 max_depth=1   (classifier: 3 orphans)
```

This same fallback is the masking mechanism behind all three defects in the
#870 family (#883 python-ansible, #886 gradle-bitwarden-android, #887 here).
Any future fixture for this area needs a root that already has an edge, or
it measures the fallback instead of the feature.

### Observed, on the corrected fixture

| | baseline | after |
|---|---:|---:|
| components | 4 | 6 |
| reachable from root | 1 | 6 |
| max depth | 1 | 3 |
| `pkg:generic/*` resolve components | none | `app-runtime`, `lint-tools` |

### Per-test verdict

| Test | Fails on baseline? | Why |
|---|---|---|
| `t007_one_resolve_component_per_declared_resolve_named_by_declaration` | yes | no `pkg:generic/*` component exists at all |
| `t007b_resolve_component_depends_on_declared_requirements_only` | yes | same — nothing to assert `depends` on |
| `t008_no_resolve_component_without_a_declaration` | **no** | regression guard for contract A-7 ("holds → must not regress"). Asserts an absence that is also true before the change |
| `t009_each_resolves_requirements_are_reachable_from_the_document_root` | yes | the two resolve components are absent, and the pypi packages are orphaned (reachable 1 of 4) |
| `t009b_the_document_is_no_longer_flat` | yes, **after the fixture fix above** | `max_depth=1` on baseline |
| `t012_anchoring_adds_resolve_components_and_invents_no_packages` | partly | the "two resolve components exist" half fails on baseline; the "packages unchanged" and "undeclared emits none" halves are A-6/A-7 regression guards and pass before the change |
| `t017_a_package_in_two_resolves_is_reachable_via_each` | yes | neither resolve component exists to start a walk from |

## T020 — US2 teeth-check

Both US2 tests fail against `waybill-baseline` for the same reason and it is
a blunt one: the pre-change binary emits no `pkg:generic/*` resolve component
at all, in any format, so there is nothing for the marker assertions to find.

| Test | Fails on baseline? | Why |
|---|---|---|
| `t019_a_resolve_is_identifiable_as_a_resolve_in_every_format` | yes | CDX assertion expects two marked components, finds zero; the SPDX 2.3 and SPDX 3 envelope counts are likewise 0 against an expected 2 |
| `t019b_the_root_to_resolve_edge_is_distinguishable_by_its_target` | yes | expects 2 anchor edges among the root's out-edges, finds 0 |

Worth naming what this teeth-check does *not* prove. It confirms the tests
notice the feature's absence; it does not confirm they would notice the
marker being emitted on the wrong components. That second property is
carried by the in-test assertion that no `pkg:pypi/*` component may hold the
marker, which fails on a hypothetical over-broad implementation rather than
on the baseline.

### T021 was verification, not implementation

`waybill:component-kind` rides the standard `PackageDbEntry.extra_annotations`
channel, which every emitter already fans out. Confirmed on the fixture
before writing any emitter code — CDX `properties[]`, SPDX 2.3
`annotations[].comment`, and SPDX 3 `Annotation.statement` all carried
`lockfile-resolve` unmodified. Same finding as T032 anticipated for C143.
No emitter change was needed or made.

## T027 — US3 teeth-check

| Test | Fails on baseline? | Why |
|---|---|---|
| `t024_declaration_beats_allowlist_when_they_disagree` | yes | `classify_resolve_with_source` does not exist; the declaration is never read |
| `t024b_declaration_is_recorded_even_when_it_agrees_with_the_allowlist` | yes | same |
| `t025_undeclared_falls_back_and_says_so` | yes | same |
| `t026_the_counts_are_emitted_even_when_both_are_zero` | yes | annotation absent entirely — see below |
| `t026b_the_annotation_is_absent_when_no_lockfile_was_found` | **no** | regression guard: asserts an absence that also holds before the change |
| `t030_undeclared_resolves_are_counted_as_weakly_classified` | yes | annotation absent |
| `t034_a_glob_discovered_lockfile_nobody_declares_is_counted_unanchored` | yes | annotation absent |

T027 asks specifically whether the checks catch an **absent** field rather
than only a wrong value, since the FR-003c requirement is precisely that
zero and missing stay distinguishable. They do, by construction: the helper
returns `Option<String>` per format and the assertions compare against
`Some(..)`, so an absent annotation yields `None` and fails the comparison.
Confirmed against the baseline binary, which emits no `waybill:resolve-ownership`
in any format. `t026b` is the pair's other half and asserts `None` on a repo
with no Pex lockfile.

## T029 / T030 / T032 — US3 on the measured target

Reader log, after:

```
pants-pex reader complete lockfiles_discovered=9 lockfiles_parsed_ok=9
  components_emitted=301 weak_classification=4 unanchored_lockfiles=0
```

Four of nine resolves are classified by something weaker than a declaration
— `python-default`, `python-kernel`, `pants-plugins` and `towncrier`, none of
which any tool section back-references. The other five (`black`, `pytest`,
`coverage-py`, `mypy`, `setuptools`) carry `install_from_resolve`. C161 reads
`weak-classification=4;unanchored-lockfiles=0`, byte-identical across CDX,
SPDX 2.3 and SPDX 3.

### T029 — two live misclassifications corrected, none introduced

Per-resolve lifecycle scope of the emitted components, before and after
T028. `?` is the absence of `waybill:lifecycle-scope`, which is how runtime
is emitted.

| resolve | before | after |
|---|---|---|
| black | development: 4 | development: 4 |
| **coverage-py** | development: 1, runtime: 1 | **development: 2** |
| mypy | development: 2 | development: 2 |
| pants-plugins | runtime: 1 | runtime: 1 |
| pytest | development: 9 | development: 9 |
| python-default | runtime: 216 | runtime: 216 |
| python-kernel | runtime: 17 | runtime: 17 |
| **setuptools** | development: 1, runtime: 1 | **development: 2** |
| towncrier | runtime: 4 | runtime: 4 |

Exactly the two resolves contract A-4 names as misclassified move, and
**nothing moves the other way** (SC-005a). The single `development` entry
each already had before T028 is the resolve component itself, which read the
declaration from the start; the package is what was left behind.

The allowlist was deliberately not widened to cover `coverage-py` and
`setuptools`. Research R1 records why: it would paper over the reason a name
allowlist is the wrong instrument, and would still be wrong for the next
project that names its coverage resolve something else.

### T032 — per-package attribution verified, not built

C143 `waybill:pants-resolve` was already shipping. Measured identically on
both binaries: **248 of 272** pypi components carry it. FR-005 / SC-005 hold
and are unregressed; no work was needed, which is what research R2
predicted.

## T036 — the corpus bounds need no change

`pants-backend-ai`'s committed expectations in `xtask/corpus/quality-corpus.toml`
turn out to be satisfied by the fixed document without moving anything.
Measured with the harness's exact invocation (`--offline`, `--root-name`,
`--root-version`, and an isolated `HOME`/`GOPATH`/`GOMODCACHE`), metrics
computed the way `xtask/src/quality/analyze.rs` computes them:

| metric | committed bound | baseline | after |
|---|---|---:|---:|
| pkgs | 243..299 | 272 ✓ | 281 ✓ |
| files | 53..65 | 59 ✓ | 59 ✓ |
| edges | 826..1010 | **761 ✗** | 919 ✓ |
| max_depth | 4..9 | **1 ✗** | 5 ✓ |
| flat | false | **true ✗** | false ✓ |

Three metrics violated before this feature and none violates after. **No
bound moves.** T036 anticipated re-authoring them; the measurement says
there is nothing to re-author, so nothing was touched.

### Why the bounds were right by accident

tasks.md notes the bounds "were authored against fabricated
primary-dependency-fallback edges". That is true about their provenance, and
it is worth being exact about why they nonetheless land correctly.

Without `--root-name`, CycloneDX's primary-dependency fallback fires and
attaches every orphan to the root. Both binaries produce the same numbers
that way:

| invocation | binary | pkgs | edges | depth | flat |
|---|---|---:|---:|---:|---|
| **no** `--root-name` | baseline | 330 | 918 | 5 | false |
| **no** `--root-name` | after | 339 | 918 | 5 | false |
| with `--root-name` (harness) | baseline | 272 | 761 | 1 | **true** |
| with `--root-name` (harness) | after | 281 | 919 | 5 | false |

The fallback's 918/5 and the genuinely-anchored 919/5 agree to within one
edge. So the original author measured a document whose reachability was
manufactured, wrote down bounds describing it, and those bounds happen to
describe a correctly-anchored document too — because the fallback was
approximating, crudely, the very edges that were missing.

That is a coincidence, not a validation of the method. A fallback that
attaches every orphan to the root will always produce a plausible-looking
depth and edge count; here it produced a plausible-looking *and* nearly
correct one. The lesson is the one contract A-8 states: a number that looks
right is not evidence the graph is right.

## T038 — scan time is unchanged

Interleaved on identical machine state, alternating binaries within each
round. Separately-taken baselines attributed machine drift to the change in
milestone 867; interleaving is what prevents that.

| round | baseline | after |
|---|---:|---:|
| 1 | 746 ms | 749 ms |
| 2 | 754 ms | 753 ms |
| 3 | 753 ms | 756 ms |
| **median** | **753 ms** | **753 ms** |

Zero median difference, and the spread within each arm (8-10 ms) exceeds the
difference between them. Indistinguishable.

Note the T003 figure recorded in research R6 was 781/778/794 ms on a
different machine state. That is exactly why the comparison here is
interleaved rather than taken against the recorded number.

## T039 — walker-audit gate

Not run by `scripts/pre-pr.sh`, so verified separately rather than assumed.

No file under `waybill-cli/src/scan_fs/walk*` is touched by this branch, and
`walk.audit-allowlist.txt` is unmodified. Comparing the `fn walk[_(]` surface
between `main` and the branch gives **78 identical lines on both sides**, so
the gate is a no-op.

Method note: the comparison diffs `git grep` output taken from the two refs
rather than re-implementing the CI gate's grep-and-diff locally. A previous
attempt at local re-implementation reported false drift (35 vs 12 entries)
because `sed` was absent from the shell's PATH.

## Deviation from tasks.md T013

tasks.md specifies computing "top-level requirements — packages nothing else
*within that resolve* depends on". The implementation instead uses the PEX
lockfile's own top-level `requirements` array.

Measured on the target, 2026-09-16, over the 9 resolves `pants.toml` declares
(script: `measurements/count-anchors.py`):

| resolve | declared | roots, naive | roots, extras-markers excluded | locked |
|---|---:|---:|---:|---:|
| python-default | 124 | 64 | 91 | 226 |
| python-kernel | 8 | 6 | 7 | 16 |
| pants-plugins | 0 | 0 | 0 | 0 |
| black | 1 | 1 | 1 | 6 |
| pytest | 9 | 5 | 7 | 23 |
| coverage-py | 1 | 1 | 1 | 1 |
| mypy | 3 | 2 | 3 | 14 |
| towncrier | 1 | 1 | 1 | 4 |
| setuptools | 2 | **0** | 2 | 2 |
| **TOTAL** | **149** | **80** | **113** | |

Two things this shows, neither of which was knowable from the task text:

1. **Deriving loses anchors.** 149 declared against 113 derived even under the
   generous marker-aware reading. A package that was explicitly requested can
   also be a transitive dependency of another requested package, which makes
   it a non-root while still being something the resolve was asked for.

2. **Deriving has no single answer.** The derived figure moves from 80 to 113
   purely on how `extra == "..."` markers are treated. `tools/setuptools.lock`
   is the sharp case: `setuptools` and `wheel` each appear in the other's
   `requires_dists` behind extras markers, so a naive derivation yields **zero**
   roots and the whole resolve stays unreachable. The declaration names both
   unambiguously.

Deriving would require waybill to take a position on marker evaluation before
it could anchor anything. Reading the declaration requires no position at all,
which is what contract A-2 (declared, never guessed) asks for.

### Correction

An earlier draft of this rationale, and the doc comment on
`PexLockfile::requirements`, cited "124 declared versus 99 graph roots". 124
is the `python-default` resolve alone, not the total, and 99 does not
correspond to either derivation. Both were restated from memory rather than
measured. The table above is the measurement; the script is committed beside
it so it can be re-run when the pin moves.
