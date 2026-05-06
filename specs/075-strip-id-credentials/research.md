# Research — milestone 075 credential stripping

Six implementation-level decisions to pin before Phase 1 design.

## §1 — `url` crate API for userinfo manipulation

**Decision**: Use `url::Url::parse` plus `set_username("")` and `set_password(None)`. Both setter calls return `Result<(), ()>` (cannot-be-base error variant); on Err, the function falls through to the FR-009 soft-fail path and returns the original string unchanged.

**Rationale**: Documented public API. Both setters succeed for any URL with a well-formed authority component (which is exactly the set of URLs that have userinfo to begin with). Cannot-be-base URLs (e.g., `mailto:`) lack an authority and reject these setters; for our auto-detect input domain (git remote URLs), this is a vanishingly rare path but still needs a safe fallback.

**Alternatives considered**:
- Manual string manipulation (split on `://`, find the next `@`) — Rejected: error-prone with corner cases (port-only userinfo, URL-encoded userinfo, bracketed IPv6 hosts). The `url` crate's parser is the well-tested standard.
- Use `Url::set_authority` with a manually-constructed host:port string — Rejected: unnecessarily reinvents what the dedicated setters already do.

## §2 — `url` crate dependency promotion

**Decision**: Add `url = "2"` to `[workspace.dependencies]` in the root `Cargo.toml`, and `url = { workspace = true }` to `[dependencies]` in `mikebom-cli/Cargo.toml`. The workspace `Cargo.lock` already pins `url 2.5.8` transitively (via `reqwest`); promotion does not change the lockfile resolution.

**Rationale**: The crate is already in the dep graph. Direct dep declaration just makes the import valid in `mikebom-cli/src/binding/identifiers/auto_detect.rs`. Cargo's MVS resolver dedups to the existing version. Lockfile churn is zero.

**Alternatives considered**:
- Re-export `url::Url` through a thin wrapper crate — Rejected: pure overhead.
- Vendor a minimal URL parser into `mikebom-cli` — Rejected: violates the project's "no NIH" posture and creates maintenance burden.

## §3 — Sanitization sentinel for `source_label`

**Decision**: Append the string ` (credentials stripped)` to the existing `source_label` value. Specifically:

- Source-tier sanitized: `"auto-detected from git remote `origin` (credentials stripped)"`
- Source-tier fallback-listed sanitized: `"auto-detected from git remote `<name>` (origin/upstream absent; first-listed) (credentials stripped)"`
- Build-tier sanitized: `"auto-detected from build-tier git remote `origin` (credentials stripped)"`
- Build-tier `git:` sanitized: `"auto-detected from build-tier `git rev-parse HEAD` (credentials stripped)"`

Pin these strings now; goldens depend on them.

**Rationale**: Suffix-append preserves the existing milestone-073/074 prefix structure that consumers may already parse (especially the build-tier / source-tier distinction). Parenthesized clause is easy to read and matches the existing `(origin/upstream absent; first-listed)` parenthetical convention from milestone 073.

**Alternatives considered**:
- Replace the entire label with a sanitization-specific phrasing — Rejected: loses the original auto-detect provenance information.
- Use a structured suffix like `[sanitized]` — Rejected: inconsistent with the existing free-prose label style.

## §4 — Log-line redaction marker

**Decision**: Use the literal placeholder string `<userinfo redacted>` in info-level log lines. The full log line shape:

```
INFO sanitized userinfo from auto-detected `<scheme>` identifier; original URL prefix: `<scheme>://<userinfo redacted>@<host><path>`
```

Both `<userinfo redacted>` and any subsequent fields exclude the actual credential value. The host and path are not sensitive and ARE included for operator debuggability.

**Rationale**: Angle-bracket placeholder is conventional for "machine-meaningful redaction marker" across log standards. Including host + path lets operators identify which remote was sanitized without revealing the secret. Matches the project's existing `tracing::warn!(reason = %err, ...)` style at `auto_detect.rs:96-104`.

**Alternatives considered**:
- `[redacted]` (square brackets) — Rejected: less distinctive in a stream of log output where square brackets often denote metadata.
- Omit the URL entirely from the log — Rejected: operators lose the ability to identify which remote was affected without re-running the scan.
- Log a hash of the userinfo — Rejected: overkill; the host already disambiguates.

## §5 — Opt-out flag plumbing

**Decision**: Add a single new boolean `--keep-credentials-in-identifiers` flag to both `ScanArgs` (in `scan_cmd.rs`) and `RunArgs` (in `run.rs`). Default false. Clap's derive macros pick it up automatically. Each call site passes the boolean to its respective auto-detect call:

```rust
auto_detect_repo_identifier(scan_root, args.keep_credentials_in_identifiers);  // source-tier
auto_detect_build_tier_identifiers(invocation_cwd, args.keep_credentials_in_identifiers);  // build-tier
```

No shared struct. Two-line CLI surface, two-line call-site update.

**Rationale**: Smallest viable plumbing. A shared `IdentifierOpts` struct is premature abstraction at this milestone's scale (one shared boolean across two `Args` structs). If a future milestone adds more identifier-related flags, the abstraction can emerge organically.

**Alternatives considered**:
- Shared `#[derive(Args)]` `IdentifierOpts` struct — Deferred. Add when there are 3+ shared flags.
- Per-tier flags (`--scan-keep-credentials`, `--trace-keep-credentials`) — Rejected: same flag, same meaning, different command. Single flag name preserves predictability.
- Environment variable opt-out (`MIKEBOM_KEEP_CREDENTIALS=1`) — Rejected: explicit CLI flag is more discoverable. Environment fallback can be added later if operators ask.

## §6 — SSH form passthrough behavior

**Decision**: When `Url::parse` rejects the input string (which happens for SSH-form URLs like `git@github.com:foo/bar.git`), `sanitize_userinfo` returns `SanitizedUrl { original, sanitized: original.clone(), was_sanitized: false }`. The downstream code path is identical to the no-userinfo case: emit verbatim, no `source_label` augmentation, no log line.

**Rationale**: SSH-form URLs have no userinfo by construction (the `git@` is a fixed SSH username, not a credential). Treating parse-failure as "nothing to sanitize" is the correct semantic: we couldn't analyze it, but we know there's nothing to strip in the SSH-form case. The downstream identifier still gets classified by milestone-073's existing soft-fail logic — if `validate_repo` accepts the SSH form (it does, per current behavior), the identifier emits as `Builtin`; if not, it soft-fails to `UserDefined`.

**Side effect for SC-007**: SSH-form URLs emit byte-identical to alpha.16 — `was_sanitized` is false, no log line, no label change, no value change. The existing milestone 073/074 contracts are preserved unchanged for the SSH case.

**Alternatives considered**:
- Return `None` on parse failure and have callers fall through — Rejected: changes the existing call-site signature; breaks the "transparent passthrough" semantic that the milestone needs for SSH-form correctness.
- Implement a manual SSH-form detection (regex match `^[^/@]+@[^/]+:`) — Rejected: not needed. The `Url::parse` failure path covers SSH-form transparently. Additional detection is duplicative.
