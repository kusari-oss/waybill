# Phase 0 Research: declared licenses from manifests

**Feature**: `1010-manifest-declared-license` · **Date**: 2026-09-26
**Spec**: [spec.md](./spec.md)

Three research questions came out of clarification: per-ecosystem license field
(R1), multi-license operator semantics (R2), inheritance rules (R3). R1 is
answered from the source tree. R2 and R3 describe external ecosystems, so per
the project rule they are answered against each ecosystem's own documentation
and each finding carries its source.

---

## R1 — Where each main-module component is built, and what it already parses

**Method**: enumerated every function in `scan_fs/package_db/` whose body emits
the `main-module` role, then located the `licenses:` construction inside each and
classified it production vs. test by the nearest preceding `#[cfg(test)]`.

**Decision**: there are **14 production main-module sites across 12 ecosystems**, of
which **13 require work** — haskell is already populated by #957, though its failure
branch still changes (FR-008a). Two ecosystems have two sites each (gem, npm), which
is why 14 sites span 12 ecosystems. The table below enumerates all 14.

| Ecosystem | Function | `licenses:` line | State |
|---|---|---:|---|
| cargo | `cargo.rs::build_cargo_main_module_entry` | 703 | empty |
| npm | `npm/walk.rs::build_npm_main_module_entry` | 670 | empty |
| npm | `npm/mod.rs::synthesize_nameless_nested_mainmods` | 716 | empty |
| pip | `pip/mod.rs::build_pip_main_module_entry` | 1022 | empty |
| gem | `gem.rs::build_gem_main_module_entry` | 1505 | empty |
| gem | `gem.rs::build_gem_application_main_module_entry` | 1786 | empty |
| maven | `maven.rs::build_maven_main_module_entry` | 4434 | empty |
| composer | `composer.rs::emit_main_module` | 560 | empty |
| elixir | `elixir.rs::emit_main_module` | 1074 | empty |
| erlang | `erlang.rs::build_main_module_component` | 1489 | empty |
| scala | `scala.rs::build_main_module_component` | 1322 | empty |
| cocoapods | `cocoapods.rs::emit_main_module` | 686 | empty |
| nuget | `nuget/mod.rs::build_nuget_main_module_entry` | 781 | empty |
| haskell | `haskell.rs::build_main_module` | 1780 | **populated** (#957) |

**Rationale for enumerating rather than trusting the issue**: three of the sites
named while scoping were wrong. `maven.rs:2653` is `pom_dep_to_entry` — a
*dependency*, not a main module. `npm/mod.rs:716` is a nested-mainmod
synthesiser, not the npm root; the npm root is in `walk.rs`. `scala.rs:1208` is
`build_lockfile_component`, also a dependency. Three further hits
(`cargo.rs:3173`, `pip/mod.rs:2170`, `gem.rs:2869`, all named
`make_main_module_entry`) are test helpers.

**Measured baseline (T007, `main` @ `f9a292e2`, this repository, `--offline`)**:

```
total components                    : 5514
components carrying any license     :   23   (all from OS-package readers)
scan-root                           : pkg:cargo/app@0.1.0   licenses=0
waybill-common@0.10.0-alpha.1        : licenses=0   <- declares license.workspace = true
pkg:cargo/* components               : 1465
```

Zero of the 1465 cargo components carry a license, including the two member
crates that inherit `Apache-2.0` from `[workspace.package]`. That is the gap this
feature closes, reproducible without any fixture.

**Alternatives considered**: grepping `licenses: Vec::new()` directly. Rejected —
122 such sites exist across the tree and only 13 are production main-module
sites, so the grep alone cannot distinguish the target set from dependency and
test construction.

---

## R2 — Multi-license operator semantics

This is the highest-value finding of Phase 0, and it **contradicts an assumption
made while scoping**.

**Finding**: most ecosystems do not define what a multi-license list means.

| Ecosystem | Field shape | Operator semantics | Source |
|---|---|---|---|
| composer | string **or array** | **Array is disjunctive.** "when there is a choice between licenses (\"disjunctive license\"), multiple can be specified as an array"; conjunction requires the parenthesised `and` string form | [getcomposer.org schema](https://getcomposer.org/doc/04-schema.md) |
| gem | `licenses` array | **Explicitly undefined.** "Note that the array itself does not state how the licenses combine." `license=` singular does not support compound expressions | [RubyGems specification reference](https://guides.rubygems.org/specification-reference/) |
| maven | `<licenses>` list | **Not specified.** The POM reference documents `name`/`url`/`distribution`/`comments` per entry and says a project "should list licenses that apply directly to this project", but states nothing about how several combine | [Maven POM reference](https://maven.apache.org/pom.html) |
| npm | `license` single string | **Not applicable** — a single SPDX expression, e.g. `"(ISC OR GPL-3.0)"`. The `licenses` array form is deprecated | [npm package.json docs](https://docs.npmjs.com/cli/v10/configuring-npm/package-json) |
| pip | `[project].license` single string | **Not applicable** — PEP 639 defines a "single top-level string" holding an SPDX expression, e.g. `"MIT AND (Apache-2.0 OR BSD-2-Clause)"`. Classifiers and the `license.text`/`license.file` table form are deprecated | [PEP 639](https://peps.python.org/pep-0639/) |
| cargo | `license` single string | **Not applicable** — single SPDX expression | cargo manifest reference (unverified; low risk, the field is well established as an expression) |
| nuget | `PackageLicenseExpression` | **Not applicable** — single SPDX expression by name | unverified |
| cocoapods | `license` | **Not applicable** — single value | unverified |
| elixir | `licenses` list | **Unverified** — check Hex package metadata docs. Hexdocs moved during research (301 then 404) and was not chased | not verified |
| erlang | `licenses` list | **Unverified** — check rebar3 / `.app.src` docs | not verified |
| scala | `licenses` Seq | **Unverified** — check sbt reference | not verified |

**Corrections to earlier belief**, both found by fetching rather than recalling:

- I believed Maven **documents** that multiple licenses are a choice. It does
  not. What it does document is inheritance (see R3).
- I believed the list-valued set was large. It is small: npm, pip, cargo, nuget
  and cocoapods all take a **single expression**, so canonicalisation handles
  them and no operator is chosen. Only **maven, gem, composer, elixir, erlang,
  scala** are list-valued, and of the three verified, exactly **one** (composer)
  defines the operator.

**Consequence for FR-010/FR-010a**: "use the operator the ecosystem documents"
is under-determined for most list-valued ecosystems. The load-bearing decision is
therefore the **fallback** for ecosystems that document nothing, and it is not a
free choice:

- Joining with **AND** asserts a consumer must satisfy every listed license. If
  the project meant a choice, this over-states the obligation.
- Joining with **OR** asserts any one suffices. If the project meant cumulative
  terms, this under-states the obligation — the more dangerous direction for a
  consumer relying on the SBOM for compliance.
- The existing shared emitter already joins with **AND** unconditionally
  (`spdx/packages.rs::reduce_license_vec`), so AND is the status quo wherever
  multiple values reach emission today.

This decision is **open** and is carried to the user rather than assumed, because
it changes the legal meaning of emitted output and no default is defensible on
technical grounds alone.

---

## R3 — Inheritance rules

**Decision**: resolve inheritance where the ecosystem defines it; treat an
unresolvable inheritance as a missing declaration (FR-011a, FR-011b).

| Ecosystem | Inheritance | Source |
|---|---|---|
| maven | **Yes.** `licenses` is named in the list of elements inherited from a parent POM | [Maven POM reference](https://maven.apache.org/pom.html) |
| cargo | **Yes.** `license.workspace = true` resolves against `[workspace.package]` | verified in-tree: both of this repository's own member crates use it |
| npm | **Not documented.** The package.json docs do not address whether workspaces inherit a license from the root | [npm package.json docs](https://docs.npmjs.com/cli/v10/configuring-npm/package-json) |
| others | Unverified | — |

**Existing precedent to reuse**: the cargo reader already resolves
`version.workspace = true` through a workspace-root lookup
(`cargo.rs::resolve_cargo_main_module_version`, with the
`[workspace.package]` table handling around lines 545–605). License inheritance
is the same lookup against a different key, so no new traversal is required.

**Why this matters more than it looks**: `license.workspace = true` appears in
**both** of waybill's own member crates. Without R3 the feature returns nothing
for the repository it is built in — which is also the most convenient available
test fixture.

---

## R4 — Preserving an uncanonicalisable declaration

**Decision**: a two-step ladder in the reader — strict canonicalisation first,
lenient preservation on failure. **No emitter change.**

**Rationale**, verified in-tree:

- `SpdxExpression` has two constructors. `try_canonical` runs the real expression
  parser and stores the canonical form. `new` is lenient: it accepts any
  non-empty string without control characters and stores it verbatim.
- `spdx/packages.rs::reduce_license_vec` already handles a value that fails
  canonicalisation by minting a `LicenseRef-<hash>` identifier plus a
  document-level extracted-text record. Its own doc comment states: *"any term
  fails canon → `(LicenseRef(id), Some(extracted_info))`"*.
- The OS-package readers (`alpm`, `opkg`, `rpm_file`, `ipk_file`) already feed
  values through this path, so it is exercised rather than theoretical.

**This inverts the expectation recorded during clarification.** Preserving a raw
declaration was assumed to *widen* scope; it turns out to need no emission work
at all, because the construct already exists, is already reached, and is already
covered by the parity extractors (`parity/extractors/spdx2.rs:237` special-cases
`LicenseRef-`).

**Alternatives considered**: minting the `LicenseRef` in the reader. Rejected —
it would duplicate identifier derivation that already exists at emission and
would risk two readers minting different identifiers for the same text.

---

## R7 — Reader output can be discarded after the reader returns it

**Found during implementation, not planning.** The site map in R1 is necessary but
not sufficient: setting `licenses` at a main-module construction site does not mean
the license reaches the document.

**What happened.** The cargo reader extracted the license correctly and seven unit
tests proved it. The emitted SBOM contained no license. The cause is in
`cargo.rs`, in the branch that runs when a lockfile-derived entry already exists
for the same PURL:

```rust
if let Some(existing) = out.iter_mut().find(|e| e.purl.as_str() == purl_key) {
    // augment in place: annotations, sbom_tier, depends, parent_purl
```

That branch keeps `existing` — the lockfile entry — and copies only the fields it
names from the synthesized manifest entry. The license was not among them. With no
lockfile the other branch pushes the synthesized entry whole and the license
appears; with a lockfile it is dropped. A unit test on the reader function cannot
distinguish the two, because both call the same function and it returns the same
value.

**Evidence.** Same manifests, lockfile toggled:

| | `waybill:source-files` | licenses |
|---|---|---|
| no `Cargo.lock` | `["path+file:///…/member"]` | 1 |
| with `Cargo.lock` | `["Cargo.lock"]` | 0 |

**Consequence for the remaining ten ecosystems.** Every reader with both a
manifest path and a lockfile path may have the same shape. Before closing each
reader's task, scan a fixture **with** its lockfile present, not only without:
`npm` (package-lock.json), `pip` (uv.lock / poetry.lock), `gem` (Gemfile.lock),
`maven` (no lockfile, but the nested-JAR path collides similarly), `composer`
(composer.lock), `elixir` (mix.lock), `erlang` (rebar.lock), `scala` (build.sbt
lock), `nuget` (packages.lock.json). The integration test
`m954_fr011a_lockfile_and_lockfile_free_scans_agree` encodes the property in
general form and should be extended per ecosystem rather than re-derived.

**A wrong turn worth recording.** `resolve::deduplicator::deduplicate` also fails
to merge licenses when it collapses a group, and a comment beside it documents the
identical defect class for `requirement_ranges` (#936). That made it a convincing
culprit and it was patched first — wrongly. The disconfirming evidence was already
in the output above: `deduplicate` unions `source_file_paths`, so had the two
entries met there the result would have listed both paths, not `["Cargo.lock"]`
alone. The patch was reverted. The latent gap is real but unexercised, so it is
recorded here rather than fixed speculatively.

## R5 — Constitution audit (Principle V native-construct clause)

**Decision**: no `waybill:*` property is introduced. Provenance is carried by
native constructs in every format.

| Requirement | Native construct | Format |
|---|---|---|
| "this license was declared by the project" | `licenses[].license.acknowledgement: "declared"` | CycloneDX 1.6 |
| same | `licenseDeclared` | SPDX 2.3 |
| same | declared-attribution license expression element | SPDX 3 |
| "this license is a third-party conclusion" | `acknowledgement: "concluded"` / `licenseConcluded` | all three |
| "this license is not on the standard list" | `LicenseRef-<id>` + extracted-text record | SPDX 2.3 / SPDX 3 |

This also discharges **Principle XII.2**, which requires data from an external
source to carry its provenance. The declared-versus-concluded distinction *is*
that provenance, expressed natively, which is why clarification declined a
`waybill:license-source` annotation: a `waybill:*` field is permitted only for
information the standard cannot express, and the standard expresses this.

**Pre-existing tension, not aggravated**: Principle II forbids static manifest
parsing *as a dependency source*, while `sbom scan` is manifest-based discovery.
That conflict is tracked in **#987** and is untouched here — this feature adds
license metadata to components discovered by whatever mechanism the scan already
uses, and introduces no component (Principle XII.1).

---

## R6 — Fallback operator for ecosystems that document nothing

**Decision**: join with **AND** where the ecosystem defines no multi-license
semantics. Record the assumption in the reader documentation. No annotation.

**Rationale**: the two directions fail asymmetrically, and only one failure is
safe.

- AND over-states the obligation: a consumer complies with more licenses than
  required. Wrong, but it cannot cause a violation.
- OR under-states it: a consumer satisfies one license and may ship in breach of
  another that also applied. Wrong *and* dangerous.

When the error is legal rather than cosmetic, the conservative direction wins.
This also keeps **one** combining rule in the system: `reduce_license_vec`
already joins with AND, so a reader-side AND agrees with the emitter instead of
introducing a second, divergent semantic.

Applies to: **maven**, **gem** (both verified as undefined), and **elixir**,
**erlang**, **scala** pending verification. Does **not** apply to **composer**,
which documents disjunction and therefore gets OR.

**Alternatives considered**:

- *OR where silent* — more often factually right, since dual licensing is the
  usual reason to list several. Rejected: being right more often does not
  compensate for a failure mode that can cause a licence breach.
- *Refuse to combine, preserve the raw list as a non-listed reference* — most
  honest about what the manifest says, and asserts no operator. Rejected: it
  yields no matchable license identifier, so compliance and scoring tools see a
  blob. That trades a small accuracy gain for a large utility loss.
- *AND plus an annotation marking the operator as inferred* — most transparent,
  and admissible under Principle V since no format expresses "operator assumed".
  Rejected for scope: it costs a parity-catalog row plus three extractors for a
  signal a reader can instead document once.

**Recorded assumption**: where this fallback fires, the emitted expression states
a conjunction the project never declared. The manifest is the authority on
*which* licenses apply; the operator between them is waybill's inference, and the
per-ecosystem table in the reader documentation must say so.
