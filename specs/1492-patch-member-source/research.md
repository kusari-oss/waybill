# Research: Locally built patches are sources, not fragments

All observations are on the pinned x86_64-linux `nix-closure-moat` closure:
- 1,749 derivations;
- captured in public-corpus run 43535531153;
- sha256 `898fcdd38aa235840cc2a76b9ee4048ef826a853ce8abe19efdfb3076c8744ab`.

They were read through the reader's own code (`waybill debug closure-dump`, and a throwaway test calling `member::classify` with `PatchOutputs::of`, since removed), never through a re-model.

## R1. The population

**Decision:** The feature moves exactly the members FR-001 names. On the pinned closure these are 5 derivations with 4 names.

**Measured:**

| C206 role (no-`pname` members) | members | listed in another derivation's `patches` |
|---|---|---|
| `source` | 553 | 0 |
| `package` | 23 | 0 |
| `check` | 46 | 0 |
| `scaffolding` | 267 | 0 |
| `fragment` | 245 | **5** |

The 5 are `001-fix-rpath.patch`, `catalog-legacy-uris.patch` (×2, distinct recipes, same `src`), `fix-build-time-run-tcl.patch` and `remove-hatch-plugins.patch`. All 5 have `dontUnpack` and a local `.patch` as `src`.

The `source` row shows 0 listed members even though 56 patch-named members are `source`. The reason is in the input, measured with `jq` over the captured JSON:
- **The fetched patches:** all 56 are fixed-output derivations (`outputs.*.hash`) with **no** `outputs.*.path`. `patches.lists`, which matches on output path, therefore cannot see them.
- **The local patches:** all 5 have a path and no hash.

This does not affect the feature. The 56 are `source` through the content-hash rung, which runs first. It does mean the `patches` evidence reaches only locally built patches in this format. A test must therefore not assume it also covers fetched ones.

**Alternatives considered:**
- **Match on the name (`*.patch`, `*.diff`).** Rejected. A name is what spec 1380 moved away from, and the closure records the fact directly.

## R2. The endpoint helper ignores the patch set, and this feature makes that wrong

**Decision:** `member::is_edge_endpoint` takes the closure's `PatchOutputs`. Both callers build that set once per closure with `PatchOutputs::of` and pass it:
- the projection's `graph::stops`, through `project` and `nearest_stopping`;
- `emit::emitted_drv_keys`.

**Rationale:** `is_edge_endpoint(drv)` calls `classify(drv, &PatchOutputs::default())`, an empty patch set. Its doc comment justifies that by saying the patch set is consulted only inside the version-name rung, whose outcomes are all endpoints. `s1380_the_patch_set_never_changes_the_endpoint_answer` pins that. This feature makes the patch set decide between `fragment` (not an endpoint) and `source` (an endpoint).

Left as it is, the classifier would emit `source` while the projection and the emitter still treat the member as walk-through. C206 would then say `source` on a member that is in no edge. That is the role/graph disagreement milestone 1280 was built to prevent, and it would also leave the `not-an-edge-endpoint` orphan reason on a member whose role is an endpoint.

**Alternatives considered:**
- **A second `is_patch_listed` check in `stops` and `emitted_drv_keys`.** Rejected. It is a second copy of the rule, and `emitted_drv_keys`' doc comment records why one copy is the requirement.
- **Precomputing the role per derivation and passing the map.** That is a larger refactor than the change needs. `PatchOutputs::of` is one pass over the closure.

## R3. Rung order (clarified)

**Decision:** The order is content hash → listed in `patches` → `env.text` → version-shaped name (spec 1380's split, minus its `patches` clause, which now runs earlier) → `fragment` (FR-010).

**Rationale:** This is the clarify answer (A). On the pinned closure it places nothing differently from the fallback-only alternative, because 0 `scaffolding` members are listed (R1).

## R4. Tests that pin spec 1380's edge-neutrality

The change deliberately reverses three spec 1380 tests in `member.rs`. Each is rewritten, not deleted, so that its control survives:

| Test | Today | After |
|---|---|---|
| `s1380_the_patch_set_never_changes_the_endpoint_answer` | the helper without the patch set agrees with `classify` with it | **replaced**: the helper *with* the patch set agrees with `classify` for every member, and a listed bare-named patch is a case where the patch set changes the answer, which is the control |
| `s1380_only_package_members_move_and_only_between_endpoints` | every move is from `package`, between endpoints | the moves are from `package`, or from `fragment` to `source` for a listed member; only the latter changes endpoint status |
| `s1380_source_trees_and_patches_are_source` | asserts `q.drv` (a listed patch with no version) is `fragment` ("R5") | asserts `source`, with an unlisted twin that stays `fragment` |

## R5. Downstream consumers of the role

| Consumer | Effect | Action |
|---|---|---|
| C184 `member-roles`, `edge-ineligible-components`, `edges` | counts change | none: read back from the emitted document (`summary.rs:267`) |
| `waybill:orphan-reason` `closure-member-not-an-edge-endpoint` | decided from the emitted C206 value (`orphan_reason.rs:659`); a reclassified member with an incoming edge is no longer an orphan, and one nothing reaches gets the C182 arm's reason | none |
| identity / PURL | a member with no content hash keeps its recipe identity whatever its role (spec 1380 FR-003) | none: no wire identity change |
| `debug closure-dump` | already classifies with `PatchOutputs::of` (`debug_cmd.rs:105`); `edge_endpoint` is derived from the role | none |
| lifecycle scope | closure edges carry `data_type: nix-build-graph`, which `scope_propagation` declines to traverse (`graph.rs`) | none |

## R6. Other closure targets

**Decision:** Regenerate every closure golden. Review each diff against FR-008 and SC-004.

**Observed** in the committed goldens, counting `fragment` components whose name ends `.patch` or `.diff`:

| target | patch-named fragments |
|---|---|
| moat | 5 |
| openai | 5 |
| shellcheck | 5 |
| slack-web | 6 |
| slacklinker | 6 |

This is a name match, not the `patches` evidence, so it bounds the review rather than predicting it. The CI regeneration is the measurement. Those closures are not captured, and evaluating them locally is out of bounds.

## R7. The edge delta

**Decision:** No figure is quoted before measurement (CLAUDE.md, "Measure external behaviour before designing around it"). The task runs `s1367_edge_accounting_over_a_real_closure` on the captured moat closure before and after the change, and diffs the moat golden's dependency edges. Every edge in the symmetric difference must have a reclassified member as an endpoint, or must replace a walk through one (SC-003).
