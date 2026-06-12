# Contract — Walker API surface

**Feature**: 113-exclude-path-flag

## Signature changes

Every helper that today asks "should I skip descending into this child directory?" gains an `&ExclusionSet` parameter and gives up the name-only signature in favor of a candidate-path-plus-rootfs signature.

### Shared helper

```text
// before (project_roots.rs:115)
pub(crate) fn should_skip_default_descent(name: &str) -> bool

// after
pub(crate) fn should_skip_default_descent(
    candidate: &Path,
    rootfs: &Path,
    exclude_set: &ExclusionSet,
) -> bool
```

Inside, the name-based built-in skips are unchanged (they call `candidate.file_name()` internally). The exclusion-set check runs AFTER the built-ins so an empty set returns the pre-feature decision unchanged (FR-003).

### `WalkConfig.should_skip` closure

```text
// before
pub should_skip: &'a dyn Fn(&str) -> bool

// after
pub should_skip: &'a dyn Fn(&Path, &Path) -> bool
```

The walker passes `(candidate, rootfs)` at each descent decision. Each ecosystem closure binds the appropriate `exclude_set` via capture.

### Per-walker helpers (cargo, maven, gem, golang/legacy, go_binary)

Same shape: `(candidate: &Path, rootfs: &Path, exclude_set: &ExclusionSet) -> bool`.

### Reader signatures

```text
// every per-ecosystem read fn — before
pub fn read(rootfs: &Path, include_dev: bool) -> Vec<PackageDbEntry>

// after
pub fn read(rootfs: &Path, include_dev: bool, exclude_set: &ExclusionSet) -> Vec<PackageDbEntry>
```

`scan_path` and `read_all` thread the borrow through.

## Match-time behavior contract

For every candidate directory the walker is about to descend into:

1. Compute `rel = candidate.strip_prefix(rootfs)`. If `strip_prefix` fails (candidate is outside rootfs — shouldn't happen but defensive), return `false` (don't skip).
2. Normalize `rel` to forward-slash form: `rel.to_string_lossy().replace('\\', "/")`.
3. Check `exclude_set.matches(&rel_normalized)`. If `true`, return `true` (skip).
4. Otherwise fall through to the existing built-in skip logic.

## Idempotence + ordering

- The exclusion set is borrowed read-only by every walker; concurrent reads across crate-internal threads are safe.
- Order of entries within `ExclusionSet` does not affect match correctness (union semantics) but DOES affect the deterministic annotation payload ordering — entries are emitted in CLI order followed by env-var order.

## Invariants the implementation MUST preserve

| Invariant | Verified by |
|---|---|
| Empty exclusion set produces byte-identical scan output to pre-feature build | Byte-identity test in `tests/exclude_path_integration.rs` against committed golden |
| Built-in skips (vendor, node_modules, …) remain in place regardless of exclusion set | Unit test asserting `should_skip_default_descent(vendor_dir, _, &empty_set) == true` |
| Pattern entries can never re-enable a built-in skip | Unit test asserting `should_skip_default_descent(vendor_dir, _, &set_with_vendor_negated) == true` — no negation exists in v1, but defensive |
| Malformed entries abort before any descent | Test asserting `ExclusionSet::from_iter([…, "[", …])` errors and the CLI never enters `scan_path` |
| Cross-platform path normalization | Unit test using `\\`-separator literal entry matching against a forward-slash candidate path |
