use std::path::PathBuf;

use clap::Args;

use super::generate::{GenerateArgs, SbomScope};
use super::scan::ScanArgs;

/// Parse a `--id <scheme>=<value>` flag for a user-defined identifier.
/// Mirrors `parse_user_defined_id_flag` in `cli/scan_cmd.rs` —
/// rejects built-in schemes at parse time so operators are directed
/// to the dedicated `--repo` / `--git-ref` / `--image-id` /
/// `--attestation` flag.
fn parse_user_defined_id_flag(
    raw: &str,
) -> Result<mikebom::binding::identifiers::Identifier, String> {
    use mikebom::binding::identifiers::{
        BuiltinScheme, Identifier, IdentifierError, IdentifierKind, IdentifierValue, SchemeName,
    };
    let Some(idx) = raw.find('=') else {
        return Err(format!(
            "--id value missing `=` separator: {raw:?} \
             (expected form: --id <scheme>=<value>)"
        ));
    };
    let scheme_str = &raw[..idx];
    let value_str = &raw[idx + 1..];
    let scheme = SchemeName::new(scheme_str.to_string())
        .map_err(|e: IdentifierError| e.to_string())?;
    if let Some(b) = BuiltinScheme::from_scheme_name(&scheme) {
        return Err(format!(
            "--id rejects the built-in scheme `{}` — use the dedicated \
             flag instead (--repo / --git-ref / --image-id / --attestation). \
             --id is for user-defined schemes only.",
            b.as_str()
        ));
    }
    let value =
        IdentifierValue::new(value_str.to_string()).map_err(|e: IdentifierError| e.to_string())?;
    Ok(Identifier::from_parts_with_label(
        scheme,
        value,
        IdentifierKind::UserDefined,
        None,
    ))
}

#[derive(Args)]
pub struct RunArgs {
    /// SBOM output format
    #[arg(long, default_value = "cyclonedx-json")]
    pub format: String,

    /// SBOM output path
    #[arg(long, default_value = "mikebom.cdx.json")]
    pub sbom_output: PathBuf,

    /// Attestation output path
    #[arg(long, default_value = "mikebom.attestation.json")]
    pub attestation_output: PathBuf,

    /// Skip enrichment step
    #[arg(long)]
    pub no_enrich: bool,

    /// Also include observed source files (not just packages)
    #[arg(long)]
    pub include_source_files: bool,

    /// Omit per-component hashes from SBOM
    #[arg(long)]
    pub no_hashes: bool,

    /// Follow forked children of the traced command
    #[arg(long)]
    pub trace_children: bool,

    /// Override libssl.so path for uprobe attachment
    #[arg(long)]
    pub libssl_path: Option<PathBuf>,

    /// Ring buffer size in bytes (must be power of two)
    #[arg(long, default_value = "8388608")]
    pub ring_buffer_size: u32,

    /// Trace timeout in seconds (0 = no timeout)
    #[arg(long, default_value = "0")]
    pub timeout: u64,

    /// Skip online PURL existence validation
    #[arg(long)]
    pub skip_purl_validation: bool,

    /// Path to a lockfile for dependency relationship enrichment
    #[arg(long)]
    pub lockfile: Option<PathBuf>,

    /// Output combined summary as JSON to stdout
    #[arg(long)]
    pub json: bool,

    /// Attach a `repo:` identifier — source repository identity.
    /// Build-tier scans auto-detect `repo:` from `git remote get-url`
    /// when the invocation cwd is a git checkout (milestone 074);
    /// pass this flag to override the auto-detected value. Pair with
    /// `--git-ref <revision>` to upgrade to a `git:` identifier (the
    /// flag overrides the milestone-074 auto-detected `git:` value).
    #[arg(long = "repo", value_name = "URL")]
    pub repo: Option<String>,

    /// Pair with `--repo <url>` to emit a `git:<repo>#<revision>`
    /// identifier (commit/branch/tag-anchored).
    #[arg(long = "git-ref", value_name = "REVISION", requires = "repo")]
    pub git_ref: Option<String>,

    /// Attach an `image:` identifier — image identity. Manual only.
    /// Named `--image-id` to keep the flag-name semantics consistent
    /// with `mikebom sbom scan --image-id`.
    #[arg(long = "image-id", value_name = "REF")]
    pub image_id: Option<String>,

    /// Attach an `attestation:` identifier — in-toto attestation IRI.
    #[arg(long = "attestation", value_name = "IRI")]
    pub attestation: Option<String>,

    /// Attach a user-defined identifier in `<scheme>=<value>` form.
    /// Repeatable. Built-in schemes (`repo`, `git`, `image`,
    /// `attestation`) are rejected — use the dedicated flag instead.
    /// See `mikebom sbom scan --help` for the full identifier docs.
    #[arg(
        long = "id",
        action = clap::ArgAction::Append,
        value_name = "SCHEME=VALUE",
        value_parser = parse_user_defined_id_flag,
    )]
    pub id: Vec<mikebom::binding::identifiers::Identifier>,

    /// Preserve userinfo (e.g., `USER:TOKEN@host`) in auto-detected git
    /// remote URLs when constructing `repo:` and `git:` identifiers.
    /// By default, mikebom strips userinfo to prevent accidental
    /// credential disclosure in published SBOMs. Use this flag only
    /// when the credentials are deliberately non-sensitive (e.g., a
    /// public read-only deploy token, internal-network-only
    /// credentials). Manual `--repo` / `--git-ref` / `--id` flag
    /// values are emitted verbatim regardless of this flag.
    #[arg(long)]
    pub keep_credentials_in_identifiers: bool,

    /// Attach a `subject:` identifier declaring "this SBOM describes
    /// the artifact with the given content hash." Format:
    /// `sha256:<64-lowercase-hex>` or `sha512:<128-lowercase-hex>`.
    /// Repeatable. On `mikebom trace run`, subject identifiers are
    /// auto-detected from the in-toto attestation envelope's subject
    /// set; manual flags augment auto-detected entries (deduplicated
    /// by exact match per milestone 073).
    #[arg(
        long = "subject-hash",
        action = clap::ArgAction::Append,
        value_name = "ALGO:HEX",
    )]
    pub subject_hash: Vec<String>,

    /// Attach a user-defined identifier to a specific component in the
    /// emitted SBOM. The PURL must byte-equal a component's `purl`
    /// field in the emitted output; the SCHEME must be a non-built-in
    /// scheme name (built-in schemes `repo`, `git`, `image`,
    /// `attestation`, `subject` are reserved for document-level use).
    /// See `mikebom sbom scan --help` for full examples. Repeatable.
    /// Zero-match selectors warn and the scan continues.
    #[arg(
        long = "component-id",
        action = clap::ArgAction::Append,
        value_name = "PURL=SCHEME:VALUE",
        value_parser = mikebom::binding::identifiers::component_id::parse_component_id_flag,
    )]
    pub component_id:
        Vec<mikebom::binding::identifiers::component_id::ComponentIdentifierFlag>,

    /// Override the auto-derived `metadata.component.name` of the
    /// emitted SBOM. Same shape and validation as
    /// `mikebom sbom scan --root-name`: accepts any non-empty UTF-8
    /// except whitespace, control characters, `?`, and `#`. URL-encoded
    /// automatically when emitted into the PURL `name` segment. When
    /// the trace produces a manifest-derived main-module component
    /// (Cargo, npm, pip, gem, Maven, Go), setting this flag drops that
    /// component from the emitted SBOM (clean replacement). See GitHub
    /// issue #151 for the demote-to-library follow-up.
    #[arg(
        long = "root-name",
        value_name = "NAME",
        value_parser = super::scan_cmd::validate_root_name,
    )]
    pub root_name: Option<String>,

    /// Override the auto-derived `metadata.component.version`. Same
    /// validation rules as `--root-name`. Independent — can be set
    /// without `--root-name` and vice versa.
    #[arg(
        long = "root-version",
        value_name = "VERSION",
        value_parser = super::scan_cmd::validate_root_version,
    )]
    pub root_version: Option<String>,

    // ────────────────────────────────────────────────────────────
    // Milestone 080 — user-provided SBOM metadata. Symmetric with
    // `mikebom sbom scan`. See `specs/080-user-sbom-metadata/`.
    // ────────────────────────────────────────────────────────────

    /// Attach a creator entry to the emitted SBOM. Repeatable. Form:
    /// `<Type>: <Name>` where `<Type>` is one of `Tool`, `Organization`,
    /// `Person` (case-sensitive). See `mikebom sbom scan --help` for
    /// per-format landing details.
    #[arg(
        long = "creator",
        action = clap::ArgAction::Append,
        value_name = "TYPE: NAME",
    )]
    pub creator: Vec<String>,

    /// Attach a document-level annotator. Repeatable. MUST pair 1:1
    /// with `--annotation-comment`.
    #[arg(
        long = "annotator",
        action = clap::ArgAction::Append,
        value_name = "TYPE: NAME",
    )]
    pub annotator: Vec<String>,

    /// Free-text comment that pairs positionally with the preceding
    /// `--annotator` flag. Repeatable.
    #[arg(
        long = "annotation-comment",
        action = clap::ArgAction::Append,
        value_name = "TEXT",
    )]
    pub annotation_comment: Vec<String>,

    /// Free-text comment about the SBOM document as a whole.
    /// Single-valued.
    #[arg(long = "metadata-comment", value_name = "TEXT")]
    pub metadata_comment: Option<String>,

    /// Operator-supplied override for the document/Sbom-level name
    /// field. See `mikebom sbom scan --help` for the per-format
    /// precedence rules vs. `--root-name`.
    #[arg(long = "scan-target-name", value_name = "NAME")]
    pub scan_target_name: Option<String>,

    /// Path to a JSON sidecar file containing user-supplied metadata.
    /// See `mikebom sbom scan --help` for the schema.
    #[arg(long = "metadata-file", value_name = "PATH")]
    pub metadata_file: Option<std::path::PathBuf>,

    /// Override the auto-detected SBOM type with an operator-asserted
    /// CISA SBOM Type. Valid values: design, source, build, analyzed,
    /// deployed, runtime. Document-level only — per-component
    /// `mikebom:sbom-tier` annotations preserve auto-detected values.
    /// See `mikebom sbom scan --help` for full per-format details.
    #[arg(
        long = "sbom-type",
        value_name = "TYPE",
        value_parser = super::scan_cmd::parse_sbom_type_flag,
    )]
    pub sbom_type:
        Option<crate::generate::lifecycle_phases::SbomType>,

    /// Directories to scan for artifact files after the traced command
    /// exits. Forwarded verbatim to `mikebom trace capture`. See the
    /// `--artifact-dir` flag there for details.
    #[arg(long, value_delimiter = ',')]
    pub artifact_dir: Vec<PathBuf>,

    /// Auto-detect artifact directories from the traced command. See
    /// `mikebom trace capture --help` for the supported tool list.
    #[arg(long)]
    pub auto_dirs: bool,

    // ─────────────────────────────────────────────────────────────
    // Feature 006 — DSSE signing flags forwarded to the scan phase.
    // ─────────────────────────────────────────────────────────────
    /// Path to a PEM-encoded private key for local-key DSSE signing.
    #[arg(long, conflicts_with = "keyless")]
    pub signing_key: Option<PathBuf>,

    /// Env var name holding the passphrase for an encrypted
    /// `--signing-key`.
    #[arg(long, value_name = "NAME")]
    pub signing_key_passphrase_env: Option<String>,

    /// Keyless signing via OIDC → Fulcio → Rekor.
    #[arg(long)]
    pub keyless: bool,

    /// Override Fulcio URL.
    #[arg(long, default_value = "https://fulcio.sigstore.dev")]
    pub fulcio_url: String,

    /// Override Rekor URL.
    #[arg(long, default_value = "https://rekor.sigstore.dev")]
    pub rekor_url: String,

    /// Skip Rekor upload + inclusion-proof embedding (keyless mode).
    #[arg(long)]
    pub no_transparency_log: bool,

    /// Fail if no signing identity was configured.
    #[arg(long)]
    pub require_signing: bool,

    /// Explicit subject artifact path (feature 006 US3). Repeatable.
    /// When set, auto-detection is suppressed.
    #[arg(long = "subject", value_name = "PATH")]
    pub subject: Vec<PathBuf>,

    /// Attestation output format (feature 006). Default `witness-v0.1`
    /// — compatible with `sbomit generate` and go-witness verifiers.
    #[arg(long = "attestation-format", value_name = "FORMAT", default_value = "witness-v0.1")]
    pub attestation_format: String,

    /// Build command to trace
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
}

pub async fn execute(args: RunArgs) -> anyhow::Result<()> {
    // Milestone 080 — strict-interleaving check on raw argv before
    // anything else (research §3 UX-friendly early diagnostic).
    let argv: Vec<String> = std::env::args().collect();
    if let Err(diag) =
        mikebom::binding::user_metadata::validate_annotator_pair_interleaving(&argv)
    {
        anyhow::bail!(diag);
    }

    // Milestone 074 — capture the invocation cwd ONCE, before any
    // subprocess work or trace capture. Wrapped-command later cwd
    // changes (e.g., the script does `cd build/`) have no effect on
    // identifier auto-detection — keeps detection deterministic per
    // FR-009 + spec edge-case "Build cwd vs wrapped-command cwd".
    let invocation_cwd = std::env::current_dir()?;

    // Milestone 074 — auto-detect build-tier identifiers (`repo:` and
    // `git:`) from the invocation cwd if it's a git checkout.
    // Returns a 0/1/2-element vec depending on git remote state and
    // HEAD commit availability. Non-git or other failure modes
    // collapse to an empty vec via `tracing::info!` per FR-003.
    let auto_detected_ids =
        mikebom::binding::identifiers::auto_detect::auto_detect_build_tier_identifiers(
            &invocation_cwd,
            args.keep_credentials_in_identifiers,
        );

    // Phase 1: capture the trace → attestation.
    let scan_args = ScanArgs {
        target_pid: None,
        output: args.attestation_output.clone(),
        trace_children: args.trace_children,
        libssl_path: args.libssl_path.clone(),
        go_binary: None,
        ring_buffer_size: args.ring_buffer_size,
        timeout: args.timeout,
        json: false,
        artifact_dir: args.artifact_dir.clone(),
        auto_dirs: args.auto_dirs,
        // Feature 006 — forward signing flags verbatim.
        signing_key: args.signing_key.clone(),
        signing_key_passphrase_env: args.signing_key_passphrase_env.clone(),
        keyless: args.keyless,
        fulcio_url: args.fulcio_url.clone(),
        rekor_url: args.rekor_url.clone(),
        no_transparency_log: args.no_transparency_log,
        require_signing: args.require_signing,
        subject: args.subject.clone(),
        attestation_format: args.attestation_format.clone(),
        command: args.command.clone(),
    };
    super::scan::execute(scan_args).await?;

    // Milestone 076 — read the attestation file the trace pipeline
    // just produced, extract its `subject[]` array (the in-toto
    // ResourceDescriptor list), and synthesize `subject:` identifiers
    // per FR-002. Subjects without sha256 digests skip with an
    // info-log; subjects with sha256 emit `subject:sha256:<hex>`
    // identifiers in input order. Append AFTER milestone-074's
    // auto-detected `repo:` / `git:` entries per data-model.md
    // "Updated `auto_detect_build_tier_identifiers` flow".
    let mut auto_detected_ids = auto_detected_ids;
    match crate::attestation::serializer::read_attestation(&args.attestation_output) {
        Ok(statement) => {
            let subject_ids =
                mikebom::binding::identifiers::auto_detect::subject_identifiers_from_attestation_subjects(
                    &statement.subject,
                );
            auto_detected_ids.extend(subject_ids);
        }
        Err(e) => {
            tracing::info!(
                error = %e,
                attestation = %args.attestation_output.display(),
                "could not re-read attestation for build-tier subject \
                 auto-detect; skipping subject: identifier auto-emit"
            );
        }
    }

    // Milestone 073/074 — assemble manual identifiers from dedicated
    // flags. Order: repo-or-git → image → attestation → --subject-hash
    // → user-defined --id flags. Then route through the shared
    // `resolve_identifiers` helper (milestone 074 T005 refactor) which
    // applies the FR-006 manual-wins precedence + dedup-by-(scheme,
    // value) per scheme — supporting the build-tier multi-auto-detected
    // case (now including auto-detected `subject:` from milestone 076).
    let mut manual_ids: Vec<mikebom::binding::identifiers::Identifier> = Vec::new();
    if let Some(repo_url) = args.repo.as_deref() {
        let raw = if let Some(rev) = args.git_ref.as_deref() {
            format!("git:{repo_url}#{rev}")
        } else {
            format!("repo:{repo_url}")
        };
        match mikebom::binding::identifiers::Identifier::parse(&raw) {
            Ok(id) => manual_ids.push(id),
            Err(e) => tracing::warn!(
                error = %e,
                raw = %raw,
                "failed to parse manual --repo/--git-ref identifier on trace; skipping"
            ),
        }
    }
    if let Some(image) = args.image_id.as_deref() {
        let raw = format!("image:{image}");
        match mikebom::binding::identifiers::Identifier::parse(&raw) {
            Ok(id) => manual_ids.push(id),
            Err(e) => tracing::warn!(
                error = %e,
                raw = %raw,
                "failed to parse manual --image-id identifier on trace; skipping"
            ),
        }
    }
    if let Some(att) = args.attestation.as_deref() {
        let raw = format!("attestation:{att}");
        match mikebom::binding::identifiers::Identifier::parse(&raw) {
            Ok(id) => manual_ids.push(id),
            Err(e) => tracing::warn!(
                error = %e,
                raw = %raw,
                "failed to parse manual --attestation identifier on trace; skipping"
            ),
        }
    }
    // Milestone 076 — manual --subject-hash flags. Wrap each
    // `<algo>:<hex>` value into a full `subject:<algo>:<hex>` shape
    // before parsing so the soft-fail-to-UserDefined path triggers
    // identically to other built-ins on validation failure (FR-005).
    for sh in &args.subject_hash {
        let raw = format!("subject:{sh}");
        match mikebom::binding::identifiers::Identifier::parse(&raw) {
            Ok(id) => manual_ids.push(id),
            Err(e) => tracing::warn!(
                error = %e,
                raw = %raw,
                "failed to parse manual --subject-hash identifier on trace; skipping"
            ),
        }
    }
    for id in &args.id {
        manual_ids.push(id.clone());
    }
    let assembled_ids =
        mikebom::binding::identifiers::resolve_identifiers(auto_detected_ids, &manual_ids);

    // Milestone 080 — assemble user-supplied SBOM metadata from the
    // trace-run flags. Same shape and merge rules as `mikebom sbom
    // scan`. The emission timestamp is captured here so all
    // annotations share a single deterministic value.
    let metadata_file = match args.metadata_file.as_deref() {
        Some(p) => Some(
            mikebom::binding::user_metadata::load_metadata_file(p)
                .map_err(|e| anyhow::anyhow!(e))?,
        ),
        None => None,
    };
    let trace_emission_ts = chrono::Utc::now();
    let trace_user_metadata = mikebom::binding::user_metadata::merge_file_and_flags(
        metadata_file,
        args.creator.clone(),
        args.annotator.clone(),
        args.annotation_comment.clone(),
        args.metadata_comment.clone(),
        args.scan_target_name.clone(),
        trace_emission_ts,
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let trace_org_count = trace_user_metadata
        .creators
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                mikebom::binding::user_metadata::CreatorKind::Organization
            )
        })
        .count();
    if trace_org_count > 1 {
        eprintln!(
            "warning: {} `--creator \"Organization: ...\"` entries supplied; \
             CDX 1.6 permits exactly one `metadata.manufacturer` so the first \
             Organization creator populates that slot and the rest are routed \
             to `bom.annotations[]`.",
            trace_org_count
        );
    }
    if trace_user_metadata.scan_target_name.is_some() && args.root_name.is_some() {
        eprintln!(
            "warning: --root-name overrides --scan-target-name for CDX \
             metadata.component.name; SPDX 2.3 / SPDX 3 honor both \
             independently."
        );
    }

    // Phase 2: derive the SBOM from the attestation.
    let generate_args = GenerateArgs {
        attestation_file: args.attestation_output.clone(),
        format: args.format.clone(),
        output: args.sbom_output.clone(),
        scope: if args.include_source_files {
            SbomScope::Source
        } else {
            SbomScope::Packages
        },
        no_hashes: args.no_hashes,
        enrich: !args.no_enrich,
        lockfile: args.lockfile.clone(),
        deps_dev_timeout: 5000,
        skip_purl_validation: args.skip_purl_validation,
        vex_overrides: None,
        json: false,
        identifiers: assembled_ids,
        // Milestone 076 — forward per-component user-defined
        // identifiers for the CDX build-tier emission path.
        component_identifiers: args.component_id.clone(),
        // Milestone 077 — forward operator-supplied root-component
        // override from the trace-run flags. Empty default when
        // neither flag was passed (back-compat).
        root_override: crate::generate::RootComponentOverride {
            name: args.root_name.clone(),
            version: args.root_version.clone(),
        },
        // Milestone 080 — forward aggregated user-supplied SBOM
        // metadata from the trace-run flags.
        user_metadata: trace_user_metadata,
        // Milestone 081 — forward the operator-asserted CISA SBOM
        // Type override from the trace-run `--sbom-type` flag.
        sbom_type_override: args.sbom_type,
    };
    // Trace's one-shot `run` wrapper doesn't thread the global --offline
    // flag through (yet). Default to online — the enrichment doesn't
    // block success when deps.dev is unreachable, so offline users get
    // the same SBOM minus the license/CPE upgrades.
    super::generate::execute(generate_args, false).await?;

    if args.json {
        let summary = serde_json::json!({
            "attestation_file": args.attestation_output.to_string_lossy(),
            "sbom_file": args.sbom_output.to_string_lossy(),
            "format": args.format,
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
    }

    tracing::info!(
        attestation = %args.attestation_output.display(),
        sbom = %args.sbom_output.display(),
        "trace run complete"
    );
    Ok(())
}
