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

    /// The pinned nixpkgs revision could not be acquired.
    #[error("could not acquire nixpkgs revision {revision}: {detail}")]
    RevisionUnfetchable { revision: String, detail: String },

    /// The flake exposes nothing this tier can evaluate.
    ///
    /// Not an error: haskell-language-server's flake exposes no `default`
    /// package output at all, only `docs` and devShells (spec, measured).
    #[error("no evaluable attribute path in the flake")]
    NoEvaluableAttribute,

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
            Self::RevisionUnfetchable { .. } => "revision-unfetchable",
            Self::NoEvaluableAttribute => "no-evaluable-attribute",
            Self::EvaluationFailed(_) => "evaluation-failed",
            Self::BudgetExceeded { .. } => "budget-exceeded",
        }
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
            DegradationReason::EvaluationFailed("x".into()),
            DegradationReason::BudgetExceeded { budget_secs: 1 },
        ];
        let wires: Vec<_> = all.iter().map(|r| r.wire()).collect();
        assert_eq!(wires.len(), 8, "FR-013 enumerates exactly eight reasons");

        let mut sorted = wires.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 8, "wire forms must be distinct: {wires:?}");

        for w in &wires {
            assert!(
                !w.is_empty() && w.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "wire form {w:?} must be kebab-case ascii"
            );
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
}
