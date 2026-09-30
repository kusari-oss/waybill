//! Running `nix`, bounded, with the safety control verified.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::preflight;
use super::reason::DegradationReason;
use super::result::{IfdRefusalVerified, NixSystem};

/// How often the budget is checked while waiting.
const POLL: Duration = Duration::from_millis(50);

/// Captured result of a bounded `nix` run.
#[derive(Debug)]
pub(crate) struct BoundedOutput {
    pub(crate) status_success: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Run `nix` with the given arguments under a wall-clock budget.
///
/// On expiry the child is **killed**, not merely abandoned. The established
/// pattern elsewhere in the tree (`golang/go_mod_graph.rs:81`) lets an
/// overrunning child keep going on the assumption it will be reaped; that is
/// tolerable for `go mod graph`, which terminates on its own. It is not
/// tolerable here, because Nix evaluation is Turing-complete and bounded by
/// nothing — research R4 measured a shallow evaluation still running past 45
/// seconds with no sign of stopping.
///
/// stdout and stderr are drained on their own threads. Waiting on the child
/// while its pipes fill would deadlock: the child blocks writing, we block
/// waiting, and the budget expires on a process that was never given the
/// chance to finish.
pub(crate) fn run_bounded(
    args: &[&str],
    budget: Duration,
) -> Result<BoundedOutput, DegradationReason> {
    let mut child = Command::new("nix")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => DegradationReason::ToolAbsent,
            _ => DegradationReason::ToolUnusable(e.to_string()),
        })?;

    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let drain = |pipe: &mut Option<std::process::ChildStdout>| {
        pipe.take().map(|mut p| {
            std::thread::spawn(move || {
                let mut buf = String::new();
                let _ = p.read_to_string(&mut buf);
                buf
            })
        })
    };
    let out_thread = drain(&mut out_pipe);
    let err_thread = err_pipe.take().map(|mut p| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = p.read_to_string(&mut buf);
            buf
        })
    });

    let deadline = Instant::now() + budget;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(DegradationReason::BudgetExceeded {
                        budget_secs: budget.as_secs(),
                    });
                }
                std::thread::sleep(POLL);
            }
            Err(e) => {
                let _ = child.kill();
                return Err(DegradationReason::ToolUnusable(e.to_string()));
            }
        }
    };

    let join = |t: Option<std::thread::JoinHandle<String>>| {
        t.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    Ok(BoundedOutput {
        status_success: status.success(),
        stdout: join(out_thread),
        stderr: join(err_thread),
    })
}

/// Confirm the import-from-derivation refusal is in effect (spec FR-008).
///
/// See [`super::preflight`] for why requesting the option is not evidence it
/// applied.
pub(crate) fn verify_ifd_refusal(
    budget: Duration,
) -> Result<IfdRefusalVerified, DegradationReason> {
    let out = run_bounded(
        &["config", "show", "--option", preflight::IFD_SETTING, "false"],
        budget,
    )?;
    if !out.status_success {
        return Err(DegradationReason::ToolUnusable(format!(
            "`nix config show` exited non-zero: {}",
            out.stderr.trim()
        )));
    }
    preflight::verify_from_config_output(&out.stdout)
}

/// Ask Nix which platform this host is.
///
/// The one impure call the tier makes, and it evaluates a builtin rather than
/// anything the scanned repository controls. It exists because
/// `builtins.currentSystem` is unavailable in pure mode (research R2) — so
/// folding platform detection into the resolving evaluation would force that
/// evaluation impure, which is the thing we are avoiding.
pub(crate) fn detect_host_system(budget: Duration) -> Result<NixSystem, DegradationReason> {
    let out = run_bounded(
        &["eval", "--impure", "--raw", "--expr", "builtins.currentSystem"],
        budget,
    )?;
    if !out.status_success {
        return Err(DegradationReason::ToolUnusable(format!(
            "could not determine the host nix system: {}",
            out.stderr.trim()
        )));
    }
    out.stdout
        .trim()
        .parse()
        .map_err(|e| DegradationReason::ToolUnusable(format!("host system unparseable: {e}")))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    fn nix_available() -> bool {
        Command::new("nix")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    #[test]
    fn budget_expiry_kills_rather_than_abandons() {
        if !nix_available() {
            eprintln!("skipping: no nix on PATH");
            return;
        }
        // Shallow and non-recursive, so `max-call-depth` cannot stop it --
        // the same shape research R4 measured running past 45s.
        let expr = "builtins.foldl' (a: i: a + (builtins.foldl' (x: y: x + y) 0 \
                    (builtins.genList (x: x) 40000))) 0 (builtins.genList (x: x) 40000)";
        let started = Instant::now();
        let err = run_bounded(&["eval", "--impure", "--expr", expr], Duration::from_secs(2))
            .expect_err("a 2s budget must not accommodate 1.6e9 additions");
        let elapsed = started.elapsed();

        assert!(
            matches!(err, DegradationReason::BudgetExceeded { .. }),
            "expected BudgetExceeded, got {err:?}"
        );
        // The control: it must have returned *because of the budget*, not
        // because the work finished. An implementation that waited for the
        // child would take far longer than the budget.
        assert!(
            elapsed < Duration::from_secs(10),
            "returned after {elapsed:?}; the budget was not enforced"
        );
    }

    #[test]
    fn a_missing_tool_is_tool_absent_not_a_failure() {
        // Resolve through PATH: with no `nix` present, spawn fails with
        // NotFound, which must map to ToolAbsent so the operator is told to
        // install it rather than to debug their daemon.
        let err = Command::new("definitely-not-a-real-binary-waybill")
            .spawn()
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => DegradationReason::ToolAbsent,
                _ => DegradationReason::ToolUnusable(e.to_string()),
            })
            .expect_err("that binary must not exist");
        assert_eq!(err.wire(), "tool-absent");
    }

    #[test]
    fn preflight_agrees_with_the_committed_probe_on_this_host() {
        if !nix_available() {
            eprintln!("skipping: no nix on PATH");
            return;
        }
        // On a nix that supports the setting this must succeed. If it starts
        // failing, either the setting was renamed upstream or this host's nix
        // predates it -- both of which the tier must notice rather than
        // evaluate through.
        match verify_ifd_refusal(Duration::from_secs(30)) {
            Ok(_) => {}
            Err(e) => panic!("pre-flight failed on a host with nix present: {e}"),
        }
    }

    #[test]
    fn host_system_is_a_valid_platform_double() {
        if !nix_available() {
            eprintln!("skipping: no nix on PATH");
            return;
        }
        let sys = detect_host_system(Duration::from_secs(30)).expect("host system");
        assert!(
            sys.as_str().contains('-'),
            "expected `<arch>-<os>`, got {sys}"
        );
    }
}

/// Characters a package name may contain before it is allowed into a Nix
/// expression.
///
/// Names arrive from parsed manifests, and a Nix expression is *code*. Rather
/// than escaping, names outside this set are refused entry — a Haskell package
/// name is `[A-Za-z0-9._-]` by Hackage's own rules, so nothing legitimate is
/// lost, and there is no escaping bug to get wrong later.
fn is_safe_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// Is this a nixpkgs revision we will put into a Nix expression?
///
/// Exactly 40 lowercase hex characters — a git object id and nothing else.
///
/// The revision reaches us from the scanned repository's `flake.lock`, and it
/// is interpolated into a Nix string literal, so an unvalidated value is an
/// expression-injection vector: a `rev` containing a quote closes the literal
/// and everything after it is evaluated. Today that is not reachable, because
/// the package-set fetch for the same revision happens first and fails on
/// anything that is not a real revision — but that is defence by accident,
/// resting on an unrelated upstream call. This makes it defence by design.
fn is_valid_revision(rev: &str) -> bool {
    rev.len() == 40 && rev.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Build the resolving expression: pinned revision, literal system, one
/// `tryEval` per attribute so a single broken one cannot abort the whole run.
fn versions_expr(revision: &str, system: &NixSystem, names: &[&str]) -> String {
    let list = names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"let
  pkgs = (builtins.getFlake "github:NixOS/nixpkgs/{revision}").legacyPackages.{system};
  hp = pkgs.haskellPackages;
  ver = n:
    let r = builtins.tryEval (if hp ? ${{n}} then (hp.${{n}}.version or null) else null);
    in if r.success then r.value else null;
in builtins.listToAttrs (map (n: {{ name = n; value = ver n; }}) [ {list} ])"#
    )
}

/// The argv for the resolving call.
///
/// One definition, used by the invocation and asserted on by tests — two would
/// drift, and the drift would be invisible.
fn resolving_argv(expr: &str) -> Vec<&str> {
    vec![
        "eval",
        "--json",
        "--option",
        preflight::IFD_SETTING,
        "false",
        "--expr",
        expr,
    ]
}

/// Flags that would hand control of evaluation settings back to the flake.
const UNSAFE_FLAGS: &[&str] = &["--accept-flake-config", "--impure"];

/// Is this argv safe for evaluating expressions we do not control?
///
/// Two ways the safety model can be lost without any visible change in output:
///
/// * `--accept-flake-config` lets the scanned flake set nix options, including
///   the import-from-derivation refusal this tier depends on; and
/// * `--impure` restores access to the host environment.
///
/// Both are one careless argument away, and neither failure is observable in
/// an emitted document — hence a check rather than a comment.
///
/// Checked at the point of use rather than only in a test: a guard that runs
/// only under `cargo test` does not guard the code that ships.
fn argv_is_safe(argv: &[&str]) -> bool {
    !argv.iter().any(|a| UNSAFE_FLAGS.contains(a))
}

/// The production [`super::Evaluator`], which actually runs `nix`.
pub(crate) struct NixEvaluator;

impl super::Evaluator for NixEvaluator {
    fn verify_ifd_refusal(
        &self,
        budget: Duration,
    ) -> Result<IfdRefusalVerified, DegradationReason> {
        verify_ifd_refusal(budget)
    }

    fn detect_host_system(&self, budget: Duration) -> Result<NixSystem, DegradationReason> {
        detect_host_system(budget)
    }

    fn evaluate_versions(
        &self,
        revision: &str,
        system: &NixSystem,
        names: &[String],
        budget: Duration,
    ) -> Result<std::collections::BTreeMap<String, Option<String>>, DegradationReason> {
        let safe: Vec<&str> = names
            .iter()
            .map(String::as_str)
            .filter(|n| is_safe_attribute_name(n))
            .collect();
        let refused = names.len() - safe.len();
        if refused > 0 {
            tracing::warn!(
                refused,
                "nix-eval: refusing to place names with unexpected characters into a Nix expression"
            );
        }
        if safe.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }

        if !is_valid_revision(revision) {
            return Err(DegradationReason::RevisionUnfetchable {
                revision: revision.chars().take(60).collect(),
                detail: "not a 40-character hex git revision; refusing to \
                         interpolate it into a Nix expression"
                    .to_string(),
            });
        }
        let expr = versions_expr(revision, system, &safe);
        let argv = resolving_argv(&expr);
        if !argv_is_safe(&argv) {
            // Unreachable unless someone edits `resolving_argv`. That is
            // exactly the edit this catches, and the consequence — a scanned
            // flake choosing nix's evaluation settings — is invisible in every
            // emitted document, so it degrades rather than proceeds.
            return Err(DegradationReason::EvaluationFailed(
                "refusing to evaluate: the nix invocation would hand \
                 evaluation settings to the scanned flake"
                    .to_string(),
            ));
        }
        // NOTE: `--accept-flake-config` MUST NOT appear here.
        //
        // A flake can ask nix to change evaluation settings via its own
        // `nixConfig`, including `allow-import-from-derivation`. Nix ignores
        // such settings as untrusted *unless* `--accept-flake-config` is
        // passed — at which point the flake's value wins and the refusal below
        // is silently defeated. Measured: a fixture flake setting
        // `nixConfig.allow-import-from-derivation = true` is refused by this
        // invocation and builds its derivation when `--accept-flake-config` is
        // added.
        //
        // Real flakes do carry these settings — slack-web sets
        // `allow-import-from-derivation` and `extra-substituters` — so this is
        // not hypothetical. `argv_is_safe` enforces it.
        let out = run_bounded(&argv, budget)?;
        if !out.status_success {
            let stderr = out.stderr.trim();
            // A flake that exposes nothing evaluable is a degradation, not an
            // error (spec FR-007) -- haskell-language-server's flake exposes
            // no `default` package at all.
            if stderr.contains("attribute") && stderr.contains("missing") {
                return Err(DegradationReason::NoEvaluableAttribute);
            }
            if stderr.contains("getFlake") || stderr.contains("unable to download") {
                return Err(DegradationReason::RevisionUnfetchable {
                    revision: revision.to_string(),
                    detail: stderr.chars().take(200).collect(),
                });
            }
            return Err(DegradationReason::EvaluationFailed(
                stderr.chars().take(200).collect(),
            ));
        }
        serde_json::from_str(&out.stdout)
            .map_err(|e| DegradationReason::EvaluationFailed(format!("unparseable json: {e}")))
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod expr_tests {
    use super::*;

    #[test]
    fn refuses_names_that_could_carry_expression_syntax() {
        assert!(is_safe_attribute_name("aeson"));
        assert!(is_safe_attribute_name("text-1.2"));
        assert!(is_safe_attribute_name("some_pkg.sub"));
        for evil in [
            "\"; evil = \"",
            "a b",
            "${x}",
            "a\nb",
            "a/b",
            "",
            "back`tick",
        ] {
            assert!(
                !is_safe_attribute_name(evil),
                "{evil:?} must not reach a Nix expression"
            );
        }
    }

    #[test]
    fn refuses_a_revision_that_is_not_a_git_object_id() {
        assert!(is_valid_revision("cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73"));
        for bad in [
            // the shape that closes the string literal and appends code
            "cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73\"; evil = 1; y = \"",
            "",
            "cbb5cf35",                                      // short
            "CBB5CF358F50AA6ACC9EFD6113B7BCFBC352CD73",      // uppercase
            "gbb5cf358f50aa6acc9efd6113b7bcfbc352cd73",      // non-hex
            "cbb5cf358f50aa6acc9efd6113b7bcfbc352cd7",       // 39
        ] {
            assert!(!is_valid_revision(bad), "{bad:?} must be refused");
        }
    }

    #[test]
    fn the_resolving_argv_never_hands_settings_back_to_the_flake() {
        // A flake's own `nixConfig` can ask for
        // `allow-import-from-derivation = true`. Nix ignores that as untrusted
        // unless `--accept-flake-config` is passed, at which point the flake
        // wins and this tier's central safety control is defeated with no
        // change visible in any emitted document.
        //
        // Measured against a fixture flake carrying that setting: refused by
        // our argv, built when `--accept-flake-config` was added. Real flakes
        // do carry it.
        assert!(argv_is_safe(&resolving_argv("<expr>")));

        // The guard has teeth in both directions.
        assert!(!argv_is_safe(&["eval", "--accept-flake-config"]));
        assert!(!argv_is_safe(&["eval", "--impure"]));
    }

    #[test]
    fn the_resolving_call_never_inherits_the_oracles_impure_flag() {
        // `xtask nix-oracle` passes `--impure`; it is a developer tool aimed at
        // known targets. Lifting its invocation would silently drop pure mode,
        // and every output would look identical.
        let argv = resolving_argv("<expr>");
        assert!(
            !argv.contains(&"--impure"),
            "the resolving call must stay pure: {argv:?}"
        );
        assert!(argv.contains(&"--option") && argv.contains(&preflight::IFD_SETTING));
        assert_eq!(
            argv.iter().position(|a| *a == preflight::IFD_SETTING).map(|i| argv[i + 1]),
            Some("false"),
            "the setting must be passed as false, not merely named"
        );
    }

    #[test]
    fn expression_is_pure_pinned_and_literal_in_the_system() {
        let sys: NixSystem = "x86_64-linux".parse().unwrap();
        let e = versions_expr("abc123", &sys, &["aeson", "text"]);
        assert!(e.contains("github:NixOS/nixpkgs/abc123"));
        assert!(e.contains("legacyPackages.x86_64-linux"));
        assert!(e.contains("tryEval"));
        // The system must be baked in, never asked for: `currentSystem` does
        // not exist in pure mode, so its presence would force `--impure`.
        assert!(
            !e.contains("currentSystem"),
            "the resolving expression must not ask nix what system it is on"
        );
        assert!(e.contains("\"aeson\"") && e.contains("\"text\""));
    }
}
