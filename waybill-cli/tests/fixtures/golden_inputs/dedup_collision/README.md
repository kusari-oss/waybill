# `dedup_collision` fixture (milestone 105 T024)

Synthetic two-reader collision fixture. Used by `tests/dedup_precedence_determinism.rs` to verify the FR-015 / SC-010 dedup determinism guarantee end-to-end.

## Shape

The same library (`abseil`) is declared by two distinct manifest sources:

- **`conanfile.txt`** — declares `abseil/20240722.0` under `[requires]`. The conan reader (active since alpha.41) emits `pkg:conan/abseil@20240722.0` with `waybill:source-mechanism: "conan-recipe"`.
- **`.gitmodules`** — declares `third_party/abseil` pointing at `https://github.com/abseil/abseil-cpp.git`. The git-submodule reader (introduced by milestone 105 US6) will emit `pkg:github/abseil/abseil-cpp@<sha>` with `waybill:source-mechanism: "git-submodule"` and a `waybill:build-reference` annotation.

## Expected dedup behavior (post-US6)

When both readers fire, they produce **different canonical PURLs** (`pkg:conan/abseil@...` vs `pkg:github/abseil/abseil-cpp@...`) so technically they wouldn't dedup against each other. But the **post-US6 dedup pipeline** is expected to canonicalize these to the same logical component during normalization (US6 implementation detail). At that point, FR-015 kicks in:

- Manifest-mode tier (Conan, vcpkg, etc.) outranks filesystem-derived tier (git-submodule).
- The Conan recipe wins; the git-submodule signal goes into `waybill:also-detected-via: ["git-submodule"]`.

## Files

```
dedup_collision/
├── README.md                                 # this file
├── CMakeLists.txt                            # `find_package(Abseil)` for build-reference correlation
├── conanfile.txt                             # `[requires]\nabseil/20240722.0`
├── .gitmodules                               # `[submodule "third_party/abseil"]` block
├── third_party/abseil/                       # submodule placeholder (empty for fixture)
└── .git/modules/abseil/HEAD                  # fake SHA file used by the git-submodule reader's HEAD resolver
```

## Current behavior (pre-US6)

With only the conan reader wired, scanning this fixture emits exactly one component (`pkg:conan/abseil@20240722.0`). The git-submodule files are inert until milestone 105's US6 phase lands the reader.

The `dedup_precedence_determinism.rs` integration test has two functions:

1. **`dedup_collision_scans_deterministically_today`** — active. Scans this fixture 10 times, asserts byte-identical SBOM output across all runs. Catches HashMap-iteration-order bugs, time-dependent fields, and other non-determinism in the *current* readers.

2. **`dedup_collision_emits_also_detected_via_after_us6`** — `#[ignore]`-gated. The full SC-010 cornerstone test: after US6 lands the git-submodule reader, this fixture produces a real cross-reader collision; the test then asserts the `waybill:also-detected-via` annotation is emitted deterministically. Remove the ignore gate when US6 wiring completes.
