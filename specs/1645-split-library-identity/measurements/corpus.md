# T017: public corpus at the final code commit

## Run 45082098704, at `cd88e02b`

- **Dispatch**: `public-corpus.yml` with `-f branch=1645-split-library-identity`.
- **Commit**: the summary reads `Scanned commit: cd88e02b…`, the final code commit.
- **Result**: 165 passed, **1 failed**: `nix-closure-openai`, drifting in all three formats. That target was T010's predicted `cpe_form_change`.

### Every difference, traced

The run's `corpus-emitted-sboms` `.actual` documents were compared with the committed goldens by plain `diff`:

| Format | Lines removed | Lines added | Content |
|---|---|---|---|
| CycloneDX | 5 | 5 | `pkg:generic/ncurses@6.4.20221231` changes `cpe`, the `evidence.identity` `concludedValue`, the `waybill:cpe-candidates` value and the two secondary CPE properties, each from `…:ncurses:6.4.20221231:*:…` to `…:ncurses:6.4:20221231:…` |
| SPDX 2.3 | 4 | 4 | the same component: its three `cpe23Type` locators and the `waybill:cpe-candidates` annotation |
| SPDX 3 | 4 | 4 | the same component: its three `cpe23` external identifiers and the `waybill:cpe-candidates` annotation |

No component is added or removed, no PURL or version changes, and no other field changes. The component is the closure tier's own `ncurses@6.4.20221231`, whose CPEs come from the curated table through `synthesize_cpes` (`nix/closure/emit.rs:754`). The cause is FR-008, which applies whichever reader produced the component (clarification 2).

**Judgement**: intended. The new CPEs have NVD's form for ncurses records: the release in the version field and the patch date in update. 81 of 110 entries put the date in update, and none put it in version (`nvd-version-forms-2026-11-22.txt`).

No other target changed. Specifically, no target outside TheRock has a curated LLVM or ncurses banner, or a merge (census T010).

## Regeneration, run 45083655380 at `c7d556ea`

- **Dispatch**: `-f branch=1645-split-library-identity -f regen_goldens=true`.
- **Commit**: `Scanned commit: c7d556ea…`. Its code differs from `cd88e02b` only in the module's doc comment.
- **Result**: `166 passed; 0 failed`. The summary lists exactly 3 changed goldens: `nix-closure-openai` in each of the three formats.
- **Copied into the tree**: all files from `corpus-goldens-regen`. `git status` shows exactly those 3 modified, and each is byte-identical (`cmp`) to the `.actual` document traced above.

## Confirmation, run 45085758825 at `4953c559`

- **Dispatch**: `-f branch=1645-split-library-identity`, without regeneration, against the committed goldens.
- **Commit**: `Scanned commit: 4953c559…`, the commit with the regenerated goldens.
- **Result**: `test result: ok. 166 passed; 0 failed`.
