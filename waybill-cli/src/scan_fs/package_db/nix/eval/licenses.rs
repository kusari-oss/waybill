//! Milestone 1281 — licenses for nix closure components (#1346 follow-up).
//!
//! The closure JSON `nix derivation show` produces carries **no license data
//! at all** — no `meta`, no `license`, no `spdx`. `meta` is evaluation-time
//! metadata and never reaches a derivation. So every closure component was
//! emitted unlicensed, and on the measured target 96 of them are licensed by
//! another tool reading the same flake.
//!
//! This module runs a second, pure `nix eval` over the pinned nixpkgs that
//! returns each package's `drvPath`, output path and declared licenses
//! together, and the closure pass joins the result to its components **by
//! exact store path** (spec FR-002).
//!
//! # Why the store path, and not the name
//!
//! Because a name is not an identity here. nixpkgs exposes `autoconf269`,
//! whose `pname` is `autoconf` and whose version is 2.69, alongside
//! `autoconf` at 2.73 (research R5):
//!
//! ```text
//! attr=autoconf     pname=autoconf  drv=31ziv3nb…-autoconf-2.73.drv
//! attr=autoconf269  pname=autoconf  drv=0kkywv8w…-autoconf-2.69.drv
//! ```
//!
//! A name join attaches 2.73's license to 2.69 and nothing in the emitted
//! document says it is wrong. The package name scopes *which* attributes are
//! looked up — it is a hint that bounds the evaluation, never the join key.
//!
//! # Reused controls, not new ones
//!
//! Spec FR-012 forbids a second, parallel set of safety controls. This module
//! adds none: it borrows [`is_safe_attribute_name`], [`argv_is_safe`] and
//! [`run_bounded`] from the m1034 tier, and the import-from-derivation
//! refusal is **already verified once per closure run** by
//! `closure::resolve`, so this path inherits it rather than re-issuing it.

use std::time::Duration;

use super::invoke::{argv_is_safe, is_safe_attribute_name, is_valid_revision, run_bounded};
use super::preflight::IFD_SETTING;
use super::reason::DegradationReason;
use super::result::NixSystem;

/// The package roots searched for a name, in order.
///
/// `pkgs` and the default Haskell set are the measured floor: of 228 joined
/// store paths, 104 came from `pkgs` and 124 from `haskellPackages`, so a
/// Haskell-only root loses nearly half the population (research R6).
///
/// Order matters only for the `attr`/`root` diagnostic fields. Where one store
/// path is reached through several roots, FR-015 requires the licenses to
/// agree, and the join — not this list — enforces that.
const ROOTS: &[(&str, &str)] = &[("pkgs", "pkgs"), ("haskellPackages", "pkgs.haskellPackages")];

/// One license as the pinned revision declares it.
///
/// Both fields are optional because nixpkgs' license attrsets are not uniform:
/// most carry `spdxId`, some carry only a `shortName` (`publicDomain` is in
/// the measured population and SPDX has no identifier for it), and a license
/// may also be stated as a bare string.
///
/// #1404 — nixpkgs also composes licenses with `lib.licenses.{AND,OR,WITH,PLUS}`
/// (`lib/licenses/operators.nix`). Such a value is a tree, projected as a node
/// carrying `op` and its operands in `terms`; a leaf has neither. Both are
/// defaulted, so a row from before #1404 still parses as a leaf.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
pub(crate) struct DeclaredLicenseRaw {
    /// nixpkgs' own `spdxId`. **The authority** where present (spec FR-014) —
    /// re-deriving an identifier from the attribute name would be a second
    /// mapping to fall out of date beside the one nixpkgs already maintains.
    #[serde(default)]
    pub(crate) spdx: Option<String>,
    /// The attribute's `shortName`/`fullName`, or a bare string declaration.
    #[serde(default)]
    pub(crate) name: Option<String>,
    /// The composing operator, as nixpkgs states it: `AND` or `OR` for a
    /// `compound`, `WITH` for an `exception`, `+` for a `plus`. `None` on a
    /// leaf.
    #[serde(default)]
    pub(crate) op: Option<String>,
    /// The operands, in nixpkgs' order: the `licenses` of a compound,
    /// `[license, exception]` for `WITH`, `[license]` for `+`.
    #[serde(default)]
    pub(crate) terms: Vec<DeclaredLicenseRaw>,
}

/// One row of the license evaluation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct LicenseRow {
    /// Store-path **basename** of the derivation, e.g.
    /// `0kky…-autoconf-2.69.drv`. This is the join key (FR-002).
    pub(crate) drv: String,
    /// The output store path.
    ///
    /// Named `out`, never `outPath`: an attrset carrying a key called
    /// `outPath` is string-coerced by `nix eval --json`, which replaces the
    /// whole row with a path string and loses it silently (research R3).
    pub(crate) out: String,
    /// The attribute the row was found under. Diagnostics only — not an
    /// identity, see the module docs.
    pub(crate) attr: String,
    /// Which root the row came from. Diagnostics only.
    pub(crate) root: String,
    /// Licenses exactly as nixpkgs states them, before canonicalisation.
    pub(crate) licenses: Vec<DeclaredLicenseRaw>,
    /// Milestone 1377 — `p.version`, for the name+version identifier join.
    /// Defaulted so a row without it (an older evaluation) still parses.
    #[serde(default)]
    pub(crate) version: Option<String>,
    /// Milestone 1377 — `p.meta.identifiers.cpe` as nixpkgs states it, before
    /// validation (`closure::identifiers::DeclaredCpe`).
    #[serde(default)]
    pub(crate) cpe: Option<String>,
}

/// The Nix bindings that turn `meta.license` into [`DeclaredLicenseRaw`] rows.
///
/// #1404 — `one` recurses through the composite shapes `lib/licenses/
/// operators.nix` builds (`compound`, `exception`, `plus`), keeping nixpkgs'
/// operator and operand order. A leaf is read as before: `spdxId` where nixpkgs
/// states one, else the name it states. Nothing here mints an identifier —
/// nixpkgs' own `toSPDX` writes `LicenseRef-nixos-<shortName>` for a leaf with
/// no `spdxId`, which is why this does not call it (spec FR-003, FR-014).
///
/// An attrset with no `licenseType` is a leaf: hand-written `meta.license`
/// attrsets predate the field. A throw anywhere in the tree is caught by the
/// row's own `deepSeq` guard and costs that row, not the pass. `op` is passed
/// through `str` because the rows are parsed as one JSON array: a non-string
/// there would fail the parse and cost every row, not one.
const LICENSE_PROJECTION: &str = r#"str = v: if builtins.isString v then v else null;
  one = l:
    if builtins.isAttrs l then
      let t = l.licenseType or "simple";
          node = terms: { spdx = null; name = null; op = str (l.operator or null); inherit terms; };
      in
      if t == "compound" then node (map one (if builtins.isList (l.licenses or null) then l.licenses else [ ]))
      else if t == "exception" then node [ (one (l.license or null)) (one (l.exception or null)) ]
      else if t == "plus" then node [ (one (l.license or null)) ]
      else { spdx = l.spdxId or null; name = l.shortName or l.fullName or null; }
    else if builtins.isString l then { spdx = null; name = l; }
    else { spdx = null; name = null; };
  lic = l:
    if builtins.isList l then map one l
    else if l == null then [ ]
    else [ (one l) ];"#;

/// Build the license-resolving expression.
///
/// Modelled on `invoke::versions_expr` — same pinned revision, same literal
/// system, same import-from-derivation refusal — but projecting rows keyed by
/// `baseNameOf drvPath` rather than an attrset keyed by name.
///
/// # The guards, and why there are three
///
/// `builtins.tryEval` catches `throw` and `assert`. It does **not** catch a
/// missing attribute or a type error, and both occur in this population.
/// Measured, each escaping the guard added for the one before it (research
/// R2):
///
/// | Shape | Error | Why `tryEval` misses it |
/// |---|---|---|
/// | `p.drvPath` on a set without it | `attribute 'drvPath' missing` | not a throw |
/// | a `null` alias in `pkgs` | `expected a set but found null` | a type error |
/// | a throw forced during output | varies | `tryEval` forces only to WHNF |
///
/// So: `hasAttr` first, then a structural `isAttrs`/`? drvPath` check, then
/// `tryEval` wrapped around a `deepSeq` — the `deepSeq` **inside** the
/// `tryEval`, so a throw in a lazy field surfaces while the guard still holds
/// instead of during JSON serialisation. A row failing any guard is omitted;
/// none of them may abort the pass (spec FR-013).
fn licenses_expr(revision: &str, system: &NixSystem, names: &[&str]) -> String {
    let list = names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let roots = ROOTS
        .iter()
        .map(|(id, path)| format!("{{ id = \"{id}\"; set = {path}; }}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"let
  pkgs = (builtins.getFlake "github:NixOS/nixpkgs/{revision}").legacyPackages.{system};
  roots = [ {roots} ];
  {LICENSE_PROJECTION}
  rowIn = root: name:
    if ! (builtins.hasAttr name root.set) then [ ] else
    let pr = builtins.tryEval (builtins.getAttr name root.set);
        p = if pr.success then pr.value else null;
    in if ! (builtins.isAttrs p) || ! (p ? drvPath) then [ ] else
    let v = {{ attr = name; root = root.id; drv = baseNameOf p.drvPath;
              out = "${{p}}"; licenses = lic (p.meta.license or null);
              version = p.version or null;
              cpe = let c = builtins.tryEval (p.meta.identifiers.cpe or null);
                    in if c.success then c.value else null; }};
        r = builtins.tryEval (builtins.deepSeq v v);
    in if r.success then [ r.value ] else [ ];
in builtins.concatMap (n: builtins.concatMap (r: rowIn r n) roots) [ {list} ]"#
    )
}

/// The argv for the license call.
///
/// One definition, asserted on by tests — two would drift, and the drift
/// would be invisible in every emitted document.
fn licenses_argv(expr: &str) -> Vec<&str> {
    vec![
        "eval",
        "--json",
        "--option",
        IFD_SETTING,
        "false",
        "--expr",
        expr,
    ]
}

/// Resolve declared licenses for `names` against the pinned revision.
///
/// Returns rows to be joined by store path. An empty result is a legitimate
/// outcome and is **not** an error — the caller records coverage so that a
/// closure whose members declare no licenses stays distinguishable from a
/// lookup that returned nothing (spec FR-008).
pub(crate) fn resolve_licenses(
    revision: &str,
    system: &NixSystem,
    names: &[String],
    budget: Duration,
) -> Result<Vec<LicenseRow>, DegradationReason> {
    let safe: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| is_safe_attribute_name(n))
        .collect();
    let refused = names.len() - safe.len();
    if refused > 0 {
        tracing::warn!(
            refused,
            "nix-closure-licenses: refusing to place names with unexpected characters \
             into a Nix expression"
        );
    }
    if safe.is_empty() {
        return Ok(Vec::new());
    }

    if !is_valid_revision(revision) {
        return Err(DegradationReason::RevisionUnfetchable {
            revision: revision.chars().take(60).collect(),
            detail: "not a 40-character hex git revision; refusing to \
                     interpolate it into a Nix expression"
                .to_string(),
        });
    }

    let expr = licenses_expr(revision, system, &safe);
    let argv = licenses_argv(&expr);
    if !argv_is_safe(&argv) {
        // Unreachable unless someone edits `licenses_argv`. That is exactly
        // the edit this catches, and the consequence — the scanned flake
        // choosing nix's evaluation settings — is invisible in every emitted
        // document, so it degrades rather than proceeds.
        return Err(DegradationReason::EvaluationFailed(
            "refusing to evaluate: the license invocation would hand \
             evaluation settings to the scanned flake"
                .to_string(),
        ));
    }

    let out = run_bounded(&argv, budget)?;
    if !out.status_success {
        return Err(DegradationReason::EvaluationFailed(
            super::stderr::salient_stderr(&out.stderr),
        ));
    }
    Ok(parse_rows(&out.stdout))
}

/// Parse the evaluation's JSON into rows.
///
/// A malformed row is dropped with a warning rather than failing the pass: a
/// license gap is a smaller loss than a failed scan (spec FR-006, FR-013).
fn parse_rows(stdout: &str) -> Vec<LicenseRow> {
    match serde_json::from_str::<Vec<LicenseRow>>(stdout) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "nix-closure-licenses: could not parse the evaluation output; \
                 continuing without licenses from this source"
            );
            Vec::new()
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use std::process::{Command, Stdio};

    use super::*;

    fn nix_available() -> bool {
        Command::new("nix")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    }

    fn system() -> NixSystem {
        "x86_64-linux".parse().unwrap()
    }

    /// T008 — the two repository-influenced values must reach the expression
    /// only in forms that cannot carry expression syntax.
    #[test]
    fn expression_is_pure_pinned_and_literal_in_the_system() {
        let rev = "a".repeat(40);
        let expr = licenses_expr(&rev, &system(), &["hello"]);
        assert!(
            expr.contains(&format!("github:NixOS/nixpkgs/{rev}")),
            "the revision must be pinned into the flakeref"
        );
        assert!(
            expr.contains("legacyPackages.x86_64-linux"),
            "the system must be a literal, not read from the host at eval time"
        );
        assert!(
            !expr.contains("builtins.currentSystem"),
            "currentSystem would make the result host-dependent and needs --impure"
        );
    }

    /// T008 — a revision that is not a git object id never reaches an
    /// expression, because it is interpolated into a string literal and a
    /// quote in it would close that literal.
    #[test]
    fn a_revision_that_is_not_an_object_id_is_refused_before_evaluating() {
        let bad = "\" + builtins.readFile /etc/passwd + \"";
        let err = resolve_licenses(
            bad,
            &system(),
            &["hello".to_string()],
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(err.wire(), "revision-unfetchable");
    }

    /// T009 — FR-011 and FR-012. Both flags are one careless argument away,
    /// and neither failure is observable in an emitted document.
    #[test]
    fn the_license_argv_is_pure_and_never_hands_settings_to_the_flake() {
        let expr = licenses_expr(&"b".repeat(40), &system(), &["hello"]);
        let argv = licenses_argv(&expr);
        assert!(argv_is_safe(&argv), "license argv must stay pure: {argv:?}");
        assert!(!argv.contains(&"--impure"));
        assert!(!argv.contains(&"--accept-flake-config"));
        assert!(
            argv.windows(3)
                .any(|w| w == ["--option", IFD_SETTING, "false"]),
            "the import-from-derivation refusal must be requested on this call too"
        );
    }

    /// T009 — names that could carry expression syntax are dropped before the
    /// expression is built, not escaped into it.
    #[test]
    fn names_that_could_carry_expression_syntax_are_refused() {
        let names = [
            "hello".to_string(),
            "\"; builtins.currentSystem; \"".to_string(),
        ];
        let safe: Vec<&str> = names
            .iter()
            .map(String::as_str)
            .filter(|n| is_safe_attribute_name(n))
            .collect();
        assert_eq!(safe, vec!["hello"]);
    }

    /// An empty name set evaluates nothing at all — no subprocess, no error.
    #[test]
    fn an_empty_name_set_resolves_to_no_rows_without_evaluating() {
        let rows =
            resolve_licenses(&"c".repeat(40), &system(), &[], Duration::from_secs(5)).unwrap();
        assert!(rows.is_empty());
    }

    /// FR-013 — malformed output costs the licenses, never the scan.
    #[test]
    fn malformed_output_yields_no_rows_rather_than_failing() {
        assert!(parse_rows("not json at all").is_empty());
        assert!(parse_rows("").is_empty());
    }

    #[test]
    fn rows_parse_with_both_license_shapes() {
        let rows = parse_rows(
            r#"[{"drv":"abc-hello-1.0.drv","out":"/nix/store/x-hello-1.0","attr":"hello",
                 "root":"pkgs","licenses":[{"spdx":"MIT","name":"mit"},
                                           {"spdx":null,"name":"Public Domain"}]}]"#,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].drv, "abc-hello-1.0.drv");
        assert_eq!(rows[0].licenses[0].spdx.as_deref(), Some("MIT"));
        assert_eq!(rows[0].licenses[1].spdx, None);
        assert_eq!(rows[0].licenses[1].name.as_deref(), Some("Public Domain"));
    }

    /// Milestone 1377 T014 — the row carries `version` and `cpe`, and the
    /// `cpe` is evaluated inside its own `tryEval`, so a package whose
    /// `meta.identifiers` throws loses its CPE and keeps its licenses.
    #[test]
    fn m1377_the_expression_reads_version_and_a_separately_guarded_cpe() {
        let expr = licenses_expr(&"a".repeat(40), &system(), &["hello"]);
        assert!(expr.contains("version = p.version or null;"), "{expr}");
        assert!(
            expr.contains("builtins.tryEval (p.meta.identifiers.cpe or null)"),
            "the cpe needs its own guard: {expr}"
        );
        // Inside the row, and so inside the row's own deepSeq guard too.
        let row_start = expr.find("let v = {").unwrap();
        let row_end = expr.find("r = builtins.tryEval (builtins.deepSeq v v)").unwrap();
        let cpe_at = expr.find("p.meta.identifiers.cpe").unwrap();
        assert!(row_start < cpe_at && cpe_at < row_end);
    }

    /// Milestone 1377 T014 — absent, null and present all parse.
    #[test]
    fn m1377_rows_parse_with_identifier_fields_absent_null_and_present() {
        let rows = parse_rows(
            r#"[{"drv":"a.drv","out":"/o/a","attr":"a","root":"pkgs","licenses":[]},
                {"drv":"b.drv","out":"/o/b","attr":"b","root":"pkgs","licenses":[],
                 "version":null,"cpe":null},
                {"drv":"c.drv","out":"/o/c","attr":"c","root":"pkgs","licenses":[],
                 "version":"10.3p1","cpe":"cpe:2.3:a:openbsd:openssh:10.3p1:*:*:*:*:*:*:*"}]"#,
        );
        assert_eq!(rows.len(), 3);
        assert_eq!((rows[0].version.as_deref(), rows[0].cpe.as_deref()), (None, None));
        assert_eq!((rows[1].version.as_deref(), rows[1].cpe.as_deref()), (None, None));
        assert_eq!(rows[2].version.as_deref(), Some("10.3p1"));
        assert_eq!(
            rows[2].cpe.as_deref(),
            Some("cpe:2.3:a:openbsd:openssh:10.3p1:*:*:*:*:*:*:*")
        );
    }

    /// #1404 — the new fields parse, and a pre-#1404 row is still a leaf.
    #[test]
    fn rows_parse_with_compound_licenses_and_without() {
        let rows = parse_rows(
            r#"[{"drv":"a.drv","out":"/o/a","attr":"a","root":"pkgs",
                 "licenses":[{"spdx":"MIT","name":"mit"},
                             {"spdx":null,"name":null,"op":"AND","terms":[
                                {"spdx":"NCSA","name":"ncsa"},
                                {"spdx":null,"name":null,"op":"WITH","terms":[
                                   {"spdx":"Apache-2.0","name":"asl20"},
                                   {"spdx":"LLVM-exception","name":"llvm-exception"}]}]}]}]"#,
        );
        assert_eq!(rows.len(), 1);
        let l = &rows[0].licenses;
        assert_eq!((l[0].op.as_deref(), l[0].terms.len()), (None, 0));
        assert_eq!(l[1].op.as_deref(), Some("AND"));
        assert_eq!(l[1].terms[0].spdx.as_deref(), Some("NCSA"));
        assert_eq!(l[1].terms[1].op.as_deref(), Some("WITH"));
        assert_eq!(
            l[1].terms[1].terms[1].spdx.as_deref(),
            Some("LLVM-exception")
        );
    }

    /// #1404 — the projection the license evaluation uses is the one this
    /// module defines, not a copy.
    #[test]
    fn the_expression_uses_the_tested_projection() {
        let expr = licenses_expr(&"a".repeat(40), &system(), &["hello"]);
        assert!(expr.contains(LICENSE_PROJECTION), "{expr}");
        assert!(expr.contains("licenses = lic (p.meta.license or null);"), "{expr}");
    }

    /// #1404 — the projection, evaluated by a real `nix` over values built the
    /// way `lib/licenses/operators.nix` builds them. No flake, no nixpkgs: the
    /// operators are restated inline so the test needs neither a fetch nor a
    /// fixture flake (evaluating one rewrote a committed `flake.lock` once).
    ///
    /// Asserts on what `parse_rows` makes of it, so the Nix and Rust halves are
    /// tested as one contract.
    #[test]
    fn the_projection_keeps_operators_nesting_and_exceptions() {
        if !nix_available() {
            eprintln!("skipping: no nix on PATH");
            return;
        }
        let expr = format!(
            r#"let
  {LICENSE_PROJECTION}
  simple = id: s: {{ spdxId = id; shortName = s; fullName = s; licenseType = "simple"; }};
  AND = ls: {{ licenseType = "compound"; operator = "AND"; licenses = ls; }};
  OR = ls: {{ licenseType = "compound"; operator = "OR"; licenses = ls; }};
  WITH = l: e: {{ licenseType = "exception"; operator = "WITH"; license = l; exception = e; }};
  PLUS = l: {{ licenseType = "plus"; operator = "+"; license = l; }};
  ncsa = simple "NCSA" "ncsa";
  asl20 = simple "Apache-2.0" "asl20";
  llvmExc = simple "LLVM-exception" "llvm-exception";
  mit = simple "MIT" "mit";
  publicDomain = {{ shortName = "publicDomain"; fullName = "Public Domain"; licenseType = "simple"; }};
  mk = l: {{ drv = "x.drv"; out = "/o/x"; attr = "x"; root = "t"; licenses = lic l; }};
in [
  (mk (AND [ ncsa (WITH asl20 llvmExc) ]))
  (mk (OR [ (AND [ mit ncsa ]) publicDomain ]))
  (mk (PLUS (simple "EUPL-1.1" "eupl11")))
  (mk [ mit (OR [ mit asl20 ]) ])
  (mk "Some License")
  (mk null)
  (mk {{ fullName = "Legacy, no licenseType"; }})
]"#
        );
        let argv = vec!["eval", "--json", "--expr", expr.as_str()];
        let out = run_bounded(&argv, Duration::from_secs(60)).expect("nix should run");
        assert!(out.status_success, "stderr: {}", out.stderr);
        let rows = parse_rows(&out.stdout);
        assert_eq!(rows.len(), 7, "every row must parse: {}", out.stdout);

        let llvm = &rows[0].licenses;
        assert_eq!(llvm.len(), 1);
        assert_eq!(llvm[0].op.as_deref(), Some("AND"));
        assert_eq!(llvm[0].terms[0].spdx.as_deref(), Some("NCSA"));
        let with = &llvm[0].terms[1];
        assert_eq!(with.op.as_deref(), Some("WITH"));
        assert_eq!(with.terms[0].spdx.as_deref(), Some("Apache-2.0"));
        assert_eq!(with.terms[1].spdx.as_deref(), Some("LLVM-exception"));

        let nested = &rows[1].licenses[0];
        assert_eq!(nested.op.as_deref(), Some("OR"));
        assert_eq!(nested.terms[0].op.as_deref(), Some("AND"));
        // No spdxId: the name survives, and no identifier is made up for it.
        assert_eq!(nested.terms[1].spdx, None);
        assert_eq!(nested.terms[1].name.as_deref(), Some("publicDomain"));

        let plus = &rows[2].licenses[0];
        assert_eq!(plus.op.as_deref(), Some("+"));
        assert_eq!(plus.terms[0].spdx.as_deref(), Some("EUPL-1.1"));

        let list = &rows[3].licenses;
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].spdx.as_deref(), Some("MIT"));
        assert_eq!(list[1].op.as_deref(), Some("OR"));

        assert_eq!(rows[4].licenses[0].name.as_deref(), Some("Some License"));
        assert!(rows[5].licenses.is_empty());
        assert_eq!(
            rows[6].licenses[0].name.as_deref(),
            Some("Legacy, no licenseType")
        );
        assert_eq!(rows[6].licenses[0].op, None);
    }

    /// T014 — the three guards, against a real `nix`.
    ///
    /// This is the test that would have caught each failure in research R2,
    /// and it asserts a **positive** signal: the good name resolves. A run
    /// where everything is dropped would otherwise look identical to a run
    /// where the guards work.
    #[test]
    fn the_guards_drop_bad_attributes_without_aborting_the_pass() {
        if !nix_available() {
            eprintln!("skipping: no nix on PATH");
            return;
        }
        // A deliberately hostile local set: a missing attribute, a `null`
        // alias, a thrower, and one good package. Only the good one may
        // survive, and the pass must not abort.
        let expr = r#"let
  mk = n: { drvPath = "/nix/store/0000000000000000000000000000000-${n}.drv";
            outPath = "/nix/store/1111111111111111111111111111111-${n}";
            meta.license = { spdxId = "MIT"; shortName = "mit"; }; };
  set = { good = mk "good"; broken = null; thrower = throw "nope"; };
  roots = [ { id = "t"; set = set; } ];
  rowIn = root: name:
    if ! (builtins.hasAttr name root.set) then [ ] else
    let pr = builtins.tryEval (builtins.getAttr name root.set);
        p = if pr.success then pr.value else null;
    in if ! (builtins.isAttrs p) || ! (p ? drvPath) then [ ] else
    let v = { attr = name; root = root.id; drv = baseNameOf p.drvPath;
              out = p.outPath; licenses = [ { spdx = p.meta.license.spdxId or null;
                                              name = p.meta.license.shortName or null; } ]; };
        r = builtins.tryEval (builtins.deepSeq v v);
    in if r.success then [ r.value ] else [ ];
in builtins.concatMap (n: builtins.concatMap (r: rowIn r n) roots)
     [ "good" "broken" "thrower" "absent" ]"#;
        let argv = vec![
            "eval",
            "--json",
            "--option",
            IFD_SETTING,
            "false",
            "--expr",
            expr,
        ];
        let out = run_bounded(&argv, Duration::from_secs(60)).expect("nix should run");
        assert!(
            out.status_success,
            "the guards must not let one bad attribute abort the pass; stderr: {}",
            out.stderr
        );
        let rows = parse_rows(&out.stdout);
        assert_eq!(
            rows.len(),
            1,
            "only the good attribute may survive, got: {rows:?}"
        );
        assert_eq!(rows[0].attr, "good");
        assert_eq!(rows[0].licenses[0].spdx.as_deref(), Some("MIT"));
    }
}
