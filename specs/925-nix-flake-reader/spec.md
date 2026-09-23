# Feature Specification: Read Nix flake.lock inputs as pinned components

**Feature Branch**: `925-nix-flake-reader`
**Created**: 2026-09-23
**Status**: Draft
**Input**: User description: "let's now work on the nix stuff"

## Context

waybill has no Nix reader. On a Nix-built repository it claims none of the Nix
files, so the artefact that actually pins the build is absent from the emitted
document.

Measured with `waybill repo report` on a public cabal + hpack + Nix library:

```
files_walked:    70
files_claimed:    5
files_unclaimed: 65
```

Among the unclaimed: `flake.nix`, `flake.lock`, `default.nix`, `shell.nix`,
`moat.nix`, and the `nix/` directory.

`flake.lock` is a content-addressed lockfile. It pins every input by exact
revision with a hash:

```json
"nixpkgs": {
  "locked": {
    "type": "github", "owner": "NixOS", "repo": "nixpkgs",
    "rev": "a799d3e3886da994fa307f817a6bc705ae538eeb",
    "narHash": "sha256-3av0pIjlOWQ6rDbNOmpUSvbNnJkGORQKKjb4LtCZsIY=",
    "lastModified": 1780749050
  },
  "original": { "type": "github", "owner": "NixOS", "repo": "nixpkgs",
                "ref": "nixos-unstable" }
}
```

That is a stronger pin than most lockfiles waybill already reads, and it is
currently discarded. For a repository whose build inputs come from a flake, a
consumer asking "what was this built against" has no answer in the document.

This feature covers **reading the lockfile**. Using the pinned inputs to resolve
downstream package versions (issue #947 — resolving Haskell package versions
through the pinned nixpkgs) is explicitly out of scope; see Out of Scope.

## Clarifications

### Session 2026-09-23

- Q: How should a flake input be identified, given purl has no `nix` type? → A: Host-typed where the input type has a purl equivalent and a revision is known (`pkg:github/<owner>/<repo>@<rev>`, gitlab, sourcehut); `pkg:generic/<name>@<rev>` with the upstream URL carried as source annotations otherwise; `path` and `indirect` inputs not emitted. Mirrors the milestone-128 FR-002a decision for Yocto `SRC_URI` + `SRCREV`, which went host-typed because OSV's commit and ecosystem queries return advisories directly against host-typed PURLs. A native `pkg:nix` type is unresolved upstream and is tracked separately for research.
- Q: How should a flake input's `narHash` be represented, given it covers a NAR serialization rather than file bytes? → A: Record it verbatim in a `waybill:` annotation that names what it covers, and emit no native checksum field. Emitting it as a native checksum would assert that the component's content hashes to that value, which is false for any consumer that verifies it, and the encoding differs too (SRI base64 vs hex). Accuracy (Principle IX) outranks native-first (Principle V) where the native field would carry a false statement.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The build's pinned inputs appear in the SBOM (Priority: P1)

An operator scans a repository containing `flake.lock`. Every input the lockfile
pins appears in the emitted SBOM as a component identified by its exact
revision, so a consumer can tell what the build was pinned to without reading
the repository.

**Why this priority**: This is the whole of the feature's value and requires
nothing external — no network, no Nix installation, no evaluation. It turns the
single most load-bearing file in a Nix build from invisible into stated.

**Independent Test**: Scan a fixture containing only `flake.lock` and assert the
emitted document contains one component per locked input, each carrying the
pinned revision.

**Acceptance Scenarios**:

1. **Given** a repository with a `flake.lock` pinning one input, **When** the
   operator scans it, **Then** the SBOM contains a component identifying that
   input at its locked revision.
2. **Given** a `flake.lock` pinning several inputs, **When** the operator scans
   it, **Then** every locked input appears exactly once.
3. **Given** a repository with `flake.nix` but no `flake.lock`, **When** the
   operator scans it, **Then** no input components are emitted and the absence
   is recorded as a reason rather than passing silently.

---

### User Story 2 - The document says what depends on those inputs (Priority: P2)

The emitted inputs are connected to the thing that consumes them, so they are
reachable from the document root rather than appearing as unattached
components.

**Why this priority**: Unreachable components degrade graph completeness and
read as noise. This is what makes US1's components usable rather than merely
present, but US1 delivers value without it.

**Independent Test**: Scan a fixture with a `flake.lock` and assert each input
component is reachable from the document root by walking dependency edges.

**Acceptance Scenarios**:

1. **Given** a `flake.lock` whose root node declares inputs, **When** the
   operator scans it, **Then** each input is reachable from the document root.
2. **Given** a `flake.lock` where one input declares its own inputs, **When**
   the operator scans it, **Then** the transitive relationship is expressed
   rather than flattened.

---

### User Story 3 - What was asked for is distinguishable from what was resolved (Priority: P3)

A flake input records both what the author wrote (`original`, e.g. the branch
`nixos-unstable`) and what it resolved to (`locked`, an exact revision). Both
are recorded, so a reader can see that a moving reference was pinned and to
what.

**Why this priority**: This is the same distinction waybill already draws
between a declared constraint and a resolved version. It is genuinely useful for
answering "would re-locking move this?", but the pinned revision alone satisfies
the primary question.

**Independent Test**: Scan a fixture whose `original` is a branch and whose
`locked` is a revision, and assert both are recoverable from the emitted
document.

**Acceptance Scenarios**:

1. **Given** an input whose `original` names a moving branch, **When** the
   operator scans it, **Then** both the branch and the locked revision are
   recoverable.
2. **Given** an input whose `original` already names an exact revision, **When**
   the operator scans it, **Then** no spurious difference is reported.

---

### Edge Cases

- **A `flake.lock` that is present but unparseable.** Must not abort the scan or
  silently drop the repository's other ecosystems. Warn, name the file, continue
  — consistent with how every other reader handles a malformed lockfile.
- **A lockfile with no inputs** (`root` declares none). Emits nothing and is not
  an error; it is a truthful statement that nothing is pinned.
- **An input of type `path`** — a local directory, not a fetched artefact. It
  has no upstream identity to publish and should not be presented as an external
  dependency.
- **An input using `follows`** — an alias to another input rather than an
  independent pin. Must not emit a second component for the same underlying
  revision.
- **A `flake.lock` schema version other than the one measured (`version: 7`).**
  The format is versioned; an unrecognised version must be reported rather than
  parsed on optimistic assumptions.
- **A repository with `flake.nix` and `default.nix` but no lock.** Nothing is
  pinned, so nothing may be claimed as pinned.
- **Several `flake.lock` files in one repository** (a monorepo with independent
  flakes). Each governs its own directory — the scoping lesson from #938.

## Requirements *(mandatory)*

### Functional Requirements

**Standards-native audit (Principle V).** Before any `waybill:` annotation is
proposed, the audit result: a flake input is a *dependency on a versioned
artefact*, which every target format already models natively as a component with
an identifier, a source location and a relationship to its consumer. No new
annotation is required for identity, location, or the dependency edge. Two
things are **not** natively expressible and are the subject of FR-006 and
FR-009: the `original`-versus-`locked` distinction, and the fact that a flake
input's hash covers a NAR serialization rather than the bytes of a file.

- **FR-001**: System MUST discover `flake.lock` files during a filesystem scan.
- **FR-002**: System MUST emit one component per input the lockfile pins,
  identified by the input's locked revision.
- **FR-003**: System MUST NOT emit a component for an input that resolves to a
  local path, which has no published upstream identity.
- **FR-004**: System MUST NOT emit duplicate components for inputs that alias
  the same underlying pin via `follows`.
- **FR-005**: System MUST record each emitted input's source location — the
  upstream it was fetched from — using the target format's native field for
  that purpose.
- **FR-006**: System MUST record the `original` reference alongside the locked
  revision when the two differ, so a moving reference that has been pinned is
  visible as such.
- **FR-007**: System MUST express the relationship between the consuming project
  and each input, such that every emitted input is reachable from the document
  root.
- **FR-008**: System MUST express an input's own declared inputs as
  relationships rather than flattening them, when the lockfile records them.
- **FR-009**: System MUST NOT present a flake input's `narHash` as though it
  were a checksum of the component's file content, and MUST NOT emit it in any
  target format's native checksum field. A `narHash` is an SRI-encoded SHA-256
  over a NAR *serialization of a directory tree*; every native checksum field
  means a hash over the component's bytes, and expects hex rather than base64.
  The mapping would therefore be wrong in both semantics and encoding, and a
  consumer that verified it would be misled by a value that looks correct.
- **FR-009a**: System MUST record the `narHash` verbatim in an annotation that
  names what the hash covers, so the lockfile's only integrity evidence is
  preserved without being misstated. This is a deliberate departure from
  Principle V (standards-native first), justified by Principle IX (accuracy):
  the native field exists but would carry a false statement. The Principle V
  audit above records that this is one of exactly two facts the target formats
  cannot express natively.
- **FR-009b**: The annotation introduced by FR-009a MUST be registered in the
  format-mapping catalogue with a matching extractor for each of the three
  emitted formats, or the existing parity tests fail by construction. Any new
  `waybill:` annotation carries this obligation.
- **FR-010**: System MUST tolerate an unparseable or unrecognised-version
  `flake.lock` without aborting the scan, logging a warning that names the file.
- **FR-011**: System MUST treat each `flake.lock` as governing its own
  directory, not the whole repository.
- **FR-012**: System MUST record why no inputs were emitted when a flake is
  present but unlocked, rather than emitting nothing silently.
- **FR-013**: Identifiers MUST be derivable from the lockfile alone, with no
  network access and no Nix installation.
- **FR-013a**: An input whose type has a purl equivalent AND whose revision is
  known MUST be identified with that host-typed identifier — `github`, `gitlab`
  and `sourcehut` inputs as `pkg:github/<owner>/<repo>@<rev>` and the
  corresponding types. This follows milestone-128 FR-002a, which made the same
  choice for Yocto `SRC_URI` + `SRCREV` on the measured grounds that OSV's
  commit and ecosystem queries return advisories directly against host-typed
  PURLs; a generic identifier forfeits that.
- **FR-013b**: An input with no host-typed equivalent (`git`, `tarball`) MUST be
  identified as `pkg:generic/<name>@<rev>` with the upstream URL recorded via the
  existing source-url / source-type annotation channel, matching the Pants
  non-registry-artifact precedent.
- **FR-013c**: The purl specification has no `nix` type, and whether a canonical
  one is even expressible is unresolved upstream — several flake references can
  denote the same package, which is in tension with the version-range matching
  vulnerability scanners perform. System MUST NOT invent a `pkg:nix` identifier
  while that is unsettled. Tracked for research separately; if an upstream type
  is standardised, FR-013a/b become a migration rather than a rewrite, because
  both emit spec-conformant identifiers today.
- **FR-014**: System MUST claim the `flake.lock` file in `waybill repo report`,
  so a Nix repository stops reporting it as unrecognised.

### Key Entities

- **Flake lockfile** — a versioned document mapping input names to resolved
  pins. Owns a schema version and a root node naming the project's direct
  inputs.
- **Flake input (locked)** — one resolved pin: a type, an upstream identity, an
  exact revision, and a NAR hash. The unit that becomes a component.
- **Flake input (original)** — the reference as the author wrote it, which may
  be a moving branch or tag. The unit that explains what the pin resolved *from*.
- **Input edge** — the relationship between a consumer (the project root, or
  another input) and an input it declares.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a repository whose build is pinned by a flake, the number of
  pinned inputs a reader can identify from the emitted SBOM goes from zero to
  all of them. Measured on the reference repository: 0 → 1.
- **SC-002**: Scanning a repository containing a `flake.lock` requires no
  network access and no Nix installation; the scan produces identical output
  offline and online.
- **SC-003**: Every emitted input component is reachable from the document root;
  the count of unreachable input components is zero.
- **SC-004**: A repository containing a `flake.lock` reports zero Nix files as
  unrecognised in `waybill repo report`, against 6 today on the reference
  repository.
- **SC-005**: Adding a `flake.lock` to a repository never reduces the number of
  components emitted for any other ecosystem — the property #937 and #938 were
  both violations of.
- **SC-006**: Two scans of an unchanged repository emit byte-identical output,
  including input ordering.
- **SC-007**: A malformed `flake.lock` leaves every other ecosystem's component
  count unchanged from a scan of the same tree with the file removed.

## Out of Scope

- **Resolving downstream package versions through a pinned input** (issue #947).
  Measured as valuable — 12 of 19 dependencies on the reference repository
  resolve to exact versions and source hashes from the pinned nixpkgs revision —
  but it requires network access, a cache keyed by revision, and a decision
  about the 7 dependencies that are GHC boot libraries whose version belongs to
  the compiler rather than to nixpkgs. Separate feature.
- **Evaluating Nix expressions.** `flake.nix`, `default.nix`, `shell.nix` and
  `*.nix` derivations are not parsed. Only the lockfile, which is JSON, is read.
- **Invoking the `nix` binary.** No subprocess, consistent with SC-002.
- **Non-flake Nix pinning** (`niv`, `npins`, a vendored `nixpkgs.json`). Distinct
  formats; worth their own assessment once this lands.

## Assumptions

- The lockfile is JSON with a `version` field, a `nodes` map, and a `root` key
  naming the entry node. Confirmed against a real `flake.lock` at `version: 7`.
- `locked` is authoritative for identity; `original` is explanatory. A lockfile
  without a `locked` block for an input is malformed.
- `lastModified` is metadata about the upstream, not about the scan, and is not
  required for identity.
- Flake inputs are a *source-tier* fact: the lockfile states what the build
  resolves to, which is a stronger claim than a declared range and a weaker one
  than an observation of a built artefact.
- Repositories may contain several independent flakes; per-directory scoping
  follows the rule established for Haskell lockfiles in #938.
