//! Milestone 1071 — `waybill sbom edit`.
//!
//! Reads one emitted SBOM, applies filter and redaction operations in
//! command-line order, attaches the derivation record, optionally signs, and
//! writes the same format. Nothing is written unless every post-condition
//! holds. See `specs/1071-sbom-edit/contracts/cli.md`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Context;
use clap::Args;

use crate::edit::derivation::OriginalSignature;
use crate::edit::redact::{RedactClass, RedactMode};
use crate::edit::select::Selector;
use crate::edit::{EditOp, EditOptions};
use crate::sbom::signer::{SigningMode, Sidecar};

/// Clap stores each repeatable flag in its own list, losing the order
/// between them. Operations are applied in command-line order, so every
/// value is stamped as clap parses it, which is left to right.
static PARSE_SEQ: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug)]
pub struct Sequenced {
    seq: usize,
    op: EditOp,
}

fn stamp(op: EditOp) -> Sequenced {
    Sequenced { seq: PARSE_SEQ.fetch_add(1, Ordering::Relaxed), op }
}

fn parse_drop(s: &str) -> Result<Sequenced, String> {
    Ok(stamp(EditOp::DropComponents { selector: s.parse::<Selector>()? }))
}

fn parse_drop_annotations(s: &str) -> Result<Sequenced, String> {
    if s.is_empty() {
        return Err("an annotation namespace is required, e.g. `waybill:` or `waybill:graph-`".into());
    }
    Ok(stamp(EditOp::DropAnnotations { namespace: s.to_string() }))
}

/// `<class>[:<mode>][=<pattern>]`. The pattern may itself contain `:`.
fn parse_redact(s: &str) -> Result<Sequenced, String> {
    let (head, pattern) = match s.split_once('=') {
        Some((h, p)) if !p.is_empty() => (h, Some(p.to_string())),
        Some((_, _)) => return Err(format!("`{s}`: empty pattern after `=`")),
        None => (s, None),
    };
    let (class_text, mode_text) = match head.split_once(':') {
        Some((c, m)) => (c, Some(m)),
        None => (head, None),
    };
    let class = RedactClass::parse(class_text)
        .ok_or_else(|| format!("`{class_text}`: redaction class must be paths, hosts or names"))?;
    let mode = match mode_text {
        Some(m) => RedactMode::parse(m).ok_or_else(|| format!("`{m}`: redaction mode must be remove or pseudonymise"))?,
        None => class.default_mode(),
    };
    if pattern.is_none() && class != RedactClass::Paths {
        return Err(format!("`--redact {class_text}` needs a pattern: `{class_text}=<glob>` or `{class_text}=re:<regex>`"));
    }
    Ok(stamp(EditOp::Redact { class, mode, pattern }))
}

#[derive(Args, Debug)]
pub struct EditArgs {
    /// The SBOM to edit: CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1 JSON.
    pub input: PathBuf,

    /// Where to write the edited SBOM (same format as the input). `-` for
    /// standard output, when not signing.
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    pub output: PathBuf,

    /// Drop the components matching a selector, bridging their dependents
    /// to their dependencies. Selector: `;`-separated `key=v1,v2` terms
    /// (AND across terms, OR within one); keys purl, ecosystem, scope,
    /// tier, role, name. Repeatable; operations apply in command-line order.
    #[arg(long = "drop", value_name = "SELECTOR", value_parser = parse_drop, action = clap::ArgAction::Append)]
    pub drop: Vec<Sequenced>,

    /// Remove annotations whose field starts with a namespace (e.g.
    /// `waybill:graph-`). `waybill:generation-context` and
    /// `waybill:derivation` are never removed. Repeatable.
    #[arg(long = "drop-annotations", value_name = "NAMESPACE", value_parser = parse_drop_annotations, action = clap::ArgAction::Append)]
    pub drop_annotations: Vec<Sequenced>,

    /// Redact values: `<class>[:<mode>][=<pattern>]`. Class is paths, hosts
    /// or names; mode is remove (default for paths) or pseudonymise
    /// (default for hosts and names); pattern is a glob or `re:<regex>`,
    /// optional for paths only. Repeatable.
    #[arg(long = "redact", value_name = "SPEC", value_parser = parse_redact, action = clap::ArgAction::Append)]
    pub redact: Vec<Sequenced>,

    /// The pseudonymisation key (bytes of the file, trailing newline
    /// ignored). Required when any redaction pseudonymises. Never written
    /// to the output or logged.
    #[arg(long = "redact-key-file", value_name = "PATH")]
    pub redact_key_file: Option<PathBuf>,

    /// The original's signature sidecar, when it isn't next to the input as
    /// `<input>.sig.json` or `<input>.sig.bundle.json`.
    #[arg(long = "original-signature", value_name = "PATH")]
    pub original_signature: Option<PathBuf>,

    /// Sign the edited SBOM with a PEM private key, as `sbom scan
    /// --sign-key` does: in-document JSF for CycloneDX, a DSSE
    /// `<output>.sig.json` sidecar for SPDX.
    #[arg(long = "sign-key", value_name = "PATH")]
    pub sign_key: Option<PathBuf>,

    /// Environment variable holding the `--sign-key` passphrase. Defaults
    /// to `WAYBILL_SIGN_KEY_PASSPHRASE`.
    #[arg(long = "sign-key-passphrase-env", value_name = "NAME")]
    pub sign_key_passphrase_env: Option<String>,

    /// Sigstore keyless signing, as `sbom scan --sign` does: a
    /// `<output>.sig.bundle.json` sidecar.
    #[arg(long = "sign", conflicts_with = "sign_key")]
    pub sign: bool,

    /// Fulcio endpoint for `--sign`.
    #[arg(long = "fulcio-url", env = "WAYBILL_FULCIO_URL", default_value = "https://fulcio.sigstore.dev", value_name = "URL")]
    pub fulcio_url: String,

    /// Rekor endpoint for `--sign`.
    #[arg(long = "rekor-url", env = "WAYBILL_REKOR_URL", default_value = "https://rekor.sigstore.dev", value_name = "URL")]
    pub rekor_url: String,

    /// Rekor inclusion-proof timeout for `--sign`, in seconds.
    #[arg(long = "rekor-timeout-secs", env = "WAYBILL_REKOR_TIMEOUT_SECS", default_value_t = 30, value_name = "SECS")]
    pub rekor_timeout_secs: u64,
}

impl EditArgs {
    /// The operations, in command-line order.
    pub fn ops(&self) -> Vec<EditOp> {
        let mut all: Vec<&Sequenced> =
            self.drop.iter().chain(&self.drop_annotations).chain(&self.redact).collect();
        all.sort_by_key(|s| s.seq);
        all.into_iter().map(|s| s.op.clone()).collect()
    }

    fn signing_mode(&self) -> SigningMode {
        if self.sign {
            SigningMode::Keyless {
                fulcio_url: self.fulcio_url.clone(),
                rekor_url: self.rekor_url.clone(),
                rekor_timeout: std::time::Duration::from_secs(self.rekor_timeout_secs),
            }
        } else if let Some(path) = &self.sign_key {
            SigningMode::StaticKey {
                key_ref: path.clone(),
                passphrase_env: self
                    .sign_key_passphrase_env
                    .clone()
                    .unwrap_or_else(|| "WAYBILL_SIGN_KEY_PASSPHRASE".to_string()),
            }
        } else {
            SigningMode::Unsigned
        }
    }
}

fn read_key(path: &Path) -> anyhow::Result<Vec<u8>> {
    let mut key = std::fs::read(path).with_context(|| format!("reading --redact-key-file {}", path.display()))?;
    while key.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
        key.pop();
    }
    if key.is_empty() {
        anyhow::bail!("--redact-key-file {} is empty", path.display());
    }
    Ok(key)
}

pub async fn execute(args: EditArgs) -> anyhow::Result<ExitCode> {
    let ops = args.ops();
    if ops.is_empty() {
        anyhow::bail!("no operation given: use --drop, --drop-annotations or --redact");
    }
    let to_stdout = args.output.as_os_str() == "-";
    let mode = args.signing_mode();
    if to_stdout && mode.is_enabled() {
        anyhow::bail!("signing needs a file: pass -o <path>, not -o -");
    }
    if !to_stdout && same_file(&args.input, &args.output) {
        anyhow::bail!("the output must not overwrite the input: the input is the original the derivation record names");
    }

    let input = std::fs::read(&args.input).with_context(|| format!("reading {}", args.input.display()))?;
    let doc: serde_json::Value = serde_json::from_slice(&input)
        .with_context(|| format!("{} is not valid JSON", args.input.display()))?;
    let opts = EditOptions {
        redact_key: args.redact_key_file.as_deref().map(read_key).transpose()?,
        original_signature: OriginalSignature::find(&args.input, &doc, args.original_signature.as_deref())?,
    };
    let outcome = crate::edit::run(&input, &ops, &opts)?;

    for op in &outcome.report.ops {
        eprintln!("{}: matched {}, changed {}", op.category, op.matched, op.changed);
    }
    if outcome.report.dependency_lists_changed > 0 {
        eprintln!(
            "{} dependency list(s) bridged across dropped components; graph completeness downgraded",
            outcome.report.dependency_lists_changed
        );
    }

    if to_stdout {
        use std::io::Write;
        std::io::stdout().write_all(&outcome.bytes)?;
        return Ok(ExitCode::SUCCESS);
    }
    write_signed(&args.output, outcome.format, &outcome.bytes, &mode)?;
    tracing::info!(
        format = outcome.format.label(),
        path = %args.output.display(),
        "wrote edited SBOM"
    );
    Ok(ExitCode::SUCCESS)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Write the document, signing it as `sbom scan` would. On any failure,
/// remove what was written.
fn write_signed(target: &Path, format: crate::edit::Format, bytes: &[u8], mode: &SigningMode) -> anyhow::Result<()> {
    let is_cdx = format == crate::edit::Format::CycloneDx16;
    let keyless = matches!(mode, SigningMode::Keyless { .. });
    let mut written: Vec<PathBuf> = Vec::new();
    let result = (|| -> anyhow::Result<()> {
        let document = if !mode.is_enabled() {
            bytes.to_vec()
        } else if is_cdx && !keyless {
            crate::sbom::signer::sign_cdx_bytes_for_write(bytes, mode).map_err(|e| anyhow::anyhow!("signing failed: {e}"))?
        } else if is_cdx {
            // Milestone 778: keyless CycloneDX names its detached bundle
            // before it is signed.
            let suffix = mode.sidecar_suffix().unwrap_or(".sig.bundle.json");
            let name = format!(
                "{}{suffix}",
                target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            );
            crate::sbom::signer::inject_signature_reference(bytes, &name)?
        } else {
            bytes.to_vec()
        };
        std::fs::write(target, &document).with_context(|| format!("writing {}", target.display()))?;
        written.push(target.to_path_buf());

        if mode.is_enabled() && (!is_cdx || keyless) {
            let sidecar = crate::sbom::signer::sign_sbom_bytes_to_sidecar(&document, mode)
                .map_err(|e| anyhow::anyhow!("signing failed: {e}"))?
                .ok_or_else(|| anyhow::anyhow!("signing produced no signature"))?;
            let path = target.with_extension(crate::sbom::signer::sidecar_extension_for(target, &sidecar));
            std::fs::write(&path, sidecar.to_json_bytes()?).with_context(|| format!("writing {}", path.display()))?;
            written.push(path.clone());
            if let Sidecar::SigstoreBundle { identity, environment, .. } = &sidecar {
                let cmd = crate::attestation::signer::VerificationCommand::render(*environment, identity, target, &path);
                eprintln!("\nTo verify:\n{}\n", cmd.rendered);
            }
        }
        Ok(())
    })();
    if result.is_err() {
        for p in &written {
            let _ = std::fs::remove_file(p);
        }
    }
    result
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Harness {
        #[command(flatten)]
        args: EditArgs,
    }

    #[test]
    fn operations_keep_command_line_order_across_flags() {
        let h = Harness::try_parse_from([
            "t", "in.json", "-o", "out.json",
            "--redact", "paths",
            "--drop", "scope=development",
            "--drop-annotations", "waybill:graph-",
            "--drop", "tier=file",
        ])
        .unwrap();
        let cats: Vec<&str> = h.args.ops().iter().map(EditOp::category).collect();
        assert_eq!(cats, ["redact-paths", "drop-components", "drop-annotations", "drop-components"]);
    }

    #[test]
    fn redact_spec_parses_class_mode_and_pattern() {
        let op = parse_redact("hosts:remove=*.corp.example:8443").unwrap().op;
        match op {
            EditOp::Redact { class, mode, pattern } => {
                assert_eq!(class, RedactClass::Hosts);
                assert_eq!(mode, RedactMode::Remove);
                assert_eq!(pattern.as_deref(), Some("*.corp.example:8443"));
            }
            _ => panic!("not a redaction"),
        }
        assert!(parse_redact("names").is_err());
        assert!(parse_redact("secrets=x").is_err());
        assert!(parse_redact("paths:shred").is_err());
        assert!(matches!(parse_redact("paths").unwrap().op, EditOp::Redact { mode: RedactMode::Remove, .. }));
    }
}
