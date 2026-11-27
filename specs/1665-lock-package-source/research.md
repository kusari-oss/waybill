# Research: Haskell lock-file packages record their lock file

## R1 — The defect on `main`, measured (FR-001)

`measurements/measure.sh` reads each component whose `waybill:source-type` is `hackage-stack-lock` or `hackage-freeze` from the binary's CycloneDX output. It ran with a release build of `main` at `c810faf9` over the 12 Haskell corpus targets, plus the three hpack `-no-generated-cabal` variants. Output is in `measurements/main.tsv` and `measurements/main/`.

| Target | Lock components | No source file | Unreachable, no reason |
|---|---|---|---|
| hpack-hs-opentelemetry (and its variant) | 6 | 6 | 0 |
| hpack-lootbox (and its variant) | 16 | 16 | 0 |
| every other target | 0 | 0 | 0 |

- **The empty source is `[""]`.** `waybill:source-files` holds one empty string, not an absent value, because `source_path` is `""`. The probe's first version counted only absent values and reported 0; that was corrected before this table was taken.
- **Reachability.** hs-opentelemetry has 5 lock components reachable and 1 unreachable (`thread-utils-finalizers`, `edgeless-lock-pin`). lootbox has 5 reachable and 11 unreachable, all `edgeless-lock-pin`.
- **No `cabal.project.freeze` anywhere in the corpus.** The freeze path is exercised only by the positive control.
- **No package pinned by two locks in the corpus.** hs-opentelemetry has two `stack.yaml.lock` files, but `instrumentation/yesod/stack.yaml.lock` pins only a snapshot. FR-004 and FR-005 are exercised only by the positive control.

## R2 — Recording the file

**Decision.**
- `StackLockEntry` and `CabalFreezeEntry::ExactPin` each gain `source: PathBuf`, set by `parse_stack_lock` or `parse_cabal_freeze` to the path they parsed.
- The builders set `source_path` from it, as `build_snapshot_placeholder` has done since spec 1663. Absolute paths are made rootfs-relative at emit, as for every reader (`scan_fs::sbom_path`).

**Rationale.** This is the same field and the same normalisation snapshots already use, so C18 stays one shape.

## R3 — One PURL from several files (FR-004), different versions (FR-005)

**Decision.**
- Phase F currently keeps the first component per PURL through `seen_purls` and drops later ones together with their file.
- It becomes a merge, mirroring spec 1663's snapshot merge, keyed by PURL across both the freeze and the stack-lock loops. A later entry's `source_path` joins the first component's `extra_source_paths`, sorted and unique.
- Different versions are different PURLs, so each keeps only its own files. This needs no special case, and the fixture tests it (FR-005).

**Alternative rejected.** Keying the merge by name would attribute one version's lock file to another version.

## R4 — Orphan classification (FR-006)

`generate/orphan_reason.rs` reads source files in three places: `LOCKFILE_NAMES` (dead-lockfile-entry), `NODE_GRAPH_LOCKFILE_NAMES` (mechanism D, `flake.lock` only), and `MANIFEST_NAMES` (mechanism C). None of the three lists contains `stack.yaml.lock` or `cabal.project.freeze`, so a newly recorded file can match none of them.

- **`hackage-stack-lock`:** the arm is chosen by source type (spec 1663) and stays `edgeless-lock-pin`.
- **`hackage-freeze`:** has `sbom_tier = source`, so mechanism E (design tier with empty sources) never applied before and does not apply now.

The prediction is that no orphan reason moves. The branch measurement checks it.

**Observed, not changed:** an unreachable `hackage-freeze` pin gets no orphan reason either before or after this change. The corpus has no freeze file, so I7 has never seen one. It was reproduced on `main` and filed as #1741.

## R5 — Merging with other components (FR-007)

`resolve::deduplicator` already unions `evidence.source_file_paths` when it merges same-PURL components (`deduplicator.rs`, around line 260) and sorts them (around line 472). A lock pin's design-tier twin never forms: a pinned name suppresses the manifest's versionless declaration (`pins_by_dir`, #938), and a versioned declaration of the same PURL would merge by union. So no new code is needed, and the measurement checks for lost or extra files. The union itself is already tested by `canonicalize_source_files_by_purl_same_purl_different_parent_unions_paths_md148` and `..._three_entries_full_union_md148`, so this spec adds no test of it (FR-007).

## R6 — Branch measurement (FR-001, T009)

This is `measure.sh` with a release build of the branch, written to `measurements/branch.tsv` and `measurements/branch/`.

- **Summary table.** The only rows that change are hs-opentelemetry and lootbox, each with its variant. `no_source_file` falls from 6 to 0 and from 16 to 0, while `lock_components` and `unreachable_unexplained` are unchanged.
- **Per component.** Comparing purl, source type, reachability and orphan reason across all 15 scanned targets, no line differs. The only column that changed is source files: all 22 lock components now read `["stack.yaml.lock"]`. hs-opentelemetry's pins are in its root lock; its `instrumentation/yesod` lock pins none.
- **Whole document.** Across the 15 targets, the set of component PURLs and the multiset of orphan reasons are identical between `main` and the branch, so no component appeared, disappeared or merged differently.
- **Prediction (R4) confirmed:** no orphan reason moved.
