# Data model: 1046

Everything is in-process for one scan. Nothing persists beyond the emitted documents.

## Twin pair (input to the merge)

| field | source | notes |
|---|---|---|
| design | a component with a `pkg:hackage/` PURL, empty version, `sbom_tier = design`, C20 `waybill:requirement-ranges` (possibly empty) | From the Haskell manifest reader. |
| build candidates | closure members (`waybill:closure-role` present, no C206 member role) with the same `name` | Several only for build variants at different versions. |
| haskell evidence | a candidate is a Haskell package when its PURL is `pkg:hackage/` (C201 `confirmed`) **or** one of its derivations is `built_by_ghc` | FR-003: a namesake that is not a Haskell package never merges. |

**Selection (FR-004).**
- **One candidate:** it is the survivor.
- **Several candidates:** the survivor is the one whose version the ranges admit (`version_range::admits == Yes`), if exactly one does. Otherwise there is no merge, and the reason is logged and counted.

## Merged component (output)

| field | value |
|---|---|
| `purl`, `version`, `sbom_tier`, scope, closure annotations | the survivor's (R6) |
| C20 `waybill:requirement-ranges` | the design tier's ranges |
| source-file evidence | the design tier's, filled in when the survivor has none |
| C172 `waybill:nixpkgs-version-disagreement` | kept when the design tier had it |
| `waybill:haskell-version-unresolved-reason`, C151 `waybill:unresolved-reason` | dropped |
| C221 `waybill:requirement-unsatisfied` | `{"requirement-ranges":[…],"version":"<v>"}` when every range rejects the version (keys sorted) |
| edges | every edge naming the design PURL is renamed to the survivor's, and duplicates are removed |

## Hackage publication check (R2, R3, R8)

**Candidate:** C201 would be `version-disagrees`, and `built_by_ghc` holds.

| outcome | C201 value | identity |
|---|---|---|
| 200 | `hackage-published` | `pkg:hackage/<name>@<version>`, plus `?vcs_url=git+<url>@<rev>` when the `src` is a VCS fetch |
| 404 | `hackage-unpublished` | `pkg:generic` (unchanged) |
| `--offline` | `hackage-unchecked-offline` | `pkg:generic` |
| `--no-hackage-lookup` | `hackage-unchecked-opted-out` | `pkg:generic` |
| timeout, transport error, other status | `hackage-unreachable` | `pkg:generic` |

**Rename.** A minted identity renames the PURL through the pass's existing `renames` list, so edges follow (#980).

## VCS source (R5)

| field | source |
|---|---|
| `url` | `attr_str("url")`, from either carrier |
| `rev` | `attr_str("rev")` |
| reference | `ExternalReference { ref_type: "vcs", url: "git+<url>@<rev>" }`, only when both are present |

**Carried by:**
- the source member itself;
- every package component whose derivation's `src` resolves (by output path) to that member.

`distribution` references are unchanged, apart from now also being read from structured attributes.
