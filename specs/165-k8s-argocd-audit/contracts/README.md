# Contracts: milestone 165 — Kubernetes + ArgoCD audit

**No new external contracts.**

Milestone 165 is a docs+measurement milestone (matches milestones 082/093/150/151 pattern). The deliverable is a Markdown report at `docs/audits/2026-07-05-kubernetes-argocd.md`.

- **CLI**: no new flags. No changes to `mikebom sbom scan` behavior.
- **Emitted SBOM shape**: no changes (SC-008 golden byte-identity guard verifies this).
- **Parity catalog**: no new rows.
- **`mikebom:*` annotations**: no new annotations.
- **Tracing conventions**: no new log lines.

The only "contract" milestone 165 defines is the **report structure** (see data-model.md E7 and research.md §R8). That structure is not an interface consumers depend on programmatically — it's a human-readable Markdown template. Future audit rounds (milestone 200, 300, etc.) SHOULD follow the same structure for cross-audit comparability, but this is a documentation convention, not a wire format.
