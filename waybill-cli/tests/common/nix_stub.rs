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
    /// #1458 — per-output listings for the local store, as `(output store
    /// basename, listing)`. When non-empty it replaces [`Self::runtime`]:
    /// `path-info -r` prints the union of the listings of the outputs its
    /// argv names, and misses when it names none of them.
    pub runtime_by_output: Vec<(&'a str, String)>,
    /// #1455 — the references `nix path-info -r --json` reports, as
    /// `(referrer basename, referenced basename)`. A listed path no pair
    /// names references nothing. The listing itself still says which paths
    /// the closure holds; under `--json` the stub prints it in nix's
    /// format-1 shape (an object keyed by full store path, each value
    /// carrying `references` as full store paths), as nix 2.34 prints for a
    /// bare `--json`.
    pub runtime_refs: Vec<(&'a str, &'a str)>,
    /// #1458 — what the output-selection evaluation (`outputSpecified` /
    /// `meta.outputsToInstall`) prints. `None`: it fails, as an evaluation
    /// error would.
    pub selection: Option<&'a str>,
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
    /// Milestone 1283 — what `config show` reports for
    /// `allow-import-from-derivation`. `None` reflects the value the argv
    /// requested, as a nix that supports the setting does; `Some(v)` always
    /// reports `v`, as a nix (or a configuration) that ignores the request.
    pub ifd_reported: Option<&'a str>,
    /// Milestone 1283 — the `sandbox = …` line `config show` prints; `None`
    /// prints none.
    pub sandbox: Option<&'a str>,
    /// Milestone 1283 — the flake evaluates through import-from-derivation:
    /// `derivation show` and the output-selection evaluation of the project's
    /// flake fail with nix's measured refusal
    /// (`fixtures/nix_ifd/cabal2nix-refusal.stderr`) unless their argv
    /// carries `allow-import-from-derivation true`.
    pub ifd_required: bool,
    /// #1455 — under `--json`, a store that does not hold the output answers
    /// `{"<path>":null}` with exit 0, as the corpus run's nix did, instead of
    /// exiting 1 with `path '…' is not valid`.
    pub json_miss_is_null: bool,
    /// Milestone 1283 — `derivation show` of [`Self::attr`] fails with this
    /// stderr instead of printing the closure.
    pub closure_stderr: Option<String>,
    /// #1436 — what the milestone-1050 declaration evaluation (the one that
    /// reads `meta.knownVulnerabilities`) prints: `{pname: [{setId, out,
    /// kv}]}`. `None`: it is unhandled and fails, as before, and the pass
    /// degrades.
    pub declarations: Option<&'a str>,
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
            runtime_by_output: Vec::new(),
            runtime_refs: Vec::new(),
            selection: None,
            substituters: Vec::new(),
            verify_ok: true,
            package_set: None,
            store_dir: None,
            ifd_reported: None,
            sandbox: None,
            ifd_required: false,
            json_miss_is_null: false,
            closure_stderr: None,
            declarations: None,
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
    fn script(&self, log_path: &Path, aux: &Path) -> String {
        let log = log_path.display();
        let rows_case = match self.rows {
            Rows::Fail => {
                "  *rowIn*) echo 'error: stub refuses this evaluation' >&2; exit 1 ;;".to_string()
            }
            Rows::Json(rows) => format!("  *rowIn*) cat <<'ROWSEOF'\n{rows}\nROWSEOF\n  ;;"),
        };
        let declarations_case = match self.declarations {
            Some(json) => format!("  \"eval\"*knownVulnerabilities*) cat <<'DECLEOF'\n{json}\nDECLEOF\n  ;;"),
            None => String::new(),
        };
        let attr = self.attr;
        let closure = self.closure;
        let subs = self.substituters.join(" ");
        // nix's own wording for an output a store does not hold (q2 e).
        let miss = r#"eval "last=\${$#}"; echo "don't know how to build these paths:" >&2; echo "error: path '$last' is not valid" >&2; exit 1"#;
        let null_miss = format!(
            r#"eval "last=\${{$#}}"; case "$*" in *--json*) printf '{{"%s":null}}\n' "$last"; exit 0 ;; esac; {miss}"#
        );
        let miss: &str = if self.json_miss_is_null { &null_miss } else { miss };
        let by_output: String = self
            .runtime_by_output
            .iter()
            .enumerate()
            .map(|(i, (base, listing))| {
                format!(
                    "case \"$* \" in *\"/{base} \"*) hit=1; cat <<'RT{i}EOF'\n{listing}\nRT{i}EOF\n ;; esac\n"
                )
            })
            .collect();
        let path_info = match (&self.runtime, &self.runtime_store) {
            _ if !self.runtime_by_output.is_empty() => format!(
                "case \"$*\" in *--store*) {miss} ;; esac\nhit=0\n{by_output}[ \"$hit\" = 1 ] || {{ {miss}; }}"
            ),
            (None, _) => miss.to_string(),
            (Some(listing), None) => format!(
                "case \"$*\" in *--store*) {miss} ;; *) cat <<'RTEOF'\n{listing}\nRTEOF\n ;; esac"
            ),
            (Some(listing), Some(url)) => format!(
                "case \"$*\" in *\"--store {url} \"*) cat <<'RTEOF'\n{listing}\nRTEOF\n ;; *) {miss} ;; esac"
            ),
        };
        // #1455 — the listing, unioned and de-duplicated, as `--json` prints
        // it. Rendered from the listing so every existing listing serves both
        // forms; the references come from `refs.txt` (see `run`).
        let path_info = format!(
            "tmp='{listing}'\n( {path_info} ) > \"$tmp\" || exit $?\n\
             case \"$(head -c 1 \"$tmp\")\" in '{{') cat \"$tmp\"; exit 0 ;; esac\n\
             case \"$*\" in *--json*) awk -v refs='{refs}' '\
             BEGIN {{ while ((getline l < refs) > 0) {{ split(l, a, \" \"); \
             r[a[1]] = r[a[1]] (r[a[1]] == \"\" ? \"\" : \",\") \"\\\"/nix/store/\" a[2] \"\\\"\" }} }} \
             NF {{ p = $1; if (seen[p]++) next; b = p; sub(/.*\\//, \"\", b); \
             out = out (n++ ? \",\" : \"\") \"\\\"\" p \"\\\":{{\\\"references\\\":[\" r[b] \"]}}\" }} \
             END {{ print \"{{\" out \"}}\" }}' \"$tmp\" ;; *) cat \"$tmp\" ;; esac",
            refs = aux.join("refs.txt").display(),
            listing = aux.join("path-info.out").display(),
        );
        let selection = match self.selection {
            Some(json) => format!("cat <<'SELEOF'\n{json}\nSELEOF\n"),
            None => "echo 'error: stub refuses this evaluation' >&2; exit 1".to_string(),
        };
        // Milestone 1283 — `config show` reflects the requested value unless
        // the test overrides it, and prints a sandbox line when asked.
        let ifd_line = match self.ifd_reported {
            Some(v) => format!("echo 'allow-import-from-derivation = {v}'"),
            None => "case \"$*\" in *'allow-import-from-derivation true'*) \
                     echo 'allow-import-from-derivation = true' ;; \
                     *) echo 'allow-import-from-derivation = false' ;; esac"
                .to_string(),
        };
        let sandbox_line = match self.sandbox {
            Some(v) => format!("; echo 'sandbox = {v}'; echo 'sandbox-fallback = true'"),
            None => String::new(),
        };
        // Gates the calls that force the package: the closure and the output
        // selection. Listing attribute names forces no value, so it needs no
        // import-from-derivation and is not gated.
        let ifd_gate = if self.ifd_required {
            format!(
                "case \"$*\" in \"derivation show\"*'{project}#'*|\"eval\"*outputSpecified*'{project}#'*) \
                 case \"$*\" in *'allow-import-from-derivation true'*) ;; \
                 *) cat '{refusal}' >&2; exit 1 ;; esac ;; esac\n",
                project = self.project.display(),
                refusal = aux.join("ifd-refusal.stderr").display(),
            )
        } else {
            String::new()
        };
        let closure_arm = match &self.closure_stderr {
            Some(_) => format!("cat '{}' >&2; exit 1", aux.join("closure.stderr").display()),
            None => format!("cat <<'CLOSUREEOF'\n{closure}\nCLOSUREEOF\n"),
        };
        let verify = if self.verify_ok {
            "exit 0".to_string()
        } else {
            r#"eval "last=\${$#}"; echo "path '$last' is untrusted" >&2; exit 2"#.to_string()
        };
        format!(
            r##"#!/bin/sh
echo "$*" >> '{log}'
{ifd_gate}case "$*" in
  *builtins.currentSystem*) printf '%s' 'aarch64-darwin' ;;
  "config show substituters"*) echo '{subs}' ;;
  "config show"*) echo 'allow-dirty = true'; {ifd_line}{sandbox_line}; echo 'substituters = {subs}' ;;
  "path-info"*) {path_info} ;;
  "store verify"*) {verify} ;;
  "eval"*outputSpecified*) {selection} ;;
{declarations_case}
{rows_case}
  "derivation show"*"#{attr}") {closure_arm}
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
        // Milestone 1283 — stderr the stub replays is passed through files:
        // nix's messages are full of single quotes a heredoc would mangle.
        let refusal = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/nix_ifd/cabal2nix-refusal.stderr");
        std::fs::copy(&refusal, dir.path().join("ifd-refusal.stderr")).expect("copy refusal");
        let refs: String =
            self.runtime_refs.iter().map(|(from, to)| format!("{from} {to}\n")).collect();
        std::fs::write(dir.path().join("refs.txt"), refs).expect("write refs");
        if let Some(e) = &self.closure_stderr {
            std::fs::write(dir.path().join("closure.stderr"), e).expect("write closure stderr");
        }
        std::fs::write(&nix, self.script(&dir.path().join("nix.log"), dir.path()))
            .expect("write stub");
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
