# #1396 pre-spec measurements — 2026-11-09

Everything below was executed in this session unless marked **inference** or
**not established**. Counts are from three identical runs where a run count is
given; sbomnix was run once and matched the earlier spike's files exactly.

## Setup

- waybill `main@49a24a63` (0.14.0), **release** build. The issue's figures were a
  debug build at `72b455e2`, before the #1399 fix.
- nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`, aarch64-darwin,
  Determinate Nix 3.20.0 (nix 2.34.6).
- Targets: wrapper flakes exposing `nixpkgs#gh` (2.93.0) and `nixpkgs#ripgrep`
  (15.1.0) as `.#default` (`targets/`).
- sbomnix 1.9.0+g95480c2. osv-scanner 2.7.0. `go version -m` (Go toolchain on
  PATH) and `rust-audit-info` 0.5.4 as independent references.
- grype is not installed on this host; the issue's grype figures were not re-run.
- Counting probe: `probe/count.jq` (recursive over nested components).

Commands:

```sh
# closure tier (run from the flake dir; a relative flake path yields 0 closure paths)
waybill sbom scan --no-deep-hash --nix-closure --no-clearly-defined --path . \
  --format cyclonedx-json --output cyclonedx-json=closure.N.cdx.json
# binary readers on the realised output
nix build .#default --no-link --print-out-paths
waybill sbom scan --no-deep-hash --no-clearly-defined --path <out> ...
# sbomnix
sbomnix --buildtime --cdx sbomnix-bt.cdx.json <flake>#default
sbomnix             --cdx sbomnix-rt.cdx.json <flake>#default
# closure realisation
nix derivation show -r .#default      # then test -e on every outputs.*.path
```

## 1. What the closure tier emits

| | components | `pkg:golang` | `pkg:cargo` | wall |
|---|---|---|---|---|
| gh, 3 runs | 494 | 0 | 0 | 2–3 s |
| ripgrep, 3 runs | 599 | 0 | 0 | 3 s |

PURL types: gh 468 generic / 24 pypi / 1 github; ripgrep 539 / 58 / 1.

The language dependencies exist only as opaque fetch derivations, all
`scope: excluded`, no version, no CPE:

- `gh-2.93.0-go-modules` → `pkg:generic/gh-2.93.0-go-modules-sha256.…`
- `ripgrep-15.1.0-vendor`, `ripgrep-15.1.0-vendor-staging` → `pkg:generic/…`

**The scanned package itself is not a component.** Neither SBOM has a component
for gh 2.93.0 or ripgrep 15.1.0 (selector validated: it finds `go 1.26.3`). The
subject is `filesystem-scan 0.0.0`, and the log warns `the closure root is not a
component, so its direct dependencies are not anchored to the subject`
(unanchored = 16 for gh, 8 for ripgrep). This is #1398, still open — there is
currently nothing to attach recovered modules *to* for the root.

## 2. What the binary readers find on the realised output

| | all components | `pkg:golang` | `pkg:cargo` | independent reference |
|---|---|---|---|---|
| gh, 3 runs | 166 | 163 | 0 | `go version -m`: 162 `dep` lines + stdlib = 163 |
| ripgrep, 3 runs | 46 | 0 | 42 | `rust-audit-info`: 42 packages (37 runtime, 5 build) |

Wall time ≤ 1 s each. The issue's "45 components" for ripgrep is 42 crates plus
the `rg` binary and two linked dylibs. With #1399 fixed, emission completes.

Observations that bear on the design:

- **gh's own module is absent.** Build info says `mod github.com/cli/cli/v2 (devel)`
  — nixpkgs builds from a tarball, so the binary does not know its version. The
  closure does (2.93.0). No `cli/cli` component is emitted by the binary scan.
- **ripgrep's own crate is present** as `pkg:cargo/ripgrep@15.1.0?source=local`.
- **The two readers shape containment differently.** The 42 crates are nested
  under the `rg` component (`components[].components[]`); the 163 Go modules are
  flat at top level with a flat root → everything edge list (165 edges).
- **Every crate-to-crate edge is dropped.** cargo-auditable records 85 dependency
  entries; waybill logs `unresolved_declared_dep_count=85` and emits
  `relationships=0`. The declared target is `pkg:cargo/memchr@2.7.6?source=crates.io`
  and a component with exactly that PURL exists, but its bom-ref carries a
  `#pkg:generic/rg?file-sha256=…` suffix. Cause is **inference**; the 85-of-85
  loss is measured. Existing defect in the binary path, independent of #1396.
- The 5 build-kind crates carry `waybill:cargo-auditable-kind: build` but no
  CDX `scope`.
- gh's `bin/gh` is a bash wrapper; the Mach-O is `bin/.gh-wrapped`.

Scanner outcome (osv-scanner, **unadjudicated**):

| SBOM | Go / crates.io findings |
|---|---|
| gh closure | 0 |
| gh binary | 65 on 12 packages (stdlib 27, grpc 8, x/net 8, x/crypto 4, sigstore-go 4, …) |
| gh sbomnix buildtime | 0 (711 packages filtered as unscannable) |
| ripgrep closure | 0 |
| ripgrep binary | 3 (memmap2, crossbeam-epoch, anyhow) |
| ripgrep sbomnix buildtime | 0 |

## 3. How sbomnix handles it

It does not. sbomnix is derivation-granular in both modes.

| | components | `pkg:golang` | `pkg:cargo` | PURL types |
|---|---|---|---|---|
| gh buildtime | 788 | 0 | 0 | 711 `pkg:nix`, 76 none |
| gh runtime | 5 | 0 | 0 | `pkg:nix` (bash, iana-etc, mailcap, tzdata + gh) |
| ripgrep buildtime | 897 | 0 | 0 | 800 `pkg:nix`, 96 none |
| ripgrep runtime | 3 | 0 | 0 | `pkg:nix` (libiconv, pcre2 + ripgrep) |

- Dependencies appear as one component each: `pkg:nix/gh-2.93.0-go-modules`,
  `pkg:nix/ripgrep-15.1.0-vendor`, with a self-named heuristic CPE and the
  store output path as a property.
- sbomnix's only language-ecosystem code is a regex that *filters out* generated
  `cargo-src-*` / `cargo-package-*` artifacts (`sbomnix/artifacts.py:35`).
- Its subject is right where waybill's is not: `gh 2.93.0`,
  `ripgrep 15.1.0` with `cpe:2.3:a:ripgrep_project:ripgrep:15.1.0`.
- Its README states runtime mode requires building the target; buildtime does not.
- A `gh search issues` on tiiuae/sbomnix for golang/cargo/vendor returned nothing.
  That is a search result, not a claim that no such request exists.

## 4. What is actually realised

| | derivations | outputs with a path | realised on this host |
|---|---|---|---|
| gh | 788 | 894 | 96 |
| ripgrep | 897 | 1,003 | 102 |

- The root output is realised in both (control). It arrived by substitution.
- **The vendored-dependency outputs are not realised** (`…-gh-2.93.0-go-modules`,
  `…-ripgrep-15.1.0-vendor`): substituting a binary does not pull its build
  inputs. Nor are the Go or Rust toolchains.
- Scanning all 104 distinct realised outputs (29 s total, offline): **exactly two
  yield Go modules or crates — the two roots.** 103 scans exit 0; one output
  (`signing-utils`) is a single file and `--path` rejects it.
- Runtime closures: gh 5 paths, ripgrep 3. Only the root carries embedded
  build info in either.

## Not established

- Whether toolchain binaries (`go`, `cargo`, `rustc`) would contribute modules of
  their own if realised — they are not realised here, so "attach to every
  realised member" was only exercised on a store with no toolchains in it.
- Anything on Linux/ELF, on a fresh CI runner (where nothing is realised), or on
  a closure with more than one Go/Rust program in it.
- The cost of realising an unrealised output. Not measured; nix can build
  arbitrary code to do it.
- osv-scanner findings are unadjudicated.
- Two targets, one nixpkgs revision, one system.

## Probe error caught on the way

The first count read `.components` only and reported **0 cargo crates** for
ripgrep's binary SBOM, which would have read as a regression against the issue.
The crates are nested one level down. `probe/count.jq` descends; it was checked
against the scan log's own `components=45`.

---

# Planning measurements — 2026-11-10

Same host, same waybill build, same nixpkgs revision. These discharge the
measurement obligations the spec handed to planning.

## P1. Registry confirmation (FR-012a)

deps.dev v3 `GetVersion`, live (`out/registry/`):

| query | HTTP |
|---|---|
| `go` `github.com/cli/cli/v2` @ `v2.93.0` | 200 |
| `go` `github.com/cli/cli/v2` @ `2.93.0` (closure's form, no `v`) | **404** |
| `go` `github.com/cli/cli/v2` @ `v2.999.0` (negative control) | 404 |
| `cargo` `ripgrep` @ `15.1.0` | 200 |
| `cargo` `ripgrep` @ `15.999.0` (negative control) | 404 |

Both identities confirm. The Go version must be given the `v` prefix the
closure does not carry; without it a real version is indistinguishable from a
nonexistent one.

## P2. A closure with toolchains present

Realised by substitution, then scanned standalone (`--offline`):

| output | files | `pkg:golang` | `pkg:cargo` | where they come from |
|---|---|---|---|---|
| `go-1.26.3` | 11,576 | 1 | 0 | `stdlib`, from `bin/go` + `share/go/VERSION`. `go version -m bin/go` lists no `dep` lines. |
| `cargo-1.95.0` | 42 | 0 | 321 | cargo-auditable data in `bin/.cargo-wrapped` |
| `cargo-auditable-0.7.2` | 3 | 0 | 97 | cargo-auditable data in its two binaries |
| `rustc-1.95.0` | 3,239 | 0 | 709 | **none from a binary** — all from `Cargo.lock` files under `lib/rustlib/rustc-src/` |

- Build-only contents are large for Rust: 418 crates from `cargo` and
  `cargo-auditable` against 42 shipped in ripgrep. For Go the toolchain adds
  one (`stdlib`).
- **A whole-directory scan is not the same as the binary readers.** rustc's 709
  crates are source-tier lockfile hits, not embedded build information. The
  feature must invoke the binary readers only; reusing the full scan on an
  output would import every lockfile a build tool happens to ship.
- Standalone scans also emit the binary itself, linked libraries and file
  entries (go: 110 without a PURL, 76 `pkg:generic`). Only ecosystem modules
  are in scope to attach.
- Go modules carry no `waybill:evidence-kind` property today; crates carry
  `cargo-auditable`. (`probe/evidence.jq`.)

## P3. Linux

`legacyPackages.aarch64-linux.{gh,ripgrep}`, substituted on the darwin host
and scanned there (ELF, not Mach-O):

| | `pkg:golang` | `pkg:cargo` |
|---|---|---|
| gh | 164 | 0 |
| ripgrep | 0 | 42 |

gh's Linux build has one module the macOS build does not,
`github.com/godbus/dbus/v5@v5.2.2` — a real platform difference. Crates are
nested under `rg` on Linux exactly as on macOS. x86_64 not measured.

## P4. Principle V audit against the vendored schemas

`waybill-cli/tests/fixtures/schemas/`:

- CycloneDX 1.6 `aggregateType`: `complete`, `incomplete`,
  `incomplete_first_party_only`, …, `unknown`, `not_specified`.
  `compositions` may scope an aggregate to `assemblies` or `dependencies`.
- CycloneDX 1.6 `componentIdentityEvidence`: `field` (incl. `purl`),
  `concludedValue`, `confidence`, `methods[].technique` (incl.
  `binary-analysis`).
- SPDX 2.3 `relationshipType` includes `DEPENDS_ON`, `DEPENDENCY_OF`,
  `CONTAINS`, `STATIC_LINK`.
- SPDX 3.0.1 defines `dependsOn`, `contains`, `hasStaticLink`, and the
  `packageUrl` external-identifier type.

The spec's FR-017 audit holds as written.

---

# Acceptance measurements — 2026-11-10

Release build of the implementation on branch `1378-closure-binary-contents`.
Same host and nixpkgs revision. The store now also holds the Go and Rust
toolchains fetched for P2, so build-only contents appear.

## A1. Positive (SC-001, SC-002, SC-005, SC-005a)

| | recovered | program | `pkg:golang` / `pkg:cargo` in document | unanchored root edges |
|---|---|---|---|---|
| gh, deps.dev on | 163 | `gh 2.93.0` → `pkg:golang/github.com/cli/cli/v2@v2.93.0` (`confirmed`) | 164 | 0 (was 16) |
| gh, `--no-deps-dev` | 163 | `pkg:generic/gh@2.93.0` (`unchecked: disabled`) | 163 | 0 |
| ripgrep, deps.dev on | 41 + 345 build-only | `ripgrep 15.1.0` → `pkg:cargo/ripgrep@15.1.0` (`confirmed`) | 387 | 0 (was 8) |
| ripgrep, `--no-deps-dev` | 41 + 345 build-only | `pkg:generic/ripgrep@15.1.0` | 386 | 0 |

- Exactly one component describes each program.
- gh → 163 module edges; ripgrep → 41 crate edges.
- ripgrep's 345 build-only crates come from `cargo` and `cargo-auditable`,
  realised on this host for P2. All 345 carry `scope: excluded`; the 42
  shipped identifiers (41 + ripgrep) do not.
- The document subject is unchanged (`filesystem-scan 0.0.0`).
- With a confirmed identity the root receives a name-derived CPE from the
  existing synthesis pass (`cpe:2.3:a:gh:gh:2.93.0`); without, none.

C213, gh: `{"absent":210,"failed":0,"members":493,"modules":{"cargo":0,"golang":163},"no-outputs":204,"partial":69,"read":10,"root":"read",...,"yielded":2}`

**204 of 493 members declare no output path** and can never be read this way.
They are mostly fetched sources. Not anticipated by the spec.

## A2. Scanner control (SC-003)

osv-scanner 2.7.0, Go findings:

| document | findings | packages |
|---|---|---|
| gh closure scan (this feature) | 65 | 12 |
| gh standalone binary scan | 65 | 12 |

The per-package identifier sets are identical (same digest). Before: 0.

## A3. Format parity (SC-006)

The set of Go/cargo identifiers is identical across CycloneDX, SPDX 2.3 and
SPDX 3 for both targets (164 and 387; `probe/modules.jq`). Both SPDX 3
documents pass `spdx3-validate` 0.0.5.

## A4. Negative and strict (SC-004, SC-004a)

`nixpkgs#gum` 0.17.0, whose output is not in the store:

- default: exit 0; C213 `root: "absent"`; the root carries
  `waybill:closure-contents: absent`; one `WARN` names it.
- `--nix-closure-require-root-contents`: exit 1, error names `gum 0.17.0`
  and `absent`, **no document written**.

A first attempt at this control set `NIX_STORE_DIR=/nonexistent`. nix honours
that variable too: the closure tier degraded with reason
`no-evaluable-attribute` and the run produced 2 components and no C213 — a
result about nix, not about the feature. Recorded in quickstart.md.

## A5. Cost (SC-008)

Three runs each, `--no-deps-dev`, 97 present outputs totalling ~2.6 GB:

| | wall |
|---|---|
| gh | 7.6 / 4.5 / 5.0 s |
| ripgrep | 5.7 / 4.7 / 5.3 s |

Before the feature the same scan took 2–3 s, so the pass adds roughly 2–3 s
for 97 outputs — about 30 ms per output, against the 279 ms per output of
scanning each individually (29 s / 104). The first implementation called the
full binary scanner per file and took 16 s; it was replaced with one read and
one section lookup per binary, and the emitted document is byte-identical to
the slower version apart from serial number and timestamp.

## A6. Nothing else moved (SC-007)

Comparing the pre-feature closure documents (§1) with the post-feature ones,
over every component that is not a Go or cargo identifier: 494 before and 494
after for gh, 599 and 599 for ripgrep, and no line differs on name, version,
identifier or scope. (The root accounts for the equal counts: it is new, and
with its identity confirmed it is itself a Go or cargo identifier.)

Document-scope changes: `waybill:nix-closure-contents` is new, and
`waybill:graph-completeness-reason` now appears, because the recovered modules
are `analyzed`-tier with no module-to-module edges — the same reason a
standalone binary scan reports. A before/after on a closure with no Go or
Rust program was not run: no pre-feature binary was kept.

## Not established

- The closure path on Linux. P3 measured the readers on ELF outputs; the
  closure tier evaluates the host system, so an end-to-end Linux run needs a
  Linux host.
- A closure containing a second shipped Go or Rust program.
- The `nix-closure-moat` corpus goldens, which change and are regenerated only
  through the CI dispatch.

---

# After rebasing onto main@a7d0409e — 2026-11-10

Main gained milestone 1377 (#1405): the evaluated package is always emitted
and is the document's subject. This branch's own root mechanism was removed
and the contents pass reads `graph.evaluated`. The acceptance sections above
were measured before the rebase; the figures that changed are restated here,
from a release build of the rebased commit.

| | gh | ripgrep | gum (root not in store) |
|---|---|---|---|
| recovered Go / cargo identifiers | 163 | 386 (41 shipped + 345 build-only) | 1 (`stdlib`, from the Go toolchain) |
| document subject | `gh 2.93.0`, `pkg:golang/github.com/cli/cli/v2@v2.93.0` | `ripgrep 15.1.0`, `pkg:cargo/ripgrep@15.1.0` | `gum 0.17.0`, `pkg:generic/gum@0.17.0` |
| edges from the subject to modules | 163 | 41 | 0 |
| root state (C213) | `read` | `read` | `absent` |
| unanchored root edges | 0 | 0 | 0 |
| identifier sets equal across the three formats | yes (164) | yes (387) | yes (1) |

- **A1's "subject unchanged (`filesystem-scan 0.0.0`)" no longer holds**, and
  is not this feature's doing: milestone 1377 makes the evaluated package the
  subject. What this feature adds is that, when the registry confirms it, the
  subject's identifier is the ecosystem one.
- **The subject then carries a name-derived CPE** (`cpe:2.3:a:gh:gh:2.93.0`,
  `cpe:2.3:a:ripgrep:ripgrep:15.1.0`) from the existing synthesis for Go and
  cargo components. With no promotion (gum) it carries none. Whether a
  promoted subject should keep that candidate is a question for milestone
  1377's CPE rules, not settled here.
- The subject and its `components[]` entry share one bom-ref on all three
  targets, including gum where nothing was promoted — main's behaviour.
  The read state (C214) and binary identity (C215) are on the `components[]`
  entry, not on `metadata.component`.
- osv-scanner Go findings on gh: 65 on 12 packages, unchanged.
- Both SPDX 3 documents pass `spdx3-validate` 0.0.5.
- Strict flag on gum: exit 1, names `gum 0.17.0`, nothing written.
- Catalog rows are C213 (document), C214 (per member), C215 (identity):
  main took C212.
