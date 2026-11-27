# Research: Nix-built Haskell packages as project members

All figures come from `measurements/rejected.sh`, run against the 12 Haskell corpus targets at their corpus pins, with a release build of `main` at `cb9d8d82`. Its output is `measurements/rejected.tsv`, `measurements/rejected/<target>.txt` and `measurements/rejected/<target>.nix-refs.txt`.

## R1 — Which directories does #1032 reject, and which of them does Nix name? (FR-001)

**Method.** The probe never models the reader. It scans each target twice: once as committed, and once with every `cabal.project` deleted. With no `cabal.project` left, the rule has nothing to filter against. A manifest directory listed in the second scan's `waybill:source-files` but not the first's is one the rule rejected. For each rejected directory, the probe greps every `*.nix` file in the repository for its relative path. It also checked whether any rejected directory holds a `.nix` file of its own, which would catch a self-reference such as `./.`. None does.

| Target | Manifest dirs (ungoverned) | Rejected | Rejected and named in Nix |
|---|---|---|---|
| haskell-aeson | 6 | 0 | 0 |
| haskell-language-server | 50 | 44 | 0 |
| haskell-security-advisories | 6 | 0 | 0 |
| hpack-hs-opentelemetry | 37 | 0 | 0 |
| hpack-hs-temporal-sdk | 7 | 1 | 1 |
| hpack-lootbox | 7 | 0 | 0 |
| hpack-ogmios | 8 | 1 | 0 |
| nix-closure-moat | 2 | 0 | 0 |
| nix-closure-openai | 1 | 0 | 0 |
| nix-closure-shellcheck | 0 | 0 | 0 |
| nix-closure-slack-web | 1 | 0 | 0 |
| nix-closure-slacklinker | 1 | 0 | 0 |

**Counts by class, over the 46 rejected directories:**

| Class | Count | Directories |
|---|---|---|
| `callCabal2nix` on a literal path | 1 | temporal `tools/tix-to-markdown` |
| `callCabal2nixWithOptions` | 0 | — |
| `callPackage` on a package directory | 0 | — |
| cabal2nix output | 0 | — |
| haskell.nix `project` / `cabalProject` | 0 | — |
| interpolated or computed path | 0 | — |
| no Nix reference | 45 | HLS 44 (test data and a CI helper, #1032's original case); ogmios `server/modules/json-via-show` (removed deliberately in spec 1662) |

`tools/tix-to-markdown` is named by two `callCabal2nix` calls, both literal and both relative to their `.nix` file:

- `nix/packages/tix-to-markdown.nix:1`: `haskellPackages.callCabal2nix "tix-to-markdown" ../../tools/tix-to-markdown { }`
- `nix/overlays/haskell/hs-temporal-sdk.nix:44`: `hfinal.callCabal2nix "tix-to-markdown" ../../../tools/tix-to-markdown { }`

The issue's other two files, `coverage.nix` and `utils/haskell.nix`, refer to the attribute `tix-to-markdown`, not to the path. They are not build references.

**Corpus-wide context: every build-form call site, not only those on rejected directories.**

| Form | Sites | Literal path | Non-literal |
|---|---|---|---|
| `callCabal2nix` | 17 | 14 (including slack-web's `./.`) | 3: hs-opentelemetry `haskell-packages.nix:77` (a variable from `mapAttrs`), `:154` (`fetchTarball`); temporal `deps.nix:14` (`"${hs-opentelemetry-src}/${subdir}"`) |
| `callCabal2nixWithOptions` | 0 | — | — |
| `callPackage` | 11 | all on `.nix` files, none on a package directory | — |
| `packageSourceOverrides` | 5 (one an `inherit`, one commented out) | a name → path map | — |
| `callHackage*` | 1 | registry, not a path | — |

The 14 literal `callCabal2nix` sites other than tix-to-markdown's all name directories that `cabal.project` already admits, or a target with no governing `cabal.project`. Recognising them changes no membership. That is the FR-005 invariant, and the corpus measurement checks it after implementation.

## R2 — Where the answer belongs (FR-002)

**Decision.** (a) Static recognition of `callCabal2nix <name> <literal path>`, at any attribute prefix (`haskellPackages.`, `hfinal.`, `hprev.`, `final.`, `pkgs.haskellPackages.`), with the path relative to the `.nix` file that contains it. The `--nix-closure` tier (b) is **not** added in this spec.

**Rationale.**
- The clarification requires the default scan to restore the package, and only (a) runs without `--nix-closure`.
- (a) reaches 1 of 1 rejected-and-Nix-named directories in the corpus, so nothing is left over for (b) to catch.
- (b) would require `nix` and network access, and would only help scans already using `--nix-closure`. In the corpus those are the five `nix-closure-*` targets, and R1 measured 0 rejected directories in each. Its measured benefit on the corpus is 0 directories.

**Alternatives rejected.**
- **Also recognising `callCabal2nixWithOptions`, haskell.nix, and `packageSourceOverrides`.** FR-003 limits recognition to the forms observed. `packageSourceOverrides` is observed, but never on a rejected directory. The 1 rejected directory gives no evidence for it, and its attribute-set form is a separate grammar. If a corpus target ever needs it, that is an issue to file, not something to guess now.
- **Flake-root-relative paths.** No `callCabal2nix` site in the corpus is written relative to the flake root rather than to its own file. Nix resolves a path literal against the file containing it, and that is the only resolution implemented. The spec's "or the flake root" covers the same thing, because a literal in `flake.nix` resolves against the flake root.

## R3 — Recognising a literal path

A path literal in Nix begins with `./`, `../` or `/`, contains no whitespace, and contains no `${`. A quoted string is not a path literal: `"${src}/x"` is the interpolated case and is never recognised. An absolute path (`/…`) is not recognised either, because it names a location on the host, not one in the repository. The recognised path is the token immediately after the quoted package name. It ends at whitespace, `)`, `;` or `"`. It does not end at `{` or `}`, so `./${name}` stays whole and the `${` check rejects it. An earlier draft also ended at `{`, which cut that path down to `./$` and accepted it; the exclusion test caught this (`measurements/mutations.md`). Tokens may be separated by any whitespace, including newlines. haskell-security-advisories `flake.nix:50` splits the call over three lines.

Comments are removed before matching: `#` to end of line, and `/* … */`. A `callCabal2nix` that appears only inside a comment is therefore not recognised (FR-006).

The resolved directory, `parent(nix file).join(path)` normalised, must lie inside the scan root and must hold a `.cabal` file or `package.yaml` the walk already found. Otherwise the reference admits nothing. It is never a reason to read files outside the root.

## R4 — Recording the admission (FR-004)

- `waybill:source-files` (C18) gains the admitting `.nix` file or files, sorted, alongside the manifest. This uses the existing row with no shape change.
- **How it was admitted.** The new per-component row C248, `waybill:membership-source`, is emitted **only** on a component a Nix build reference admitted. Its value is the closed string `nix-callcabal2nix`. Members admitted by `cabal.project` or `stack.yaml` do not carry it, so every existing document stays byte-identical.
- **Principle V audit.** There is no native carrier.
  - CDX `evidence.identity` describes how the identity was established, not why the component is in scope.
  - SPDX `software_primaryPurpose` and the relationship types do not express "in the project because a build file names it".
  - C18 alone records the `.nix` file but not that it was the reason for membership, because the manifest is listed too.
  
  The row ships with its parity extractors in the same change (C200's precedent).

## R5 — Precedence (FR-005)

Membership is the union of three sources, evaluated per governing `cabal.project`:

1. its `packages:` entries;
2. a sibling `stack.yaml`'s `packages:`;
3. Nix build references whose resolved directory lies under that `cabal.project`'s directory.

A directory admitted by (1) or (2) is never marked as admitted by (3), even when Nix also names it, so C248 appears only where Nix made the difference. Spec 1652's root-only resolution (`resolve_root_declared_members`) takes the same union, so `--project-discovery=root-only` and the default scan agree on what a member is.

## R6 — Cost

The tree is searched for `*.nix` only when some governing `cabal.project` rejected at least one manifest; only then are the files listed and read. The shared walker does not collect them: doing so would register the Haskell reader as claiming `flake.nix` in the m924 repo report. On a repository whose rule rejects nothing, no `.nix` file is opened. That is 10 of the 12 corpus targets.

## R7 — Branch measurement (FR-009, T014)

This is the same probe run with a release build of the branch: `measurements/rejected.branch.tsv`, `membership.branch.txt` and `purl-diff.txt`. `measurements/rejected/` keeps the `main` listing.

- `rejected.tsv`: the only row that changes is `hpack-hs-temporal-sdk`, where rejected goes 1 → 0 and Nix-named goes 1 → 0. The other 11 rows are identical. In HLS, 44 directories are still rejected and none is admitted.
- C248 appears once in the corpus, on `pkg:hackage/tix-to-markdown@2025.10.1.0`. Its `waybill:source-files` lists `nix/overlays/haskell/hs-temporal-sdk.nix`, `nix/packages/tix-to-markdown.nix` and `tools/tix-to-markdown/package.yaml`.
- Component PURLs, `main` → branch: only hs-temporal-sdk changes, and it gains three components:
  - `pkg:hackage/tix-to-markdown@2025.10.1.0`;
  - `pkg:hackage/hpc`;
  - `pkg:hackage/filepath`.
  
  The root reaches `tix-to-markdown`, and `tix-to-markdown` depends on `base`, `containers`, `filepath` and `hpc`. `filepath` and `hpc` are new because `tix-to-markdown` is their only dependent. #1731 named `hpc`; `filepath` has the same cause.
- The other 14 literal `callCabal2nix` sites from R1 changed nothing, as predicted: each names a directory that is already a member.

**Goldens** (T018). They come from `public-corpus.yml` run 46036180952 with `regen_goldens=true` on `1664-nix-built-members`. Of 88 regenerated files, the 3 `hpack-hs-temporal-sdk` goldens differ and the other 85 are byte-identical.
- In CycloneDX, the component PURLs gain exactly `pkg:hackage/filepath`, `pkg:hackage/hpc` and `pkg:hackage/tix-to-markdown@2025.10.1.0`, and C248 appears once.
- C200 now reads 7 of 7 `package.yaml`, including `tix-to-markdown`'s `!include` of `../../hpack-defaults.yaml`.
- The SPDX 3 output again carried the unreferenced `Organization "Microsoft"` element from #1738. It was removed from the committed golden, as in #1737, with no other byte changed. The file round-trips through `json.dumps(indent=2)`.
- No `invariant-declarations.toml` row moved, and the run passed.
