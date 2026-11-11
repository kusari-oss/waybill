//! Why the evaluation tier did not contribute to a scan.
//!
//! Every variant degrades: the scan emits file-parsing results unchanged and
//! exits successfully (spec FR-011). Constitution Principle III ("fail closed")
//! is scoped by its own text to the eBPF trace path; for enrichment, Principle
//! XI requires the opposite — emit the document with the field omitted and a
//! transparency annotation naming the gap.
//!
//! The variants are deliberately finer-grained than "it did not work", because
//! the operator's remedy differs: a missing `nix` is installed, an unusable one
//! is started, and one too old to honour the import-from-derivation refusal is
//! upgraded.

/// The closed set of reasons the tier degraded (spec FR-013).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum DegradationReason {
    /// The operator asked for `--offline`, which the tier cannot honour.
    ///
    /// Evaluation resolves the pinned revision through `builtins.getFlake`,
    /// and Nix fetches it when the store does not already hold it — measured:
    /// `unpacking 'github:NixOS/nixpkgs/<rev>' into the Git cache...`. Nix's
    /// own `--offline` does not prevent this; it governs substituters, not
    /// flake inputs.
    ///
    /// The tier *would* succeed on a warm store, so refusing costs a real
    /// capability. It is refused anyway because `--offline` promises "disable
    /// all outbound network calls", and a promise kept only when the cache
    /// happens to be warm is not one.
    #[error("`--offline` was requested; evaluation may fetch the pinned revision")]
    OfflineRequested,

    /// No `nix` on `PATH`.
    #[error("no `nix` on PATH")]
    ToolAbsent,

    /// `nix` is present but cannot be used — daemon down, store not writable.
    #[error("`nix` is present but unusable: {0}")]
    ToolUnusable(String),

    /// The import-from-derivation refusal could not be confirmed to be in
    /// effect.
    ///
    /// This is its own variant rather than a flavour of [`Self::ToolUnusable`]
    /// because `nix` accepts an unknown setting with a warning and exit code 0
    /// (research R3). A `nix` that does not support
    /// `allow-import-from-derivation` would take the flag, ignore it, and
    /// evaluate with import-from-derivation *enabled* — so refusing to
    /// evaluate at all is the only safe response, and the operator needs to
    /// know it was their `nix` version rather than their daemon.
    #[error("could not confirm `allow-import-from-derivation = false` is in effect: {0}")]
    IfdRefusalUnverified(String),

    /// Milestone 1283 (#1283) — `--nix-allow-ifd` was passed and nix did not
    /// report `allow-import-from-derivation = true`.
    ///
    /// The mirror of [`Self::IfdRefusalUnverified`]: requesting the
    /// allowance is not evidence it applied, so nothing is evaluated. Its own
    /// variant because the remedy differs — here the operator's nix (or a
    /// configuration that outranks the command line) is declining what they
    /// opted into.
    #[error("could not confirm `allow-import-from-derivation = true` is in effect: {0}")]
    IfdAllowanceUnverified(String),

    /// The pinned nixpkgs revision could not be acquired.
    #[error("could not acquire nixpkgs revision {revision}: {detail}")]
    RevisionUnfetchable { revision: String, detail: String },

    /// The flake exposes nothing this tier can evaluate.
    ///
    /// Not an error: haskell-language-server's flake exposes no `default`
    /// package output at all, only `docs` and devShells (spec, measured).
    #[error("no evaluable attribute path in the flake")]
    NoEvaluableAttribute,

    /// Milestone 1066 (#1052): the flake has no `packages` output and
    /// defines more than one system configuration. waybill does not pick one:
    /// choosing by hostname or by order would make the same command describe
    /// different machines on different hosts.
    #[error(
        "several system configurations ({names}); choose one with \
         --nix-closure-attr, e.g. --nix-closure-attr {example}"
    )]
    AmbiguousSystemConfiguration {
        /// Qualified names (`<kind>.<name>`), sorted, joined with `, `.
        names: String,
        /// One full system output path the operator could pass.
        example: String,
    },

    /// Evaluation would have required building a derivation, and the tier
    /// refuses import-from-derivation on purpose (#1034, #1114).
    ///
    /// Milestone 1274 (#1283 part 1). Distinct from [`Self::EvaluationFailed`]
    /// because the operator's next action differs: this one tells them *what
    /// they would have to accept*, where the generic reason tells them only
    /// that something went wrong. Measured: a `callCabal2nix` flake and a
    /// handwritten one for the same `.cabal` both reported
    /// `evaluation-failed`, so a deliberate policy refusal was
    /// indistinguishable from a broken flake.
    ///
    /// Also distinct from [`Self::IfdRefusalUnverified`], and close to its
    /// opposite. That variant means the refusal could not be *confirmed to be
    /// in effect* — the installed `nix` may not honour the setting, so
    /// nothing was evaluated. This one means the refusal was in effect and
    /// did its job. Conflating them would tell an operator their `nix` is too
    /// old when in fact their flake needs a grant.
    #[error("evaluation requires import-from-derivation, which is refused: {0}")]
    ImportFromDerivationRequired(String),

    /// Milestone 1283 (#1283) — import-from-derivation was allowed
    /// (`--nix-allow-ifd`), and a derivation nix built during evaluation
    /// failed.
    ///
    /// Distinct from [`Self::ImportFromDerivationRequired`], which is the
    /// policy refusing, and from [`Self::EvaluationFailed`]: the flake's own
    /// generated expression did not build, which is a property of the flake
    /// on this host (slacklinker's build fails `disallowedReferences` on
    /// darwin), not of waybill's policy (spec FR-010).
    #[error("a derivation built during evaluation (import-from-derivation) failed: {0}")]
    ImportFromDerivationBuildFailed(String),

    /// `nix` ran and exited non-zero.
    #[error("`nix eval` failed: {0}")]
    EvaluationFailed(String),

    /// Evaluation outlasted its wall-clock budget.
    ///
    /// waybill imposes this bound because `nix` does not: `max-call-depth`
    /// stops runaway *recursion*, and `timeout` is the *build* timeout, which
    /// this tier never reaches. A shallow non-recursive evaluation ran past 45
    /// seconds unimpeded in the research probe (R4).
    #[error("evaluation exceeded its {budget_secs}s budget")]
    BudgetExceeded { budget_secs: u64 },
}

impl DegradationReason {
    /// The stable wire form emitted in `waybill:nix-eval-degraded` (C180).
    ///
    /// Kebab-case and free of the variants' interpolated detail, so the value
    /// is a closed set a consumer can branch on. The human-readable detail
    /// stays in the log line.
    pub(crate) fn wire(&self) -> &'static str {
        match self {
            Self::OfflineRequested => "offline-requested",
            Self::ToolAbsent => "tool-absent",
            Self::ToolUnusable(_) => "tool-unusable",
            Self::IfdRefusalUnverified(_) => "ifd-refusal-unverified",
            Self::IfdAllowanceUnverified(_) => "ifd-allowance-unverified",
            Self::RevisionUnfetchable { .. } => "revision-unfetchable",
            Self::NoEvaluableAttribute => "no-evaluable-attribute",
            Self::AmbiguousSystemConfiguration { .. } => "several-system-configurations",
            Self::ImportFromDerivationRequired(_) => "import-from-derivation-required",
            Self::ImportFromDerivationBuildFailed(_) => "import-from-derivation-build-failed",
            Self::EvaluationFailed(_) => "evaluation-failed",
            Self::BudgetExceeded { .. } => "budget-exceeded",
        }
    }

    /// The operator-actionable text this reason carries, if any.
    ///
    /// Milestone 1274 (#1283 part 1). Returns `None` for the variants that
    /// have nothing to add: `offline-requested`, `tool-absent` and
    /// `no-evaluable-attribute` are fully described by their wire string, and
    /// `several-system-configurations` and `budget-exceeded` put their detail
    /// in [`Display`] where it is already reconstructible from the document's
    /// other fields.
    ///
    /// An exhaustive `match` rather than a `matches!` allowlist: a
    /// `_ => None` arm would silently drop the detail of every variant added
    /// after this one, and that is the exact shape of the defect #1218 and
    /// #892 both recorded.
    ///
    /// [`Display`]: std::fmt::Display
    pub(crate) fn detail(&self) -> Option<&str> {
        match self {
            Self::OfflineRequested | Self::ToolAbsent | Self::NoEvaluableAttribute => None,
            Self::AmbiguousSystemConfiguration { .. } | Self::BudgetExceeded { .. } => None,
            Self::ToolUnusable(d)
            | Self::IfdRefusalUnverified(d)
            | Self::IfdAllowanceUnverified(d)
            | Self::ImportFromDerivationRequired(d)
            | Self::ImportFromDerivationBuildFailed(d)
            | Self::EvaluationFailed(d) => Some(d.as_str()),
            Self::RevisionUnfetchable { detail, .. } => Some(detail.as_str()),
        }
    }

    /// A sentence for the operator log saying what this reason means for
    /// them, where the wire string and [`Display`] do not already.
    ///
    /// #1446. Log-only: C190 is unchanged. Exhaustive for the same reason
    /// [`Self::detail`] is.
    ///
    /// [`Display`]: std::fmt::Display
    pub(crate) fn explanation(&self) -> Option<&'static str> {
        match self {
            // Milestone 1283 (FR-006): the second sentence names the opt-in
            // and repeats the untrusted-flake caution where the operator
            // decides.
            Self::ImportFromDerivationRequired(_) => Some(
                "The flake evaluates through import-from-derivation (e.g. callCabal2nix), \
                 which waybill refuses because it would build during evaluation. \
                 Pass --nix-allow-ifd to allow it, only for a flake you trust.",
            ),
            Self::OfflineRequested
            | Self::ToolAbsent
            | Self::ToolUnusable(_)
            | Self::IfdRefusalUnverified(_)
            | Self::IfdAllowanceUnverified(_)
            | Self::RevisionUnfetchable { .. }
            | Self::NoEvaluableAttribute
            | Self::AmbiguousSystemConfiguration { .. }
            | Self::ImportFromDerivationBuildFailed(_)
            | Self::EvaluationFailed(_)
            | Self::BudgetExceeded { .. } => None,
        }
    }

    /// The C190 / C180 document value: `{"reason": ...}`, plus `"detail"`
    /// when there is one.
    ///
    /// **FR-002a / FR-004 — one carrier.** Built here rather than in each
    /// emitter so no emitter learns about any individual variant, and so the
    /// three formats cannot drift (the C190 parity extractors assert they are
    /// equal).
    ///
    /// Rendered through `serde_json::Map`, which is a `BTreeMap` in this
    /// build — no `preserve_order` feature — so the keys sort and the string
    /// is byte-stable across runs. That is what lets a golden pin it, and it
    /// is the same property `ClosureSummary::to_document_value` relies on.
    ///
    /// `detail` is **omitted**, never `null`: a consumer testing for the
    /// key's presence is then asking the question it means to ask.
    pub(crate) fn to_document_value(&self) -> String {
        let mut m = serde_json::Map::new();
        m.insert("reason".into(), serde_json::Value::from(self.wire()));
        if let Some(d) = self.detail() {
            m.insert("detail".into(), serde_json::Value::from(d));
        }
        serde_json::Value::Object(m).to_string()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn wire_forms_are_the_closed_set_fr_013_names() {
        let all = [
            DegradationReason::OfflineRequested,
            DegradationReason::ToolAbsent,
            DegradationReason::ToolUnusable("x".into()),
            DegradationReason::IfdRefusalUnverified("x".into()),
            DegradationReason::RevisionUnfetchable {
                revision: "abc".into(),
                detail: "x".into(),
            },
            DegradationReason::NoEvaluableAttribute,
            DegradationReason::AmbiguousSystemConfiguration {
                names: "a".into(),
                example: "b".into(),
            },
            DegradationReason::ImportFromDerivationRequired("x".into()),
            DegradationReason::EvaluationFailed("x".into()),
            DegradationReason::BudgetExceeded { budget_secs: 1 },
            DegradationReason::IfdAllowanceUnverified("x".into()),
            DegradationReason::ImportFromDerivationBuildFailed("x".into()),
        ];
        let wires: Vec<_> = all.iter().map(|r| r.wire()).collect();
        // m1034 FR-013's eight, plus m1066's `several-system-configurations`,
        // plus m1274's `import-from-derivation-required`, plus m1283's
        // `ifd-allowance-unverified` and `import-from-derivation-build-failed`.
        assert_eq!(wires.len(), 12, "the closed set has exactly twelve reasons");

        let mut sorted = wires.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 12, "wire forms must be distinct: {wires:?}");
        // m1283 — named, not counted: a consumer branches on these strings.
        assert!(wires.contains(&"ifd-allowance-unverified"), "{wires:?}");
        assert!(wires.contains(&"import-from-derivation-build-failed"), "{wires:?}");

        for w in &wires {
            assert!(
                !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "wire form {w:?} must be kebab-case ascii"
            );
        }
    }

    /// FR-013 (m1274) — the nine wire strings that existed before milestone
    /// 1274 are unchanged. Named rather than counted: a count survives a
    /// rename, and a consumer branching on these strings does not.
    #[test]
    fn the_pre_1274_wire_strings_are_unchanged() {
        for (reason, wire) in [
            (DegradationReason::OfflineRequested, "offline-requested"),
            (DegradationReason::ToolAbsent, "tool-absent"),
            (DegradationReason::ToolUnusable("x".into()), "tool-unusable"),
            (
                DegradationReason::IfdRefusalUnverified("x".into()),
                "ifd-refusal-unverified",
            ),
            (
                DegradationReason::RevisionUnfetchable {
                    revision: "abc".into(),
                    detail: "x".into(),
                },
                "revision-unfetchable",
            ),
            (DegradationReason::NoEvaluableAttribute, "no-evaluable-attribute"),
            (
                DegradationReason::AmbiguousSystemConfiguration {
                    names: "a".into(),
                    example: "b".into(),
                },
                "several-system-configurations",
            ),
            (DegradationReason::EvaluationFailed("x".into()), "evaluation-failed"),
            (DegradationReason::BudgetExceeded { budget_secs: 1 }, "budget-exceeded"),
        ] {
            assert_eq!(reason.wire(), wire, "FR-013: this wire string is published");
        }
    }

    /// The one distinction most easily lost. `ifd-refusal-unverified` means
    /// the refusal could not be confirmed to be in effect; the new reason
    /// means it was in effect and stopped the evaluation. Conflating them
    /// sends an operator to upgrade `nix` when their flake needs a grant.
    #[test]
    fn the_refusal_and_the_unverified_refusal_are_different_reasons() {
        let in_effect = DegradationReason::ImportFromDerivationRequired("d".into());
        let unverified = DegradationReason::IfdRefusalUnverified("d".into());
        assert_ne!(in_effect.wire(), unverified.wire());
        assert_eq!(in_effect.wire(), "import-from-derivation-required");
        assert_eq!(unverified.wire(), "ifd-refusal-unverified");
    }

    /// FR-002a — the shape, and the sorted keys a golden depends on.
    #[test]
    fn the_document_value_is_an_object_with_sorted_keys() {
        let r = DegradationReason::ImportFromDerivationRequired(
            "error: cannot build 'x.drv' during evaluation because the option \
             'allow-import-from-derivation' is disabled"
                .into(),
        );
        let v = r.to_document_value();
        assert!(
            v.starts_with(r#"{"detail":"#),
            "keys must sort, so `detail` precedes `reason`: {v}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&v).unwrap();
        assert_eq!(parsed["reason"], "import-from-derivation-required");
        assert!(parsed["detail"].as_str().unwrap().contains("is disabled"));
        assert_eq!(v, r.to_document_value(), "byte-stable across calls");
    }

    /// Omitted, never null. A consumer testing for the key then asks the
    /// question it means to ask.
    #[test]
    fn a_reason_with_no_detail_omits_the_key_entirely() {
        for r in [
            DegradationReason::OfflineRequested,
            DegradationReason::ToolAbsent,
            DegradationReason::NoEvaluableAttribute,
        ] {
            let v = r.to_document_value();
            assert!(!v.contains("detail"), "{v}");
            assert!(!v.contains("null"), "{v}");
            let parsed: serde_json::Value = serde_json::from_str(&v).unwrap();
            assert_eq!(parsed.as_object().unwrap().len(), 1, "{v}");
        }
    }

    /// Every variant answers, and the answer is a decision rather than a
    /// default. Enumerated so a variant added later has to choose.
    #[test]
    fn every_variant_decides_whether_it_has_a_detail() {
        let with = [
            DegradationReason::ToolUnusable("d".into()),
            DegradationReason::IfdRefusalUnverified("d".into()),
            DegradationReason::IfdAllowanceUnverified("d".into()),
            DegradationReason::ImportFromDerivationRequired("d".into()),
            DegradationReason::ImportFromDerivationBuildFailed("d".into()),
            DegradationReason::EvaluationFailed("d".into()),
            DegradationReason::RevisionUnfetchable {
                revision: "abc".into(),
                detail: "d".into(),
            },
        ];
        for r in &with {
            assert_eq!(r.detail(), Some("d"), "{}", r.wire());
        }
        let without = [
            DegradationReason::OfflineRequested,
            DegradationReason::ToolAbsent,
            DegradationReason::NoEvaluableAttribute,
            DegradationReason::AmbiguousSystemConfiguration {
                names: "a".into(),
                example: "b".into(),
            },
            DegradationReason::BudgetExceeded { budget_secs: 1 },
        ];
        for r in &without {
            assert_eq!(r.detail(), None, "{}", r.wire());
        }
        assert_eq!(with.len() + without.len(), 12, "all twelve variants are covered");
    }

    /// #1446 — only the import-from-derivation refusal needs a sentence its
    /// wire string and Display do not already give.
    #[test]
    fn only_the_ifd_refusal_carries_an_explanation() {
        let ifd = DegradationReason::ImportFromDerivationRequired("d".into());
        let e = ifd.explanation().unwrap();
        assert!(e.contains("callCabal2nix") && e.contains("would build during evaluation"), "{e}");
        // m1283 (FR-006): names the opt-in, with the caution.
        assert!(e.contains("--nix-allow-ifd") && e.contains("only for a flake you trust"), "{e}");
        for r in [
            DegradationReason::OfflineRequested,
            DegradationReason::IfdRefusalUnverified("d".into()),
            DegradationReason::IfdAllowanceUnverified("d".into()),
            DegradationReason::ImportFromDerivationBuildFailed("d".into()),
            DegradationReason::EvaluationFailed("d".into()),
        ] {
            assert_eq!(r.explanation(), None, "{}", r.wire());
        }
    }

    #[test]
    fn wire_form_carries_no_interpolated_detail() {
        // A consumer branches on the wire form, so it must not vary with the
        // detail string — that is what keeps the set closed.
        let a = DegradationReason::ToolUnusable("daemon down".into());
        let b = DegradationReason::ToolUnusable("store read-only".into());
        assert_eq!(a.wire(), b.wire());
        assert_ne!(a.to_string(), b.to_string(), "detail belongs in Display");
    }

    #[test]
    fn several_configurations_names_them_and_how_to_choose() {
        let r = DegradationReason::AmbiguousSystemConfiguration {
            names: "darwinConfigurations.laptop, nixosConfigurations.web01".into(),
            example: "darwinConfigurations.laptop.system".into(),
        };
        assert_eq!(r.wire(), "several-system-configurations");
        let msg = r.to_string();
        assert!(msg.contains("darwinConfigurations.laptop, nixosConfigurations.web01"), "{msg}");
        assert!(msg.contains("--nix-closure-attr darwinConfigurations.laptop.system"), "{msg}");
    }
}
