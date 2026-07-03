# Quickstart — milestone 156

Validation walkthrough for the CMake walker depth extension. SC-001 is manual operator-cadence (Kamailio); everything else is automated pre-PR.

## Scenario 1 — SC-001 Kamailio testbed ≥10 (MANUAL operator-cadence)

After the milestone-156 PR merges, the maintainer runs:

```bash
# 1. Build mikebom at milestone-156 HEAD:
cargo +stable build --release -p mikebom

# 2. Ensure a Kamailio checkout at /Users/mlieberman/Projects/kamailio.
# If missing:
git clone --depth 1 https://github.com/kamailio/kamailio /tmp/kamailio

# 3. Run mikebom against the Kamailio tree:
./target/release/mikebom --offline sbom scan \
    --path /Users/mlieberman/Projects/kamailio \
    --format cyclonedx-json \
    --output cyclonedx-json=/tmp/mikebom-m156/kamailio.cdx.json \
    --no-deep-hash

# 4. Count cmake-derived components:
jq '[.components[] | select(.properties[]?
      | select(.name == "mikebom:source-mechanism"
               and (.value == "cmake-find-package"
                    or .value == "cmake-pkg-check-modules")))] | length' \
    /tmp/mikebom-m156/kamailio.cdx.json
```

**Expected output**: integer **≥ 10**. Up from **1** post-milestone-155.

```bash
# 5. Verify the specific expected names appear:
jq -r '[.components[] | select(.properties[]?
      | select(.name == "mikebom:source-mechanism"
               and (.value == "cmake-find-package"
                    or .value == "cmake-pkg-check-modules")))
    | .name] | sort | unique | .[]' \
    /tmp/mikebom-m156/kamailio.cdx.json
```

**Expected names** (order-independent, subset match — Kamailio HEAD may have moved by shipping time):
```
erlang
ldap
libev
libfreeradiusclient
mariadbclient
netsnmp
openssl
oracle
radcli
radius
unistring
```

If both checks pass → ✅ SC-001 PASS. Report in the PR comments + close the milestone-155 F1 remediation debt reference.

## Scenario 2 — SC-002 byte-identical guard (automated)

```bash
cargo +stable test --workspace --no-fail-fast --test cdx_regression --test spdx_regression --test spdx3_regression 2>&1 | grep -E '^test result|FAILED'
```

**Expected**: 33 tests total (11 per format × 3 formats); all pass. **Zero golden regenerations required**.

Additionally the milestone-155 Kamailio-shape fixture integration test:

```bash
cargo +stable test --workspace --test cmake_find_package_kamailio_shape_integration
```

**Expected**: still passes with the SAME 5 cmake-find-package + 1 cmake-pkg-check-modules count. The depth-2 `FindLibev.cmake` file that milestone 156 now discovers contains ONLY a `find_package_handle_standard_args(Libev ...)` call — FR-009 correctly rejects it.

## Scenario 3 — SC-003 symlink cycle safety (automated)

```bash
cargo +stable test --workspace --test cmake_walker_depth_symlink_cycle
```

**Expected**: test passes in <5s (test-level timeout gate). safe_walk's canonicalize-keyed visited-set catches the `cmake/loop -> cmake/` cycle on second arrival.

## Scenario 4 — SC-004 depth-3 emission (automated)

```bash
cargo +stable test --workspace --test cmake_walker_depth_deep_emission
```

**Expected**: fixture with `find_package(Foo 2.5)` at `cmake/modules/vendor/Extra.cmake` (depth-3 from scan root) → one `pkg:generic/foo@2.5` component emitted with `mikebom:source-files` naming the depth-3 path.

## Scenario 5 — SC-005 cross-depth version consolidation (automated)

```bash
cargo +stable test --workspace --test cmake_walker_depth_cross_depth_version
```

**Expected**: fixture with `find_package(OpenSSL 1.1.0)` at `<root>/CMakeLists.txt` (depth-0) AND `find_package(OpenSSL 3.0)` at `<root>/cmake/modules/FindOpenSSL.cmake` (depth-3). Milestone 155's Q1 highest-version-wins consolidation fires across depths → one `pkg:generic/openssl@3.0` component; both file paths in `mikebom:source-files`.

## Scenario 6 — SC-006 exclude-path integration (automated)

```bash
cargo +stable test --workspace --test cmake_walker_depth_exclude_path
```

**Expected**: fixture with `find_package(Foo)` at `cmake/modules/FindFoo.cmake`. Test invokes `mikebom sbom scan --exclude-path cmake/modules/` → 0 `cmake-find-package` components emitted.

## Scenario 7 — SC-007 pre-PR gate (mandatory)

```bash
./scripts/pre-pr.sh
```

**But also** — per the milestone-155 fix memory (`feedback_prepr_gate_bails_on_first_failure.md`) — explicitly run:

```bash
cargo +stable test --workspace --no-fail-fast 2>&1 | grep -E '^---- .+ stdout ----'
```

**Expected**: only `sbomqs_spdx_score_meets_or_beats_cdx_across_ecosystems` appears (documented env-only flake). Every other test binary passes.

**Do not open a PR without both commands green.** The `--no-fail-fast` re-run is the mandatory belt-and-suspenders check for any emission-changing milestone.

## Scenario 8 — SC-008 unit-test count (automated)

```bash
grep -cE "^\s+fn discover_cmake_files_" mikebom-cli/src/scan_fs/package_db/cmake.rs
```

**Expected**: ≥6 (per SC-008 floor; research.md §R6 inventories 6 unit tests + 5 integration tests = 11 total, ≥6 with the `discover_cmake_files_` prefix).

## Scenario 9 — SC-009 CHANGELOG (manual)

```bash
sed -n '/^## \[Unreleased\]/,/^## \[v/p' CHANGELOG.md \
  | grep -E 'walker-depth|discover_cmake_files|cmake-third-party-recursive|milestone 156'
```

**Expected**: entries present naming the walker-depth extension, Kamailio impact, opt-in flag, and F1 remediation reference.

## Scenario 10 — SC-010 wire-format guard (manual diff check)

```bash
git diff main --name-only
```

**Expected file list** (order may vary):
- `CHANGELOG.md`
- `CLAUDE.md`
- `mikebom-cli/src/cli/scan_cmd.rs` (new arg field + env propagation)
- `mikebom-cli/src/scan_fs/package_db/cmake.rs` (primary — recursive walker + safe_walk integration)
- `mikebom-cli/src/scan_fs/package_db/mod.rs` (single-line cmake::read call-site update)
- `mikebom-cli/src/scan_fs/binary/mod.rs` (single-line cmake::read call-site update)
- `mikebom-cli/tests/cmake_walker_depth_*.rs` (5 new integration test files)
- `mikebom-cli/tests/fixtures/cmake-walker-depth/**` (new fixture directory)
- `specs/156-cmake-walker-depth/**` (speckit branch artifacts)

**Prohibited changes** (must be empty):
```bash
git diff main --name-only -- mikebom-cli/src/generate/     # No emitter changes
git diff main --name-only -- mikebom-cli/src/parity/       # No parity extractor changes
git diff main --name-only -- docs/reference/sbom-format-mapping.md   # No catalog row changes
git diff main --name-only -- mikebom-common/ mikebom-ebpf/ # No other-crate changes
git diff main --name-only -- mikebom-cli/tests/fixtures/golden/       # No golden regeneration
git diff main --name-only -- mikebom-cli/src/scan_fs/package_db/ | grep -v 'cmake.rs\|^mod.rs$'  # No other-reader changes
```

## Scenario 11 — SC-011 opt-in flag off-by-default + on-when-set (automated)

```bash
cargo +stable test --workspace --test cmake_walker_depth_third_party_opt_in
```

**Expected**: 2 sub-tests in one binary — one asserting zero `cmake-find-package` components without the flag; one asserting exactly one after setting `MIKEBOM_CMAKE_THIRD_PARTY_RECURSIVE=1`.

## Post-merge — operator-cadence external review

SC-001 (Kamailio manual scan) is manual per spec Assumption 2. Everything else runs pre-PR automatically. Report SC-001 result in the PR merge comment; open a follow-up issue if Kamailio HEAD has moved past ≥10 findable dep names.

## Known deferrals (spec Out of Scope)

- No recursive walk of `<root>/src/**/CMakeLists.txt` (FR-017).
- No `add_subdirectory` chain following.
- No `include(...)` directive resolution.
- No `CMAKE_MODULE_PATH` cache variable evaluation.
- No auto-exclusion of well-known CMake build directories (FR-018 — operators use `--exclude-path` if needed).
- No new `mikebom:*` annotation keys (FR-015).
- No CDX / SPDX 2.3 / SPDX 3 emitter code changes (SC-010).
- No catalog row additions (milestone-155's C55 + C103 cover everything).
