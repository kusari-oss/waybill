//! `waybill debug closure-dump` — what the closure reader actually sees.
//!
//! **Why this exists.** On 2026-11-06 a single session produced four wrong
//! findings about the nix closure, including a nonexistent defect in code
//! that had merged an hour earlier. Every one came from the same move:
//! answering "what does waybill see for this derivation?" with a Python
//! model of the reader instead of with the reader.
//!
//! The models drifted in ways that each looked right:
//!
//! * `pname` is read by [`RawDerivation::declared_pname`], which consults the
//!   flat `env` map **and** the typed `structuredAttrs` carrier. A probe that
//!   read `env` only called 66 of 1,275 derivations nameless and reported ten
//!   of them as a milestone-1282 gap. They declare a `pname`; the gap did not
//!   exist.
//! * Emission is decided by [`emit::emits_as_component`], not by a `pname`
//!   test — the fallback in [`RawDerivation::pname`] makes a `pname` test
//!   true for every member.
//! * Components are named by `pname`, so comparing raw derivation names
//!   against another tool's output counted 74 packages as missing that were
//!   present under a different spelling.
//!
//! None of those is a knowledge problem: the rules are documented and were
//! quoted while being violated. The fix is structural — this command calls
//! the same functions the emitter does, so a question answered here cannot
//! disagree with what waybill emits.
//!
//! It reads a closure JSON file and writes JSON. It never evaluates a flake:
//! `nix derivation show --recursive <flakeref> > closure.json` is the
//! operator's step, deliberately, because evaluating a flake in place has
//! rewritten a committed `flake.lock` before now.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};

use crate::scan_fs::package_db::nix::closure::classify::classify;
use crate::scan_fs::package_db::nix::closure::derivation::RawClosure;
use crate::scan_fs::package_db::nix::closure::{emit, member};

#[derive(Debug, Parser)]
pub struct DebugCommand {
    #[command(subcommand)]
    pub command: DebugSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum DebugSubcommand {
    /// Report, per derivation, what the closure reader sees: which carrier
    /// holds its `pname`, its C182 role, its milestone-1282 member role and
    /// identity, and whether it is emitted and edge-eligible.
    ClosureDump(ClosureDumpArgs),
}

#[derive(Debug, Args)]
pub struct ClosureDumpArgs {
    /// A closure JSON file, as produced by
    /// `nix derivation show --recursive <flakeref>`.
    #[arg(long, value_name = "FILE")]
    pub closure_json: PathBuf,

    /// Where to write. Defaults to stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Report only the counts, not the per-derivation rows.
    #[arg(long)]
    pub summary_only: bool,

    /// Report only derivations whose name contains this substring.
    #[arg(long, value_name = "SUBSTRING")]
    pub filter: Option<String>,
}

pub fn execute(cmd: DebugCommand) -> anyhow::Result<ExitCode> {
    match cmd.command {
        DebugSubcommand::ClosureDump(args) => closure_dump(args),
    }
}

fn closure_dump(args: ClosureDumpArgs) -> anyhow::Result<ExitCode> {
    let text = std::fs::read_to_string(&args.closure_json).map_err(|e| {
        anyhow::anyhow!("cannot read --closure-json {}: {e}", args.closure_json.display())
    })?;
    let closure = RawClosure::parse(&text)
        .map_err(|e| anyhow::anyhow!("{} is not a closure document: {e}", args.closure_json.display()))?;
    let roles = classify(&closure);

    let mut rows: Vec<Value> = Vec::new();
    let mut n_emitted = 0usize;
    let mut n_members = 0usize;
    let mut by_carrier: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut by_c182: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut by_member: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut identities: std::collections::BTreeSet<String> = Default::default();

    for (key, drv) in &closure.derivations {
        let Some(role) = roles.get(key).copied() else {
            continue;
        };
        let emits = emit::emits_as_component(role, drv);
        if emits {
            n_emitted += 1;
        }

        // Which carrier holds `pname` — the distinction that produced a
        // phantom defect. `declared_pname` is the real reader; the two
        // booleans below say where it found the value, which is exactly what
        // a hand-rolled probe gets wrong.
        let declared = drv.declared_pname();
        let carrier = drv.attr_carrier("pname");
        *by_carrier.entry(carrier).or_default() += 1;
        *by_c182.entry(role.wire()).or_default() += 1;

        let member_role = declared.is_none().then(|| member::classify(drv));
        let basis = member_role.is_some().then(|| member::identity(drv, key));
        if let Some(r) = member_role {
            n_members += 1;
            *by_member.entry(r.wire()).or_default() += 1;
        }
        if let Some(b) = &basis {
            identities.insert(b.hash().to_string());
        }

        if args.summary_only {
            continue;
        }
        if let Some(f) = &args.filter {
            if !drv.name.as_deref().unwrap_or_default().contains(f.as_str()) {
                continue;
            }
        }
        rows.push(json!({
            "drv": key,
            "name": drv.name,
            "declared_pname": declared,
            "pname_carrier": carrier,
            "pname_with_fallback": drv.pname(),
            "version": drv.version(),
            "c182_role": role.wire(),
            "emits_as_component": emits,
            "member_role": member_role.map(|r| r.wire()),
            "edge_endpoint": member_role.map(|r| r.is_edge_endpoint()),
            "identity_hash": basis.as_ref().map(|b| b.hash()),
        }));
    }

    let out = json!({
        "schema": "waybill-closure-dump/v1",
        "source": args.closure_json.display().to_string(),
        "derivations": closure.derivations.len(),
        "emitted_as_component": n_emitted,
        "members_1282": n_members,
        "distinct_member_identities": identities.len(),
        "pname_carrier": by_carrier,
        "c182_roles": by_c182,
        "member_roles": by_member,
        "rows": rows,
    });
    let text = serde_json::to_string_pretty(&out)?;
    match args.output {
        Some(p) => std::fs::write(&p, text)
            .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", p.display()))?,
        None => println!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}
