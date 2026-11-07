# Measurements — spec 1375 (issue #1381)

**Date**: 2026-11-07 | **Host**: aarch64-darwin
**Closure**: `nix-closure-moat` at `d069055` | **waybill**: `122b052e`
**Source**: NVD CPE dictionary API, unauthenticated

---

## 1. The population

| | |
| --- | --- |
| unaddressable components (post-milestone-1374) | **194** |
| distinct names | **186** |
| …with at least one NVD dictionary entry | **51** |
| …nixpkgs construction names that can never be in NVD | **53** |
| …canonical names NVD simply does not have | **82** |

The 51 are **all canonical upstream names**; none is nixpkgs-shaped. The 53
are `bootstrap-stage0-stdenv-darwin`, `apple-sdk-private-frameworks-hook`,
`cctools-binutils-darwin-wrapper` and similar — their absence is structural.

## 2. The vendor is almost never the package name

186 queries with the vendor **wildcarded**, zero errors. Discovered vendors:

| name | vendor |
| --- | --- |
| `bash` | `gnu` |
| `libxml2`, `libxslt` | `xmlsoft` |
| `expat` | `libexpat` |
| `brotli` | `google` |
| `gawk` | `fossies` |
| `ngtcp2` | `tatsuhiro-t` |
| `libxcrypt` | `elektrobit` |

`expat` → `libexpat` is the defect `scripts/probe-cpe-dictionary.sh`'s header
cites (anchore/syft#4788). Assuming `(name, name)` would match nothing for
nearly all of these, silently.

## 3. Entry counts cannot decide a row, and neither can vendor count

All 51 names were re-queried for their entry **titles**. 50 returned in the
batch; `texinfo` timed out and was re-probed individually (`gnu`, 2 entries).

### The heuristic that does not work

"Does a title mention the name?" says **50 of 50 are clean**. It is useless:
`ninja`'s title is *"Ninja Framework Ninja 7.0.0"* — it mentions the name
while being a different product.

### Collisions found in the multi-vendor group

| name | the trap |
| --- | --- |
| `ninja` | **all three** vendors are other products — Java framework, ITRS, a WordPress plugin |
| `flex` | `apache` is **Apache Flex**, the Flash framework, not the lexical analyzer |
| `libidn2` | `gnu`'s title is **"GNU libidn 0.3"** — libidn **v1**, a different library |
| `bzip2` | `bzip2_project` is **"bzip2 0.4.3 for Rust"** — a Rust crate |
| `brotli` | `andybalholm` is **a Go port**, not Google's implementation |
| `patch` | `tanium` is **Tanium Patch**, endpoint management |
| `clang` | `arm` is **ARM Clang**, a different compiler product |

### And one in the single-vendor group, which is the important one

**`cmake` has exactly one vendor, `cmake_project`, and its title is
"cmake Project cmake 0.0.1 **for Node.js**".** It is an npm package.

`cpe:2.3:a:kitware:cmake` returns **0**: Kitware's CMake is not in NVD at all.

> **"Single vendor" is therefore not a safety signal.** A rule of "accept when
> there is only one vendor" would have added `cmake` — a row asserting that
> an npm package's vulnerabilities apply to the build system. Both groups
> contain collisions and only the title distinguishes them.

## 4. What this means for the feature

Reading titles is a **judgement**, not a computation, and the judgements are
not all obvious: `apache:flex`, `gnu:libidn2`-vs-libidn, and a Rust crate
sharing a C program's name each need someone to look.

So the sweep yields **candidates**, and the row count N is established by
working through them rather than predicted. SC-004 is stated as a property for
this reason.

The titles are committed in `nvd-titles.txt` so a later reader can disagree
with a specific judgement rather than re-derive all 51.

## Reproducing

```sh
# vendor discovery for one name
python3 -I measurements/cpe-discover.py bash bzip2

# the titles that decide a row
python3 -I measurements/cpe-titles.py ninja cmake flex

# and the control that settles cmake
curl -sS --get https://services.nvd.nist.gov/rest/json/cpes/2.0 \
  --data-urlencode 'cpeMatchString=cpe:2.3:a:kitware:cmake'
```

---

# Implementation

**Date**: 2026-11-07 | **Host**: aarch64-darwin | **waybill**: `122b052e`

## T001–T004 — baseline

| | |
| --- | --- |
| components already carrying a CPE | **78** |
| C209 before | `78 / 166 / 194 / 539 / 0` = 977 |

### The blast radius is small, and that is the point

| | goldens containing any of the 48 names as `pkg:generic` |
| --- | --- |
| local | **0** |
| corpus | **1 target** — `nix-closure-moat` |

**This is unlike the last two milestones**, where 33 local and 48 corpus
goldens moved. C209 is on every document, but its *value* only moves where an
affected component exists, and these 48 names appear in exactly one target.

So the usual golden check inverts: **a local golden moving is a finding**, not
a refresh, because no local fixture contains any of these names.

## The remediation found a second defect in its own check

`/speckit.analyze` raised U1: T009a's test would fail on all 12 pre-existing
table rows, which were verified by `probe-cpe-dictionary.sh` under #1246 and
never title-queried. They were **back-filled** rather than exempted — twelve
queries buys a uniform invariant instead of an exemption list that rots.

Running that back-fill then exposed a defect in the test's *shape*:

```
=== node (0 entries) ===
```

`node` returns nothing, because the row's **slug** is `node` while its
**product** is `node.js` — which has 1,716 entries under `joyent` and
`nodejs`. A slug-keyed check reports zero for a correct row.

T009a now keys on the product, and `data-model.md` gains V4: slug and product
coincide for 59 of the 60 rows and must not be assumed equal. The one
exception was found by doing the work the analysis asked for, not by reading
the code.
