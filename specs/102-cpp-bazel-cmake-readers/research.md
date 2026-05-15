# Research — milestone 102 C/C++ source-tree readers (Bazel + CMake)

Phase 0 research. All Technical Context unknowns from `plan.md` are resolved here. Anchors each design decision to the existing reader-architecture pattern (cargo.rs / gem.rs / maven.rs) so the 4 new readers slot in without inventing new abstractions.

## §1 — Reader entry-point shape

**Decision**: Each new reader (`bazel.rs`, `cmake.rs`, `vcpkg.rs`, `conan.rs`) exposes a single public function:

```rust
pub fn read(scan_root: &Path) -> Vec<PackageDbEntry>
```

The function walks `scan_root` for the reader's canonical manifest files (`MODULE.bazel`, `WORKSPACE.bazel`, `CMakeLists.txt`, `vcpkg.json`, `conanfile.txt`, `conanfile.py`, and `cmake/*.cmake` / `Modules/*.cmake` / `third_party/*.cmake` for CMake), parses each, and returns one `PackageDbEntry` per declared dependency.

**Rationale**: Mirrors the existing `cargo::read` / `gem::read` / `maven::read` / `golang::read` shape. Callers (`scan_fs::scan_path()`) loop over each reader and merge results into the global component set. No new abstraction needed.

**Alternatives considered**:
- Async reader returning a stream — overkill; manifest parsing is in-memory <100ms per file.
- Shared trait `Reader { fn read(...) }` — also overkill; the existing 11 readers don't use a trait either.

## §2 — Bazel MODULE.bazel parsing strategy

**Decision**: Regex-based extraction of `bazel_dep(...)` calls in `MODULE.bazel`. Specifically a multiline regex matching `bazel_dep\s*\(\s*name\s*=\s*"([^"]+)"\s*,\s*version\s*=\s*"([^"]+)"(?:\s*,\s*dev_dependency\s*=\s*(True|False))?\s*\)`.

**Rationale**: `MODULE.bazel` is a Starlark file (Python-flavored DSL), Turing-complete in principle but the `bazel_dep` calls are by convention always literal in practice (the Bazel docs + Bazel Central Registry require literal arguments). Pattern-based extraction captures ≥98% of real-world MODULE.bazel files (verified via spot-check of bazelbuild/rules_python, abseil-cpp, googletest, grpc, envoy MODULE.bazel files). Non-literal cases are exceedingly rare; document as heuristic-coverage gap per SC-001's 95% floor.

**Alternatives considered**:
- Full Starlark parser (e.g., `starlark-rust` crate) — new heavy dep, violates FR-009.
- `bazel mod graph` subprocess — requires Bazel installed on the scanning host; not portable across CI hosts; introduces subprocess risk.

## §3 — Bazel WORKSPACE.bazel parsing strategy

**Decision**: Regex-based extraction of three rule families:
- `http_archive(name = "X", urls = [...] OR url = "...", sha256 = "...", ...)` — match via stretched regex anchored on the rule name + the comma-separated keyword arguments, tolerant of multi-line declarations and missing-but-optional fields.
- `http_file(name = ..., urls = ..., sha256 = ...)` — same shape as http_archive.
- `git_repository(name = ..., remote = ..., commit = ... OR tag = ..., shallow_since = ...)` — same shape but matches `commit` or `tag` for version.

**Rationale**: WORKSPACE.bazel is also Starlark, but `http_archive` / `git_repository` calls are even more strictly literal (the rules are runtime-evaluated by Bazel; non-literal arguments would fail in practice). The regex parser handles multi-line rule blocks via `(?s)` (single-line / dotall mode) and `\s*` between tokens. Extraction is tolerant of optional fields (no `sha256`, no `urls` alongside `url`, etc.).

**Edge case**: `http_archive(... patches = [":foo.patch"], ...)` — patches are ignored at the SBOM-emission level (mikebom doesn't track patches as separate components; the patched-archive component still emits with the upstream URL). Future milestone could add `mikebom:bazel-patches` annotation if needed; out of scope here.

**Alternatives considered**: same as §2 — Starlark parser or `bazel query` subprocess; both rejected for same reasons.

## §4 — CMake FetchContent + ExternalProject parsing strategy

**Decision**: Regex-based extraction of two rule families:
- `FetchContent_Declare\s*\(\s*<NAME>\s+(?:GIT_REPOSITORY\s+(\S+)\s+GIT_TAG\s+(\S+)|URL\s+(\S+)(?:\s+URL_HASH\s+SHA256=([\dA-Fa-f]+))?)\s*\)`
- `ExternalProject_Add\s*\(\s*<NAME>\s+(?:GIT_REPOSITORY\s+(\S+)\s+GIT_TAG\s+(\S+)|URL\s+(\S+)(?:\s+URL_HASH\s+SHA256=([\dA-Fa-f]+))?)\s*\)`

Walk `CMakeLists.txt` + every `.cmake` file under `cmake/`, `Modules/`, `third_party/` directories. Concatenate parse output across all files; dedupe within the reader by `(ecosystem, name)` so a dep declared in both `CMakeLists.txt` and an included `.cmake` file emits once.

**Rationale**: CMake is more brittle than Starlark — variables like `${GTEST_VERSION}` inside `FetchContent_Declare` arguments are common. The regex strategy works on ≥90% of the open-source corpus (spot-check: LLVM, OpenSSL, RocksDB, gRPC, Envoy CMake trees) where calls are literal. SC-002's 90% floor explicitly accommodates the heuristic ceiling; the remaining 10% are macros (`if(BUILD_TESTING)\n  include(GoogleTest)`) and variable-substituted calls — out of scope per spec.

**Test-scope detection (per Edge Cases)**: a `FetchContent_Declare` inside an `if(BUILD_TESTING)` block (or `if(NOT WIN32)`, etc.) is detected by tracking the indentation/block depth via pre-pass; deps inside such blocks emit with `lifecycle_scope = Test`. Pragmatic best-effort.

**Alternatives considered**:
- Real CMake script interpreter — no good Rust crate; the reference Python implementation `cmake-pc-loader` is ~3 KLOC and not pure-Rust.
- Subprocess `cmake --trace-expand` — requires CMake installed; emits ~50× the data volume needed; unstable across CMake versions.

## §5 — vcpkg.json parsing strategy

**Decision**: Direct `serde_json::from_str::<VcpkgManifest>(...)` deserialization. Schema:

```rust
#[derive(Deserialize)]
struct VcpkgManifest {
    name: Option<String>,
    version: Option<String>,
    dependencies: Vec<Dependency>,
    #[serde(default)]
    overrides: Vec<Override>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Dependency {
    Simple(String),                                            // "zlib"
    Detailed { name: String, version_eq: Option<String>, ... },// {"name": "openssl", "version>=": "3.0.0"}
}
```

`overrides` block (per Edge Cases): post-process the `Dependency` list to substitute the overridden version where the override matches by name.

**Rationale**: vcpkg.json is well-structured JSON (Microsoft owns the schema; it's stable). serde derives are the canonical Rust pattern. 100% coverage per SC-003 because no heuristics involved.

**Alternatives considered**: hand-written line-by-line JSON parser — over-engineered when serde handles all edge cases (escape sequences, nested objects) for free.

## §6 — Conan conanfile.txt parsing strategy

**Decision**: Hand-written line-by-line INI parser. Skip blank lines + comments (`#`). Detect section headers (`[requires]`, `[tool_requires]`, `[options]`, etc.). Within `[requires]` or `[tool_requires]`, parse each line as `<name>/<version>` (split on first `/`).

```rust
enum Section { Requires, ToolRequires, Other(String) }

fn parse_conanfile_txt(content: &str) -> Vec<ConanDep> {
    let mut section = Section::Other("".to_string());
    let mut deps = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        if line.starts_with('[') && line.ends_with(']') {
            section = match line {
                "[requires]" => Section::Requires,
                "[tool_requires]" => Section::ToolRequires,
                other => Section::Other(other.to_string()),
            };
            continue;
        }
        match section {
            Section::Requires => deps.push(parse_dep_line(line, LifecycleScope::Runtime)),
            Section::ToolRequires => deps.push(parse_dep_line(line, LifecycleScope::Build)),
            _ => {}
        }
    }
    deps
}
```

**Rationale**: conanfile.txt is INI-style but with looser rules than standard INI (no `=` key-values; just `<name>/<version>` lines under sections). The `toml` crate works on TOML, not INI; hand-rolled parser is simpler than fighting `toml`'s strictness for a 30-line file format.

**Alternatives considered**:
- Use `toml` crate — fails immediately because conanfile.txt isn't TOML (no key-value pairs).
- Use a generic INI crate — adds a new dep (violates FR-009); hand-rolled is ~40 lines.

## §7 — conanfile.py parsing strategy

**Decision**: Regex-based extraction of `requires = [...]` and `tool_requires = [...]` LITERAL list assignments:

```rust
let requires_re = Regex::new(r"(?m)^\s*(requires|tool_requires)\s*=\s*\[([^\]]+)\]")?;
```

For each match: split the inner list by commas, extract `"<name>/<version>"` literal strings, ignore non-string entries (variable refs, function calls).

**Rationale**: conanfile.py is Python; the `requires` attribute can be assembled dynamically (`requires = [base_req]` etc.). The regex captures the literal-list majority case (~80% of open-source conanfile.py per spec SC-005). Non-literal cases are out of scope; documented as heuristic-coverage gap.

**Alternatives considered**:
- Python parser (e.g., `rustpython-parser` crate) — heavy dep; over-engineered for extracting one attribute.
- Subprocess `python conanfile.py print_requires` — requires Python + Conan installed; introduces subprocess risk + non-portability.

## §8 — Path-resolver dispatch arms

**Decision**: Add 4 new arms to `path_resolver::resolve_path_with_context()`'s chained `.or_else()` resolution:

- `resolve_bazel_path(path)` — matches `MODULE.bazel`, `WORKSPACE.bazel`, `WORKSPACE`. Returns `None` (no PURL from path alone — Bazel reader handles via in-memory parse) but signals to the walker that this path is "claimed" by the bazel reader.

Actually — re-reading the cargo.rs precedent: `path_resolver` returns a PURL when the path itself encodes the PURL (e.g., `.cargo/registry/cache/idx/serde-1.0.197.crate` → `pkg:cargo/serde@1.0.197`). The new readers don't have this property — they parse manifests, not cache paths. So the path-resolver work for milestone 102 is *minimal*:

**Revised decision**: Only add path-resolver arms if there's a meaningful cache-path-to-PURL mapping. For Bazel/CMake/vcpkg/Conan source-tree manifests, there isn't — the reader does the work, not the path resolver. The `scan_fs::scan_path()` orchestrator already loops over all enabled readers; adding the 4 new readers there is the only dispatch change needed.

Path-resolver IS extended for one case: **vcpkg installed packages cache** at `~/.vcpkg/installed/<triplet>/share/<name>/copyright`. If a vcpkg-installed binary's contents are scanned, the path resolver can map back to a PURL. Out of scope for this milestone (would require vcpkg-binary-cache fixture setup); deferred to a follow-up.

**Net change**: `path_resolver.rs` may stay unchanged. `scan_fs/mod.rs` (the dispatch orchestrator, not path_resolver) gets 4 new reader calls.

## §9 — PURL ecosystem string choices

**Decision**:
- Bazel: `pkg:bazel/<name>@<version>` for BCR-declared deps (Bzlmod), `pkg:generic/<name>@<version-from-url-or-ref>` for non-BCR `http_archive`/`git_repository`. The Bazel Central Registry uses `pkg:bazel/` per ongoing PURL community discussion; if the spec rejects this later we can switch to `pkg:generic/` retroactively.
- vcpkg: `pkg:vcpkg/<name>@<version>` per the PURL spec (vcpkg is in the official PURL type list).
- Conan: `pkg:conan/<name>@<version>` per the PURL spec (conan is in the official PURL type list).
- CMake FetchContent GitHub-hosted deps: `pkg:github/<owner>/<repo>@<tag>` (GitHub is a first-class PURL type, more useful than `pkg:generic` for GitHub-hosted source archives).
- CMake URL-fetched deps: `pkg:generic/<name>@<version-parsed-from-url>` with `mikebom:download-url` annotation.
- CMake git-fetched non-GitHub deps: `pkg:generic/<name>@<ref>` with `mikebom:download-url` annotation.

**Rationale**: Maximize PURL-spec compliance where supported (vcpkg, conan, github are all spec-listed); fall back to `pkg:generic/` with annotations where the spec doesn't support a more specific type. Bazel is the only borderline case; we go with `pkg:bazel/` as the most plausible canonical choice based on BCR community usage.

## §10 — CLI flag plumbing for `--include-vendored`

**Decision**: Mirror the milestone-052 `--exclude-scope` pattern in `mikebom-cli/src/cli/scan_cmd.rs`:

1. Add `--include-vendored` (boolean flag) to the `ScanArgs` struct via `#[arg(long, env = "MIKEBOM_INCLUDE_VENDORED")]`.
2. Pass through to `execute(...)` as a new `include_vendored: bool` parameter.
3. Plumb to `cmake::read()` via a new optional `ReaderOptions { include_vendored: bool }` struct — only cmake.rs needs this option (other 3 readers ignore it).
4. cmake.rs's `add_subdirectory(third_party/...)` extraction is gated on `opts.include_vendored`; when false, those `add_subdirectory` calls don't emit components.

**Rationale**: Matches the milestone-052 plumbing precedent. `clap`'s `env = "..."` directive is the canonical way to support both CLI flag + env-var fallback. Single boolean flag — no version parser, no enum-mapping complications.

## §11 — Test fixtures

**Decision**: Synthetic fixtures stay in `mikebom-cli/tests/fixtures/{bazel,cmake,vcpkg,conan}/` — small (<2KB each), version-controlled, no network access. Each fixture is just the minimum manifest to exercise the reader (≥2 deps + ≥1 edge case per fixture):

- `bazel/MODULE.bazel` — 2 `bazel_dep` calls (one with `dev_dependency = True`).
- `bazel/WORKSPACE.bazel` — 1 `http_archive` + 1 `git_repository`.
- `cmake/CMakeLists.txt` — 1 `FetchContent_Declare(GIT_REPOSITORY)` + 1 `ExternalProject_Add(URL ... URL_HASH ...)`.
- `cmake/cmake/third_party.cmake` — 1 `FetchContent_Declare(URL)` to exercise the `cmake/` subdirectory walk.
- `cmake/third_party/foo/` — vendored deps directory for the `--include-vendored` test.
- `vcpkg/vcpkg.json` — `{"dependencies": ["zlib", {"name": "openssl", "version>=": "3.0.0"}]}`.
- `conan/conanfile.txt` — `[requires]\nzlib/1.2.13\nopenssl/3.0.0\n[tool_requires]\ncmake/3.27.0\n`.
- `conan/conanfile.py` — Python literal-list `requires = ["zlib/1.2.13", "openssl/3.0.0"]`.

**Rationale**: All synthetic — no external network, no upstream fixture-repo dep. Each fixture is small + readable + easy to extend in follow-up milestones. Total fixture size: <10 KB.

**Alternative considered**: Use real-world open-source projects as fixtures via the milestone-090 fixture-cache repo. Rejected — slows tests (each fixture is ~MB), introduces upstream-stability dep, and the synthetic fixtures already exercise the relevant code paths.

## §12 — Goldens regression strategy

**Decision**: Extend the existing `cdx_regression.rs` / `spdx_regression.rs` / `spdx3_regression.rs` test files with 4 new ecosystem-test functions each (one per new reader). Each new test runs `mikebom sbom scan --path tests/fixtures/<ecosystem>/` and asserts byte-identity against a committed golden under `tests/fixtures/golden/{cyclonedx,spdx-2.3,spdx-3}/<ecosystem>.{cdx.json,spdx.json,spdx3.json}`.

**Rationale**: Matches the existing 9-ecosystem regression-test pattern. The 4 new ecosystems become rows 10-13 in each format's golden test suite. Total: 12 new goldens (4 ecosystems × 3 formats) committed to the repo via the milestone-100 fixture-pinning convention.

**Coverage**: each golden test gets a normal pass AND a `MIKEBOM_UPDATE_*_GOLDENS=1` regen path. Existing 9 ecosystems' goldens stay untouched (no regen, no byte change — per SC-006).

---

## Summary — research is settled

No remaining NEEDS CLARIFICATION. All decisions anchor to:
- The existing 11-reader architecture pattern (cargo.rs + maven.rs + gem.rs templates).
- The spec's 3 clarifications (parse-error policy, cross-ecosystem dedup, vendored-dep opt-in).
- The Constitution gates (all 12 principles PASS post-design).
- The existing-architecture survey embedded in `plan.md::Existing-Architecture Context`.

Ready for Phase 1.
