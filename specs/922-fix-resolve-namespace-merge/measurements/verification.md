# Teeth-check (T030)

The pre-change `waybill 0.9.0` release binary was preserved before any edit
(T002) and re-run over both collision fixtures.

| | pre-change | after |
|---|---|---|
| `pants_namespace_collision` documents | **2** — merged `default` + `lint` | **3** — `python-default`, `jvm-default`, `lint` |
| …namespace-qualified filenames | **0** | 2 |
| `pants_namespace_collision_only` documents | **0** — fallback fired, no split at all | **2** |
| components carrying C164 | **0** | every component with membership |

## Proofs — fail against the pre-change binary

| Test | What it proves |
|---|---|
| `same_name_in_two_namespaces_yields_two_documents` | SC-001, SC-002 |
| `colliding_documents_have_distinct_filenames_and_manifest_entries` | SC-004a / FR-002a |
| `a_collision_only_repository_still_splits` | SC-004 / FR-003 — pre-change this fixture produced **nothing** |
| `same_name_in_two_namespaces_is_distinguishable` (m912, inverted) | one identity per document |
| `an_unsplit_document_partitions_into_two_namespaces` | SC-006 |
| `generic_purl_members_of_a_python_resolve_read_python` | C-2 |
| `the_namespace_decodes_identically_in_all_three_formats` | SC-009 |
| `every_component_with_membership_has_exactly_one_namespace` | C-1 |

## Guards — pass on both sides

| Test | What it protects |
|---|---|
| `a_genuinely_single_resolve_repository_still_falls_back` | **FR-009.** The obvious wrong fix for US3 is deleting the `groups.len() <= 1` check, which passes US3 and turns every single-resolve repository into a one-document "split" it never asked for |
| `membership_keeps_its_v090_key_and_bare_name_shape` | **FR-006a / SC-007.** The additive guarantee the whole design rests on |
| `the_repository_wide_ownership_statement_stays_repository_wide` (m912) | C161 unchanged |
| `no_document_gains_a_component` (m912) | FR-006 of m912 — no invented anchors |
| the three m912 absence tests | identity absent outside per-resolve splits |

The two most valuable entries are guards, not proofs. `a_genuinely_single_
resolve_repository_still_falls_back` is the only thing standing between US3
and an implementation that "fixes" it by deleting the degenerate-split check,
and `membership_keeps_its_v090_key_and_bare_name_shape` is the only check on
the promise that made this design preferable to qualifying C143 in place.

## Two defects found during implementation, neither by the tests meant to cover them

**Only the JVM side got qualified.** A declared resolve has an m868 anchor and
the naming root was taken from that anchor's PURL — the bare name. So the
declared side stayed `default.generic.cdx.json` while the unanchored side
became `jvm-default.…`. Distinct, asymmetric, and the bare one
indistinguishable from a repository with no collision. The distinctness test
passed, because the names *are* distinct. Found by diffing filenames against
the T001 baseline.

**The C163 identity vanished.** Milestone 912 derived it by looking the group
key up in a document-scope index, which worked only while the key was the bare
resolve name. Changing the key to the slug broke it silently — no error, just
an absent field. Found by running m912's own suite, which is the argument for
running neighbouring milestones' tests rather than only the new ones.
