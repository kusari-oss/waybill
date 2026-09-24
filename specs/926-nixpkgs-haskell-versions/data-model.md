# Phase 1 Data Model — nixpkgs-resolved Haskell versions (#947)

Entities are in-process for the duration of one scan, matching every reader
milestone since 002. The only persisted state is the per-revision cache (R5).

---

## `PinnedNixpkgs`

The nixpkgs input as `flake.lock` records it. Produced by the existing m925
lockfile reader, not by new parsing.

| field | meaning | source |
|---|---|---|
| `revision` | exact commit the lock pins | m925 `NodeRef` → locked `rev` |
| `location` | where to retrieve it from — type, owner/repo or URL | m925 locked entry (**not** assumed upstream, FR-016) |
| `pin_state` | `Exact` / `NamedRef` / `DefaultBranch` | m925 `OriginalPinState` |

**Validation**: only `pin_state == Exact` is resolvable. `NamedRef` and
`DefaultBranch` take the FR-012 no-op path — there is no reproducible revision
to resolve against.

**Identity**: `revision` is the cache key (R5) and the provenance recorded on
every resolved version (FR-007).

---

## `PackageSet`

The name → (version, source hash) mapping carried by one revision. Measured at
19,058 unique names from 19,437 derivation blocks (R1).

| field | meaning |
|---|---|
| `entries` | `name → PackageSetEntry` |
| `revision` | the revision it came from |

**Lifecycle**: retrieved once per revision, cached, then read-only.
Later definitions of the same name shadow earlier ones (Nix attrset
semantics), so construction is last-wins.

### `PackageSetEntry`

| field | meaning | notes |
|---|---|---|
| `version` | exact version | from `version = "…"` |
| `source_hash` | SHA-256 of the source tarball | stored as **hex**, converted from Nix base32 at parse time (R2) |

**Validation**: a Nix-base32 value that does not decode to exactly 32 bytes is
rejected and the entry carries no hash rather than a malformed one
(Principle IX). Decoding is verified equal to the tarball's own SHA-256 (R2),
which is what makes the native checksum field honest.

---

## `CompilerConfiguration`

The per-GHC-series record of which packages the compiler supplies. One per
series present at the revision.

| field | meaning |
|---|---|
| `series` | e.g. `9.6.x` |
| `boot_libraries` | names bound to `null` in the **top-level** package-override attrset |

**Validation**: every nulled name belongs here, unfiltered (R3). Two narrower
rules were tried and rejected by measurement — nesting depth drops
`directory-ospath-streaming` (a real package at v0.3), and package-set
membership drops `rts`, `ghc-platform`, `ghc-toolchain` and
`system-cxx-std-lib` (real GHC-bundled packages, absent from the package set
precisely because they are never built from Hackage). `editedCabalFile` stays
in and is inert: this set is only consulted for names a project declared.

**Measured shape**: 35–47 nulled bindings per series at the pinned revision;
across all eight series, 50 in union and 32 in intersection (R3, R4).

---

## `CandidateCompilers`

Which compiler package sets the project might be building against.

| field | meaning |
|---|---|
| `series` | one or more `CompilerConfiguration` series |
| `derivation` | `FlakeAttributePath` (the flake named them) or `AllSeriesAtRevision` (fallback) |

**State transition** (FR-014 / FR-014a):

```
flake names exactly one series   -> resolve against that series' configuration
flake names several              -> boot := union of their boot_libraries
flake names none                 -> boot := union over every series at the revision
```

Taking the **union** of boot libraries is the same statement as "resolve only
what is non-boot in every candidate" — a package nulled in any candidate is
treated as boot. Fail closed, Principle III.

**Measured cost of the union**: 5 genuine packages when the flake names the
target's three series; 18 under the `AllSeriesAtRevision` fallback (R4).

---

## `ResolutionOutcome`

One per declared Haskell dependency. The unit FR-006 and SC-002 are asserted
against.

```
Resolved { version, source_hash, revision }
Unresolved { reason }
```

`reason` is a closed set:

| reason | when |
|---|---|
| `compiler-supplied` | the name is nulled by any candidate compiler (FR-014a) |
| `absent-from-package-set` | reached the revision; the name is not in it |
| `source-unreachable` | could not reach, refused, unauthorized, or timed out (FR-017) |
| `source-unsupported` | the lock pins a shape no single file can be retrieved from (bare git URL, tarball) |
| `no-exact-revision` | the lock pins a moving reference (FR-012) |
| `offline` | operator requested offline (FR-009) |

**Invariant (FR-005, SC-003)**: an `Unresolved` outcome contributes **no**
version, no hash, and no versioned identifier. There is no partial state.

**Invariant (FR-013)**: a version already established by a project-local
lockfile or freeze file wins. When both exist and disagree, the local value is
emitted and the disagreement is recorded — neither is silently dropped.

---

## Emission mapping

| datum | carrier | catalog row | why |
|---|---|---|---|
| resolved version | existing component version field | — | native |
| `source_hash` | **native** `hashes[]` / `checksums[]` / content identifier | — | R2 — verified flat SHA-256 of the tarball, so Principle V requires the native field and forbids a row. Needed **no new emitter code**: all three formats already map `ResolvedComponent.hashes` generically for every reader |
| resolution provenance + revision | `waybill:nixpkgs-resolved-via` | **C169** | no native carrier for "which external package set established this version" |
| `Unresolved.reason` | `waybill:haskell-version-unresolved-reason` | **C170** | FR-006; CDX omits `version` and SPDX 2.3 says `NOASSERTION`, both of which collapse six distinct causes into one |
| candidate compilers considered | `waybill:nixpkgs-candidate-compilers` | **C171** | FR-014b; emitted only when the count exceeds one |
| local-vs-nixpkgs disagreement | `waybill:nixpkgs-version-disagreement` | **C172** | FR-013; the local value wins, and the difference is recorded rather than discarded |
| degradation at document scope | existing degradation annotation channel | — | FR-008; reuse rather than add |

Each new `waybill:` row needs its catalog entry in
`docs/reference/sbom-format-mapping.md` plus three extractors, or
`parity::extractors::tests::every_catalog_row_has_an_extractor` fails.
