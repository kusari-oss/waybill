//! A stub `nix` for closure-tier integration tests.
//!
//! Milestone 1377 (T005) — extracted from `closure_licenses.rs`, which it now
//! serves unchanged, so that the subject and declared-CPE suites drive the
//! same stub rather than a third copy of it. It adds two degrees of freedom:
//!
//! * **the meta-evaluation rows** are any JSON the test supplies, so a row can
//!   carry `version` and `cpe` (absent, null or present) beside `licenses`;
//! * **the evaluated attribute** is chosen by the test, so a closure can be
//!   served for a NixOS configuration's `…system.build.toplevel` as well as
//!   for `packages.<system>.default`. Which derivation is `graph.root` is a
//!   property of the closure JSON itself: the one no other member consumes.
//!
//! The fixtures are JSON handed to this stub, **never evaluated by a real
//! nix**: on 2026-11-04 evaluating a fixture flake rewrote a committed
//! `flake.lock` and broke 11 tests in an unrelated file.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The revision the fixture `flake.lock` pins.
pub const REV: &str = "a799d3e3886da994fa307f817a6bc705ae538eeb";

/// The attribute a scan evaluates when the test does not name one.
pub const DEFAULT_ATTR: &str = "packages.aarch64-darwin.default";

/// What the stub answers to the milestone-1281 meta evaluation.
#[derive(Clone, Copy)]
pub enum Rows<'a> {
    /// The evaluation succeeds and prints this JSON.
    Json(&'a str),
    /// The evaluation fails. Not a crash and not an empty result: the two are
    /// different outcomes and the document has to tell them apart.
    Fail,
}

/// One stubbed scan.
pub struct NixStub<'a> {
    /// The project directory scanned with `--path`.
    pub project: PathBuf,
    /// `nix derivation show -r` output for [`Self::attr`].
    pub closure: &'a str,
    pub rows: Rows<'a>,
    /// The flakeref suffix (after `#`) whose `derivation show` succeeds.
    pub attr: &'a str,
    /// Milestone 1423 — what `nix path-info -r` lists for the evaluated
    /// output, one store path per line. `None`: every store answers
    /// `path '…' is not valid` (exit 1), as nix does for an output no store
    /// holds.
    pub runtime: Option<String>,
    /// Which store holds [`Self::runtime`]: `None` for the local store, or a
    /// substituter URL, in which case only `--store <url>` answers.
    pub runtime_store: Option<String>,
    /// The configured substituters, printed in `config show`'s full listing
    /// (`substituters = …`) and by `config show substituters`.
    pub substituters: Vec<String>,
    /// Whether `nix store verify` accepts the remote answer. `false` answers
    /// as nix does for an untrusted signature (exit 2).
    pub verify_ok: bool,
    /// The seeded `hackage-packages.nix`, and the text of every
    /// `configuration-ghc-*.nix`. `None` seeds the empty set `{ }`.
    pub package_set: Option<(&'a str, &'a str)>,
    /// `NIX_STORE_DIR` for the scan, so the contents pass reads synthesized
    /// outputs instead of the host's store.
    pub store_dir: Option<PathBuf>,
}

impl Default for NixStub<'_> {
    fn default() -> Self {
        Self {
            project: PathBuf::new(),
            closure: "",
            rows: Rows::Json("[]"),
            attr: DEFAULT_ATTR,
            runtime: None,
            runtime_store: None,
            substituters: Vec::new(),
            verify_ok: true,
            package_set: None,
            store_dir: None,
        }
    }
}

/// The three documents one scan wrote, plus what the binary printed.
pub struct Scan {
    pub cdx: Value,
    pub spdx23: Value,
    pub spdx3: Value,
    pub stdout: String,
    pub stderr: String,
    /// Every argv the stub was invoked with, one per line.
    pub nix_log: String,
    _dir: tempfile::TempDir,
    _cache: tempfile::TempDir,
}

/// The nixpkgs cache the Haskell tiers read, seeded so no test fetches:
/// empty unless the test supplies a package set and compiler configuration.
fn seed_cache(set: Option<(&str, &str)>) -> tempfile::TempDir {
    // Every series the resolver may ask for when a supplied set is used, as
    // `nix_closure_hackage_identity.rs` seeds it; the original five otherwise,
    // so the suites written against this stub see exactly what they did.
    let series: &[&str] = match set {
        Some(_) => &["9.16.x", "9.14.x", "9.12.x", "9.10.x", "9.8.x", "9.6.x", "9.4.x", "9.0.x"],
        None => &["9.4.x", "9.6.x", "9.8.x", "9.10.x", "9.12.x"],
    };
    let (packages, config) = set.unwrap_or(("{ }\n", "{ }\n"));
    let dir = tempfile::tempdir().expect("tempdir");
    let rev = dir.path().join(REV);
    std::fs::create_dir_all(&rev).expect("mkdir rev");
    std::fs::write(rev.join("hackage-packages.nix"), packages).expect("write");
    for s in series {
        std::fs::write(rev.join(format!("configuration-ghc-{s}.nix")), config).expect("write");
    }
    dir
}

impl NixStub<'_> {
    fn script(&self, log_path: &Path) -> String {
        let log = log_path.display();
        let rows_case = match self.rows {
            Rows::Fail => {
                "  *rowIn*) echo 'error: stub refuses this evaluation' >&2; exit 1 ;;".to_string()
            }
            Rows::Json(rows) => format!("  *rowIn*) cat <<'ROWSEOF'\n{rows}\nROWSEOF\n  ;;"),
        };
        let attr = self.attr;
        let closure = self.closure;
        let subs = self.substituters.join(" ");
        // nix's own wording for an output a store does not hold (q2 e).
        let miss = r#"eval "last=\${$#}"; echo "don't know how to build these paths:" >&2; echo "error: path '$last' is not valid" >&2; exit 1"#;
        let path_info = match (&self.runtime, &self.runtime_store) {
            (None, _) => miss.to_string(),
            (Some(listing), None) => format!(
                "case \"$*\" in *--store*) {miss} ;; *) cat <<'RTEOF'\n{listing}\nRTEOF\n ;; esac"
            ),
            (Some(listing), Some(url)) => format!(
                "case \"$*\" in *\"--store {url} \"*) cat <<'RTEOF'\n{listing}\nRTEOF\n ;; *) {miss} ;; esac"
            ),
        };
        let verify = if self.verify_ok {
            "exit 0".to_string()
        } else {
            r#"eval "last=\${$#}"; echo "path '$last' is untrusted" >&2; exit 2"#.to_string()
        };
        format!(
            r##"#!/bin/sh
echo "$*" >> '{log}'
case "$*" in
  *builtins.currentSystem*) printf '%s' 'aarch64-darwin' ;;
  "config show substituters"*) echo '{subs}' ;;
  "config show"*) echo 'allow-dirty = true'; echo 'allow-import-from-derivation = false'; echo 'substituters = {subs}' ;;
  "path-info"*) {path_info} ;;
  "store verify"*) {verify} ;;
{rows_case}
  "derivation show"*"#{attr}") cat <<'CLOSUREEOF'
{closure}
CLOSUREEOF
  ;;
  *"#packages.aarch64-darwin") echo '["default"]' ;;
  *"#packages") echo '["aarch64-darwin"]' ;;
  "derivation show"*) echo "error: flake 'stub' does not provide attribute" >&2; exit 1 ;;
  *) echo 'stub nix: unhandled invocation' >&2; exit 1 ;;
esac
"##
        )
    }

    /// Scan [`Self::project`] in all three formats with the stub first on
    /// `PATH`. `extra` is appended to the argv; pass `--nix-closure` for the
    /// tier to run at all.
    pub fn scan(&self, extra: &[&str]) -> Scan {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().to_path_buf();
        self.run(dir, &out, ["out.cdx.json", "out.spdx23.json", "out.spdx3.json"], extra)
    }

    /// As [`Self::scan`], but writes the three documents into `out_dir` as
    /// `<stem>.cdx.json`, `<stem>.spdx.json` and `<stem>.spdx3.json`, so
    /// several scans can share one output directory.
    pub fn scan_to(&self, out_dir: &Path, stem: &str, extra: &[&str]) -> Scan {
        let dir = tempfile::tempdir().expect("tempdir");
        let names = [
            format!("{stem}.cdx.json"),
            format!("{stem}.spdx.json"),
            format!("{stem}.spdx3.json"),
        ];
        self.run(dir, out_dir, [&names[0], &names[1], &names[2]], extra)
    }

    fn run(&self, dir: tempfile::TempDir, out_dir: &Path, names: [&str; 3], extra: &[&str]) -> Scan {
        let cache = seed_cache(self.package_set);
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir bin");
        let nix = bin.join("nix");
        std::fs::write(&nix, self.script(&dir.path().join("nix.log"))).expect("write stub");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755))
                .expect("chmod stub");
        }

        let cdx_path = out_dir.join(names[0]);
        let spdx23_path = out_dir.join(names[1]);
        let spdx3_path = out_dir.join(names[2]);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_waybill"));
        if let Some(store) = &self.store_dir {
            cmd.env("NIX_STORE_DIR", store);
        }
        let out = cmd
            .args(["sbom", "scan", "--path"])
            .arg(&self.project)
            .args([
                "--no-deep-hash",
                "--format",
                "cyclonedx-json",
                "--format",
                "spdx-2.3-json",
                "--format",
                "spdx-3-json",
                "--output",
            ])
            .arg(format!("cyclonedx-json={}", cdx_path.display()))
            .arg("--output")
            .arg(format!("spdx-2.3-json={}", spdx23_path.display()))
            .arg("--output")
            .arg(format!("spdx-3-json={}", spdx3_path.display()))
            .args(extra)
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("WAYBILL_NIXPKGS_CACHE", cache.path())
            .output()
            .expect("spawn waybill");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            cdx_path.exists(),
            "scan produced no document.\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        let read = |p: &Path| -> Value {
            serde_json::from_slice(&std::fs::read(p).expect("read output")).expect("parse output")
        };
        let nix_log = std::fs::read_to_string(dir.path().join("nix.log")).unwrap_or_default();
        Scan {
            nix_log,
            cdx: read(&cdx_path),
            spdx23: read(&spdx23_path),
            spdx3: read(&spdx3_path),
            stdout,
            stderr,
            _dir: dir,
            _cache: cache,
        }
    }
}
