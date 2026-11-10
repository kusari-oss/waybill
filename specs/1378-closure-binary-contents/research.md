# Research: closure binary contents

Each decision cites either a measurement in `measurements/FINDINGS.md` or code
read on 2026-11-10 at `main@49a24a63`. Line numbers are as of that commit and
must be re-verified at implement time.

> **Superseded on rebase (2026-11-10).** While this was being built, main
> landed milestone 1377 (#1405, issues #1398 and #1394): the package a closure
> scan evaluated is always emitted and is the document's subject, identified
> as `graph.evaluated` — the closure's sole unconsumed derivation. That is the
> root-as-component work this feature had taken on as "the minimum of #1398".
> On rebase the feature's own mechanism (a `root` value for
> `waybill:closure-role`, `DerivationRole::Root`, `graph::sole_root`) was
> **removed** in favour of main's. Text below that describes it is history.
> Main also took row C212, so this feature's rows are **C213, C214, C215**.

## R1. Making the closure root a component

**Observed.** The root is the derivation no other names
(`closure/graph.rs` `root_of`). Nothing names it, so `classify` gives it
`DerivationRole::Unreferenced` and `emits_as_component` drops it
(`closure/emit.rs`). `graph::relationships` then counts its edges as
`unanchored` and drops them.

**Decision.** Add `DerivationRole::Root` (wire value `root`), assigned in
`classify` to `root_of`'s answer when no declared attribute or graph referrer
gives it another role. `Root` emits on role alone.

**Rationale.** The role enum is matched exhaustively in the classifier, the
emitter and the summary, so a new variant forces each site to decide. The
role-scope pass matches wire strings with a fall-through that classifies
nothing, so `root` is not marked absent from the artifact — which is correct,
the root *is* the artifact. An explicit `"root"` arm is added so that is a
decision rather than an accident.

**Alternatives rejected.** Special-casing the root key inside `components()`
without a role: the C182 annotation would still say `unreferenced` and the
role-scope pass would mark the scanned package `excluded`. Reusing
`artifact-input`: nothing references the root, so the label would be false.

**Consequence.** C182 gains a value; the C184 role tally gains a key and
`unreferenced` drops by one. When another reader already emitted the root's
`(name, version)`, `merge_closure_components` merges as today.

## R2. Reading an output: the two readers, not a scan

**Measured.** A full scan of `rustc-1.95.0` returns 709 crates, none from a
binary — all from `Cargo.lock` files under `lib/rustlib/rustc-src/`. A full
scan of `go-1.26.3` returns 186 non-module entries.

**Decision.** New `scan_fs/binary/embedded.rs` exposing
`pub(crate) fn embedded_contents(path: &Path) -> EmbeddedContents`, which:

- discovers binaries with the existing `discover_binaries` (accepts a single
  file or a directory);
- for each, calls `go_binary::read_binary` and, on success, builds dependency
  and `stdlib` entries with the existing entry builder, **omitting the main
  module**, which is returned separately as a self-description;
- for each, calls `scan_binary` and, when it returns a cargo-auditable
  manifest, builds entries with `cargo_auditable_packages_to_entries`, clears
  `parent_purl`, and returns the `root: true` crate separately as a
  self-description instead of as an entry.

**Rationale.** FR-001 requires the same recognition as the standalone scan;
calling the same functions is the only way two paths cannot drift. Nothing
else the binary reader produces (file-level entries, linkage, version strings)
is a module.

**Seams to open.** `go_binary::emit_entries_from_info` is private;
`scan_binary` and `cargo_auditable_packages_to_entries` are `pub(super)` and
reachable from a sibling module in `binary/`.

**Not carried over.** `depends` on crate entries (crate-to-crate edges) is
ignored — out of scope, and the root crate they point at is not emitted.

## R3. Mapping members to outputs and to components

**Observed.** A member is a plain `ResolvedComponent`; several derivations
(build variants) collapse into one by `(pname, version, identity-hash)`. No
component holds its `.drv` or output paths. `RawClosure` survives in
`ClassifiedClosure.raw`. Output paths appear with and without the store
prefix; `store_basename` normalises. The closure modules never touch the
filesystem today.

**Decision.** The pass iterates `raw.derivations`, keeps those
`emits_as_component` admits, and groups them by the `(pname, version)` join
`graph::relationships` already uses. For each group it resolves every
`outputs.*.path` to `<store dir>/<basename>` and checks existence.

**Store directory.** `NIX_STORE_DIR` when set, else `/nix/store`. This is
nix's own variable, covers the "store not at the conventional location" edge
case, and is the test seam.

**Read state per member**, over all outputs of all its derivations that
declare a path:

| state | meaning |
| --- | --- |
| `read` | every declared output was present and examined |
| `partial` | some present and examined, some absent |
| `absent` | none present |
| `failed` | a present output could not be examined (I/O error); wins over `partial` |
| `no-outputs` | the member declares no output path (floating content-addressed) |

A member that was `read` and yielded nothing is still `read` — that is the
FR-014 distinction.

## R4. Where the pass runs

**Observed.** `graph::relationships` joins `.drv` → component by
`(name, version)` and reads `c.purl` from the live slice at call time. The
m1366 identity pass runs immediately before it for that reason.

**Decision.** Run the contents pass after `run_closure_ecosystem_identity` and
before `graph::relationships`. Promotion changes only `purl`, so the
`(name, version)` join still finds the member and the closure edges pick up
the promoted identifier.

**Later passes that then apply for free:** CPE synthesis
(`fill_missing_cpes`), so recovered modules get the same CPE candidates the
standalone scan gives them.

**Scope inheritance (FR-010).** The pass sets `artifact_presence` on a
recovered module directly: `NotInArtifact` only if **every** member it was
recovered from has role `build-tooling` or `unreferenced`. C182 is not copied
onto modules — it would make them "closure components" to `orphan_reason` and
to tests that select on it.

## R5. Identity promotion

**Measured.** deps.dev confirms `go github.com/cli/cli/v2 @ v2.93.0` (200) and
`cargo ripgrep @ 15.1.0` (200). The closure's own form `2.93.0` returns 404,
identical to a nonexistent version.

**Decision.**

- Candidate = self-described name + closure version. Go: version prefixed
  with `v` unless it already has one. Cargo: version unchanged.
- A member gets a candidate only when its binaries agree on **one**
  self-description. Several distinct self-descriptions (measured:
  `cargo-auditable` ships two binaries with different root crates) → status
  `ambiguous`, no promotion.
- A member with no closure version gets no candidate (nothing to join).
- Confirmation through `DepsDevSource::confirm` with
  `EnrichmentKey::from_purl_parts`, exactly as the m1366 pass does. Disabled
  deps.dev → `unchecked: disabled`.
- `Found` → the member's `purl` becomes the ecosystem PURL. Anything else →
  unchanged PURL, candidate recorded in C215.

**Not reused.** `ecosystem_identity.rs`'s candidate rule and record are
PyPI-specific by construction; only the key-and-confirm call is shared.

## R6. Annotations and native constructs

Audited against the vendored schemas (FINDINGS P4).

| need | native | decision |
| --- | --- | --- |
| member → module | dependency relationship in all three | native, no row |
| confirmed identity | primary identifier | native, no row |
| "contents incomplete" | CycloneDX `compositions`; none in SPDX | native in CycloneDX: withhold the root-completeness claim when any member is unread (same mechanism as `read_outcome.has_gap()`); C213 for counts |
| read counts | none | **C213** `waybill:nix-closure-contents`, document scope |
| per-member read state | none | **C214** `waybill:closure-contents`, component scope |
| candidate / reconciled identity | CycloneDX only | **C215** `waybill:closure-binary-identity`, component scope (Complexity Tracking) |

Highest existing row is C211. Per-component rows travel through
`extra_annotations`; the document-scope row needs the `ScanArtifacts`
plumbing the m1372 row C210 used.

## R7. The strict opt-in

**Observed.** `sbom scan` has no `strict`/`require`/`fail-on` flag. Elsewhere:
`--require-signing`.

**Decision.** `--nix-closure-require-root-contents`, `requires = "nix_closure"`.
After the pass, if the root's read state is not `read` — or the closure
degraded so there is no root — the scan fails before any document is written.
`read` with nothing recovered passes.

## R8. Graph completeness

**Observed.** `TransitiveEdgesUnresolvable` fires for `analyzed`-tier
components with no `source`/`deployed`/`build` counterpart. Recovered modules
are `analyzed`, as in the standalone scan.

**Decision.** Keep `analyzed`. The resulting `partial` for `golang`/`cargo` is
true: the module-to-module graph is not known (flat for Go, dropped for
cargo — the out-of-scope defect). FR-016 is satisfied a second way by R6's
compositions change.

## R9. Testing without nix or real packages

**Observed.** Closure integration tests put a stub `nix` script first on
`PATH` that answers `derivation show` from a fixture JSON. Go build-info and
cargo-auditable unit tests already synthesize minimal binaries in memory.

**Decision.**

- Unit tests for `embedded_contents` reuse those synthesizers (exposed
  `#[cfg(test)]`).
- Unit tests for the pass build a `ClassifiedClosure` from fixture JSON (as
  `emit.rs` tests do) and a temp store directory.
- Integration tests extend the stub-`nix` pattern with `NIX_STORE_DIR` and a
  binary fixture written at test time. Whether a synthesized binary survives
  `discover_binaries`' size and magic checks is verified first; if not, the
  fixture is a committed minimal binary with synthetic module names.
- The real-target checks (SC-001 to SC-003) are quickstart steps run by hand
  against the wrapper flakes — they need nix and network and cannot be CI
  tests.
