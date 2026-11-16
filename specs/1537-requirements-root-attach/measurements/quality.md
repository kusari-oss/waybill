# Quality corpus and public corpus: two regressions, found and fixed (T018, T020)

Both corpora went red on the branch at `502231ff`:
- quality run 43939691749: 5 violations;
- public corpus run 43939649463: 156 passed, 2 failed.

Both were caused by this feature. This file records how each number was traced, and what changed.

## Method

The quality harness was run locally with `cargo run -p xtask -- quality --filter <t> --waybill-bin <bin> --no-gate`. It is the same harness as CI: `--offline`, `--root-name <target>`, `--root-version <short pin>`.

There were three binaries, and nothing else changed between arms:
- **main:** `main` at `1d25955c`, built in a worktree;
- **head:** the branch at `502231ff`, the code CI measured;
- **fix:** the branch at `e3dfd637`.

Edge diffs come from scanning the harness's cached checkout with each binary, using the harness's flags. Edges are compared as `from-PURL -> to-PURL`.

The local harness reproduced CI on every target it could fetch. `cpp-mongo`'s cache was half-initialized, which is #1580; it was moved aside and re-fetched.

## The numbers

| target | main | head (CI 502231ff) | fix | band |
|---|---|---|---|---|
| python-ansible | edges 7, depth 2 | edges 18, depth 1, **flat** | edges 13, depth 2 | 5..11 |
| python-flask | edges 176 | edges 195 | edges 202 | 151..185 |
| python-pytorch | edges 45 | edges 99 | edges 97 | 40..50 |
| cpp-mongo | edges 442 | 478 (CI) | edges 475 | 381..467 |

## Cause 1: FR-002 flattened a scan-root project (ansible, pytorch)

FR-002 gave a scan-root requirements file to the document's subject, even beside a scan-root main module. Under `--root-name`, the subject is a synthetic root and the main module is its child. So each package got a second, direct edge from the root.

**ansible:**
- `ansible-core` already depends on its five requirements, through PEP 621 dynamic metadata (#883).
- The root gained `-> jinja2` and the rest, so every package sat at depth 1.
- 11 extra edges. Inference, from the counts rather than from the head document, which was not kept: they are the 5 duplicates plus the 6 `test/units/requirements.txt` packages that are on `ansible-core` after the fix. Those six had no edge at `main`.

**pytorch:** all 54 new edges came from `pkg:generic/python-pytorch`. None of the 54 targets had an incoming edge at `main`. They come from:
- `docs/requirements.txt` (22);
- `requirements.txt` (12);
- `requirements-build.txt` (11);
- three nested tool directories (8);
- both root files (1).

**Fix:** the owner walk includes the scan root (`declared_attach.rs`).

**After the fix:**
- ansible has 13 edges at depth 2. That is main's 7, plus the six `test/units` packages, now on `ansible-core`.
- pytorch has 52 new edges, all from `pkg:pypi/torch`. The other 2 of the 54 were already `torch`'s targets at `main`, and V3 doesn't duplicate an edge.

**Why it survived:** in every contract case with a scan-root project, the root selector promoted that project to subject. There, the two rules produce the same document. C-14 now has the corpus shape.

## Cause 2: a compiled file was flattened (flask)

flask's `examples/celery/requirements.txt` is pip-compile output with 19 pins. Only `celery` and `flask` are `# via flask-example-celery (pyproject.toml)`. The pass gave the example an edge to all 19.

**Fix:** the pip reader reads `# via` into exact-PURL edges between pins of one file, and the pass attaches only the pins no sibling reaches (FR-002a).

**After the fix:**
- The example gains no edge of its own: `celery` and `flask` already came from its pyproject.
- 26 edges are added, one per parent named on a `# via` line: `click` has 5, `vine` 3, `markupsafe` 2, every other transitive pin 1.
- That is the closure that #1234 recorded as dangling, and the I7 cell now passes. The declaration is removed.

## cpp-mongo

There are 33 new edges, all from `pkg:pypi/mdb-python-deps`, the scan-root poetry project. They come from nested requirements files that have no project of their own:
- `buildscripts/cost_model` (22);
- two `wiredtiger/tools` directories (6);
- `query/benchmark/data_generator` (5).

25 of the 33 targets had no incoming edge at `main`. The other 8 were reachable through `poetry.lock`, and they now also carry the declared edge.

## Public corpus (run 43939649463)

**python-flask I7, `StaleDeclaration — holds`:** that is cause 2's thirteen. At `502231ff` they were reached by flat edges. With the fix, they are reached through their parents.

**`m1218_foreign_build_system_can_be_made_to_fail`:** this test stripped `codespell`'s `foreign-build-system` marker in go-opentelemetry. `codespell` is declared by the scan-root `requirements.txt`, so it is now a dependency of the scan-root Go module, and there is no marker left to strip.
- The test is re-anchored on maven-guice's two `pkg:bazel/*` rules, the only other population with that reason.
- It is count-based, because maven-guice's I7 is a declared failure (#1235): the count goes from 3 to 5.

**`m1218_dead_lockfile_entry_withheld_raises_the_residual_count`:** it now starts from a passing flask cell. Withholding the ten `uv.lock` markers fails exactly ten.

**Other changed goldens, in PyPI edges only:**
- **haskell-language-server:** `docs/requirements.txt` belongs to the scan-root hackage main module. That is 4 edges, which were on the root at `502231ff`.
- **pants-example-python and pants-example-django:** there is no project, so root `requirements.txt` packages go to the subject. These are 2 edges and 1 edge, to packages already reachable through the resolve.
- **go-opentelemetry:** `otel -> codespell`.

The local go-opentelemetry regen also differs in Go transitives, which come from the host's module cache. The committed goldens come from CI.

## Recorded, not changed

`classify_requirements_scope` (`pip/requirements_txt.rs`) reads a file's scope from two places:
- its **immediate** parent directory: `docs`, `doc`, `documentation`, `test`, `tests`, `ci` or `.ci`;
- failing that, its file name: `-dev`, `-test`, `-docs` or `-ci`.

So `docs/requirements.txt` (pytorch, haskell-language-server) is optional, and its owner edges are typed optional.

A deeper test layout is not. ansible's `test/units/requirements.txt` has the parent `units`, so it is classified as `Main`, and its six packages are required dependencies of `ansible-core`. The components already had that scope at `main`. This feature only gave them an edge.

Classifying by any ancestor would change other documents. It is a separate issue, filed with this PR.

*Correction:* an earlier draft of this section said scope ignores the directory. It reads the immediate parent.
