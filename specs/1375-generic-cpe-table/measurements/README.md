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

## T013 / T014 — delivered, and a prediction corrected

```
with_candidates   78 → 126   +48
unaddressable    194 → 146   −48
addressable_without_cpe, excluded_members, excluded_files   unchanged
sum 977 == .components | length
```

Every one of the 48 carries the vendor `research.md` R4 assigned; **zero
vendor mismatches**.

### N is 48, not the 49 predicted

49 components carry an accepted name. One does not gain a CPE:

```
NO CPE: name='libedit' version='20251016-3.1'
```

`is_release_shaped` declines that version **upstream of the table** — and
milestone 1367's own comment names `20251016-3.1 (libedit)` as one of its
five declining shapes. The prediction counted the population without applying
a gate that was already documented in the file being edited.

`gettext` does appear twice and both gain a CPE, so that part of the
prediction held; it was simply offset by a component nobody had subtracted.

Corrected in `research.md`, `plan.md`, `data-model.md`, `contracts/table.md`
and `quickstart.md`.

## T007 — the guard was seen to fail before being trusted

| mutation | result |
| --- | --- |
| a row whose pair has no observation | **failed**, naming the pair and the script to run |
| an observation with a zero count | **failed** |

## T009a caught its own evidence file being wrong on the first run

`texinfo` timed out in the batch title sweep and was re-probed individually.
The committed file carried a note saying *"its result is appended above by
that run"* — **and it was not**. The note was written and the append was not.

`m1375_every_product_has_a_committed_title_query` failed on `texinfo` the
first time it ran, which is exactly what a traceability check is for: the
prose claimed evidence that was not there.

## T017 — the disposition of every name in the population

All **186** names, so none is left in an unknown state and the
population is answered once rather than re-probed (FR-007, SC-006).

| disposition | names |
| --- | --- |
| **accepted** | 48 |
| **rejected** | 3 |
| **nixpkgs-shaped** | 47 |
| **absent from NVD** | 88 |

Totals: 186 = 186.

- **accepted** — a dictionary-verified, title-confirmed row was added.
- **rejected** — in NVD, but its entries are a different product. Three:
  `ninja`, `cmake`, `libxcrypt`; see `research.md` R4 and the comment above
  the table in `cpe.rs`.
- **nixpkgs-shaped** — a construction name no one files advisories against
  (`bootstrap-stage0-stdenv-darwin`, `apple-sdk-private-frameworks-hook`,
  `cctools-binutils-darwin-wrapper`). Absence is **structural**; no probe
  is needed and none was wasted on them.
- **absent from NVD** — a canonical upstream name the dictionary simply
  does not carry. Probed, found empty, recorded. This is the population a
  future NVD addition could move, and the only one worth re-probing later.

<details><summary>the full list</summary>

- `AvailabilityVersions` — absent from NVD
- `CoreSymbolication` — absent from NVD
- `DarwinTools` — absent from NVD
- `adv_cmds` — absent from NVD
- `apple-sdk` — nixpkgs-shaped
- `apple-sdk-private-frameworks-hook` — nixpkgs-shaped
- `asciidoc` — absent from NVD
- `atf` — absent from NVD
- `auto-sign-darwin-binaries-hook` — nixpkgs-shaped
- `autoconf` — absent from NVD
- `automake` — accepted
- `autoreconf-hook` — nixpkgs-shaped
- `bash` — accepted
- `bash-interactive` — absent from NVD
- `bc` — accepted
- `bison` — accepted
- `bmake` — absent from NVD
- `bootstrap-stage-xclang-stdenv-darwin` — nixpkgs-shaped
- `bootstrap-stage0-clang-wrapper` — nixpkgs-shaped
- `bootstrap-stage0-stdenv-darwin` — nixpkgs-shaped
- `bootstrap-stage1-stdenv-darwin` — nixpkgs-shaped
- `bootstrap-stage2-stdenv-darwin` — nixpkgs-shaped
- `bootstrap-stage3-stdenv-darwin` — nixpkgs-shaped
- `bootstrap-stage4-stdenv-darwin` — nixpkgs-shaped
- `bootstrap_cmds` — nixpkgs-shaped
- `brotli` — accepted
- `byacc` — absent from NVD
- `bzip2` — accepted
- `c-ares` — accepted
- `cctools` — nixpkgs-shaped
- `cctools-binutils-darwin` — nixpkgs-shaped
- `cctools-binutils-darwin-wrapper` — nixpkgs-shaped
- `clang` — accepted
- `clang-wrapper` — nixpkgs-shaped
- `cmake` — rejected
- `cmake-minimal` — absent from NVD
- `compiler-rt` — accepted
- `compiler-rt-libc` — absent from NVD
- `copyfile` — absent from NVD
- `coreutils` — accepted
- `cpio` — accepted
- `cups-headers` — absent from NVD
- `die-hook` — nixpkgs-shaped
- `diffutils` — accepted
- `docbook-xml` — absent from NVD
- `docbook-xsl-nons` — absent from NVD
- `docbook-xsl-ns` — absent from NVD
- `ed` — accepted
- `ensure-newer-sources-hook` — nixpkgs-shaped
- `expat` — accepted
- `file` — accepted
- `file_cmds` — absent from NVD
- `find-xml-catalogs-hook` — nixpkgs-shaped
- `findutils` — accepted
- `fix-darwin-dylib-names-hook` — nixpkgs-shaped
- `flex` — accepted
- `gawk` — accepted
- `gdbm` — absent from NVD
- `getopt` — absent from NVD
- `gettext` — accepted
- `ghc` — absent from NVD
- `ghc-binary` — absent from NVD
- `ghc-platform` — absent from NVD
- `ghc-settings-edit` — absent from NVD
- `ghc-toolchain` — absent from NVD
- `git-minimal` — absent from NVD
- `gmp-with-cxx` — absent from NVD
- `gnu-config` — absent from NVD
- `gnugrep` — absent from NVD
- `gnum4` — absent from NVD
- `gnumake` — absent from NVD
- `gnused` — absent from NVD
- `gnutar` — absent from NVD
- `gtest` — absent from NVD
- `gtk-doc` — absent from NVD
- `gzip` — accepted
- `hadrian` — absent from NVD
- `help2man` — absent from NVD
- `install-shell-files` — absent from NVD
- `jq` — accepted
- `krb5` — absent from NVD
- `kyua` — absent from NVD
- `ld64` — absent from NVD
- `less` — accepted
- `libSystem` — absent from NVD
- `libarchive` — accepted
- `libcxx` — absent from NVD
- `libedit` — accepted
- `libev` — absent from NVD
- `libffi` — accepted
- `libiconv` — absent from NVD
- `libidn2` — accepted
- `libmd` — absent from NVD
- `libpng-apng` — absent from NVD
- `libpsl` — absent from NVD
- `libresolv` — absent from NVD
- `libsbuf` — absent from NVD
- `libssh2` — accepted
- `libstemmer` — absent from NVD
- `libtool` — accepted
- `libunistring` — accepted
- `libutil` — absent from NVD
- `libuv` — accepted
- `libxcrypt` — rejected
- `libxml2` — accepted
- `libxo` — absent from NVD
- `libxslt` — accepted
- `lld` — absent from NVD
- `llvm-binutils` — absent from NVD
- `llvm-tblgen` — absent from NVD
- `locale` — absent from NVD
- `lua` — accepted
- `lutok` — absent from NVD
- `lzip` — absent from NVD
- `lzo` — accepted
- `macOS-SDK` — absent from NVD
- `mailcap` — absent from NVD
- `make-binary-wrapper-hook` — nixpkgs-shaped
- `make-shell-wrapper-hook` — nixpkgs-shaped
- `meson` — absent from NVD
- `mpdecimal` — absent from NVD
- `ncurses` — accepted
- `nghttp2` — accepted
- `nghttp3` — absent from NVD
- `ngtcp2` — accepted
- `ninja` — rejected
- `nixpkgs` — absent from NVD
- `nss-cacert` — absent from NVD
- `oniguruma` — accepted
- `openpam` — absent from NVD
- `os-string` — absent from NVD
- `patch` — accepted
- `patchutils` — absent from NVD
- `pbzx` — absent from NVD
- `perl` — accepted
- `pkg-config` — absent from NVD
- `pkg-config-role-hook` — nixpkgs-shaped
- `pkg-config-wrapper` — nixpkgs-shaped
- `pkgconf` — accepted
- `pkgconf-wrapper` — nixpkgs-shaped
- `pkgs-config-setup-hook` — nixpkgs-shaped
- `publicsuffix-list` — absent from NVD
- `pypa-build-hook.sh` — absent from NVD
- `pypa-install-hook` — nixpkgs-shaped
- `pytest-check-hook` — nixpkgs-shaped
- `python-catch-conflicts-hook` — nixpkgs-shaped
- `python-imports-check-hook.sh` — absent from NVD
- `python-namespaces-hook.sh` — absent from NVD
- `python-output-dist-hook` — nixpkgs-shaped
- `python-relax-deps-hook` — nixpkgs-shaped
- `python-remove-bin-bytecode-hook` — nixpkgs-shaped
- `python-remove-tests-dir-hook` — nixpkgs-shaped
- `python-runtime-deps-check-hook.sh` — absent from NVD
- `python3` — absent from NVD
- `python3.13-bootstrap-build` — nixpkgs-shaped
- `python3.13-bootstrap-flit-core` — nixpkgs-shaped
- `python3.13-bootstrap-installer` — nixpkgs-shaped
- `python3.13-bootstrap-packaging` — nixpkgs-shaped
- `python3.13-bootstrap-pyproject-hooks` — nixpkgs-shaped
- `python3.13-bootstrap-tomli` — nixpkgs-shaped
- `re2c` — accepted
- `readline` — accepted
- `removefile` — absent from NVD
- `rhash` — accepted
- `setuptools-build-hook` — nixpkgs-shaped
- `shell_cmds` — absent from NVD
- `sigtool` — absent from NVD
- `stdenv-darwin` — nixpkgs-shaped
- `system_cmds` — absent from NVD
- `tcl` — accepted
- `texinfo` — accepted
- `tzdata` — absent from NVD
- `unifdef` — absent from NVD
- `unittest-check-hook` — nixpkgs-shaped
- `unzip` — accepted
- `update-autotools-gnu-config-scripts-hook` — nixpkgs-shaped
- `version-check-hook` — nixpkgs-shaped
- `which` — absent from NVD
- `wrap-python-hook` — nixpkgs-shaped
- `writable-tmpdir-as-home-hook` — nixpkgs-shaped
- `xar-minimal` — absent from NVD
- `xcbuild` — absent from NVD
- `xcode-project-check-hook` — nixpkgs-shaped
- `xz` — accepted
- `zlib-ng` — absent from NVD
- `zstd` — absent from NVD

</details>

## T020 — the mutation check

| mutation | result |
| --- | --- |
| a row's vendor "corrected" to the component name (`gnu:bash` → `bash:bash`) | **failed** |
| an observation deleted | **failed**, naming the pair |
| a `ninja` row added | **failed** |
| T016's control removed **and** `synthesize_cpes` broken outright | **passed** |

**The fourth is the informative one and it is meant to pass.** It demonstrates
what the control prevents: with the control gone, a synthesizer that returns
nothing for everything satisfies "the rejected names emit no CPE" perfectly.
The control is what makes that assertion mean something, and this is the
evidence rather than the claim.

Every mutation was reverted with `cp` from a backup. Milestone 1373 used
`git checkout` on an uncommitted file for one of its mutations, which deleted
the work under test while the gate still passed.

## T021 — no documentation change, and why

The task named `docs/reference/sbom-format-mapping.md`'s **C19** row and said:
if it does not discuss the table's coverage, **add nothing** and record that.

It does not. C19 describes the *carrier* for multiple CPE candidates — the
JSON-array-in-string shape, and #995's pipe-joined-vs-array fix — and says
nothing about which names the curated table covers. The table's coverage is
documented where it is maintained: in `cpe.rs`, above the table and in the
rejections block.

**Nothing was added.** Inventing a doc section to satisfy a task is worse than
the task turning out to be unnecessary.

## T022 / T023 — the gate, and the goldens that correctly did not move

```
GATE_RESULT=pass   371 targets   7,469 passed   0 failed   0 failure blocks
m1375 tests executed: 4 of 4
```

7,469 is four more than milestone 1374's 7,465, matching the four tests this
milestone adds — checked by enumerating which `m1375_*` tests the run
executed, not by reading the total.

**Zero local goldens moved**, which is the predicted outcome and the inverse
of the last two milestones. T004 established that no local fixture contains
any of the 48 names, so here a local golden moving would have been a
**finding** rather than a refresh. None did.

## T024 / T025 — the corpus refresh

Run `42395026732`. **Scanned commit `159a959a` is this branch's tip** — checked,
not assumed.

51 committed files, 51 in the artifact, none new, none missing. **Two targets
differ, which is the prediction**: `nix-closure-moat` (contains the names) and
`image-postgres16` (896 unrelated lines, #1372's drift, excluded). The other
15 are untouched — T004 said no other target carries these names.

### Every changed line in `nix-closure-moat` is attributable

| lines | what |
| --- | --- |
| 281 | CPE strings, C19 `waybill:cpe-candidates`, the C209 value |
| 246 | JSON structure — braces from the added objects |
| 45 | SPDX `externalRef` scaffolding (`referenceCategory: SECURITY`) |
| 118 | SPDX annotation fields for **8 new C19 annotations** |

The 8 are `c-ares`, `file`, `flex`, `libarchive`, `libuv`, `ncurses`, `unzip`,
`xz` — exactly the multi-candidate rows that exist on this closure. C19 is
emitted only when more than one candidate was synthesized, so 8 new ones is
the expected consequence of 10 multi-vendor rows, 8 of whose names are here.

### The x86_64-linux delta

```
with_candidates    6 →  51   +45
unaddressable    360 → 315   −45
addressable_without_cpe, excluded_members, excluded_files   unchanged
sum 1615 == .components | length
```

Larger than darwin's +48/−48 in proportion but smaller in absolute terms,
because the linux closure's unaddressable population is differently composed.
**Measured, not scaled.**

### A seven-component gap that was my arithmetic, not the product

An intermediate check reported 38 components gaining a `cpe` field against a
`with_candidates` delta of 45. The seven are not missing: I counted a **set of
names**, which collapses duplicates, and `bash` appears 3× on this closure
with `automake`, `diffutils`, `gawk`, `gettext` and `gzip` 2× each —
3−1 + 5×(2−1) = 7. Counting `(name, version)` gives 45 and reconciles.

Recorded because the first reading looked like components silently missing a
CPE, which would have been a real defect; it was a de-duplicating count.
