# Contract: CLI flag surface

**Feature**: `1035-nix-closure-sbom` · **Command**: `waybill sbom scan`
(`ScanArgs` in `waybill-cli/src/cli/scan_cmd.rs`)

| Flag | Type | Default | Meaning |
|---|---|---|---|
| `--nix-closure` | boolean | **off** | Emit the derivation closure of the selected attribute. **Executes the project's own flake.** |
| `--nix-closure-attr <ATTR>` | string | `default` | Attribute under `packages.<system>`. Requires `--nix-closure`. |

Shape follows `--nix-eval` (m1034), `--gradle-resolve` (m235) and
`--helm-render` (m203): one boolean, companions rejected without it.

## Independent of `--nix-eval`

Deliberately a separate flag, not an extension. An operator may reasonably want
evaluated versions without a 1,500-derivation closure; the inverse is less
common but not incoherent. They compose, and each degrades on its own.

## Help text obligations

`--nix-closure`'s help MUST state that it evaluates **the scanned project's own
flake** — a stronger statement than `--nix-eval`'s, which evaluates nixpkgs
only. It MUST carry the same sandbox-or-trusted-flake guidance.

This is a correction, not an addition: `docs/reference/nix-evaluation.md`
currently tells operators the project's own flake is *not* evaluated. True of
m1034, false on this path (spec FR-017).

## Precedence

1. Flag off → no closure query; output unchanged.
2. On, succeeds → closure components supplement the manifest set, each carrying
   a role marker; patches become pedigree; backports produce two VEX statements.
3. On, degrades → manifest set intact, reason recorded, exit success.

## Validation

- `--nix-closure-attr` without `--nix-closure`: argument error.
- Empty `--nix-closure-attr`: argument error.
- `--offline` with `--nix-closure`: **not** an argument error — degrades with
  `offline-requested`, matching `--nix-eval`. The operator's two requests are
  each valid; they are incompatible in effect, and the safer one wins.
