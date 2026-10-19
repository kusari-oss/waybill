# Contract: the C45 `waybill:orphan-reason` vocabulary after this feature

**Feature**: `1218-explain-every-orphan` | **Date**: 2026-10-19
**Catalog row**: C45, `docs/reference/sbom-format-mapping.md:97`

This is the wire contract. SBOM consumers key on these strings, so they are
frozen here **before any emission task runs** and may not be changed by an
implementation task. C45's row is the published home; this file is what the
row must come to say.

## The eight values

Five existing, frozen since milestone 167. Three new.

| Value | Meaning | Ecosystems | Since |
|---|---|---|---|
| `flat-attached-fallback` | Incoming edge attribution unknown or synthesized | golang | m061 |
| `unresolved-indirect-require` | Indirect requirement with no reachable same-module sibling | golang | m061 |
| `stale-go-sum-entry` | A version in `go.sum` that the build does not use; a reachable sibling at another version exists | golang | m167 |
| `dead-lockfile-entry` | A lockfile entry at a version the chosen resolution does not use; a reachable sibling at another version exists | npm, **pypi** | m167, **widened here** |
| `hoisted-unused` | Hoisted into the tree but depended on by nothing; no sibling | npm | m167 |
| `build-toolchain` | A compiler, snapshot or toolchain pin. Build tooling, not a dependency of the subject. Requires `design` tier **and** no source files **and** no workspace member — the tier alone is not sufficient | any | **this feature** |
| `foreign-build-system` | Declared by a build system the subject does not build with, co-located in the same repository | any | **this feature** |
| `unreferenced-lock-node` | A node in a lockfile that no other node in that lockfile references | any | **this feature** |

## Why these three names

- **`build-toolchain`** says what the thing *is*, which is what a consumer
  filters on. It deliberately does not say "unresolved": `ghc-9.6.5` is
  pinned exactly, and C151 `waybill:unresolved-reason` asserts a resolution
  boundary was hit, which would be false. C197's row draws that line
  explicitly and this name stays on the correct side of it.
- **`foreign-build-system`** says *why there is no edge* — the subject does
  not build with it — rather than naming the build system. The build system
  is identified separately and machine-readably by C55
  `waybill:source-mechanism`, which FR-002a extends to the pip reader so
  all three components carry it. Encoding the tool into this value would
  grow the enum once per build system forever.
- **`unreferenced-lock-node`** describes the lockfile-internal fact, which
  is what a consumer can act on: the entry is real, waybill read it
  correctly, and nothing in that lockfile points at it. It does **not** say
  the component is unused by the artifact, which waybill cannot know from a
  `flake.lock` alone.

All three are lower-kebab-case, consistent with the existing five.

## What each must NOT be used for

The negative half of the contract. Each of these would make I7 pass while
the document got worse, and each has a named counterexample in the corpus.

| Value | MUST NOT be emitted on | Counterexample |
|---|---|---|
| `dead-lockfile-entry` | a coordinate declared by a manifest, where the workspace resolved a different version | `maven-guice`'s `pkg:maven/com.google.inject/guice@5.1.0` (#1235) |
| any value | a component unreachable only because waybill did not write an edge the source of truth states | `python-flask`'s thirteen `examples/celery/requirements.txt` components (#1234) |
| `foreign-build-system` | a component from the subject's own build system in a sibling module | the same #1235 three; their build system is maven, as is the subject's |
| `build-toolchain` | a component whose version could not be resolved | that is C151's job, and C151 is `design`-tier-only |
| `build-toolchain` | a `design`-tier component that has source files or a workspace member | `haskell-language-server`'s four `docs/requirements.txt` components, already explained by C151 — they are documentation dependencies of a sibling member, and they are not I7 offenders, so marking them moves no count |

The first and third are the same trap from two directions: after the
ecosystem guards are deleted, the maven three satisfy "has a reachable
same-name sibling" and "is co-located in a repository with another build
system". Both arms must exclude them. **SC-004 is the test**, and it asserts
by name that all sixteen deferred components still fail I7.

## Priority order

Total, most-specific first. The three new values sit strictly below every
m167 value, so FR-004 ("every component carrying an m167 code still carries
it") holds by position rather than by case analysis.

```
flat-attached-fallback        (preserved, never overwritten)
stale-go-sum-entry
dead-lockfile-entry
hoisted-unused
unresolved-indirect-require
build-toolchain               (new)
foreign-build-system          (new)
unreferenced-lock-node        (new)
```

C45's row already documents m167's order; this replaces it wholesale rather
than appending, so there is one order in one place.

## Emission invariants

1. **One value per component.** The order is total; the first matching arm
   wins and classification stops.
2. **Open enum, closed in practice.** C45 is declared an open enum, so a
   future value needs no new row — the C160/C197 precedent. This feature
   adds no new row.
3. **Unreachable components only**, except `flat-attached-fallback`, whose
   m061 semantic widened to "incoming edge attribution synthesized" and
   which is therefore usually reachable (`orphan_reason.rs` step 2b).
4. **Never alongside another I7 explaining marker** — FR-008.
5. **No value means no value.** A component matching no arm carries nothing
   and keeps failing I7. There is no catch-all — FR-011.
6. **All three formats, one source.** The classifier mutates the shared
   `components` Vec before any emitter runs (`scan_cmd.rs:4940`), so CDX,
   SPDX 2.3 and SPDX 3 cannot disagree. C45's existing extractor carries
   whatever string is there.

## Expected effect on the corpus

Measured before: 34 unexplained across 5 targets. Expected after: **16**
across 2.

| Value | Components | Targets |
|---|---|---|
| `dead-lockfile-entry` | 10 | python-flask |
| `foreign-build-system` | 3 | maven-guice (2), go-opentelemetry (1) |
| `unreferenced-lock-node` | 3 | haskell-security-advisories |
| `build-toolchain` | 2 | haskell-language-server |
| *(none — deferred)* | 13 | python-flask — #1234 |
| *(none — deferred)* | 3 | maven-guice — #1235 |

I7: 12 `Pass` → **15 `Pass`**, 2 `Fail`.
