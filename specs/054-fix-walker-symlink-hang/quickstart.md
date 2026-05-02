# Quickstart: milestone 054 (walker symlink-loop fix)

3-step verification recipe. Run after implementation lands; doubles as the SC-001 + SC-006 acceptance evidence.

## Prerequisites

- Debug build of post-054 mikebom: `cargo build -p mikebom`.
- `git`, `jq` on `$PATH`.
- Network access for the initial clone (steps thereafter are offline).

## Step 1: Reproduce the user's exact original case

```sh
cd /tmp && rm -rf knative-func-054 && \
  git clone --depth 1 --branch knative-v1.22.0 \
    https://github.com/knative/func.git knative-func-054
```

## Step 2: Run mikebom against the cloned project

```sh
HOME=$(mktemp -d) GOMODCACHE=$(mktemp -d)/empty \
  /Users/mlieberman/Projects/mikebom/target/debug/mikebom \
    --offline sbom scan \
    --path /tmp/knative-func-054 \
    --format spdx-2.3-json \
    --output /tmp/knative-func-054.spdx.json \
    --no-deep-hash
```

## Expected outcome

- **Pre-054**: 100% CPU spin, no output, would have to be killed after 10+ min.
- **Post-054**: scan completes in ≤ 60 seconds with exit 0; emits a valid SPDX 2.3 SBOM at the output path.

## Step 3: Validate the output

```sh
jq '{
  total_rels: (.relationships | length),
  total_pkgs: (.packages | length),
  golang_pkgs: ([.packages[] | select(.externalRefs[]?.referenceLocator | startswith("pkg:golang/"))] | length),
  has_main_module: ([.packages[] | select(.primaryPackagePurpose == "APPLICATION")] | length),
  spdx_version: .spdxVersion
}' /tmp/knative-func-054.spdx.json
```

**Expected output** (exact counts may shift slightly with knative-v1.22.0 content updates):

```json
{
  "total_rels": 700,
  "total_pkgs": 420,
  "golang_pkgs": 200,
  "has_main_module": 9,
  "spdx_version": "SPDX-2.3"
}
```

Invariants (per SC-001 + SC-007):

- `total_pkgs` ≥ 200 — knative/func has many Go modules.
- `golang_pkgs` ≥ 200 — most components are Go modules from go.sum.
- `has_main_module` ≥ 1 — milestone 053's main-module component emits per Go workspace.
- `spdx_version` is `"SPDX-2.3"` — schema-valid.

## Smoke test as a regression guard (CI-enforced)

The post-054 implementation MUST include:

1. **Unit test per walker** under `#[cfg(test)] mod tests` exercising a synthesized minimal symlink-loop fixture:

   ```rust
   #[test]
   fn walks_symlink_loop_without_hanging() {
       let tmp = tempfile::tempdir().expect("tempdir");
       let loop_dir = tmp.path().join("loop");
       std::fs::create_dir_all(&loop_dir).unwrap();
       std::os::unix::fs::symlink(&loop_dir, loop_dir.join("link")).unwrap();
       // The walk must terminate, not hang.
       let _ = walk_<...>(tmp.path(), /* ... */);
   }
   ```

2. **New CI workflow** `.github/workflows/realistic-projects.yml` that clones knative/func at `knative-v1.22.0` per CI run + scans + asserts schema-validity + asserts component floor.

## Failure modes

If Step 2 hangs in the post-054 build:

- A walker patch was missed. Re-run `grep -rn "fn walk" mikebom-cli/src/scan_fs/` and audit each match for the visited-set + depth-limit invariants from the contract.
- The visited-set was created inside the recursive function (creating a fresh empty set per call) instead of at the top-level entry point. Insert MUST happen against a set scoped to the entire walker invocation.
- The visited-set membership check happened AFTER recursion instead of BEFORE. Order matters: insert-then-recurse or check-then-skip.

If Step 3 reports zero `golang_pkgs` or zero `has_main_module`:

- Milestone 053's main-module emission may have regressed. Verify with `git log --oneline mikebom-cli/src/scan_fs/package_db/golang.rs | head -5` shows the build_main_module_entry commit is present.
- The visited-set may be too aggressively skipping (e.g., keying by path-equality instead of canonicalize + collapsing legitimate-but-symlinked subtrees). Compare against `golang.rs:1162-1167` reference pattern.

## Outside this milestone

- The migration to a shared `safe_walk` helper is tracked in issue #108. After that lands, the per-walker patches in this milestone become the "before" state; post-#108 every walker delegates to one helper.
- The realistic-project CI matrix expansion (npm, cargo, maven projects beyond knative/func) is out of scope here; future milestones may add per-ecosystem realistic fixtures.
