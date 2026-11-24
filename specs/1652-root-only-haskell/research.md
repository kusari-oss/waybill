# Research: `root-only` keeps the packages the root builds, and records what it dropped

Code references are at `main` `317fbae`. Every figure is from `measurements/baseline.md`, which says how each was produced.

## R1. Why a nested Haskell package drops (FR-001, first question)

**Finding: neither suspected cause holds.** It is root detection.

- **How root-level is decided today:**
  - `ProjectDiscoveryMode::is_root_in_scope` (`generate/project_discovery/mod.rs`) calls `is_root_level`.
  - That compares `scan_root.join(root.source_dir)` with `scan_root`.
  - `source_dir` comes from milestone 215's `split::source_dir_for`. It takes the component's first evidence path, and strips the file name only when `is_manifest_basename` recognises it.
  - The list has no `package.yaml` and no `*.cabal`. So a root Haskell package's `source_dir` is `package.yaml`, which is a file and not the scan root, and the package is never root-level.
- **The effect:**
  - With no other root manifest, nothing is in scope, and the #1045 path returns the whole scan. That is f2, f3, f5, and all 8 Haskell corpus targets.
  - With another ecosystem's root manifest, the nested Haskell packages are not reached and are dropped. That is f1 and f4.
- **The projection already crosses main modules.**
  - `split::project_for_root` is a plain BFS over dependency edges. Its sibling demotion only removes `waybill:component-role` from the copies `--split` emits, and `apply_scope_filter` reads only the PURLs it reaches.
  - Evidence:
    - The one-variable experiment, with `package.yaml` recognised, keeps `a` and `c` and drops `d` under both modes;
    - f6 keeps a nested npm package through an edge;
    - go-opentelemetry keeps 3 nested main modules.
- **#1676's account is not reproduced at `317fbae`.** It reports the root and `base` kept and `waybill-fixture-a` dropped. Here the same shape (f2) falls back instead.
- **Decision:**
  - The directory test moves into `project_discovery`. A new `root_dir(root, scan_root)` returns `source_dir`, or its parent when `scan_root.join(source_dir)` is a file.
  - It is used by `is_root_level`, and by `filter.rs` where the in-scope directory set is seeded (`dir_key(&r.source_dir)`).
  - It is not an extension of `is_manifest_basename`. That list is consumed by `--split`'s file naming and `--split=directory` grouping, which FR-007 keeps unchanged.
  - The test covers every ecosystem whose evidence path is a manifest file, not only Haskell.
- **A consequence to state:** an ecosystem whose root manifest is missing from the list (`*.cabal`, `package.yaml`, `pubspec.yaml`, `*.csproj`, …) becomes root-level under `root-only` and `strict` where it used to fall back. No corpus target uses `--project-discovery`, so no golden changes. `all` is not touched.
- **Out of scope:** `--split`'s own `source_dir_for` has the same gap for Haskell. A Haskell sub-SBOM's directory includes the manifest file name. This is #1691 rather than part of this spec, because FR-007 holds `--split` fixed.

## R2. `waybill:workspace-member`, per reader (FR-001, second question)

- **Measured on the fixtures:**
  - every component carries the directories of its own evidence paths, as milestone 176 stamps them (`scan_fs::tag_components_with_workspace_member`);
  - a nested hpack package carries `["local-packages/a"]`;
  - `base`, which several manifests declare, carries every one of those directories.
- **Cargo members** carry `["."]` because their only evidence is the root `Cargo.lock`, which `filter.rs`'s own doc comment records.
- **Not used for FR-003:**
  - stamping a member's directory differently would change `all` output, which FR-007 forbids;
  - the directory follow-up does not walk edges from what it pulls in.
- **Decision:** declared members become additional in-scope roots (R4), and each is projected like a root.

## R3. The true drop set, and the count the log reports (FR-004)

- **The defect:** `nested_projects_ignored` is `all_roots.len() - in_scope_roots.len()`. That counts every nested main module, including the ones the projection then keeps:
  - f6 reports 1 when 0 were dropped;
  - the R1 experiment reports 3 when 1 was;
  - go-opentelemetry reports 28 when 25 were;
  - ripgrep `strict` reports 9 when 0 were.
- **Decision:**
  - The dropped set is computed after filtering: the PURLs of `enumerate_workspace_roots` that are not in the reachable set, sorted.
  - `ProjectDiscoveryReport` gains `dropped_main_modules: Vec<String>`, and `nested_projects_ignored` becomes its length. The INFO line and the docs page are then true.
  - `Copy` is dropped from the report's derives. Nothing outside the module copies it (grep).

## R4. Declared members (FR-003, Clarifications 1 and 2)

**The reader resolves; the filter matches.** The Haskell reader already parses `cabal.project` `packages:` (#1032, `parse_cabal_project_packages`) and discovers every `cabal.project`, `stack.yaml`, `package.yaml` and `*.cabal`.

- **What the reader supplies:**
  - A new `HaskellParseSummary` field, `root_declared_member_dirs: Vec<String>`. It holds the sorted, scan-root-relative, `/`-separated directories of the packages declared by a root `cabal.project` or root `stack.yaml`. It is never rendered.
  - The reader resolves globs against the package directories it discovered, so the filter matches directories exactly and does no globbing.
- **`cabal.project` resolution:**
  - Uses #1032's entry forms: a directory, a directory glob, or a `.cabal` path or glob, whose parent is the package directory.
  - Uses `literal_separator(true)`, as #1032 requires.
  - **Commas are separators.** hs-opentelemetry writes `, api` on each line. `parse_cabal_project_packages` keeps `,` as a token, which matches nothing and is harmless there. The new resolver strips commas.
  - #1032's own function and its tests are left as they are. The new resolver sits beside it and shares only the entry normalisation.
- **`stack.yaml`:**
  - `StackYaml` gains `packages: Option<Vec<String>>`.
  - The entries are directories; Stack does not glob them. An absent `packages:` means Stack's default, `[.]`, so the root package is the only member, and it declares nothing nested.
- **What counts as a root declaration:**
  - Only a `cabal.project` or `stack.yaml` whose parent is the scan root.
  - When both exist, the union. Haskell-language-server's two lists agree.
  - A declaration whose entries match no discovered package is treated as no declaration. #1032 uses the same rule ("more likely a shape this parser does not understand"), so the scan falls back rather than emptying itself.
- **The filter:**
  - `apply_scope_filter` takes `declared_member_dirs: &[String]`.
  - Under `root-only` only (`follows_workspace_members()`), a main module whose `root_dir` key is in that set joins `in_scope_roots` and is projected like a root.
  - Under `strict` the set is ignored (Clarification 2). So a root declaration alone gives `strict` no roots, and `strict` falls back as it does today.
  - With a declaration and no root main module, the declared modules are the roots. #1045's path then fires only when neither exists (Clarification 1).

## R5. The drop record: a new row, C244 (FR-004, Clarification 3)

**The standards-native audit (Principle V):**
- **CycloneDX 1.6:** `compositions[].aggregate` says whether an assembly is complete, and R6 already sets it, but it cannot name what scoping left out. `scope: excluded` would keep the dropped projects as components, which Clarification 3 rejected.
- **SPDX 2.3:** no equivalent.
- **SPDX 3.0.1:** no equivalent. `Sbom` carries no scope-exclusion list.
- **Finding: KEEP-NO-NATIVE.**

**Why not extend C140:** C140 is a string, `root-only`, and changing it to an object would break every consumer that compares it. "Extended compatibly" is not possible.

**Decision: C244 `waybill:project-discovery-dropped`**, document scope. Its value is a canonical JSON string, the convention C139 uses:

```json
{"count":2,"main-modules":["pkg:hackage/waybill-fixture-a@0.1.0","pkg:hackage/waybill-fixture-b@0.1.0"],"reason":"project-discovery-scope"}
```

- It is emitted only when `count > 0`, and only on a scan that filtered, the same branch on which C140 is emitted.
- Keys are sorted (`serde_json::Map`), and `main-modules` is sorted, so a golden can pin it.
- It has three extractors with `SymmetricEqual` parity, written in the same change, because the gate is in the lib target.

## R6. Completeness (FR-005)

**Decision: a new reason code**, `ReasonCode::ProjectDiscoveryScope { count, ecosystems }`. It renders as:

```
project-discovery-scope: <N> project(s) dropped from <eco, eco>
```

- The ecosystems are PURL types, sorted and deduplicated, and the detail contains no `;`.
- **How it reaches the pass:** the same way as milestone 1273's `ManifestUnread`. `graph_completeness::for_document` gains a parameter carrying the drop, which all four callers pass: the CycloneDX builder, SPDX 2.3, SPDX 3, and `orphan_reason`.
- **It fires on both paths:**
  - when the remaining set is empty (f1, f4: only the file-tier component is left, which the pass excludes), the result is `partial`, not `trivially_complete()`;
  - otherwise the code joins `reason_codes`, which already makes the value `partial`.
  - `partial` rather than `unknown`, because the gap is known and classified.
- **CycloneDX `compositions`:** `degraded_ecosystems` (`cyclonedx/compositions.rs`) gains an explicit arm for the new code, marking its ecosystems incomplete. Without it, the `_ => {}` arm would let `compositions[]` say `complete` while C104 says `partial`. That is #892's shape, which the m1273 arm's comment warns about.
- **The vocabulary:** C105's row lists a closed vocabulary of 9 codes and omits m1273's `manifest-unread`. The row is corrected to 11 codes, and the CHANGELOG records the addition.

## R7. C200 (FR-006, Clarification 4)

- **Where C200 is built:** `HpackReadOutcome::to_document_value` renders it inside the reader, before the filter runs, into `HaskellParseSummary.hpack_read`.
- **Decision:** after the filter, `scan_cmd` amends the rendered value through a helper in `haskell/hpack.rs`, `amend_scope_dropped(value, dropped_manifests)`.
  - It adds `manifests-dropped-by-scope`: the sorted entries of `manifests-read` that belong to a dropped `pkg:hackage` main module. It leaves `manifests-read` unchanged.
  - It is inserted only when non-empty, so every existing C200 is byte-identical.
- **How entries are matched:** a dropped main module's evidence path is scan-root-relative (`local-packages/a/package.yaml`), while `manifests-read` holds the reader's joined path. The helper matches the entry equal to the reader root joined with the evidence path.
  - The test runs on a tree reached through a symlink, so a canonicalised and a raw root cannot silently disagree.
- **Parity:** C200's extractor is `SymmetricEqual` over the whole value, so the new key is covered as it stands. The row text gains the key.

## R8. What stays the same (FR-007)

- **`all`:** `apply_scope_filter` returns at its first line. The other changes are reached only from the filtered branch, or are new keys emitted only there (C244, the reason code, the C200 key). The haskell reader's new field is never rendered.
- **`--split`:** `split.rs` is not edited.
- **#1045:** the early return is unchanged. It fires when, after R1 and R4, `in_scope_roots` is empty.
- **The corpus:** no target is scanned with `--project-discovery` (`corpus_harness_195/manifest.rs`), so every golden is byte-identical. A verification corpus dispatch is still run, because the corpus golden tests are ignored locally.

## R9. Test and mutation plan (SC-001 to SC-006)

- **Integration tests** in `waybill-cli/tests/project_discovery_haskell.rs`, built on the six trees from `make-fixtures.sh`, written as Rust fixtures under `tests/fixtures/project_discovery_haskell/`. All three formats, plus `waybill sbom parity-check`. A tree reached through a symlink is added for R7.
- **Unit tests:**
  - `root_dir`;
  - the declared-member resolver: the `cabal.project` forms, commas, `stack.yaml` with and without `packages:`, and matching nothing;
  - the dropped set and the corrected count;
  - the reason code's rendering;
  - `amend_scope_dropped`.
- **Mutations, each of which must fail a test:**
  - `root_dir` returns `source_dir`;
  - declared roots ignored, for the `cabal.project` and `stack.yaml` sources separately;
  - declared roots applied under `strict`;
  - the drop set left empty;
  - the reason code not pushed, on each of its two paths;
  - the compositions arm removed;
  - the C200 key not inserted.
- **The positive control (SC-003):** f1 under `root-only` must carry C244, with count 2.
- **SC-005:** the corpus rows in `baseline.md` are rerun with the built binary, and the "after" table is committed beside them.
