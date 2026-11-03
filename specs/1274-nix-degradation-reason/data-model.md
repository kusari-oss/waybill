# Data model: the nix-closure tier says why it degraded

**Milestone**: 1274 | **Spec**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

All state is in-process per scan. Nothing persists; the only durable output is
the emitted document's C190 value.

---

## 1. `DegradationReason` — one new variant

`nix/eval/reason.rs` holds a closed set of nine variants. One is added.

```rust
 pub(crate) enum DegradationReason {
     OfflineRequested,
     ToolAbsent,
     ToolUnusable(String),
     IfdRefusalUnverified(String),
     RevisionUnfetchable { revision: String, detail: String },
     NoEvaluableAttribute,
     AmbiguousSystemConfiguration { .. },
     EvaluationFailed(String),
     BudgetExceeded { budget_secs: u64 },
+    /// Evaluation would have required building a derivation, and the tier
+    /// refuses import-from-derivation on purpose (#1034, #1114).
+    ///
+    /// Distinct from `EvaluationFailed` because the operator's next action
+    /// differs: this one tells them what they would have to accept, where
+    /// the generic reason tells them only that something went wrong. It is
+    /// also distinct from `IfdRefusalUnverified`, which means the refusal
+    /// could not be *confirmed to be in effect* — the opposite situation.
+    ImportFromDerivationRequired(String),
 }
```

Wire string: `import-from-derivation-required`.

**FR-013**: every existing variant keeps its wire string and meaning. The new
variant is additive, and the one it is most confusable with —
`ifd-refusal-unverified` — is left exactly as it is.

## 2. The C190 value — string → object

Today `waybill:nix-closure-degraded` is a bare string. Under FR-002a it
becomes:

```json
{
  "reason": "import-from-derivation-required",
  "detail": "error: cannot build '/nix/store/…-cabal2nix-x.drv' during evaluation because the option 'allow-import-from-derivation' is disabled"
}
```

Rendered through `serde_json::Map`, which is a `BTreeMap` here (no
`preserve_order`), so keys sort and the string is stable across runs — the
property a golden needs, and the same one `ClosureSummary::to_document_value`
and `EnrichmentSummary::to_document_value` already depend on.

`detail` is omitted when the reason carries none — `OfflineRequested`,
`ToolAbsent`, `NoEvaluableAttribute` have nothing to add. So the object has
one key or two, never a `"detail": null`.

### What this breaks, measured

| consumer | effect |
|---|---|
| corpus goldens | **none** — no committed golden carries C190 (R4) |
| the three C190 parity extractors | none — they read the value generically |
| `tests/nix_config_closure.rs` | **6 assertions** across 5 tests read the bare string; corrected, not pinned (FR-016) |
| the C190 catalog row's prose | says "document-scope string"; must change with the shape, which `mapping_doc_bidirectional` enforces |
| external consumers | a bare-string reader breaks. This is the wire change FR-002a accepts |

## 3. The salient-line rule — one word

`nix/eval/invoke.rs::salient_stderr` selects where to start keeping text.

```rust
     let start = stderr
         .lines()
-        .position(|l| l.trim_start().starts_with("error") || l.contains(": error:"))
+        .rposition(|l| l.trim_start().starts_with("error") || l.contains(": error:"))
         .unwrap_or(0);
```

**R1 measured why.** Nix emits the stack trace first and the cause last. On a
real `callCabal2nix` refusal the first matching line is a bare `error:`
opener at line 1 of 26, and the explanatory clause is on line 25 — byte 1320
of 1382, which is 1,155 characters past the end of the 300-character window.
`position` fills the window with the trace; `rposition` lands on the cause.

Strictly better rather than a trade-off: where there is one matching line the
two are identical, and where there is a trace `rposition` picks the cause.
Confirmed against both probe cases, including the FR-005a false positive,
which `rposition` still classifies generic.

`unwrap_or(0)` is retained deliberately: it is **FR-009**. Where no line can
be identified as the explanation, something is still recorded — the stderr's
own opening — rather than nothing. An unparseable failure is not an excuse
for silence, and a detail an operator has to interpret beats an absent one.

The function is currently private to `eval/invoke.rs`. FR-008 requires the
closure path to use the **same** rule, so it moves to where both can reach
it — `nix/eval/mod.rs` or a small sibling — rather than being copied. Copying
is the defect this milestone fixes.

## 4. The classifier

A predicate over the selected line, not over the whole stderr.

```rust
/// FR-005a — BOTH conditions, each insufficient alone.
///
/// Measured: a flake whose `nixConfig` requests the setting and then fails
/// for an unrelated cause emits a WARNING naming it —
///   warning: ignoring untrusted flake configuration setting 'allow-import-from-derivation'.
///   error: attribute 'this-attribute-does-not-exist' missing
/// — so matching the bare setting name anywhere in stderr reports a missing
/// attribute as a policy refusal. `slack-web` sets this option, so the false
/// positive is on the path of ordinary use.
const IFD_REFUSED_CLAUSE: &str =
    "because the option 'allow-import-from-derivation' is disabled";
```

Applied to the salient line, which already excludes warnings by construction
— the second of FR-005a's two conditions is satisfied by the selection rather
than by a separate check.

**FR-005c precedence**: where both a refusal and another error appear, the
refusal wins. With `rposition` that is not automatic — the last error line
might be the *other* error — so precedence is an explicit search for the
clause across error lines rather than a property of the selection. This is
the one place where the two rules interact and must not be collapsed.

**FR-006**: no clause, no classification. The generic reason stands, which is
correct behaviour rather than a failure.

## 5. What does NOT change

| invariant | why |
|---|---|
| whether any flake evaluates | FR-010. The control fixture must emit 1,170 derivations into 304 components, byte-identical (SC-003) |
| the refusal itself | FR-011. Whether to offer an opt-in is #1283's second question, pending #971 and #1034 |
| every other degradation reason's wire string | FR-013. `ifd-refusal-unverified` especially: a different condition, easily conflated |
| the catalog row count | FR-002b / R3. C190 exists with three extractors; extending a row is not adding one |
| CLI surface, dependencies, network | FR-011, FR-012, R6 |
