# T001: the inventory, re-derived at the branch

Branch HEAD `3eebf9cf` (source identical to `main` `86bce89`). The four searches of research R1 were run over `waybill-cli/src` and `waybill-common/src`, writing to files and counting with `wc -l`:

| Search | Command (from `waybill-cli/src`) | Hits |
|---|---|---|
| A, URL qualifiers | `grep -rnE 'vcs_url=\|download_url=\|repository_url=\|\("vcs_url"\|\("download_url"\|\("repository_url"' . ../../waybill-common/src` | 48 lines in 13 files |
| B, annotation keys | `grep -rhoE '"waybill:[a-z0-9-]+"' scan_fs resolve enrich binding \| sort -u`, then filtered for URL-like names | 208 keys, 15 URL-like |
| B', key constants | `grep -rnE 'const [A-Z_]+: &str = "waybill:' .` | 99 |
| C, external references | `grep -rn 'ExternalReference {' . ../../waybill-common/src` | 19 lines in 5 files |
| D, URL-named fields | `grep -rnE '^\s*pub(\(crate\))? [a-z_]*(url\|homepage\|repository\|repo\|remote\|location\|website)[a-z_]*: '` | 108 |

## Diff against research R1

No row is added or removed. The hits outside R1's D-, S- and N-rows are all one of:
- **tests:** `scan_fs/mod.rs` ~4065, `pants_resolve.rs` ~483, `binding/alias.rs` ~399, `waybill-common/src/types/purl.rs` ~224, `generate/spdx/mod.rs` ~1094, `waybill-common/src/resolution.rs` ~1069;
- **type definitions:** `ExternalReference` in `resolution.rs`;
- **operator input**, not a manifest: `--alias` in `binding/alias.rs`;
- **signing endpoints** (`fulcio_url`, `rekor_url`): CLI configuration, not emitted per component;
- **counts only:** `cmake_download::DownloadCoverage`, whose `url_template` and `Resolution::Literal` never leave the reader except through `dataset_entry` (D2).

## D3, located

`waybill-cli/src/scan_fs/package_db/cmake.rs::fetch_entry`:
- `GIT_REPOSITORY`, `GIT_TAG` and `URL` are read by `extract_keyword` at ~1155–1157;
- the GitHub regex `^https?://github\.com/([^/]+)/([^/\.\s]+)` (~1140) is applied by `github_pair` at ~1176 (unresolved tag) and ~1206 (resolved).

A credentialled `https://<token>@github.com/o/r.git` does not match the regex. It becomes `pkg:generic/<name>`, not `pkg:github/o/r`. That is I-2's defect, traced and not yet run. Sanitising `git_repo` and `url` right after `extract_keyword` fixes D1 and D3 together. The URL-form path's `parse_version_from_url` and `fetches.rs::name_from_archive` read only the filename, which userinfo cannot reach.

## Log sites (T016)

Every `tracing::` call in a D-row reader whose arguments mention a URL, a repository or a PURL was read:

| Site | Logs | Reachable with a credential? |
|---|---|---|
| `helm.rs` ~551 | `%url` when the repository URL has no host | No. Measured with the `url` crate 2.5.8: `https://u:p@/path`, `oci://u:p@/x`, `git+https://u:p@/x` and `foo://u@/x` all fail with "empty host", so a URL with userinfo always has a host and this branch never sees one. Helm needs no change (N7). |
| `elixir.rs` ~1292, `erlang.rs` ~1784 | the PURL string, which holds `vcs_url` | Only before T012. After T012 the PURL is built from the sanitised URL. |
| `pants/lockfile.rs` ~558, `pants_jvm/lockfile.rs` ~370 | the PURL string | No URL in it. |
| `nix/mod.rs` ~335, `nix/closure/emit.rs` ~148 | a PURL | Only before T010, for the closure's `vcs_url`. |

T016 therefore changes no file.

## Credential strings

`git grep 'wbfxuser\|wbfxtoken7c41' -- ':!specs/1640-strip-url-credentials'` counts 0 lines.

## Addendum: rows the four searches missed

The searches matched URL-like key names and qualifier strings. They missed fields that carry a URL inside free text or derive a PURL from a URL. Two later sources found five more rows:
- the agent's fixture survey;
- a sweep, measured.

### The sweep

One tree held a credentialled URL in common manifest forms:
- `package.json` URL dependencies;
- a Gemfile `git:` option;
- a Cargo.toml `git` dependency;
- pyproject direct URLs;
- a `pom.xml` repository and SCM URL;
- a `build.gradle` repository;
- a Pipfile source and git dependency;
- a `composer.json` vcs repository;
- `pubspec.yaml` git and hosted dependencies;
- a Podfile `:git`;
- `Package.resolved`;
- a `pnpm-lock.yaml` tarball;
- a `yarn.lock` `resolved`;
- `west.yml`;
- a Dockerfile `ADD`/`git clone`;
- requirements `-e git+…` and `--extra-index-url`.

It was scanned with the branch binary after T007–T013, at `RUST_LOG=trace`.

Before the new fixes, the token occurred in CycloneDX 10 times, SPDX 2.3 7, SPDX 3 8 and the log 0. Every occurrence was in one of three fields:

| Field | From |
|---|---|
| `waybill:requirement-ranges` | `package.json` URL dependencies, pyproject direct URLs, and requirements direct URLs (why D-17 still failed after T013) |
| `waybill:version-constraint` | pyproject direct URLs |
| the PURL, `bom-ref` and dependency refs | `Package.resolved` `location`: `pkg:swift/wbfxuser:wbfxtoken7c41@git.example/org/j@1.0.0` |

A clean result from a reader that did not fire proves nothing, so every sweep manifest was checked for components. These produced none, so the sweep says nothing about them:
- the Gemfile, Pipfile, `west.yml` and Dockerfile;
- the pubspec git and hosted dependencies, which have no lockfile;
- the Cargo git dependency, since only the main crate is emitted;
- the requirements `-e` line.

The pnpm tarball, the yarn `resolved`, the `pom.xml` and `build.gradle` repositories and the Composer vcs repository all came from readers that did fire. None of their URLs reached the output.

After the fixes below, the same sweep gives 0 / 0 / 0 / 0, with five warnings. Each names its manifest (`pyproject.toml` twice, `Package.resolved`, `package.json` twice) and none contains the credential.

### Rows added

| # | Reader | Field | Fix |
|---|---|---|---|
| D18 (b) | cargo-auditable | `waybill:cargo-auditable-source`, the raw non-registry source (the agent's survey) | the same sanitised `source` as D18 (`binary/entry.rs`) |
| D20 | every reader that records a declared requirement | `waybill:requirement-ranges` | `sanitize_declared_text`, applied once, where a `PackageDbEntry`'s ranges become the component's (`scan_fs/mod.rs`) |
| D21 | pip (pyproject) | `waybill:version-constraint` | `sanitize_declared_text` on the constraint (`pip/mod.rs`) |
| D22 | Swift | PURL from `Package.resolved` `location` | `sanitize_declared_url` on the location, before projection or logging (`swift/lockfile.rs`) |
| D19 (b) | Yocto | the host-typed PURL derived from `SRC_URI` (`recipe.rs` ~915) | covered by D19's sanitising of `SRC_URI` where it is read |
| N8 | Composer | `waybill:path` | a path repository's local path, not a URL |

### Cases added

- **D-4g:** Bazel `git_repository` `remote`.
- **D-6u:** a uv.lock `url` source.
- **D-20:** a `package.json` URL dependency.
- **D-21:** a pyproject direct URL.
- **D-22:** `Package.resolved`.

D-20 to D-22 failed before their fixes. D-4g and D-6u were written after theirs, and T022's mutations of those call sites stand in for the failing run.
