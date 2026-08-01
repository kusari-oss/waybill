# Quickstart — Scanning a Pants Python repo with waybill

**Feature**: 223-pants-pex-reader
**Audience**: platform teams running waybill against Pants monorepos,
Pants operators auditing their SBOM coverage, compliance stakeholders
verifying Python-package inventory from Pants-built artifacts.

---

## Prerequisites

- waybill built with feature 223 landed (see `waybill --version`)
- A Pants Python project with at least one Pex lockfile generated
  (`pants generate-lockfiles` has been run at least once)

---

## 1. Basic scan (default lockfile layout)

If your Pants repo puts lockfiles under `3rdparty/python/*.lock` (the
default layout), no additional configuration is needed:

```bash
waybill sbom scan \
    --path ~/src/my-pants-repo \
    --format cyclonedx-json \
    --output my-repo.cdx.json
```

waybill discovers every `.lock` file under `3rdparty/python/` and
emits one component per locked distribution. Grep for
`"pkg:pypi/"` in the output to verify Python coverage:

```bash
jq '.components[] | select(.purl | startswith("pkg:pypi/")) | .name' my-repo.cdx.json | wc -l
```

That count should match the number of locked distributions across
your Pex lockfiles.

## 2. Verify the FR-010 diagnostic

waybill logs one summary line per scan naming what it found:

```bash
RUST_LOG=info waybill sbom scan --path ~/src/my-pants-repo --format cyclonedx-json --output out.cdx.json 2>&1 | grep 'pants-pex reader complete'
```

Expected output:

```text
INFO waybill::scan_fs::package_db::pants: pants-pex reader complete
  lockfiles_discovered=3
  lockfiles_parsed_ok=3
  lockfiles_skipped_corrupt=0
  components_emitted=147
```

If `lockfiles_discovered=0`: your Pants repo either has no
`3rdparty/python/*.lock` files OR uses a custom `pants.toml`
`[python].lockfile` path that also doesn't exist. See §3.

## 3. Custom lockfile path via `pants.toml`

Some Pants repos declare a non-default lockfile location:

```toml
# pants.toml
[python]
lockfile = "build-support/python.lock"
```

waybill honors this automatically — no additional flag needed. The
FR-010 log line reflects the actual paths discovered:

```text
lockfiles_discovered=1  # picked up build-support/python.lock via pants.toml
```

## 4. Multi-resolve repositories

Pants supports multiple named resolves (default + mypy + pytest + ci
+ etc.), each with its own lockfile at
`3rdparty/python/<resolve-name>.lock`. waybill scans every resolve
and tags components by their source:

```bash
# Every component gets a waybill:pants-resolve annotation identifying
# its source lockfile. Group components by resolve:
jq -r '.components[] |
    select(.purl | startswith("pkg:pypi/")) |
    "\((.properties[]? | select(.name == "waybill:pants-resolve") | .value) // "unknown")\t\(.name)"' \
    my-repo.cdx.json | sort | uniq -c | head -20
```

waybill also tags components from known dev-tool resolves
(`mypy`, `pytest`, `black`, `ruff`, `isort`, `flake8`, `bandit`,
`coverage`, `sphinx`, `lint`, `test`, `dev`, `ci`, `check`, `tools`
— see [research.md §R2](./research.md#r2--dev-resolve-name-allowlist-for-fr-008-lifecycle-scope-tagging))
with `lifecycle_scope=Dev`, so downstream security tooling can
filter them out of production dependency inventories.

Grep for dev-scope components:

```bash
jq '.components[] |
    select(.properties[]? | .name == "waybill:lifecycle-scope" and .value == "dev") |
    .purl' my-repo.cdx.json
```

## 5. Coexistence with `requirements.txt`

If your repo has BOTH a Pex lockfile AND a `requirements.txt`
(common in repos migrating to Pants, or repos that export
requirements for non-Pants tooling like Dependabot / IDEs), waybill
emits each package exactly once. The Pex lockfile wins because it
carries artifact hashes; the requirements.txt source is recorded via
the existing `waybill:also-detected-via` annotation for audit.

Verify no duplicates:

```bash
jq '.components[] | select(.purl | startswith("pkg:pypi/")) | .purl' \
    my-repo.cdx.json | sort | uniq -d
```

The output should be empty. Any duplicate PURL indicates a dedup bug
— please file an issue with the offending fixture.

## 6. Non-PyPI locked entries (git URLs, direct downloads, local wheels)

Pex lockfiles can carry entries whose source is a git URL, direct
download URL, or a local `.whl` file — not just PyPI-published
distributions. waybill emits these as `pkg:generic/*` PURLs to keep
vuln-scanner semantics honest (vuln scanners that pivot on PURL will
not falsely look up PyPI CVEs for a git-sourced package).

Every `pkg:generic/*` entry carries two annotations for traceability:

```bash
jq '.components[] |
    select(.purl | startswith("pkg:generic/")) |
    {
        purl,
        source_url: (.properties[]? | select(.name == "waybill:source-url") | .value),
        source_type: (.properties[]? | select(.name == "waybill:source-type") | .value)
    }' my-repo.cdx.json
```

Expected `source_type` values: `git`, `url`, `local`.

## 7. What if my Pants repo isn't scanned correctly?

**Case A: `lockfiles_discovered=0` but you know your repo uses Pants.**

Verify the lockfile path:

```bash
find ~/src/my-pants-repo -name '*.lock' -path '*python*' -not -path '*/node_modules/*'
```

If lockfiles exist at a non-default path, either:
- Move them to `3rdparty/python/*.lock` (Pants's default convention), OR
- Add a `[python].lockfile = "..."` entry to your `pants.toml` so
  waybill can discover them.

**Case B: `lockfiles_skipped_corrupt >= 1`.**

Grep for the WARN diagnostic — waybill names the offending file:

```bash
RUST_LOG=warn waybill sbom scan --path ~/src/my-pants-repo ... 2>&1 | grep pants-pex
```

Common causes:
- `pants generate-lockfiles` was interrupted (empty or partial JSON).
- Manual edit corrupted the file. Run `pants generate-lockfiles`
  again to regenerate.
- Pex format version mismatch (e.g., a Pex 1.x plaintext lockfile).
  Upgrade to Pants 2.x + Pex 2.x.

**Case C: waybill's component count is much lower than your `pants
peek --filter-target-type=python_requirement` output.**

`pants peek` reports design-tier declarations from `BUILD` files;
waybill parses source-tier lockfiles. Discrepancy is expected when
some `python_requirement` targets are declared but not actually
resolved into any lockfile (i.e., no `python_source` target ever
requested them). See the follow-up milestone note in
[plan.md §Follow-ups](./plan.md#follow-ups-out-of-scope-for-this-branch)
about design-tier `BUILD` file parsing.

## What this feature does NOT change

- Repos with no Pants config or Pex lockfiles: SBOM output is
  byte-identical to pre-feature-223 goldens per FR-007 / SC-003.
- The pip / poetry / uv readers are unchanged; they run alongside
  the new pants-pex reader without conflict.
- No new CLI flags. No new subcommands.
- No new dependencies (Pex lockfile is JSON; `serde_json` is already
  a workspace dep).
- `waybill trace` (the eBPF path) is untouched. This feature is
  user-space filesystem parsing only.
