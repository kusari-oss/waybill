# Research: Label pip requirements files under docs and test directories

Traced in the source at `main` `52b96f1d`. TheRock counts come from waybill's own document of CI measure-source run 44477517009 (spec Background).

## R1. The name rule's scope words

**Finding.** `matches_scope_filename(name, scope)` accepts `requirements-<scope>` or `<scope>-requirements`, followed by the end of the stem, `-` or `.`. It is called for the scope words `dev`, `test`, `docs` and `ci`, in `classify_requirements_scope` and in `is_requirements_file`. `requirements-doc.txt` fails because `doc` is not in the list. `requirements-tests.txt` fails too: after `requirements-test` comes `s`, which the boundary rejects. The unit table `m670_scope_classify_filename_signals` covers neither.

**Decision.** Each scope keeps one C155 value, and the rule accepts a set of words for it:

| C155 | words in a file name |
|---|---|
| `dev` | `dev`, `development` |
| `test` | `test`, `tests`, `testing` |
| `docs` | `doc`, `docs`, `documentation` |
| `ci` | `ci` |

`doc`/`docs`/`documentation` is the parent-directory rule's existing set, and `tests` is its plural, as `docs` is of `doc`. The boundary is unchanged, so `requirements-docker.txt`, `requirements-testinfra.txt` and `requirements-devel.txt` stay runtime.

**Spec amendment.** FR-006 named the long forms. `tests` is added as the same defect in the plural (a second component, the SBOM, observes it).

**Alternatives rejected:**
- *Substring match* (`doc` anywhere): `requirements-docker.txt` would read as docs. Rejected by the existing word-boundary test.
- *Underscore as a separator* (`requirements_dev.txt`): no evidence it is common, and the boundary has never accepted it. Out of scope.

## R2. Which files are read

**Finding.** Spec 1613 made the reader read `requirements*.txt`, a `<scope>-requirements*.txt` the classifier labels, and any `*requirements*.txt` under a listed dot-directory (`is_requirements_file`). Discovery calls the same function (`pip/mod.rs`, `on_pip_file` and the project-root check), and the walker's pattern already admits `**/*requirements*.txt`.

**Decision.** `is_requirements_file` uses R1's word sets. Newly read: `doc-requirements*.txt`, `development-requirements*.txt`, `documentation-requirements*.txt`, `tests-requirements*.txt` and `testing-requirements*.txt`. Each is labelled by its name, so none reads as runtime. Every `requirements-<word>*.txt` name is already read; R1 only changes its label.

FR-002's directories do **not** widen what is read, unlike spec 1613's dot-directories. `docs/mkdocs/mkdocs-requirements.txt` stays unread: no name rule labels it, and its directory would be the only signal. Spec 1613 widened reading for dot-directories only because the walker had skipped them; `docs/` and `tests/` were always walked.

## R3. The ancestor search

**Finding.** `listed_ancestor_class(path, root)` (`project_roots.rs`) strips `root` from `path`, then walks the parent's components nearest-first and returns the first listed dot-directory. A path outside `root` is compared whole. It has three callers:
- `classify_requirements_scope_under`, the pip classifier;
- `is_requirements_file`, for the dot-directory reading rule;
- `ci_scope::apply` (spec 1613 R5), which scopes **any** ecosystem's component declared only under listed dot-directories.

**Decision.** A new function in `requirements_txt.rs`, used only by `classify_requirements_scope_under`, walks the same ancestors nearest-first. Each ancestor is checked as a listed dot-directory (its class) and as FR-002's names (`docs`, `doc`, `documentation` → docs; `test`, `tests` → test), case-insensitively; the first hit of either kind wins (FR-003). It runs only when the parent and the name give nothing, as today.

- **The scan root bounds it** (FR-004, clarified). The search covers the path relative to `root`, so the root's own name never counts.
- **A path outside `root`** keeps today's behaviour: the dot-directory search only, over the whole path. FR-002's names are not applied there, so a scan of `/home/u/tests/proj` cannot label every file by `tests`.

`listed_ancestor_class` and `ci_scope` are untouched. FR-002's rule is for pip only (FR-010), and `ci_scope`'s directory rule moves every ecosystem.

**Alternative rejected:** adding `docs`/`tests` to `LISTED_DOT_DIRECTORIES`. That list also opens dot-directories to the walker and feeds `ci_scope`, which would scope the 15 Cargo crates FR-010 leaves out.

## R4. Scope merge and C155's single value

**Finding:**
- The deduplicator merges lifecycle scopes per PURL (`merge_lifecycle_scope`): one runtime declaration makes the component runtime.
- `ci_scope::apply` (spec 1613 R8) then removes C155 from any component that ends runtime.
- When a package is declared in several scoped files, C155 keeps the first file's value (spec 1613 research: "the first file wins").

**Decision.** No change. FR-007 holds through the existing merge and R8. C155 for a package declared in, say, a docs file and a test file stays first-file-wins. The scope is non-runtime either way, and the value set is unchanged.

## R5. Attachment and orphans

**Finding.** `declared_attach.rs` attaches packages from files named `requirements*.txt` to the owning project (spec 1537). Its own `is_requirements_file` is that prefix test. Packages from `<scope>-requirements*.txt`, read since spec 1613, are not attached that way, and spec 1613's TheRock run had 0 unreachable components without an orphan reason.

**Decision.** No change to attachment. Files labelled by R3 are `requirements*.txt` and attach as today. Newly read R2 names are handled as spec 1613's `docs-requirements.txt` is. A contract case (W-11) asserts that no component is unreachable without an orphan reason.

## R6. Why the existing passes miss these files

- **Spec 1533's test-scope pass** (`scan_fs/test_scope.rs`) scopes only binaries, file-tier components and installed-library records (`kind()`). Its directory rule runs only when the tree is an installed one. A source tree's pip package never qualifies, which is why rocjitsu's `tests/corpus` packages stay required.
- **Spec 1269's propagation** carries a non-runtime scope to what a non-runtime component reaches. A package declared only in a runtime-labelled file has no such source.

Neither changes here.

## R7. Predicted movement in the corpora

*Prediction, not measurement.* These are files on disk in the local corpus caches, and in waybill's committed goldens, that the new rules would label. The CI runs measure the effect.

- **Public corpus:** no file in any golden's pip source files has a `docs`/`test` ancestor above its parent, or a new name word. Predicted unchanged.
- **Quality corpus:**
  - **python-ansible:** `test/units/requirements.txt`. Other `test/**` files there are named `*.requirements.txt`, which the reader does not read.
  - **cpp-mongo:** wiredtiger's vendored `test/3rdparty/testtools-2.7.1/requirements.txt`, and `readthedocs-requirements.txt` beside it (unread; `readthedocs` is not a scope word). This is FR-004's vendored-under-tests case.
  - **cmake-nlohmann-json:** `docs/mkdocs/requirements.txt`.
  - pytorch, flask and pants-backend-ai: none.
- **TheRock:** the 14 files; plus `requirements-development.txt` (3 packages, dev).
- **TheRock, files labelled today:** this one is measured, by reading the labels in run 44477517009's document. None of the 20 files with a C155 label today changes label. Each is labelled by its parent, its name, or a dot-directory nearer than any `docs`/`test` ancestor. The one with both kinds above it, `base/rocm-cmake/test/docsphinx/docs/.sphinx/requirements.txt`, keeps `docs` from `.sphinx`. FR-003's amendment of spec 1613 (`.github/docs/x`) has no instance on TheRock.

## R8. Scan time

The classifier runs once per requirements file read (71 on TheRock), walking a handful of path components. No new file reads beyond R2's names, of which TheRock has none. **Budget:** ≤ 1.05× `main`'s median, interleaved (SC-007).
