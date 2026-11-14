# Golden review (FR-008 to FR-010, SC-003, SC-004, SC-006)

## Source

- **The goldens:** public-corpus run 43637780760, `regen_goldens=true`, on branch `1503-fragment-positive-evidence` at `726ae23d`.
- **An earlier run, 43633286041,** on the same commit, hit the job's 45-minute timeout. `nix-closure-slacklinker` never finished there: it ran for more than 39 minutes, against 9–13 minutes in each of the six runs before it.
  - On the re-run, slacklinker finished in 8 minutes.
  - Locally, with a stub `nix` serving the captured closure and a runtime closure, both the base binary and this branch scan slacklinker in 0.69 s, and both make identical nix calls.
  - The hang is therefore a property of that CI run, not of this change.
  - The other four targets' goldens from that run are byte-identical to this run's, so the regeneration is deterministic across runs.

## Completeness (SC-006)

- The artifact `corpus-goldens-regen` holds 67 files, the same set as `waybill-cli/tests/fixtures/public_corpus/`, with nothing missing in either direction.
- **Exactly 15 files differ:** CycloneDX, SPDX 2.3 and SPDX 3 for each of the five `nix-closure-*` targets.
- The other 49 files, and the three closure targets' `recall-truth.json`, are byte-identical.

## Method

The probe is `probe/golden_review.py`. It reads only the old and new documents, and its null control (the committed moat golden against itself) reports 0 changes. Per target, on CycloneDX, it checks:
- **Components**, recursively, nested ones included, and the same `bom-ref` and PURL sets before and after (R4: identity unchanged).
- **Changed fields.** Every component whose fields changed must be a reclassified member. Its only changed properties may be `waybill:closure-member-role` and `waybill:orphan-reason`, and no non-property field may change.
- **Edges** (`dependencies[].dependsOn`). Every added edge must touch a reclassified component. Every removed edge `a → c` must either touch one, or be replaced in the new graph by a path `a → r … → c` through a reclassified component `r`. Anything else counts as unexplained (SC-003).
- **C184.** `edges` must equal `edge-accounting-after.txt`'s shipped figure. `member-roles` and `edge-ineligible-components` must equal the document's own counts (analysis C1).

FR-008 is checked separately with `waybill sbom parity-check` on the old and new triples.

## Result

| target | → `scaffolding` | → `package` | edges ± (doc) | added, incident | removed, incident + replaced | unexplained | C184 `edges` (= accounting) | `edge-ineligible` | `identifiers.wanted` | `identifiers.received` |
|---|---|---|---|---|---|---|---|---|---|---|
| moat | 10 | 15 | +611 / −13 | 611 | 0 + 13 | 0 | 6,891 → **7,489** | 507 → 492 | 1,242 → 1,257 | 326 → 327 |
| shellcheck | 11 | 15 | +1,063 / −28 | 1,063 | 15 + 13 | 0 | 10,224 → **11,274** | 508 → 493 | 1,701 → 1,716 | 527 → 528 |
| slack-web | 3 | 63 | +3,566 / −39,036 | 3,566 | 0 + 39,036 | 0 | 44,426 → **8,956** | 74 → 11 | 1,219 → 1,282 | 444 → 444 |
| openai | 3 | 64 | +3,595 / −42,453 | 3,595 | 0 + 42,453 | 0 | 48,336 → **9,478** | 75 → 11 | 1,373 → 1,437 | 450 → 450 |
| slacklinker | 15 | 20 | +1,289 / −42 | 1,289 | 0 + 42 | 0 | 12,169 → **13,416** | 517 → 497 | 2,001 → 2,021 | 595 → 596 |

The `→` columns count components. Members that merge by identity are counted once, so slack-web's 63 `package` components are its 63 derivations (36 setup-hook packages and 27 defaults).

On every target:
- **Identity.** The `bom-ref` and PURL sets are identical before and after. No component's non-property fields changed.
- **Properties.** The role changes on exactly the reclassified members. `waybill:orphan-reason` changes on exactly the members that became `package`: they lose `closure-member-not-an-edge-endpoint` because they are endpoints now. Members that became `scaffolding` keep it.
- **Edges.** 0 are unexplained (SC-003).
- **C184.** `edges` equals the measured shipped figure on all five. `member-roles` and `edge-ineligible-components` equal the document's own counts. `edge-ineligible` is `scaffolding` plus `fragment`.
- **Parity (FR-008).** Unchanged. 239 of 240 universal rows pass before and after, C206 among them. The one failure is A5, before and after, so it pre-exists this change, as spec 1492's review found.

## SC-004: edges per spec-1492 patch, and whether the `stdenv-linux` fan-out remains

A patch member's incident edges in the CycloneDX document, before → after:

| target | patch members | before | after |
|---|---|---|---|
| openai | 41 | 40 of them 39–42; `bzip2-1.0.6.2-autoconfiscated.patch` 1 | 40 of them 3–7; that one 1 |
| slack-web | 46 | 45 of them 39–42; the same `bzip2` patch 1 | 45 of them 3–7; that one 1 |
| moat | 61 | 1–6 | 1–7; 36 rise by 1, 25 unchanged |

The count is every `source` component named `*.patch`. The single-edge `bzip2` patch has only its consumer edge in both versions.

**The fan-out is gone.** A patch on openai or slack-web now has the 3–7 edges a patch has on moat. Those ~38 extra edges per patch came from walking through `stdenv-linux`, which is now a `package` endpoint. On moat, 36 patches gain one edge each, to the new `package` endpoint `mirrors-list`, which every `fetchurl` consumes.

## Explained differences beyond FR-007's list

1. **Shipped edges fall on openai (48,336 → 9,478) and slack-web (44,426 → 8,956).** 42,453 and 39,036 edges are removed. Every one is replaced by a path through a reclassified member, almost all of them through `stdenv-linux` or a bootstrap stage.
   - Each consumer used to carry an edge to every toolchain member reached through `stdenv-linux`.
   - Now it carries one edge to `stdenv-linux`, and `stdenv-linux` carries the toolchain edges once.
   - Reachability is preserved. This is the reshaping, not a loss.
2. **Shipped edges rise on moat (+598), shellcheck (+1,050) and slacklinker (+1,247).** Consumers of `nuke-refs`, `expand-response-params`, `mirrors-list` and the other new `package` members now name them, instead of walking through to what built them.
3. **shellcheck removes 15 edges incident to reclassified members.** Its subject fans out to every non-endpoint member, as spec 1492's review recorded. The 15 members that became `package` are endpoints with real consumers now, so they leave that fan-out.
4. **C184 `identifiers.wanted` and `licenses.wanted` rise by the number of new `package` components.** The nixpkgs-metadata population is the emitted endpoints plus the evaluated package (`coverage_drv_keys`), and a new endpoint joins it, as in spec 1492.
5. **`identifiers.received` and `licenses.received` rise by 1 on moat, shellcheck and slacklinker.** One `expand-response-params` member already carried its MIT license as a `fragment` (unchanged byte for byte). As a `package` it is in the population, so the match it already had is now counted. No component's license or identifier changed.
