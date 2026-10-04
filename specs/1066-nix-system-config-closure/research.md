# Research: Closure SBOMs for Nix system-configuration flakes

Every number is measured. Raw data and probes are in `measurements/`.

## R1 — Telling a full path from a package name (FR-001)

**Measured:** a flake's top-level output names cannot be listed through the `#` installable syntax. `nix eval .#` resolves to the default package, not to the outputs attribute set (`measurements/README.md` §3). Listing them would need `builtins.getFlake`, which in pure mode needs a locked reference, for which a local working tree has none.

**Decision:** a lexical rule, with no extra evaluation. A `--nix-closure-attr` value is a full output path when it contains a `.` and its first segment is a standard flake output name:

> `packages`, `legacyPackages`, `checks`, `devShells`, `apps`, `formatter`, `overlays`, `hydraJobs`, `templates`, `lib`, `nixosConfigurations`, `nixosModules`, `darwinConfigurations`, `darwinModules`, `homeConfigurations`, `homeManagerModules`, `defaultPackage`, `defaultApp`, `devShell`.

Any other value keeps today's meaning, a name under `packages.<system>`.

**Spec impact:** FR-001's "or any other top-level output the flake defines" becomes "the standard output names", because the broader form cannot be decided without an evaluation pure mode does not allow. Recorded in spec FR-001.

**Rationale:**
- Byte-identical for every existing bare name, unless one starts with a standard output name followed by `.`. No such package name appears in any closure test or corpus target.
- A non-standard top-level output is still reachable by setting its first segment as a standard one. Where that is impossible, it is out of scope, and the CHANGELOG says so.

**Alternatives considered:**
- *A new flag for full paths*: rejected by FR-010.
- *Try the bare name under `packages.<system>` first and fall back to raw*: two evaluations, and a silent change of meaning depending on what the flake defines.

## R2 — Counting configurations (FR-002)

**Decision:** only when no `--nix-closure-attr` was given, and the existing `packages.<system>` listing failed or lacks `default`, run:

```
nix eval --json --option allow-import-from-derivation false --apply builtins.attrNames <root>#darwinConfigurations
nix eval --json --option allow-import-from-derivation false --apply builtins.attrNames <root>#nixosConfigurations
```

- **An absent output is zero entries.** Its call fails with `does not provide attribute` (measured). Any other failure is also counted as zero entries, so the outcome is `no-evaluable-attribute`, today's reason for a flake that does not evaluate. That is logged with the stderr head.
- **`attrNames` is lazy**, so no configuration is evaluated by the listing (measured: an instant result on the synthetic flake).
- **Selection:**
  - exactly one name across both → the system path: `darwinConfigurations.<n>.system` or `nixosConfigurations.<n>.config.system.build.toplevel`;
  - zero → `no-evaluable-attribute`;
  - more → the new reason (R5).

**Needed code change:** `ClosureConfig` currently fills in `"default"` when the flag is absent, so "operator asked for `default`" and "nothing was asked" are indistinguishable. The attribute becomes `Option<String>`. An explicit `--nix-closure-attr default` keeps today's exact behaviour, including its failure when `default` is absent (FR-009).

## R3 — Platform (FR-004)

**Measured:** an `x86_64-linux` NixOS toplevel evaluates on an `aarch64-darwin` host: 2,444 derivations, all `system = x86_64-linux`, nothing built.

**Decision:** a full path is evaluated as given, with no `<system>` inserted; the configuration carries its own platform. Host-system detection still runs, because the `packages.<system>` listing needs it first.

## R4 — Cost (SC-002)

**Measured** (`measurements/README.md`):
- minimal NixOS: 2,444 derivations, 11.8 s cold / 0.25 s warm;
- minimal nix-darwin: 2,135 derivations, 2.85 s cold / 0.23 s warm;
- a real darwin system: 5,325 derivations, 1.24 s warm.

**Decision:** keep the existing 300 s budget. The worst measured cold case uses 4% of it.

## R5 — The several-configurations reason (FR-008)

**Decision:**
- A new `DegradationReason::AmbiguousSystemConfiguration(Vec<String>)`, with wire code `several-system-configurations`.
- Its log message lists qualified names sorted (`darwinConfigurations.laptop`, `nixosConfigurations.web01`) and how to choose one: `--nix-closure-attr nixosConfigurations.web01.config.system.build.toplevel`.
- Log-only, like every closure degradation (Q3; #1115).

## R6 — Safety of a full path

**Found:**
- **Bare names are safe today because of the listing.** The tier admits a bare name only if it appears in the flake's own `attrNames` listing.
- **A full path has no such gate.** It is interpolated into `<root>#<path>`, so a `?`, `#` or space could change what `nix` resolves.
- **Configuration names can be anything.** Nix attribute names are arbitrary strings, including quotes and spaces.

**Decision:**
- A full path, and every listed configuration name, must pass the existing `eval::invoke::is_safe_attribute_name` (ASCII alphanumerics, `.`, `_`, `-`).
- An unsafe full path fails as `no-evaluable-attribute`, with a log line saying it was refused.
- Unsafe configuration names are dropped from the count, with a log line. If that leaves one, it is selected.

This mirrors how the declarations pass treats names (`declarations/evaluate.rs:120`).

## R7 — C184 `attribute` value (FR-006)

**Decision:**
- **Package closures selected by name:** unchanged. The value stays the bare name (today's value, e.g. `default`).
- **Full paths and auto-selected configurations:** the full path that was evaluated.

The value tells a consumer which kind of closure they hold, with no new field (no schema change to C184).

## R8 — Tests

- **No mocking seam:** `closure::resolve` calls `nix` directly.
- **Pure unit tests** therefore cover:
  - the path classifier (R1);
  - the selection function (counts → choice or reason), extracted as a pure function over the two listings;
  - the system-path builder;
  - name safety.
- **Integration tests** follow `nix_eval_tier.rs`'s skip-when-`nix`-is-absent pattern, with synthetic input-free fixture flakes. They need no network:
  - `one_darwin/`: one darwin configuration, no packages;
  - `two_configs/`: one darwin and one nixos;
  - `package_and_config/`: `packages.<system>.default` plus a configuration.
- **Corpus:** byte-identical (FR-009). No new corpus target, because a real configuration would need a nixpkgs fetch in every run. The real-configuration measurement lives in `measurements/`.
