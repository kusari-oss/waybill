# Research: graph completeness counts a partial closure read

Read against `main` at 1f85936f on 2026-11-25. Each section ends with what it decides.

## R1: One predicate, from typed counts

**Today.** The "closure read was partial" rule exists twice:
- `ScanArtifacts::closure_read_partial()` (`generate/mod.rs:586`), which the SPDX emitters read;
- an inline copy in the CycloneDX builder (`cyclonedx/builder.rs:~1118`).

Both read `read_outcome.has_gap()` and a `nix_closure_contents_unread: bool`. The bool is all the emitters receive about contents: `ScanArtifacts` and the builder hold the rendered C213 string and that flag, nothing else. The counts behind the flag (`ContentsRecord::absent + partial + failed`, and `members`) stop in `scan_cmd`'s `ClosureContentsSummary` (`scan_cmd.rs:7076`).

**Decision.**
- **Replace the bool with typed counts.** `nix_closure_contents_unread: bool` becomes `Option<ContentsUnread { unread, members }>`, built once from `ContentsRecord` where `has_unread()` was. That changes 14 sites: `scan_cmd`, `ScanArtifacts`, five struct literals that pass `false`, the builder's field and setter, `cyclonedx/mod.rs`, and the test at `scan_cmd.rs:12151`.
- **Add one free function.** `closure_read_gaps(read_outcome, contents) -> ClosureReadGaps` lives beside the graph-completeness codes.
  - `ScanArtifacts::closure_read_partial()` becomes `closure_read_gaps(...).any()`.
  - The builder's inline copy calls the same function.
  - The new reason step reads the same value.

  FR-003's equivalence then holds by construction, and its test pins it.

**Alternatives.**
- **Parse the counts back out of the rendered C213 string.** Rejected: the builder's own comment exists to avoid exactly that ("withhold a completeness claim without parsing it back").
- **Add a counts field beside the bool.** Rejected: two sources for one fact can drift, which is the defect this feature fixes.

## R2: Where the codes are added, and who passes the input

**Today.** `graph_completeness::for_document` is called in four places:
- the CycloneDX builder;
- SPDX 2.3 (`spdx/document.rs:707`);
- SPDX 3 (`spdx/v3_document.rs:684`);
- `orphan_reason.rs:796`.

The three format emitters pass spec 1652's scope-drop input. `orphan_reason` passes `None`, because "a scope drop changes reason codes, never the reachable set", and per-component orphan status depends only on the reachable set.

**Decision.**
- **A post-pass.** `apply_closure_read(result, gaps)` mirrors `apply_scope_drop`: it appends the codes after classification and after any `project-discovery-scope` clause, and it follows the same value rule. Codes present means `partial`, except that an empty set already `unknown` stays `unknown`.
- **A new parameter.** `for_document` gains `closure_read: Option<&ClosureReadGaps>`. The three format emitters pass theirs; `orphan_reason` passes `None` for the same reason as before. The spec's FR-005 is corrected to say so.
- **A test for the orphan pass.** It asserts that `orphan_reason`'s per-component output is unchanged on a closure fixture with a gap.

**Alternative.** Threading the counts through `compute_graph_completeness` itself. Rejected: as with the scope drop, these codes change the verdict, never reachability, so the classifier's invariants are not their business.

## R3: No ecosystem is degraded

`cyclonedx/compositions.rs::degraded_ecosystems` maps reason codes to ecosystems. Milestone 1278 deliberately named no ecosystem for an unconsulted declaration, because which ecosystem it would have touched is unknown.

**Decision.** Both new codes map to no ecosystem.
- If that `match` is exhaustive, the new arms are written out explicitly as empty.
- If it has a wildcard, a unit test asserts both codes yield the empty set. A future wildcard change cannot then silently degrade an ecosystem.

## R4: Fixtures for the controls

- **Declaration gap:** `closure_structured_attrs.rs` (milestone 1278). Its `CLOSURE_JSON` has an unconsulted declaration, which the existing test asserts as a control (`s.read_outcome().is_some()`).
- **Contents gap only:** the stub fixture `tests/fixtures/nix_closure_boot_versions`, where 4 of 4 members are absent and there is no read row (spec 1655's `stub-before.md`).
- **Neither gap:** `closure_binary_contents.rs::populated_store()` serves members' outputs from a temporary store. Whether any of its scans read every member, and so has no contents gap, is measured at T001 through the project's own harness. If none does, one test there adds the missing outputs. This is the positive control that `complete` survives (spec acceptance 3).

## R5: Expected golden effect, recorded before the dispatch

From `measurements/read-gaps-before.txt`, each closure golden's C105 gains, appended in this order:

| target | appended clauses |
|---|---|
| moat | `closure-declarations-unconsulted: 211 declaration(s) in 16 derivation(s); closure-contents-unread: 1087 of 1590 member(s)` |
| openai | `…: 198 declaration(s) in 16 derivation(s); closure-contents-unread: 471 of 1330 member(s)` |
| shellcheck | `…: 211 … in 16 …; closure-contents-unread: 1284 of 2045 member(s)`. C104 goes from `complete` to `partial`, and these become its only clauses. |
| slack-web | `…: 211 … in 16 …; closure-contents-unread: 416 of 1193 member(s)` |
| slacklinker | `…: 228 … in 17 …; closure-contents-unread: 1153 of 2351 member(s)` |

- **Formats:** C104 and C105 are written in all three formats, so 15 files change.
- **Everything else stays the same:** no other key changes, `compositions[]` and SPDX `completeness` are byte-identical, and no non-closure golden changes.
- **Scoring:** spec 1278's FR-016 practice applies. These expectations are scored against the diff afterwards in `measurements/after.md`.
