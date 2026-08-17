# Quickstart: `waybill:unresolved-reason` universalization

**Milestone**: 236

## Verify shipped behavior (after implementation)

### 1. Sanity scan on any design-tier project

```bash
# scan a Cargo project without Cargo.lock
mkdir /tmp/waybill-236-quickstart && cd /tmp/waybill-236-quickstart
cat > Cargo.toml <<EOF
[package]
name = "waybill-fixture-quickstart"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
EOF

waybill sbom scan --path . --format cyclonedx-json --output out.cdx.json --no-deep-hash
jq '.components[] | select(.properties[]?.name == "waybill:sbom-tier" and .properties[].value == "design") | .properties[] | select(.name == "waybill:unresolved-reason")' out.cdx.json
```

**Expected**: JSON output shows the cargo reader's reason string on every design-tier component.

### 2. Verify cross-format parity

```bash
waybill sbom scan --path . --format spdx-json --output out.spdx-2.3.json --no-deep-hash
waybill sbom scan --path . --format spdx-3-json --output out.spdx-3.json --no-deep-hash

# grep across all 3 formats
grep -l "waybill:unresolved-reason" out.cdx.json out.spdx-2.3.json out.spdx-3.json
```

**Expected**: All 3 files match.

### 3. Run the full test suite

```bash
# unit + integration
cargo +stable test --workspace unresolved_reason

# specifically the cross-reader test
cargo +stable test --workspace --test unresolved_reason_universal
```

**Expected**: 18 unit tests (one per reader) + 1 integration test = 19 passing tests.

## Add a new design-tier-emitting reader (recipe)

If a future milestone adds a new reader that emits `waybill:sbom-tier: "design"`, follow this recipe to keep m236 coverage universal:

1. **Locate the design-tier emission call-site** — grep for `sbom_tier.*design` in the new reader's file.

2. **Add the sibling annotation insert**:

   ```rust
   extra_annotations.insert(
       "waybill:sbom-tier".to_string(),
       serde_json::Value::String("design".to_string()),
   );
   // ADD THIS:
   extra_annotations.insert(
       "waybill:unresolved-reason".to_string(),
       serde_json::Value::String(
           "<one-line human-readable reason>".to_string(),
       ),
   );
   ```

3. **Add the reason string** to `contracts/per-reader-strings.md` (or wherever the m236 successor doc lives).

4. **Add a unit test** in the reader's `mod tests` asserting the exact string on a deterministic fixture.

5. **Add the fixture** to `waybill-cli/tests/fixtures/golden_inputs/unresolved_reason/<reader>/` and update the cross-reader integration test's fixture-directory-count assertion if it's a hardcoded count.

6. **Sanity-check** by running the pre-PR gate + `cargo test unresolved_reason`.

## Regression guard

Never modify the NuGet reason string (`no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry`) without an explicit spec change. The byte-identity test in `waybill-cli/tests/unresolved_reason_universal.rs` will fail otherwise, and downstream tools that hard-coded the NuGet string will break.
