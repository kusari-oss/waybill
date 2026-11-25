# Data model

All state is in process for one scan. There are no new types on the wire.

## Present boot library

A `ResolvedComponent` that carries C218 (`waybill:boot-library-compiler`) when `boot_library_edges::apply` starts.
- **Key:** its PURL.
- **Also indexed by** (C218 label, `purl.name()`).

## Release

The C218 label with `ghc-` or `ghc-binary-` stripped (`release_of`), for example `9.8.4`. A release is **walkable** when both `boot_table` (966) and `boot_dependencies` (1653) have it. They cover the same 36 releases (`the_releases_are_milestone_966s`).

## Reachable boot library

A (release, name) pair the 1653 table's `depends` reach from a present library of that release.
- **Identity:** `pkg:hackage/<name>@<966 version_of(release, name, system)>`.
- **Satisfied** when its PURL is in the document, or (label, name) is indexed.
- **Unsatisfiable** when 966 has no version for it. It is then never synthesized, and stays in C247 `absent`.

## Added boot library

A reachable PURL that no component held.
- **Built by** `compiler_supplied_component(name, Some((release, version)))`.
- **Fields:**
  - `sbom_tier` is `build`;
  - `lifecycle_scope` is `None`;
  - `extra_annotations` holds C218 = the label below;
  - C247 is written by the edge step like any peer's.
- **Label:** the project compiler's label when its release ships the same version; otherwise the label that reached it; the lexically first on a tie.
- **Invariant:** one PURL, one component.

## Boot edge

A `DependsOn` relationship with provenance `ghc-boot-dependencies`, from a boot library of release R to the component holding the PURL of the version R ships. Its lifecycle typing is unchanged: pass 3 (`type_artifact_absent_edges`) runs after it.

## BootEdgeSummary

The existing `edges`, `complete`, `incomplete` and `unknown`, plus **`added`**: the count of added boot libraries.
