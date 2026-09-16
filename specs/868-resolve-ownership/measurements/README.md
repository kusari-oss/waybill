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
