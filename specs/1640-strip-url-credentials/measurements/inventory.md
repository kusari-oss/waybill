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
