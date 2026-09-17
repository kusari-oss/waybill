# Feature Specification: Trustworthy `.cabal` dependency parsing

**Feature Branch**: `895-fix-cabal-parser`
**Created**: 2026-09-16
**Status**: Draft
**Input**: User description: "891"

Closes [#891](https://github.com/kusari-oss/waybill/issues/891).

## Context

waybill reads a Haskell project's declared dependencies from its `*.cabal`
file. On a project with no lockfile — no `stack.yaml.lock`, no
`cabal.project.freeze` — that file is the only source, so every component in
the emitted SBOM comes through this path.

It currently produces components that do not exist. Scanning a real
cabal-only project emitted 24 components of which two were pure parse
garbage and a third carried a malformed name. The names were assembled from
adjacent configuration lines and source-code comments that the parser
mistook for dependency declarations.

A fifth defect sits alongside those four, found while analysing this plan
rather than in the original report: only the **first** dependency list in
each section is read. Cabal permits a field to repeat, and idiomatic files
group dependencies under comment headings by repeating `build-depends:` —
the corpus target's library section carries five such lists and the system
reads one. That one is a completeness failure rather than an accuracy one,
and by volume it is the larger of the two problems.

The four original defects are a Principle IX violation (Accuracy — no false
positives), not a coverage gap. A fabricated component is worse than a missing one: a
consumer cannot tell it apart from a real dependency, and every downstream
use — vulnerability matching, licence compliance, provenance — inherits the
fiction.

## Clarifications

### Session 2026-09-16

- Q: Identifier shape for a dependency with no resolved version — versionless, or an `unspecified` sentinel? → A: Versionless (`pkg:hackage/<name>`), matching cargo / gem / pip / nuget / cmake / vcpkg. The PURL spec makes the version segment optional, and a sentinel puts a non-version token in the version slot — a milder form of the defect this feature exists to fix.
- Q: How should a build-tool dependency be represented? → A: Emit `pkg:hackage/<package>` (the package half, which is a genuine Hackage coordinate) scoped build-time via the existing `LifecycleScope::Build`, with the executable name recorded as an annotation. Keeps the identifier resolvable against the registry, so vulnerability matching continues to work, and reuses vocabulary that already exists rather than inventing an identifier shape.
- Q: What happens to a dependency list that is only partly readable? → A: Skip only the entries that cannot be read, emit the rest, and report how many were skipped. The defect being fixed is emitting a guess, not emitting less; a cleanly-parsed sibling entry is not a guess, and discarding it trades a Principle IX violation for a Principle VIII one.
- Q: How is SC-002 verified, given no Haskell target exists in either corpus? → A: Add one to the public corpus, sourced from a `kusari-sandbox` fork as `pants-example-javascript` already is. This feature is specifically about fabricated components, which is what whole-document comparison catches and unit tests do not; the ecosystem currently has no such coverage.


## User Scenarios & Testing *(mandatory)*

### User Story 1 - Nothing is invented (Priority: P1)

Someone scans a Haskell project that has no lockfile. Every component in the
resulting SBOM corresponds to a dependency the project actually declares.
Nothing is assembled out of configuration lines, comments, or whitespace
that happened to sit next to a dependency list.

**Why this priority**: This is the accuracy violation. Until it holds, the
document actively misinforms, and the other stories are improvements to
output that cannot be trusted anyway. It is also the only story that can
make the SBOM *wrong* rather than merely *incomplete*.

**Independent Test**: Scan a `.cabal` file whose dependency lists are
followed by ordinary cabal fields and comment lines. Compare the emitted
component names against the dependency names written in the file — the two
sets match exactly.

**Acceptance Scenarios**:

1. **Given** a dependency list whose last entry is followed on the next line
   by another indented configuration field, **When** the project is scanned,
   **Then** the emitted component is the dependency alone and carries no
   trace of the following field.
2. **Given** a dependency list followed by a commented-out line, **When** the
   project is scanned, **Then** the comment contributes nothing to any
   component.
3. **Given** a build-tool dependency list immediately followed by a regular
   dependency list, **When** the project is scanned, **Then** neither list's
   contents leak into the other's entries.
4. **Given** a `.cabal` file declaring N distinct dependency names across all
   its sections, **When** the project is scanned, **Then** every emitted
   Haskell component's name is one of those N names.

---

### User Story 2 - A dependency is identified by what it is, not by what version it wants (Priority: P2)

Someone looks up a component from the SBOM against a package registry, or
feeds the document to a vulnerability scanner. The component's identifier
names the package. The version constraint the project declared is still
present in the document, but it is carried as a constraint, not as if it
were the resolved version.

**Why this priority**: Every component from this path is affected, so the
impact is broad — but the identifiers are wrong in a *recognisable* way (a
reader can see `>=4.11 && <4.22` is not a version), whereas Story 1's
failures are indistinguishable from real data. Broad and visible ranks below
narrow and invisible.

**Independent Test**: Scan a project whose dependencies all carry version
constraints. Every emitted identifier resolves against the package registry;
every constraint appears in the document as declared.

**Acceptance Scenarios**:

1. **Given** a dependency declared with a version constraint, **When** the
   project is scanned, **Then** the component's identifier names the package
   without the constraint embedded in it.
2. **Given** that same dependency, **When** the project is scanned, **Then**
   the declared constraint is present in the document verbatim.
3. **Given** the same package declared with different constraints in two
   sections of one file, **When** the project is scanned, **Then** one
   component is emitted carrying both constraints, not two components with
   differing identifiers.
4. **Given** a dependency declared with no constraint at all, **When** the
   project is scanned, **Then** its identifier is formed the same way as one
   that had a constraint, and the two are distinguishable only by the
   constraint record.

---

### User Story 3 - A build tool is not a library (Priority: P3)

Someone filters the SBOM for the project's library dependencies, or for the
executables its build requires. Entries that a cabal file declares as build
*tools* are distinguishable from entries it declares as library
dependencies.

**Why this priority**: Smallest blast radius — most cabal files declare no
build tools at all — and the current output is malformed in a way a reader
notices immediately rather than one that silently misleads. Worth doing, not
worth blocking the other two.

**Independent Test**: Scan a project declaring both a build-tool dependency
and a library dependency, and confirm a consumer can tell which is which
without consulting the original `.cabal` file.

**Acceptance Scenarios**:

1. **Given** a build-tool dependency declaration, **When** the project is
   scanned, **Then** the emitted identifier names the package alone and is
   one a consumer can look up in the registry.
2. **Given** a file declaring both kinds, **When** the project is scanned,
   **Then** the build tool is marked build-time and the library dependency
   is not.
3. **Given** a build-tool declaration naming an executable, **When** the
   project is scanned, **Then** the executable's name is recoverable from
   the emitted document.
4. **Given** a package declared as both a build tool and a library
   dependency in one file, **When** the project is scanned, **Then** the
   result does not claim the package is exclusively build-time.

---

### Edge Cases

- A dependency list whose entries are separated by leading commas at the
  start of continuation lines, rather than trailing commas.
- A dependency list that is the final field in the file, with no following
  field and no trailing newline.
- A section containing a conditional block (`if flag(...)`) with its own
  nested dependency list at deeper indentation.
- A section that repeats `build-depends:` under separate comment headings.
- A dependency list nested inside an `if` conditional within a section.
- A list in which one entry is unreadable and its siblings are fine — the
  siblings must survive.
- A list in which every entry is unreadable — the reported skip count must
  account for all of them.
- A file using tabs, or mixed tabs and spaces, for indentation.
- A file with Windows line endings.
- An empty dependency list — the field is present with nothing under it.
- A comment appearing *between* two dependency entries rather than after the
  last one.
- The same package named in both a build-tool list and a regular dependency
  list.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST treat a dependency list as ending where the
  cabal format says it ends, rather than only at a blank line, a line
  starting in the first column, or the end of the file.
- **FR-001a**: The system MUST read EVERY dependency list in a section, not
  only the first. Cabal permits a field to be repeated, and grouping
  dependencies under comment headings by repeating `build-depends:` is
  idiomatic.
- **FR-001b**: The system MUST read dependency lists nested inside
  conditional blocks, and MUST attribute them to the enclosing section.
- **FR-002**: The system MUST ignore comment lines when reading a dependency
  list, wherever in the list they appear.
- **FR-003**: The system MUST NOT emit a component whose name or version was
  derived from text outside the dependency list it was read from.
- **FR-004**: Every Haskell component the system emits from a `.cabal` file
  MUST correspond to a dependency name declared in that file.
- **FR-005**: The system MUST NOT place a version constraint expression in
  the slot reserved for a resolved version. Where no version has been
  resolved, the identifier MUST omit the version segment entirely rather
  than carry a placeholder token.
- **FR-006**: The system MUST record a declared version constraint in the
  emitted document, verbatim as written.
- **FR-007**: Where the same package is declared more than once in one file
  with differing constraints, the system MUST emit a single component
  carrying every declared constraint.
- **FR-008**: The system MUST form identifiers identically for dependencies
  with and without a declared constraint, so that the two are not
  accidentally treated as different packages. Both are versionless; the
  constraint is visible only in the constraint record.
- **FR-009**: The constraint record — the `waybill:requirement-ranges`
  annotation, catalogued as C20 — MUST be carried consistently across
  every output format the system supports, and MUST be registered in the
  project's format-mapping catalogue — it becomes the sole carrier of
  information previously (incorrectly) visible in the identifier.
- **FR-010**: The system MUST distinguish build-tool dependencies from
  library dependencies in the emitted document, by marking them build-time
  using the lifecycle vocabulary the system already carries for that purpose.
- **FR-010a**: A build-tool dependency's identifier MUST name the package,
  not the package-and-executable pair, so that it remains resolvable against
  the package registry.
- **FR-010b**: The executable a build-tool dependency names MUST be
  recoverable from the emitted document, so the package-and-executable pair
  the project declared is not lost.
- **FR-011**: Scanning a project whose `.cabal` file the system already
  parsed correctly MUST NOT change that project's emitted components, apart
  from the identifier and constraint corrections FR-005 through FR-008
  require.
- **FR-012**: When the system cannot read an individual dependency entry,
  it MUST emit nothing for that entry rather than emit a guess. Entries in
  the same list that read cleanly MUST still be emitted.
- **FR-012a**: The system MUST report how many entries it skipped this way,
  in a form an operator can see, so that silent under-reporting is
  distinguishable from a project that genuinely declares fewer dependencies.
- **FR-012b**: The count MUST be reported even when it is zero, so that "the
  file was fully readable" and "the count is missing" remain distinguishable.

### Key Entities

- **Declared dependency**: a package name a project's build configuration
  names as required, optionally with a version constraint and a declaration
  kind (library dependency or build tool). It is a *statement of intent* —
  it does not by itself say which version will be used.
- **Version constraint**: the range expression a project writes next to a
  dependency name. Distinct from a resolved version: a constraint describes
  an acceptable set, a version names one member of it.
- **Dependency list**: a bounded region of a build configuration file
  containing declared dependencies. Its boundaries are the thing currently
  misread.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Scanning the reproducer in #891 emits exactly four components
  — `waybill-fixture-core`, `waybill-fixture-vec`, `waybill-fixture-cmt` and
  `waybill-fixture-tool` — whose names are exactly the four distinct
  dependency names the file declares, `core` appearing twice in the file and
  deduplicating to one component. Today it also emits four, but three of them
  are malformed and one of those names does not exist as a package.
- **SC-002**: A real cabal-only Haskell project is present in the public
  corpus at a pinned revision, and the count of its emitted component names
  that do not appear anywhere in its `.cabal` files is **zero**, checked on
  every nightly run rather than once. Today, on the project that prompted
  this issue, that count is two of twenty-four.
- **SC-002a**: On a real project whose sections repeat `build-depends:` and
  nest it inside conditionals, the count of declared dependency names the
  system fails to emit is **zero**. Measured on the intended corpus target:
  its library section carries five such lists, of which the system currently
  reads one.
- **SC-003**: Every identifier emitted from a `.cabal` scan is a
  syntactically valid package URL whose name segment is the declared package
  name, carrying no version constraint and no placeholder token. Whether the
  package exists upstream is deliberately NOT asserted — that would need a
  registry the scanner does not contact, and the plan forbids network access.
- **SC-004**: Every version constraint written in a scanned `.cabal` file is
  recoverable from the emitted document, in every output format.
- **SC-005**: A project whose `.cabal` file contains no dependency lists, or
  whose lists the system already read correctly, emits an unchanged set of
  component names before and after this change.
- **SC-006**: On a file with one unreadable entry among N, exactly N-1
  components are emitted and the reported skip count is 1. No component is
  emitted for the unreadable entry, and no sibling is lost.
- **SC-007**: The nightly public-corpus lane passes with the new Haskell
  target installed, and its goldens were produced by CI rather than locally.
- **SC-008**: The parser's behaviour is pinned by tests covering each edge
  case enumerated above, and each new test fails against the current
  implementation for the reason it was written — not merely because the
  implementation is absent.

## Assumptions

- Scope is the `.cabal` dependency-reading path only. The lockfile-driven
  paths (`stack.yaml.lock`, `cabal.project.freeze`) resolve to concrete
  versions and are unaffected; they are not touched by this feature.
- A declared dependency with no resolved version is identified without a
  version segment, per the clarification above. This is the convention the
  cargo, gem, pip, nuget, cmake and vcpkg readers already use; the Haskell
  reader's current `@unspecified` output changes accordingly, so its
  existing tests move with it.
- The other readers still emitting an `unspecified` sentinel (Elixir,
  Erlang, CocoaPods, and the Go workspace path) are out of scope. Converging
  them is worth doing but is not a Haskell parsing fix, and bundling it
  would put a cross-ecosystem change behind a bug-fix ticket.
- No Haskell project appears in either corpus today, so no *existing*
  committed golden is expected to change. This is verified, not assumed, and
  if it turns out false the affected goldens are refreshed through the
  documented CI procedure rather than locally.
- This feature adds a Haskell target to the public corpus (per the
  clarification above). Its goldens are generated in CI, never locally — they
  embed runner-absolute paths, and generating them on a developer machine
  produces goldens that pass only there.
- The corpus target must come from an ecosystem community organisation (the
  `haskell` org, the same category as `rust` or `pantsbuild`) rather than
  from a company, per the project's external-name policy, and be mirrored to
  a `kusari-sandbox` fork so the pin cannot move underneath the gate. It must
  **not** be the project that surfaced this bug: that one is company-published,
  and being open source does not exempt it.
- Adding a corpus target is scope this feature deliberately accepts. It is
  the only mechanism in the repository that compares whole documents across
  targets, and an ecosystem with none has no protection against precisely
  the class of defect this feature fixes.
- Marking build tools build-time is a deliberate, visible change: consumers
  filtering for runtime dependencies stop seeing them. This is the correct
  direction — a build tool is not a runtime dependency — and it is the same
  asymmetry the project applies elsewhere, where over-reporting is preferred
  to hiding something a consumer is filtering for.
- Constraint expressions are recorded as written rather than normalised or
  evaluated. Deciding which versions a constraint admits requires a registry
  and a resolver, and is out of scope.
- Reading a `.cabal` file that hpack generated from a `package.yaml` remains
  the supported path; parsing `package.yaml` directly is out of scope.
- `common` stanza inheritance via `import:` is out of scope. The intended
  corpus target does not use it (measured: zero occurrences); a target that
  does would need its own feature rather than a widened edge case here.
- Dependency names containing characters the parser might treat as
  separators are out of scope: the cabal package-name grammar does not admit
  them, so the case is hypothetical rather than observed.
- The reproducer and any fixtures use synthetic package names per the
  project's fixture policy, not real Hackage coordinates.
