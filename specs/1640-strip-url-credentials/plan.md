# Implementation Plan: Strip credentials from declared URLs (#1647)

**Branch**: `1640-strip-url-credentials` | **Date**: 2026-11-21 | **Spec**: [spec.md](spec.md)

## Summary

waybill copies URLs that projects declare as fetch locations into the SBOM verbatim, credentials included. Only three sites sanitise them today.

**The approach** (research R1–R6):
- **An inventory** (R1) of every emitted field that can carry a declared URL. It has 20 rows to sanitise (D1–D20), 3 already sanitised (S1–S3) and 6 that are not declared URLs (N1–N6). It is re-run at implement time.
- **One reader-facing helper** (R4), `sanitize_declared_url(raw, manifest)`. It removes userinfo and emits milestone 105 FR-016's warning. Each reader calls it where it first reads the URL from the manifest (R2), so PURLs, repository identity, annotations, external references and logs all see the clean form.
- **The helper keeps every other byte** (R3, measured). Today's `sanitize_userinfo` re-serialises a credentialled URL, which lowercases an `https` host, drops a default port and resolves dot segments, so a credentialled and a clean spelling would differ. It now deletes the userinfo span from the original string, and falls back to re-serialising if the span cannot be found.
- **Documentation:** the C53 row and the consumer guide say what is removed (userinfo, including a bare username) and what is not (the query string, #1652).

## Technical Context

**Language/Version**: Rust stable, pinned by `rust-toolchain.toml` (1.98.1). No nightly; `waybill-ebpf` untouched. No `+toolchain` on the gate (#1189).

**Primary Dependencies**: existing only.
- `url` 2.5.8, already a direct dependency used by the helper.
- Code:
  - `identifiers/sanitize.rs` (the helper);
  - the readers in research R1: `package_db/{cmake.rs, cmake/fetches.rs, bazel.rs, pip/uv_lock.rs, pip/requirements_txt.rs, nix/mod.rs, nix/identity.rs, nix/closure/emit.rs, nix/haskell_packages/closure_identity.rs, pants/lockfile.rs, pants_jvm/lockfile.rs, cocoapods.rs, dart.rs, composer.rs, elixir.rs, erlang.rs, yocto/recipe.rs, helm.rs}` and `binary/entry.rs`.

**Zero new Cargo dependencies** (FR-010).

**Storage**: N/A.

**Testing**:
- **Unit**, in `identifiers/sanitize.rs`:
  - splice cases: host case, default port, dot segments, a space, `git+https`, `git://…;params`, `ssh://git@`, query and fragment kept, a token as the username, `%40` inside the userinfo;
  - the fall-back path;
  - `sanitize_declared_url` emitting one event, which contains no credential.
- **Integration:** `waybill-cli/tests/declared_url_credentials.rs`, one case per D-row (contracts/wire.md):
  - each scans in three formats and captures the log;
  - it asserts that `wbfxuser` and `wbfxtoken7c41` occur 0 times in all four, that the field is present with the clean URL, and that the redacted warning appears;
  - it runs parity, CycloneDX schema validation and the unique-`bom-ref` check, and 10 repeat scans.

  Identity cases I-1 to I-3 cover User Story 2. Guard cases G-1 to G-4 cover N5. Reuses `common::{cmake_fetch, git_fixture, cdx_schema}`.
- **Control before the fix** (R5): the D-cases land before any reader change, when D-row output still equals `main`'s. Every D-case's credential assertion must fail there, and the failures are recorded.
- **Mutations:** delete each `sanitize_declared_url` call, one at a time (SC-005). Restore with `cp`, rebuild after the last one, and use a `^error\[E…\]` guard so a compile failure is not counted as a kill.
- **CI:**
  - the public corpus (`regen_goldens`): 67 files byte-identical expected;
  - TheRock measure-source against `main`;
  - the quality corpus against `main`.

  Every difference is traced (SC-004).

**Target Platform**: the waybill CLI on Linux, macOS and Windows.

**Project Type**: the existing Rust workspace (`waybill-cli`).

**Performance Goals**: none in the spec. The helper parses each declared URL once. TheRock's measure-source timing is recorded, not gated.

**Constraints**:
- FR-006: byte-identical output to `main` when no declared URL carries userinfo;
- FR-004: identity must not depend on whether userinfo was present;
- no log line may contain a removed credential.

**Scale/Scope**: about 25 call sites in 18 reader files, plus the helper.

## Constitution Check

| Principle | Assessment |
|---|---|
| I. Pure Rust, statically linked | Pass. No new crate; `url` is already direct. |
| III. Fail closed | N/A to scan-mode sanitising. A URL that cannot be parsed passes through unchanged and is not guessed at (Principle X's scan-mode rule). |
| IV. Type-driven correctness | Pass. No `.unwrap()` in production. URLs stay `&str`/`Cow<str>`: URLs are not among the listed domain values, and a newtype was weighed and rejected (R2). |
| V. Specification compliance | Pass. No field changes shape. A sanitised URL is still a valid URI for CycloneDX `externalReferences[].url`, SPDX `downloadLocation` and PURL qualifiers. |
| VII. Test isolation | Pass. Synthetic `waybill-fixture-*` fixtures, no network, credential strings unique in the repository. |
| VIII/IX. Completeness, accuracy | Pass. No component is added or removed. The location stays correct without its userinfo. |
| X. Transparency | Pass. The removal is reported to the operator by the FR-016 warning. The SBOM does not record that a credential was found, which would itself disclose something about the project. FR-009 documents the rule for consumers. |
| Strict boundaries | Unaffected. |

**Gate: pass.** Re-checked after design: unchanged.

## Project Structure

### Documentation (this feature)

```text
specs/1640-strip-url-credentials/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/wire.md
├── measurements/
│   └── url-probe/   (probe.rs, run.sh, output.txt)
├── checklists/requirements.md
└── tasks.md         (/speckit-tasks)
```

### Source Code (repository root)

```text
waybill-cli/src/identifiers/sanitize.rs        # splice-based sanitize_userinfo; sanitize_declared_url
waybill-cli/src/scan_fs/package_db/...         # the D-rows' readers (research R1)
waybill-cli/src/scan_fs/binary/entry.rs        # D18
waybill-cli/tests/declared_url_credentials.rs  # D-, I-, G-cases
docs/reference/sbom-format-mapping.md          # C53 row (FR-009)
docs/reference/reading-a-mikebom-sbom.md       # consumer guide (FR-009)
CHANGELOG.md
```

**Structure Decision**: the existing workspace. One helper in the library's `identifiers` module, which readers already import for `sanitize_userinfo`. One new integration file.

## Complexity Tracking

None.
