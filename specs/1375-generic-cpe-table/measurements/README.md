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
