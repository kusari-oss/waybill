# Phase 0 research — the curated CPE table

**Feature**: `1375-generic-cpe-table` | **Issue**: #1381

The central work of this phase is **deciding all 51 candidates**, because
nothing downstream can be sized until that is done. The dispositions are in
R4; the findings that shape how they were made are in R1–R3.

---

## R1 — The vendor must be discovered, never guessed

186 names queried against NVD's dictionary with the **vendor wildcarded**.
186 queries, **zero errors**, controls behaved (`openssl:openssl` 385, a
fixture negative 0).

| name | vendor NVD files it under |
| --- | --- |
| `bash` | `gnu` |
| `libxml2`, `libxslt` | `xmlsoft` |
| `expat` | `libexpat` |
| `brotli` | `google` |
| `gawk` | `fossies` |
| `ngtcp2` | `tatsuhiro-t` |

**Decision**: the table's vendor comes from the dictionary, never from the
component name. **Rationale**: `expat` → `libexpat` is the exact defect
`scripts/probe-cpe-dictionary.sh`'s header cites (anchore/syft#4788), and a
wrong vendor fails *silently* — zero results, indistinguishable from a clean
component.

## R2 — Entry counts cannot decide a row, and the obvious heuristic fails

All 51 names with entries were re-queried for their **titles**.

**"Does a title mention the name?" reports 50 of 50 clean.** It is useless:
`ninja`'s title is *"Ninja Framework Ninja 7.0.0"* — it mentions the name
while being a different product. Any string-matching rule passes every
collision.

**Decision**: a row requires reading the title and making a judgement; there
is no automated substitute. **Rationale**: see R3 — the collisions are not
all obvious, and two of them are invisible to every heuristic tried.

## R3 — "Single vendor" is not a safety signal either

The multi-vendor group holds seven collisions, several subtle:

| name | the trap |
| --- | --- |
| `flex` | `apache` is **Apache Flex**, the Flash framework |
| `libidn2` | `gnu`'s title is *"GNU libidn 0.3"* — libidn **v1**, a different library |
| `bzip2` | `bzip2_project` is *"bzip2 0.4.3 **for Rust**"* |
| `brotli` | `andybalholm` is *"Brotli 1.0.0 **for Go**"* |
| `patch` | `tanium` is **Tanium Patch**, endpoint management |
| `clang` | `arm` is **ARM Clang** |
| `ninja` | **all three** vendors are other products |

And one in the **single**-vendor group, which is the important one:

> **`cmake` has exactly one vendor**, `cmake_project`, whose title is
> *"cmake Project cmake 0.0.1 **for Node.js**"*. It is an npm package.
> `cpe:2.3:a:kitware:cmake` returns **0** — the build system is not in NVD.

**Decision**: neither entry count nor vendor count may gate a row.
**Rationale**: a rule of "accept when there is one vendor" would have added
`cmake`, asserting an npm package's vulnerabilities apply to the build system.
**Alternative rejected**: accepting the highest-count vendor — that picks
`ninjaframework` for `ninja` (126 entries) and `apache` for `flex`.

## R4 — The 51 dispositions

### Accept, single vendor — 33 names

`gnu`: `automake`, `bash`, `bc`, `bison`, `coreutils`, `cpio`, `diffutils`,
`ed`, `findutils`, `gettext`, `gzip`, `libtool`, `libunistring`, `readline`,
`texinfo`.

Own-name vendor: `libssh2`, `lua`, `nghttp2`, `perl`, `pkgconf`, `re2c`,
`tcl`.

`*_project` vendor: `libedit`, `libffi`, `lzo`, `oniguruma`, `rhash`.

Other: `expat`→`libexpat`, `libxml2`/`libxslt`→`xmlsoft`,
`compiler-rt`→`llvm`, `ngtcp2`→`tatsuhiro-t`, `gawk`→`fossies`.

> `gawk`→`fossies` is the odd one and is accepted with the oddity recorded.
> Fossies is a source-archive site, not the GNU project, and 15 entries are
> filed there. It is the only vendor NVD has for gawk, so a CPE-keyed scanner
> would use it. The attribution is upstream's to explain.

### Accept, several legitimate vendors — 9 names

| name | vendors |
| --- | --- |
| `c-ares` | `c-ares`, `c-ares_project` |
| `file` | `file_project`, `christos_zoulas` (the maintainer), `gentoo` |
| `jq` | `jq_project`, `jqlang` (the current org) |
| `less` | `gnu`, `greenwoodsoftware` (upstream) |
| `libarchive` | `libarchive`, `freebsd` |
| `libuv` | `libuv`, `libuv_project` |
| `ncurses` | `gnu`, `ncurses_project` |
| `unzip` | `info-zip`, `unzip_project` |
| `xz` | `tukaani`, `xz_project` |

### Accept partially — 6 names, one vendor dropped each

| name | kept | dropped, and why |
| --- | --- | --- |
| `clang` | `llvm` | `arm` — ARM's compiler |
| `patch` | `gnu` | `tanium` — endpoint management |
| `flex` | `flex_project`, `westes` | `apache` — the Flash framework |
| `brotli` | `google` | `andybalholm` — a Go port |
| `bzip2` | `bzip` | `bzip2_project` — a Rust crate |
| `libidn2` | `libidn2_project` | `gnu` — its title says libidn **v1** |

### Reject — 3 names

| name | why |
| --- | --- |
| `ninja` | all 126 entries are other products; the build tool is absent from NVD |
| `cmake` | the only vendor is an npm package; `kitware:cmake` = 0 |
| `libxcrypt` | **one** entry, `elektrobit` — an automotive software vendor |

> `libxcrypt` is the thinnest of the three calls and is recorded as such. The
> version in the title, 4.4.2, is a real upstream libxcrypt release, so the
> entry may be an upstream advisory NVD attributed to a downstream vendor —
> or it may describe libxcrypt as vendored inside an Elektrobit product. With
> a single entry there is no way to tell from the dictionary, and a wrong
> vendor matches nothing. **Rejected pending better evidence**, which is the
> conservative direction: a missing CPE understates, a wrong one asserts.

**Decision**: 48 names accepted, 3 rejected.

## R5 — What that is worth, in components

| | |
| --- | --- |
| unaddressable now | **194** |
| components covered by the 48 accepted names | **49** ← N |
| components on rejected names | 3 |
| unaddressable after | **145** |

N is 49 rather than 48 because `gettext` appears as two components.

So C209 moves `with_candidates` 78 → **127** and `unaddressable` 194 → **145**
on aarch64-darwin. The five counts still sum to 977.

**This is a 25% reduction in the unaddressable population from 48 table
rows** — and the remaining 145 is mostly the 53 nixpkgs construction names
plus 82 canonical names NVD does not carry, neither of which more table rows
can reach.

## R6 — The existing infrastructure is right and needs no change

`GENERIC_LIBRARY_CPES` is `(slug, &[(vendor, product)])`, matched by exact
lowercase component name. `NVD_DICTIONARY_OBSERVED` records what the probe
saw, and `every_table_pair_exists_in_the_nvd_dictionary` refuses any row
without an observation — offline, no network at test time (#1246).

**Decision**: reuse all three unchanged; add rows and observations.
**Rationale**: the enforcement this feature needs already exists; what it
lacks is the *title* half, which R2/R3 show cannot be automated and which the
dispositions above supply instead.

## R7 — The rejections need to be durable

`ninja` and `cmake` each look attractive to a future contributor: 126 and 5
dictionary entries respectively, and a name that matches. Without a record,
the next person re-derives this and may reach the opposite conclusion.

**Decision**: record the rejections **in the source**, next to the table, with
the title that justified them. **Rationale**: a contributor adding `ninja`
will be editing that file; a note in a spec directory is not where they will
be looking. **Alternative rejected**: a test asserting `ninja` yields no CPE —
worth having *as well*, but a test says what, not why.

---

## Constitution alignment

| Principle | Check |
| --- | --- |
| **I. Pure Rust, statically linked** | No dependency change; the table is `const` data. |
| **IV. Type-driven correctness** | No new types. The existing offline test is what makes a wrong row a build failure rather than a silent zero. |
| **V. Specification compliance** | CPE 2.3 strings in the existing native `cpe`/`cpes` slots; no `waybill:*` row added. |
| **VIII. Completeness** | 49 components become matchable that were reachable by nothing. |
| **IX. Accuracy** | The feature is this principle twice over: the right vendor, and **no** vendor where NVD's entries are a different product. |
| **X. Transparency** | Every disposition recorded, including the 3 rejections and the one thin call. |
| **XII. External data source enrichment** | No scan-time lookup. The dictionary observations are dated, committed facts. |
