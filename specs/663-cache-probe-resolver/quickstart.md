# Quickstart: cache-probe resolver

**Milestone**: 663

## Verify shipped behavior

### 1. Air-gapped scan with pre-warmed Maven + Go caches

```bash
# Assumes an in-toto attestation.json exists naming Maven + Go paths.
# Assumes ~/.m2/repository and $GOMODCACHE are warm.

waybill trace verify --attestation attestation.json --offline

# Emitted components should carry:
#   - "confidence": 0.92
#   - properties[]: {"name": "waybill:resolver-tier", "value": "local_cache_hit"}
#   - PURLs derived from cache paths (pkg:maven/g/a@v, pkg:golang/host/user/pkg@vX.Y.Z)
```

### 2. Verify deps.dev was skipped for cache-hit paths

```bash
# Run with debug logs enabled
RUST_LOG=waybill=debug waybill trace verify --attestation attestation.json 2>&1 | grep "deps_dev_hash"

# Expected: zero "deps_dev_hash resolved" lines for cache-hit paths.
```

### 3. Env-var override

```bash
export GOMODCACHE=/opt/go-cache/pkg/mod
waybill trace verify --attestation attestation.json --offline
# Verifier honors the non-default GOMODCACHE.
```

### 4. Run the test suite

```bash
cargo +stable test --workspace cache_probe

# Expected: 6+ per-ecosystem unit tests + 1 cross-ecosystem integration test + 1 parity test = 8+ green.
```

## Add a new ecosystem probe

1. **Locate the ecosystem's cache root** — grep the ecosystem's docs for the standard cache location and env-var override.

2. **Add a new `EcosystemProbe` variant** in `waybill-cli/src/resolve/resolvers/cache_probe/mod.rs`:

   ```rust
   pub(super) enum EcosystemProbe {
       // ... existing ...
       MyNewEcosystem,
   }
   ```

3. **Implement `try_match_myeco(path: &Path) -> Option<Purl>`** in a new sibling file `waybill-cli/src/resolve/resolvers/cache_probe/myeco.rs`:

   ```rust
   pub(super) fn try_match_myeco(path: &Path) -> Option<Purl> {
       // 1. Compute cache roots via env vars + dirs::home_dir()
       // 2. Prefix match
       // 3. Extract name + version
       // 4. Construct Purl or decline (per Q1 clarification)
       // 5. Log tracing::warn! on decline reason
   }
   ```

4. **Add to the dispatch** in `EcosystemProbe::try_match()`.

5. **Add to the `probes` Vec** in `CacheProbeResolver::new()`.

6. **Add a per-ecosystem unit test** using `tempfile::tempdir()` scaffolding. Fixture uses synthetic package name per project convention.

7. **Add an entry to `contracts/per-ecosystem-cache-shapes.md`** (or the m663 successor doc) locking the path shape.

8. **Sanity-check** — run the pre-PR gate + `cargo test cache_probe`.

## Regression guard

Never modify the `probes` iteration order or the per-probe path-extraction regex without a spec change. Existing per-ecosystem tests + the cross-ecosystem integration test at `waybill-cli/tests/cache_probe_universal.rs` will fail otherwise.
