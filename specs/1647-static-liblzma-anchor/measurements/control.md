# Controls (T001, T007)

## T001: `main`'s binary

- **Commit**: `b6ad3c26bb57b1c7f4aeeda6a9d13d75242fe95e`. `git diff b6ad3c26 -- waybill-cli waybill-common Cargo.toml Cargo.lock` was empty when it was built: the branch had changed only `specs/` and `CLAUDE.md`.
- **Build**: debug, saved in the session scratchpad as `waybill-main-b6ad3c26` (`waybill 0.17.0`).

## T007: S1–S8 on the log-only commit

From `cargo test -p waybill --test version_string_static -- --include-ignored`, run on the log-only working tree:

| Case | Result | Where a red case fails |
|---|---|---|
| S1 | red, as planned | line 245, the component: `left: []`, `right: ["pkg:generic/liblzma@5.4.5"]`. Its census check, which comes first, passed: `rule_a` liblzma `versioned` `5.4.5`, a `symtab` anchor, `elf_type` `exec` |
| S2 | red, as planned | line 256, the same, for `lzma_version_number` |
| S3 | pass | |
| S4 | pass | |
| S5 | red, as planned | line 286, "gmp kept a shared version". Its census check passed first: `rule_a` has gmp and liblzma `ambiguous-shared` |
| S6 | pass | |
| S7 | pass | |
| S8 | pass | `rule_b` liblzma `versioned` (log-only) |

The red cases were then marked `#[ignore = "spec 1647: red until T011"]`.

## T007: byte identity against `main`

`control_byte_identical_to_main`, with `WAYBILL_VERSION_STATIC_MAIN_BIN` set to T001's binary and `WAYBILL_VERSION_STATIC_IDENTICAL=S1,S2,S3,S4,S5,S6,S7,S8`: **passes**. All three documents of all eight cases are identical to `main`'s, so the log-only commit changes no output.

**The mask, and why it is needed.** The first run failed on S1's CycloneDX document. The only difference was `serialNumber`. The serial is `Uuid::new_v4()` (`generate/cyclonedx/builder.rs:854`), so it is new on every run. Comparing this binary with itself failed the same way, which ruled out the branch. The control now masks every version-4 UUID before comparing.

The mask replaces only strings of that exact form, so any other difference still fails the comparison. At the wired commit, S1, S2 and S5 are expected to differ, and the default case list leaves them out.

## Where the plan's tests landed (as built)

- **T005's ".symtab read" checks are integration tests, not unit tests in `scan.rs`.** The ELF builder that writes a `.symtab` (`tests/common/installed_lib.rs::elf_with_symtab`) is only reachable from integration tests. The four claims are covered by these cases:
  - defined symbols are collected: S1, S2;
  - undefined symbols are not: S3;
  - a `.strtab` without the needle is skipped: S7;
  - a file with the symbol in both tables: S6.
- **T004's and T006's unit tests are in `version_strings.rs`**, `s1647_*`, five tests. They live in the binary target, as all of `waybill-cli`'s unit tests do, so `cargo test -p waybill --bins s1647` runs them, not `--lib`.
