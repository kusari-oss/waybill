# Quickstart — milestone 097 maintainer recipes

Four recipes for landing CPE candidate emission for binary-identified components.

## Recipe 1 — Add the mapping table + ecosystem arm (FR-001, FR-002)

Open `mikebom-cli/src/generate/cpe.rs`. Above the `synthesize_cpes` function, add the const table from `data-model.md §cpe.rs — extension shape`:

```rust
const GENERIC_LIBRARY_CPES: &[(&str, &[(&str, &str)])] = &[
    ("openssl",  &[("openssl",  "openssl")]),
    ("zlib",     &[("zlib",     "zlib")]),
    ("sqlite",   &[("sqlite",   "sqlite")]),
    ("curl",     &[("haxx",     "curl"), ("curl", "curl")]),
    ("pcre",     &[("pcre",     "pcre")]),
    ("pcre2",    &[("pcre",     "pcre2")]),
    ("gnutls",   &[("gnu",      "gnutls")]),
    ("libressl", &[("openbsd",  "libressl"), ("libressl", "libressl")]),
    ("llvm",     &[("llvm",     "llvm")]),
    ("openjdk",  &[("oracle",   "openjdk")]),
];
```

In the `match ecosystem` block (line ~30 of `synthesize_cpes`), add the `"generic"` arm **before** the existing `_ => { return Vec::new(); }` catch-all:

```rust
"generic" => {
    let mapping = GENERIC_LIBRARY_CPES
        .iter()
        .find(|(slug, _)| *slug == name.as_str())
        .map(|(_, vendors)| *vendors);
    let Some(vendors) = mapping else {
        return Vec::new();
    };
    let cpe_version = if name == "openjdk" {
        version
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .next()
            .unwrap_or(version)
    } else {
        version.as_str()
    };
    return vendors
        .iter()
        .map(|(vendor, product)| format_cpe(vendor, product, cpe_version))
        .collect();
},
```

Compile-check: `cargo +stable check -p mikebom`.

## Recipe 2 — Add unit tests (Contract 1-5)

Add the 5 new tests to the existing `tests` module per `data-model.md §cpe.rs`:
- `generic_openssl_emits_canonical_cpe`
- `generic_curl_emits_dual_candidates`
- `generic_openjdk_strips_build_suffix`
- `generic_symbol_fingerprint_only_emits_no_cpe`
- Rename `unknown_ecosystem_returns_empty` → `generic_unknown_library_returns_empty`

Plus the two well-formedness tests:
- `mappings_alphabetically_sorted`
- `mappings_cover_all_curated_libraries`

Run via:

```bash
cargo +stable test -p mikebom --bin mikebom \
    --no-fail-fast cpe:: 2>&1 | grep "test result:"
# Expected: ok. <N>+5 passed (existing + 5 new).
```

## Recipe 3 — Add the integration test (SC-001 negative control)

Create `mikebom-cli/tests/cpe_binary_id.rs` per `data-model.md §cpe_binary_id.rs`. The test:
1. Copies the mikebom binary itself into a temp dir
2. Runs `sbom scan --path <tempdir> --output <file> --no-deep-hash`
3. Asserts the emitted CDX contains NO `cpe:2.3:a:openssl:openssl:` substring

Mikebom uses `rustls` (not OpenSSL), so the assertion is a negative-control regression guard: if the milestone-096 binary scanner or the milestone-097 CPE table ever fires spuriously on mikebom's own bytes, this test catches it.

Run via:

```bash
cargo +stable test -p mikebom --test cpe_binary_id 2>&1 | tail -5
# Expected: ok. 1 passed.
```

## Recipe 4 — Run pre-PR gate + verify diff scope (Contract 6, Contract 7)

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# Expected: `>>> all pre-PR checks passed.`

# Diff scope (Contract 6):
git diff --name-only main | sort
# Expected (allowlist):
#   mikebom-cli/src/generate/cpe.rs
#   mikebom-cli/tests/cpe_binary_id.rs
#   specs/097-cpe-candidates/...
#   CLAUDE.md (auto-updated by /speckit-plan)

# No Cargo.* changes (FR-007):
git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' && echo "DEP CHURN" || echo "clean"
# Expected: clean

# Goldens regen scope (FR-009 / SC-006):
git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
# Expected: empty output (no goldens regenerated).
```

## When in doubt

- **A new library was added to milestone-098's version-string scanner but `mappings_cover_all_curated_libraries` test fails**: add a row to `GENERIC_LIBRARY_CPES` (1 line). If the library has no NVD-tracked CPE, add it to the documented-omission set in the test and document the omission rationale in a `//` comment above the table.
- **A library's NVD vendor was renamed**: update the row's vendor string in-place. If the old vendor is still cited by older NVD records, demote it to a secondary candidate (add a second `(vendor, product)` pair after the primary).
- **An OpenJDK-style version-suffix issue emerges for a new library**: extend the per-library special-case logic. Keep it inline in the `"generic"` arm rather than abstracting until a third such case appears.
- **A binary triggers a spurious version-string match and the spurious component gets a CPE**: fix the milestone-096 version-string anchor (tighten the regex), not the CPE table. The CPE emission is downstream of identification; identification is the right layer to filter.
- **An operator reports a missing CPE for an actual library that mikebom does identify**: add the row. v1 starter set is small by design; growth is one PR per library.
