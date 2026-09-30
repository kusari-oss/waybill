# Contract: how waybill takes a derivation closure

**Feature**: `1035-nix-closure-sbom` (#1034, #1040)

Everything here traces to a probe in
[`../research.md`](../research.md) or milestone 1034's
`measurements/probe-nix-behaviour.sh`.

## 1. Inherit milestone 1034's safety sequence, unchanged

Before any closure query: the import-from-derivation pre-flight
(`nix config show --option allow-import-from-derivation false`, accepting only
on the exact reflected line), then platform resolution. Both already exist and
are not re-specified here.

**PR #1044's argv guard applies and is now load-bearing.** No
`--accept-flake-config`, no `--impure`. Milestone 1034 evaluates nixpkgs at a
pinned revision, so repository-authored expressions never run and the guard is
precautionary. This feature evaluates the project's own flake, so they do —
and a flake can request `allow-import-from-derivation` through its own
`nixConfig`. Measured: slack-web does exactly that.

## 2. Address the flake through the CLI, never `builtins.getFlake`

```
nix eval --json --option allow-import-from-derivation false \
         --apply builtins.attrNames '<path>#packages.<system>'

nix derivation show -r --option allow-import-from-derivation false \
         '<path>#packages.<system>.default'
```

**Why the CLI form is mandatory, not stylistic** (research R4, MEASURED):

```
$ nix eval --expr 'builtins.attrNames (builtins.getFlake "path:/…/moat").packages.…'
error: cannot call 'getFlake' on unlocked flake reference … (use --impure to override)
```

`--impure` is refused by §1's guard. The CLI flakeref form works without it, so
it is the only route that stays pure.

## 3. Attribute selection

1. `packages.<system>.default` (spec FR-015a).
2. An operator-named attribute overrides it.
3. `packages.<system>` present but no `default` → degrade with a reason naming
   the attributes that *are* available, so the operator can choose rather than
   guess (FR-015b).
4. No `packages.<system>` → degrade. Measured case: haskell-language-server
   exposes only `docs` and devShells.

**Never merge attributes.** moat exposes `default`, `moat-ghc910`,
`moat-ghc94`, `moat-ghc96` — the same library against three compilers. Merging
would put three GHC toolchains and three copies of every dependency into one
document, describing a build nobody performed.

## 4. Parsing

`nix derivation show -r` output is `{"derivations": {...}, "version": N}`.
Members are keyed by `.drv` path.

**The join that silently fails**: output paths under `outputs.*.path` are
stored *without* the `/nix/store/` prefix; the `env` fields referencing them
carry it. Compare basenames. Comparing raw strings matches nothing and produces
a classification in which every member is `Unreferenced` — with no error. The
first draft of the classifier did precisely this and reported 1,275 of 1,275
unreferenced, which looked like a finding.

Role assignment reads only nix's own fields:

| Role | Fields |
|---|---|
| artifact input | `buildInputs`, `propagatedBuildInputs`, `depsHostHost` |
| build tooling | `nativeBuildInputs`, `depsBuildBuild`, `depsBuildHost`, `nativeCheckInputs` |

## 5. Budget and offline

One wall-clock budget covering acquisition and query, inheriting milestone
1034's structure and its reasoning: `getFlake` fetches during evaluation and
there is no seam to bound separately.

`--offline` refuses the whole path, for m1034's measured reason — the fetch is
not suppressible, and nix's own `--offline` governs substituters rather than
flake inputs.

Measured cost, warm store: 1.09 s / 5.7 MB (moat), 1.07 s / 7.4 MB
(slack-web). Cold-store cost unmeasured; task T-R3.

## 6. What this contract deliberately does not do

- **Does not build anything.** `derivation show` instantiates; it does not
  realise. The IFD refusal remains the guarantee that evaluation cannot build.
- **Does not merge attributes** (§3).
- **Does not replace the manifest-derived set** — research R1 measured why.
