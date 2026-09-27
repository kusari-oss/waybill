# Declared licenses

How waybill reports the license a project declares about **itself**, per
ecosystem — and why some components carry none.

Issue #954. For the emitted wire shapes see
[`sbom-format-mapping.md`](./sbom-format-mapping.md); for how declared differs
from concluded see [Declared vs concluded](#declared-vs-concluded) below.

## Why the project's own license is a special case

A dependency's license is recoverable later: any consumer holding its PURL can
look it up in a registry. The **scanned project's** is not, because a local
project is usually not a published package — `hackage.haskell.org/package/moat`
returns 404 for a library that exists only in a working tree.

So if waybill does not read it from the manifest at scan time, it is absent from
the document permanently. It is also the reason licenses appear at all in an
`--offline` scan: enrichment needs a network, a manifest does not.

## Per-ecosystem table

**Operator** is how waybill combines several declared licenses into the single
expression it emits. Where an ecosystem documents the relationship, waybill
follows it. Where it does not, waybill joins with **AND**, and that is an
inference by waybill rather than a statement by the project — see
[Why conjunction is the fallback](#why-conjunction-is-the-fallback).

| Ecosystem | Manifest | Key | Multiple? | Operator | Inheritance |
|---|---|---|---|---|---|
| cargo | `Cargo.toml` | `[package].license` | No — one SPDX expression | n/a | `license.workspace = true` → `[workspace.package]` |
| npm | `package.json` | `license` | No — one SPDX expression | n/a | none |
| pip | `pyproject.toml` | `[project].license` | No — PEP 639 single string | n/a | none |
| maven | `pom.xml` | `<licenses><license><name>` | **Yes** | **AND** (inferred) | inherited from parent POM |
| gem | `.gemspec` | `licenses` / `license` | **Yes** | **AND** (inferred) | none |
| composer | `composer.json` | `license` | **Yes** | **OR** — documented | none |
| elixir | `mix.exs` | `package: [licenses: …]` | **Yes** | **AND** (inferred) | none |
| scala | `build.sbt` | `licenses :=` | **Yes** | **AND** (inferred) | none |
| nuget | `.csproj` | `PackageLicenseExpression` | No — one SPDX expression | n/a | via MSBuild property inheritance |
| haskell | `.cabal` | `license:` | No | n/a | none |
| **go** | *not the manifest* | LICENSE file `SPDX-License-Identifier:` header | No | n/a | none |

Notes that matter in practice:

- **composer** is the only ecosystem in this table that documents the
  relationship: its array means *a choice between* licenses, so it joins with
  `OR`. Conjunction is expressed instead by a parenthesised `and` string, which
  passes through untouched.
- **scala** names are **free-form, not SPDX**. sbt's own documented example is
  `"Apache 2"`, not `Apache-2.0`, so most scala projects produce a non-listed
  reference rather than a listed identifier — see
  [Values that are not valid SPDX](#values-that-are-not-valid-spdx).
- **gem** exposes both `license` and `licenses`; both are read, and the plural is
  matched first so a two-entry array is not truncated to its first element.
- **go** is in the table but reads no manifest field: `go.mod` has none. It
  extracts an `SPDX-License-Identifier:` header from `LICENSE`/`LICENCE`/
  `COPYING` at the workspace root. This predates #954 (milestone 057).

## Why conjunction is the fallback

Most ecosystems do not say how several licenses combine. RubyGems states it
outright — *"the array itself does not state how the licenses combine"* — and the
Maven POM reference is silent.

Any operator waybill picks there is its own inference, and the two directions
fail asymmetrically:

- **AND** over-states the obligation. A consumer complies with more licenses than
  required. Wrong, but it cannot cause a breach.
- **OR** under-states it. A consumer satisfies one license and may ship in breach
  of another that also applied. Wrong, *and* dangerous.

When the error is legal rather than cosmetic, the conservative direction wins. It
also keeps one combining rule in the system, since the emitter joins with `AND`
where several values reach it.

**If you need the project's literal declaration rather than waybill's combined
expression**, read the manifest. The SBOM records which licenses apply; the
operator between them is waybill's reading unless the ecosystem defined it.

## Values that are not valid SPDX

A manifest may declare something that is not a valid SPDX expression — a legacy
spelling like `AllRightsReserved`, a pre-SPDX name like `"Apache 2"`, or a
proprietary marker.

waybill **preserves** it rather than discarding it, in the slot each format
reserves for a non-listed license:

| Format | Valid SPDX | Not valid SPDX |
|---|---|---|
| CycloneDX | `licenses[].license.id` | `licenses[].license.name` |
| SPDX 2.3 | `licenseDeclared: "<expr>"` | `LicenseRef-<hash>` + `hasExtractedLicensingInfos` |
| SPDX 3 | license-expression element | custom-license element |

### Where the operator lives, per format

CycloneDX shapes the two operators differently, on purpose:

| Declared | CycloneDX | SPDX 2.3 `licenseDeclared` |
|---|---|---|
| `MIT OR Apache-2.0` | one `{expression}` entry | `MIT OR Apache-2.0` |
| `MIT AND Apache-2.0` | two `{license:{id}}` entries | `MIT AND Apache-2.0` |

`OR` needs the `expression` slot because CycloneDX defines no operator between
array entries, so splitting a choice would read as "both apply". `AND` is split
instead, because multiple entries already read as conjunctive and splitting keeps
each listed identifier in `license.id` where compliance tooling can match it.

**Consequence worth knowing:** for a conjunction, the operator is not observable in
CycloneDX — but it is always present in **SPDX 2.3 `licenseDeclared`**, for both
operators. If you need the exact expression a project declared, read the SPDX
output; if you need matchable identifiers, read CycloneDX.

Note also that the CycloneDX array carries `acknowledgement` in two positions:
nested under `license` for the id/name form, and at the entry's top level for the
`expression` form. Any consumer reading only one position under-reports.

So an unrecognised value is never presented **as though** it were a listed
identifier, and is never silently dropped either. Those two properties are not in
tension, because the formats separate the slots.

Dropping would leave a consumer unable to distinguish *"this project declared no
license"* from *"this project declared something we could not parse"* — and for a
marker meaning all rights reserved, that is the most legally significant line in
the file.

## Declared vs concluded

| | Meaning | Source | Offline? |
|---|---|---|---|
| **declared** | the project's own assertion about itself | its manifest | yes |
| **concluded** | a third party's determination | enrichment (ClearlyDefined, deps.dev) | needs network |

They occupy different fields, so they never overwrite one another and no
precedence rule is needed. If a manifest and a registry disagree, the document
shows both and the disagreement is visible rather than resolved silently.

This distinction is also the provenance record: `acknowledgement: "declared"`
says the value came from the project, which is why waybill adds no separate
`waybill:license-source` property.

## When a component carries no license

Three different reasons, worth keeping distinct because only the first two could
ever change:

### 1. The ecosystem's manifest has no license field, and there is no other path

**Swift**, **Dart**. Nothing to read. Serving these would need license-*text*
matching — identifying a license from its prose — which is a different mechanism
with its own accuracy and dependency questions, and is out of scope.

Note that extracting an `SPDX-License-Identifier:` header is **not** text
matching; it is reading a declaration. That is why Go is not in this group.

### 2. The reader parses a file that has no license field, though the ecosystem
declares one elsewhere

| Site | Reads | Where the license actually lives |
|---|---|---|
| cocoapods main-module | Podfile target, else directory name | `.podspec`, which this reader never parses |
| gem application main-module | `Gemfile` | the `.gemspec` — its sibling site *is* covered |

A Podfile declares pods to *consume*; a Gemfile declares gems to *install*.
Neither states the application's own license. These could be served by parsing an
additional file, which would be new reader capability rather than license
extraction.

### 3. The project simply declared nothing

The common case in several ecosystems, and not an error. waybill emits no warning
for it, because a per-component warning would make the ordinary case noisy.

## Not yet covered

**erlang.** Where `licenses` is declared for a rebar3 Hex package is not
documented in the sources checked (`hexdocs.pm/rebar3_hex` redirects,
`rebar3-hex.hexdocs.pm/readme.html` covers `{hex, [...]}` for doc providers but
is silent on licenses, `hex.pm/docs/rebar3_publish` 404s). Erlang publishes to
Hex, so the Hex contract likely applies — a required list of SPDX identifiers
with `LicenseRef-` supported for custom licenses — but the key path is unverified
and is not implemented on a guess.

## Checking what a scan produced

```sh
waybill --offline sbom scan --path . --format cyclonedx-json --output cyclonedx-json=sbom.json
```

The scanned project is usually the document's **primary** component
(`.metadata.component`), not an entry in `.components[]` — with a single
main-module it *is* the root. So every recipe below unions both, or it will return
nothing for the most common case. (The first draft of this page did exactly that.)

```sh
# every component carrying a declared license, primary included
jq -r '[.metadata.component] + (.components // [])
       | .[]
       | select((.licenses // []) | length > 0)
       | "\(.purl // .name)\t\(.licenses | map(.license.id // .license.name) | join(","))"' sbom.json

# declared vs concluded counts
#
# `acknowledgement` sits in two places depending on the entry shape: nested under
# `license` for the id/name form, at the entry's top level for the `expression`
# form. Reading only the nested position silently misses every compound
# expression — which the test helper for this feature did until it was caught.
jq '[[.metadata.component] + (.components // [])
     | .[].licenses[]?
     | (.license.acknowledgement // .acknowledgement)]
    | group_by(.) | map({ack: .[0], n: length})' sbom.json

# values preserved as non-listed references: a `name` with no `id`.
#
# Compound expressions are NOT in this set — they use the `expression` slot and
# are valid SPDX. This finds only what waybill could not canonicalise.
jq -r '[.metadata.component] + (.components // [])
       | .[]
       | select((.licenses // [])[]? | .license.name and (.license.id | not))
       | "\(.purl // .name)\t\(.licenses[0].license.name)"' sbom.json

# compound expressions (operator preserved in the `expression` slot)
jq -r '[.metadata.component] + (.components // [])
       | .[]
       | select((.licenses // [])[]? | .expression)
       | "\(.purl // .name)\t\(.licenses[0].expression)"' sbom.json
```

The third recipe is the useful one when auditing coverage: a populated `name`
without an `id` means the project declared something waybill could not
canonicalise, which is worth a human look.
