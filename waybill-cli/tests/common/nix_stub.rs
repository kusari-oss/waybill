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

/// The nixpkgs cache the Haskell tiers read, seeded empty so no test fetches.
fn seed_cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let rev = dir.path().join(REV);
    std::fs::create_dir_all(&rev).expect("mkdir rev");
    std::fs::write(rev.join("hackage-packages.nix"), "{ }\n").expect("write");
    for s in ["9.4.x", "9.6.x", "9.8.x", "9.10.x", "9.12.x"] {
        std::fs::write(rev.join(format!("configuration-ghc-{s}.nix")), "{ }\n").expect("write");
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
        format!(
            r##"#!/bin/sh
echo "$*" >> '{log}'
case "$*" in
  *builtins.currentSystem*) printf '%s' 'aarch64-darwin' ;;
  "config show"*) echo 'allow-dirty = true'; echo 'allow-import-from-derivation = false' ;;
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
        let cache = seed_cache();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir bin");
        let nix = bin.join("nix");
        std::fs::write(&nix, self.script(&dir.path().join("nix.log"))).expect("write stub");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&nix, std::fs::Permissions::from_mode(0o755))
                .expect("chmod stub");
        }

        let cdx_path = dir.path().join("out.cdx.json");
        let spdx23_path = dir.path().join("out.spdx23.json");
        let spdx3_path = dir.path().join("out.spdx3.json");
        let out = Command::new(env!("CARGO_BIN_EXE_waybill"))
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
