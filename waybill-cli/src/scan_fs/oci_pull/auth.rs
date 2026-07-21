//! Docker keychain credential resolution for OCI registry pulls
//! (milestone 034 / 031.x — closes #66).
//!
//! Resolves credentials for a target registry from `~/.docker/config.json`
//! (or `$DOCKER_CONFIG/config.json`), implementing the standard precedence:
//!
//!   1. `credHelpers.<registry>` — per-registry credential helper override.
//!   2. `credsStore` — registry-wide credential helper.
//!   3. `auths.<registry>.auth` — base64-encoded `user:password`.
//!   4. `auths.<registry>.identitytoken` — registry-issued identity token.
//!
//! Each helper is invoked as a subprocess (`docker-credential-<name> get`)
//! per the documented Docker cred-helper API:
//!   <https://github.com/docker/docker-credential-helpers>
//!
//! Secret material is held in `Credential`, whose `Debug` is hand-written
//! to redact both fields. No `tracing::*!` / `anyhow::Context` /
//! `eprintln!` macro in this module interpolates a secret value;
//! commit-3's grep audit verifies this mechanically.
//!
//! Sync by design: the cred-helper subprocess completes in ~100ms in
//! practice (worst case a few seconds for ECR's STS round-trip), and
//! mikebom is a one-shot CLI. Brief blocking in the tokio runtime is
//! acceptable here. If this ever needs to become async, switch to
//! `tokio::process::Command` + `tokio::fs::read`.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use base64::engine::general_purpose::STANDARD as B64_STANDARD;
use base64::Engine as _;
use serde::Deserialize;

/// A resolved credential pair, with redacting `Debug` impl.
///
/// `secret` may be a password (Basic auth), a PAT, or an identity token
/// depending on the cred source. Treat it as opaque from the perspective
/// of credential consumers.
#[derive(Clone)]
pub(super) struct Credential {
    pub(super) username: String,
    pub(super) secret: String,
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("username", &"<redacted>")
            .field("secret", &"<redacted>")
            .finish()
    }
}

/// Parsed `~/.docker/config.json`. All fields default to empty so that
/// partial / minimal configs (a common shape — `docker login` writes
/// only what it needs) parse successfully.
#[derive(Debug, Default, Deserialize)]
pub(super) struct DockerConfig {
    #[serde(default)]
    pub(super) auths: HashMap<String, AuthEntry>,
    #[serde(rename = "credsStore", default)]
    pub(super) creds_store: Option<String>,
    #[serde(rename = "credHelpers", default)]
    pub(super) cred_helpers: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct AuthEntry {
    #[serde(default)]
    pub(super) auth: Option<String>,
    #[serde(default)]
    pub(super) identitytoken: Option<String>,
}

/// Locate and parse the user's Docker config. Returns `None` (NOT an
/// error) if no config exists or it can't be parsed; this is the
/// common case for systems that have never run `docker login`, and we
/// want to fall through to anonymous in that case.
pub(super) fn load_default_docker_config() -> Option<DockerConfig> {
    let path = docker_config_path()?;
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(
                config_path = %path.display(),
                error = %e,
                "could not read Docker config; proceeding without registry credentials"
            );
            return None;
        }
    };
    match serde_json::from_slice::<DockerConfig>(&bytes) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            tracing::warn!(
                config_path = %path.display(),
                error = %e,
                "could not parse Docker config; proceeding without registry credentials"
            );
            None
        }
    }
}

fn docker_config_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("DOCKER_CONFIG") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir).join("config.json"));
        }
    }
    let home = std::env::var("HOME").ok()?;
    if home.is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(".docker").join("config.json"))
}

/// Issue #235 — layered credential resolver for in-cluster
/// operation. Tries each source in order, falling through to the
/// next if it returns `None`:
///
/// 1. **Per-registry env vars**:
///    `MIKEBOM_REGISTRY_<HOST>_USERNAME` + `_PASSWORD`, where
///    `<HOST>` is the registry hostname normalized to uppercase
///    with `[^A-Z0-9]` replaced by `_` (e.g. `ghcr.io` →
///    `MIKEBOM_REGISTRY_GHCR_IO_USERNAME`,
///    `my-ecr.amazonaws.com` →
///    `MIKEBOM_REGISTRY_MY_ECR_AMAZONAWS_COM_USERNAME`).
/// 2. **Generic env vars**: `MIKEBOM_REGISTRY_USERNAME` +
///    `_PASSWORD`. Applies to every registry — useful when a
///    cluster scan only ever hits one registry.
/// 3. **Credentials directory** (`--registry-credentials-dir`):
///    when supplied, reads a Docker-format config file from the
///    directory. Filename probe order: `config.json` (plain
///    Docker), `.dockerconfigjson` (K8s
///    `kubernetes.io/dockerconfigjson` secret), `.dockercfg`
///    (legacy K8s `kubernetes.io/dockercfg` secret). First hit
///    wins; same `resolve_credentials` logic applies to the
///    loaded config.
/// 4. **Default Docker config**: `$DOCKER_CONFIG/config.json`
///    or `$HOME/.docker/config.json`. Existing pre-fix behavior.
///
/// Returns `None` when every source fails — caller falls through
/// to anonymous registry access (which works for public registries
/// hosting public images).
pub(super) fn resolve_credentials_layered(
    registry: &str,
    creds_dir: Option<&Path>,
) -> Option<Credential> {
    // 1 + 2. Env-var sources.
    if let Some(c) = env_credential(registry) {
        return Some(c);
    }
    // 3. K8s-style credentials directory.
    if let Some(dir) = creds_dir {
        if let Some(cfg) = load_docker_config_from_dir(dir) {
            if let Some(c) = resolve_credentials(&cfg, registry) {
                return Some(c);
            }
        }
    }
    // 4. Default Docker config.
    if let Some(cfg) = load_default_docker_config() {
        if let Some(c) = resolve_credentials(&cfg, registry) {
            return Some(c);
        }
    }
    None
}

/// Issue #235 — env-var credential lookup. Tries per-registry
/// `MIKEBOM_REGISTRY_<HOST>_USERNAME` + `_PASSWORD` first, then
/// generic `MIKEBOM_REGISTRY_USERNAME` + `_PASSWORD`. Returns
/// `None` when either USERNAME or PASSWORD is missing for the
/// chosen source (partial config is treated as no config — we
/// don't synthesize a half-complete `Credential`).
fn env_credential(registry: &str) -> Option<Credential> {
    let normalized = normalize_registry_key(registry);
    let env_host = normalized
        .chars()
        .map(|c| {
            let up = c.to_ascii_uppercase();
            if up.is_ascii_alphanumeric() {
                up
            } else {
                '_'
            }
        })
        .collect::<String>();
    let per_user = format!("MIKEBOM_REGISTRY_{env_host}_USERNAME");
    let per_pass = format!("MIKEBOM_REGISTRY_{env_host}_PASSWORD");
    if let (Ok(u), Ok(p)) = (std::env::var(&per_user), std::env::var(&per_pass)) {
        if !u.is_empty() && !p.is_empty() {
            return Some(Credential {
                username: u,
                secret: p,
            });
        }
    }
    if let (Ok(u), Ok(p)) = (
        std::env::var("MIKEBOM_REGISTRY_USERNAME"),
        std::env::var("MIKEBOM_REGISTRY_PASSWORD"),
    ) {
        if !u.is_empty() && !p.is_empty() {
            return Some(Credential {
                username: u,
                secret: p,
            });
        }
    }
    None
}

/// Issue #235 — load a Docker-format config from a credentials
/// directory. Probes the K8s secret-mount filenames in order:
/// `config.json` (plain Docker), `.dockerconfigjson` (K8s
/// `kubernetes.io/dockerconfigjson` secret type), `.dockercfg`
/// (legacy K8s `kubernetes.io/dockercfg` secret type). First
/// readable + parseable file wins. Parse failures are logged
/// at warn-level and the next filename is tried.
fn load_docker_config_from_dir(dir: &Path) -> Option<DockerConfig> {
    for filename in ["config.json", ".dockerconfigjson", ".dockercfg"] {
        let path = dir.join(filename);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "could not read credentials file from --registry-credentials-dir; trying next filename"
                );
                continue;
            }
        };
        match serde_json::from_slice::<DockerConfig>(&bytes) {
            Ok(cfg) => return Some(cfg),
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "could not parse credentials file from --registry-credentials-dir; trying next filename"
                );
            }
        }
    }
    None
}

/// Resolve credentials for `registry`, applying the precedence:
/// `credHelpers` > `credsStore` > `auths.<reg>.auth` >
/// `auths.<reg>.identitytoken` > `None`.
pub(super) fn resolve_credentials(
    cfg: &DockerConfig,
    registry: &str,
) -> Option<Credential> {
    let normalized = normalize_registry_key(registry);

    // 1. Per-registry helper override (covers ECR's typical setup:
    // `credHelpers: { "<account>.dkr.ecr.<region>.amazonaws.com": "ecr-login" }`).
    let helper_key = cfg
        .cred_helpers
        .keys()
        .find(|k| normalize_registry_key(k) == normalized);
    if let Some(key) = helper_key {
        if let Some(helper) = cfg.cred_helpers.get(key) {
            if let Some(c) = run_credential_helper(helper, registry) {
                return Some(c);
            }
        }
    }

    // 2. Registry-wide helper (covers `credsStore: desktop` / `osxkeychain`
    // / `wincred` / `pass` / `secretservice`).
    if let Some(helper) = cfg.creds_store.as_deref() {
        if let Some(c) = run_credential_helper(helper, registry) {
            return Some(c);
        }
    }

    // 3 & 4. Direct auth / identitytoken.
    let auth_key = cfg
        .auths
        .keys()
        .find(|k| normalize_registry_key(k) == normalized);
    if let Some(key) = auth_key {
        if let Some(entry) = cfg.auths.get(key) {
            if let Some(c) = entry.auth.as_deref().and_then(decode_auth_field) {
                return Some(c);
            }
            if let Some(token) = entry.identitytoken.as_deref() {
                if !token.is_empty() {
                    return Some(Credential {
                        username: "<token>".to_string(),
                        secret: token.to_string(),
                    });
                }
            }
        }
    }

    None
}

/// Decode an `auth` field's `<base64(user:secret)>` form. Empty input
/// or malformed payloads return `None` (caller treats as
/// "no credentials available").
fn decode_auth_field(b64: &str) -> Option<Credential> {
    let trimmed = b64.trim();
    if trimmed.is_empty() {
        return None;
    }
    let decoded = B64_STANDARD.decode(trimmed).ok()?;
    let s = std::str::from_utf8(&decoded).ok()?;
    let (user, secret) = s.split_once(':')?;
    if user.is_empty() {
        return None;
    }
    Some(Credential {
        username: user.to_string(),
        secret: secret.to_string(),
    })
}

/// Normalize a registry-name string for matching keys in `auths` /
/// `credHelpers`. Strips `http(s)://` schemes, drops any path segment,
/// lowercases the host, and treats `index.docker.io` as `docker.io`.
fn normalize_registry_key(s: &str) -> String {
    let mut t = s.trim().to_ascii_lowercase();
    for prefix in ["https://", "http://"] {
        if let Some(stripped) = t.strip_prefix(prefix) {
            t = stripped.to_string();
            break;
        }
    }
    if let Some(slash) = t.find('/') {
        t.truncate(slash);
    }
    if t == "index.docker.io" {
        t = "docker.io".to_string();
    }
    t
}

/// Invoke a Docker credential helper subprocess by its short name
/// (e.g. `osxkeychain`, `ecr-login`). Resolves to the full program name
/// `docker-credential-<helper>` and dispatches to
/// [`run_credential_helper_program`].
fn run_credential_helper(helper: &str, registry: &str) -> Option<Credential> {
    let program = format!("docker-credential-{helper}");
    run_credential_helper_program(&program, registry)
}

/// Invoke a credential helper by full program path or PATH-resolvable
/// program name. Returns `None` for "credentials not found" or any
/// helper failure (spawn / wait / non-zero exit / unparseable JSON).
///
/// The helper's stderr is piped to `Stdio::null()` — some helpers
/// emit partial credentials (or auth-failure details) on stderr, and
/// we don't want those leaking into mikebom's logs.
fn run_credential_helper_program(program: &str, registry: &str) -> Option<Credential> {
    use std::io::Write as _;

    let mut child = match Command::new(program)
        .arg("get")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                helper = %program,
                error = %e,
                "could not invoke credential helper; falling back to anonymous"
            );
            return None;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{registry}");
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(
                helper = %program,
                error = %e,
                "credential helper subprocess failed; falling back to anonymous"
            );
            return None;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);

    // "credentials not found in native keychain" is the documented sentinel
    // emitted by helpers when no credentials are cached for the registry.
    // (osxkeychain, wincred, secretservice, pass all use this string.)
    if !output.status.success()
        || stdout.to_ascii_lowercase().contains("credentials not found")
    {
        return None;
    }

    #[derive(Deserialize)]
    struct HelperOutput {
        #[serde(rename = "Username")]
        username: String,
        #[serde(rename = "Secret")]
        secret: String,
    }

    let parsed: HelperOutput = serde_json::from_str(&stdout).ok()?;
    if parsed.username.is_empty() && parsed.secret.is_empty() {
        return None;
    }
    Some(Credential {
        username: parsed.username,
        secret: parsed.secret,
    })
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn credential_debug_redacts_both_fields() {
        let c = Credential {
            username: "alice".to_string(),
            secret: "supersecretpat".to_string(),
        };
        let dbg = format!("{c:?}");
        assert!(!dbg.contains("alice"), "username leaked in Debug: {dbg}");
        assert!(
            !dbg.contains("supersecretpat"),
            "secret leaked in Debug: {dbg}"
        );
        assert!(dbg.contains("redacted"));
    }

    #[test]
    fn normalize_registry_key_handles_common_forms() {
        assert_eq!(normalize_registry_key("docker.io"), "docker.io");
        assert_eq!(normalize_registry_key("index.docker.io"), "docker.io");
        assert_eq!(
            normalize_registry_key("https://index.docker.io/v1/"),
            "docker.io"
        );
        assert_eq!(
            normalize_registry_key("https://index.docker.io/v2/"),
            "docker.io"
        );
        assert_eq!(normalize_registry_key("ghcr.io"), "ghcr.io");
        assert_eq!(normalize_registry_key("GHCR.IO"), "ghcr.io");
        assert_eq!(
            normalize_registry_key("https://gcr.io"),
            "gcr.io"
        );
        assert_eq!(
            normalize_registry_key("123456789012.dkr.ecr.us-east-1.amazonaws.com"),
            "123456789012.dkr.ecr.us-east-1.amazonaws.com"
        );
    }

    #[test]
    fn decode_auth_field_decodes_user_password() {
        // base64("alice:hunter2") = YWxpY2U6aHVudGVyMg==
        let c = decode_auth_field("YWxpY2U6aHVudGVyMg==").unwrap();
        assert_eq!(c.username, "alice");
        assert_eq!(c.secret, "hunter2");
    }

    #[test]
    fn decode_auth_field_handles_empty_secret() {
        // base64("alice:") = YWxpY2U6
        let c = decode_auth_field("YWxpY2U6").unwrap();
        assert_eq!(c.username, "alice");
        assert_eq!(c.secret, "");
    }

    #[test]
    fn decode_auth_field_rejects_empty_input() {
        assert!(decode_auth_field("").is_none());
        assert!(decode_auth_field("   ").is_none());
    }

    #[test]
    fn decode_auth_field_rejects_missing_separator() {
        // base64("aliceonly") = YWxpY2Vvbmx5
        assert!(decode_auth_field("YWxpY2Vvbmx5").is_none());
    }

    #[test]
    fn decode_auth_field_rejects_empty_username() {
        // base64(":secret") = OnNlY3JldA==
        assert!(decode_auth_field("OnNlY3JldA==").is_none());
    }

    #[test]
    fn resolve_credentials_returns_auth_field() {
        // base64("alice:pat") = YWxpY2U6cGF0
        let json = r#"{
            "auths": {
                "ghcr.io": { "auth": "YWxpY2U6cGF0" }
            }
        }"#;
        let cfg: DockerConfig = serde_json::from_str(json).unwrap();
        let c = resolve_credentials(&cfg, "ghcr.io").unwrap();
        assert_eq!(c.username, "alice");
        assert_eq!(c.secret, "pat");
    }

    #[test]
    fn resolve_credentials_returns_identitytoken_when_no_auth() {
        let json = r#"{
            "auths": {
                "myregistry.example.com": { "identitytoken": "tok-abc-123" }
            }
        }"#;
        let cfg: DockerConfig = serde_json::from_str(json).unwrap();
        let c = resolve_credentials(&cfg, "myregistry.example.com").unwrap();
        assert_eq!(c.username, "<token>");
        assert_eq!(c.secret, "tok-abc-123");
    }

    #[test]
    fn resolve_credentials_normalizes_docker_io_aliases() {
        // base64("u:p") = dTpw
        let json = r#"{
            "auths": {
                "https://index.docker.io/v1/": { "auth": "dTpw" }
            }
        }"#;
        let cfg: DockerConfig = serde_json::from_str(json).unwrap();
        // The reference parser hands us "docker.io"; the config key is
        // the legacy index.docker.io URL. Normalization should bridge them.
        let c = resolve_credentials(&cfg, "docker.io").unwrap();
        assert_eq!(c.username, "u");
        assert_eq!(c.secret, "p");
    }

    #[test]
    fn resolve_credentials_returns_none_when_empty_auth_entry_and_no_helper() {
        // `docker login` writes `{"auths": {"<reg>": {}}}` when credentials
        // live in a credential store and no creds_store is configured here.
        // With no helper we should fall through to anonymous.
        let json = r#"{
            "auths": {
                "ghcr.io": {}
            }
        }"#;
        let cfg: DockerConfig = serde_json::from_str(json).unwrap();
        assert!(resolve_credentials(&cfg, "ghcr.io").is_none());
    }

    #[test]
    fn resolve_credentials_returns_none_when_registry_unknown() {
        let json = r#"{ "auths": { "ghcr.io": { "auth": "dTpw" } } }"#;
        let cfg: DockerConfig = serde_json::from_str(json).unwrap();
        assert!(resolve_credentials(&cfg, "gcr.io").is_none());
    }

    #[test]
    fn resolve_credentials_skips_empty_auth_string() {
        // `{"auth": ""}` is the marker `docker logout` writes; ignore it.
        let json = r#"{
            "auths": { "ghcr.io": { "auth": "" } }
        }"#;
        let cfg: DockerConfig = serde_json::from_str(json).unwrap();
        assert!(resolve_credentials(&cfg, "ghcr.io").is_none());
    }

    #[test]
    fn docker_config_default_parses_minimal_json() {
        let cfg: DockerConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.auths.is_empty());
        assert!(cfg.creds_store.is_none());
        assert!(cfg.cred_helpers.is_empty());
    }

    #[test]
    fn docker_config_parses_full_shape() {
        let json = r#"{
            "auths": {
                "ghcr.io": { "auth": "dTpw" },
                "myreg.example.com": { "identitytoken": "t" }
            },
            "credsStore": "desktop",
            "credHelpers": {
                "123.dkr.ecr.us-east-1.amazonaws.com": "ecr-login"
            }
        }"#;
        let cfg: DockerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.auths.len(), 2);
        assert_eq!(cfg.creds_store.as_deref(), Some("desktop"));
        assert_eq!(
            cfg.cred_helpers
                .get("123.dkr.ecr.us-east-1.amazonaws.com")
                .map(String::as_str),
            Some("ecr-login")
        );
    }

    // -------- helper-subprocess tests (unix-only shim script) --------

    #[cfg(unix)]
    fn write_helper_shim(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;

        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn run_credential_helper_program_parses_helper_stdout() {
        let tmp = tempfile::tempdir().unwrap();
        // base64("u:p") = dTpw -> we don't actually invoke that path here;
        // helpers emit JSON, not base64.
        let body = r#"#!/bin/sh
read REG
cat <<JSON
{"ServerURL":"$REG","Username":"alice","Secret":"hunter2"}
JSON
"#;
        let prog = write_helper_shim(tmp.path(), "docker-credential-mikebomtest", body);
        let c = run_credential_helper_program(prog.to_str().unwrap(), "ghcr.io").unwrap();
        assert_eq!(c.username, "alice");
        assert_eq!(c.secret, "hunter2");
    }

    #[cfg(unix)]
    #[test]
    fn run_credential_helper_program_returns_none_for_credentials_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let body = r#"#!/bin/sh
echo "credentials not found in native keychain"
exit 1
"#;
        let prog = write_helper_shim(tmp.path(), "docker-credential-mikebomtest-missing", body);
        assert!(run_credential_helper_program(prog.to_str().unwrap(), "ghcr.io").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn run_credential_helper_program_returns_none_on_nonzero_exit() {
        let tmp = tempfile::tempdir().unwrap();
        let body = r#"#!/bin/sh
exit 2
"#;
        let prog = write_helper_shim(tmp.path(), "docker-credential-mikebomtest-broken", body);
        assert!(run_credential_helper_program(prog.to_str().unwrap(), "ghcr.io").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn run_credential_helper_program_returns_none_for_missing_program() {
        assert!(
            run_credential_helper_program(
                "/nonexistent/path/docker-credential-doesnotexist",
                "ghcr.io"
            )
            .is_none()
        );
    }

    // -------- Issue #235 — env-var + credentials-dir resolution --------

    /// `std::env::var` reads process-wide state — tests that mutate
    /// the environment serialize on this lock to avoid cross-test
    /// races. Matches the convention from `cache.rs` and
    /// `attestation/signer.rs`.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Wipe every env var this module's resolvers consult so each
    /// test starts from a clean slate. The per-registry env var
    /// names are constructed dynamically; the caller passes any
    /// per-host names that need clearing too.
    fn clear_creds_env(extra_host_names: &[&str]) {
        std::env::remove_var("MIKEBOM_REGISTRY_USERNAME");
        std::env::remove_var("MIKEBOM_REGISTRY_PASSWORD");
        for host in extra_host_names {
            std::env::remove_var(format!("MIKEBOM_REGISTRY_{host}_USERNAME"));
            std::env::remove_var(format!("MIKEBOM_REGISTRY_{host}_PASSWORD"));
        }
    }

    #[test]
    fn env_credential_returns_per_registry_pair_when_set() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_creds_env(&["GHCR_IO"]);
        std::env::set_var("MIKEBOM_REGISTRY_GHCR_IO_USERNAME", "alice");
        std::env::set_var("MIKEBOM_REGISTRY_GHCR_IO_PASSWORD", "shh");
        let cred = env_credential("ghcr.io").expect("per-registry pair returned");
        assert_eq!(cred.username, "alice");
        assert_eq!(cred.secret, "shh");
        clear_creds_env(&["GHCR_IO"]);
    }

    #[test]
    fn env_credential_normalizes_hostname_to_uppercase_underscores() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_creds_env(&["MY_ECR_AMAZONAWS_COM"]);
        std::env::set_var(
            "MIKEBOM_REGISTRY_MY_ECR_AMAZONAWS_COM_USERNAME",
            "aws-id",
        );
        std::env::set_var(
            "MIKEBOM_REGISTRY_MY_ECR_AMAZONAWS_COM_PASSWORD",
            "aws-secret",
        );
        let cred = env_credential("my-ecr.amazonaws.com")
            .expect("dotted/dashed hostname mapped to env-safe form");
        assert_eq!(cred.username, "aws-id");
        clear_creds_env(&["MY_ECR_AMAZONAWS_COM"]);
    }

    #[test]
    fn env_credential_falls_back_to_generic_pair_when_per_registry_missing() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_creds_env(&["GHCR_IO"]);
        std::env::set_var("MIKEBOM_REGISTRY_USERNAME", "generic");
        std::env::set_var("MIKEBOM_REGISTRY_PASSWORD", "generic-secret");
        let cred = env_credential("ghcr.io").expect("generic pair returned as fallback");
        assert_eq!(cred.username, "generic");
        clear_creds_env(&["GHCR_IO"]);
    }

    #[test]
    fn env_credential_returns_none_when_only_one_half_set() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_creds_env(&["GHCR_IO"]);
        std::env::set_var("MIKEBOM_REGISTRY_GHCR_IO_USERNAME", "alice");
        // PASSWORD intentionally not set.
        assert!(
            env_credential("ghcr.io").is_none(),
            "partial credential (USERNAME without PASSWORD) treated as no-cred"
        );
        clear_creds_env(&["GHCR_IO"]);
    }

    #[test]
    fn env_credential_per_registry_wins_over_generic() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_creds_env(&["GHCR_IO"]);
        std::env::set_var("MIKEBOM_REGISTRY_USERNAME", "generic");
        std::env::set_var("MIKEBOM_REGISTRY_PASSWORD", "generic-secret");
        std::env::set_var("MIKEBOM_REGISTRY_GHCR_IO_USERNAME", "per-host");
        std::env::set_var("MIKEBOM_REGISTRY_GHCR_IO_PASSWORD", "per-host-secret");
        let cred = env_credential("ghcr.io").expect("per-registry wins precedence");
        assert_eq!(cred.username, "per-host");
        clear_creds_env(&["GHCR_IO"]);
    }

    #[test]
    fn load_docker_config_from_dir_probes_config_json_first() {
        let dir = tempfile::tempdir().unwrap();
        // Write a valid auths entry. We just need to confirm the
        // returned DockerConfig parses correctly from the right
        // file — credential decoding is exercised by other tests.
        let auth_b64 = B64_STANDARD.encode("bob:bobpw");
        let body = format!(
            r#"{{ "auths": {{ "ghcr.io": {{ "auth": "{auth_b64}" }} }} }}"#
        );
        std::fs::write(dir.path().join("config.json"), &body).unwrap();
        let cfg = load_docker_config_from_dir(dir.path()).expect("config.json found");
        assert!(cfg.auths.contains_key("ghcr.io"));
    }

    #[test]
    fn load_docker_config_from_dir_falls_back_to_dockerconfigjson() {
        let dir = tempfile::tempdir().unwrap();
        let auth_b64 = B64_STANDARD.encode("bob:bobpw");
        let body = format!(
            r#"{{ "auths": {{ "private.example.com": {{ "auth": "{auth_b64}" }} }} }}"#
        );
        // Only the K8s `.dockerconfigjson` filename present —
        // `config.json` deliberately absent so we exercise the
        // fallback branch.
        std::fs::write(dir.path().join(".dockerconfigjson"), &body).unwrap();
        let cfg = load_docker_config_from_dir(dir.path())
            .expect(".dockerconfigjson found via filename fallback");
        assert!(cfg.auths.contains_key("private.example.com"));
    }

    #[test]
    fn load_docker_config_from_dir_returns_none_when_no_filename_matches() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            load_docker_config_from_dir(dir.path()).is_none(),
            "empty directory should return None (caller falls through to default config)"
        );
    }

    #[test]
    fn load_docker_config_from_dir_skips_malformed_and_tries_next() {
        let dir = tempfile::tempdir().unwrap();
        // First-probe file is garbage; second-probe file is valid.
        std::fs::write(dir.path().join("config.json"), b"not-json").unwrap();
        let auth_b64 = B64_STANDARD.encode("bob:bobpw");
        let body = format!(
            r#"{{ "auths": {{ "ghcr.io": {{ "auth": "{auth_b64}" }} }} }}"#
        );
        std::fs::write(dir.path().join(".dockerconfigjson"), &body).unwrap();
        let cfg = load_docker_config_from_dir(dir.path())
            .expect("malformed config.json skipped; .dockerconfigjson used");
        assert!(cfg.auths.contains_key("ghcr.io"));
    }
}
