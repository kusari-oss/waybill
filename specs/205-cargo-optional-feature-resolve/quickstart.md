# Quickstart: Cargo Optional-Dep Feature-Resolution Fix

**Date**: 2026-07-17
**Audience**: mikebom maintainer implementing or reviewing m205.

## Prerequisites

- Working mikebom checkout on branch `205-cargo-optional-feature-resolve`.
- `cargo +stable` toolchain (workspace default).
- Reproducer 1 requires the external `test-vaultwarden` repo (or any Cargo project with default-feature-activated optional deps).
- Reproducer 4 requires that you can invoke mikebom with a scrubbed PATH (macOS + Linux OK; Windows skips).

## Reproducer 1 — Reporter's `test-vaultwarden` case (SC-001)

```bash
# Post-fix, reqsign-aws-v4 must appear scope: runtime, not scope: excluded.
git clone --depth 1 https://github.com/kusari-sandbox/test-vaultwarden /tmp/test-vaultwarden
mikebom --offline sbom scan --no-deps-dev --path /tmp/test-vaultwarden \
  --format cyclonedx-json --output /tmp/tvw.cdx.json --no-deep-hash

jq '.components[] | select(.purl | test("reqsign-aws-v4")) | {purl, scope, props: (.properties // [] | map(select(.name | startswith("mikebom:optional") or startswith("mikebom:lifecycle"))))}' \
  /tmp/tvw.cdx.json
```

**Expected pre-m205 (buggy)**: `scope: "excluded"`, `mikebom:optional-derivation = "cargo-optional-true"`, `mikebom:lifecycle-scope = "optional"`.
**Expected post-m205 (fixed)**: `scope: "runtime"` (or absent, defaulting to runtime); NO `mikebom:optional-derivation` property.

Also confirm the previously-orphaned subtree reappears:

```bash
jq '.components[] | select(.purl | test("quick-xml@0.40")) | {purl, scope}' /tmp/tvw.cdx.json
```

**Expected post-m205**: `quick-xml@0.40.1` appears with `scope: "runtime"`. Downstream vuln-scanners will now report RUSTSEC-2026-0194/0195.

## Reproducer 2 — Synthetic default-feature-activated optional dep (SC-002)

```bash
mkdir -p /tmp/m205-us1/src && cat > /tmp/m205-us1/Cargo.toml <<'EOF'
[package]
name = "m205-us1"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", optional = true }

[features]
default = ["serde"]
EOF
echo 'fn main() {}' > /tmp/m205-us1/src/main.rs
(cd /tmp/m205-us1 && cargo generate-lockfile)

mikebom --offline sbom scan --path /tmp/m205-us1 \
  --format cyclonedx-json --output /tmp/m205-us1.cdx.json --no-deep-hash

jq '.components[] | select(.purl | test("serde@")) | {purl, scope}' \
  /tmp/m205-us1.cdx.json
```

**Expected post-m205**: `serde@1.x.y` component has `scope: "runtime"` (or absent). NOT `scope: "excluded"`.

## Reproducer 3 — Truly-optional dep stays Optional (SC-003)

```bash
mkdir -p /tmp/m205-us2/src && cat > /tmp/m205-us2/Cargo.toml <<'EOF'
[package]
name = "m205-us2"
version = "0.1.0"
edition = "2021"

[dependencies]
regex = { version = "1", optional = true }

[features]
enable-regex = ["regex"]
EOF
echo 'fn main() {}' > /tmp/m205-us2/src/main.rs
(cd /tmp/m205-us2 && cargo generate-lockfile)

mikebom --offline sbom scan --path /tmp/m205-us2 \
  --format cyclonedx-json --output /tmp/m205-us2.cdx.json --no-deep-hash

jq '.components[] | select(.purl | test("regex@")) | {purl, scope, props: (.properties // [] | map(select(.name == "mikebom:optional-derivation")))}' \
  /tmp/m205-us2.cdx.json
```

**Expected post-m205**: `regex@1.x.y` has `scope: "excluded"` AND `mikebom:optional-derivation = "cargo-optional-true"` — m179 signal preserved for TRULY-optional deps.

## Reproducer 4 — FR-004 fallback + WARN when cargo absent

```bash
# Scrub PATH so `cargo` is not resolvable, then rescan Reproducer 2's workspace.
PATH="" mikebom --offline sbom scan --path /tmp/m205-us1 \
  --format cyclonedx-json --output /tmp/m205-fallback.cdx.json --no-deep-hash 2>&1 | \
  grep -E "cargo metadata|falling back"
```

**Expected post-m205**: WARN line mentions `cargo metadata failed` AND `falling back`. Scan exits 0.

```bash
# Verify safe over-inclusion: serde is now Runtime (not Optional).
jq '.components[] | select(.purl | test("serde@")) | {purl, scope, props: (.properties // [] | map(select(.name | startswith("mikebom:optional") or startswith("mikebom:lifecycle"))))}' \
  /tmp/m205-fallback.cdx.json
```

**Expected post-m205**: `serde@1.x.y` has `scope: "runtime"` (or absent). NO `mikebom:optional-derivation` property. NO `mikebom:lifecycle-scope: optional` annotation.

Safe over-inclusion means: even without cargo resolving activation, optional deps default to Runtime so vuln-scanners don't silently miss them.

## Reproducer 5 — Non-Cargo scan byte-identity (SC-004)

```bash
# Scan any non-Cargo public_corpus fixture pre + post-fix; assert byte-identical.
git worktree add /tmp/m205-baseline main
cd /tmp/m205-baseline && cargo build -p mikebom --release 2>&1 | tail -3
cd -

BASELINE=/tmp/m205-baseline/target/release/mikebom
POSTFIX=$(cargo build -p mikebom --release 2>&1 && echo target/release/mikebom)

FIXTURE=mikebom-cli/tests/fixtures/public_corpus/npm-express
$BASELINE --offline sbom scan --path $FIXTURE --format cyclonedx-json --output /tmp/baseline.cdx.json --no-deep-hash
$POSTFIX  --offline sbom scan --path $FIXTURE --format cyclonedx-json --output /tmp/postfix.cdx.json  --no-deep-hash
diff /tmp/baseline.cdx.json /tmp/postfix.cdx.json && echo "byte-identical (FR-005 verified)"
```

**Expected post-m205**: `byte-identical (FR-005 verified)`. If diff shows drift on a non-Cargo fixture, the fix has leaked into an unrelated code path.

## Pre-PR gate

```bash
./scripts/pre-pr.sh
```

Both `cargo +stable clippy --workspace --all-targets` (zero errors, zero warnings) AND `cargo +stable test --workspace` (every suite `ok. N passed; 0 failed`) MUST pass green.

## Empirical re-verification at implement time (m199-m204 lesson)

Per `feedback_verify_research_empirical_claims` memory: before finalizing tasks.md, re-run:

```bash
grep -n "optional_names.contains" mikebom-cli/src/scan_fs/package_db/cargo.rs
grep -n "collect_optional_dep_keys\|fn parse_lockfile" mikebom-cli/src/scan_fs/package_db/cargo.rs
grep -c "parse_lockfile(" mikebom-cli/src/scan_fs/package_db/cargo.rs
```

**Expected**: line 1155 confirmed as the sole classifier site; line 1057 confirmed as `parse_lockfile` def; ~1 production call at line 1259 + N test call sites (tally them so the T007 signature-update task is precise).
