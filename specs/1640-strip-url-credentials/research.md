# Research: Strip credentials from declared URLs

All file references are to `waybill-cli/src/` at `main` `86bce89` unless stated. "Traced" means read in code. "Measured" means run.

## R1. The inventory (FR-001)

**Method.** Four searches over `waybill-cli/src` and `waybill-common/src`, none piped through `head`:
1. PURL qualifiers whose value is a URL: format strings and pushes containing `vcs_url=`, `download_url=` or `repository_url=`, and `from_parts` qualifier lists.
2. External references: every `ExternalReference {` construction outside tests.
3. Struct fields named like a URL (`url`, `repository`, `repo`, `location`, `homepage`).
4. Annotation keys: all 208 distinct `"waybill:…"` literals under `scan_fs`, `resolve`, `enrich` and `binding`, filtered for URL-like names, plus constants declared as `const …: &str = "waybill:…"`.

Each hit was then read to find where its value comes from.

**Re-verify at implement time** (T001). A mis-scoped search returns nothing silently, so the implementer reruns the four searches and diffs the result against this table.

### Rows to sanitise: a declared URL reaches the output

| # | Reader | Field | Where (traced) |
|---|---|---|---|
| D1 | CMake fetch | C53 `waybill:download-url`, from `GIT_REPOSITORY`/`URL`. It also feeds spec 1538 R9's repository identity (`cmake/fetches.rs::repository_identity`) | `package_db/cmake.rs::build_cmake_entry` |
| D2 | CMake dataset download | `download_url=` PURL qualifier and C53 | `package_db/cmake.rs::dataset_entry` |
| D3 | CMake GitHub recognition | the `pkg:github` PURL derived from a `GIT_REPOSITORY` URL | wherever the fetch URL is parsed into owner/repo. T001 locates it; it must see the clean URL |
| D4 | Bazel | C53 | `package_db/bazel.rs` (`http_archive` URL) |
| D5 | uv.lock | `waybill:pypi-source-url` (index URL) | `package_db/pip/uv_lock.rs` (~734) |
| D6 | uv.lock | `waybill:source-url` (git, url and path sources), three writes | `package_db/pip/uv_lock.rs` (~773–787) |
| D7 | Nix flake.lock | `waybill:source-url`, from the lock node's `url`/`repo` | `package_db/nix/mod.rs` (~149), `nix/identity.rs` |
| D8 | Pants pex lockfile | `waybill:source-url` | `package_db/pants/lockfile.rs` |
| D9 | Pants JVM (coursier) lockfile | `waybill:source-url` | `package_db/pants_jvm/lockfile.rs` |
| D10 | Nix closure | `vcs` and `distribution` external references from a derivation's source | `package_db/nix/closure/emit.rs::source_references`, `VcsSource` |
| D11 | Nix Haskell closure identity | `vcs_url` qualifier from `location` | `package_db/nix/haskell_packages/closure_identity.rs` (~1004) |
| D12 | CocoaPods | `vcs_url` qualifier | `package_db/cocoapods.rs` (~946) |
| D13 | Dart | `vcs_url` (git) and `repository_url` (self-hosted) qualifiers | `package_db/dart.rs` (~683, ~710) |
| D14 | Composer | `vcs_url` and `repository_url` qualifiers | `package_db/composer.rs` (~934, ~946) |
| D15 | Elixir | `vcs_url` qualifier, two sites | `package_db/elixir.rs` (~968, ~1254) |
| D16 | Erlang | `vcs_url` qualifier, two sites | `package_db/erlang.rs` (~1569, ~1750) |
| D17 | pip requirements | `waybill:direct-url-source`'s `url` | `package_db/pip/requirements_txt.rs` (~553) |
| D18 | cargo-auditable | `waybill:cargo-vcs-source-url`, then the `vcs` external reference | `binary/entry.rs` (~416), `scan_fs/mod.rs::external_refs_from_purl` |
| D19 | Yocto | `waybill:src-uri` array | `package_db/yocto/recipe.rs` (~710) |
| D20 | Helm | the PURL namespace is the repository URL's host, which carries no userinfo. The parse-failure branch uses the raw string as the namespace, and a `warn!` logs the raw URL | `package_db/helm.rs::build_helm_purl` |

### Already sanitised (unchanged, except through R3)

| # | Reader | Field |
|---|---|---|
| S1 | git submodule | `vcs_url` qualifier (`git_submodule/identity.rs`) |
| S2 | source identifiers | document identifiers (`binding/identifiers/auto_detect.rs`) |
| S3 | uv.lock | `waybill:pypi-artifact-url`, which becomes the `distribution` reference |

### Not a declared URL

| # | Field | Reason |
|---|---|---|
| N1 | `waybill:source-files-nested-url` (Maven) | a nested-JAR path (`<outer>!<inner>`) inside the scanned tree, not a fetch location |
| N2 | deps.dev external references (`enrich/depsdev_source.rs`) | fetched from the enrichment service, not declared by the project |
| N3 | external references built from coordinates (`external_refs_from_purl`: Go hosts, crates.io, NuGet, PyPI sdist, Maven Central) | composed from PURL fields, which have no userinfo |
| N4 | `repository_url=https://repo.hex.pm` (Elixir, Erlang private org) | a constant |
| N5 | Gemfile.lock `remote:`, Cargo.lock `registry+…`, pip `--index-url`, npm `resolved` | read for classification and never emitted. A credentialled guard fixture shows each stays absent (contracts, G-cases) |
| N6 | `waybill:nix-original-ref` | a branch or tag name, not a URL |

## R2. Where to sanitise: in the reader, at the point the URL is read

- **Decision:** each reader passes a declared URL through one helper (R3) at the point where it first takes the string from the manifest. Every later use (PURL, repository identity, annotation, external reference, log line) then sees the clean form.
- **Rationale:** a PURL qualifier is identity, and spec 1538 R9 derives repository identity from the C53 value. Sanitising when the document is written would come after deduplication had already compared the credentialled spelling (User Story 2). It would also leave logs written in between unsanitised (FR-005).
- **Alternative rejected: a single pass at emission.** It would be too late for identity, and it would have to rewrite PURLs and `bom-ref`s after relationships were built.
- **Alternative rejected: a `DeclaredUrl` newtype through every reader.** It would enforce the rule by type (Principle IV lists PURLs, hashes, licences and component identifiers, not URLs), but it touches every reader's data structures for no behavioural gain. Mutations and the fixture per row (SC-001, SC-005) enforce coverage instead.

## R3. The helper must keep every byte but the userinfo (measured)

- **Finding** (`measurements/url-probe/`, run with `sh run.sh` against `url` 2.5.8, the version in `Cargo.lock`): today's `sanitize_userinfo` re-serialises the URL when it strips userinfo. For special schemes (`http`, `https`) that also:
  - lowercases the host (`GitHub.com` → `github.com`);
  - drops a default port (`:443`);
  - resolves dot segments (`/a/../org` → `/org`);
  - percent-encodes spaces.

  A clean URL is returned untouched (`Cow::Borrowed`). So a credentialled and a clean spelling of one URL can differ after sanitising, which breaks FR-003 and FR-004. Non-special schemes (`git+https`, `git`, `ssh`) kept their host case in the probe.
- **Decision:** `sanitize_userinfo` keeps detecting credentials with the `url` crate (a non-empty username or a present password). It removes them by deleting the userinfo span from the original string:
  - the authority runs from after `://` to the first `/`, `?` or `#`;
  - within it, the span runs up to and including the last `@`.

  The result is parsed again. If it still has a username or password, or the span cannot be found, the helper falls back to today's re-serialised form. It never returns a string that still holds the credential.
- **Effect on existing callers:** S1–S3 change only for URLs that carry userinfo. There are none in the committed public corpus (measured, spec Background), and only the normalisation of the remainder changes. Their unit tests gain cases for host case, default port and dot segments.
- **Alternative rejected: keep re-serialising and normalise the clean spelling too.** That changes output for every clean URL, breaking FR-006, and the normalisation would apply to every URL in every reader.

## R4. The warning (FR-005)

- **Decision:** a reader-facing function, `identifiers::sanitize::sanitize_declared_url(raw, manifest)`:
  - calls `sanitize_userinfo`;
  - when it returns `Cow::Owned`, emits milestone 105 FR-016's event: `warn!(target: "waybill::identifiers::sanitize", manifest_file = <manifest>, url_redacted = <redact_userinfo_for_log(raw)>, "stripped credentials from URL")`.

  The existing `uv_lock` warning moves to this function, so one event shape covers every site.
- **Log lines before sanitising:** Helm's no-host `warn!` logs `%url` raw. With R2 the URL is sanitised before it is logged. T001 also searches readers for `tracing::` calls that format a URL variable before the sanitise point.

## R5. Fixtures and the positive control (SC-001, SC-002)

- **Strings:** every fixture uses the user `wbfxuser` and the token `wbfxtoken7c41`. Neither occurs anywhere in the repository (T001 checks this), so a byte search of the output and the log is unambiguous.
- **Control on the branch:** each case also checks that its field is present with the clean URL. This proves the fixture reaches the field, so an absent token means stripped, not "never emitted".
- **Control before the fix:** the D-cases land after the helper (which no D-row reader calls yet) and before any reader change (T006). That commit's D-row output equals `main`'s. Each case's credential assertion is run and must fail there, and the failures are recorded per case. A case that passes on `main` has a fixture that does not reach its field.
- **Guards:** the N5 rows get G-cases that pass on `main` and on the branch.

## R6. `ssh://git@` (Clarification Q1)

The helper strips `git` like any username (probe: `ssh://git@Host.Example:22/org/repo.git` → `ssh://Host.Example:22/org/repo.git`). The committed public corpus has no URL with userinfo (measured). TheRock's and the quality corpus's `ssh://git@` URLs, if any, are counted by the CI comparison (T0xx) and each traced (SC-004).
