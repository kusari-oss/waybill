# Measurements for m1066 / #1052 (2026-10-04)

Host: Apple Silicon Mac, `aarch64-darwin`, Determinate Nix 3.20.0 (Nix 2.34.6).
Every number below was observed.

## 1. A real nix-darwin system (this host, already built)

- **Runtime closure:** `nix path-info --json --recursive /run/current-system` gives 941 store paths and 14.51 GB NAR, in 0.13 / 0.06 / 0.06 s.
- **Build closure:** `nix derivation show -r <deriver of /run/current-system>` gives **5,325 derivations** in 1.24 s.

The closure tier reads the build closure. The runtime closure needs the system realised, which the tier never does (spec Q2).

## 2. Minimal real configurations — `probe_config_closure.sh`

Locking the inputs (nixpkgs 25.05 and nix-darwin 25.05) took 12.0 s on the network.

| configuration | derivations | cold | warm |
|---|---:|---:|---:|
| NixOS, `x86_64-linux`, evaluated on this `aarch64-darwin` host | 2,444 | 11.82 s | 0.25 s |
| nix-darwin, `aarch64-darwin` | 2,135 | 2.85 s | 0.23 s |

- **Another platform evaluates fine:** the NixOS configuration targets another platform and evaluates without any builder (FR-004).
- **Nothing built:** `allow-import-from-derivation false` was set on every call, and nothing was built.

## 3. Listing configurations, synthetic flake (no inputs)

- **Configurations are listed:** `nix eval --json --apply builtins.attrNames .#darwinConfigurations` returns `["laptop"]`. An absent output fails with `does not provide attribute ...`.
- **Top-level outputs are not listable:** `nix eval ... .#` does not list them; it resolves to the default package. So top-level output names cannot be enumerated cheaply through the installable syntax (research R1).
- **Shell note:** in zsh, an unquoted `$OPT` holding several words is passed as one argument. The probe spells `--option` arguments out.

## 4. End to end with the built binary (T023)

`waybill sbom scan --nix-closure` on the §2 flake (one NixOS and one nix-darwin configuration), debug build at `c5531aab`, warm store:

```
auto(2 configs) wall=  0.4s  closure: -  degraded: several-system-configurations
nixos path     wall=  4.6s  closure: nixosConfigurations.web01.config.system.build.toplevel drv=2444 emitted=712  degraded: -
darwin path    wall=  1.7s  closure: darwinConfigurations.laptop.system drv=2135 emitted=691  degraded: -
```

Without a path, the two configurations degrade, and C190 records why. Each full path records its configuration's closure in a few seconds (SC-002).
