# Phase 0 — Research

Every claim here was re-verified against the tree at `d9c4f21e` rather than
carried over from the issue. Where a figure appears, the command that produced
it is given.

---

## R1 — Performance: the deferred question answers itself

**Deferred from clarification as "set a measured budget rather than assume it
is free."** It is free, and the reason is structural rather than empirical.

Resolution today, at `waybill-cli/src/scan_fs/mod.rs:952-954`:

```rust
for dep_name in &entry.depends {
    let key = (ecosystem.clone(), normalize_dep_name(&ecosystem, dep_name));
    if let Some(to) = name_to_purl.get(&key) {
```

`ecosystem` is derived from the requirer's PURL. This feature substitutes a
different **value** into that key. It remains exactly one `HashMap::get` per
declared dependency — the same number of lookups, the same complexity, no
iteration introduced.

The expensive shape does exist in this file, and it belongs to the opt-in
inference capability: on a miss it walks `name_to_purl.iter()` per candidate
ecosystem per dep name. That path is untouched and stays behind its flag. The
worry that made this a deferred item was really a worry about *that* path
becoming default, which the clarified design does not do.

**Decision**: budget is "within run-to-run noise of baseline", justified
structurally, and confirmed by measurement in a Phase-3 task rather than
assumed.

**Baseline** (`gradle-bitwarden-android` target, 347 components, release
build at `d9c4f21e`, three consecutive runs): **920 / 603 / 600 ms**. First
run carries cold page-cache cost; the 600 ms pair is the steady state. The
confirmation task compares against the steady state, not the first run.

**Alternatives considered**: a lookup-count instrumentation harness — rejected
as disproportionate once the change was seen to be key-substitution; a
per-ecosystem sub-index — rejected as solving a cost that is not incurred.

---

## R2 — Where the recorded ecosystem lives

FR-001a requires the reader to record the ecosystem its dependency names
belong to, with absence meaning "behave exactly as today".

`PackageDbEntry` (`scan_fs/package_db/mod.rs:77`) already carries
`depends: Vec<String>` alongside per-entry metadata that readers populate
selectively (`lifecycle_scope: Option<_>`, `source_type: Option<String>`,
`buildinfo_status`, `evidence_kind`). An `Option`-shaped field whose `None`
preserves current behaviour is the established pattern in this struct, not a
new one.

**Decision**: one additional optional field on `PackageDbEntry` naming the
ecosystem of the names in `depends`. `None` → today's behaviour, by
construction, which is what makes SC-004a assertable rather than merely
tested.

**Consequence worth stating**: the key's *normalisation* changes with it.
`normalize_dep_name` is ecosystem-keyed (`mod.rs:1748` — `pypi` maps `_`→`-`,
everything else lowercases). Today a gem name declared by a `pkg:generic`
requirer is normalised under generic rules; after this change it is
normalised as a gem name. That is a correctness fix in the same motion, and
it is why the recorded ecosystem must drive normalisation too, not only the
lookup.

**Alternatives considered**: a side table keyed by requirer PURL — rejected,
puts the fact somewhere other than where the reader produces it and can go
stale; encoding the ecosystem into each dep string — rejected, changes a
field many readers write and would need parsing back out.

---

## R3 — FR-005b: the standards-native audit, and a family that already exists

**This is the finding that most changes the shape of the work.** Three
per-component annotations for "declared but unresolved" already ship:

| Row | Annotation | Scope today |
|---|---|---|
| C77 | `waybill:depends-unresolved` | Yocto recipe `DEPENDS` |
| C78 | `waybill:rdepends-unresolved` | Yocto recipe `RDEPENDS` |
| C115 | `waybill:unresolved-declared-dep` | npm workspace-peer `package.json` |

C115 is the closest and its catalogue entry already records the disposition
this feature needs: *"the edge is SUPPRESSED and the source-side annotation is
the auditor's signal"*, plus a completed Principle V audit concluding
**KEEP-NO-NATIVE** — CycloneDX `component.evidence` tracks identity
confidence, not unresolved declarations; SPDX `Package.externalRef` is for
resolved URIs.

So FR-005b's audit is **already done for the per-component half**, by C115,
and its conclusion is reusable rather than re-derivable.

For the document-scope count the clarification asked for, the same audit
applies: CycloneDX `compositions[].aggregate` was considered and rejected for
C104 on the grounds that it describes composition completeness rather than
graph reachability, and the same objection holds here.

**Decision**: do not invent a vocabulary. Add the document-scope count as the
aggregate half, and reuse the C115 annotation as the localising half,
broadening its scope from "npm workspace peers" to any requirer. This matches
the aggregate/localise pairing the catalogue already documents between C104
(document) and C45 (component): *"C44 aggregates … C45 localizes"*.

**Hard constraint carried into tasks**: a new catalogue row without a matching
entry in `parity/extractors/mod.rs::EXTRACTORS` fails
`every_catalog_row_has_an_extractor` and `holistic_parity`. Row and extractor
land in the same change or neither does.

**Alternatives considered**: a fourth new per-component annotation — rejected,
would make four annotations for one concept; document-scope only with no
per-component detail — rejected, C115 already ships the detail and removing
it would regress npm.

---

## R4 — Which readers adopt, and in what order

Two confirmed, by different means:

- **gem** — measured. `bitwarden/android` @ `d817f6b`: the application
  main module resolves **0 of 9** declared dependencies by default and **9 of
  9** under the opt-in flag. The 9 are exactly the `Gemfile.lock`
  `DEPENDENCIES` block.
- **nuget** — by construction. The documented version ladder falls back to
  `pkg:generic/<stem>@0.0.0`, and the same reader separately populates that
  main module's `depends` from the lockfile. Pinned by the existing test
  `main_module_version_ladder_falls_through_to_generic`.

**Decision**: gem first — it has the measured before/after and a corpus
target holding the evidence. nuget second, exercised through its fallback
path. Every other reader is untouched and, by FR-001a, provably unchanged.

**Not surveyed exhaustively, deliberately.** `pkg:generic/` identities are
constructed in at least ten readers. Auditing all of them is a discovery task
in its own right and would gate this change on work it does not need, since
an unadopted reader is unchanged by construction. A follow-up survey is
listed under Deferred.

---

## R5 — What remains of the opt-in inference capability

Its own implementation note describes bridging generic main modules "and
future m216-alikes" to matching components — which is this defect, addressed
speculatively and behind a flag.

After this change that flag keeps a genuinely distinct job: resolving names
whose ecosystem **nobody recorded**, by searching every ecosystem and
annotating the result as inference. This feature never guesses; the flag
exists precisely to guess, under supervision.

**Decision**: leave it entirely alone. FR-008 (enabling it changes no edge
this feature produces) becomes near-trivial to satisfy, because the default
path resolves before the fallback is reached — but it is still asserted, since
"near-trivial" is how the original defect survived.

---

## Deferred

- A survey of every reader constructing a `pkg:generic/` identity, to find
  further adopters. Discovery work, not blocking.
- Whether a project component's PURL type should match its ecosystem at all.
  Out of scope per the spec's Assumptions; it would move identity.
