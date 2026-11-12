# #1455: runtime paths reached only through documentation outputs

Collected 2026-11-12 on aarch64-darwin, `nix (Determinate Nix 3.20.0) 2.34.6`,
against moat at `d0690555` (the `nix-closure-moat` corpus pin), whose `out`
output was already in the local store. Nothing was built or substituted.

**The decision (maintainer, on #1455):** a closure member the runtime closure
reaches only through documentation outputs does not count as shipping.

## Scripts

| script | output | what it does |
|---|---|---|
| `collect.sh <outdir> <output path>…` | not committed (6 MB) | `nix path-info -r` plain and `--json`, 3 timed runs each, and `nix derivation show -r` of each output's deriver |
| `analyze.py <outdir> <root>…` | `out/analysis.txt` | output names of the runtime paths, name vs path-suffix agreement, and what the rule sets aside |
| `chains.py <outdir> <regex> <root>…` | `out/chains-test-frameworks.txt` | the shortest reference chain to each matching package that avoids documentation outputs |
| `moat-scan.sh <waybill> <moat checkout> <outdir>` | `out/scan-{before,after}.txt`, `out/marked-{before,after}.txt` | the waybill binary's own scan, as the corpus harness runs it, then `sbom edit --drop 'scope=excluded'` in all three formats, counting the test-framework packages each runtime view keeps |

`out/why-moat-hspec.txt` is `nix why-depends --precise` from moat's `out` to hspec.

## How a documentation output is identified

By its **output name** in the derivation that produced it: the closure the
tier already reads (`nix derivation show -r`) gives `outputs.<name>.path` for
every member, so a runtime path's output name is a lookup, with no extra nix
call. On moat:

- all 295 runtime paths are named by a closure derivation output (0 unnamed);
- names: `out` 157, `doc` 123, `dev` 7, `lib` 4, `man` 1, `data` 1, `libtool` 1, `xcrun` 1;
- the `-doc`/`-man`/`-info`/`-devdoc` path suffix agrees with the name on all
  295. The name is still the one used: a package whose own name ends in
  `-doc` has an `out` output with that suffix (`closure-doc-outputs.json`
  pins that case).

The references come from the runtime query itself, switched from
`nix path-info -r` to `nix path-info -r --json` (same store, same
verification). On this host the two cost the same (`out/timing.txt`, local
store, warm: 0.119/0.141/0.138 s plain against 0.120/0.133/0.144 s `--json`).
A substituter's `--json` answer was not timed; it reads the same narinfos the
plain listing does, so the cost should match (inference). nix 2.34 warns that
a bare `--json` is deprecated in favour of `--json-format 1|2`; a nix older
than that flag rejects it, so waybill passes bare `--json` and parses format
1, format 2 and the pre-2.19 array.

### A `null` answer is a miss

The plain listing exits 1 with `path '…' is not valid` for an output the
store does not hold. `--json` need not:

- locally (nix 2.34.6), `nix path-info --json <unbuilt output>` prints
  `{"/nix/store/1lmn…-patchutils-0.4.2":null}` and exits 0; the recursive
  form exits 1 here;
- on the first corpus run of this change (public-corpus run 43162793943,
  x86_64-linux, the Determinate installer's nix), the recursive form did the
  same for moat's and shellcheck's outputs. Inference from the documents, not
  observed directly: C216 read `runtime-closure`, `store: local`, with one
  static-link root and nothing unmatched, i.e. a one-path closure, and nearly
  every member was marked.

So a `null` entry anywhere in the answer is `not-in-any-store`, and the next
store, or the walk, decides (`a_null_json_answer_is_not_a_runtime_closure`).

## What the rule changes on moat

`out/analysis.txt` (the probe) and `out/scan-after.txt` (the binary) agree:

- 126 of 295 runtime paths are documentation-only: 124 documentation outputs
  and 2 paths only they reach;
- 2 members are excluded only because of the rule: `bash-interactive 5.3p9`
  and `readline 8.3p3`, reached through `ghc-doc` (the same pair q7 found on
  hlint). Before vs after, the SPDX 2.3 `BUILD_TOOL_OF` set differs by exactly
  these two (`out/marked-*.txt`), and both carry C203 `runtime-closure`;
- the runtime view (`--drop 'scope=excluded'`, SPDX 2.3) goes from 185 to 183
  packages.

**It does not change the test frameworks.** The runtime view keeps 23 of 23
test-framework packages before and after (hspec and its parts, tasty and its
parts, QuickCheck and `quickcheck-*`, test-framework and its parts, HUnit,
doctest, ChasingBottoms, smallcheck; the regex is in `moat-scan.sh`, so the
count differs from the issue's 21). Each one's `out` output is referenced
from an `out` output, not from documentation (`out/chains-test-frameworks.txt`):
moat's own `out` has darwin `lib/links` symlinks to hspec, HUnit, QuickCheck
and tasty (`out/why-moat-hspec.txt`), and libraries such as
`unordered-containers`, `case-insensitive`, `vector` and `hashable` link the
rest the same way. Only their `-doc` outputs are documentation-only, and
those are set aside without excluding the member.
