//! Milestone 1071 — `waybill sbom verify-chain`.
//!
//! Walks the derivation chain of an edited SBOM: its own signature, then for
//! each `--original` (newest first) the hash the derivation record names and
//! the original signature the record carries. Keyless signatures are checked
//! as far as waybill can without a trust root (the signed digest matches the
//! document) and the rest is delegated to `cosign verify-blob`, whose command
//! is printed. See `specs/1071-sbom-edit/data-model.md` (ChainReport).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Context;
use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine;
use clap::Args;
use serde::Serialize;
use serde_json::Value;

use crate::edit::derivation::{canonical, sha256_hex, strip_payload};

#[derive(Args, Debug)]
pub struct VerifyChainArgs {
    /// The derived (edited) SBOM.
    pub derived: PathBuf,

    /// The document each derivation step was made from, newest first: the
    /// derived document's original, then that one's original, and so on.
    #[arg(long = "original", value_name = "PATH", action = clap::ArgAction::Append)]
    pub originals: Vec<PathBuf>,

    /// A public key (PEM) for static-key signatures. Repeatable. Embedded
    /// keys are never trusted. Giving a key also requires the derived
    /// document itself to be signed.
    #[arg(long = "key", value_name = "PEM", action = clap::ArgAction::Append)]
    pub keys: Vec<PathBuf>,

    /// An original's signature material, when a derivation record carries
    /// only its digest (it held a value the edit redacted). Matched by
    /// digest; signatures next to the originals are found without this.
    #[arg(long = "original-signature", value_name = "PATH", action = clap::ArgAction::Append)]
    pub original_signatures: Vec<PathBuf>,

    /// Machine-readable report.
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Check {
    Verified,
    Delegated { command: String },
    Unsigned,
    Failed { reason: String },
    Matched,
    Mismatched { expected: String, actual: String },
    OriginalNotSupplied,
}

impl Check {
    fn is_failure(&self) -> bool {
        matches!(self, Self::Failed { .. } | Self::Mismatched { .. })
    }

    fn failed(reason: impl Into<String>) -> Self {
        Self::Failed { reason: reason.into() }
    }

    fn line(&self) -> String {
        match self {
            Self::Verified => "verified".into(),
            Self::Delegated { command } => format!("delegated — run:\n{command}"),
            Self::Unsigned => "unsigned".into(),
            Self::Failed { reason } => format!("FAILED: {reason}"),
            Self::Matched => "matched".into(),
            Self::Mismatched { expected, actual } => format!("MISMATCHED: record names {expected}, supplied is {actual}"),
            Self::OriginalNotSupplied => "original not supplied".into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LinkCheck {
    pub document: String,
    pub sha256: String,
    pub own_signature: Check,
    pub original_hash: Check,
    pub original_signature: Check,
}

#[derive(Debug, Serialize)]
pub struct ChainReport {
    pub steps: Vec<LinkCheck>,
    pub ok: bool,
}

struct Doc {
    path: PathBuf,
    bytes: Vec<u8>,
    value: Value,
}

impl Doc {
    fn read(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let value = serde_json::from_slice(&bytes).with_context(|| format!("{} is not valid JSON", path.display()))?;
        Ok(Self { path: path.to_path_buf(), bytes, value })
    }

    fn derivation_record(&self) -> anyhow::Result<Option<Value>> {
        let format = crate::edit::detect(&self.value)?;
        Ok(crate::edit::adapter_for(format, self.value.clone()).derivation_record())
    }

    fn sidecar(&self, suffix: &str) -> Option<PathBuf> {
        let mut name = self.path.file_name()?.to_os_string();
        name.push(suffix);
        let p = self.path.with_file_name(name);
        p.is_file().then_some(p)
    }
}

pub async fn execute(args: VerifyChainArgs) -> anyhow::Result<ExitCode> {
    let keys: Vec<String> = args
        .keys
        .iter()
        .map(|p| std::fs::read_to_string(p).with_context(|| format!("reading --key {}", p.display())))
        .collect::<anyhow::Result<_>>()?;
    let extra: Vec<Value> = args
        .original_signatures
        .iter()
        .map(|p| -> anyhow::Result<Value> {
            let bytes = std::fs::read(p).with_context(|| format!("reading {}", p.display()))?;
            serde_json::from_slice(&bytes).with_context(|| format!("{} is not JSON", p.display()))
        })
        .collect::<anyhow::Result<_>>()?;

    let report = verify(&args.derived, &args.originals, &keys, &extra)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for (i, s) in report.steps.iter().enumerate() {
            println!("step {}: {} (sha256 {})", i + 1, s.document, s.sha256);
            println!("  own signature:      {}", s.own_signature.line());
            println!("  original hash:      {}", s.original_hash.line());
            println!("  original signature: {}", s.original_signature.line());
        }
        println!("result: {}", if report.ok { "ok" } else { "FAILED" });
    }
    Ok(if report.ok { ExitCode::SUCCESS } else { ExitCode::from(1) })
}

pub fn verify(derived: &Path, originals: &[PathBuf], keys: &[String], extra: &[Value]) -> anyhow::Result<ChainReport> {
    let mut steps = Vec::new();
    let mut current = Doc::read(derived)?;
    let mut top = true;
    let mut originals = originals.iter();
    loop {
        let Some(record) = current.derivation_record()? else {
            if top {
                anyhow::bail!("{} carries no derivation record: it wasn't produced by `waybill sbom edit`", derived.display());
            }
            break;
        };
        let own = own_signature(&current, keys, top);
        let Some(original_path) = originals.next() else {
            steps.push(LinkCheck {
                document: current.path.display().to_string(),
                sha256: sha256_hex(&current.bytes),
                own_signature: own,
                original_hash: Check::OriginalNotSupplied,
                original_signature: Check::OriginalNotSupplied,
            });
            break;
        };
        let original = Doc::read(original_path)?;
        let expected = record.pointer("/original/sha256").and_then(Value::as_str).unwrap_or("").to_string();
        let actual = sha256_hex(&original.bytes);
        let hash = if expected == actual { Check::Matched } else { Check::Mismatched { expected, actual } };
        let sig = match &hash {
            Check::Matched => original_signature(&record, &original, keys, extra),
            _ => Check::failed("not checked: the supplied original is not the one the record names"),
        };
        steps.push(LinkCheck {
            document: current.path.display().to_string(),
            sha256: sha256_hex(&current.bytes),
            own_signature: own,
            original_hash: hash,
            original_signature: sig,
        });
        current = original;
        top = false;
    }
    let ok = steps
        .iter()
        .all(|s| !s.own_signature.is_failure() && !s.original_hash.is_failure() && !s.original_signature.is_failure());
    Ok(ChainReport { steps, ok })
}

/// The document's own signature: in-document JSF (CycloneDX static key), or
/// a DSSE or Sigstore sidecar next to it.
fn own_signature(doc: &Doc, keys: &[String], top: bool) -> Check {
    if doc.value.get("signature").is_some_and(Value::is_object) {
        return check_jsf(&doc.value, keys);
    }
    for suffix in [".sig.json", ".sig.bundle.json"] {
        if let Some(path) = doc.sidecar(suffix) {
            let material = match std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
                Some(v) => v,
                None => return Check::failed(format!("{} is not a readable signature", path.display())),
            };
            return check_material(&material, &doc.bytes, &doc.path, Some(&path), keys);
        }
    }
    if top && !keys.is_empty() {
        return Check::failed("--key was given but the document is unsigned");
    }
    Check::Unsigned
}

/// The original's signature as the derivation record carries it, checked
/// against the supplied original's bytes.
fn original_signature(record: &Value, original: &Doc, keys: &[String], extra: &[Value]) -> Check {
    let Some(sig) = record.pointer("/original/signature") else {
        return Check::failed("derivation record has no original.signature");
    };
    let kind = sig.get("kind").and_then(Value::as_str).unwrap_or("none");
    if kind == "none" {
        return Check::Unsigned;
    }
    let material = if sig.get("embedded").and_then(Value::as_bool) == Some(true) {
        match sig.get("material") {
            Some(m) => m.clone(),
            None => return Check::failed("record says the signature is embedded, but carries no material"),
        }
    } else {
        let Some(digest) = sig.get("material_sha256").and_then(Value::as_str) else {
            return Check::failed("record carries neither signature material nor its digest");
        };
        let mut candidates: Vec<Value> = extra.to_vec();
        candidates.extend(original.value.get("signature").cloned());
        for suffix in [".sig.json", ".sig.bundle.json"] {
            if let Some(v) = original.sidecar(suffix).and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()) {
                candidates.push(v);
            }
        }
        match candidates.into_iter().map(|c| strip_payload(&c)).find(|c| sha256_hex(canonical(c).as_bytes()) == digest) {
            Some(m) => m,
            None => {
                return Check::failed(
                    "the record carries only the original signature's digest (it held a redacted value); \
                     pass the signature with --original-signature",
                )
            }
        }
    };
    match kind {
        "jsf" => {
            let mut doc = original.value.clone();
            doc["signature"] = material;
            check_jsf(&doc, keys)
        }
        "dsse" | "sigstore-bundle" => check_material(&material, &original.bytes, &original.path, None, keys),
        other => Check::failed(format!("unknown signature kind `{other}`")),
    }
}

fn check_jsf(doc: &Value, keys: &[String]) -> Check {
    if keys.is_empty() {
        return Check::failed("a static-key signature: pass --key <public.pem> to verify it");
    }
    match crate::sbom::signer::verify_cdx_jsf(doc, keys) {
        Ok(true) => Check::Verified,
        Ok(false) => Check::failed("JSF signature does not verify under any supplied key"),
        Err(e) => Check::failed(format!("JSF signature: {e}")),
    }
}

/// A detached signature (DSSE envelope or Sigstore bundle) over `bytes`.
fn check_material(material: &Value, bytes: &[u8], doc_path: &Path, sidecar_path: Option<&Path>, keys: &[String]) -> Check {
    if material.get("payloadType").is_some() {
        return check_dsse(material, bytes, keys);
    }
    if material.get("verificationMaterial").is_some() || material.get("messageSignature").is_some() {
        return check_bundle(material, bytes, doc_path, sidecar_path);
    }
    Check::failed("signature is neither a DSSE envelope nor a Sigstore bundle")
}

fn check_dsse(env: &Value, bytes: &[u8], keys: &[String]) -> Check {
    use sigstore::crypto::verification_key::CosignVerificationKey;
    use sigstore::crypto::{Signature as SigstoreSig, SigningScheme};

    let payload_type = env.get("payloadType").and_then(Value::as_str).unwrap_or("");
    // Embedded material has its payload stripped (it is the original
    // document); a sidecar's must be exactly the document checked.
    if let Some(p) = env.get("payload") {
        if p.as_str().and_then(|p| BASE64_STD.decode(p).ok()).as_deref() != Some(bytes) {
            return Check::failed("DSSE payload is not this document's bytes");
        }
    }
    if keys.is_empty() {
        return Check::failed("a static-key signature: pass --key <public.pem> to verify it");
    }
    let pae = waybill_common::attestation::envelope::dsse_pae(payload_type, bytes);
    let sigs = env.get("signatures").and_then(Value::as_array).cloned().unwrap_or_default();
    for s in &sigs {
        let Some(sig) = s.get("sig").and_then(Value::as_str).and_then(|v| BASE64_STD.decode(v).ok()) else { continue };
        for pem in keys {
            for scheme in [SigningScheme::ECDSA_P256_SHA256_ASN1, SigningScheme::ED25519] {
                if let Ok(vk) = CosignVerificationKey::from_pem(pem.as_bytes(), &scheme) {
                    if vk.verify_signature(SigstoreSig::Raw(&sig), &pae).is_ok() {
                        return Check::Verified;
                    }
                }
            }
        }
    }
    Check::failed("DSSE signature does not verify under any supplied key")
}

/// waybill holds no Sigstore trust root, so a keyless signature is checked
/// only as far as the signed digest; certificate, identity and Rekor checks
/// are delegated to cosign.
fn check_bundle(bundle: &Value, bytes: &[u8], doc_path: &Path, sidecar_path: Option<&Path>) -> Check {
    let digest = bundle
        .pointer("/messageSignature/messageDigest/digest")
        .and_then(Value::as_str)
        .and_then(|d| BASE64_STD.decode(d).ok());
    let actual = sha2::Digest::finalize(<sha2::Sha256 as sha2::Digest>::new_with_prefix(bytes)).to_vec();
    if digest.as_deref() != Some(actual.as_slice()) {
        return Check::failed("the Sigstore bundle signs a different digest than this document's");
    }
    let cert = bundle
        .pointer("/verificationMaterial/certificate/rawBytes")
        .or_else(|| bundle.pointer("/verificationMaterial/x509CertificateChain/certificates/0/rawBytes"))
        .and_then(Value::as_str)
        .and_then(|c| BASE64_STD.decode(c).ok());
    let identity = cert.and_then(|der| crate::attestation::signer::extract_signing_identity(&der).ok());
    let sidecar = sidecar_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("<the original.signature.material from the derivation record, saved as a file>"));
    let command = match identity {
        Some(id) => {
            crate::attestation::signer::VerificationCommand::render(
                crate::attestation::signer::SigstoreEnvironment::Production,
                &id,
                doc_path,
                &sidecar,
            )
            .rendered
        }
        None => format!(
            "cosign verify-blob --bundle {} --certificate-identity <IDENTITY> --certificate-oidc-issuer <ISSUER> {}",
            sidecar.display(),
            doc_path.display()
        ),
    };
    Check::Delegated { command }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn only_failed_and_mismatched_fail_the_chain() {
        assert!(Check::failed("x").is_failure());
        assert!(Check::Mismatched { expected: "a".into(), actual: "b".into() }.is_failure());
        for c in [Check::Verified, Check::Unsigned, Check::Matched, Check::OriginalNotSupplied, Check::Delegated { command: "c".into() }] {
            assert!(!c.is_failure());
        }
    }

    #[test]
    fn dsse_payload_must_be_the_document() {
        let env = serde_json::json!({"payloadType": "t", "payload": BASE64_STD.encode(b"other"), "signatures": []});
        assert!(check_dsse(&env, b"doc", &["k".into()]).is_failure());
    }
}
