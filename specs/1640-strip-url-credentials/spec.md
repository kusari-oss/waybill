# Feature Specification: Strip credentials from declared URLs (#1647)

**Feature Branch**: `1640-strip-url-credentials`
**Created**: 2026-11-21
**Status**: Draft
**Input**: #1647. A URL declared where a build or lockfile says to fetch from, such as `https://user:token@host/repo.git`, reaches the SBOM verbatim.

## Clarifications

### Session 2026-11-21

- Q: Is the `git` user of an `ssh://` URL a credential? → A: Yes, for this spec. It is stripped like any other username, as the helper and the submodule reader already do. One rule applies everywhere, and the readers that sanitise today keep their output.
- Q: Are credential-bearing query parameters removed too? → A: No. Only userinfo is removed. The C53 row and the consumer guide say so (FR-009). How often query-string tokens appear in declared URLs is to be measured before any rule is written (#1652).

## Background

An SBOM is a document people share. A project that fetches from a private host often writes the credential into the URL it declares, as a token in the userinfo part (`https://<token>@host/…`) or as a user and password (`https://user:token@host/…`). waybill copies some of those URLs into its output as declared.

The capability to remove them already exists:
- milestone 075 added a helper that strips RFC 3986 userinfo (`identifiers::sanitize::sanitize_userinfo`), and a companion that redacts it for logs;
- milestone 105 FR-016 made the helper public so the C/C++ readers it introduced would call it, and required a warning whenever it strips something.

It has three production callers: source identifiers, the git submodule reader, and the uv.lock reader's artifact URL. The readers below never call it. All of the following was **traced in code at `main` `86bce89`. None of it has been run.**

- **CMake:**
  - writes a fetch's `GIT_REPOSITORY` or `URL` into `waybill:download-url` (C53) as declared;
  - writes a dataset download's URL into a `download_url=` PURL qualifier;
  - derives a repository identity from the declared URL to match a fetch with the submodule or component that supplies it (spec 1538 R9). That normalisation removes the scheme and an scp-form `git@` prefix, but not userinfo. So a credentialled fetch and the sanitised submodule URL of one repository would not compare equal.
- **Bazel:** writes `waybill:download-url` as declared.
- **uv.lock:** sanitises `waybill:pypi-artifact-url`, but not `waybill:pypi-source-url` (the index URL, where a private index's credential usually sits) or its three `waybill:source-url` writes.
- **`waybill:source-url`** is also written by the Nix, Pants and Pants-JVM lockfile readers.
- **`vcs_url=` PURL qualifiers** are built from declared git URLs by the CocoaPods, Dart, Composer, Elixir and Erlang readers. The Pants resolve reader builds `download_url=`.
- **pip:** a requirements file's PEP 508 direct URL is recorded in `waybill:direct-url-source`.
- **Maven** writes `waybill:source-files-nested-url`.
- **cargo-auditable:** a git source is carried in `waybill:cargo-vcs-source-url` and becomes a CycloneDX `vcs` external reference.

**Measured on the 67 committed public-corpus goldens:** no URL in any of them carries userinfo. The only `%40` inside a URL qualifier is the `@<commit>` suffix of one `vcs_url`. Public projects do not commit credentials, so every behaviour this spec requires is shown with a synthetic fixture.

This list came from a search for URL-shaped annotation keys and qualifiers. It is a starting point, not the scope. FR-001 defines the scope.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A shared SBOM carries no credential from a declared URL (Priority: P1)

A team builds against a private Git host and a private package index. Their CMake files, lockfiles and requirements files declare URLs with an access token in them. They scan and send the SBOM to a customer. The document says where each component came from, but contains no token.

**Why this priority**: A leaked credential is the harm. Everything else in the spec follows from removing it.

**Independent Test**: For each kind of declared URL in the inventory, scan a fixture whose URL carries a distinctive user and token in all three formats. The bytes of every output document contain neither string. On `main` the same fixture's output does contain them, which proves the fixture reaches the field (the positive control).

**Acceptance Scenarios**:

1. **Given** a CMake `FetchContent_Declare` with `GIT_REPOSITORY https://user:token@host/repo.git`, **When** it is scanned, **Then** the component's download URL reads `https://host/repo.git` in every format.
2. **Given** a uv.lock whose index URL is `https://user:token@index.example/simple`, **When** it is scanned, **Then** neither the index URL nor any artifact URL in the output carries the user or token.
3. **Given** a CocoaPods, Dart, Composer, Elixir or Erlang git source with userinfo, **When** it is scanned, **Then** the `vcs_url` qualifier in the component's PURL carries none.
4. **Given** a URL whose userinfo is a token alone (`https://<token>@host/…`, the form personal access tokens take), **When** it is scanned, **Then** the token is removed.

---

### User Story 2 - Removing a credential does not change what a component is (Priority: P1)

The same repository can be declared twice, once with a credential (a CMake fetch) and once without (the git submodule that supplies it). They are one component today when both URLs are clean, and must stay one when one URL carries a credential.

**Why this priority**: A PURL qualifier is part of a component's identity, and deduplication compares PURLs. If a credential were removed only when the document is written, after identity had been decided, a clean and a credentialled spelling of one repository would already be two components. Two components with the same PURL would then follow.

**Independent Test**: A tree with a git submodule and a conditional `FetchContent_Declare` of the same repository, the fetch's URL carrying userinfo. The output has one component for the repository, the same one the tree yields with both URLs clean.

**Acceptance Scenarios**:

1. **Given** a submodule and a credentialled fetch of one repository at one commit, **When** scanned, **Then** one component is emitted, identical to the clean tree's.
2. **Given** a credentialled `GIT_REPOSITORY` on GitHub, **When** scanned, **Then** the component's PURL is the one the clean URL gives.
3. **Given** two components whose declared URLs differ only in userinfo, **When** scanned, **Then** they have the same PURL.

---

### User Story 3 - The operator learns that a credential was found (Priority: P2)

A credential in a committed manifest is a problem in its own right. When waybill removes one, it says so, so the operator can rotate the token. The warning itself never reveals the credential.

**Why this priority**: The SBOM is protected by Stories 1 and 2. This adds the signal milestone 105 FR-016 already requires and that the new sites do not yet give.

**Independent Test**: Scan a credentialled fixture and capture the log. One warning per stripped URL names the manifest and shows the URL with its userinfo replaced by the redaction marker. No log line contains the user or token.

**Acceptance Scenarios**:

1. **Given** a credentialled declared URL, **When** scanned, **Then** a warning names the manifest file and gives the redacted URL.
2. **Given** a scan with no credentialled URL, **When** scanned, **Then** no such warning is emitted.

### Edge Cases

- **Unresolved variables:** a CMake URL that is unresolved (`${VAR}`) or does not parse as a URL passes through unchanged, as the helper does today. It cannot be shown to carry a credential.
- **scp-form git remotes:** an scp-form remote (`git@host:org/repo.git`) does not parse as a URL and passes through. It can name a user but cannot carry a password.
- **`ssh://git@host/…`:** this form parses, and its conventional `git` user is stripped like any other username (Clarifications). The emitted URL is then `ssh://host/…`, which no longer names the login user.
- **Credentials in the query string:** a signed URL or token parameter (`?private_token=…`, `access_token=`, `X-Amz-Signature=`) is not userinfo and is **not** removed (Clarifications). FR-009 documents the limit.
- **Encoded qualifiers:** a URL inside a PURL qualifier is percent-encoded. Removal happens before encoding, so an encoded `%40` from userinfo never reaches the qualifier. A `%40` that belongs to the URL itself, such as a `vcs_url` `@<commit>` suffix, is unaffected.
- **Enrichment data:** a URL waybill fetched from an enrichment service rather than read from the scanned project is not a declared URL. The inventory records it, with that reason.
- **Merges:** when a component absorbs another (spec 1639's merge sites), the surviving value is already sanitised, because each member's value was sanitised at its reader.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: An inventory MUST list every emitted field whose value can come from a URL the scanned project declares. Fields include annotations, PURL qualifiers, external references, SPDX `downloadLocation`, and SPDX 3 `software_sourceInfo` and `software_downloadLocation`. It MUST be built by searching the code at planning time, not taken from the Background's list. Each row names the reader, the field, and either "sanitised" or "not a declared URL" with the reason.
- **FR-002**: Every inventory row marked sanitised MUST have its URL's credential removed (FR-003) in the reader, before the URL contributes to a PURL, a repository identity, an annotation or an external reference.
- **FR-003**: Removing a credential means removing the URL's userinfo, both a user alone and a user with a password. A username with no password, including the `git` user of an `ssh://` URL, counts as userinfo. Everything else in the URL, including the query string, is kept.
- **FR-004**: Two declarations of one URL that differ only in userinfo MUST yield the same PURL, and so the same component wherever they meet today. CMake's repository identity (spec 1538 R9) MUST compare the sanitised form.
- **FR-005**: Each removal MUST emit milestone 105 FR-016's warning: the manifest path, and the URL as the log-redaction helper renders it. No log line, at any level, may contain the removed user or password.
- **FR-006**: A scan in which no declared URL carries userinfo (FR-003) MUST produce output byte-identical to `main`. A scan whose only userinfo is the `git` user of an `ssh://` URL changes in that URL alone.
- **FR-007**: The three formats MUST agree on every inventory field (`waybill sbom parity-check`). CycloneDX MUST validate against the vendored 1.6 schema, and no two components may share a `bom-ref`.
- **FR-008**: Repeat scans MUST produce identical output.
- **FR-009**: The C53 row of `docs/reference/sbom-format-mapping.md` and the consumer guide (`docs/reference/reading-a-mikebom-sbom.md`) MUST state what is removed from a declared URL (userinfo, including a bare username) and what is not (the query string), so a reader knows what "sanitised" covers.
- **FR-010**: No new Cargo dependencies.

### Key Entities

- **Declared URL**: a URL the scanned project writes down as a place to fetch from: a git remote, an archive URL, a package index, a direct requirement URL. Its userinfo is the credential this spec removes.
- **Inventory row**: one emitted field that can carry a declared URL. It has a reader, a field, a disposition (sanitised, or not a declared URL) and a fixture.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Every inventory row marked sanitised has a fixture whose declared URL carries a distinctive user and token. On `main`, that fixture's output contains the token (the positive control). A row without a passing control is not counted as covered.
- **SC-002**: On the branch, the user and token of every SC-001 fixture occur 0 times in the bytes of the CycloneDX, SPDX 2.3 and SPDX 3 output and in the scan's log.
- **SC-003**: User Story 2's trees each give one component per repository, identical to the clean tree's component.
- **SC-004**: The public corpus regenerated on the branch (`regen_goldens`) is byte-identical to the committed goldens: no target declares a credentialled URL (measured, Background). TheRock's source tree and the quality corpus, scanned at the branch and at `main`, have equal component counts. Every changed field is a removed userinfo (FR-003), traced component by component.
- **SC-005**: Deleting any one sanitising call makes at least one test fail.
- **SC-006**: Every fixture scanned 10 times gives byte-identical output, parity-check agrees on the inventory fields, and the CycloneDX output validates.

## Assumptions

- **The helper is reused:** the existing milestone 075 helper's userinfo rule is the rule, as the clarifications confirm. It already treats an empty userinfo (`https://@host`) as nothing to remove, and passes unparseable input through unchanged.
- **Removal, not a marker:** userinfo is removed outright, not replaced with a marker in the SBOM. A URL without userinfo is still a valid, meaningful location, and a marker would be a second spelling of the same URL (User Story 2).
- **Out of scope:**
  - Credentials outside URLs (a token in an environment-variable reference, a `.netrc`) are not read by waybill and are not addressed.
  - Source identifiers and git submodule URLs are already sanitised and only appear in the inventory.
