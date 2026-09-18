# Teeth-check (T030)

The pre-change `waybill 0.9.0` release binary was preserved before any edit and
re-run over every fixture the new tests use:

| Fixture | documents emitted | documents carrying an identity |
|---|---|---|
| `pants_discovered_resolves` | 2 | **0** |
| `pants_resolve_edges` (declared) | 2 | **0** |
| `pants_coursier_jvm/multi_resolve` (JVM) | 3 | **0** |
| `pants_namespace_collision` | 2 | **0** |

The annotation did not exist, so every assertion of its *presence* fails
pre-change for its own reason rather than incidentally.

## Which tests are proofs, and which are guards

A test that passes on both sides of the change is a guard. It is worth having
and it is not evidence the feature works, so it is labelled rather than
counted.

### Proofs — fail against the pre-change binary (8)

| Test | What it proves |
|---|---|
| `each_discovered_resolve_document_states_its_own_resolve` | SC-001, SC-002 |
| `renaming_a_document_does_not_change_the_answer` | SC-003 / FR-003 |
| `a_jvm_repository_identifies_its_resolves_too` | R3 — the whole JVM ecosystem |
| `same_name_in_two_namespaces_is_distinguishable` | SC-002a / FR-001a / C-6 |
| `where_a_root_also_names_the_resolve_the_two_agree` | SC-005 / C-3 |
| `one_reading_procedure_answers_for_every_provenance` | SC-004 / FR-004 |
| `the_identity_decodes_identically_in_all_three_formats` | C-1 cross-format |
| `cyclonedx_carries_a_string_and_spdx_carries_an_array` | the encoding split is real |

### Guards — pass on both sides (7)

| Test | What it protects |
|---|---|
| `no_document_gains_a_component` | **SC-007 / FR-006 / C-5** — the one that catches an invented anchor |
| `the_repository_wide_ownership_statement_stays_repository_wide` | FR-007 / SC-006 — rejects narrowing C161 instead of adding C163 |
| `a_discovered_resolve_document_still_has_no_invented_root` | FR-006 from the root side |
| `a_workspace_split_document_carries_no_resolve_identity` | FR-008 |
| `a_directory_split_document_carries_no_resolve_identity` | FR-008 |
| `an_unsplit_document_carries_no_resolve_identity` | FR-008 + SC-009 |
| `the_collision_fixture_needs_its_third_resolve_to_split_at_all` | keeps the #919 reproducer reproducing |

The two most valuable entries in this document are in the guard column, not
the proof column: `no_document_gains_a_component` is the only thing that
catches the easy wrong implementation, and the ownership guard is the only
thing that rejects the obvious wrong fix for the feature as a whole.

## A vacuous pass that was caught and fixed

The first `--split=workspace` / `--split=directory` absence check ran against a
Pants fixture with no workspace boundaries. The split emitted **no documents at
all**, so "no document carries an identity" was true and meaningless. Both
tests now run against `pants_go`, which produces three workspace documents, and
both assert the document set is non-empty before checking anything about it.

## A finding about #919, from building its reproducer

With **only** the two same-named resolves, `--split=resolve` sees one group,
decides the repository is not partitionable, and emits a single unsplit SBOM.
The #919 merge is invisible behind the degenerate-split fallback. The fixture
carries a third, uncollided resolve (`lint`) specifically so the split runs at
all — and `the_collision_fixture_needs_its_third_resolve_to_split_at_all` pins
that, so a future tidy-up cannot silently remove the thing that makes the bug
observable.

With the third resolve present, the merged document states both identities and
its component list shows the defect plainly — a maven jar and a pypi wheel
inside one "resolve":

```
default.generic.cdx.json
  identity   = ["jvm:default","python:default"]
  components = [pkg:maven/dev.waybill.fixture/waybill-fixture-jvmside@1.0.0,
                pkg:pypi/waybill-fixture-pyside@1.0.0]
```

C163 makes #919 legible in the output for the first time.
