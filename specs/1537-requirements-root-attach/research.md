# Research: declared requirements attach to the project that declares them

Base: `main` at `283e4c19` (code read) and `1d25955c` (branch base). The measurements below were run with a release build of `283e4c19`. The scratch fixtures are under the session scratchpad and are rebuilt as committed tests by T002.

## R1. The cause, measured and read

**Measured: a submodule switches the fallback off** (#1566's reproduction). A `requirements.txt` pins two packages, scanned with and without one git submodule:

| Format | No submodule | With submodule | Verdict |
|---|---|---|---|
| CycloneDX | 2 of 2 attached | 0 of 2 | `complete` |
| SPDX 2.3 | 2 of 2 | 0 of 2 | `complete` |
| SPDX 3 | 2 of 2 | 2 of 2 | `complete` |

**Measured: a Python project switches it off too, with no submodule.** The scan root holds a `pyproject.toml` declaring `waybill-fixture-c` and a `requirements.txt` pinning `-a` and `-b`.
- The root is the main module `pkg:pypi/waybill-fixture-app@1.0.0`.
- `-c` is attached; `-a` and `-b` are not, and carry no orphan reason.
- The verdict is `partial`, which is correct here.

So the defect predates #1515. #1515 only made it visible in repositories that have no Python project.

**Measured: nested requirements files with no project anywhere attach.** `requirements.txt` at the root and `tools/requirements.txt` both attach, and the verdict is `complete`. The fallback fires because the root has no edges.

**Read: why the formats disagree.** A top-level submodule's root edge is not a relationship. It is a *reader root anchor* (spec 1530, research R5), passed through a thread-local (`supplement::install_reader_root_anchors`, installed in `cli/scan_cmd.rs:4462–4490`). It shares the cell with #1009's supplement anchors. Each copy of the fallback adds the anchors at a different point:

| Copy | Where the anchors are added | Effect |
|---|---|---|
| CycloneDX, `generate/cyclonedx/dependencies.rs:116` | before the gate (`target_has_no_edges`) | the gate is off |
| SPDX 2.3, `generate/spdx/document.rs:595` | before the gate (`:729–` #236 block) | the gate is off |
| SPDX 3, `generate/spdx/v3_document.rs:862` | after the gate (`:746–`) | the gate is on |
| completeness, `generate/graph_completeness/mod.rs:221–272` | not at all (no `current_root_anchors` call) | the mirror fires, so `complete` |

That explains each row of the first table: the CycloneDX and SPDX 2.3 orphans, SPDX 3 attaching, and every document saying `complete`.

**Read: why requirements packages need the fallback at all.**
- `pip/requirements_txt.rs:105` builds each entry with `source_path` set to the requirements file and `depends` empty.
- No main module names them, except through `[tool.setuptools.dynamic]` (#883).
- So the fallback is their only route to the root.

## R2. One root-edge rule, shared by all four consumers

**Decision.** A new `generate/root_edges.rs` computes the root's direct targets once:

```
root_targets = rel_targets(root)                                   // the scan's own edges
             ∪ anchors                                             // supplement + reader + declared-root (R4)
             ∪ (graph_tops  if rel_targets(root) ⊆ resolve_anchors)  // #236 fallback, gate unchanged
```

All four consumers call it: the CycloneDX dependency builder, SPDX 2.3, SPDX 3 and the completeness classifier. Each still renders the edges in its own format, including SPDX's scope-typed spelling (`synthesized_root_edge`).

**Anchors never count toward the gate.** The gate keeps exactly one meaning: *the scan's dependency graph gives the root no edge*. Anchors are root edges supplied out of band, added afterwards in every consumer. That is what SPDX 3 does today.

This is not a new exemption. It removes the ordering difference that created the disagreement, and it deletes the per-copy anchor handling instead of adding a fifth case to it.

**The copies differ in three more ways** (read at `283e4c19`). The shared function has to settle each:

| Question | CycloneDX | SPDX 2.3 | SPDX 3 | Completeness | Decision |
|---|---|---|---|---|---|
| Which roots get the fallback | any root | only a synthetic root (`if synthetic_root_added`, `document.rs:763`) | only a synthetic root (`v3_document.rs:746`) | any root | **Any root.** A main module promoted to root that declares nothing still needs the scan's other components reachable. CycloneDX and the classifier already agree. |
| Edges exempt from the gate | resolve anchors (#894) | none: any outgoing edge counts | resolve anchors, `dependsOn` only (#1108, #1438) | resolve anchors | **Resolve anchors, and only dependency edges count.** Tool-use and containment never switch it off. This is the union of the fixes already made one copy at a time. |
| Which components are graph-tops | every component no edge targets | additionally `parent_purl.is_none()` | additionally `parent_purl.is_none()` | every component no edge targets | **`parent_purl.is_none()`.** A nested component is reached through its container's containment. Attaching it to the root as a dependency would assert something nothing declared. CycloneDX and the classifier change. |

All three shift output in some format. R9 predicts where, and T019 checks it.

**Consequence.** In a scan whose root has anchors but no relationship edge, CycloneDX and SPDX 2.3 will now run the fallback, as SPDX 3 already does. Examples are a repository with submodules and no main module, or a supplement scan. That is #236's behaviour for a root with no edges (FR-005), not a new star (FR-004). On #1566's super-project it restores the pre-#1515 root edges, 160 per the reporter, plus the 11 submodules.

**Alternatives considered:**
- **Anchors count toward the gate everywhere**, as CycloneDX does today, and requirements are anchored. Rejected. It fixes the requirements packages, but strands every other component the fallback covered in any repository with a submodule. On #1566's super-project that is roughly 149 of 160. It also contradicts SPDX 3 instead of agreeing with it.
- **Exempt reader anchors only** in each copy. Rejected by the spec: a fifth exemption in four places is the pattern that drifted.
- **Move root selection to scan time** and turn anchors into relationships. Rejected for this feature. Each format chooses its root at emit time (#1009's comment), and moving that is a larger change than this defect needs.

## R3. Which project declares a requirements file

**Decision.** A scan-time pass, `scan_fs/declared_attach.rs`, runs after component resolution and before lifecycle-scope edge typing (`apply_lifecycle_scope_to_edges`).
- **Which components it covers:** each `pkg:pypi` component with at least one declaring file whose basename matches the pip reader's own glob, `requirements*.txt`. A cget `requirements.txt` is not pip (#1522) and produces no `pkg:pypi` component.
- **Where the walk starts:** the declaring file's directory.

**Rule, per declaring file:**
1. Walk the ancestors from the file's directory up to **and including** the scan root. The first directory with an owner wins, and the pass adds `owner DependsOn component` (FR-002, FR-003).
2. **No owner anywhere:** a declared root anchor (R4; FR-003: "ultimately the root").
3. **Only direct pins (FR-002a, R15):** a pin that another pin of the same file reaches is skipped.

*Corrected at implementation.* The rule first stopped the walk below the scan root, so a scan-root file went to the subject even beside a scan-root main module. See R15.

**Owners, built into an `OwnerIndex` from directory to owner PURL:**
- **Main modules:** every component whose `waybill:component-role` is `main-module`. Its directory is the parent of its manifest path, `source_file_paths[0]`. All 21 readers that emit main modules qualify (`cargo`, `npm`, `pip`, `gem`, `maven`, `golang`, `nuget`, …).
- **Git submodules:** their directories. The reader computes them (`git_submodule/mod.rs:261`, `purl_of_dir`) and then drops them, because the entry's `source_path` is the `.gitmodules` file. The map is passed out through `DbScanResult.diagnostics`, as `git_submodule_coverage` already is. **No wire change.**
- **CMake, Conan and vcpkg emit no project component** (grep: no `main-module` role in `cmake.rs`, `conan.rs` or `vcpkg.rs`). A CMake-only subdirectory therefore owns nothing, and the walk continues upward. The spec's FR-003 named CMake as an example; that is corrected to match.

**Several owners in one directory.** The choice is deterministic:
1. a `pkg:pypi` main module, because it is the same ecosystem as the declaration;
2. then any other main module, lowest PURL first;
3. then a submodule.

A submodule's own `pyproject.toml` therefore owns its requirements files before the submodule component does.

**No self-edges.** If the owner is the component itself, the pass skips it.

**Edges carry no scope.** The pass emits plain `DependsOn`. The existing `apply_lifecycle_scope_to_edges` turns it into `DevDependsOn` and the others from the target's scope (`classify_requirements_scope`), as it does for every other edge (FR-010).

## R4. Declared root anchors

The pass returns the PURLs that land on the root (rules 1 and 3) as `ScanResult.declared_root_anchors`. `scan_cmd` adds them to the reader anchors it already installs for submodules (`:4462–4490`). They then reach every format through `root_edges` (R2).

Using anchors, not relationships, is required. The root is chosen per format at emit time, and may be a main module, a placeholder or an operator override.

## R5. Every declaring file is kept

`pip/mod.rs:572`, `merge_without_override`, drops a later entry with the same PURL. A package pinned in both `requirements.txt` and `tools/requirements.txt` keeps only the first file. The second declaring project is then lost.

**Decision.** On a duplicate, append the later entry's `source_path` to the kept entry's `extra_source_paths`, instead of dropping it. This uses an existing field, which feeds C18 `waybill:source-files` through milestone 148's union.

**Effect.** C18 gains the second path where a package is declared twice. That is a corpus-visible, correct change (SC-004, research R9).

## R6. Completeness reads the same edges

`compute_graph_completeness` builds the root's adjacency from `root_edges` (R2), not from its own mirror at `mod.rs:221–272`. The multi-root seed set is unchanged (clarification 3). The verdict can then no longer disagree with the emitted graph about the root. That was SC-003's failure in R1.

## R7. Orphan reasons (FR-008)

Under R3, every requirements-declared component is placed: on an owner, or on the root. The population that this feature's rule *declines* to attach is therefore empty by construction. FR-008 is enforced as an invariant: a test asserts that no `requirements*.txt`-declared component is unreachable from the seed set.

**Recorded, not changed:** an owner project that is itself unreachable is outside this feature. That is #1214, and its reason is the owner's.

## R8. `-r` includes are not followed

`requirements_txt.rs` reads every `requirements*.txt` in a project directory. It does not follow `-r` or `-c`; both are skipped as option lines. A file reached only through `-r` is never read, so its packages are not components. The usual case is Django's `requirements/base.txt`.

The spec's edge case "an include declares the included file's packages" therefore holds only for included files the reader also reads directly. Following includes is a reader-coverage gap, filed as a follow-up. It is not attachment.

## R9. Predicted corpus effect (SC-004)

**Prediction, not measurement.** Goldens change only where one of these holds:
1. a scanned tree has a `requirements*.txt` and the root, or an enclosing project, has relationship edges, so the packages gain edges;
2. a package is pinned in two requirements files, so C18 gains a path;
3. a scan has anchors and a root with no relationship edges (submodules or a supplement, with no main module), so the CycloneDX and SPDX 2.3 fallback now runs;
4. a completeness verdict changes because of R6;
5. **SPDX only:** a non-synthetic root (a promoted main module) with no dependency edges now gets the fallback, matching CycloneDX;
6. **SPDX 2.3 only:** a root whose only outgoing edges are resolve anchors, or non-dependency edges, now gets the fallback;
7. **CycloneDX only:** a nested component (one with `parent_purl`) that nothing depends on is no longer attached to the root by the fallback.

Cases 5–7 are format-parity corrections the spec requires (FR-006), not side effects. T019 checks every changed file against these seven. If one of 5–7 turns out to touch many targets, the review records the count and proceeds or stops on that number before merge.

## R10. Measuring #1538 in CI (SC-006)

The fixture repository's workflows scan SDK tarballs and wheels, not TheRock's source tree. A new `measure-source.yml` in `kusari-sandbox/waybill-fixture-therock-sdk`:
- **Clone:** TheRock at #1538's pin, `b1cae390cc745283dce25581415353f9bc906162`.
- **Scans:** at `base_ref` and `waybill_ref`, before and after `git submodule update --init --depth 1`.
- **Disk:** #1538 measured the checkout at about 16 GB, more than a hosted runner has free. The job first removes preinstalled toolchains with plain `rm -rf` (dotnet, android, ghc, CodeQL), with no third-party action.
- **Report:** #1538's counting rule. PyPI components whose source files are the super-project's own requirements files (`requirements*.txt` at the top, `build_tools/`, `external-builds/`), each checked for an incoming edge and an orphan reason. Also the root's direct-dependency count.

## R11. SC-008, the root-only completeness count

For each corpus target, a committed probe (`measurements/root_only.py`, run with `python3 -I -B`) reads the CycloneDX document from the regen artifact. It computes reachability from `metadata.component` alone over `dependencies[]` and reports documents marked `complete` that have an unreachable component.
- **Check against the tool:** the probe is first validated against the tool's own verdict on one target, where root-only and multi-root must agree.
- **Use:** the result is recorded in a follow-up issue only.

## R12. Other manifests that rely on the fallback

**Read.** Conan, vcpkg, CMake `FetchContent` and any other design-tier reader whose components have no owning main module reach the root only through the fallback. #1566's table shows them attached because its arms have no submodule. Placed next to a root main module, they would be stranded as requirements packages are.

**Decision.** Out of scope; FR-001 is about pip requirements files. T001 measures one such arm (`pyproject.toml` + `vcpkg.json`) and files a follow-up with the result. The shared rule (R2) and `OwnerIndex` (R3) are built so that other declaring readers can join later without touching the emitters.

## R13. Cost

- **The pass:** one walk up the ancestors per declaring file, over an index built once per scan. The index holds every main module and submodule. Cost is linear in the number of declaring files times directory depth.
- **`root_edges`:** replaces four computations with one.
- **No I/O, subprocess or network** (FR-011).

## R14. A file named like a constraints file (measured, T001)

**Measured:** the reader reads `requirements-constraints.txt` as a declaration, and its package is attached like any other (`measurements/repro.md`, arm c12).

**Decision:** attach it like any other file the reader reads. Use no filename heuristic.
- In pip, "constraint" is a property of how a file is referenced (`-c`), not of its name. The reader follows neither `-r` nor `-c` (#1583).
- Guessing from a name would drop packages the project may install directly.

The spec's "not a declaration" edge case applies to files reached through `-c`, and those are not read today. FR-008's population stays empty (R7).

**Vendored directories (arm c11):** `vendor/x/requirements.txt` is not read, so there is nothing to attach (spec edge case: "whether such a file is read is unchanged").

## R15. Two corrections the corpora found (implementation, 2026-11-16)

Both corpora went red at `502231ff` (quality run 43939691749, public corpus run 43939649463). The cause was this feature in both cases. The evidence is local harness runs at `main` and the branch, with one variable changed (`measurements/quality.md`).

**1. FR-002 flattened scan-root projects.** A scan-root file went to the subject even beside a scan-root main module. Both harnesses scan with `--root-name`, so the subject is a synthetic root and the main module is its child. Every package the main module already depended on also got a direct edge from the named root:
- `python-ansible`: `ansible-core` already declares its five requirements through PEP 621 dynamic metadata (#883). Depth went from 2 to 1, which is flat, and edges from 7 to 18.
- `python-pytorch`: 54 packages that were orphans at `main` were attached to the root rather than to `torch`.

**Decision.** The owner walk includes the scan root, and the subject gets a file only when no project encloses it. The unit test that asserted the old rule, and mutation M6, which guarded it, are inverted. The contract never had the corpus shape, a scan-root project under a named root. C-14 adds it, and it fails under M6.

**Why it survived:**
- Every contract case with a scan-root project let the root selector promote that project to subject. There, "the subject" and "the scan-root project" are the same component, so they cannot tell the two rules apart.
- Corpus step 1 ran before US1 existed.

**A diagnostic trap:** the first local repro used a binary built between `16959878` and `10c2660a`. That binary had neither copy of the scan-root guard, so it showed the correct shape. The repro was rebuilt at `HEAD` before the cause was named.

**2. Every pin of a compiled file was made direct.** pip-compile pins the whole closure. flask's `examples/celery/requirements.txt` has 19 pins, and only `celery` and `flask` are `# via flask-example-celery (pyproject.toml)`. The pass gave the example all 19.

**Decision.** The pip reader reads `# via` in both forms, block (pip-compile ≥ 6) and inline (older versions). Each record becomes an exact-PURL `depends` edge from the parent pin to the pin. A scan can hold several versions of one name (flask has three `click`), so a name-only edge could bind to the wrong one. The pass then attaches to the owner only the pins no sibling reaches, plus any pin in a sibling-only cycle. A `via` that is not a pin (`-r requirements.in`, or the project itself) matches nothing, and that pin stays direct.

On flask, the 26 new edges match the file's `# via` lines one for one. The thirteen unreachable pins of #1234 are now reached through their real parents, and that I7 declaration is removed.

**Alternatives considered:**
- **Leave compiled files flat and give the 17 an orphan reason.** Rejected. The file states the edges, and a reason would describe waybill's omission as a property of the repository. #1234's declaration says exactly that.
**Recorded.** A sibling edge is any relationship between two pins of the same file, whatever its source. Today only the file's own `# via` writes such edges. Another reader, such as venv metadata, could add one between two packages that one hand-written file both lists. That pin would then lose its direct edge, but it is still reached, through the sibling.
