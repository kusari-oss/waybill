# Contract: what `--nix-closure` evaluates

## `--nix-closure-attr <value>`

| value | meaning |
|---|---|
| absent | auto-selection (below) |
| contains `.` and its first segment is a standard output name | full output path, evaluated as given: `<root>#<value>` |
| anything else | a name under `packages.<system>` (unchanged), admitted only if the flake's `packages.<system>` listing contains it |

Standard output names: `packages`, `legacyPackages`, `checks`, `devShells`, `apps`, `formatter`, `overlays`, `hydraJobs`, `templates`, `lib`, `nixosConfigurations`, `nixosModules`, `darwinConfigurations`, `darwinModules`, `homeConfigurations`, `homeManagerModules`, `defaultPackage`, `defaultApp`, `devShell`.

**Full path safety:** a full path is admitted only if every character is an ASCII letter or digit, `.`, `_` or `-`. Otherwise it degrades as `no-evaluable-attribute`, and the log says it was refused.

## Auto-selection (no `--nix-closure-attr`)

1. `packages.<system>` lists `default` → evaluate `packages.<system>.default`. Unchanged.
2. Otherwise, list `darwinConfigurations` and `nixosConfigurations`. An absent output counts as zero entries. Names failing the safety rule are dropped, with a log line.
3. Count the names across both:

| count | outcome |
|---|---|
| 0 | degrade `no-evaluable-attribute` (unchanged) |
| 1 (darwin `<n>`) | evaluate `darwinConfigurations.<n>.system` |
| 1 (nixos `<n>`) | evaluate `nixosConfigurations.<n>.config.system.build.toplevel` |
| > 1 | degrade `several-system-configurations`; the log lists every `<kind>.<name>`, sorted, and an example `--nix-closure-attr` |

No host property (hostname, user, platform) takes part in the choice.

## What is recorded

- **Closure handling:** same as milestone 1035. Evaluate-only `nix derivation show -r` with import-from-derivation refused, same classification, same components and annotations.
- **C184 `attribute`:**
  - the bare name for package closures selected by name (unchanged; `default` when auto-selected);
  - the full path otherwise (`darwinConfigurations.laptop.system`, …).
- **Degradations stay log-only** (#1115).

## Examples

```
$ waybill sbom scan --path ./my-mac --nix-closure ...
# one darwin configuration "laptop", no packages
C184 attribute = "darwinConfigurations.laptop.system"

$ waybill sbom scan --path ./fleet --nix-closure ...
WARN nix-closure: degrading ... reason="several-system-configurations"
     detail="darwinConfigurations.laptop, nixosConfigurations.web01; choose one with --nix-closure-attr, e.g. --nix-closure-attr nixosConfigurations.web01.config.system.build.toplevel"

$ waybill sbom scan --path ./fleet --nix-closure --nix-closure-attr nixosConfigurations.web01.config.system.build.toplevel ...
C184 attribute = "nixosConfigurations.web01.config.system.build.toplevel"
```
