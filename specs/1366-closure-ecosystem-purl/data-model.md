# Phase 1 data model: an ecosystem identity for closure components

**Feature**: `1366-closure-ecosystem-purl` | **Issue**: #1365

Three entities, all in-process for the duration of one scan. Nothing is
persisted by this feature; the only durable outputs are the emitted document's
C207 and C208 values, and the deps.dev disk-cache entries the confirmation
already shares with enrichment (research R10).

---

## 1. `EcosystemCandidate`

A closure component the recorded evidence suggests is a published package.

| field | type | source | notes |
| --- | --- | --- | --- |
| `component_index` | `usize` | position in the merged `Vec<ResolvedComponent>` | the pass mutates in place; no clone of the component set |
| `ecosystem` | `&'static str` | the candidate rule | `"pypi"` for every candidate this milestone produces |
| `declared_name` | `String` | `attr_str("pname")` | **both carriers** — `env` and top-level `structuredAttrs` (research R3) |
| `normalised_name` | `String` | `pip::normalize_pypi_name_for_purl` | PEP 503; FR-004 |
| `version` | `String` | `attr_str("version")` | verbatim from the closure; never rewritten |
| `evidence` | `CandidateEvidence` | the candidate rule | what suggested it — carried into C207 |

### `CandidateEvidence`

Closed enum. One variant this milestone; the type exists so a second ecosystem
is an added variant rather than a reshaped field.

| variant | meaning |
| --- | --- |
| `VersionedInterpreterPrefix` | the derivation `name` matched `^python3\.\d+-` and `pname` did not itself begin `python3` |

### Validation rules

- **V1** — `declared_name` non-empty. A derivation with no `pname` in either
  carrier is not a candidate; it is milestone 1282's population.
- **V2** — `version` non-empty and not a placeholder. `EnrichmentKey::new`
  (`request_key.rs:47`) already rejects both, so a candidate failing V2 can
  form no key and is declined as `Unchecked { reason: Unkeyable }` rather than
  silently dropped — a dropped candidate is invisible to FR-008's counts.
- **V3** — the component must carry **no** `waybill:closure-member-role`
  (C206). FR-005 protects the 539 members absolutely; a member is never a
  candidate, and this is asserted rather than assumed from the candidate rule.
- **V4** — the component must have been **appended** by the closure merge, not
  merged into an existing one. A merged component belongs to another reader
  (FR-006) and already carries that reader's identifier.
- **V5** — after normalisation, `(ecosystem, normalised_name, version)` must be
  unique across the candidate set. A collision declines **every** colliding
  candidate (research R8); it never picks a winner.

---

## 2. `ConfirmationOutcome`

What the enrichment resolution path concluded about one candidate. A direct
projection of `LookupResult` (`depsdev_source.rs:50-60`), which is why the
mapping is total and has no default arm.

| variant | from `LookupResult` | effect on the identifier | FR |
| --- | --- | --- | --- |
| `Confirmed` | `Found(_)` | replace with `pkg:<ecosystem>/<normalised_name>@<version>` | FR-002 |
| `Declined` | `Absent` | **unchanged** — today's identifier | FR-002, FR-003 |
| `Unchecked(UncheckedReason)` | `Failed`, `Unqueried` | **unchanged** | FR-009, FR-012 |

### `UncheckedReason`

Closed enum; the value that reaches C208's `reason` key.

| variant | cause |
| --- | --- |
| `Disabled` | `--no-deps-dev` / `--no-deps-dev-license`. **The corpus path** (research R7) |
| `Offline` | `--offline` with no disk-cache hit. Unreachable in practice — the closure tier refuses `--offline` at `closure::admission` (`closure/mod.rs:315`) — and present so the state is representable rather than assumed away |
| `Unreachable` | transport error |
| `Throttled` | `deps_dev_client::Throttled` |
| `Unkeyable` | V2: no key could be formed |
| `Collision` | V5: two candidates normalised to one coordinate |

**State transitions**: none. An outcome is computed once per candidate per
scan and never revised. There is no retry: a `Failed` candidate keeps today's
identifier, which FR-009 names as correct behaviour rather than a fallback.

---

## 3. `ConfirmationRecord`

The document-scope summary. One per scan; the C208 value.

| field | type | notes |
| --- | --- | --- |
| `schema` | `&'static str` | `"waybill-closure-ecosystem-identity/v1"` |
| `candidates` | `usize` | before confirmation. **Zero is meaningful** — it distinguishes a closure with no Python packages from a step that did nothing (FR-008) |
| `confirmed` | `usize` | |
| `declined` | `usize` | |
| `unchecked` | `usize` | |
| `reason` | `Option<UncheckedReason>` | present iff `unchecked > 0`; the dominant reason. Mirrors C190's `{"reason": "<code>"}` shape |

**Invariant**: `confirmed + declined + unchecked == candidates`. Asserted in a
test, not only in a comment — an arithmetic identity that holds by construction
is the cheapest thing to break in a later edit.

**Emitted when**: `--nix-closure` ran and produced a closure, i.e. wherever
C204 is emitted. Absent otherwise, so its presence alone states that the pass
was reached.

---

## Relationship to existing entities

```text
RawDerivation ──(candidate rule, R3)──▶ EcosystemCandidate
                                               │
                              EnrichmentKey ◀──┤ (system, name, version)
                                    │          │
                        DepsDevSource          │
                       (cache→disk→net)        │
                                    │          │
                          LookupResult ────────▶ ConfirmationOutcome
                                                       │
                     ┌─────────────────────────────────┼──────────────────┐
                     ▼                                 ▼                  ▼
          ResolvedComponent.purl              C207 (per-component)   ConfirmationRecord
           (mutated iff Confirmed)             CDX: evidence.identity      │
                                               SPDX: annotation            ▼
                                                                      C208 (doc scope)
```

**Not touched**: `ClassifiedClosure`, `MemberRole`, `DerivationRole`,
`emits_as_component`, `merge_closure_components`, and every component the merge
reported as `merged`. The pass reads the closure's classification; it does not
reclassify.
