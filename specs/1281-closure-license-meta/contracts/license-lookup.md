# Contract: the closure license lookup

## Surface

A new method on the existing `Evaluator` trait
(`waybill-cli/src/scan_fs/package_db/nix/eval/mod.rs`), alongside
`evaluate_versions`:

```rust
fn evaluate_licenses(
    &self,
    revision: &str,
    system: &NixSystem,
    names: &[String],
    budget: Duration,
) -> Result<Vec<LicenseRow>, DegradationReason>;
```

```rust
pub(crate) struct LicenseRow {
    /// Store-path basename of the derivation, e.g. `0kky…-autoconf-2.69.drv`.
    /// This is the join key (FR-002), never the name.
    pub(crate) drv: String,
    pub(crate) out: String,
    /// The attribute the row was found under, for diagnostics only.
    pub(crate) attr: String,
    /// Root id the row came from, for diagnostics only.
    pub(crate) root: String,
    /// Licenses exactly as nixpkgs states them, before canonicalisation.
    pub(crate) licenses: Vec<String>,
}
```

`out` is named `out`, not `outPath`: an attrset carrying a key named `outPath`
is string-coerced by `nix eval --json` and the row silently disappears (R3).

## Obligations on the caller

* Filter `names` through `is_safe_attribute_name` and log the refused count.
* Reject a `revision` that fails `is_valid_revision` before interpolation.
* Build argv through `resolving_argv` and assert `argv_is_safe`.
* Never pass `--impure` or `--accept-flake-config`.
* Bound the call with `run_bounded`.

These are not restated as new code; they are the existing
`evaluate_versions` scaffolding, reused (R8).

## Obligations on the expression

The expression evaluates against a pinned revision and a literal system, with
`allow-import-from-derivation` false, and must return rows rather than abort
when individual attributes misbehave. Per R2 the guards are, in order:

1. `builtins.hasAttr name root.set` — a missing attribute escapes `tryEval`.
2. `builtins.isAttrs p && p ? drvPath` — a `null` alias in `pkgs` raises a type
   error, which also escapes `tryEval`.
3. `builtins.tryEval (builtins.deepSeq v v)` — `deepSeq` **inside** the
   `tryEval`, so a throw in a lazy field surfaces while the guard still holds
   rather than during serialisation.

A row that fails any guard is omitted. No guard failure may abort the run.

## Roots

`pkgs` and the default Haskell package set at minimum (R6). The root list is an
input, not a constant: the measured target builds from
`pkgs.haskell.packages.ghc910`, and while that added no joins at this revision,
that is a property of the target rather than a general one.

## Join

By exact store-path basename against the closure's derivation keys. A name
join is forbidden: `autoconf269` carries `pname = autoconf`, so a name join
attaches 2.73's license to 2.69 (R5).

Where a row is found for a store path, its licenses canonicalise through
`SpdxExpression::try_canonical`. A value nixpkgs states in its own vocabulary
that is not an SPDX identifier is preserved as stated and marked, never
rewritten into a plausible-looking SPDX id (spec US2).

## Degradation

Every failure path returns `DegradationReason` and leaves the document
emittable (FR-006): the scan still emits its components, without licenses from
this source, and records why.
