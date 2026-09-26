# Contract: per-ecosystem declared-license extraction

**Feature**: `1010-manifest-declared-license` · **Date**: 2026-09-26

The interface this feature exposes is not a CLI surface or an API — it is the
obligation each reader takes on. This is that contract, one row per production
main-module site.

## Shared obligations (every reader)

1. Read the license from a manifest **already parsed** for identity. Do not open a
   new file, spawn a process, or reach the network.
2. Resolve through the shared ladder: `try_canonical` first; on failure preserve
   the raw text via the lenient constructor. Never construct leniently first.
3. Emit **at most one** expression. Combine several declarations yourself.
4. Never fail the scan. A missing, malformed or unresolvable license yields
   `Absent`.
5. Emit no warning for an absent declaration. Emit a debug diagnostic only when a
   value was found and could not be canonicalised.

## Per-ecosystem contract

**Evidence column** states whether the field and semantics were verified in Phase 0
or remain to be confirmed during implementation. A row marked *to verify* must be
confirmed against the named source before its task is closed — it must not be
implemented from assumption.

| Ecosystem | Manifest | License key | Multiple? | Operator | Inheritance | Evidence |
|---|---|---|---|---|---|---|
| cargo | `Cargo.toml` | `[package].license` | No — single SPDX expression | n/a | `license.workspace = true` → `[workspace.package]` | field to verify; inheritance **verified** (this repo's own crates) |
| npm | `package.json` | `license` | No — single SPDX expression; `licenses` array deprecated | n/a | not documented → treat as absent | **verified** (npm docs) |
| pip | `pyproject.toml` | `[project].license` | No — PEP 639 mandates a single top-level string | n/a | none defined | **verified** (PEP 639) |
| gem | `.gemspec` | `licenses` (array), `license` (single) | **Yes** | **AND** (fallback — ecosystem states the array "does not state how the licenses combine") | none defined | **verified** (RubyGems reference) |
| maven | `pom.xml` | `<licenses><license><name>` | **Yes** | **AND** (fallback — POM reference specifies nothing) | inherited from parent POM | **verified** (Maven POM reference) |
| composer | `composer.json` | `license` (string or array) | **Yes** | **OR** — array is documented as disjunctive | none defined | **verified** (Composer schema) |
| elixir | `mix.exs` | package metadata `licenses` | **Yes** | **AND** (fallback) | none defined | *to verify* — Hex package metadata docs |
| erlang | `.app.src` / `rebar.config` | `licenses` | **Yes** | **AND** (fallback) | none defined | *to verify* — rebar3 docs |
| scala | `build.sbt` | `licenses` | **Yes** | **AND** (fallback) | none defined | *to verify* — sbt reference |
| cocoapods | `.podspec` | `license` | No | n/a | none defined | *to verify* — podspec reference |
| nuget | `.csproj` | `PackageLicenseExpression` | No — single SPDX expression | n/a | `Directory.Build.props` may supply it | *to verify* — MSBuild pack docs |
| haskell | `.cabal` | `license:` | No | n/a | none defined | **implemented** (#957) — correct to preserve rather than drop |

### PackageLicenseFile / license-file / license.file

Out of scope (FR-011). Where a manifest declares a license **only** by file
reference, the contract is `Absent`. A file path is not a license identifier, and
resolving it requires reading and matching file content.

## Emission contract (no change required)

Stated for completeness, because Phase 0 verified it and a reviewer should not have
to re-derive it:

| Reader output | CycloneDX 1.6 | SPDX 2.3 | SPDX 3 |
|---|---|---|---|
| `Canonical` | `licenses[].license.id` + `acknowledgement: "declared"` | `licenseDeclared: "<expr>"` | declared-attribution expression |
| `Preserved` | expression carrying the raw text | `LicenseRef-<hash>` + `hasExtractedLicensingInfos` entry | custom-license element |
| `Absent` | no `licenses` key | `NOASSERTION` | no element |

`spdx/packages.rs::reduce_license_vec` already produces the `Preserved` row's
behaviour: its doc comment states *"any term fails canon → `(LicenseRef(id),
Some(extracted_info))`"*, and the OS-package readers already exercise it.

## Anti-contract — what a reader must not do

- Must not push multiple values and let the emitter join them. The emitter joins
  with an unconditional `AND`; only the reader knows whether that is right for its
  ecosystem.
- Must not construct leniently without first attempting canonicalisation, which
  would emit a non-canonical spelling that had a canonical form available.
- Must not mint a `LicenseRef` identifier itself. That derivation lives at
  emission; duplicating it risks two readers minting different identifiers for the
  same text.
- Must not infer a license from a LICENSE file, a classifier, or a URL. Those are
  different mechanisms with different accuracy, and are out of scope.
- Must not warn on absence. Most projects in some ecosystems declare nothing, and
  a per-component warning would make the common case noisy.
