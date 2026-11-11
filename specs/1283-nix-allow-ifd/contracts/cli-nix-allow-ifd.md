# Contract: `--nix-allow-ifd`

```
--nix-allow-ifd
    Allow import-from-derivation during Nix evaluation. Nix will BUILD
    derivations while evaluating the flake (e.g. callCabal2nix). Do not use
    on an untrusted flake. Where nix's sandbox is off (the macOS default)
    those builds run unsandboxed.
```

- **Requires** `--nix-closure` or `--nix-eval` (clap `ArgGroup`). Without either, clap rejects the invocation with its standard "requires" error.
- **Under `--offline`:** accepted, but no tier runs. C190 records `offline-requested`, and an INFO line notes that `--nix-allow-ifd` had no effect.
- **Effect:** every Nix evaluation passes `--option allow-import-from-derivation true`, verified (C217 `verified`). Nothing else changes: `--accept-flake-config` and `--impure` stay refused.

## Logs

| when | level | text (substance) |
|---|---|---|
| tier degrades with `import-from-derivation-required` | WARN | the #1446 line, plus "Pass --nix-allow-ifd to allow it, only for a flake you trust." |
| `--nix-allow-ifd` and nix reports `sandbox = false` | WARN | "--nix-allow-ifd: nix's sandbox is off; derivations built during evaluation run unsandboxed with your user's access." |
| `--nix-allow-ifd` and allowance unverified | WARN | the degradation line with reason `ifd-allowance-unverified` |
| IFD build failed | WARN | the degradation line with reason `import-from-derivation-build-failed` and nix's salient stderr |
