# Tasks: hpack `!include` and YAML merge keys are read as hpack reads them (#1673, #1674)

**Input**: `specs/1649-hpack-include-merge/` (spec.md, plan.md, research.md, data-model.md, contracts/wire.md, quickstart.md)

**Tests**: requested by the spec (SC-001 to SC-006).
- **Written red.** M1–M4 and I1–I6 are written before the code that makes them pass. U1–U5 are written red against the include code before the bounds land.
- **Guards.** M5 and N1 pass throughout.

**Paths**:
- `HP` = `waybill-cli/src/scan_fs/package_db/haskell/hpack.rs`
- `HK` = `waybill-cli/src/scan_fs/package_db/haskell.rs`
- `IT` = `waybill-cli/tests/hpack_include_merge.rs` (new)
- `MS` = `specs/1649-hpack-include-merge/measurements/`
- `SP` = the session scratchpad
- Research decisions are R1–R9, and cases are contracts/wire.md's M1–M5, I1–I6, U1–U5 and N1.

## Phase 1: Setup

- [X] T001 Build `main`'s binary at `ea149065` into `SP` as `waybill-main-ea149065`, after confirming `git diff ea149065 -- waybill-cli waybill-common Cargo.toml Cargo.lock` is empty. Record the full SHA in `MS/control.md`.
- [X] T002 [P] Create `IT` with the harness from `waybill-cli/tests/binary_kind_technique.rs`: `tree`, `scan_with(bin, dir)` returning three documents' bytes and the log, CycloneDX components, parity, unique `bom-ref`s, and the control `control_byte_identical_to_main` (`WAYBILL_HPACK_MAIN_BIN`, `WAYBILL_HPACK_IDENTICAL`, version-4 UUIDs masked). Add: *As built: `hackage(s)` became `deps_of(s, package)`, the `pkg:hackage` names of one package's dependency edges, which is what every case asserts; `includes(s, kind)` added.*
  - `hackage(s) -> BTreeSet<String>`: the `pkg:hackage` names;
  - `c200(s) -> Value`: the parsed C200;
  - `outcome(s, file)`: `read`, `partial` or `unread` for one manifest.
- [X] T003 [P] Corpus copies for measurement (R9). In `SP`, check out the four #1677 repositories at their pins (hs-opentelemetry `25f5b2d6…`, lootbox `2cd7b1e0…`, hs-temporal-sdk `d24313e3…`, ogmios `435189ae…`), each as committed and as a copy with the hpack-generated `.cabal` removed (the first four lines contain `hpack`, #1677's rule). Record the commands and file counts in `MS/corpus-copies.md`. *As built: 4 committed copies and 4 no-generated-cabal copies; hs-opentelemetry keeps 13 hand-written `.cabal` in the latter.*
- [X] T004 Write `MS/hpack_agreement.py` (R7): *As built: the comparison drops a package's own name, reads `build-tool-depends` too, and lists hand-written-`.cabal` packages as not compared; `agreement-main.md` records why. On `main` 16 of 46 agree, and every difference is a missing package. Found #1684.*
  - for each repository, make a **third, dedicated copy** for hpack (hpack resolves `../` includes, so it needs the whole tree, and it writes a `.cabal` beside each `package.yaml`). Run `nix run nixpkgs#hpack -- --force` there for each manifest, then delete that copy. T003's eight copies are never written to: run inside a no-generated-cabal copy, hpack would put back exactly the files it removed (analyze U1);
  - take the dependency names of every `build-depends` in the generated `.cabal`, unioned per package;
  - compare them with the names of the `pkg:hackage` dependency edges of that package's main module in a waybill CycloneDX document;
  - print per-package agreement and every difference.

  Run it against T001's binary on the four no-generated-cabal copies and record the result in `MS/agreement-main.md`. It must show disagreement where #1673 and #1674 bite; that is the control proving the comparison can fail.

## Phase 2: Foundational

- [X] T005 In `HP`, unit tests `s1649_serde_yaml_*` for R8: *As built: the unit tests live in `hpack/prep.rs`; the alias test uses 40 (corpus maximum 7).*
  - an anchor redefined later in a document resolves to the later definition;
  - a key named `<<__waybill_merge_0` survives `serde_yaml::from_str` as an ordinary key;
  - a document with 20 aliases to one anchor parses. T018 counts the aliases in the corpus's largest spliced document, and this test's number is raised to at least that (analyze L1).

  If any fails, stop and amend research R1 or R3 before going on.
- [X] T006 In `HK` and `HP`, thread `rootfs` from `haskell::read` through `finalize` to `hpack::read_manifest(path, root)`. There is no behaviour change; every existing test passes.
- [X] T007 In `HP`, add `IncludeResolution { manifest, file, include, outcome }` and `UnresolvedReason { Missing, OutsideRoot, Cycle, TooDeep, Unreadable, UnsupportedPosition }`, each with `as_str`. Carry them in `HpackReadOutcome`, and emit `includes-resolved` and `includes-unresolved` in `to_document_value` only when non-empty (R6). Nothing fills them yet, so output is unchanged. *As built: `UnresolvedReason`, `IncludeOutcome` and `IncludeResolution` live in a new child module `haskell/hpack/prep.rs` with the splice, rename and fold, re-exported from `hpack.rs`. `read_manifest` returns its include resolutions beside the result, so an unread manifest still reports them.*

## Phase 3: User Story 1 — a manifest that merges shared settings keeps them (P1) 🎯 MVP

**Goal**: `<<` keys apply with hpack's precedence, and repeated ones are read.
**Independent test**: M1–M5 pass, and N1 is byte-identical.

- [X] T008 [US1] Write M1–M5 in `IT` (contracts/wire.md). M1–M4 are red on this commit, and M5 passes. Record the red/green table in `MS/control.md`. *As built: M4 is green on `main` (the merge is ignored, so the explicit list survives); it is a guard, not a red case. `MS/control.md`.*
- [X] T009 [US1] In `HP`, `rename_merge_keys(text) -> String` (R4): rename each line-key `<<` (after indentation, optionally after `- `) to `<<__waybill_merge_<n>`, numbered in document order. Skip block-scalar bodies and multi-line quoted scalars. Unit tests `s1649_rename_*`:
  - every non-`<<` line is byte-identical;
  - `<<:` inside a `|` block, a `>-` block, a single-quoted string and a double-quoted string is not renamed;
  - mapping the renamed keys back to `<<` restores the input exactly.
- [X] T010 [US1] In `HP`, `fold_merges(&mut Value)` (R3, data-model's rule), bottom-up. In each mapping, take `<<__waybill_merge_<n>` in number order; a value is a mapping, or a sequence whose mapping items are taken in order. Insert only absent keys, ignore non-mapping sources, and remove the renamed keys. Unit tests `s1649_fold_*`: *As built: a literal `<<` key (flow syntax) is folded too; a merge whose value is still a tag is left for `strip_tagged`.*
  - explicit beats merged in either order;
  - first merge wins;
  - the sequence form;
  - a non-mapping source;
  - a merge nested inside a merged mapping.
- [X] T011 [US1] Wire T009 and T010 into `read_manifest`: rename before `serde_yaml::from_str`, fold after it, before `strip_tagged`. M1–M5 pass, and N1 (written now) is byte-identical to T001's binary. *As built: N1, M4 and M5 are byte-identical to `main`; M1 differs (positive control).*

## Phase 4: User Story 2 — a manifest that includes shared files is read through them (P1)

**Goal**: block-context `!include` is spliced, with anchors across files.
**Independent test**: I1–I6 pass; T004's agreement improves.

- [X] T012 [US2] Write I1–I6 in `IT`. They are red on this commit.
- [X] T013 [US2] In `HP`, `splice_includes(text, file, root, chain) -> (String, Vec<IncludeResolution>)` (R1):
  - for a line whose value is `!include <path>` (quoted or unquoted) after a key or `- `, replace the tag with the included file's text, recursively spliced first, relative to `file`'s directory. A key is any plain or quoted scalar ending at the first `: `, slashes included (analyze U2);
  - re-indent that text by the key's column + 2, and drop `---`, `...` and a BOM;
  - record each `Resolved` include;
  - leave any other `!include` in place.

  Wire it before T009's rename (an included file's `<<` keys are renamed with the rest). Unit tests `s1649_splice_*`: indentation at columns 0, 2 and 4; a sequence item; a nested include relative to the including file; a block scalar inside an included file keeps its relative indentation; a `_common/lib:` key; a quoted key; an unquoted path.
- [X] T014 [US2] Fill C200's `includes-resolved` from `read_manifest`'s resolutions in `HK`. A manifest whose includes all resolve, and that has no `defaults:`, is `read`: an `Include` unmodelled construct is reported only for an unresolved include. I1–I6 pass, and N1 is still byte-identical.

## Phase 5: User Story 3 — an include that cannot be resolved is reported (P2)

**Goal**: FR-004's bounds, with reasons.
**Independent test**: U1–U5 pass.

- [X] T015 [US3] Write U1–U5 in `IT`.
- [X] T016 [US3] In `HP`'s splice, add the bounds (R5): *As built: `unsupported-position` is detected on the line during the splice, not from leftover tags. A missing file is `outside-root` when its nearest existing ancestor resolves outside the root. `read_manifest` also marks the manifest partial on any unresolved include, beside `strip_tagged` (`MS/mutations.md`, Mj/Mk).*
  - canonicalise the target, and refuse one outside the canonical root (`OutsideRoot`);
  - refuse one already on the chain (`Cycle`) or past depth 8 (`TooDeep`);
  - a missing file is `Missing`, and a read error `Unreadable`;
  - an `!include` not in a splice position is `UnsupportedPosition`, detected in the leftover tagged values.

  The tag stays, `strip_tagged` removes it, and the manifest is `partial`. Fill `includes-unresolved`. U1–U5 pass.

## Phase 6: User Story 4 — nothing else changes (P2)

**Goal**: every difference traced; agreement with hpack.
**Independent test**: N1; the corpus diffs; SC-002.

- [X] T017 [US4] Mutations (SC-005), with `MS/mutate.py` copied from spec 1648's runner: *As built: 11 mutations killed, including Mc2 and Mg–Mi for FR-009 and FR-010; Mj and Mk each survive alone, by design.*
  - M-a merge precedence reversed (last merge wins);
  - M-b explicit-over-merge reversed;
  - M-c the rename also rewrites a non-`<<` key;
  - M-d includes resolved relative to the manifest, not the including file;
  - M-e the root bound removed;
  - M-f the depth bound removed.

  Record the killing tests in `MS/mutations.md`. Every mutation must be killed, the source is restored with `cp`, and the binary rebuilt.
- [X] T018 [US4] FR-001 measurement (R9): first confirm each no-generated-cabal copy still has 0 hpack-generated `.cabal` (analyze U1). Then scan T003's eight copies with this branch and with T001's binary. Record in `MS/corpus-counts.md`, per target, the manifests moving between `read`, `partial` and `unread`, and the `pkg:hackage` components gained or lost. Run T004's agreement on the four no-generated-cabal copies and record it in `MS/agreement-branch.md`. Every difference is fixed or explained (SC-002). *As built: all 46 manifests read; 49 includes resolved, 0 unresolved; 46 of 46 packages agree with hpack. The checkpoint fired: T018a.*
  - **Checkpoint**: if a count contradicts the spec, amend the spec first.
- [X] T018a [US4] Checkpoint outcome (added): T018 found three lowering defects that FR-003 exposes (`MS/probe_lowering.md`). Amend the spec with FR-009 and FR-010. In `HP`, keep the declared spelling and split `package:qualifier` as the `.cabal` reader does (`split_entry`, `merge_entries`), and accept a `when:` written as one mapping. Add cases L1–L3 to `IT` and contracts/wire.md. Then re-run T018's counts and agreement. *As built: also found #1685 (a package outside `cabal.project` is emitted) and #1686 (edges fold case-differing names), both pre-existing and filed.*
- [ ] T019 [US4] Corpus: dispatch `public-corpus.yml` with `-f branch=1649-hpack-include-merge -f regen_goldens=true`, and confirm the scanned commit.
  - Install only the `hpack-*` goldens from the artifact.
  - Confirm every other golden is byte-identical to the committed one.
  - Trace each hpack golden diff to manifests using `!include` or `<<`, matching T018.
  - Any new invariant failure on a newly readable manifest is investigated, then fixed or declared with its reason. A declaration made stale by this change is removed.

  Then run a verification dispatch (no regeneration) at the final commit, which must pass. Record in `MS/corpus.md`.
- [X] T020 [US4] SC-007 timing: on the four committed copies, 3 interleaved runs per arm (this branch, T001's binary). Record min, median and max and the median ratio in `MS/timing.md`. *As built: repeated at 7 runs; median ratios at most 1.027.*

## Phase 7: Polish

- [X] T021 [P] Docs: *As built: `docs/ecosystems.md` § hpack and `docs/reference/sbom-format-mapping.md` C200.*
  - `HP`'s module doc: `!include` is no longer unmodelled; `defaults:` still is;
  - the Haskell reader reference in `docs/reference/` (find the hpack section with grep): includes, merge precedence, the bounds, the C200 keys.
- [X] T022 [P] `CHANGELOG.md`: a Fixed entry citing #1673 and #1674, built from T018 and T019.
- [ ] T023 Run the full gate on the final commit, and record in `MS/gate-log.md` the per-target totals, `GATE_RESULT`, the expected line, and that `git diff ea149065 -- Cargo.lock '*Cargo.toml'` is empty.
- [ ] T024 Close-out: check every task's claim against the tree and `MS/`, note as-built differences inline, and tick each task.

## Dependencies

- **Setup**: T001 → T004 and T008. T002 → T008. T003 → T004.
- **Foundational**: T005 → T009 and T010. T006 → T013. T007 → T014.
- **US1**: T008 → T009, T010 → T011.
- **US2**: T011 → T012 → T013 → T014. The splice feeds the rename, so US1 lands first.
- **US3**: T014 → T015 → T016.
- **US4**: T017 after T016. T018 after T016. T019 after T018's checkpoint. T020 after T016.
- **Polish**: T021 and T022 after T019. T023 and T024 last.

## Parallel opportunities

- T002 with T003.
- T009 with T010 (different functions in one file; take care with merges).
- T020 alongside T019's CI run.
- T021 with T022.

## Implementation strategy

- **MVP**: Phases 1–3. Merge keys alone fix #1674's synthetic cases. On the corpus they change little until includes land (spec Background), so US1 and US2 ship together.
- **The corpus result is the acceptance check**: agreement with hpack on every corpus manifest (T018), and traced golden diffs (T019).
