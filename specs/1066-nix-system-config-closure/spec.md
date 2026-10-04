# Feature Specification: Closure SBOMs for Nix system-configuration flakes

**Feature Branch**: `1066-nix-system-config-closure`
**Created**: 2026-10-04
**Status**: Draft
**Input**: Issue #1052, "Nix: a flake exposing only darwinConfigurations/nixosConfigurations degrades with no-evaluable-attribute", plus the research posted on it on 2026-10-04.

## Background (measured, not assumed)

The Nix closure tier (milestone 1035, `--nix-closure`) evaluates a flake and lists everything the chosen output is built from, without building anything. Today it only looks under `packages.<system>`, at `default` or at the name given with `--nix-closure-attr`. A flake that describes a whole machine — a NixOS or nix-darwin system configuration — has no `packages` output, so the tier degrades with `no-evaluable-attribute` and the scan carries no closure.

A system configuration's closure is arguably the most useful Nix inventory there is: it is everything a machine's software is built from. Measured on a real nix-darwin system (`aarch64-darwin`, this repository's research comment on #1052):

| closure | members | enumeration (warm store) |
|---|---:|---:|
| build (derivations, what the tier reads) | 5,325 | 1.24 s |
| runtime (installed store paths, *not* read by the tier) | 941 | 0.06–0.13 s |

The build closure is about 4× milestone 1035's largest measured package closure (moat, 1,275 derivations) and well inside the tier's existing 300 s budget.

## Clarifications

### Session 2026-10-04

- Q: How should waybill pick which system configuration to scan when a flake defines several? → A: `--nix-closure-attr` accepts a full output path (e.g. `darwinConfigurations.laptop.system`). When no path is given and the flake defines exactly one system configuration, it is selected automatically. With several and no path, the tier degrades with a reason listing their names. No new flag; the machine's hostname is never consulted.
- Q: Which closure should a system-configuration scan describe? → A: The build closure, by the same evaluate-only approach as package closures (nothing is built, so it works on any machine and in CI). It is labelled as what it is: a build-closure inventory of the configuration, not a list of what is installed on a running machine.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A single-machine configuration flake gets a closure SBOM with no extra flags (Priority: P1)

An engineer keeps their laptop's nix-darwin configuration (or a server's NixOS configuration) in a flake that defines exactly one system configuration and no packages. They run waybill with `--nix-closure`. Today the tier degrades and the SBOM has no closure. With this feature, waybill finds the one configuration and records everything it is built from.

**Why this priority**: the one-machine flake is the commonest shape of configuration flake, and it is the exact case #1052 was filed from.

**Independent Test**: scan a flake defining one `darwinConfigurations` entry and no `packages`, with `--nix-closure`. The closure components are present, and the document records which configuration they came from.

**Acceptance Scenarios**:

1. **Given** a flake with exactly one system configuration and no `packages.<system>.default`, **When** it is scanned with `--nix-closure`, **Then** the closure of that configuration's system output is recorded, exactly as a package closure would be.
2. **Given** the same scan, **When** the document is read, **Then** the closure summary names the full attribute path that was evaluated (e.g. `darwinConfigurations.laptop.system`).

---

### User Story 2 - Several configurations: the operator names one (Priority: P1)

A flake describes a fleet: several machines under `nixosConfigurations` and/or `darwinConfigurations`. waybill must not guess which one the operator means. Without a path, the tier degrades and says which configurations exist; with `--nix-closure-attr nixosConfigurations.web01.config.system.build.toplevel`, it records that machine's closure.

**Why this priority**: guessing (by hostname, by order) would make the same command produce different SBOMs on different machines, which the pure-evaluation design exists to prevent.

**Independent Test**: a flake with two configurations. Without a path, the scan succeeds without a closure, and the log names both configurations and how to choose one. With a full path, that configuration's closure is recorded.

**Acceptance Scenarios**:

1. **Given** a flake with two system configurations and no path, **When** scanned with `--nix-closure`, **Then** no closure is added, the scan still succeeds, and the degradation message lists both configuration names and the option to choose one.
2. **Given** the same flake and `--nix-closure-attr` set to one configuration's full output path, **When** scanned, **Then** only that configuration's closure is recorded.
3. **Given** a full path that does not exist in the flake, **When** scanned, **Then** the tier degrades with `no-evaluable-attribute`, as for a missing package attribute today.

---

### User Story 3 - Package flakes behave exactly as before (Priority: P1)

A flake with a `packages.<system>.default` (with or without system configurations), and every existing use of `--nix-closure-attr <name>`, produces the same output as before this feature.

**Why this priority**: the package closure is the shipped behaviour; system configurations are added beside it, never in its place.

**Independent Test**: existing closure tests and corpus targets are byte-identical.

**Acceptance Scenarios**:

1. **Given** a flake with `packages.<system>.default` and one system configuration, **When** scanned with `--nix-closure` and no path, **Then** the package closure is recorded, as before; the configuration is not chosen over it.
2. **Given** `--nix-closure-attr pkg-ghc96` (a bare name), **When** scanned, **Then** it still means `packages.<system>.pkg-ghc96`.

### Edge Cases

- **A configuration for another platform** (a Linux NixOS configuration scanned from a Mac): evaluation needs no building and no matching host. The configuration's own platform applies, not the scanning machine's, and the closure is recorded.
- **Both `darwinConfigurations` and `nixosConfigurations` present, one entry in total**: that one entry is auto-selected (the count is across both).
- **One entry in each** (two in total): treated as several; degrade and list both.
- **Home Manager (`homeConfigurations`) and other outputs**: not auto-selected. An operator can still name any evaluable output by full path; whether a closure of it is meaningful is theirs to judge.
- **`--offline`**: unchanged; the tier never starts.
- **A configuration whose evaluation needs import-from-derivation**: refused as for packages today (the refusal itself is #1114's subject).
- **Very large closures**: bounded by the tier's existing wall-clock budget; on expiry the tier degrades as it does today.

## Requirements *(mandatory)*

### Functional Requirements

**Selecting what to evaluate:**

- **FR-001**: `--nix-closure-attr` MUST accept a full flake output path. A value whose first segment is a top-level flake output name (`packages`, `legacyPackages`, `darwinConfigurations`, `nixosConfigurations`, `homeConfigurations`, or any other top-level output the flake defines) is a full path; any other value keeps today's meaning, a name under `packages.<system>`.
- **FR-002**: With no `--nix-closure-attr`, the tier MUST use `packages.<system>.default` when it exists (unchanged). Only when it does not, the tier MUST count the system configurations the flake defines under `darwinConfigurations` and `nixosConfigurations` together:
  - exactly one → evaluate its system output: `darwinConfigurations.<name>.system` or `nixosConfigurations.<name>.config.system.build.toplevel`;
  - none → degrade with `no-evaluable-attribute`, as today;
  - more than one → degrade, naming every configuration and how to choose one with `--nix-closure-attr`.
- **FR-003**: The machine's hostname, user name or any other host property MUST NOT influence which configuration is chosen. The same flake and flags produce the same choice everywhere.
- **FR-004**: A system configuration MUST be evaluated for its own platform, not the scanning machine's, so a configuration for another platform is recorded rather than degraded.

**What is recorded:**

- **FR-005**: A system configuration's closure MUST be recorded by the same means as a package closure: evaluated, never built; members classified and emitted exactly as milestone 1035 does; same per-component and document-level annotations.
- **FR-006**: The closure summary's `attribute` field (C184) MUST hold the full output path that was evaluated whenever a system configuration or other full path was used. For package closures selected by name, it MUST keep today's value (FR-009).
- **FR-007**: Documentation for `--nix-closure` and `--nix-closure-attr`, and the C184 catalogue row, MUST say that a system configuration's closure is what it is *built from*, not what is installed on a running machine.

**Degradation:**

- **FR-008**: The several-configurations degradation MUST use a reason distinct from `no-evaluable-attribute`, because the remedy differs: name one versus fix the flake. Its message MUST list the configuration names in a stable (sorted) order.

**Unchanged:**

- **FR-009**: Every scan that evaluated a package closure before this feature MUST produce byte-identical output, and every scan that degraded for a reason other than "no packages output" MUST degrade identically.
- **FR-010**: No new command-line flag.

### Key Entities

- **Closure target**: what the tier evaluates. Either a package (a name under `packages.<system>`) or a full output path, which a system configuration resolves to when auto-selected.
- **System configuration**: a named entry under `darwinConfigurations` or `nixosConfigurations`, with its own platform and a known system output path.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A flake with exactly one system configuration and no packages produces a closure SBOM with no flags beyond `--nix-closure`, where today it degrades.
- **SC-002**: On a real nix-darwin system flake, the closure is recorded within the tier's existing budget. The measured build closure of 5,325 derivations enumerated in 1.24 s warm; the end-to-end scan is reported as a measurement in the plan, not predicted here.
- **SC-003**: For a flake with several configurations, 100% of runs without a path degrade with a message naming every configuration. 0% pick one.
- **SC-004**: The same flake and flags select the same configuration on two machines with different hostnames (verified by a test that varies the host's reported name, or by construction where no host property is read).
- **SC-005**: All existing closure tests, in-repo goldens and public-corpus goldens are byte-identical (FR-009).

## Assumptions

- **Only the two configuration kinds the issue names are auto-selected.** Home Manager and other outputs are reachable by full path only; auto-selecting them is a separate decision.
- **The system output paths** are the conventional ones (`.system` for nix-darwin, `.config.system.build.toplevel` for NixOS). A configuration exposing its system elsewhere is reachable by full path.
- **Root component**: unchanged. As today, closure components supplement the manifest-derived set; this feature does not create a "machine" root component.
- **Runtime closure is out of scope** (clarification Q2). It requires the system to be realised, which this tier never does.
- **IFD verification** for this tier is tracked separately in #1114. This feature relies on whatever that issue settles and does not change it.
- **Cold-store cost** is not yet measured. The plan measures it on a fresh store before committing any number to a test.
