# Migrating from mikebom (≤ v0.1.0-alpha.65) to Waybill (v0.1.0-alpha.66+)

> **BREAKING CHANGE**: The project formerly known as `mikebom` was renamed to `Waybill` in `v0.1.0-alpha.66`. This is a mechanical text substitution — no functionality changes; consumers migrate by updating identifiers.

## Who this affects

- **Operators** running `mikebom` in CI scripts, Makefiles, docker-compose files, or shell profiles.
- **Downstream tooling** (security scanners, VEX processors, compliance dashboards) parsing `mikebom:*` annotation keys from SBOMs.
- **Container-image consumers** pulling `ghcr.io/kusari-oss/mikebom:v*-alpha.*`.
- **Rust developers** vendoring or embedding the `mikebom-cli` / `mikebom-common` crates.

**Not affected**: pre-rename release tags (`v0.1.0-alpha.7` … `v0.1.0-alpha.65`) remain accessible — installations pinned to those versions continue to work. Pre-rename Docker image tags at `ghcr.io/kusari-oss/mikebom:*` remain on GHCR as historical artifacts.

## What renamed

| Category                     | Pre-rename                                                       | Post-rename                                                       |
|------------------------------|------------------------------------------------------------------|-------------------------------------------------------------------|
| Primary binary               | `mikebom`                                                        | `waybill`                                                         |
| Workspace crates             | `mikebom-cli`, `mikebom-common`, `mikebom-ebpf`                  | `waybill-cli`, `waybill-common`, `waybill-ebpf`                   |
| Rust module paths            | `mikebom_common::…`, `mikebom_cli::…`, `mikebom_ebpf::…`         | `waybill_common::…`, `waybill_cli::…`, `waybill_ebpf::…`          |
| Environment variables        | `MIKEBOM_*` (73 variables — see full table below)                | `WAYBILL_*`                                                       |
| SBOM annotation keys         | `mikebom:*` (192 distinct keys)                                  | `waybill:*` (mechanical prefix swap; suffixes unchanged)          |
| SBOM tool-metadata name      | `mikebom` (in CDX `metadata.tools[]`, SPDX `creators[]`)         | `waybill`                                                         |
| Docker image                 | `ghcr.io/kusari-oss/mikebom:v*-alpha.*`                          | `ghcr.io/kusari-oss/waybill:v*-alpha.*`                           |
| Release-artifact filename    | `mikebom-v${version}-${target}.{tar.gz,zip}`                     | `waybill-v${version}-${target}.{tar.gz,zip}`                      |

## Migration recipes

### 1. Rename the binary in CI + scripts

```bash
# Before:
mikebom sbom scan --path .
mikebom trace capture -- cargo build
mikebom sbom parity-check

# After:
waybill sbom scan --path .
waybill trace capture -- cargo build
waybill sbom parity-check
```

Subcommand structure, arguments, exit codes, and output formats are **unchanged**. Only the binary name flips.

Batch-update any script that invokes `mikebom`:

```bash
find . -type f \( -name "*.sh" -o -name "Makefile" -o -name "*.yml" -o -name "*.yaml" \) \
  -not -path "./.git/*" \
  -exec sed -i.bak 's/\bmikebom\b/waybill/g' {} \;
```

### 2. Rename environment variables

Any environment variable starting with `MIKEBOM_` renames to `WAYBILL_` with the suffix preserved verbatim. 73 variables surveyed at rename time (full list at [`specs/214-rename-to-waybill/contracts/env-var-migration.md`](../../specs/214-rename-to-waybill/contracts/env-var-migration.md)):

```bash
# Before:
export MIKEBOM_LOG=debug
export MIKEBOM_HELM_RENDER_TIMEOUT_SECS=120
export MIKEBOM_FIXTURES_DIR=/opt/fixtures

# After:
export WAYBILL_LOG=debug
export WAYBILL_HELM_RENDER_TIMEOUT_SECS=120
export WAYBILL_FIXTURES_DIR=/opt/fixtures
```

Batch-update:

```bash
sed -i.bak 's/\bMIKEBOM_/WAYBILL_/g' <your-script-files>

# Verify:
grep -E '\bMIKEBOM_' <files>   # should return empty
```

### 3. Update SBOM-parsing downstream tooling

Any tool that parses `metadata.properties[].name` (CycloneDX), `Annotation.comment` (SPDX 2.3), or `Annotation` values (SPDX 3) from Waybill SBOMs needs one prefix update:

**Rust example** — before:

```rust
if property.name == "mikebom:build-inclusion" { … }
```

After:

```rust
if property.name == "waybill:build-inclusion" { … }
```

**Handle both** during dual-version support (only if your tool needs to parse both pre-rename AND post-rename SBOMs):

```rust
match property.name
    .strip_prefix("mikebom:")
    .or_else(|| property.name.strip_prefix("waybill:"))
{
    Some(suffix) => { /* suffix-driven logic */ }
    None => { /* not a Waybill annotation */ }
}
```

**jq example**:

```jq
# Pre-rename query
.metadata.properties[] | select(.name | startswith("mikebom:"))

# Post-rename query
.metadata.properties[] | select(.name | startswith("waybill:"))
```

The 192 distinct suffixes are unchanged. Full list is grep-derivable from any pre-rename SHA:

```bash
git clone https://github.com/kusari-oss/waybill.git
cd waybill && git checkout v0.1.0-alpha.65
grep -rho '"mikebom:[a-z-]*"' mikebom-cli/src/ mikebom-common/src/ | sort -u
```

### 4. Update Docker image pull

```bash
# Before:
docker pull ghcr.io/kusari-oss/mikebom:v0.1.0-alpha.65

# After:
docker pull ghcr.io/kusari-oss/waybill:v0.1.0-alpha.66
```

Pre-rename image tags at `ghcr.io/kusari-oss/mikebom:*` remain accessible on GHCR; new tags land in `ghcr.io/kusari-oss/waybill:*`.

Container entrypoint changed from `mikebom` to `waybill`; volume-mount + argument-passing patterns are otherwise unchanged.

### 5. Update release-artifact URL patterns

```bash
# Before (pinned to alpha.65):
curl -LO https://github.com/kusari-oss/waybill/releases/download/v0.1.0-alpha.65/mikebom-v0.1.0-alpha.65-x86_64-unknown-linux-gnu.tar.gz

# After (alpha.66+):
curl -LO https://github.com/kusari-oss/waybill/releases/download/v0.1.0-alpha.66/waybill-v0.1.0-alpha.66-x86_64-unknown-linux-gnu.tar.gz
```

The GitHub org remains `kusari-oss`; the repo redirected from `mikebom` → `waybill` during the m214 rename.

## CI configuration migration example

**Before** (`.github/workflows/scan.yml`):

```yaml
- name: Install mikebom
  run: |
    curl -L https://github.com/kusari-oss/mikebom/releases/download/v0.1.0-alpha.65/mikebom-v0.1.0-alpha.65-x86_64-unknown-linux-gnu.tar.gz | tar xz
    sudo install mikebom /usr/local/bin/

- name: Scan
  env:
    MIKEBOM_LOG: info
    MIKEBOM_FIXED_TIMESTAMP: "2026-01-01T00:00:00Z"
  run: mikebom sbom scan --path . --format cyclonedx-json > sbom.cdx.json
```

**After**:

```yaml
- name: Install waybill
  run: |
    curl -L https://github.com/kusari-oss/waybill/releases/download/v0.1.0-alpha.66/waybill-v0.1.0-alpha.66-x86_64-unknown-linux-gnu.tar.gz | tar xz
    sudo install waybill /usr/local/bin/

- name: Scan
  env:
    WAYBILL_LOG: info
    WAYBILL_FIXED_TIMESTAMP: "2026-01-01T00:00:00Z"
  run: waybill sbom scan --path . --format cyclonedx-json > sbom.cdx.json
```

Diff: three occurrences of `mikebom` → `waybill` + two `MIKEBOM_` → `WAYBILL_`. Everything else identical.

## FAQ

**Q: Do my previously-generated SBOMs (with `mikebom:*` annotations) still work?**

Yes. Pre-rename SBOMs are unchanged; the m214 rename affects only what NEW SBOMs emit. Old SBOMs continue to be valid CycloneDX 1.6 / SPDX 2.3 / SPDX 3.0.1 documents. Downstream tools that only need to parse historical archives can keep their `mikebom:*` parsers.

**Q: Can Waybill accept SBOMs with `mikebom:*` annotations as INPUT for tools like `sbom parity-check` or `sbom enrich`?**

Post-rename, Waybill emits and parses only `waybill:*` annotations for its own outputs. Pre-rename SBOMs from third-party sources (or historical Waybill output) can still be read — the `sbom parity-check` and `sbom enrich` code paths ignore unknown-prefix annotations rather than rejecting them. But any invariant that specifically searches for `waybill:build-inclusion` etc. will not find `mikebom:build-inclusion` in the input; if your workflow crosses the rename boundary, run a preprocessing step: `jq '.metadata.properties[].name |= sub("^mikebom:"; "waybill:")' input.cdx.json`.

**Q: Do I need to update local Rust source that embeds Waybill as a library?**

Yes. Any `use mikebom_common::…` import needs to become `use waybill_common::…`. Cargo path deps referencing `mikebom-common` → `waybill-common`. See the Rust-identifier row of the "What renamed" table above.

**Q: My CI is failing after the rename with "command not found: mikebom". What now?**

Update your install step to use the new binary name (see the CI configuration migration example above). If you're pinned to a pre-rename release tag (`v0.1.0-alpha.7`..`v0.1.0-alpha.65`), the old binary is still available at those tags — you can keep using `mikebom` until you're ready to upgrade to `v0.1.0-alpha.66`+.

**Q: Why the rename?**

Project-identity decision made by the maintainer. The rename ships as a wire-shape breaking release with this migration guide as mitigation; no dual-emit or bridge-release period was implemented (project is pre-1.0 alpha).

## Full env-var mapping

See [`specs/214-rename-to-waybill/contracts/env-var-migration.md`](../../specs/214-rename-to-waybill/contracts/env-var-migration.md) for the exhaustive 73-entry table.

## Full annotation mapping

See [`specs/214-rename-to-waybill/contracts/annotation-migration.md`](../../specs/214-rename-to-waybill/contracts/annotation-migration.md). The prefix swap is mechanical (`mikebom:` → `waybill:`); all 192 suffixes are unchanged.

## Rollback

If you need to revert to the pre-rename state:

```bash
# Install the last pre-rename release
curl -L https://github.com/kusari-oss/waybill/releases/download/v0.1.0-alpha.65/mikebom-v0.1.0-alpha.65-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo install mikebom /usr/local/bin/
```

Downstream tooling parsing `mikebom:*` annotations continues to work against pre-rename SBOMs.
