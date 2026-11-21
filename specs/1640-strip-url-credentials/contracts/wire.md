# Wire contract: declared URLs without credentials

## Output fields

No field changes shape. For every D-row in research R1, the value written is the sanitised form of the declared URL (data-model). All three formats carry the same value, and the existing parity rows apply: C53 for `waybill:download-url`, the external-reference and PURL rows elsewhere.

| Declared | Emitted |
|---|---|
| `https://wbfxuser:wbfxtoken7c41@git.example/org/repo.git` | `https://git.example/org/repo.git` |
| `https://wbfxtoken7c41@Git.Example:443/org/repo.git` | `https://Git.Example:443/org/repo.git` (host case and port kept, research R3) |
| `git+https://wbfxuser:wbfxtoken7c41@git.example/org/repo.git` (in a `vcs_url` qualifier) | `vcs_url=git+https://git.example/org/repo.git`, encoded as the reader encodes today |
| `ssh://git@git.example/org/repo.git` | `ssh://git.example/org/repo.git` (Clarification Q1) |
| `https://git.example/a.tar.gz?private_token=…` | unchanged (Clarification Q2, #1652) |
| `git@git.example:org/repo.git` | unchanged (not a URL) |
| `${FX_URL}` (unresolved CMake variable) | unchanged |

## Log contract

Each sanitised occurrence emits the event in data-model ("Sanitisation event"). No log line at any level contains `wbfxuser` or `wbfxtoken7c41` in any test.

## Documentation contract (FR-009)

- **C53 row** of `docs/reference/sbom-format-mapping.md:` "The declared URL with its userinfo removed (a user, or a user and password, including the `git` user of an `ssh://` URL; spec 1640). The query string is not altered (#1652)."
- **Consumer guide** (`docs/reference/reading-a-mikebom-sbom.md`): one paragraph saying the same for every URL waybill copies from a manifest, naming the fields.

## Integration cases (`waybill-cli/tests/declared_url_credentials.rs`)

Every D-case checks four things:
- the credential strings occur 0 times in the CycloneDX, SPDX 2.3 and SPDX 3 bytes and in the captured log;
- the field is present with the clean URL (the control on the branch);
- one redacted warning names the fixture's manifest;
- parity, CycloneDX schema validation and unique `bom-ref`s hold, and 10 scans are byte-identical.

| Case | Row | Fixture |
|---|---|---|
| D-1 | D1 | `FetchContent_Declare(waybill-fixture-cred GIT_REPOSITORY https://wbfxuser:wbfxtoken7c41@git.example/org/waybill-fixture-cred.git GIT_TAG v1.0.0)` |
| D-1u | D1 | the same with `URL https://wbfxuser:wbfxtoken7c41@dl.example/waybill-fixture-cred-1.0.0.tar.gz` |
| D-2 | D2 | a CMake dataset download whose URL carries userinfo |
| D-3 | D3 | `GIT_REPOSITORY https://wbfxtoken7c41@github.com/waybill-org/waybill-fixture-cred.git`: the PURL is the clean URL's `pkg:github` |
| D-4 | D4 | Bazel `http_archive` with a credentialled URL |
| D-5 | D5 | uv.lock whose index URL carries userinfo |
| D-6 | D6 | uv.lock git source with userinfo |
| D-7 | D7 | flake.lock node whose `url` carries userinfo |
| D-8 | D8 | Pants pex lockfile artifact URL with userinfo |
| D-9 | D9 | Pants coursier lockfile artifact URL with userinfo |
| D-10 | D10 | Nix closure derivation source URL with userinfo (the closure tier's stub-nix harness) |
| D-11 | D11 | Nix Haskell closure location with userinfo (same harness) |
| D-12 … D-16 | D12–D16 | CocoaPods, Dart (git and self-hosted), Composer (vcs and repository), Elixir, Erlang git sources with userinfo |
| D-17 | D17 | `waybill-fixture-cred @ https://wbfxuser:wbfxtoken7c41@dl.example/waybill-fixture-cred-1.0.0.tar.gz` in a requirements file |
| D-18 | D18 | cargo-auditable data with a `git+https://wbfxuser:wbfxtoken7c41@…` source (unit level, `binary/entry.rs`, if no binary fixture exists) |
| D-19 | D19 | Yocto recipe `SRC_URI = "git://wbfxuser:wbfxtoken7c41@git.example/repo;protocol=https;branch=main"` |
| D-20 | D20 | Helm `Chart.yaml` dependency `repository: https://wbfxuser:wbfxtoken7c41@charts.example/` |
| I-1 | FR-004 | a git submodule with a clean URL, plus a conditional `FetchContent_Declare` of the same repository with a credentialled URL: one component, equal to the all-clean tree's |
| I-2 | FR-004 | D-3's PURL equals the PURL of the same fetch without userinfo |
| I-3 | FR-004, R3 | `https://wbfxtoken7c41@Git.Example/Org/Repo.git` and `https://Git.Example/Org/Repo.git` give equal C53 values |
| G-1 … G-4 | N5 | Gemfile.lock `remote:`, Cargo.lock `registry+…`, requirements `--index-url`, package-lock `resolved`, each credentialled: absent on `main` and on the branch |
| C-ctl | FR-006 | the D-fixtures with clean URLs: output byte-identical to a golden generated from `main`'s binary before any source change |
