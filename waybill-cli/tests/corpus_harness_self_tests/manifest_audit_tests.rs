#[allow(unused_imports)]
use crate::corpus_harness_195::harness::{AssertionFailure, EmittedSboms};
use crate::corpus_harness_195::manifest::*;

// -----------------------------------------------------------------------
// Manifest audit tests (US3 — T037 / T038 / T038a)
// -----------------------------------------------------------------------

#[test]
fn public_only_audit() {
    // FR-003 intent: reject Kusari-internal (private) hostnames. The
    // `kusari-sandbox` GitHub org is a PUBLIC fork host used for
    // pantsbuild/example-* mirrors (insurance against upstream force-push
    // or deletion) — those URLs are exempt.
    let mut offenders: Vec<&str> = Vec::new();
    for t in TARGETS {
        let ref_str = match &t.source {
            SourceKind::Git { clone_url } => *clone_url,
            SourceKind::OciImage { image_ref } => *image_ref,
        };
        let lower = ref_str.to_ascii_lowercase();
        if lower.contains("kusari")
            && !lower.starts_with("https://github.com/kusari-sandbox/")
        {
            offenders.push(t.name);
        }
    }
    assert!(
        offenders.is_empty(),
        "m195 FR-003 violation — corpus targets reference Kusari-internal \
         hostnames: {offenders:?}. All corpus targets MUST be publicly-\
         reachable per spec §User Story 3. Public forks under \
         `github.com/kusari-sandbox/*` are exempt.",
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
    if !crate::corpus_harness_195::harness::env_gate() {
        println!("skipping: WAYBILL_RUN_PUBLIC_CORPUS not set (no-credentials probe hits the public network)");
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

/// #978 — every target must have its three goldens committed.
///
/// `compare_golden` writes the golden and returns `Ok` when none exists, so a
/// target added without goldens **passes silently, forever**: CI discards the
/// workspace each run, so it re-writes and re-passes every night while
/// comparing nothing. That is not hypothetical — `haskell-language-server`
/// landed in #977 with no goldens and reported `ok`; they arrived only in
/// #979.
///
/// The silent-write behaviour is reasonable for bootstrapping a new target.
/// What was missing is anything that notices the bootstrap never finished.
/// This runs in the DEFAULT cargo lane — not behind
/// `WAYBILL_RUN_PUBLIC_CORPUS` — so it fires at PR time on the machine of
/// whoever adds the target, rather than nightly and unread.
///
/// It is three `Path::exists` per target: no network, no scan, no fixtures.
///
/// Skipped in regen mode, and only there. A new target's goldens are produced
/// by a `regen_goldens=true` dispatch, and this test shares a binary with the
/// corpus targets — so firing during that run aborts it before it can write
/// the very files it is demanding, and the target can never be added at all.
/// The guard's whole point is to notice a bootstrap that never FINISHED, so
/// the one run that performs the bootstrap is the one run it must not block.
/// Every verify run — the PR lane and nightly, which is where an unfinished
/// bootstrap would otherwise hide — still fires.
#[test]
fn every_target_has_committed_goldens() {
    if std::env::var("WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS").as_deref() == Ok("1") {
        eprintln!(
            "#978 guard skipped: regen mode writes the goldens this test requires. \
             It fires on every verify run, which is where a bootstrap that never \
             finished would otherwise go unnoticed."
        );
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/public_corpus");
    let mut missing: Vec<String> = Vec::new();
    for t in TARGETS {
        for f in ["cdx.json", "spdx-2.3.json", "spdx-3.json"] {
            let p = root.join(t.name).join(f);
            if !p.exists() {
                missing.push(format!("{}/{f}", t.name));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "#978: {} golden file(s) are missing, so those targets pass while \
         comparing nothing:\n  {}\n\n\
         Generate them through CI -- `gh workflow run \"Public corpus regression\" \
         -f branch=<your-branch> -f regen_goldens=true`, then install the \
         `corpus-goldens-regen` artifact. Never locally: see rule zero in \
         docs/development/refreshing-corpus-goldens.md.",
        missing.len(),
        missing.join("\n  ")
    );
}

/// Feature 1283 — `recall-truth.yml` reads its targets from
/// `specs/1283-nix-allow-ifd/measurements/recall/targets.tsv`, and the
/// truth it produces is only meaningful for the commit the corpus scans.
/// A pin bumped here and not there would produce truth for the wrong
/// commit; the nightly would catch it through `target-pin`, but only after
/// a dispatch had been spent. This catches it at PR time, with no network.
#[test]
fn recall_targets_table_matches_manifest() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../specs/1283-nix-allow-ifd/measurements/recall/targets.tsv");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("1283: cannot read {}: {e}", path.display()));
    let mut table: Vec<(String, String, String, String)> = Vec::new();
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), 4, "1283: targets.tsv row is not 4 tab-separated columns: {line:?}");
        table.push((cols[0].into(), cols[1].into(), cols[2].into(), cols[3].into()));
    }
    let mut manifest: Vec<(String, String, String, String)> = Vec::new();
    for t in TARGETS {
        let ScanMode::NixClosure { attr, allow_ifd: true } = t.scan_mode else {
            continue;
        };
        let SourceKind::Git { clone_url } = &t.source else {
            panic!("1283: {} allows IFD but is not a git target", t.name);
        };
        let PinnedRef::Sha { hex } = &t.pinned else {
            panic!("1283: {} allows IFD but is not pinned by SHA", t.name);
        };
        manifest.push((t.name.into(), (*clone_url).into(), (*hex).into(), attr.into()));
    }
    table.sort();
    manifest.sort();
    // Positive control: an empty pair of lists would compare equal.
    assert_eq!(manifest.len(), 3, "1283: expected three IFD corpus targets, found {manifest:?}");
    assert_eq!(
        table, manifest,
        "1283: targets.tsv and the manifest's allow_ifd targets disagree -- update both together"
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
