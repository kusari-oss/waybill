# Phase 0 Research: Nix flake.lock reader

All findings below are measured against real lockfiles, not inferred from
documentation. Probe commands are recorded so they can be re-run when the format
moves — an undocumented shape is one that can change without notice.

## Sample set

Five `flake.lock` files: the reference repository used throughout #937/#936/#938
plus four public community flakes (`nix-community/home-manager`,
`nix-community/nixvim`, `nix-community/disko`, `NixOS/nixos-hardware`).

```sh
curl -sfL "https://raw.githubusercontent.com/<org>/<repo>/master/flake.lock"
```

Deliberately small and community-sourced. The purpose is to establish which
*shapes* occur, not to estimate their frequency in the wild — a five-file sample
cannot support a frequency claim and none is made below.

## R1 — Schema version

**Finding**: all five samples report `version: 7`.

**Decision**: parse `version: 7`; treat any other value as unrecognised and warn
rather than parsing optimistically (FR-010).

**Rationale**: the field exists precisely because the format is expected to
change. Five samples agreeing tells us 7 is current, not that it is the only
version we will ever meet. Failing loudly on an unknown version is cheaper than
misreading one.

**Alternatives considered**: version-agnostic best-effort parsing — rejected,
because a silently misparsed lockfile produces components that look authoritative
and are wrong, which is the failure mode #937 and #943 both had.

## R2 — Input types present

**Finding**: `github` (5 occurrences) and `tarball` (2) across the sample.

A `tarball` input carries **both** a `url` and a `rev`:

```json
{ "type": "tarball",
  "url": "https://releases.nixos.org/nixpkgs/nixpkgs-26.11pre1075288.a32edd765451/nixexprs.tar.zst",
  "rev": "a32edd7654519351e48e80372a928df336394670",
  "narHash": "sha256-j08lBbqaYwjsW0x..." }
```

**Decision**: a `tarball` input is identifiable by revision even though it has no
`owner`/`repo`, so FR-013b's `pkg:generic/<name>@<rev>` applies with the `url`
carried as a source annotation. It is **not** a reason to drop the input.

**Rationale**: the presence of `rev` on a non-host-typed input was not obvious
and materially changes FR-013b — without measuring it, the natural assumption is
that only `github`-type inputs have revisions.

**Alternatives considered**: treating `tarball` as unidentifiable and skipping —
rejected on the measurement; it would discard a pinned nixpkgs on two of five
samples.

## R3 — `follows` aliases

**Finding**: confirmed present. In `nodes.<key>.inputs`, a value is **either**

- a **string** — the key of another node, or
- an **array** — a `follows` path through the input graph, e.g.
  `"nixpkgs-lib": ["nixpkgs"]`

**Decision**: an array-valued entry is an alias and MUST NOT mint a second
component (FR-004). It is resolved to the node it points at and expressed as a
relationship to that node.

**Rationale**: this is the single most important structural finding of the
research. Designing from the reference repository alone — which has one input and
no `follows` — would have produced a reader that treats every `inputs` value as a
node key and either crashes or fabricates components on any real-world flake.

**Alternatives considered**: emitting the alias as its own component — rejected,
it duplicates one underlying pin under two names, which is the defect class #936
was.

## R4 — Nested inputs

**Finding**: confirmed present. A non-root node declares its own inputs
(`flake-parts` declares `nixpkgs-lib`).

**Decision**: express these as relationships between input components (FR-008),
not flattened into the root.

**Rationale**: the input graph is genuinely a graph. Flattening would misreport
who depends on what, and would make the root appear to declare inputs it does
not.

## R5 — `narHash` encoding

**Finding**: SRI form, `sha256-<base64>`, e.g.
`sha256-3av0pIjlOWQ6rDbNOmpUSvbNnJkGORQKKjb4LtCZsIY=`.

**Decision**: carried in an annotation, never in a native checksum field
(spec FR-009/FR-009a, settled in clarification Q2).

**Rationale**: two independent mismatches with every native checksum field —
the hash covers a NAR *serialization of a directory tree* rather than the
component's bytes, and the encoding is base64 rather than hex. A consumer that
verified the native field would be misled by a value that looks correct.

## R6 — Where the reader attaches

**Finding**: the shared-walker reader registry (m664) dispatches by filename
pattern via `ReaderRegistration { reader_id, patterns, state, on_file, on_dir,
descend_into }`.

**Decision**: register one reader matching `flake.lock`. No `on_dir` callback and
no `descend_into` override are required — the file is found wherever the walker
already goes.

**Rationale**: every reader added since m664 attaches this way; a bespoke walk
would reintroduce the per-reader traversal m664 removed.

## R7 — Identifier construction precedent

**Finding**: milestone 128 FR-002a already resolved the host-typed-versus-generic
question for Yocto, at `yocto/recipe.rs`:

> when `SRC_URI` contains a git URI whose host matches {github, gitlab,
> bitbucket, codeberg} AND `SRCREV` is set, emit a host-typed PURL … instead of
> the FR-011 `pkg:generic/...` fallback. OSV's commit + ecosystem queries return
> advisories directly against host-typed PURLs.

**Decision**: follow the same rule (spec FR-013a/b). The existing helper
(`detect_host_typed_purl_inputs`) parses `SRC_URI` **strings**, whereas
`flake.lock` supplies structured `{type, owner, repo}` fields — so this is a
pattern to follow, not a function to call.

**Rationale**: reusing the decision keeps one convention in the codebase and
inherits its measured justification. Re-litigating it would risk two readers
disagreeing about what a git-hosted dependency's identifier looks like.

**Alternatives considered**: a shared helper refactored to accept both shapes —
deferred. It is a worthwhile cleanup but couples this feature to the Yocto
reader's tests for no functional gain; worth revisiting if a third caller appears.

## R8 — Annotation registration obligation

**Finding**: the parity infrastructure asserts that every row in the format
mapping catalogue has a matching extractor for all three formats
(`every_catalog_row_has_an_extractor`, `holistic_parity`).

**Decision**: the FR-009a annotation ships with its catalogue row and three
extractors in the same change (spec FR-009b).

**Rationale**: adding the row without the extractors fails the suite by
construction; adding the annotation without the row leaves it outside the parity
guarantee, so the three formats could drift.

## Open questions deliberately NOT resolved here

- **A native `pkg:nix` PURL type** — unresolved upstream; tracked as a research
  issue. Both identifier shapes this feature emits are spec-conformant today, so
  adopting a standardised type later is a migration of identifier construction
  rather than a rewrite.
- **Resolving package versions through a pinned input** — out of scope per the
  spec; needs network, a revision-keyed cache, and a decision about GHC boot
  libraries.
