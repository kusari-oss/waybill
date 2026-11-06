# Phase 0 research: an ecosystem identity for closure components

**Feature**: `1366-closure-ecosystem-purl` | **Issue**: #1365 | **Date**: 2026-11-06

Every finding below was read out of the tree at `34dc5e01`, with the file and
line cited, rather than recalled. Four probe errors in the measurement session
that produced this spec all had the same shape — modelling what the code does
instead of reading it — so each decision here names what was read.

---

## R1 — Where the confirmation pass runs

**Decision**: immediately after the closure merge in
`waybill-cli/src/cli/scan_cmd.rs`, in the same post-merge block as the
milestone-1275 Hackage identity pass — **not** in the enrichment stage.

**Rationale**: read, not assumed. The ordering in `scan_cmd::execute`
(`pub async fn execute` at `scan_cmd.rs:3126`) is:

| line | step |
| --- | --- |
| 3960 | `deps_dev_source` built (unconditionally) |
| 3963 | `enrich_components(.., EnrichmentPass::Initial).await` |
| 4080 | `enrich_components(.., EnrichmentPass::PostGraph).await` |
| 4503 | `closure::admission(args.nix_closure, offline)` |
| 4748 | `merge_closure_components(&mut components, added)` |
| 4760+ | milestone-1275 Hackage identity pass |

**The closure components do not exist yet when either enrichment pass runs.**
A confirmation pass placed in the enrichment stage would see zero candidates
and pass vacuously — the exact failure mode
`feedback_a_passing_test_may_exercise_nothing` records.

The post-merge slot is also already justified by measurement rather than
preference. The comment at `scan_cmd.rs:4755-4775` records that milestone 1035
tried the obvious alternative — moving the merge above the Haskell enrichment
so the existing pass would reach these components — and **measured 41 PURLs
downgraded** from `pkg:hackage/QuickCheck@2.15.0.1` to `pkg:generic/…`. The
same hazard applies here: the closure's generic identifier must not reach a
pass that would overwrite a better one.

`execute` is `async`, so an `.await` is available at line 4748.

**Alternatives rejected**:

- *A third `EnrichmentPass` variant.* The variant would still be invoked from
  the enrichment stage, which is upstream of the components it must act on.
  Invoking `enrich_components` a third time from the post-merge block is
  possible, but that function applies licenses and `externalReferences` as
  well — scope this feature does not ask for (see R10).
- *Inside the closure reader (`closure::emit::components`).* The reader is
  synchronous, lives under `scan_fs/`, and is reached from `closure::resolve`
  before any merge. It also has no access to `deps_dev_source`. Confirmation
  there would mean a second network client in a tier that has none.

---

## R2 — The confirmation carrier

**Decision**: `DepsDevSource`, the enrichment path the spec's clarification
selected, reached through a new narrow entry point beside
`enrich_components`.

**Rationale**: the four states FR-002/FR-003/FR-009/FR-012 need already exist,
named, in `waybill-cli/src/enrich/depsdev_source.rs:50-60`:

```rust
pub(crate) enum LookupResult {
    Found(VersionInfo),
    /// deps.dev answered and holds no record: a 404, or a batch item
    /// echoed without `version`.
    Absent,
    /// Nothing was learnt: a transport error, or an unanswered batch slot
    /// whose per-key retry also failed.
    Failed,
    /// `--offline`, and neither cache held it.
    Unqueried,
}
```

This maps 1:1 onto the spec's **Confirmation outcome** entity:

| `LookupResult` | outcome | requirement |
| --- | --- | --- |
| `Found` | confirmed | FR-002 — emit the ecosystem identifier |
| `Absent` | declined | **FR-003** — "no such package at this version" is a decline |
| `Failed` | could-not-check | FR-009, FR-012 |
| `Unqueried` | could-not-check | FR-009, SC-006 |

FR-003's hardest clause — *treat "the published ecosystem has no such package
at this version" as a decline rather than as a failure* — is already true of
the client. `DepsDevClient::get_version` (`deps_dev_client.rs:259`) returns
`Ok((None, max_age))` on HTTP 404, with the comment: *"That is an answer,
cacheable under the response's own `max-age` … and `Err` is reserved for
failures that said nothing about the package (#1094)."* Throttling is a
distinct `Throttled` error (`deps_dev_client.rs:32`), which is what FR-012
must contain.

**What must be added**: `fetch_results` (`depsdev_source.rs:333`) is a private
`async fn` on `DepsDevSource`. A `pub(crate)` entry point taking
`&[EnrichmentKey]` and returning the outcomes is the whole of the new surface.
`EnrichmentKey` (`request_key.rs:41`) is already `{system, name, version}` and
already returns `None` for an ecosystem deps.dev does not index or for an
empty/placeholder version.

---

## R3 — The candidate rule

**Decision**: a derivation is a PyPI candidate when its `name` matches
`^python3\.\d+-` **and** its declared `pname` does not itself start with
`python3`.

**Rationale**: both halves are load-bearing, and both were measured on the
pinned closure by `specs/1276-boot-library-scope/measurements/pypi-identity-probe.py`:

- A bare `python-` prefix matches nixpkgs **setup hooks**
  (`python-relax-deps-hook`, `python-namespaces-hook.sh`) — build machinery,
  not packages. Twenty of them on this closure. The versioned prefix excludes
  them.
- nixpkgs **bootstrap variants** carry the prefix inside `pname` itself
  (`python3.13-bootstrap-tomli`). They are not the PyPI project; the second
  clause drops all six.

Yield: **83 derivations, 75 distinct names.**

**The `pname` read must consult both carriers.** `attr_str` reads the flat
`env` map *and* the top-level `structuredAttrs` object
(`closure/derivation.rs`); 66 of 1,275 moat derivations keep `pname` only in
the typed carrier. Reading `env` alone counts those as nameless — the error
that produced the worst of this session's four probe mistakes. The reader's
own `attr_carrier()` helper and `waybill debug closure-dump` exist precisely so
this is read rather than re-modelled.

**Alternative rejected**: a maintained list of Python package names. The spec's
first Assumption forbids it — *"no second evaluation, no maintained list"* —
and a list is wrong the day nixpkgs adds a package.

---

## R4 — Name normalisation

**Decision**: PEP 503 — `re.sub(r"[-_.]+", "-", name).lower()` — applied to
the `pname` before the coordinate is formed.

**Rationale**: FR-004. The repository already holds this rule twice:

- `waybill-cli/src/enrich/clearly_defined_coord.rs:107-111` lowercases for the
  `pypi` coordinate, with the comment *"PyPI normalizes names to lowercase per
  PEP 503."*
- `pip::normalize_pypi_name_for_purl` at
  `waybill-cli/src/scan_fs/package_db/pip/mod.rs:99` is the reader-side
  implementation, already reused cross-reader by milestone 674 (FR-015).

**Reuse `pip::normalize_pypi_name_for_purl`.** A third implementation would be
a second model of the same rule, which is the failure this spec was written
after.

The probe reports how many of the 75 names need normalisation; that count
belongs in the measurement record, not in a requirement.

---

## R5 — Native-field audit (Constitution Principle V) — **MANDATORY**

Principle V: *"Before introducing any new `waybill:*` property … every spec
MUST audit each target format for an existing native construct that carries
the same semantic."* Audited against the vendored schemas at
`waybill-cli/tests/fixtures/schemas/`, following the method milestone 1275
recorded for C201.

**CycloneDX 1.6 — a native carrier EXISTS and MUST be used.**
`component.evidence.identity[]` carries `field` (enum including `purl`),
`confidence` (0–1), `concludedValue`, and `methods[].technique` (closed enum
including `manifest-analysis`). This is exactly "an identifier was concluded,
by this technique, with this confidence" — which is FR-007.

waybill already emits it: `cyclonedx/evidence.rs::build_evidence` emits
`evidence.identity` as an array, and milestone 105 already extends
`identity[0].methods[]` with a `waybill-source-mechanism` key for C56.

> **Note the distinction from C201.** C201's audit *rejected*
> `evidence.identity[]`, correctly: C201 records **why a candidate identity was
> rejected**, and there is no slot for a rejection reason. This feature records
> that an identity **was concluded and on what basis** — the construct's actual
> semantic. The two audits reach opposite conclusions because they are asking
> about different statements.

**SPDX 2.3** — no native carrier. `Package.comment` is untyped prose;
`sourceInfo` describes acquisition; `primaryPackagePurpose` is a closed enum
about what a package *is*.

**SPDX 3.0.1** — no native carrier. `software_Package`'s members resolve to
`comment`, `description`, `summary`, `software_sourceInfo`,
`software_primaryPurpose`, `software_packageUrl`, `externalIdentifier`,
`externalRef` and the artifact bases. None states that an identifier was
derived.

**Decision**: hybrid emission on the C56 template — CDX native via
`evidence.identity[].methods[]`, SPDX 2.3 / SPDX 3 via a parity-bridging
annotation, catalogued with the missing-native-field justification clause
Principle V requires. `C56`'s extractor is `Directionality::SymmetricEqual`
(`parity/extractors/mod.rs:311`) because the extractors normalise both sides to
one value set; this row follows it.

**Existing `waybill:` rows audited and rejected**:

- **C201 `waybill:closure-identity`** — the closest row, and the reason this
  audit was done carefully. C201 is a *closed enum* about the **Hackage**
  identity pass on **closure-appended members**, and its documented values
  (`confirmed`, `version-disagrees`, `not-in-package-set`, …) are statements
  about a pinned nixpkgs package set. Overloading it would make a closed enum
  open and make one row mean two passes.
- **C151 `waybill:unresolved-reason`** — emitted iff the component carries
  `waybill:sbom-tier: "design"`; closure components are build tier. The same
  constraint #1268 hit.
- **C191/C192 `waybill:deps-dev-outcome(s)`** — these answer *why deps.dev did
  not enrich a component in one of its six ecosystems*. A candidate here is
  `pkg:generic` at the time of the query, which C192 already counts as
  `not-queried:unsupported-ecosystem`. Reusing it would conflate "deps.dev
  declined to enrich this component" with "deps.dev declined to confirm a
  coordinate this component does not yet carry".

**New rows**: `C207` (per-component, hybrid) and `C208` (document-scope
record). Next free id confirmed by `grep -oE "^\| C[0-9]+"` over
`docs/reference/sbom-format-mapping.md` — highest existing is **C206**.

Every new row needs a matching entry in `parity/extractors/mod.rs::EXTRACTORS`
or `every_catalog_row_has_an_extractor` and `holistic_parity` fail
(`feedback_sbom_format_mapping_extractor_gate`).

---

## R6 — The document-scope record (FR-008, FR-009)

**Decision**: one document-scope JSON object, `C208`, on the C192/C204
template: a schema key plus counts (`candidates`, `confirmed`, `declined`,
`unchecked`) and, when nothing was confirmed, a `reason` naming why.

**Rationale**: FR-008 exists so *"a reader can tell a closure with no Python
packages from a confirmation step that silently did nothing"* — which is
`feedback_zero_results_can_mean_broken` stated as a requirement. A count of
zero candidates and a count of 75 candidates all unchecked are different
documents under this shape and identical under a boolean.

Precedents read: `C192 waybill:deps-dev-outcomes` (document-scope JSON object
counting per-component outcomes), `C204 waybill:nix-closure-read`
(document-scope JSON object recording what the closure reader saw **and did
not consult**), `C190 waybill:nix-closure-degraded` (`{"reason": "<code>"}`
plus `detail`). `C190`'s shape is the model for the FR-009 half.

---

## R7 — **The corpus cannot witness this feature** (SC-001, SC-007, SC-008)

**Finding, read from the harness, not assumed.** Exactly one corpus target
runs the closure tier — `nix-closure-moat`
(`corpus_harness_195/manifest.rs:227`). The harness invokes it like this
(`harness.rs:218-220`):

```rust
if let ScanMode::NixClosure { attr } = target.scan_mode {
    cmd.arg("--nix-closure").arg("--nix-closure-attr").arg(attr);
    cmd.arg("--no-deps-dev").arg("--no-clearly-defined");
}
```

and `ScanMode::NixClosure`'s own doc comment (`manifest.rs:125-136`) states the
reason: *"the scan turns off waybill's own online enrichment … so nix is the
only process that could reach the network at all."* Every other target passes
`--offline`.

**Consequences, stated plainly:**

1. **`--no-deps-dev` must suppress confirmation.** The operator said no
   deps.dev; a confirmation pass that queried anyway would be a flag violation.
   So the one corpus target that reaches this code **always takes the FR-009
   degraded path**.
2. **SC-008 holds for all 17 targets**, trivially — 16 never run a closure, and
   the 17th never confirms. The moat goldens still change **once**, by the
   addition of the C208 record saying confirmation did not run. That refresh
   goes through the `public-corpus.yml` workflow dispatch;
   `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is never set locally.
3. **SC-007 cannot be satisfied by the corpus.** "Disabling the feature makes a
   test fail" needs a test that actually confirms something. `wiremock = "0.6"`
   is already a dev-dependency (`waybill-cli/Cargo.toml:236`) and
   `DepsDevClient::with_base_url` (`deps_dev_client.rs:137`) already exists for
   exactly this; the US1 test stands up a stub deps.dev and a synthetic closure
   fixture.
4. **SC-001's number cannot come from CI.** It is a manual, recorded
   measurement on the pinned closure — aarch64-darwin, like every other closure
   measurement — committed under `specs/1366-closure-ecosystem-purl/measurements/`
   with the command that produced it. The spec already forbids predicting it.

This is the `feedback_a_passing_test_may_exercise_nothing` shape found **before**
implementation rather than after, which is the whole point of doing R7.

---

## R8 — Collision after normalisation (edge case)

**Finding**: `merge_closure_components` (`scan_cmd.rs:6197`) indexes the
existing set by `(name, version)` — **not** by PURL:

```rust
let index: HashMap<(String, String), usize> = components
    .iter().enumerate()
    .map(|(i, c)| ((c.name.clone(), c.version.clone()), i))
    .collect();
```

So a closure component named `urllib3 2.6.3` has *already merged* into a
manifest-derived `pkg:pypi/urllib3@2.6.3` before this pass runs. **A confirmed
identifier therefore cannot collide with another reader's component** — that
case never reaches the pass as a separate component. This also discharges a
risk FR-006 would otherwise carry.

**The residual risk is internal and narrow**: two appended closure components
whose names differ only in case or separator (`Foo_Bar`, `foo-bar`) normalise
to one PEP 503 name and, at equal versions, to one coordinate. The pass must
detect this and decline both rather than emit a duplicate identifier — the
`feedback_a_dedup_can_delete_the_only_copy` hazard in its additive direction.
Whether any such pair exists on the pinned closure is **measurable before
implementation** and is a research task in `tasks.md`, not an assumption here.

---

## R9 — What `Found` proves about the version (FR-010)

**Decision**: a `Found` result for `(pypi, <pep503 name>, <closure version>)`
is the evidence FR-010 requires, and nothing beyond it is claimed.

**Rationale**: the query is version-exact — `GetVersion`, not `GetPackage`. A
`Found` means PyPI published *that project at that version*. A nixpkgs-patched
or re-versioned package whose version string does not correspond to a published
release returns `Absent` and is declined by FR-002 without any extra rule.

The limit is the one the spec's Assumptions already record and must not be
overstated: this establishes **existence**, not that *this derivation is that
package*. `librt` and `roman-numerals` both fail it, which is why the mechanism
works on the measured population; a name that is both a published project and a
plausible local one (`build`, `toml`) would pass. FR-007's disclosure exists so
a consumer can see the basis.

---

## R10 — Cache reuse (FR-011) and failure containment (FR-012)

**FR-011 is satisfied by construction.** `fetch_results`
(`depsdev_source.rs:333-380`) consults, in order: the in-memory cache, then the
disk cache at `~/.cache/waybill/deps-dev/` — *"FR-012: disk before network. A
hit here is still a hit even under `--offline` … reading a local file is not a
request."* Entering through that function inherits both. Keys are
`(system, name, version)`, so a confirmation and a later enrichment of the same
coordinate share one cache entry.

**FR-012** is satisfied by mapping `Failed` and `Unqueried` to the
could-not-check outcome and by the pass returning a record rather than a
`Result` the caller can `?` on. The one thing that could still fail a scan is a
panic; the pass takes no `.unwrap()` (Constitution Principle IV and the
`clippy::unwrap_used` deny at the `waybill-cli` crate root).

**Deliberate non-goal.** A confirmed lookup returns a full `VersionInfo` —
licenses and `links[]` included. Applying it would enrich the confirmed
components for free, and is **out of scope**: the spec asks for identity, SC-003
and SC-004 do not sanction it, and it would change the goldens in ways no
requirement predicts. Recorded here so the implementer does not drift into it;
worth a follow-up issue once SC-001 is measured.

---

## R11 — The ordering constraint is the design (edges)

**The hazard, read first.** `Relationship.from` and `.to` are **PURL strings**
(`waybill-common/src/resolution.rs:413-418`), not stable component handles. An
identifier changed after edges are built orphans every edge that pointed at it
— which would turn each correctly-identified Python package into an orphan and
make this feature a net regression against milestone 158's graph-completeness
record.

**Why it does not happen**: `closure::graph::relationships`
(`closure/graph.rs:240-262`) builds its join table from the **live component
slice**:

```rust
let by_identity: HashMap<Identity, &str> = components
    .iter()
    .map(|c| ((c.name.clone(), c.version.clone()), c.purl.as_str()))
    .collect();
```

It keys on `(name, version)` — which this pass never changes — and reads
`c.purl` **at call time**. It is invoked at `scan_cmd.rs:4833`, i.e. **after**
the merge at 4748. So a pass that runs between them has its corrected
identifiers picked up automatically, with no edge rewriting.

**The required placement is therefore exact:**

```text
4748  merge_closure_components          ← components exist
4760+ milestone-1275 Hackage identity pass
      ▶ THIS PASS                       ← the only correct window
4833  closure::graph::relationships     ← edges read c.purl here
4854  relationships.extend(edges)
```

The comment at `scan_cmd.rs:4824-4828` records the neighbouring hazard in the
other direction: *"an earlier placement would dangle: `apply_renames` moves
relationship endpoints alongside identities (#980/#981), which this milestone's
research R1 got wrong and has retracted."* An edge build placed before an
identity change is a mistake this repository has already made once and written
down.

**Consequence for `tasks.md`**: the window is enforced by a test, not by a
comment. Moving the pass after 4833 must make a test fail — the same standard
SC-007 sets for the feature as a whole.

---

## Constitution alignment

| Principle | Finding |
| --- | --- |
| **I. Pure Rust, statically linked** | Zero new Cargo dependencies. Every crate named here is already a direct dependency. |
| **IV. Type-driven correctness** | Coordinates flow as `waybill_common::types::purl::Purl`, never `String`. No `.unwrap()` in production. |
| **V. Specification compliance** | Native-field audit performed in R5 against vendored schemas; CDX native carrier found and adopted; SPDX parity bridge justified. Every emitted PURL is `Purl::new`-validated, so FR-002 cannot emit a malformed identifier. |
| **IX. Accuracy** | US2 **is** this principle: *"ambiguous or low-confidence matches MUST be flagged rather than silently included as definitive."* Unconfirmed candidates keep today's identifier. |
| **X. Transparency** | FR-007 (per-component basis), FR-008 (document-scope counts), FR-009 (unavailability recorded). CDX uses the spec-native `evidence` mechanism, as Principle X prefers. |
| **XII. External data source enrichment** | Scan mode. Constraint 1 — *MUST NOT introduce new components*: this pass changes identifiers on components that already exist; the count is invariant. Constraint 2 — provenance annotation: FR-007. Constraint 3 — unavailability must not prevent generation: FR-012. Constraint 4 — the scanned evidence stays the authority; deps.dev confirms, it does not name. |
| **Operating Modes** | Scan mode throughout; `waybill:generation-context` unchanged. |
| **Strict Boundary 1** | Trace mode only. N/A. |

**No violations. No Complexity Tracking entries.**
