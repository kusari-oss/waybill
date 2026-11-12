# Recall probe for 1283

The scripts that measured waybill's recall on Nix-built Haskell projects
(the recall corpus, 2026-11-11), and the rule that turns scanner output into
the truth the nightly corpus targets are scored against (research R7).

Python runs as `python3 -I -B` throughout. `-I` keeps the script's own
directory off `sys.path`, which is why `recall.py` loads `classify.py` by path.

## How truth is defined

Truth is the set of real vulnerabilities in the shipped output of one flake:

1. **grype findings on sbomnix's runtime document.** sbomnix (runtime mode, its
   default) describes the realised output's runtime closure; grype scans that
   CycloneDX.
2. **minus name collisions.** A grype match on a Haskell package whose CVE
   describes another product of the same name (Haskell `warp` against
   Cloudflare WARP, Haskell `zlib` 0.x against the C library) is excluded.
   The rule is `classify()` in `classify.py`:

   | condition | verdict |
   |---|---|
   | not a Haskell package | right-product |
   | Haskell, and vendors C sources (`BUNDLED_C`) | questionable (excluded) |
   | Haskell, and a CVE description names Haskell or Hackage | right-product |
   | Haskell otherwise | collision (excluded) |

   "Haskell package" means the matched (name, version) is a member of the
   target's Haskell package set. `recall.py` took that set from waybill's own
   document; the truth workflow takes it from nix (`link-closure.nix`,
   `universe`), so the truth does not depend on the tool it scores.

   The rule is also checked against `REVIEWED`, the verdicts a person reached
   by reading the CVE text in stage 2 (`../stage2-classify.tsv`). A
   disagreement stops the truth file from being written (exit 2). Checked on
   openai's stage 2 data with the Haskell set taken from waybill's degraded
   document: the check fired on `foundation` and `hedgehog`, which that set
   lacked. That is the failure it exists to catch, and the reason the
   workflow's set is nix's `universe` rather than the link closure alone.
3. **plus HSEC advisories.** osv-scanner on a minimal `pkg:hackage` document
   of the output's Haskell **link** closure (`link-closure.nix`, `packages`:
   library and executable dependencies, transitively).

Each finding's ID is canonicalised to its CVE alias when it has one
(`canon()`), so a GHSA or HSEC and its CVE count once. Findings are keyed by
(canonical ID, normalised name, version).

### The truth file

`waybill-cli/tests/fixtures/public_corpus/<target>/recall-truth.json`, the shape
in data-model §RecallTruth. Two additions, both read by the harness:

- `matcher.cpe` (and `matcher.purl`) may be a list when one finding was
  reached through more than one CPE vendor:product; any of them matches.
- A finding reported by both grype and osv-scanner carries both a `cpe` and
  a `purl` matcher; either satisfies it.

A right-product grype finding with no CPE stops the build (exit 2): the proxy
would have nothing to match it on, and dropping it would shrink the truth
without saying so.

`excluded[].excerpt` is folded to ASCII (NFKC, then `?` for anything left),
because CVE descriptions carry no-break spaces and the repository's homoglyph
check rejects them in committed files.

### What the nightly checks

`expectations::recall_proxy` in `waybill-cli/tests/corpus_harness_195/`. For
each finding, the runtime-only view of waybill's CycloneDX (components walked
recursively, `scope: excluded` dropped with its subtree) must hold a component
with the same normalised name and version and a CPE whose vendor:product
equals the matcher's, or a purl equal to the matcher's ignoring qualifiers.
At least 90% must be found. A missing truth file fails the target and names
the dispatch below.

## Producing the truth (CI)

`.github/workflows/recall-truth.yml`, dispatch only, x86_64-linux:

```sh
gh workflow run recall-truth.yml --ref <branch> -f targets=all
```

`targets` takes names from `targets.tsv`, space- or comma-separated, or `all`.
Each target's artifact `recall-truth-<target>` holds `recall-truth.json` and
every intermediate (sbomnix document, grype and osv-scanner output, link
closure, tool versions, grype database status). Review the `excluded` list
against the rule above, then commit `recall-truth.json` beside the target's
goldens.

Pinned: sbomnix 1.9.0 from its own flake at the v1.9.0 tag
(`github:tiiuae/sbomnix/95480c2168f0156eea4a3aac34d2bbb964aa3b4b`, the build
the measurement used; nixpkgs' sbomnix does not build on Linux at the revision
below), grype 0.120.0 and osv-scanner 2.7.0 from nixpkgs
`0bb2eedc20e4739f29e8045034f2828ad7054f8f`; the grype database by archive URL
and sha256 (schema v6.1.10, built 2026-11-08). osv-scanner reads the live OSV
API, so the HSEC half of truth is as of the run date (`produced`).

## The scripts

| file | what it does | invocation |
|---|---|---|
| `classify.py` | the classifier, plus `truth` (writes recall-truth.json) and `link-cdx` (link closure → pkg:hackage CycloneDX) | `python3 -I -B classify.py truth --grype S.cdx.json.grype.json --osv link.cdx.json.osv.json --link-closure link-closure.json --target-pin <sha> --tool k=v... --out recall-truth.json` |
| `link-closure.nix` | the Haskell link closure and universe of a flake output | `nix eval --json --option allow-import-from-derivation true --file link-closure.nix --argstr flake <flake ref> --argstr system x86_64-linux --argstr attr default result` |
| `targets.tsv` | the IFD targets, pinned; read by the workflow, checked against the corpus manifest | — |
| `recall.py` | the stage 1 recall measurement (8 nixpkgs packages): W/WR recall, miss causes, adjudicated truth | `python3 -I -B recall.py <work dir>` |
| `survey.py` | stage 1 per-arm counts, C190, C216 | `python3 -I -B survey.py <work dir>` |
| `realise.sh` | stage 1: realise each wrapper's output from cache only | `RECALL_CORPUS_DIR=<work dir> ./realise.sh <pkg>...` |
| `scan.sh` | stage 1: grype + osv-scanner over each arm | `RECALL_CORPUS_DIR=<work dir> GRYPE=<grype> ./scan.sh <pkg> [W WR S]` |

`<work dir>` is the stage 1 layout: `wrappers/<pkg>/` (one flake per package),
`out/<pkg>/{W,WR,S}.cdx.json` plus scanner output, `realise/`, `storecheck/`,
`linkage/`, `linkcheck/`. The stage 1 artifacts themselves are not committed;
the results are in `../README.md`.

`recall.py` was checked after the classifier moved out of it: on the stage 1
work directory it writes a `recall.json` byte-identical to the original
script's, with identical stdout.
