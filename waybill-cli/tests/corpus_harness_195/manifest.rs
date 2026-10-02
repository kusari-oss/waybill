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
    pub scan_mode: ScanMode,
}

/// How the harness invokes `waybill sbom scan` for a target.
pub enum ScanMode {
    /// `--offline`: waybill makes no network call. Every target but the
    /// closure one.
    Offline,
    /// `--nix-closure --nix-closure-attr <attr>`, which `--offline`
    /// refuses by design (m1034: resolving a flake can fetch inputs, and
    /// nix's own `--offline` governs substituters, not flake inputs).
    ///
    /// The one sanctioned exception to the offline rule. To keep it as
    /// close to offline as the tier allows: hydration runs
    /// `nix flake archive` first, so every input is in the store before
    /// the scan and nix has nothing to fetch; and the scan turns off
    /// waybill's own online enrichment (`--no-deps-dev
    /// --no-clearly-defined`), so nix is the only process that could
    /// reach the network at all.
    NixClosure { attr: &'static str },
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
    Haskell,
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
        scan_mode: ScanMode::Offline,
    },
    // #879 — the first MULTI-MODULE Go target. `go-cobra` and
    // `pants-example-golang` each have one go.sum, so the path where
    // several modules in one tree declare the same dependency had no corpus
    // coverage at all. Two defects lived there, both fixed in #1066: a
    // shared module recorded only the first declaring go.sum, and every
    // module after the first that shared a `go` version lost its stdlib
    // edge.
    //
    // Measured before adding, offline, in this harness's configuration:
    // 29 go.mod / 29 go.sum, 334 components, every one of the 29 modules
    // linked to stdlib, and `testify@v1.12.1` recorded in all 29 go.sum
    // files. Before #1066 the same tree gave 2 stdlib edges and no
    // component with more than one go.sum occurrence.
    CorpusTarget {
        name: "go-opentelemetry",
        source: SourceKind::Git {
            clone_url: "https://github.com/open-telemetry/opentelemetry-go",
        },
        pinned: PinnedRef::Sha {
            // HEAD of `main` as of 2026-10-01.
            hex: "55899e389dcbc559393069a2a46dac41a88b0772",
        },
        ecosystem: Ecosystem::Go,
        exercises: "#879 multi-module Go: a dependency shared by several modules records \
                    every declaring go.sum, and every module sharing a `go` version keeps \
                    its stdlib edge",
        layer1: super::layer1_assertions::go_opentelemetry_layer1,
        scan_mode: ScanMode::Offline,
    },
    // The first corpus target that runs `--nix-closure`, so the first to
    // exercise milestones 1034 (closure query), 1035 (closure-derived
    // components, patch evidence) and 1050 (nixpkgs declaration pass) end to
    // end. Until it existed those tiers had unit tests and no corpus
    // coverage, because every target ran `--offline`, which the tier refuses.
    //
    // moat is the project milestone 1034/1035 measured against. Measured
    // here before adding (aarch64-darwin, so counts differ from CI's
    // x86_64-linux closure, which the goldens pin): 1,275 derivations,
    // all four roles populated (264 artifact-input / 134 build-tooling /
    // 52 both / 825 unreferenced), 316 closure components appended and 32
    // merged into manifest-derived ones, 187 patches of which 18 distinct
    // CVEs, and 280 members checked by the declaration pass. Its
    // `flake.lock` pins nixpkgs `a799d3e3` — the revision #1051's census
    // was taken at.
    //
    // Not covered: no member of this closure carries
    // `meta.knownVulnerabilities`, so the declaration → VEX path (#1051) is
    // exercised only up to "checked, nothing declared".
    CorpusTarget {
        name: "nix-closure-moat",
        source: SourceKind::Git {
            clone_url: "https://github.com/MercuryTechnologies/moat",
        },
        pinned: PinnedRef::Sha {
            // HEAD of `master` as of 2026-10-02 (last commit 2026-06-11).
            hex: "d06905558ba68b49b17f36c875e97f7234a9c19b",
        },
        ecosystem: Ecosystem::Haskell,
        exercises: "--nix-closure end to end: closure classification and roles (m1034), \
                    closure-derived components and patch evidence (m1035), and the nixpkgs \
                    declaration pass's coverage record (m1050)",
        layer1: super::layer1_assertions::nix_closure_moat_layer1,
        scan_mode: ScanMode::NixClosure { attr: "default" },
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
        scan_mode: ScanMode::Offline,
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
        scan_mode: ScanMode::Offline,
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
        scan_mode: ScanMode::Offline,
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
        scan_mode: ScanMode::Offline,
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
        scan_mode: ScanMode::Offline,
    },
    // Pants example repos — permanent regression gates for m223 (pex-lockfile
    // reader), m224 (coursier-JVM), m226 (Pants Go enricher), m672 (front-
    // matter tolerance + `[python.resolves]` map), m673 (repo-root +
    // `lockfiles/` discovery), m674 (uv.lock reader + Pants FR-002 fallback).
    // Forked into kusari-sandbox/* as insurance against upstream force-push
    // or deletion; refresh via `git fetch upstream && git merge upstream/main`
    // in each fork, then bump the SHA below.
    CorpusTarget {
        name: "pants-example-python",
        source: SourceKind::Git {
            clone_url: "https://github.com/kusari-sandbox/example-python",
        },
        pinned: PinnedRef::Sha {
            // Fork of pantsbuild/example-python HEAD as of 2026-09-02
            hex: "e8cfd79b5e2670b242d2e680515473fa48a2b6b2",
        },
        ecosystem: Ecosystem::Python,
        exercises: "m673 US1 (repo-root `python-default.lock` discovery) + m223 Pants pex-lockfile reader",
        layer1: super::layer1_assertions::pants_example_python_layer1,
        scan_mode: ScanMode::Offline,
    },
    CorpusTarget {
        name: "pants-example-django",
        source: SourceKind::Git {
            clone_url: "https://github.com/kusari-sandbox/example-django",
        },
        pinned: PinnedRef::Sha {
            // Fork of pantsbuild/example-django HEAD as of 2026-09-02
            hex: "7da716ffb9af72c9ab5d10d43c221ae7d6469ad9",
        },
        ecosystem: Ecosystem::Python,
        exercises: "m673 US2 (`lockfiles/python-default.lock` discovery) + m223 Pants pex-lockfile reader",
        layer1: super::layer1_assertions::pants_example_django_layer1,
        scan_mode: ScanMode::Offline,
    },
    // Feature 676 (issue #756 fix) — re-enable pants-example-jvm now that
    // the m224 reader accepts both string and coord-table shapes for the
    // `dependencies` field. Fork was created during PR #757 preparation.
    CorpusTarget {
        name: "pants-example-jvm",
        source: SourceKind::Git {
            clone_url: "https://github.com/kusari-sandbox/example-jvm",
        },
        pinned: PinnedRef::Sha {
            // Fork of pantsbuild/example-jvm HEAD as of 2026-09-02
            hex: "675ee75d36f2c1b096b0def51efcfffd02bd1251",
        },
        ecosystem: Ecosystem::JavaMaven,
        exercises: "m224 Pants coursier-JVM reader (3rdparty/jvm/default.lock) — \
                    unblocked by #676 (coord-table directDependencies + \
                    dependencies fix)",
        layer1: super::layer1_assertions::pants_example_jvm_layer1,
        scan_mode: ScanMode::Offline,
    },
    CorpusTarget {
        name: "pants-example-golang",
        source: SourceKind::Git {
            clone_url: "https://github.com/kusari-sandbox/example-golang",
        },
        pinned: PinnedRef::Sha {
            // Fork of pantsbuild/example-golang HEAD as of 2026-09-02
            hex: "048c22e53f0fac68a4b1d49e1c99b8ce6746cf0a",
        },
        ecosystem: Ecosystem::Go,
        exercises: "m226 Pants Go enricher + m053/m055 Go go.sum reader",
        layer1: super::layer1_assertions::pants_example_golang_layer1,
        scan_mode: ScanMode::Offline,
    },
    // Feature 675 — Pants JavaScript / npm regression gate.
    // Locks in current behavior of the standard npm reader stack
    // (m066 + m147 + m180) against a Pants-managed JS monorepo.
    // Issue #760 tracks the follow-up "option A" pants_js enricher;
    // this entry is the "option B" corpus-only regression gate.
    // Layer 2 goldens are JS-filtered per FR-008 clarification.
    CorpusTarget {
        name: "pants-example-javascript",
        source: SourceKind::Git {
            clone_url: "https://github.com/kusari-sandbox/example-javascript",
        },
        pinned: PinnedRef::Sha {
            // Fork of pantsbuild/example-javascript HEAD as of 2026-09-03
            hex: "da76d5dbb407d82c136cfe8f18dc06f3c8a440e5",
        },
        ecosystem: Ecosystem::Npm,
        exercises: "npm reader stack (m066 + m147 + m180) against a \
                    Pants-managed JavaScript monorepo — regression-locks \
                    issue #760 option-B behavior",
        layer1: super::layer1_assertions::pants_example_javascript_layer1,
        scan_mode: ScanMode::Offline,
    },
    // #898 — Haskell source target. The FIRST Haskell target in either
    // corpus; its absence is what let the #891 defects live, and the gap
    // recurred as #937, #936, #938 and #943 in a single week, each found by
    // hand rather than by a gate.
    //
    // Pinned upstream by SHA rather than through a kusari-sandbox mirror,
    // matching go-cobra / rust-ripgrep / npm-express / python-flask /
    // maven-guice. A commit SHA is already immutable; the Pants examples are
    // mirrored for reasons specific to them.
    //
    // Verified at this revision before pinning: no `cabal.project.freeze` and
    // no `stack.yaml.lock` (so the design-tier path is exercised, not a
    // lockfile path); six `*.cabal` manifests across six directories, one
    // reached through a `benchmarks/examples -> ../examples` symlink; eight
    // `build-depends:` blocks and five conditional branches in the root
    // manifest; 58 distinct declared dependencies, 48 of them in `aeson.cabal`
    // alone. That denominator is what SC-002 measures against.
    CorpusTarget {
        name: "haskell-aeson",
        source: SourceKind::Git { clone_url: "https://github.com/haskell/aeson" },
        pinned: PinnedRef::Sha {
            // v2.3.2.0 — resolved via `git ls-remote --tags https://github.com/haskell/aeson v2.3.2.0`
            hex: "682162c66d26a770fbfb6271c797e646bc5c4f2e",
        },
        ecosystem: Ecosystem::Haskell,
        exercises: "m143 .cabal design-tier emission + #936 cross-manifest constraint union + #938 per-dependency lockfile scoping + #943 case-preserving Hackage identifiers",
        layer1: super::layer1_assertions::haskell_aeson_layer1,
        scan_mode: ScanMode::Offline,
    },
    // The SECOND corpus target that resolves through nixpkgs, and the reason
    // it exists: until it did, `haskell-language-server` was the only one, so
    // every statement about nixpkgs Haskell resolution — including "100%
    // agreement with `nix eval`" after #1032 and #1033 — was a statement about
    // one project rather than about waybill.
    //
    // It was chosen for three properties, each verified before it was added:
    //
    //   * a DIFFERENT pinned nixpkgs revision (567a49d1…, against HLS's
    //     cbb5cf35…), so a defect specific to one revision's package set
    //     cannot hide behind agreement on the other;
    //
    //   * a DIFFERENT `cabal.project` shape. HLS lists directories
    //     explicitly; this declares `packages: code/*/*.cabal`, a two-wildcard
    //     glob. Issue #1032's filter has to handle both, and the glob arm was
    //     otherwise unexercised by any corpus target;
    //
    //   * a large TRANSITIVE population — 59 declared against 189 transitive,
    //     1,469 closure edges. #1033's defect lived on the transitive path
    //     and waybill's version-disagreement detector still does not cover it,
    //     so that is where an undisclosed wrong answer would appear.
    //
    // Measured against `nix eval` before adding: 217 agree, 4 disclosed,
    // 0 absent, 0 disagreeing.
    CorpusTarget {
        name: "haskell-security-advisories",
        source: SourceKind::Git {
            clone_url: "https://github.com/haskell/security-advisories",
        },
        pinned: PinnedRef::Sha {
            // HEAD of `main` as of 2026-09-28.
            hex: "4dc0b9b921bd71688388b1cd8486c26fde568b76",
        },
        ecosystem: Ecosystem::Haskell,
        exercises: "nixpkgs-backed Haskell resolution at a second revision (#971 oracle \
                    generality), the #1032 cabal.project glob arm (`code/*/*.cabal`), and \
                    the transitive closure path where #1033's defect lived",
        layer1: super::layer1_assertions::haskell_security_advisories_layer1,
        scan_mode: ScanMode::Offline,
    },
    // #969 — the FIRST corpus target that exercises nixpkgs-backed Haskell
    // version resolution. `haskell-aeson` has no `flake.lock` at all, so the
    // whole #947 code path was uncovered by any corpus target; every one of
    // its four defects was caught by hand or by CI, none by a test.
    //
    // The decisive property is the `flake.lock` shape. Verified at the
    // pinned tag (not merely at master):
    //
    //   locked   {owner: NixOS, repo: nixpkgs, rev: cbb5cf35…}
    //   original {owner: NixOS, repo: nixpkgs, ref: nixpkgs-unstable}
    //
    // `original.ref` set with `original.rev` ABSENT, and `locked.rev`
    // present. That is the ordinary shape -- and it is exactly what the
    // #947 gate rejected, resolving 0 of 19 and 0 of 44 on two real
    // repositories. Every fixture written for that milestone encoded the
    // same assumption as the code, so the whole suite agreed with the bug.
    // This target would have failed on its first CI run.
    //
    // Also verified at this revision: `flake.lock` present,
    // `cabal.project.freeze` and `stack.yaml.lock` ABSENT (so the design
    // tier is exercised, not a lockfile path), and `flake.nix` naming five
    // GHC series (ghc96/98/910/912/914), which is what drives the candidate
    // set the boot-library union is taken over.
    //
    // Size was measured before pinning rather than assumed: 25.9 MB against
    // `haskell/aeson`'s 41.5 MB, so this does NOT raise the corpus's largest
    // clone.
    CorpusTarget {
        name: "haskell-language-server",
        source: SourceKind::Git {
            clone_url: "https://github.com/haskell/haskell-language-server",
        },
        pinned: PinnedRef::Sha {
            // 2.15.0.0 -- resolved via
            // `git ls-remote --tags https://github.com/haskell/haskell-language-server 2.15.0.0`
            hex: "1b4b3c6bdd2bf8d1e1182e2e770f5dea9198db80",
        },
        ecosystem: Ecosystem::Haskell,
        exercises: "m926 nixpkgs-backed Haskell version resolution (#947) -- \
                    the flake.lock gate, package-set version + native SHA-256 \
                    assignment, boot-library classification, and the #973 \
                    document-scope resolution record",
        layer1: super::layer1_assertions::haskell_language_server_layer1,
        scan_mode: ScanMode::Offline,
    },
];

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
    if !super::harness::env_gate() {
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
