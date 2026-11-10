# #1436 — which VEX shape do scanners honour for a nixpkgs backport?

Measured 2026-11-10 on aarch64-darwin, before any design work (CLAUDE.md,
"Measure external behaviour"). Result: **the only OpenVEX shape grype honours
on an SBOM conflicts with milestone 1035's statement design**, so this work
stopped at the measurements and hands the decision back (#1421).

## Inputs

- Documents: the `--nix-closure` CycloneDX documents from the 0.15.0 sbomnix
  comparison (#1430), for curl, openssh, httpie, gh, ripgrep and moat
  (nixpkgs a799d3e3). Written by the `waybill-f891b4fe` build kept beside them;
  the documents' tool version string reads `0.14.0`.
- grype 0.120.0, DB schema v6.1.10 built 2026-11-08, `GRYPE_DB_AUTO_UPDATE=false`.
- Trivy 0.71.1, DB v2 updated 2026-11-10, `--skip-db-update`.
- Source read: grype v0.120.0 `grype/vex/openvex/implementation.go`, go-vex
  v0.2.9 (`pkg/vex/{vex,statement,product,component}.go`), Trivy v0.71.1
  `pkg/vex/{openvex,vex}.go` and `pkg/sbom/io/encode.go`.

## Probes

| script | what it varies | results |
|---|---|---|
| `shape-probe.sh` | 21 statement shapes for one CVE (CVE-2014-8139) on one component (`pkg:generic/unzip@6.0`), six targets, grype and Trivy | `shape-results.tsv` |
| `coverage-probe.sh` | 4 shapes over **every** `pedigree.patches[].resolves` (component, CVE) pair in each document, grype | `coverage-results.tsv` |
| `trivy-probe.sh` | Trivy's matching rules, on a finding Trivy does report (lxml, synthetic statements) | `trivy-results.tsv` |

`shape-probe.sh` and `coverage-probe.sh` add #1435's `syft:cpe23 =
unzip_project:unzip` to unzip where noted, without which grype matches nothing
on unzip. Every variant shares its base document, so the statement shape is
the only variable. `others_lost` counts findings that vanish but are not the
subject; it is 0 for every row that ran.

## Scanner behaviour

One subject (unzip / CVE-2014-8139), six targets. "Suppressed" = the subject
finding disappears; `others_lost` was 0 in every row.

| shape | statements | grype 0.120.0 | Trivy 0.71.1 |
|---|---|---|---|
| (a) `a_pair` — **today's m1035 shape** | `affected` {unzip}; `not_affected` {product: root PURL, sub: unzip} | 0/6 | n/a¹ (lxml: no) |
| (a) `a_only` | the `not_affected` half alone | 0/6 | n/a¹ (lxml: no) |
| (a) `a_generic` | `not_affected` {product: `pkg:generic/<root name>@<root version>`, sub: unzip} | 0/6 | n/a¹ |
| (a) `a_parent` (Trivy only) | `not_affected` {product: lxml's graph parent, sub: lxml} | — | lxml: no |
| (b) `b_only` | `not_affected` {product: unzip} | **6/6** | n/a¹ (lxml: **yes**) |
| (b) `b_fixed` | `fixed` {product: unzip} | **6/6** | n/a¹ |
| (b) `b_pair` | `affected` {unzip}, then `not_affected` {unzip} | **6/6** | n/a¹ (lxml: no) |
| (b) `b_pair_rev` | `not_affected` {unzip}, then `affected` {unzip} | 0/6 | n/a¹ (lxml: **yes**) |
| (b) `b_pair_ts` | as `b_pair_rev`, `not_affected` stamped later than `affected` | 0/6 | — |
| (c) `c_embed_na` / `c_embed_res` | CycloneDX `vulnerabilities[]` in the scanned SBOM, `not_affected` / `resolved`, `affects[].ref` = unzip's bom-ref | 0/6 | n/a¹ (lxml: no) |
| (c) `c_vexflag` / `c_link_comp` | separate CycloneDX VEX via `--vex`, `affects[].ref` a BOM-Link to the component | **rejected**² | lxml: **yes** |
| (c) `c_link_pair` | separate CycloneDX VEX in m1068's shape (`exploitable` → component, `not_affected` → root) | — | lxml: no |
| `q_pair` | SBOM's unzip PURL gains a build qualifier; `affected` {unqualified}, then `not_affected` {qualified} | **6/6** | — |
| `q_pair_rev` | the same, opposite order | 0/6 | — |
| `q_lone` | `not_affected` {qualified} alone | **6/6** | — |
| `q_lone_other` | `q_lone`'s VEX against the **unqualified** document | 0/6 (scoping holds) | — |

¹ Trivy does no CPE matching and reports nothing on any `pkg:generic`
nixpkgs component: the subject is absent from its output on every target and
every variant, so nothing it does with VEX can change these counts. Its rules
were measured on `pkg:pypi/lxml@6.0.2` / CVE-2026-41066 in curl's document
instead (`trivy-probe.sh`; the statements are synthetic and claim nothing
about lxml).
² grype exits 1: `unable to detect document format`. It accepts OpenVEX and
CSAF only (`grype/vex/processor.go`). The 6/6 in `shape-results.tsv` for
`c_vexflag` is the empty output of a failed run, not a suppression.

Adding the qualifier changed no finding counts (`q_none` equals `none` on all
six targets), so it does not disturb grype's CPE matching.

## Why, from the source

**grype never uses the document subject for an SBOM.** `findMatchingStatement`
(implementation.go:259) tries two passes. Pass 1 uses product identifiers from
the scan context, which for a non-image source is
`pkg:generic/<Source.Name>@<Source.Version>` only when both are set
(implementation.go:59–73). An `sbom:` source carries neither — grype's JSON
`source` is `{"type":"sbom-file","target":…}` — and `--name` does not help
(one-off, not scripted: `grype sbom:base.cdx.json --name curl --vex
a_generic.vex.json` on curl left `source` without a name and still reported
the subject). So pass 1 has no products, and
pass 2 asks `doc.Matches(vuln, <package PURL>, nil)`: the statement's
**product** must match the scanned package's PURL. A subcomponent is never
consulted. That is why every (a) shape, today's included, suppresses nothing,
and why `a_generic` fails too.

**grype and Trivy break a tie in opposite directions.** go-vex's
`VEX.Matches` (vex.go:196) collects matching statements from last to first,
then stable-sorts by timestamp ascending (statement.go:142). grype takes
`stmts[0]` (implementation.go:262/268): the **last** statement in the document
for equal timestamps, the **earliest** timestamp otherwise. Trivy takes
`stmts[len-1]` (openvex.go:35): the **first** in the document, or the
**latest** timestamp. Hence `b_pair` suppresses on grype and not on Trivy,
`b_pair_rev` the reverse, and `b_pair_ts` (`not_affected` later) loses on
grype. Any document holding an `affected` and a `not_affected` that both match
the scanned PURL is honoured by at most one of the two scanners, whatever its
order or timestamps.

**Trivy honours the subcomponent shape, but not for an SBOM's root.** Trivy
suppresses when every path from the vulnerable package to the root crosses a
component C with a `not_affected` {product: C, sub: package} (vex.go:148–219),
which is m1035's shape. But it rebuilds the graph from its own report with
`ForceRegenerate` (vex.go:79); for an SBOM input the root it builds has no
PURL (encode.go:91–139), and packages Trivy does not model, such as
`pkg:generic`, are dropped, so neither the root nor a generic parent can be a
matching product (`a_only`, `a_parent`: no).

**Neither scanner reads `vulnerabilities[]` embedded in the scanned SBOM**
(`c_embed_*`: 0 everywhere). Embedded VEX reaches CycloneDX consumers such as
Dependency-Track, not these scanners.

## Coverage: every patched CVE, per target (grype)

From `coverage-results.tsv`. "Patched findings" are grype findings whose
(artifact PURL, vulnerability id) is in the document's
`pedigree.patches[].resolves`. `others_lost` was 0 in every row.

| target | patched findings (as scanned / with #1435 unzip CPE) | today (m1035 pair, root as product)³ | `lone_na` | `pair_grype` | `pair_trivy` |
|---|---|---|---|---|---|
| curl | 6 / 17 | 0 / 0 | 6 / 17 | 6 / 17 | 0 / 0 |
| openssh | 6 / 17 | 0 / 0 | 6 / 17 | 6 / 17 | 0 / 0 |
| httpie | 6 / 17 | 0 / 0 | 6 / 17 | 6 / 17 | 0 / 0 |
| gh | 6 / 17 | 0 / 0 | 6 / 17 | 6 / 17 | 0 / 0 |
| ripgrep | 6 / 17 | 0 / 0 | 6 / 17 | 6 / 17 | 0 / 0 |
| moat | 6 / 17 | 0 / 0 | 6 / 17 | 6 / 17 | 0 / 0 |
| **total** | **36 / 102** | **0 / 0** | **36 / 102** | **36 / 102** | **0 / 0** |

³ What waybill would emit with #1436 part 1 fixed (subject taken from
`metadata.component`). As shipped, the five wrapper flakes emit no
statements, and moat's emitted pair suppresses 0.

The 6 per target are jq (5) and libssh2 (1); unzip contributes 11 more once
its second CPE is present. #1436 quotes 114 patched findings across the six
targets with #1435 applied; this probe adds only the unzip CPE and counts 102.
The difference is not measured here (inference: #1435 surfaces CPEs for
httpie's giflib/fontforge too).

Trivy reports 0 patched findings on every target, before and after, because
it does not match `pkg:generic` components.

## What this means for the design

The shapes grype honours (`b_only`, `b_fixed`, `b_pair`, `q_pair`, `q_lone`)
all name the **component** as the `not_affected` product. Milestone 1035 names
the **build** as that product, with the component as a subcomponent, precisely
so the statement does not assert the version itself is clean
(`openvex/mod.rs`, `backport_statements`). And any shape that keeps m1035's
version-level `affected` alongside a matching `not_affected` works on at most
one scanner, by tie-break. The scanner-honoured shape and the m1035 contract
cannot both hold, so the choice is the contract change #1421 already raises.

Reproduce:

```sh
export SRC_DIR=<dir holding <target>/waybill.cdx.json> GRYPE=<grype 0.120.0>
OUT_DIR=$(mktemp -d) ./shape-probe.sh    > shape-results.tsv
OUT_DIR=$(mktemp -d) ./coverage-probe.sh > coverage-results.tsv
SRC=$SRC_DIR/curl/waybill.cdx.json OUT_DIR=$(mktemp -d) ./trivy-probe.sh > trivy-results.tsv
```
