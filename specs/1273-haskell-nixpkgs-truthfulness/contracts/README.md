# Contracts: Haskell/nixpkgs resolution correctness

**Milestone**: 1273

waybill exposes no API to other systems. Its interfaces are the **emitted
document** and the **CLI**, so these contracts are stated as properties of
emitted documents, each testable by reading one document with no access to
the scan log — which is SC-009's requirement and FR-018's point.

Every contract names the requirement it serves and the test that must fail
when the behaviour is removed (SC-006, mutation-tested).

## CLI surface: unchanged

**C1.** This milestone adds no flag, no subcommand, no environment variable
and no positional argument. `waybill sbom scan --help` is byte-identical
before and after.

FR-021 and FR-022. The second is the stronger one: no behaviour may vary
with what is on the host's `PATH`, so two scans of one tree on two machines
agree. The project has already shipped fixtures whose Gradle tier was
silently decided by a system `gradle` on CI's runner image, and Phase 0
measured that a subprocess tier could not do better here anyway — hpack
itself refuses a `defaults:` manifest outright when it cannot fetch.

---

## Document contracts

### C2 — an hpack-only project emits its declared dependencies

FR-001, SC-001. Given a `package.yaml` with no `*.cabal`, every dependency
it declares appears as a component with `pkg:hackage/<name>`.

Measured today: **0 components** for a manifest declaring 3, with the reader
dispatched to the file (`per_reader_dispatch_counts{"haskell": 1}`).

### C3 — inheritance reaches every stanza kind, with its own scope

FR-001a, R1/A1. A top-level `dependencies` list reaches `library`,
`executables`, `tests` and `benchmarks`. A component reached only through a
`tests:` or `benchmarks:` stanza carries `Development`; one reached through
`library` or `executables` carries `Runtime`; a name in both carries
`Runtime` (`merge_scope`'s most-binding-wins, unchanged).

### C4 — a repeated name keeps the last declaration's constraint, and only that

FR-001a, R1/A3. For one name, the last declaration in the ordered merge
(top-level, then per-component) wins and **replaces** any earlier
constraint. `waybill:requirement-ranges` carries that one range.

Negative half, and the reason this is a contract rather than a detail: a
document MUST NOT carry an intersected range. `text >= 1.0` plus `text < 3`
yields `< 3`, never `>= 1.0 && < 3` — the intersection is a constraint the
project does not declare.

### C5 — the range is recorded as declared, not as hpack renders it

FR-001a, R1/A4. hpack rewrites `>= 4 && < 5` into `==4.*`. The document
carries the manifest's text.

### C6 — a `when:` block becomes a conditional stanza, evaluated once

FR-001a, R1/A5. A literal `false` condition contributes nothing. Any other
condition — `os(...)`, `flag(...)`, including a flag whose default is
declared in the same file — lowers into the representation
`stanza_default_buildable` already evaluates.

The contract is about agreement, not mechanism: **a dependency declared by
a default-off `when: flag(x)` block in `package.yaml` and the same
dependency declared by a `buildable: False` stanza in a `*.cabal` MUST
receive the same classification and the same
`waybill:optional-derivation`.** Two evaluators would let one project get
two answers depending on which file declared its dependencies, which is the
defect class this milestone exists to remove.

### C7 — exactly one manifest is the source of truth, and the document says which

FR-002. Where a `package.yaml` sits beside an hpack-generated `*.cabal`,
the manifest wins and the generated file is suppressed. No component is
emitted twice, and the document records which file was read.

Suppression is keyed on the hpack generation header (`hpack_generated`,
already parsed and already tested). A **hand-written** `*.cabal` beside a
`package.yaml` is not suppressed: it is not hpack output, so the manifest
is not its source.

### C8 — an unread manifest is announced, and defeats the completeness claim

FR-003, FR-005, SC-002, SC-009. A `package.yaml` that is dispatched and not
read puts a reason code into `waybill:graph-completeness`, naming the file
and why, and the value is not `complete`.

Measured today: `waybill:graph-completeness=complete` over **0 components**,
with no diagnostic anywhere. `compute_graph_completeness` returns
`trivially_complete()` for an empty set — true of a dependency-free project,
false of one whose manifest went unread, and indistinguishable at that point
in the code. The fact must therefore be *carried* from the reader; it cannot
be inferred at the completeness pass.

### C9 — an unmodelled construct is announced in the document

FR-001b, SC-010. A manifest using `defaults:` or `!include` yields a
document naming the construct and the file.

Partial-but-says-so, rather than complete-looking-and-not. The rejected
alternative — refusing any manifest with an unmodelled construct — would
emit nothing for the many real projects that use `when:` for
OS-conditional dependencies, which is #1278 again with a louder failure
mode.

### C10 — no network, under any manifest

FR-001c, SC-010. A scan of a `defaults:`-using manifest performs zero
network requests. Verified by asserting on the document **and** by running
under `--offline`, because a pass that silently skipped itself offline
would satisfy the first assertion alone.

### C11 — an optional mark is withdrawn only on non-seed reach

FR-006, SC-003. A mark is withdrawn only when a closure seeded from the
declared set **with the marked names removed** reaches the component.

Measured today on the public library: 2 of 2 directly-marked components lose
their mark, and 10 components are reachable from the root only through those
2 — the pair plus 8 non-boot components below it.

The negative half carries the defect: a document MUST NOT withdraw a mark
whose only justification is the marked component's own presence in the seed.

### C12 — a withdrawal names what justified it

FR-007. Each withdrawal records the non-seed parent whose closure edge
reached the component, in the document.

### C13 — "ran and withdrew nothing" is distinguishable from "did not run"

FR-008, FR-017. The document distinguishes three states: the reconciliation
did not run; it ran and withdrew nothing; it ran and withdrew N.

These are one state today — `restored=0`, or no record at all — which is
the condition under which a feature that *cannot* fire looks like a feature
with nothing to do. Milestone 973 exists because milestone 926 made exactly
this mistake with the closure summary, and `ClosureSummary`'s own doc
comment records it.

### C14 — a declined withdrawal keeps both the scope and its derivation

FR-009. Where a withdrawal is declined, the component keeps
`lifecycle-scope=optional` **and**
`waybill:optional-derivation=cabal-flag-disabled-stanza`.

Restoring one without the other publishes a scope with no account of where
it came from.

### C15 — a resolved component carries the license the cached set declares

FR-010, SC-004. A component resolved from the cached package set carries
that set's license.

Measured today: **0 of 66**. Probe B measured that every derivation block in
the file declares one, so the current figure is not a coverage ceiling in
the source — nothing reads the field.

### C16 — an uncanonicalisable license is preserved, and flagged

FR-011. A declaration that will not canonicalise appears as declared, and
the document records that it is uncanonicalised rather than presenting it
as a canonical expression.

Measured population: 586 unversioned strings (`"GPL"`, `"LGPL"`, `"AGPL"`)
plus `publicDomain` (313), which has no clean SPDX identifier.

### C17 — a declaration that the license is unknown yields no license

FR-011a, FR-012. `"unknown"` (360 of 994 bare strings) produces an absent
license field, not the string `unknown` in `licenses[]`.

Publishing it would be worse than absence: a consumer filtering for
components with no license information would not find it, so the document
would answer "what is this licensed under" with a value that is not a
license.

### C18 — no license is invented

FR-012. A component the resolution did not resolve carries no license from
this pass.

### C19 — resolved and unresolved-reason are mutually exclusive

FR-013, SC-005. No component carries both a resolved version and
`waybill:unresolved-reason`.

Measured today: **2**. The reason is not wrong, it is stale — the reader
stamps it correctly when there is no version, and the nixpkgs pass later
fills the version and leaves the annotation. So the fix belongs where the
fact changes, not in a sweep at emission time.

### C20 — an unresolved component says why, and distinguishes the two whys

FR-014. A component the pass did not resolve carries a reason, and
compiler-supplied is distinguishable from undeterminable.

`pretty` at the measured revision is the live case: nixpkgs binds it to
`null` in `configuration-ghc-9.10.x.nix`, so it is supplied by the compiler
and present in every build. Reporting that as "could not be determined"
would tell an operator to go and find something that does not exist.

### C21 — the hpack warning's advice is true

FR-004a. The disagreement warning no longer advises regenerating the
`*.cabal`.

Under FR-002 waybill reads the manifest, so regenerating cannot change the
document. Leaving the text would instruct an operator to perform a step that
cannot affect the output, which is worse than silence because it looks like
a remedy.

### C22 — the disagreement stays visible

FR-004. A `package.yaml` disagreeing with its generated `*.cabal` remains
reportable.

Measured: `emit_hpack_warnings` iterates `cabal_manifests`, so
`package_yaml_paths` can only *filter* that loop and a `package.yaml` with
no sibling can never reach it. FR-001's gap and this behaviour are separate
code paths; fixing one does not fix the other.

### C23 — every signal is in the document, and also logged

FR-018, FR-020, SC-009. Every condition C8–C14 and C16–C17 introduce is
observable by reading the emitted document alone, with no access to the
scan log. Each is additionally logged, so an operator can act before the
SBOM is published rather than only discover it afterwards.

Asserted on the document, never on captured logs. A log-only diagnostic
leaves the artifact exactly as dishonest as the defect being fixed.

---

## Non-regression contracts

### C24 — no new catalog row, or an extractor ships with it

FR-019. Every signal lands in `waybill:graph-completeness` (via a new
`ReasonCode` variant, which renders through the existing value),
`waybill:unresolved-reason` (C151), or the nixpkgs document-scope records
(C173/C175). All already have extractors.

If a row is nonetheless added, its extractor ships in the same change.
`parity::extractors::tests::every_catalog_row_has_an_extractor` and
`holistic_parity` enforce this, and a documentation row ahead of emission
code is a known way to break them.

### C25 — every other ecosystem is byte-identical

FR-015, SC-007. The development-axis and optional-axis output of every
ecosystem this milestone does not name is unchanged.

R7 names the live hazard. Milestone 1272 measured that folding an extra
section into a runtime seed silently reassigns a component from Optional to
Development — a change to the *development* axis, forbidden by that
milestone's FR-010. This milestone touches an optional seed set, so the
same failure shape is available: the re-seeded closure removes the marked
names and changes nothing else about the seed.

### C26 — every diagnostic is reachable

FR-017. Each diagnostic added here has a committed fixture whose sole
purpose is to trigger it.

Not a wish. US1 and US2 are both instances of a signal no input could
produce, and `emit_hpack_warnings` is a measured case of a warning wired
for one branch only. Mutation-testing (SC-006) is what proves the fixture
is not vacuous — a passing test may exercise nothing.

### C27 — `haskell-aeson` is the corpus control

FR-016, Probe E. The `haskell-aeson` golden has no `flake.lock` and no
`package.yaml` at its pinned revision, so no code path this milestone
touches can reach it.

A refresh that changes `haskell-aeson` is evidence of unintended blast
radius, not of this milestone working. The other three Haskell/Nix targets
are expected to change, and `nix-closure-moat` — a root `package.yaml` with
no sibling `*.cabal` — is a live witness to #1278 already under version
control.
