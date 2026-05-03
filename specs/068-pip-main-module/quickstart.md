# Quickstart: Verify pip main-module emission

Three recipes covering single-project PEP 621, name normalization, and Poetry-only skip.

## Prerequisites

```sh
cargo +stable build -p mikebom
```

## Recipe A — Single-project PEP 621

```sh
mkdir -p /tmp/pip-068
cat > /tmp/pip-068/pyproject.toml <<'EOF'
[project]
name = "my_pkg"
version = "1.0.0"
EOF

target/debug/mikebom sbom scan \
  --path /tmp/pip-068 \
  --format cyclonedx-json \
  --output /tmp/pip-068.cdx.json \
  --no-deep-hash

jq '.metadata.component | {bom_ref: ."bom-ref", type, name, version, purl}' /tmp/pip-068.cdx.json
```

**Expect**:
```json
{
  "bom_ref": "pkg:pypi/my-pkg@1.0.0",
  "type": "application",
  "name": "my_pkg",          // verbatim manifest value
  "version": "1.0.0",
  "purl": "pkg:pypi/my-pkg@1.0.0"   // PEP 503-normalized (underscore → hyphen)
}
```

## Recipe B — Name normalization edge case

```sh
mkdir -p /tmp/pip-norm
cat > /tmp/pip-norm/pyproject.toml <<'EOF'
[project]
name = "Some_Package.Name"
version = "0.5.0"
EOF

target/debug/mikebom sbom scan --path /tmp/pip-norm --format cyclonedx-json --output /tmp/pip-norm.cdx.json --no-deep-hash
jq '.metadata.component.purl' /tmp/pip-norm.cdx.json
```

**Expect**: `"pkg:pypi/some-package-name@0.5.0"` (PEP 503: lowercase + collapse `_` and `.` → `-`).

## Recipe C — Poetry-only skip (FR-002)

```sh
mkdir -p /tmp/pip-poetry
cat > /tmp/pip-poetry/pyproject.toml <<'EOF'
[tool.poetry]
name = "poetry-only-app"
version = "1.0.0"
EOF

target/debug/mikebom sbom scan --path /tmp/pip-poetry --format spdx-2.3-json --output /tmp/pip-poetry.spdx.json --no-deep-hash 2>&1 | grep -i "poetry"
```

**Expect**: a `tracing::info!` mentioning the Poetry-only skip. The SBOM should NOT contain a `pkg:pypi/poetry-only-app` package — `documentDescribes` should fall through to the synthetic placeholder.

```sh
jq '[.packages[] | select(.primaryPackagePurpose == "APPLICATION")] | length' /tmp/pip-poetry.spdx.json
# → 0 (no main-module emitted)
```

## When to run

- **Recipe A** during US1 / SC-001 verification
- **Recipe B** during SC-002 (PEP 503 normalization correctness)
- **Recipe C** during FR-002 verification (Poetry-only skip)

All three recipes should also be exercised as integration tests in `tests/scan_pip.rs`.
