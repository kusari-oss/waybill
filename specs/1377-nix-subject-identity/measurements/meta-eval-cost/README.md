# Reading `meta.identifiers` in milestone 1281's meta evaluation — 2026-11-09

nixpkgs `a799d3e3`, aarch64-darwin, Determinate Nix 3.20.0 (nix 2.34.6).
Names: the 358 distinct, name-safe component names of waybill's openssh
`--nix-closure` document (`names.txt`). `probe.py` reproduces it.

## Cost and safety — three variants of one expression

`original` is `licenses_expr` from `waybill-cli/src/scan_fs/package_db/nix/eval/licenses.rs`
verbatim. The other two add to each row's attribute set only:

- `naive`: `version = p.version or null; cpe = p.meta.identifiers.cpe or null;`
- `guarded`: the same, with `cpe` evaluated inside its own `builtins.tryEval`

One warm-up run per variant, then N = 3:

| variant | min | median | max | rows | rows with a license | rows with a CPE |
| --- | --- | --- | --- | --- | --- | --- |
| original | 0.79 s | 0.80 s | 0.80 s | 118 | 114 | 0 |
| naive | 0.79 s | 0.79 s | 0.79 s | 118 | 114 | 18 |
| guarded | 0.81 s | 0.82 s | 0.83 s | 118 | 114 | 18 |

- Adding identifiers costs nothing measurable: ±0.02 s on a 0.8 s call,
  within run-to-run spread, and no additional evaluation.
- License coverage is unchanged (114 → 114) in both extended forms.
- The naive form lost no row on this closure. It still risks it: every row is
  one `tryEval (deepSeq v v)`, so a package whose `identifiers` throws would
  drop that row's licenses too. The guarded form removes that risk for
  +0.02 s. That is a defence against an unobserved failure, not a fix for an
  observed one.

## Reach — the drvPath join, and the name+version fallback

The expression returns a row for `pkgs.<name>` (and `pkgs.haskellPackages.<name>`)
whether or not its `drvPath` matches the closure member; milestone 1281 joins
in Rust on the `.drv` basename. For the 17 components carrying a
nixpkgs-declared CPE in openssh's buildtime closure (sbomnix 1.9.0
`--exclude-cpe-matching`, which keeps only nixpkgs-declared CPEs), plus the root:

| | members |
| --- | --- |
| reached by exact `drvPath` | 15 of 17, and the root (`openssh-10.3p1.drv` = `pkgs.openssh`) |
| missed by `drvPath` | 2: `bash` 5.3p9 (stdenv's) and one of two `perl` 5.42.0 |
| missed members whose `pkgs.<pname>` has the same version and the same declared CPE | 2 of 2 |

`pkgs.bash`: version `5.3p9`, `meta.identifiers.cpe` = `cpe:2.3:a:gnu:bash:5.3:9:…`,
identical to the CPE sbomnix attaches to the stdenv `bash-5.3p9.drv`
(`g32bfwk6…`). Likewise both perl derivations carry `perl:perl:5.42.0`. A CPE
names vendor, product and version, not a build, so a name+version join
recovers exactly the declared CPE for the two members a drvPath join misses.
sbomnix evidently joins this way: it attaches the declared CPE to all three
build-context variants.

## Independent reach count for SC-009 (`reach.py`)

Population from waybill's own reader, join computed independently. Run:
`nix derivation show -r path:<flake>#default > closure.json`, then
`waybill --offline debug closure-dump --closure-json closure.json > dump.json`,
then `CLOSURE_DUMP=dump.json python3 -I -B reach.py`.

Openssh closure, 2026-11-09 (before any 1377 code):

| | count |
| --- | --- |
| wanted (population) | **670**, equal to C184's `licenses.wanted` (670) |
| received | 187 = 78 by drv + 109 by pname+version |
| with a declared CPE | 39 derivations |
| ambiguous (name+version rows with different CPEs) | 0 |
| unreached | 483 |

**Since milestone 1377 the population is read from the binary.** `debug closure-dump` reports `in_coverage_population` per row (`closure/emit.rs::coverage_drv_keys`: the emitted members plus the evaluated package), and reach.py uses that instead of re-deriving it. The evaluated package is counted (clarification 2026-11-09). openssh's root was already in the population because it applies patches, so its count stays 670. gh's root was not: its `wanted` moves from 721 to 722 and `received` from 85 to 86, and reach.py agrees on 722.

**Before that, getting the population right took two corrections, both recorded because each gave a plausible wrong figure:**

1. `emits_as_component` alone gives 732.
2. The population milestone 1281 counts (`closure/emit.rs::emitted_drv_keys`) also requires a pname, and for milestone-1282 members only roles `source`/`package`. The excluded 62 are `fragment` (59) and `scaffolding` (3) members.

**Why the license cross-check is a bracket, not an equality.** Counting drv-joined members with a license gives 76 when "license" means non-null and 74 when it means "parses under 1281's `one`/`lic`". waybill reports 75. The members that move between those counts are `llvm`, `lld` and `clang-wrapper`. Their `meta.license` is nixpkgs' compound shape, `{"licenseType":"compound","licenses":[…]}`, which 1281's parser does not read, so waybill emits **no license** for them. That is a separate license-coverage gap, outside this milestone.

The license count therefore cannot validate the join exactly. SC-009 compares C184's `identifiers` against this probe directly, after implementation, and requires exact equality.
