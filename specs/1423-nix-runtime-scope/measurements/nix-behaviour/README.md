# How nix behaves, for #1423

These probes check how nix itself behaves before the plan relies on it: reading the configured substituters, querying a closure without building, timing, offline behaviour, multi-output packages, and how Haskell static linking appears in derivations.

- **Date:** 2026-11-10 (EST)
- **Host:** Michaels-MacBook-Pro.local, aarch64-darwin, Darwin 27.2.0 (arm64)
- **nix:** `nix (Determinate Nix 3.20.0) 2.34.6`, multi-user. The daemon reports `{"trusted":false}` for this user, and `trusted-users = root`.
- **nixpkgs:** `github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb`, written `$R` below.
- **Python:** every script is run as `python3 -I -B`.
- **Network:** this host's network was never switched off. For Q5, network loss was simulated with proxy variables; see Q5 for what that does and doesn't cover.

Each `q<N>-*.sh` script writes raw stdout, stderr, exit code and wall time to `q<N>-out/`. `timed.py` captures exit status and wall time per command, in one process. Anything inferred rather than measured is marked **(inference)**.

## Files

| file | what it does |
|---|---|
| `q1-config.sh` → `q1-out/` | reads substituters and keys three ways, plus where the config comes from and whether the user is trusted |
| `outpaths.sh` → `outpaths.tsv` | outPath, drvPath, outputs, outputName and local validity for each probe attribute |
| `q2-query.sh` → `q2-out/` | local store vs `--store <url>`, two `--store` flags, `nix build --dry-run`, signatures, an uncached path |
| `q2-deriver.sh` → `q2-out/deriver-*` | compares the `deriver` path-info reports with the drvPath the evaluation gives |
| `q2-priority.sh` → `q2-out/cache-info.*` | the `Priority` each configured cache advertises |
| `q3-timing.sh` → `q3-out/`, `q3-summary.txt` | 5 runs × {local, cache warm, cache cold} × 4 attributes |
| `q4-multi-output.sh` → `q4-out/`, `q4-summary.txt` | curl's outputs and each output's closure |
| `q5-offline.sh`, `q5b-offline-flag.sh` → `q5-out/`, `q5-summary.txt` | dead proxy (refused and blackholed), timeouts, retries, `--offline` |
| `q5c-connect-timeout-units.sh` → `q5-out/c-*` | which `connect-timeout` values nix honours (whole seconds; 0 is no limit) |
| `cabal-closure.nix` | truth set: transitive Haskell **library** deps from `getCabalDeps` |
| `q6-static-link.sh` + `q6-rule.py` → `q6-out/*.rule-score.txt` | scores candidate derivation-field rules against that truth |
| `q6-binary.sh` + `q6-linked-units.py` + `q6-binary-vs-rule.py` → `q6-out/*.linked-units.tsv`, `*.binary-vs-rule.txt` | second truth: the GHC units with code defined in the shipped binary |
| `q6-fp-why.py` → `q6-out/pandoc.fp-why.txt` | which derivation propagates a false positive |
| `q7-graph.py`, `q7-why.sh`, `q7-hs-extra.py`, `q7-linux-compare.sh` → `q7-out/` | why hlint's runtime closure holds -doc outputs, the C toolchain and test frameworks |
| `help/` | `nix path-info --help`, `nix build --help`, `nix store --help` as installed |
| `q8-verify-cost.sh` → `q8-out/`, `q8-summary.txt` | T002 / R11: `nix store verify --no-contents -r` cost beside the query it follows, cold and warm |
| `q9-installable.sh` → `q9-out/` | T001: path-info and verify given the **installable** rather than an outPath |
| `q9-default-output.sh` + `q9-default-output.py` → `q9-out/d-*` | T001: whether the default output can be read from the closure itself |
| `q10-linux-shellcheck.sh` → `q10-out/` | whether the x86_64-linux runtime closure of the shellcheck corpus target holds its static `ShellCheck-` build, against aarch64-darwin; and which Haskell builds the wrapper root consumes |

`q6-out/*.drv-closure.json` total about 36 MB. They are `nix derivation show -r` output, and `q6-static-link.sh` regenerates any that are missing.

---

## Q1. Reading the configured substituters and trusted keys

```sh
nix config show substituters            # exit 0
nix config show trusted-public-keys     # exit 0
nix config show trusted-substituters    # exit 0
nix config show extra-substituters      # exit 1: "error: could not find setting 'extra-substituters'"
nix show-config                         # exit 0, stderr: "warning: 'show-config' is a deprecated alias for 'config show'"
nix config show --json                  # exit 0, 77 KB
```

All three work on 2.34.6. `show-config` is deprecated.

- **`nix config show <name>`** prints one line, space-separated:
  ```
  https://cache.nixos.org/ https://install.determinate.systems
  ```
- **`nix config show` / `nix show-config`** print `name = value` lines. For example, `substituters = https://cache.nixos.org/ https://install.determinate.systems`.
- **`nix config show --json`** gives an object keyed by setting name. Each value looks like `{"aliases","defaultValue","description","documentDefault","experimentalFeature","value"}`, where `value` is a JSON array for list settings. Example: `"substituters": {"value": ["https://cache.nixos.org/", "https://install.determinate.systems"], ...}`. Full selection: `q1-out/config-show-json.selected`.

`extra-*` settings are already merged into the base setting and are not keys of their own. `/etc/nix/nix.conf` sets `extra-substituters = https://install.determinate.systems`, and that URL appears inside `substituters`.

Other values on this host:

| setting | value |
|---|---|
| `trusted-substituters` | `https://cache.flakehub.com https://install.determinate.systems` |
| `require-sigs` | `true` |
| `connect-timeout` | `15` |
| `download-attempts` | `5` |
| `stalled-download-timeout` | `300` |
| `narinfo-cache-positive-ttl` | `2592000` (30 days) |
| `narinfo-cache-negative-ttl` | `3600` |

The values are not uniform: `cache.nixos.org/` has a trailing slash and `install.determinate.systems` has none. `--store` accepts both forms (Q2).

These are the client's settings, read from nix.conf. The daemon may run with different ones; that was not measured.

## Q2. Querying a substituter for a closure without building

`SC` = shellcheck's outPath `/nix/store/xpjnkisx…-shellcheck-0.11.0-bin`, absent from this host's store. `CURL` = curl's outPath, present.

**a) No `--store`: does path-info consult substituters, or fail?** It does both. It asks the substituters what is missing, prints that to stderr, then fails with exit 1:

```
$ nix path-info -r $SC          # same with --json
these 2 paths will be fetched (15.1 MiB download, 116.9 MiB unpacked):
  /nix/store/0qxyki0k…-ShellCheck-0.11.0
  /nix/store/xpjnkisx…-shellcheck-0.11.0-bin
error: path '/nix/store/xpjnkisx…-shellcheck-0.11.0-bin' is not valid
```

The "fetched" list covers only the paths missing locally (2 of the 5). `nix path-info --help` says so in words: "does not build or substitute … if the corresponding store paths don't already exist, this command will fail". `--store local` fails for a non-root user (`opening lock file "/nix/var/nix/db/big-lock": Permission denied`), so the local store has to be queried through the default (daemon) store.

**b) `--store <substituter-url>`, per configured substituter**

| store | result |
|---|---|
| `https://cache.nixos.org/` | exit 0, 5 paths |
| `https://cache.nixos.org` | exit 0, 5 paths |
| `https://install.determinate.systems` | exit 1, `path … is not valid`; that cache doesn't hold shellcheck |

`--store <url>` is opened in the client process, so it works for any URL, whatever `trusted-substituters` says.

**c) Is there one invocation that tries them all?** Not through `path-info`:

- Given two `--store` flags, the last one wins (`c-two-stores-*`).
- `nix build --dry-run $SC` and `nix-store --realise --dry-run $SC` consult every configured substituter, build nothing and exit 0. But they print only a human-readable "will be fetched" list on stderr, and it omits paths already valid locally. `nix build --dry-run --json` prints only `["<outPath>"]`. No machine-readable closure comes out of either.
- So the caller must loop over the substituters itself.

**Order.** Nix orders substituters by the `Priority` each cache advertises, lower first, not by their position in the setting (the `substituters` help text says so). Measured with `q2-priority.sh`: `cache.nixos.org` advertises `Priority: 40` and `install.determinate.systems` advertises `Priority: 39`. Nix therefore asks the second-listed cache first.

**d) Signatures**

- `nix path-info --json` includes `"signatures": [...]` with or without `--sigs`. Example: `"cache.nixos.org-1:NZ9Trx…"`.
- **path-info does not verify signatures.** With `--option trusted-public-keys 'bogus-1:AAAA…'` it still exits 0 and prints all 5 paths.
- **`nix store verify --no-contents -r --store https://cache.nixos.org $SC` does verify:**
  - trusted keys: exit 0
  - bogus key: exit 2, with `path '…' is untrusted` for each of the 5 paths
- **JSON format:** `--json-format 1` gives full paths keyed at the top level. `--json-format 2` gives `{"info": {<basename>: …}, "storeDir", "version": 2}` with basenames. The help says `--json-format` "will be required in a future release".
- **Each entry also carries `deriver`.** See the FR-003 finding below.

**e) A path missing from every cache.** The probe path is `hello.overrideAttrs (_: { WAYBILL_PROBE = "1423"; })`, giving `/nix/store/qzavl6ss…-hello-2.12.3`. The default store, `--store https://cache.nixos.org` (with or without `--json`, and repeated) and `--store https://install.determinate.systems` all return the same:

```
exit 1
stderr:
don't know how to build these paths:
  /nix/store/qzavl6ss…-hello-2.12.3
error: path '/nix/store/qzavl6ss…-hello-2.12.3' is not valid
```

stdout is empty. `nix build --dry-run` on the same path exits **0**, printing the same "don't know how to build" lines.

### Finding for FR-003: `deriver` is not the evaluated `.drv`

| | |
|---|---|
| curl's eval drvPath | `6jvivpv1…-curl-8.20.0.drv` |
| deriver reported for curl's output, local store and cache alike | `pdhn4vf0…-curl-8.20.0.drv` |
| eval drv's `outputs.bin.path` (`nix derivation show`) | `56qb7awb…-curl-8.20.0-bin`, the queried path |

For shellcheck, the deriver and the eval drvPath happen to agree. All 20 of curl's local and cache derivers agree with each other (`q2-out/deriver-compare.txt`).

So matching runtime paths to closure members works by **output path** (from the drv's `outputs.*.path`), not by `deriver`. **(inference)** Input-addressed outputs can come from several equivalent `.drv`s, for example ones that differ only in fixed-output fetcher inputs, and nix records whichever built first.

## Q3. Timing of `nix path-info -r` (5 runs each, seconds)

| attr | mode | min | median | max | exit | paths |
|---|---|---|---|---|---|---|
| shellcheck | local | 0.086 | 0.089 | 0.134 | 1 | 0 |
| shellcheck | cache-warm | 0.057 | 0.059 | 0.062 | 0 | 5 |
| shellcheck | cache-cold | 0.262 | 0.270 | 0.308 | 0 | 5 |
| curl | local | 0.080 | 0.088 | 0.094 | 0 | 20 |
| curl | cache-warm | 0.058 | 0.059 | 0.065 | 0 | 20 |
| curl | cache-cold | 0.749 | 0.760 | 0.827 | 0 | 20 |
| haskellPackages.hlint | local | 0.098 | 0.141 | 0.422 | 1 | 0 |
| haskellPackages.hlint | cache-warm | 0.069 | 0.087 | 0.174 | 0 | 322 |
| haskellPackages.hlint | cache-cold | **9.982** | **11.006** | **11.925** | 0 | 322 |
| pandoc (= pandoc-cli) | local | 0.071 | 0.082 | 0.083 | 1 | 0 |
| pandoc | cache-warm | 0.053 | 0.053 | 0.056 | 0 | 8 |
| pandoc | cache-cold | 0.350 | 0.361 | 0.380 | 0 | 8 |

- **local:** the default store. Only curl's output is present; the others fail fast.
- **cache-warm:** `--store https://cache.nixos.org`, served from nix's narinfo disk cache (positive TTL 30 days).
- **cache-cold:** the same plus `--option narinfo-cache-positive-ttl 0 --option narinfo-cache-negative-ttl 0`, so every narinfo is fetched over the network.

Cold cost scales with closure size: about 34–38 ms per path (curl 0.76 s / 20, hlint 11.0 s / 322), which suggests the narinfos are fetched one at a time **(inference)**. `pandoc` is the top-level name; there is no top-level `pandoc-cli` attribute at this revision.

## Q4. Multi-output: curl (`bin dev out man devdoc`)

- `curl.outputName = bin`, so `curl.outPath` is the **`bin`** output (`…-curl-8.20.0-bin`).
- `curl.meta.outputsToInstall = ["bin","man"]`.
- `nix path-info -r` on one output returns that output's closure only, and other outputs appear only if referenced:

| output | closure | other curl outputs inside |
|---|---|---|
| bin | 20 | `out` (libcurl) |
| out | 19 | none |
| dev | 47 | `bin`, `out`, `man` |
| man | 1 | none |
| devdoc | 1 | none |
| union of all five | 48 | |

Locally, bin, out and man are valid; dev and devdoc are not (`q4-out/local.*`).

## Q5. Offline

The probe simulates network loss with `HTTPS_PROXY`, `HTTP_PROXY`, `https_proxy`, `http_proxy` and `NO_PROXY=localhost,127.0.0.1,::1`, in two variants:

- **refused:** proxy `http://127.0.0.1:9`; a control `curl` fails at once with exit 7.
- **blackhole:** proxy `http://192.0.2.1:9` (TEST-NET-1); connections time out.

The proxy reaches only the nix **client**. Queries the client forwards to the daemon (the default store) run in the daemon's unproxied environment, so those cases do not test network loss for the daemon. `cold` means narinfo TTLs set to 0, as in Q3. Runs: n=3. Full data: `q5-summary.txt`.

| case | min / med / max (s) | exit | result |
|---|---|---|---|
| refused, local, path present (curl) | 0.087 / 0.087 / 0.089 | 0 | 20 paths; local store unaffected |
| refused, local, path absent | 0.084 / 0.084 / 0.085 | 1 | still prints "these 2 paths will be fetched" (the daemon) |
| refused, cache, **warm** | 0.056 / 0.058 / 0.059 | **0** | **5 paths, answered from the narinfo disk cache with no network** |
| refused, cache, cold | 4.153 / 4.380 / 4.780 | 1 | 5 attempts, backoff 316 / 690 / 1004 / 2085 ms |
| refused, cache, cold, `download-attempts 1` | 0.062 / 0.064 / 0.076 | 1 | one attempt |
| refused, cache, cold, nix `--offline` | 0.056 / 0.057 / 0.057 | 1 | **still attempts the download**, once |
| refused, cache, warm, nix `--offline` | 0.054 / 0.056 / 0.058 | 0 | disk cache |
| blackhole, cache, cold (defaults: connect-timeout 15, attempts 5) | **79.263 / 79.357 / 79.594** | 1 | `Connection timed out after 15000 milliseconds` ×5 |
| blackhole, cold, `connect-timeout 2` | 14.227 / 14.701 / 15.247 | 1 | 5 × 2 s + backoff |
| blackhole, cold, `connect-timeout 2`, `download-attempts 1` | 2.121 / 2.127 / 2.140 | 1 | |
| blackhole, cold, `connect-timeout 2`, `download-attempts 2` | 4.332 / 4.419 / 4.481 | 1 | |
| blackhole, **hlint** (322 paths), `connect-timeout 2`, attempts 1 | 2.064 / 2.069 / 2.128 | 1 | fails on the first narinfo, the root; no per-path accumulation |
| default store, absent, nix `--offline` (`q5b`) | 0.096 | 1 | "don't know how to build these paths"; no substituter report |

The stderr of a substituter timeout looks like:

```
warning: unable to download 'https://cache.nixos.org/<hash>.narinfo': Timeout was reached (28) Connection timed out after 15000 milliseconds; retrying in 269 ms (attempt 1/5)
…
error: unable to download 'https://cache.nixos.org/<hash>.narinfo': Timeout was reached (28) Connection timed out after 15000 milliseconds
```

Conclusions:

- `--option connect-timeout N --option download-attempts M` bound the failure time to about `M × N` seconds plus backoff.
- With defaults, an unreachable cache costs about 79 s.
- nix's own `--offline` does **not** suppress an explicit `--store https://…` query. It suppresses only the daemon's substituter report.
- A warm narinfo cache answers correctly with the network down.

### Q5c. `connect-timeout` takes whole seconds, and 0 means no limit

`q5c-connect-timeout-units.sh` → `q5-out/c-*`. No network: `nix config show --option connect-timeout <v> connect-timeout` prints the value nix would use.

| value passed | effective value | stderr |
|---|---|---|
| `2` | 2 | |
| `1` | 1 | |
| `1.5` | **15** (the default) | `warning: setting 'connect-timeout' has invalid value '1.5'` |
| `0.5` | **15** (the default) | same warning |
| `0` | 0 | (the setting's description: "A value of 0 means no limit") |

So the remote budget can shrink the timeout only in whole seconds, and never below 1: a fractional value silently becomes 15 s and 0 removes the bound, each worse than the 2 s it replaced. waybill passes `min(2, floor(remaining))` and does not start a substituter with under 1 s left (`closure/runtime.rs::query_with`).

## Q6. Static linking in Haskell

### Where the libraries are recorded

`shellcheck` at this revision is a plain `stdenv.mkDerivation` (`q6-out/shellcheck.drv.json`) with outputs `bin man doc out`.

- Its `buildInputs` and `propagatedBuildInputs` are empty, and `nativeBuildInputs` holds only pandoc-cli (for the man page).
- It reaches the Haskell build only through `inputs.drvs` (`zkqbza9w…-ShellCheck-0.11.0.drv`) and a string in `installPhase`: `install -Dm755 /nix/store/0qxyki0k…-ShellCheck-0.11.0/bin/shellcheck $bin/bin/shellcheck`.
- That ShellCheck output, `0qxyki0k…`, the `justStaticExecutables` build, **is in the runtime closure** (5 paths: libffi, gmp-with-cxx, libiconv, ShellCheck-0.11.0, shellcheck-bin). None of its libraries are.

The static ShellCheck derivation (`q6-out/ShellCheck-static.env.txt`):

- **`buildInputs`:** aeson, Diff, fgl, QuickCheck, regex-tdfa, listed **3 times**. These are the generic builder's test + lib + exe lists. Because the package is built as a non-library, the lib/exe deps land in `buildInputs`, and `doCheck = 1` adds the test deps there too.
- **`propagatedBuildInputs`:** empty.
- **`nativeBuildInputs`:** ghc and remove-references-to. `depsBuildBuild`: ghc. `disallowedRequisites`: ghc.

For a Haskell **library** derivation, lib + exe deps are in `propagatedBuildInputs`, and test deps go into `buildInputs` only when `doCheck` is set.

### Is QuickCheck test-only for ShellCheck?

No.

- The cabal file (`q6-out/ShellCheck-0.11.0.cabal`, from the Hackage tarball in the cache) lists `QuickCheck >= 2.14.2 && < 2.17` under **`library` build-depends** (line 61), as well as under the executable and the test-suite.
- `getCabalDeps` (`q6-out/ShellCheck.getCabalDeps.json`) agrees: QuickCheck is in `libraryHaskellDepends`. ShellCheck's test-suite deps are a subset of its library deps, so it has **no test-only dependency at all** (`testOnlyDirect = []`).
- The binary has 4,004 defined QuickCheck symbols (`q6-out/shellcheck.linked-units.tsv`).

### Two truth sets

- **CABAL:** `cabal-closure.nix` starts from the package's `libraryHaskellDepends ++ executableHaskellDepends` and adds each library's `libraryHaskellDepends`, transitively, from the `getCabalDeps` passthru. Boot packages are `null` there and drop out.
- **BIN:** units with defined (non-`U`) symbols in the binary. The binary is read from the cache with `nix store cat` into a scratch dir outside the repo. Symbol names are z-decoded with `q6-linked-units.py`, and units without their own derivation (GHC boot libraries) are listed separately. BIN is "code that survived dead-stripping". It cannot see pure C libraries such as `libyaml-clib`, or TH-only packages such as `file-embed` and `th-lift`.

### Candidate rules

Each rule starts from every Haskell derivation (env has `setupCompilerEnvironmentPhase`) with an output in the runtime closure, walks derivation fields, and matches by output path.

- **R1:** root `buildInputs + propagatedBuildInputs`, then `propagatedBuildInputs` transitively through Haskell derivations.
- **R2:** `buildInputs + propagatedBuildInputs` at every step.
- **R3:** `inputs.drvs` at every step.
- **R4:** `propagatedBuildInputs` only.
- **R5:** like R1, but the root's start set comes from `getCabalDeps` lib+exe (eval-time metadata, not in the `.drv`).

Scores against CABAL, counting only libraries the runtime closure doesn't already hold (`q6-out/*.rule-score.txt`):

| target | runtime paths | CABAL | R1 found / P / R | R2 P | R3 P | R4 R | R5 found / P / R |
|---|---|---|---|---|---|---|---|
| shellcheck | 5 | 50 | 50 / **1.000** / **1.000** | 0.467 | 0.463 | 0 | 50 / 1.000 / 1.000 |
| pandoc (pandoc-cli) | 8 | 218 | 220 / 0.991 / 1.000 | 0.692 | 0.677 | 0 | 220 / 0.991 / 1.000 |
| ormolu (static, has test-only deps) | 4 | 29 | 75 / **0.387** / 1.000 | 0.248 | 0.240 | 0 | 29 / 1.000 / 1.000 |
| hlint (dynamic) | 322 | 73 | adds 0; all 73 already in the runtime closure | | | | |
| hadolint (dynamic) | 363 | 94 | adds 0; all 94 already in the runtime closure | | | | |

The same rules against BIN (`q6-out/*.binary-vs-rule.txt`):

| target | BIN libs with own drv | BIN ⊆ R1 | R1 precision vs BIN | CABAL − BIN (declared, no code survived) |
|---|---|---|---|---|
| shellcheck | 28 (+17 GHC boot) | yes | 28 / 50 = 0.56 | 22 (OneTuple, StateVar, comonad, optparse-applicative, tasty, …) |
| pandoc | 177 (+20 boot) | yes | 177 / 220 = 0.80 | 41 |
| ormolu | 22 (+18 boot) | yes | 22 / 75 = 0.29 | 7 |

- **No library with surviving code was missed by R1 or by CABAL** in any of the three binaries.
- **pandoc's 2 R1 false positives** (`attoparsec-aeson`, `cmdargs`): `aeson-pretty` is a lib+exe package, and the generic builder propagates its `executableHaskellDepends` as well (`q6-out/pandoc.fp-why.txt`, `q6-out/aeson-pretty.getCabalDeps.json`). Neither appears in the pandoc binary.
- **ormolu's 46 R1 false positives** come from its test-only deps (`hspec`, `hspec-megaparsec`, `path`, `path-io`, `QuickCheck`, `temporary`) and their library closures. They sit in the static root's `buildInputs` because `doCheck = 1`. None is in the binary.

**Candidate rule.** From each runtime member that is a Haskell generic-builder derivation:

1. Take its `buildInputs + propagatedBuildInputs` that are Haskell derivations.
2. Follow `propagatedBuildInputs` transitively through Haskell derivations.
3. Match by output path.

Score: shellcheck 50/50 exact against cabal metadata, and a superset of the 28 libraries with code in the binary.

**Test-only dependencies are not distinguishable in the derivation fields** at a static root built with `doCheck`. The generic builder puts test, lib and exe deps into the same `buildInputs` (ormolu: 46 false positives, precision 0.387). Telling them apart needs `getCabalDeps` (R5: 29/29 on ormolu), which is a passthru seen at evaluation, not something in the `.drv`. Within libraries, test deps don't leak, because the walk follows only `propagatedBuildInputs`. Exe-only deps of lib+exe packages do leak: 2 on pandoc.

## Q7. hlint: linked, or merely referenced?

hlint (dynamically linked) has 322 runtime paths on aarch64-darwin. `q7-graph.py` works from the narinfo `references` alone; `q7-why.sh` runs `nix why-depends --precise --store https://cache.nixos.org`.

**-doc outputs (134).** Haddock links them:

- `hlint-3.10 → hlint-3.10-doc` via `lib/ghc-9.10.3/lib/package.conf.d/hlint-3.10-….conf` (`haddock-interfaces:`)
- `hlint-3.10-doc → aeson-2.2.4.1-doc` via `share/doc/hlint-3.10/html/Language-Haskell-HLint.html` (`<a href="file:///nix/store/…-aeson-2.2.4.1-doc/…`)

Removing the -doc outputs leaves 186 paths. Only `bash-interactive` and `readline` are reachable solely through a -doc output: `ghc-9.10.3 → ghc-9.10.3-doc → bash-interactive`.

**C toolchain (cctools, clang, llvm, ld64, xcbuild, apple-sdk, …).** All of it comes through ghc:

- `hlint-3.10 → ghc-9.10.3`. The first file `why-depends` shows is `…/hlint-3.10-…/CmdLine.dyn_hi`, an interface file that embeds a ghc store path.
- **(inference)** The dynamic `libHS*` dylibs of GHC's boot packages, which live in ghc's store path, also reference it. This was not separately measured.
- ghc's `lib/ghc-9.10.3/lib/settings` names `clang-wrapper-21.1.8/bin/cc` (C/C++/CPP), `cctools-binutils-darwin-wrapper-1010.6/bin/{ar,ld,ranlib}` and `cctools-binutils-darwin-1010.6/bin/{otool,install_name_tool}` (`q7-out/ghc.settings.storepaths`).
- Example chains:
  - `hlint → ghc → cctools-binutils-darwin → cctools`
  - `hlint → ghc → clang-wrapper → clang`
  - `hlint → ghc → cctools-binutils-darwin → llvm`
- 32 runtime paths are reachable only through ghc. Removing both the -doc outputs and ghc leaves 154.

**Test frameworks.** 62 runtime Haskell libraries are not in hlint's cabal library closure of 73: hspec\*, tasty-\*, test-framework\*, doctest, ChasingBottoms, HUnit, … (`q7-out/hlint.hs-extra.txt`). They arrive through darwin `lib/links` symlinks:

- `yaml-0.11.11.2/lib/links/libHShspec-2.11.17-….dylib -> /nix/store/…-hspec-2.11.17/…`
- the same pattern in `hlint → tasty`, `unordered-containers → ChasingBottoms` and `case-insensitive → test-framework`

The same libraries built for x86_64-linux, queried from the cache (`q7-out/xsys.summary.txt`):

| package | x86_64-linux closure | aarch64-darwin closure | test-framework paths, linux / darwin |
|---|---|---|---|
| hlint | 220 | 322 | 4 / 54 |
| yaml | 183 | 287 | 4 / 54 |
| unordered-containers | 74 | 138 | 0 / 34 |

So the test-dependency inflation in runtime closures is **darwin-specific** at this revision.

---

## Where this contradicts or sharpens the spec

1. **FR-004, "in nix's configured order".** Nix orders substituters by their advertised `Priority`, not by list position. Here `install.determinate.systems` (39) is asked before `cache.nixos.org` (40). No single `path-info` invocation queries several stores, so waybill has to loop itself.
2. **FR-003, "derivation identity (the `.drv` that produced the path)".** The `deriver` that path-info reports is not the evaluated drv for curl, either locally or in the cache. The reliable join is the evaluated drv's `outputs.*.path` against the runtime path.
3. **SC-005 and the "0–1 s" figure.** These hold only with a warm narinfo cache or a small closure. With cold caches, hlint (322 paths) takes 9.98–11.93 s. An unreachable cache with nix's defaults costs about 79 s before failing, which `--option connect-timeout` and `--option download-attempts` bound.
4. **FR-014 / `--offline`.** nix's `--offline` doesn't stop an explicit `--store https://…` query, so waybill must skip the query itself. A warm narinfo cache can still answer with the network down.
5. **SC-007, "A dependency reached only through test edges is marked".** shellcheck can't exercise this: it has no test-only dependencies, and QuickCheck is a genuine library dependency with code in the binary. ormolu at the same revision can (6 test-only deps).
6. **FR-002, test edges.** At a static Haskell root, test deps share `buildInputs` with lib/exe deps when `doCheck` is set, so the derivation alone can't exclude them. ormolu's R1 precision is 0.387. Excluding them needs `getCabalDeps` from evaluation. The m1269 lifecycle propagation was not examined here.
7. **The Assumptions' hlint description.** The 322 paths are 134 haddock -doc outputs, the C toolchain via ghc (32 paths only via ghc), and 62 test-framework libraries pulled in by darwin `lib/links` symlinks. The linux closure is 220. The "References count as shipping" rule will therefore report test frameworks as runtime on darwin.
8. **Multi-output edge case.** An evaluated attribute's `outPath` names one output (`bin` for curl), while `meta.outputsToInstall` names two (`bin`, `man`). The spec's "outputs the evaluated attribute selects" needs to say which of these it means.

---

## Implementation-time re-checks (T001, T002)

Re-run on 2026-11-10 (EST), same host, same nix (`nix (Determinate Nix 3.20.0) 2.34.6`), so the nix version recorded above still holds.

### T001: `q2-query.sh` re-run

Every case gave the same exit code and the same stdout as the committed `q2-out/`. The only differences were timings and the order of `nix store verify`'s `checking …` lines on stderr, which it prints from parallel workers. The committed `q2-out/` was kept. The plain `nix path-info -r --store <url> <path>` output is still one full store path per line.

### Q9. The installable form does not work; query the output path (T001)

Q2 always passed an **outPath**. The plan proposed passing the **installable** (`<flakeref>#<attr>`) instead. `q9-installable.sh` tried that for shellcheck and curl (`q9-out/`):

| case | shellcheck | curl |
|---|---|---|
| `path-info -r --store <cache> <installable>` | exit 1: evaluation tries to **upload** the `.drv` to the cache (`unable to upload '…narinfo': HTTP error 404`) | exit 1: `'…curl.meta.outputsToInstall' is not a list of strings` |
| the same with `--eval-store auto` | exit 0, **7** paths (bin + man + doc) | exit 1, same error |
| `path-info -r <installable>` (local) | selects bin, man, doc; fails on the absent doc | exit 1, same error |
| `path-info -r --store <cache> <outPath>` | exit 0, 5 paths | exit 0, 20 paths |
| `store verify --no-contents -r --store <cache> <outPath>` | exit 0 | exit 0 |

So the installable form fails three ways:

1. With `--store <binary cache>`, the cache is also the evaluation store, and evaluation writes the `.drv` there.
2. curl's installable does not evaluate at all at this revision, and curl is one of the five targets. `nix derivation show "$R#curl"` fails the same way (`q9-default-output.sh` has to go through `drvPath`).
3. An installable selects `meta.outputsToInstall` (shellcheck: `bin man doc`), not `outPath`. The committed runtime truth (`measurements/rt/*.paths`) was taken from `outPath`.

**Decision:** query the evaluated derivation's **default output path**, never the installable. It needs no second evaluation of the scanned flake, which also means no repository code runs for the query.

**Can the default output be read from the closure?** `outPath` is the first entry of the derivation's `outputs` attribute. `nix derivation show` keys its `outputs` object alphabetically, so the order survives only in that attribute (`env.outputs`, or `structuredAttrs.outputs`). `q9-default-output.sh` compared the head of that attribute with the evaluated `outPath` for shellcheck, curl, hlint and pandoc: **4 of 4 agree** (`q9-out/d-*.default-output.txt`). An attribute that selects a non-default output explicitly (`pkgs.curl.out`) would not be seen this way **(inference; not probed)**.

A smaller finding: with `--eval-store auto`, the client forwards `--option download-attempts` and `--store` to the daemon, which ignores them as restricted settings for an untrusted user and prints a warning. Querying a store path needs no evaluation store, so this does not arise.

### Q8. Signature verification cost (T002, research R11)

`q8-verify-cost.sh`, 5 runs per cell, `--store https://cache.nixos.org --option connect-timeout 2 --option download-attempts 1 <outPath>` (outPath per Q9). `cold` sets both narinfo TTLs to 0. Full table: `q8-summary.txt`.

| attr | paths | query cold, median (min–max) | verify cold, median (min–max) | query warm | verify warm |
|---|---|---|---|---|---|
| shellcheck | 5 | 0.272 s (0.246–0.310) | 0.259 s (0.239–0.469) | 0.052 s | 0.052 s |
| curl | 20 | 0.838 s (0.772–1.006) | 0.616 s (0.596–1.461) | 0.053 s | 0.054 s |
| hlint | 322 | 13.515 s (11.094–26.391) | 7.878 s (7.173–18.060) | 0.062 s | 0.066 s |

- **Per path, cold:** verification costs 24.5 ms (hlint), 30.8 ms (curl) and 51.8 ms (shellcheck, where the fixed cost dominates). That is well under the 0.2 s per path at which T002 says to stop, so verification stays.
- **In practice verification follows the query,** which has just fetched every narinfo, so it runs warm: 0.066 s for hlint's 322 paths. The cold figures are the bound if the narinfo cache is bypassed.
- **Query cold, hlint:** 42 ms per path median, against Q3's 34 ms. The max run (26.4 s) is one outlier of five; this host's network was not controlled.
- Every run exited 0.

### SC-005 against these figures (T022)

Arithmetic over the Q8 and Q5 medians above, not a separate run.

| | per runtime path, median | SC-005 bound |
|---|---|---|
| hlint, query cold | 13.515 / 322 = **42 ms** (max run 82 ms) | ≤ 100 ms |
| hlint, query cold + verify warm (the order waybill runs them) | (13.515 + 0.066) / 322 = **42 ms** | |
| hlint, query cold + verify cold (narinfo cache bypassed for both) | (13.515 + 7.878) / 322 = **66 ms** | |
| curl, query cold + verify warm | (0.838 + 0.054) / 20 = **45 ms** | |
| shellcheck, query cold + verify warm | (0.272 + 0.052) / 5 = **65 ms** | |
| shellcheck, query cold + verify cold | (0.272 + 0.259) / 5 = **106 ms** | over, by fixed cost on a 5-path closure (0.53 s total) |
| unreachable substituter | **2.1 s** per substituter (Q5, `connect-timeout 2`, `download-attempts 1`) | ≤ 5 s |

- **Per-path bound: met** for every closure measured in the order waybill runs the calls (verification follows the query, so its narinfos are warm). Only the 5-path shellcheck closure with both calls cold exceeds 0.1 s per path, and that is 0.53 s in total: a fixed cost, not a per-path one.
- **Unreachable bound: met per substituter.** waybill asks each configured substituter in turn, so N unreachable substituters cost about N × 2.1 s before the fallback takes over: within 5 s for up to two, not for three or more **(inference from Q5's per-call figure; several unreachable substituters were not measured together)**. The verification call is never made against a store whose query failed.
- **Unreachable bound across substituters (T041 and its follow-up).** waybill now keeps a 5 s remote budget. Before each substituter it computes what is left; with under 1 s left it starts no further one (`query-timed-out`), otherwise it passes `connect-timeout min(2, floor(remaining))` (Q5c: whole seconds only). At 2.1 s per unreachable cache that is two calls (4.2 s) and the third skipped, where the first T041 version, which only checked whether the budget was already spent, started the third at 4.2 s and ran to about 6.3 s. The bound is then the budget plus nix's overhead past its connect timeout on the last call (about 0.1 s in Q5) **(inference from Q5's per-call figure; tested with a stub runner and an injected clock, not against unreachable caches)**. The timeout covers the connect phase only, so a reachable cache's slow answer (hlint cold, about 11 s in Q3) is not cut short.

### Q10. x86_64-linux's runtime closure does not hold the static build (2026-11-11)

The Linux corpus regen of `nix-closure-shellcheck` (fixture `kusari-sandbox/waybill-fixture-nix-shellcheck@dc4e4509`, nixpkgs `a799d3e3`) emitted C216 `static-link-source: none` and marked every Haskell library linked into shellcheck excluded. The static-link rule seeds from runtime members built with the Haskell generic builder, and Q6 measured it only on aarch64-darwin. `q10-linux-shellcheck.sh` asks cache.nixos.org for the runtime closure of the fixture flake's `packages.<system>.default` default output on both systems, and lists the Haskell-builder derivations the wrapper root consumes through `inputs.drvs`. It builds nothing; evaluating the x86_64-linux derivation from darwin only instantiates `.drv` files. Run 2026-11-11 04:53 UTC on the host above, same nix. Raw output: `q10-out/`.

| system | default output | runtime paths | `ShellCheck-` builds in it |
|---|---|---|---|
| x86_64-linux | `2lfacwyv…-shellcheck-0.11.0-bin` | 27 | **0** |
| aarch64-darwin | `xpjnkisx…-shellcheck-0.11.0-bin` | 5 | 1 (`0qxyki0k…-ShellCheck-0.11.0`) |

- On Linux the 27 paths are the wrapper's own output and C libraries: glibc, gmp-with-cxx, libffi, numactl, and elfutils with what it pulls in (curl, openssl, krb5, brotli, zstd, xz and others). That is consistent with the GHC runtime linking libdw on Linux **(inference; the binary's `DT_NEEDED` was not read)**. No Haskell-builder member is in it, so nothing seeds the rule.
- On both systems the wrapper root consumes two Haskell-builder derivations through `inputs.drvs`: `ShellCheck-0.11.0`, named in no dependency field (only in `installPhase`), and `pandoc-cli-3.7.0.2`, named in `nativeBuildInputs`. pandoc renders the man page on the build machine and is not in the binary.

**Consequence:** under the runtime closure, static linking is also seeded from the Haskell builds the evaluated root consumed through `inputs.drvs`, when the root is a wrapper rather than a Haskell build itself. A build the root names only in a build-machine field (`pandoc-cli`) is not one of them. Without that exclusion pandoc-cli and every library linked into it would be kept runtime, on both systems, and under the T042 walk guard as well.
