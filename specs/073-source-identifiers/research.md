# Research — milestone 073 source identifiers

## Decision summary

| Decision | Choice | Section |
|---|---|---|
| D1 — Per-built-in-scheme validators | Per-scheme syntactic checks (URL parse for `repo:` / `git:` / `attestation:`; `image:` regex per Q3 clarified shape); soft-fail on validation error → emit-as-opaque under `mikebom:source-identifiers` | §1 |
| D2 — CDX `externalReferences[].type` mapping | `repo:` / `git:` → `vcs`; `image:` → `distribution`; `attestation:` → `attestation` | §2 |
| D3 — Image-reference → identifier extraction site | Reuse the resolved-image-reference output from `scan_fs/oci_pull/` (registry pulls) and `scan_fs/docker_image.rs` (docker daemon + tarball loads); emit at the `--image` resolution boundary | §3 |
| D4 — Git remote auto-detection algorithm | 3-step fallback per spec Q1: `origin` → `upstream` → first-listed (alphabetical) via `git remote get-url <name>` shell-out | §4 |
| D5 — User-defined identifier annotation envelope | Reuse milestone-071's `MikebomAnnotationCommentV1` envelope for the `mikebom:source-identifiers` annotation. Single envelope, `value` is a JSON array of `{scheme, value, source_label}` objects | §5 |
| D6 — JSON canonicalization for determinism | Reuse milestone-071's `canonicalize_for_compare` helper (sorted keys, sorted arrays); identifier emit-side uses `BTreeMap<&str, &str>` as the source-of-truth ordering | §6 |
| D7 — Manual override semantics | Manual `--with-source <scheme>:<value>` for a scheme that auto-detection also produced: manual wins; auto-detected entry is dropped from emission; both URLs logged at info level so operator audit trail captures the override | §7 |

---

## §1 — Per-built-in-scheme validators

**Decision per scheme**:

### `repo:`

- **Validator**: parse value as a URL or git-style ssh URL (matches `^(https?://|ssh://|git@|git://)` or follows the ssh-pseudo `<user>@<host>:<path>` shape). Validation is permissive — git itself accepts many input shapes; we mirror that.
- **Output normalization**: store the value verbatim. No canonicalization (e.g., `git@github.com:foo/bar.git` and `https://github.com/foo/bar.git` are NOT collapsed — they're emitted as supplied). Operators choose their canonical form.
- **Failure mode**: malformed values (e.g., `not_a_url`) emit a `tracing::warn!` and pass through under `mikebom:source-identifiers` as opaque.

### `git:`

- **Validator**: `repo:`-style URL plus an optional `#<commit-or-ref>` fragment (e.g., `git:https://github.com/foo/bar.git#abc123`). When the fragment is present, the value-after-`#` SHOULD look like a git revision (40-char hex SHA, short SHA, branch name, tag) — but we don't validate; this is a hint, not enforcement.
- **Use case**: a `repo:` identifier identifies the repository; a `git:` identifier identifies a specific commit/ref within it. Useful for build-tier SBOMs that want to record commit-anchored identity beyond the bare repo URL.
- **Failure mode**: same as `repo:` — soft-fail to opaque pass-through.

### `image:`

- **Validator**: parse the canonical shape per Q3 clarification: `image:<registry>/<name>:<tag>@sha256:<digest>` (or omit registry / digest as documented). Regex: `^([a-zA-Z0-9.\-_]+/)?[a-zA-Z0-9.\-_/]+(:[a-zA-Z0-9.\-_]+)?(@sha256:[a-fA-F0-9]{64})?$`.
- **Auto-detection**: at image-tier scan time, the resolved image reference is `<registry>/<name>:<tag>@sha256:<digest>` (the OCI pull resolves the digest; docker daemon's `docker image inspect` returns it; tarball loads carry it in their manifest). Emit verbatim.
- **Failure mode**: malformed → opaque pass-through.

### `attestation:`

- **Validator**: parse value as a URL/IRI. Permissive — any RFC 3986 URI shape accepted.
- **Use case**: bind a SBOM to a specific in-toto attestation document by IRI. Pre-073 mikebom emits in-toto attestations via `mikebom attestation`; the `attestation:` identifier provides the cross-reference handle.
- **Failure mode**: soft-fail to opaque pass-through.

**Common rule across all built-in schemes**: validation is best-effort; the soft-fail-to-opaque path means a typo never breaks a scan. Operators see the `tracing::warn!` and can inspect the `mikebom:source-identifiers` annotation to confirm what was emitted.

**Rationale**: balances strictness (built-in schemes have semantic meaning the consumer can rely on) with robustness (a typo doesn't crash CI). Constitution Principle X transparency: warn-and-emit is more useful than silent-rewrite or hard-fail.

**Alternatives considered**:

- *Strict validation that hard-fails on malformed input*. Rejected — too brittle for CI workflows where operators sometimes typo.
- *No validation at all (everything opaque)*. Rejected — defeats the point of built-in schemes; CDX/SPDX consumers benefit from typed slots when the value is well-formed.
- *Strict validation but recoverable via a `--allow-malformed-identifiers` flag*. Rejected — unnecessary knob; the soft-fail-to-opaque path already gives operators a way to see what got emitted.

---

## §2 — CDX `externalReferences[].type` mapping per scheme

**Decision**: Per-scheme map to CDX 1.6 `externalReferences[].type` enum values.

| Scheme | CDX `type` |
|---|---|
| `repo:` | `vcs` |
| `git:` | `vcs` |
| `image:` | `distribution` |
| `attestation:` | `attestation` |

**Rationale**:

- CDX 1.6 specifies `externalReferences[].type` as one of: `vcs`, `issue-tracker`, `website`, `advisories`, `bom`, `mailing-list`, `social`, `chat`, `documentation`, `support`, `source-distribution`, `distribution`, `license`, `build-meta`, `build-system`, `release-notes`, `security-contact`, `model-card`, `log`, `configuration`, `evidence`, `formulation`, `attestation`, `threat-model`, `adversary-model`, `risk-assessment`, `vulnerability-assertion`, `exploitability-statement`, `pentest-report`, `static-analysis-report`, `dynamic-analysis-report`, `runtime-analysis-report`, `component-analysis-report`, `maturity-report`, `certification-report`, `codified-infrastructure`, `quality-metrics`, `poam`, `electronic-signature`, `digital-signature`, `rfc-9116`, `other`.
- `vcs` is the obvious fit for `repo:` and `git:`.
- `distribution` covers image-tier components — the image is the distribution form of the source.
- `attestation` is a CDX 1.6 native type, perfect fit for in-toto attestation IRIs.
- `other` is the fallback for any future built-in scheme that doesn't map cleanly. Won't apply to today's 4 built-ins.

**Alternatives considered**:

- *Use `source-distribution` for `repo:`*. Rejected — `source-distribution` is for "the source code distribution" (e.g., a tarball download URL), not the source repository identity. `vcs` is correct.
- *Use `bom` for `attestation:`*. Rejected — `bom` is for cross-document references to other SBOMs (which milestone 072 uses). Attestations are a separate construct.

---

## §3 — Image-reference → `image:` identifier extraction

**Decision**: Auto-detect the `image:` identifier at the `--image` resolution boundary. Three input paths:

1. **OCI registry pull** (`scan_fs/oci_pull/`): the registry pull resolves the image reference to a content-addressed digest. Emit `image:<registry>/<name>:<tag>@sha256:<digest>` from the post-pull state.
2. **Docker daemon load** (`scan_fs/docker_image.rs`): `docker image inspect <ref>` returns the digest; `<ref>` carries the tag. Emit the same shape.
3. **Tarball load**: the tarball's `manifest.json` carries the `RepoTags` + the image's content digest. Emit the same shape, possibly omitting the registry if the tarball doesn't carry one.

**Where to wire it**: `scan_fs/<image-source>/mod.rs` already returns a `ResolvedImage` (or equivalent) struct after pull/load. Add a method `ResolvedImage::canonical_identifier() -> Option<Identifier>` that synthesizes the `image:` form from the struct's fields. The CLI dispatch in `cli/scan_cmd.rs::execute` calls this and prepends to the `Vec<Identifier>` passed to the emitters (auto-detected always first, manual flags after, per FR-009).

**Rationale**: Reuses the existing image-resolution code paths. No new walk, no new pull. The `ResolvedImage` struct is the single point where mikebom commits to "this is what we're scanning" — the natural emission boundary.

**Alternatives considered**:

- *Re-parse the user-supplied `--image` flag*. Rejected — operator may pass `foo:latest` which is a tag-only reference; the digest is only known after resolution.
- *Auto-detect at SBOM-emit time by walking back to the image source*. Rejected — emit-time should be a pure function of resolved scan state.

---

## §4 — Git remote auto-detection algorithm

**Decision** per Q1 clarification: 3-step fallback. Implementation:

```text
fn auto_detect_repo_identifier(scan_root: &Path) -> Option<Identifier> {
    if !scan_root.join(".git").exists() { return None; }
    for name in ["origin", "upstream"] {
        if let Ok(url) = git_remote_get_url(scan_root, name) {
            return Some(Identifier::new(SchemeName::repo(), url, source_label: format!("auto-detected from git remote `{name}`")));
        }
    }
    // Fallback: first-listed remote (alphabetical per `git remote`)
    if let Ok(remotes) = git_remote_list(scan_root) {
        if let Some(first) = remotes.first() {
            if let Ok(url) = git_remote_get_url(scan_root, first) {
                return Some(Identifier::new(SchemeName::repo(), url, source_label: format!("auto-detected from git remote `{first}` (origin/upstream absent; first-listed)")));
            }
        }
    }
    tracing::info!(
        scan_root = %scan_root.display(),
        "no git origin / upstream / first-listed remote found; source identifier auto-detection skipped"
    );
    None
}
```

**Subprocess invocations**: each `git_remote_get_url` is `Command::new("git").args(["-C", scan_root, "remote", "get-url", name])` — the same pattern milestone 053 uses for `git describe` and milestone 072 uses for `git rev-parse HEAD`. No subprocess timeout (operator-controlled local config; deterministic command).

**Failure handling**: any subprocess error → log info, return `None`. Never fail the scan.

**Rationale**: matches Q1's chosen behavior + reuses an established subprocess pattern. The submodule case (`scan_root` is inside a submodule's working tree) is gracefully handled because `git -C` operates on the directory's resolved git context.

---

## §5 — User-defined identifier annotation envelope

**Decision**: Reuse milestone-071's `MikebomAnnotationCommentV1` envelope (defined at `generate/spdx/annotations.rs:31`) for the `mikebom:source-identifiers` annotation. The envelope's `value` field is a JSON array, one entry per user-defined identifier:

```json
{
  "schema": "mikebom-annotation/v1",
  "field": "mikebom:source-identifiers",
  "value": [
    { "scheme": "acme_corp_id", "value": "abc123", "source_label": null },
    { "scheme": "internal_ticket", "value": "PROJ-456", "source_label": null }
  ]
}
```

The `value` is an array (not a single object) because operators can attach multiple user-defined identifiers in one scan. The array is sorted by `(scheme, value)` lexicographically before serialization for determinism (FR-009).

**Built-in identifiers do NOT appear in this annotation** — they ride the standards-native carrier. The annotation is *exclusively* for the user-defined namespace.

**Rationale**: Reuses milestone-071 + milestone-072 infrastructure end-to-end. The cross-format-parity test infrastructure already decodes this envelope shape via `extract_mikebom_annotation_values`. The `value`-is-array shape means consumers walk the same code path regardless of whether 0, 1, or N user-defined identifiers are present.

**Alternatives considered**:

- *One annotation entry per identifier*. Rejected — N annotations are noisier on the wire than one annotation with an array value. The envelope shape is designed for single-key-per-annotation; arrays inside `value` are perfectly fine.
- *Custom non-envelope annotation shape*. Rejected — would fork milestone-071's parity catalog. Reuse is essential.

---

## §6 — JSON canonicalization for determinism

**Decision**: Reuse `parity::extractors::common::canonicalize_for_compare(value, false)` — the milestone-071 helper. Apply at every emit-side serialization boundary (CDX, SPDX 2.3, SPDX 3) so byte-identity goldens are stable.

For the user-defined identifier array: sort by `(scheme, value)` lex before serialization, so two scans with identical inputs produce byte-identical output.

For the built-in identifier carriers: emit auto-detected entries FIRST (per FR-009), then manual entries in the order supplied. CDX `externalReferences[]` array order matters for the goldens; SPDX 2.3 `Package.externalRefs[]` array order matters; SPDX 3 `Element.externalIdentifier[]` array order matters.

**Rationale**: Determinism is contractual (FR-009, SC-007 from milestone 072 carries the same posture). Reusing the milestone-071 helper avoids forking canonicalization rules.

---

## §7 — Manual override semantics

**Decision**: When auto-detection produces a `repo:` identifier AND the operator passes `--with-source repo:<different-url>`:

1. Manual entry wins. Auto-detected entry is dropped from emission.
2. `tracing::info!` log records both URLs and which won (`auto-detected: <a>; manual override: <b>; emitting manual`).

When auto-detection produces a `repo:` identifier AND the operator passes `--with-source repo:<same-url>` (already-detected URL):

1. Single entry emitted (deduplicated).
2. `tracing::info!` log notes "manual --with-source matches auto-detected; emitting once."

When the operator passes a DIFFERENT scheme entirely (e.g., `--with-source acme_corp_id:abc`):

1. Auto-detected `repo:` and manual `acme_corp_id:` both emit (different schemes → different namespaces → no override).

**Rationale**: Matches FR-006 + the Q1 clarified user expectation. The override is loud (info-level log) so operators auditing the SBOM can see the precedence chain.

**Alternatives considered**:

- *Auto-detected takes precedence on conflict*. Rejected — operator-explicit > tool-inferred is the universal CLI convention.
- *Both emit (no dedup)*. Rejected — emits semantically-confusing duplicate carriers; fails the FR-009 dedup requirement.

---

## Open items deferred to Phase 1

None. All architectural choices are pinned; data-model.md and contracts/ artifacts capture the concrete shapes; quickstart.md captures operator-visible behavior.
