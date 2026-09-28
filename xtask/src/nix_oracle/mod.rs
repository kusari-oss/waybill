//! Issue #971 part B — differential testing of waybill's nixpkgs Haskell
//! resolution against `nix eval`.
//!
//! waybill resolves Haskell versions by fetching and parsing nixpkgs files.
//! `nix eval` computes the same answer by evaluating them, which is an
//! independent mechanism over the same data — so a disagreement is a parser
//! defect by construction.
//!
//! That is the whole point. Issue #1033 was found this way and nothing else
//! found it: 6,000+ tests passed over it, because every fixture was written by
//! someone who did not know the situation existed. A check whose oracle is
//! something the author wrote cannot discover what the author did not know.
//!
//! # Three confounds, each of which made this oracle wrong before it was right
//!
//! Running it by hand the first time produced three "defects", one of which was
//! real. The other two were the oracle's own errors, and each is now handled:
//!
//! 1. **System.** `hinotify` is `0.4.2` on `x86_64-linux` and `0.1.8` on
//!    `aarch64-darwin` at the same revision. Evaluating for the wrong system
//!    reports a disagreement that does not exist. The system is a parameter,
//!    defaulting to the host's.
//!
//! 2. **The project's own packages.** A source tree at `2.15.0.0` against a
//!    nixpkgs that ships `2.13.0.0` is not a defect — waybill reads the tree,
//!    which is right. These carry `waybill:nixpkgs-version-disagreement`
//!    already, so the rule below skips anything waybill has disclosed.
//!
//! 3. **GHC package set.** A flake may name several; at the measured revision
//!    all six agreed, so `haskellPackages` is used and the assumption is
//!    recorded here rather than left implicit. A future disagreement traced to
//!    a non-default set belongs in this comment, not in a silent workaround.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use std::error::Error;

use clap::Args;

type R<T> = Result<T, Box<dyn Error>>;

#[derive(Args, Debug)]
pub struct NixOracleArgs {
    /// CycloneDX SBOM emitted by waybill, carrying
    /// `waybill:nixpkgs-component-origin` annotations.
    #[arg(long)]
    pub sbom: PathBuf,

    /// Nix system to evaluate for. Defaults to the host's.
    #[arg(long)]
    pub system: Option<String>,

    /// Write the full classification here as JSON.
    #[arg(long)]
    pub json: Option<PathBuf>,

    /// Report disagreements without failing. Used to observe a target before
    /// it becomes a gate.
    #[arg(long)]
    pub allow_failure: bool,
}

/// What the oracle concluded about one component.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// waybill and `nix eval` report the same version.
    Agree,
    /// waybill already annotated this as disagreeing, with a reason. Not a
    /// finding: the document is honest about it.
    DisclosedByWaybill,
    /// nixpkgs has no such attribute. Not a version defect — it means waybill
    /// emitted a package the package set does not contain, which is #1032.
    AbsentFromNixpkgs,
    /// waybill and `nix eval` disagree, undisclosed. The failure case.
    Disagree,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Row {
    pub name: String,
    pub waybill_version: String,
    pub nix_version: Option<String>,
    pub origin: Option<String>,
    pub verdict: Verdict,
}

/// One component's claim, as the SBOM states it.
struct Claim {
    name: String,
    version: String,
    origin: Option<String>,
    /// waybill already annotated this component as disagreeing with nixpkgs
    /// and said why. The document is honest about it, so the oracle has
    /// nothing to add.
    disclosed: bool,
}

/// Components waybill says it resolved from nixpkgs, and the revision it used.
fn read_sbom(sbom: &serde_json::Value) -> R<(Vec<Claim>, String)> {
    let components = sbom["components"]
        .as_array()
        .ok_or("no components[] in the SBOM")?;
    let prop = |c: &serde_json::Value, key: &str| -> Option<String> {
        c["properties"].as_array()?.iter().find_map(|p| {
            (p["name"].as_str()? == key).then(|| p["value"].as_str().unwrap_or("").to_string())
        })
    };

    let mut out = Vec::new();
    let mut revision: Option<String> = None;
    for c in components {
        let Some(origin) = prop(c, "waybill:nixpkgs-component-origin") else {
            continue;
        };
        let Some(version) = c["version"].as_str().filter(|v| !v.is_empty()) else {
            continue;
        };
        let Some(name) = c["name"].as_str() else { continue };
        // The revision waybill actually used, taken from the document rather
        // than from the flake.lock — comparing against a revision the scan did
        // not use would manufacture disagreements.
        if revision.is_none() {
            if let Some(via) = prop(c, "waybill:nixpkgs-resolved-via") {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&via) {
                    revision = v["revision"].as_str().map(str::to_string);
                }
            }
        }
        let disclosed = prop(c, "waybill:nixpkgs-version-disagreement").is_some();
        out.push(Claim {
            name: name.to_string(),
            version: version.to_string(),
            origin: Some(origin),
            disclosed,
        });
    }
    let revision = revision.ok_or(
        "no waybill:nixpkgs-resolved-via revision found; this SBOM did not resolve through nixpkgs",
    )?;
    Ok((out, revision))
}

/// One `nix eval` for every name, tolerating attributes the revision lacks.
fn eval_versions(
    names: &[String],
    revision: &str,
    system: &str,
) -> R<BTreeMap<String, Option<String>>> {
    if names.is_empty() {
        return Ok(BTreeMap::new());
    }
    let list = names
        .iter()
        .map(|n| format!("{:?}", n))
        .collect::<Vec<_>>()
        .join(" ");
    // `tryEval` so one broken attribute cannot abort the whole evaluation, and
    // `or null` so a missing one is reported rather than fatal.
    let expr = format!(
        r#"let
  pkgs = (builtins.getFlake "github:NixOS/nixpkgs/{revision}").legacyPackages.{system};
  hp = pkgs.haskellPackages;
  ver = n:
    let r = builtins.tryEval (if hp ? ${{n}} then (hp.${{n}}.version or null) else null);
    in if r.success then r.value else null;
in builtins.listToAttrs (map (n: {{ name = n; value = ver n; }}) [ {list} ])"#
    );
    let out = Command::new("nix")
        .args(["eval", "--json", "--impure", "--expr", &expr])
        .output()
        .map_err(|e| format!("running `nix eval` — is nix on PATH? ({e})"))?;
    if !out.status.success() {
        return Err(format!(
            "nix eval failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    Ok(serde_json::from_slice(&out.stdout)?)
}

fn host_system() -> R<String> {
    let out = Command::new("nix")
        .args(["eval", "--impure", "--raw", "--expr", "builtins.currentSystem"])
        .output()
        .map_err(|e| format!("running `nix eval` to detect the host system: {e}"))?;
    if !out.status.success() {
        return Err("could not determine the host nix system".into());
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

pub fn run(args: NixOracleArgs) -> R<()> {
    let text = std::fs::read_to_string(&args.sbom)
        .map_err(|e| format!("reading {}: {e}", args.sbom.display()))?;
    let sbom: serde_json::Value = serde_json::from_str(&text)?;
    let (claims, revision) = read_sbom(&sbom)?;
    let system = match args.system {
        Some(s) => s,
        None => host_system()?,
    };

    let names: Vec<String> = {
        let mut v: Vec<String> = claims.iter().map(|c| c.name.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    eprintln!(
        "nix-oracle: {} components, {} distinct names, revision {revision}, system {system}",
        claims.len(),
        names.len()
    );
    let versions = eval_versions(&names, &revision, &system)?;

    let mut rows = Vec::new();
    for Claim { name, version: waybill_version, origin, disclosed } in claims {
        let nix_version = versions.get(&name).cloned().flatten();
        let verdict = if disclosed {
            Verdict::DisclosedByWaybill
        } else {
            match &nix_version {
                None => Verdict::AbsentFromNixpkgs,
                Some(v) if *v == waybill_version => Verdict::Agree,
                Some(_) => Verdict::Disagree,
            }
        };
        rows.push(Row { name, waybill_version, nix_version, origin, verdict });
    }

    let count = |v: &Verdict| rows.iter().filter(|r| r.verdict == *v).count();
    let disagree = count(&Verdict::Disagree);
    println!("agree                  {}", count(&Verdict::Agree));
    println!("disclosed by waybill   {}", count(&Verdict::DisclosedByWaybill));
    println!("absent from nixpkgs    {}   (issue #1032, not a version defect)", count(&Verdict::AbsentFromNixpkgs));
    println!("DISAGREE               {disagree}");
    for r in rows.iter().filter(|r| r.verdict == Verdict::Disagree) {
        println!(
            "   {:34} waybill={:<14} nix={}",
            r.name,
            r.waybill_version,
            r.nix_version.as_deref().unwrap_or("-")
        );
    }

    if let Some(path) = &args.json {
        std::fs::write(path, serde_json::to_vec_pretty(&rows)?)?;
    }

    if disagree > 0 && !args.allow_failure {
        return Err(format!(
            "{disagree} component(s) disagree with `nix eval` at {revision} and waybill does not \
             disclose it — a parser defect by construction (see #971, #1033)"
        )
        .into());
    }
    Ok(())
}
