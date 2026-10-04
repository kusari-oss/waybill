# Data model: Closure SBOMs for Nix system-configuration flakes

All state is in-process for one scan.

## AttrRequest (parsed from `--nix-closure-attr`)

```
enum AttrRequest {
    Auto,                     // flag absent
    PackageName(String),      // today's meaning: under packages.<system>
    FullPath(String),         // first segment is a standard output name
}
```

`ClosureConfig.attribute` changes from `String` (defaulting to `"default"`) to `AttrRequest`. An explicit `default` is `PackageName("default")`, and keeps today's failure when `default` is absent (FR-009).

## SystemConfiguration

```
struct SystemConfiguration { kind: ConfigKind, name: String }
enum ConfigKind { Darwin, Nixos }
```

- `system_path()`: `darwinConfigurations.<name>.system` or `nixosConfigurations.<name>.config.system.build.toplevel`.
- Ordered by `(kind as output name, name)`, giving the sorted list in the degradation message.

## Selection (pure function)

```
fn select(package_default_present: bool,
          darwin: Option<Vec<String>>,   // None = output absent / listing failed
          nixos:  Option<Vec<String>>) -> Selection
enum Selection { PackageDefault, Configuration(SystemConfiguration), Degrade(DegradationReason) }
```

Names failing `is_safe_attribute_name` are dropped before counting.

## DegradationReason (extended)

`AmbiguousSystemConfiguration(Vec<SystemConfiguration>)`. Its wire code is `several-system-configurations`, and its detail lists qualified names sorted.

## Closure degradation (C190)

`Option<&'static str>`: `Some(reason.wire())` iff `--nix-closure` was requested and no closure was recorded. Carried like C180's `nix_eval_degraded`.

## Selection input change (I1)

`select` takes `packages_output_present: bool` (from `<root>#packages` attrNames succeeding) in place of `package_default_present`. When it is true, selection returns `PackageDefault` and today's path runs, including today's degradation when the host platform lacks `default`.

## ClassifiedClosure.attribute

The value written to C184:
- the bare name for `PackageName` and for auto-selected `default`;
- the full path for `FullPath` and for an auto-selected configuration.
