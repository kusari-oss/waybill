# Quickstart — milestone 090 maintainer recipes

Six maintainer-facing recipes for bootstrapping the new fixture repo, migrating fixtures out of mikebom main repo, wiring the build.rs + helper, smoke-testing, regenerating CI cache, and confirming the post-090 scan-cleanliness payoff.

## Recipe 1 — Bootstrap the new mikebom-test-fixtures repo

```bash
# 1. Create the new repo on GitHub (org-level action — done outside this milestone):
gh repo create kusari-sandbox/mikebom-test-fixtures \
    --public \
    --description "Intentionally-vulnerable test fixtures for the mikebom SBOM tool. See README.md."

# 2. Seed the new repo with the move-set content (44 directories per research §4):
WORK=/tmp/mikebom-test-fixtures-seed
mkdir -p "$WORK"
cd "$WORK"
git init -b main
# Copy the move-set from mikebom main repo:
MIKEBOM=/Users/mlieberman/Projects/mikebom
# manifest-bearing dirs from mikebom-cli/tests/fixtures/:
for d in cargo-workspace maven-multi-module-reactor npm-scoped-package npm-workspace \
         pip-pyproject-pep621 pip-pyproject-poetry-only; do
  cp -r "$MIKEBOM/mikebom-cli/tests/fixtures/$d" "./$d"
done
mkdir -p transitive_parity
for d in cargo gem go maven npm pip_plain pip_poetry; do
  cp -r "$MIKEBOM/mikebom-cli/tests/fixtures/transitive_parity/$d" "./transitive_parity/$d"
done
# manifest-bearing dirs from tests/fixtures/:
for d in cargo gem go maven npm polyglot-monorepo python; do
  cp -r "$MIKEBOM/tests/fixtures/$d" "./$d"
done

# 3. Write README.md documenting the design intent:
cat > README.md <<'EOF'
# mikebom-test-fixtures

These are intentionally vulnerable test fixtures for mikebom. **DO NOT use as a reference.**

This repo exists to keep mikebom's main repo free of fake-project trigger surface for security scanners. mikebom's test suite clones this repo at build time via the `tests/fixtures.rev` pin in mikebom main repo + `mikebom-cli/build.rs`.

See `specs/090-split-test-fixtures-repo/` in the mikebom main repo for the migration design.

## Layout

Mirrors mikebom's pre-090 directory structure:
- `transitive_parity/<eco>/` — milestone-083 audit fixtures.
- `<eco>/<name>/` — per-ecosystem reader fixtures.
- `polyglot-monorepo/` — multi-ecosystem fixture.
- `cargo-workspace/`, `maven-multi-module-reactor/`, `npm-scoped-package/`, `npm-workspace/`, `pip-pyproject-pep621/`, `pip-pyproject-poetry-only/` — workspace-style fixtures.

## Adding a fixture

1. Add the fixture directory in this repo.
2. Bump the pin in mikebom main repo: edit `tests/fixtures.rev` to the new SHA.
3. Update tests in mikebom main repo to reference the new fixture via `fixture_path("...")`.
EOF

# 4. Initial commit + push:
git add .
git commit -m "Initial seed from mikebom main repo @ alpha.27 (milestone 090)"
git remote add origin https://github.com/kusari-sandbox/mikebom-test-fixtures.git
git push -u origin main

# 5. Capture the SHA for the mikebom main repo's tests/fixtures.rev:
git rev-parse HEAD
# (copy this SHA — you'll write it to tests/fixtures.rev)
```

## Recipe 2 — Migrate mikebom main repo

```bash
cd /path/to/mikebom

# 1. Create the pin file:
echo "<sha-from-recipe-1-step-5>" > tests/fixtures.rev

# 2. Remove the moved fixture directories:
git rm -r mikebom-cli/tests/fixtures/cargo-workspace
git rm -r mikebom-cli/tests/fixtures/maven-multi-module-reactor
git rm -r mikebom-cli/tests/fixtures/npm-scoped-package
git rm -r mikebom-cli/tests/fixtures/npm-workspace
git rm -r mikebom-cli/tests/fixtures/pip-pyproject-pep621
git rm -r mikebom-cli/tests/fixtures/pip-pyproject-poetry-only
git rm -r mikebom-cli/tests/fixtures/transitive_parity
git rm -r tests/fixtures/cargo
git rm -r tests/fixtures/gem
git rm -r tests/fixtures/go
git rm -r tests/fixtures/maven
git rm -r tests/fixtures/npm
git rm -r tests/fixtures/polyglot-monorepo
git rm -r tests/fixtures/python

# 3. Write build.rs (see Recipe 3).

# 4. Add the fixture_path helper to mikebom-cli/tests/common/fixtures.rs (see contract).

# 5. Mechanical path rewrites in test files (see Recipe 4).

# 6. Smoke test (see Recipe 5).
```

## Recipe 3 — Write `mikebom-cli/build.rs`

```rust
// mikebom-cli/build.rs
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../tests/fixtures.rev");
    println!("cargo:rerun-if-env-changed=MIKEBOM_FIXTURE_CACHE");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let pin_path = manifest_dir.join("..").join("tests").join("fixtures.rev");
    let sha = std::fs::read_to_string(&pin_path)
        .unwrap_or_else(|e| panic!("\nfailed to read fixture pin at {}: {}\n", pin_path.display(), e))
        .trim()
        .to_string();
    if !sha.chars().all(|c| c.is_ascii_hexdigit()) || sha.len() != 40 {
        panic!("\ntests/fixtures.rev MUST be a 40-char hex SHA; got {sha:?}\n");
    }

    let cache_parent = std::env::var("MIKEBOM_FIXTURE_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").expect("HOME unset");
            PathBuf::from(home).join(".cache").join("mikebom").join("fixtures")
        });
    let cache_target = cache_parent.join(&sha);
    let url = "https://github.com/kusari-sandbox/mikebom-test-fixtures.git";

    if cache_target.exists() && std::fs::read_dir(&cache_target).map(|d| d.count()).unwrap_or(0) > 0 {
        // Cache hit — skip fetch.
        println!("cargo:rustc-env=MIKEBOM_FIXTURES_DIR={}", cache_target.display());
        return;
    }

    // Cache miss — clone + pin.
    std::fs::create_dir_all(&cache_parent).expect("create cache parent");
    println!("cargo:warning=fetching mikebom-test-fixtures @ {sha} (one-time per pin)");
    let clone_status = Command::new("git")
        .args(["clone", "--depth", "1", url, cache_target.to_str().unwrap()])
        .status();
    if !matches!(clone_status, Ok(s) if s.success()) {
        panic!("\nFailed to fetch mikebom-test-fixtures revision {sha}:\n    URL:   {url}\n    Cache: {}\n\nWorkaround:\n    1. Verify network access to github.com.\n    2. Manually clone: git clone {url} {}\n                       git -C {} reset --hard {sha}\n    3. Re-run cargo build.\n",
            cache_target.display(), cache_target.display(), cache_target.display());
    }
    // Pin to exact SHA:
    let _ = Command::new("git").args(["-C", cache_target.to_str().unwrap(), "fetch", "origin", &sha]).status();
    let reset_status = Command::new("git").args(["-C", cache_target.to_str().unwrap(), "reset", "--hard", &sha]).status();
    if !matches!(reset_status, Ok(s) if s.success()) {
        panic!("\nfailed to pin {} to {sha}\n", cache_target.display());
    }

    println!("cargo:rustc-env=MIKEBOM_FIXTURES_DIR={}", cache_target.display());
}
```

## Recipe 4 — Path rewrites in test files

The mechanical rewrite covers ~76 call sites. Use a script:

```bash
cd /path/to/mikebom

# Find all files referencing the move-set paths:
git grep -l 'workspace_root().join("tests/fixtures/' \
         -e 'workspace_root().join("mikebom-cli/tests/fixtures/transitive_parity/' \
         -e 'workspace_root().join("mikebom-cli/tests/fixtures/cargo-workspace' \
         -e 'workspace_root().join("mikebom-cli/tests/fixtures/maven-multi-module-reactor' \
         -e 'workspace_root().join("mikebom-cli/tests/fixtures/npm-' \
         -e 'workspace_root().join("mikebom-cli/tests/fixtures/pip-' \
         mikebom-cli/

# For each file, rewrite per the data-model.md migration mapping:
#   workspace_root().join("tests/fixtures/<rel>")    → fixture_path("<rel>")
#   workspace_root().join("mikebom-cli/tests/fixtures/<move-set-prefix>/<rel>") → fixture_path("<move-set-prefix>/<rel>")
#
# Each affected file gets a `use crate::common::fixtures::fixture_path;`
# (or equivalent) at the top, replacing whatever workspace_root import.

# Verify no leftover references:
! git grep 'workspace_root().join("tests/fixtures/cargo/' mikebom-cli/  # cargo move-set
! git grep 'workspace_root().join("mikebom-cli/tests/fixtures/transitive_parity' mikebom-cli/

# Goldens references should be UNCHANGED:
git grep 'workspace_root().join("mikebom-cli/tests/fixtures/golden' mikebom-cli/
# Expected: same matches as pre-090.
```

## Recipe 5 — Smoke test

```bash
# 1. Clear cache for a fresh test:
rm -rf ~/.cache/mikebom/fixtures

# 2. Build (triggers fetch):
time cargo +stable build --workspace
# Expected: warning: fetching mikebom-test-fixtures @ <sha> (one-time per pin)
# First-fetch wall-time ≤30 s.

# 3. Confirm cache populated:
ls ~/.cache/mikebom/fixtures/<sha>/
# Expected: README.md, transitive_parity/, cargo/, etc.

# 4. Run tests:
cargo +stable test --workspace
# Expected: every test suite `0 failed`.

# 5. Cache-warm test:
cargo +stable test --workspace
# Expected: build.rs fast-paths (no `fetching...` warning); tests still pass.

# 6. Offline test (cache-warm + no network):
# (Disable network in your shell, then)
cargo +stable test --workspace
# Expected: pass.

# 7. Pre-PR gate:
./scripts/pre-pr.sh
# Expected: zero clippy warnings, every test suite `0 failed`.
```

## Recipe 6 — Confirm post-090 scan cleanliness

```bash
# Run trivy against the post-090 mikebom main repo WITHOUT --skip-dirs flags:
trivy --quiet fs --scanners vuln --skip-dirs target --format json --output /tmp/post-090.json .

# Count vulns:
jq '[.Results[]?.Vulnerabilities[]?] | length' /tmp/post-090.json
# Expected: 4 (only the rustls-webpki@0.102.x residuals from milestone 089's known-acceptances.md).

# Compare against pre-090:
trivy --quiet fs --scanners vuln --skip-dirs target --format json --output /tmp/pre-090.json /path/to/pre-090-clone-of-mikebom
jq '[.Results[]?.Vulnerabilities[]?] | length' /tmp/pre-090.json
# Expected pre-090: ≥38 (the trigger surface this milestone is removing).

# Confirm fixture cache is OUTSIDE the scan (lives in ~/.cache/, not the repo):
ls ~/.cache/mikebom/fixtures/<sha>/
# Expected: fixture content present, but trivy scan against the mikebom repo doesn't see it
# because it's not under the scanned working directory.
```

## Recipe 7 — Update CI for the fixture cache

Edit `.github/workflows/ci.yml`. For each lane (`lint-and-test`, `lint-and-test-macos`, `lint-and-test-ebpf`), add a step BEFORE `Clippy`:

```yaml
- name: Cache fixture repo
  uses: actions/cache@v4
  with:
    path: ~/.cache/mikebom/fixtures
    key: mikebom-fixtures-${{ runner.os }}-${{ hashFiles('tests/fixtures.rev') }}
```

Cache survives across CI runs at the SHA level; fixture-pin bumps cleanly invalidate.

## When in doubt

- **build.rs panic on a fresh clone with network access**: check `tests/fixtures.rev` exists + matches `^[0-9a-f]{40}$`. If file is missing (e.g., checking out a pre-090 commit during git-bisect), the panic message points the way.
- **Tests pass locally but fail in CI**: the CI cache step might be misconfigured. Check the `actions/cache` step is BEFORE the `Tests` step in the lane. Also check the cache key includes `runner.os` (macOS lane uses different cache scope).
- **Disk usage growing in `~/.cache/mikebom/fixtures/`**: each historical SHA adds ~17 MB. After 100 pins, that's 1.7 GB. Manual cleanup: `rm -rf ~/.cache/mikebom/fixtures` and re-run cargo build (will re-fetch the current pin).
- **Goldens regenerate post-migration**: scope creep. The migration is content-preserving. Investigate whether a path-rewrite accidentally pointed at the wrong location.
- **`mikebom-test-fixtures` repo clone fails over corporate proxy**: build.rs shells out to `git`, so set `https_proxy` / `http_proxy` env vars per your local convention. The `git` config picks them up automatically.
