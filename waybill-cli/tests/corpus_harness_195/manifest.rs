//! Corpus manifest — data-model.md Entity 1.
//!
//! Each entry pins one publicly-reachable source (git repo or OCI image)
//! and binds it to a Layer 1 assertion function. Public-only constraint
//! (FR-003) is enforced at test-time by `public_only_audit` +
//! `public_hostname_allowlist` + `no_credentials_required`.

use super::harness::{AssertionFailure, EmittedSboms};

/// One corpus target — the manifest is `TARGETS: &[CorpusTarget]`.
pub struct CorpusTarget {
    pub name: &'static str,
    pub source: SourceKind,
    pub pinned: PinnedRef,
    pub ecosystem: Ecosystem,
    pub exercises: &'static str,
    pub layer1: fn(&EmittedSboms) -> Result<(), AssertionFailure>,
}

pub enum SourceKind {
    Git { clone_url: &'static str },
    OciImage { image_ref: &'static str },
}

pub enum PinnedRef {
    Sha { hex: &'static str },
    Digest { algo_hex: &'static str },
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Ecosystem {
    Go,
    Rust,
    Npm,
    Python,
    JavaMaven,
    PolyglotImage,
}

/// The corpus manifest — populated per US1 / US2 per tasks.md.
/// Empty until at least T017 (go-cobra) lands.
pub const TARGETS: &[CorpusTarget] = &[
    // T017 (US1) — Go source target:
    CorpusTarget {
        name: "go-cobra",
        source: SourceKind::Git { clone_url: "https://github.com/spf13/cobra" },
        pinned: PinnedRef::Sha {
            // v1.9.1 — resolved via `git ls-remote --tags https://github.com/spf13/cobra v1.9.1`
            hex: "a655097faf7d54f78933a815984b9919d51a05d2",
        },
        ecosystem: Ecosystem::Go,
        exercises: "m194 US1 (Go stdlib edge synthesis) + m053 main-module version-resolution + m055 transitive-edges",
        layer1: super::layer1_assertions::go_cobra_layer1,
    },
    // T022 (US2) — Rust source target:
    CorpusTarget {
        name: "rust-ripgrep",
        source: SourceKind::Git { clone_url: "https://github.com/BurntSushi/ripgrep" },
        pinned: PinnedRef::Sha {
            // 14.1.1 — resolved via `git ls-remote --tags https://github.com/BurntSushi/ripgrep 14.1.1`
            hex: "0e8390a66fbcf6eeac1aeb0541b367663a597c79",
        },
        ecosystem: Ecosystem::Rust,
        exercises: "m064 cargo main-module + m087 workspace-version + m088 procmacro edges",
        layer1: super::layer1_assertions::rust_ripgrep_layer1,
    },
    // T025 (US2) — npm source target:
    CorpusTarget {
        name: "npm-express",
        source: SourceKind::Git { clone_url: "https://github.com/expressjs/express" },
        pinned: PinnedRef::Sha {
            // v5.1.0 — resolved via `git ls-remote --tags https://github.com/expressjs/express v5.1.0`
            hex: "e99649895f714c9dc9b3538e2cb0f58954f0ecfa",
        },
        ecosystem: Ecosystem::Npm,
        exercises: "m066 npm main-module + m147 peer-edges + m180 optional-dep classification",
        layer1: super::layer1_assertions::npm_express_layer1,
    },
    // T028 (US2) — Python source target:
    CorpusTarget {
        name: "python-flask",
        source: SourceKind::Git { clone_url: "https://github.com/pallets/flask" },
        pinned: PinnedRef::Sha {
            // 3.1.2 — resolved via `git ls-remote --tags https://github.com/pallets/flask 3.1.2`
            hex: "80be49be88b534d2a72ef6bf5ea4aabf89f3305b",
        },
        ecosystem: Ecosystem::Python,
        exercises: "m068 pip main-module + m183 pip extras/optional",
        layer1: super::layer1_assertions::python_flask_layer1,
    },
    // T031 (US2) — Java/Maven source target:
    CorpusTarget {
        name: "maven-guice",
        source: SourceKind::Git { clone_url: "https://github.com/google/guice" },
        pinned: PinnedRef::Sha {
            // 7.0.0 — resolved via `git ls-remote --tags https://github.com/google/guice 7.0.0`
            hex: "b0e1d0fab0167cd555ab8d262333c1a32db7d492",
        },
        ecosystem: Ecosystem::JavaMaven,
        exercises: "m070 Maven main-module + m085 Maven SPDX dep edges + m184 optional deps",
        layer1: super::layer1_assertions::maven_guice_layer1,
    },
    // T034 (US2) — Polyglot container image target:
    CorpusTarget {
        name: "image-postgres16",
        source: SourceKind::OciImage {
            image_ref: "docker.io/library/postgres:16",
        },
        pinned: PinnedRef::Digest {
            // m196 US2 — resolved 2026-07-15 via:
            //   docker manifest inspect --verbose docker.io/library/postgres:16 \
            //     | jq -r '.[] | select(.Descriptor.platform.architecture == "amd64"
            //         and .Descriptor.platform.os == "linux"
            //         and (.Descriptor.platform.variant // "") == "") | .Descriptor.digest'
            // linux/amd64-specific digest — the nightly runner is ubuntu-latest
            // (amd64), so pinning the amd64 platform descriptor guarantees
            // reproducibility. Multi-arch top-level manifest would drift per-runner.
            algo_hex: "sha256:4c9405bdf36a7a96c5637acec4b39545681f0d2154a7b1e622890607aad6bf56",
        },
        ecosystem: Ecosystem::PolyglotImage,
        exercises: "deb reader + Go BuildInfo (gosu bin) + m177 TransitiveEdgesUnresolvable classifier",
        layer1: super::layer1_assertions::image_postgres16_layer1,
    },
];

// -----------------------------------------------------------------------
// Manifest audit tests (US3 — T037 / T038 / T038a)
// -----------------------------------------------------------------------

#[test]
fn public_only_audit() {
    let mut offenders: Vec<&str> = Vec::new();
    for t in TARGETS {
        let ref_str = match &t.source {
            SourceKind::Git { clone_url } => *clone_url,
            SourceKind::OciImage { image_ref } => *image_ref,
        };
        if ref_str.to_ascii_lowercase().contains("kusari") {
            offenders.push(t.name);
        }
    }
    assert!(
        offenders.is_empty(),
        "m195 FR-003 violation — corpus targets reference Kusari-internal \
         hostnames: {offenders:?}. All corpus targets MUST be publicly-\
         reachable per spec §User Story 3.",
    );
}

#[test]
fn public_hostname_allowlist() {
    const ALLOWED_HOSTS: &[&str] = &[
        "github.com",
        "docker.io",
        "registry-1.docker.io",
        "ghcr.io",
    ];
    let mut offenders: Vec<(String, String)> = Vec::new();
    for t in TARGETS {
        let (raw, host) = match &t.source {
            SourceKind::Git { clone_url } => {
                let host = extract_host_from_url(clone_url).unwrap_or_default();
                ((*clone_url).to_string(), host)
            }
            SourceKind::OciImage { image_ref } => {
                let host = extract_host_from_image_ref(image_ref).unwrap_or_default();
                ((*image_ref).to_string(), host)
            }
        };
        if !ALLOWED_HOSTS.iter().any(|allowed| host == *allowed) {
            offenders.push((t.name.to_string(), format!("{raw} → host={host}")));
        }
    }
    assert!(
        offenders.is_empty(),
        "m195 FR-003 hostname-allowlist violation: {offenders:?}",
    );
}

/// FR-004 (no auth credentials): for each Git target, spawn
/// `git ls-remote <clone_url>` with the credential-helpers disabled
/// and HOME redirected to an empty tmpdir. Anonymous public access
/// MUST be sufficient. OCI-image targets are exempt (public
/// Docker Hub images are pullable-by-digest without auth by definition).
#[test]
fn no_credentials_required() {
    if !super::harness::env_gate() {
        println!("skipping: MIKEBOM_RUN_PUBLIC_CORPUS not set (no-credentials probe hits the public network)");
        return;
    }
    use std::process::Command;
    let empty_home = tempfile::tempdir().expect("tempdir");
    let mut failures: Vec<(String, String)> = Vec::new();
    for t in TARGETS {
        let SourceKind::Git { clone_url } = &t.source else {
            continue;
        };
        let output = Command::new("git")
            .arg("ls-remote")
            .arg(*clone_url)
            .env_clear()
            .env("HOME", empty_home.path())
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "/bin/false")
            .env("SSH_ASKPASS", "/bin/false")
            .output()
            .expect("git binary must be on PATH for corpus tests");
        if !output.status.success() {
            failures.push((
                t.name.to_string(),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "m195 FR-004 violation — the following git targets required credentials for a public-clone probe: {failures:#?}",
    );
}

/// FR-002 / SC-002 — cross-ecosystem coverage assertion.
#[test]
fn cross_ecosystem_coverage_check() {
    use std::collections::HashSet;
    let present: HashSet<&Ecosystem> = TARGETS.iter().map(|t| &t.ecosystem).collect();
    let required = [
        Ecosystem::Go,
        Ecosystem::Rust,
        Ecosystem::Npm,
        Ecosystem::Python,
        Ecosystem::JavaMaven,
        Ecosystem::PolyglotImage,
    ];
    let missing: Vec<&Ecosystem> = required.iter().filter(|e| !present.contains(e)).collect();
    assert!(
        missing.is_empty(),
        "m195 FR-002 violation — missing ecosystem coverage: {missing:?}",
    );
}

// -----------------------------------------------------------------------
// URL parsing helpers (stdlib-only)
// -----------------------------------------------------------------------

fn extract_host_from_url(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://")?.1;
    Some(
        after_scheme
            .split(['/', ':'])
            .next()?
            .to_ascii_lowercase(),
    )
}

fn extract_host_from_image_ref(image_ref: &str) -> Option<String> {
    let first = image_ref.split('/').next()?;
    // If the first segment has a dot, colon, or is "localhost", treat as
    // a registry host; otherwise the image is `library/...` under Docker
    // Hub implicitly.
    if first.contains('.') || first.contains(':') || first == "localhost" {
        Some(first.to_ascii_lowercase())
    } else {
        Some("docker.io".to_string())
    }
}
