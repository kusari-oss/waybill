# Corpora (T015, SC-005, SC-006, FR-008)

**Runs at `064e264f`:**

| run | ref | result |
|---|---|---|
| public corpus 44184431989 | branch | 158 passed, 4 failed |
| public corpus 44184436769 | `main` (control) | pending at time of writing |
| quality corpus 44184442942 | branch | success |
| quality corpus 44184449481 | `main` (control) | success |

Each failing target is traced below from the emitted documents (artifact `corpus-emitted-sboms`) against the committed goldens. Every component's PURL, C45, C42 and C151 were compared, along with every `dependencies[]` edge and the completeness fields, sorted with `LC_ALL=C`.

## The four failing targets

| target | failure | what changed | rule |
|---|---|---|---|
| `go-opentelemetry` | golden drift, 3 formats | **56** `unresolved-indirect-require` codes removed. The golden carried 320 codes against a stated 264 unreachable, so 56 sat on components the document reaches. The branch carries 264, equal to the count. No edge changed. | R1, FR-002 (I-A: no reachable component carries C45) |
| `nix-closure-shellcheck` | golden drift, 3 formats | **495** `closure-member-not-an-edge-endpoint` codes removed. The document is `complete` at both refs, and the root `dependsOn` 496 components at both, including these. The golden coded components the document reaches. No edge changed. | R1, FR-002 |
| `python-flask` | golden drift, 3 formats | *First run (`064e264f`):* unreachable **10 → 5**. Forked versions are now wired: `click` 8.1.8 and 8.2.1, `sphinx` 7.4.7 and 8.1.3, `alabaster` 0.7.16, from each dependent the lock pins them to. Three edges a bare name had resolved to another version were removed (`gha-update` and `uvicorn` → `click@8.1.3`, `sphinx@7.4.7` → `alabaster@1.0.0`). The five that became reachable carried `dead-lockfile-entry`. `colorama` loses `optional` scope, because a runtime `click` fork now reaches it. | R4 (forks); FR-006 as amended (reachable through an edge this feature adds) |
| `haskell-security-advisories` | I7 cell declared failing now **holds** (stale declaration) | The three flake inputs (`nixpkgs`, `flake-utils`, `nix-systems/default`) carry `unreferenced-lock-node`. | R1, FR-002, FR-008 |

## `haskell-security-advisories`: #1237

The declaration (#1237) recorded that feature 1218 added `unreferenced-lock-node` "for exactly this population", and that it could not reach them because the classifier and the document disagree about reachability. Feature 1218's unit test for the code models this repository's own `flake-utils` input. With one reachability answer, the classifier agrees with the document and the code is assigned.

**Traced cause of the disagreement.** It is not the edge the declaration names:
- The repository has six `.cabal` main modules and one root `flake.nix`. `attach_inputs_to_projects` attaches inputs only to a single owning project, so these inputs get no edge.
- The document roots at the operator subject, whose re-anchored main-module edges close the root fallback, so the inputs are unreachable.
- The old classifier rooted at a placeholder with no edges, which opened the fallback and attached every graph-top. This is the same mechanism as on TheRock.

**Open question, for the maintainer.** `unreferenced-lock-node` reads "a node in a lock file … which no other node references". The lock's root node references `nixpkgs` and `flake-utils`, and `flake-utils` references `nix-systems`. Feature 1218 chose this code for this population, and this feature does not re-decide it.

**Action:** the stale declaration is removed (T015).

## Quality corpus

Both runs succeeded, with no violations at either ref. Every metric except wall time and document size was compared per target (`pkgs`, `edges`, `files`, `flat`, `max_depth`, `nodes_with_out_edges`, `graph_completeness`, `sbomqs`, `status`). Two targets moved:

| target | `main` | branch | traced |
|---|---|---|---|
| `python-flask` | 202 edges | 217 edges | R4's fork wiring (the public-corpus trace above). sbomqs is unchanged at 7.2171. |
| `rust-zizmor` | 473 pkgs, 1184 edges, 300 nodes with out-edges, sbomqs 7.6975 | 474, 1188, 301, 7.6956 | R4. zizmor's `uv.lock` (pinned `fb814d66`, read through the GitHub API) has `name = "zizmor"`, `source = { editable = "." }` and no `version`, with dev groups `bench` (`pytest`, `pytest-codspeed`, `urllib3`) and `docs` (`zensical`). It is now emitted as versionless `pkg:pypi/zizmor` with C151 and those four dev-group edges: +1 package, +4 edges, +1 node with out-edges. The 0.0019 sbomqs drop is one more component with no version. |

## Goldens regenerated (`2aecdf6e`, run 44193732149)

- **What changed:** 12 golden files, exactly the four traced targets in three formats each. The run's own "every target regenerated" assertion passed.
- **`python-flask` (final):** unreachable **10 → 0**, and completeness `partial` → **`complete`**. Beyond the fork wiring traced above, a name the lock lists once now resolves inside the lock. The project's own dependencies (`blinker`, `itsdangerous`, `jinja2@3.1.6`, `markupsafe@3.0.2`, `werkzeug`) are wired to the lock's versions instead of the `requirements/*.txt` pins (`jinja2@3.1.2`, `markupsafe@2.1.2`) a bare name used to find, so the lock's own entries are no longer `dead-lockfile-entry`.
- **Harness expectations followed:**
  - `python_flask_expectations` now pins `complete`, so an orphan returning is caught.
  - Two feature-1218 controls lost their populations: python-flask's ten forks, and haskell-security-advisories' three flake inputs. One now asserts that the forks are **reached**. The other moved to `maven-guice`'s three #1235 components, still a declared non-Go/npm population.

## Quality corpus, final (`2aecdf6e`, run 44193736601, against `main` control 44184449481)

No violations. Two targets moved, both from FR-005:

| target | `main` | branch |
|---|---|---|
| `python-flask` | 202 edges, `partial`, sbomqs 7.2171 | 222 edges, **`complete`**, sbomqs **7.5098** |
| `rust-zizmor` | 473 pkgs, 1184 edges, 300 nodes with out-edges, `partial`, 7.6975 | 473 pkgs, 1188 edges, 301, **`complete`**, 7.6975 |

zizmor's package count no longer rises (the first run had 474). The lock's `zizmor` entry now takes the pyproject reader's `0.0.0-unknown` and merges with that component. Its four dev-group edges remain.
