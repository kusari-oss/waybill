# Data Model — milestone 090 fixture-repo split

This is a test-infrastructure refactor with no new domain types and no SBOM-emission impact. The "model" is the new fetch + cache + path-resolver pipeline.

## Entities

### Fixture repo (external, new)

A new Git repository at `kusari-sandbox/mikebom-test-fixtures` (HTTPS clone URL: `https://github.com/kusari-sandbox/mikebom-test-fixtures.git`).

**Internal layout** (mirrors mikebom main repo's pre-090 paths flattened):
```
mikebom-test-fixtures/
├── README.md                          # design intent + advisory: intentionally vulnerable
├── transitive_parity/
│   ├── cargo/                         # was: mikebom-cli/tests/fixtures/transitive_parity/cargo
│   ├── gem/
│   ├── go/
│   ├── maven/
│   ├── npm/
│   ├── pip_plain/
│   └── pip_poetry/
├── cargo/
│   ├── lockfile-v1-refused/           # was: tests/fixtures/cargo/lockfile-v1-refused
│   ├── lockfile-v2-refused/
│   ├── lockfile-v3/
│   └── lockfile-v4/
├── cargo-workspace/                   # was: mikebom-cli/tests/fixtures/cargo-workspace
├── gem/
│   └── simple-bundle/
├── go/
│   ├── argo-style-no-cache/
│   └── simple-module/
├── maven/
│   ├── pom-three-deps/
│   └── pom-with-property-ref/
├── maven-multi-module-reactor/        # was: mikebom-cli/tests/fixtures/maven-multi-module-reactor
├── npm/
│   ├── lockfile-v1-refused/
│   ├── lockfile-v3/
│   ├── lockfile-v3-transitive/
│   ├── node-modules-walk/
│   ├── package-json-only/
│   ├── pnpm-v8/
│   └── scoped-package/
├── npm-scoped-package/
├── npm-workspace/
├── pip-pyproject-pep621/
├── pip-pyproject-poetry-only/
├── polyglot-monorepo/
└── python/
    ├── pipfile-project/
    ├── poetry-project/
    ├── pyproject-only/
    ├── requirements-only/
    └── simple-venv/
```

**Validation rules**:
- VR-090-001: every directory in the move-set per research §4 MUST appear at the corresponding path in the new repo's initial commit.
- VR-090-002: the new repo's `README.md` MUST include the line "These are intentionally vulnerable test fixtures for mikebom. DO NOT use as a reference."
- VR-090-003: every fixture file (`*.lock`, `*.json`, `*.toml`, `*.xml`, etc.) MUST be byte-identical to its mikebom-pre-090 counterpart (verified by SHA256 cross-check during migration).

### Fixture revision pin (`tests/fixtures.rev`)

A single-line text file at the mikebom main repo root containing the pinned `mikebom-test-fixtures` Git SHA.

**Schema**:
```text
<40-char-lowercase-hex-sha>\n
```

(One trailing newline; no other content.)

**Validation rules**:
- VR-090-004: file MUST exist at `tests/fixtures.rev` (NOT inside `mikebom-cli/`).
- VR-090-005: file content MUST match the regex `^[0-9a-f]{40}\n$`.
- VR-090-006: the SHA MUST resolve to a commit in the `mikebom-test-fixtures` repo. Verified by build.rs at fetch time (clone fails fast if the SHA doesn't exist).

### Fixture cache directory

Per-host on-disk location where the fixture repo is cloned at build time.

**Default path**: `~/.cache/mikebom/fixtures/<pinned-sha>/`
**Override**: set `MIKEBOM_FIXTURE_CACHE` env var to a different parent directory; build.rs uses `$MIKEBOM_FIXTURE_CACHE/<pinned-sha>/`.

**Cache key**: the pinned SHA (40-char hex). Multiple SHAs co-exist (useful during git-bisect through mikebom history).

**Validation rules**:
- VR-090-007: cache directory MUST be readable + writable by the user running the build.
- VR-090-008: cache layout under `<cache>/<sha>/` MUST exactly mirror the fixture repo's root layout (no nested `mikebom-test-fixtures/` directory inside the SHA dir; the SHA dir IS the repo root).
- VR-090-009: build.rs MUST early-exit (no fetch) when `<cache>/<sha>/` exists AND contains a non-empty file count. Empty dir = re-fetch.

### `MIKEBOM_FIXTURES_DIR` compile-time env var

Set by build.rs via `cargo:rustc-env=MIKEBOM_FIXTURES_DIR=<path>`. Read by test code via `env!("MIKEBOM_FIXTURES_DIR")`.

**Validation rules**:
- VR-090-010: build.rs MUST emit `cargo:rustc-env=MIKEBOM_FIXTURES_DIR=<absolute-path>` exactly once per build.
- VR-090-011: the path MUST be absolute (not relative).
- VR-090-012: test code that uses `env!("MIKEBOM_FIXTURES_DIR")` MUST fail to compile if the env var is unset (compile-time guard against build.rs misconfiguration).

### `fixture_path(rel: &str) -> PathBuf` helper

New helper at `mikebom-cli/tests/common/fixtures.rs` (or extending an existing common module).

**Signature**:
```rust
pub fn fixture_path(rel: &str) -> std::path::PathBuf;
```

**Behavior**: returns `PathBuf::from(env!("MIKEBOM_FIXTURES_DIR")).join(rel)`. Pure function, no I/O.

**Validation rules**:
- VR-090-013: returned path MUST exist on disk if and only if the requested fixture migrated successfully.
- VR-090-014: helper MUST NOT shell-out, fetch, or otherwise touch the network — that's the build.rs's job.
- VR-090-015: existing `transitive_parity_common::fixture_path(subpath: &str)` helper at `mikebom-cli/tests/transitive_parity_common/mod.rs:50` MUST be updated to delegate to the new common helper (to avoid two parallel resolvers diverging).

## Migration mapping

The mechanical path-rewrite mapping for the 70 test files + 6 source files:

| Old pattern | New pattern |
|-------------|-------------|
| `workspace_root().join("tests/fixtures/<rel>")` | `fixture_path("<rel>")` |
| `workspace_root().join("mikebom-cli/tests/fixtures/<rel>")` (where `<rel>` is in move-set) | `fixture_path("<rel>")` |
| `workspace_root().join("mikebom-cli/tests/fixtures/golden/<rel>")` | UNCHANGED — goldens stay in main repo |
| `workspace_root().join("mikebom-cli/tests/fixtures/schemas/<rel>")` | UNCHANGED — schemas stay |
| `workspace_root().join("tests/fixtures/binaries/<rel>")` | UNCHANGED — binaries stay |
| `workspace_root().join("tests/fixtures/{apk,deb,rpm-files,bdb-rpmdb}/<rel>")` | UNCHANGED — OS-package fixtures stay |
| `workspace_root().join("tests/fixtures/{polyglot-rpm-binary,polyglot-five,reference,gem-source-project}/<rel>")` | UNCHANGED — non-manifest fixtures stay |
| `workspace_root().join("tests/fixtures/sample-attestation.json")` | UNCHANGED — single attestation stays |

**Validation rules**:
- VR-090-016: post-migration `git grep` for `workspace_root().join("tests/fixtures/<move-set-prefix>/...")` returns zero matches in mikebom-cli/.
- VR-090-017: post-migration `git grep` for `workspace_root().join("mikebom-cli/tests/fixtures/<move-set-prefix>/...")` returns zero matches.
- VR-090-018: post-migration `git grep` for `workspace_root().join("...golden...")` returns the same matches as pre-migration (FR-008 — goldens unchanged).

## CI cache layout

GitHub Actions cache scope: per-OS-lane (Linux, macOS, eBPF feature), keyed by the SHA in `tests/fixtures.rev`.

**Validation rules**:
- VR-090-019: `actions/cache` step's `key` parameter MUST hash `tests/fixtures.rev` so that pin bumps invalidate the cache cleanly.
- VR-090-020: `actions/cache` step's `path` parameter MUST point at `~/.cache/mikebom/fixtures` (the parent of the SHA-keyed subdirectory) so multiple historical SHAs stay cached.
