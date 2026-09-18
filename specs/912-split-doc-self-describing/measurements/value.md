# T001 — has this feature's value been established?

**Question**: does a consumer read a per-resolve split document *without* its
manifest? If not, #914 is tidiness, and Phase 2 (the namespace work, the
largest block) is spent on an unexamined premise.

## Finding 1 — the manifest premise is true, including for discovered resolves

#914's own text asserts "the manifest already answers the practical question".
Verified in code rather than assumed.

`waybill-cli/src/generate/split.rs:278-281`:

```rust
let root_purl = anchor
    .as_ref()
    .map(|c| c.purl.clone())
    .or_else(|| Purl::new(&format!("pkg:generic/{resolve}")).ok());
```

A discovered resolve has no anchor, so the `or_else` arm fires and the manifest
entry still carries `pkg:generic/default`, `pkg:generic/lint`. The manifest
answers "which resolve is this file" for **every** entry, declared or not.

So the gap is exactly and only the one #914 names: a document separated from
its manifest. Nothing narrower.

## Finding 2 — the manifest's answer carries the ambiguity FR-001a exists to remove

That fallback interpolates the **bare** resolve name. It is the same bare name
`--split=resolve` groups on (`split.rs:219`), which is what #919 is about. A
repository declaring `default` under both `[python.resolves]` and
`[jvm.resolves]` gets one manifest entry reading `pkg:generic/default`, with
nothing saying which namespace.

This cuts both ways and should be said plainly:

- **Against building**: the manifest already answers the practical question, so
  the document-side identity is redundant wherever the manifest travels.
- **For building**: "just use the manifest" is not a complete answer even when
  the manifest *is* present, because the manifest's answer is ambiguous in
  precisely the case FR-001a was written for. Fixing it in the manifest is the
  same work as fixing it in the document — the namespace has to be recorded
  either way (R2).

## Finding 3 — the resolve name already exists in the projection, as a non-component

The `SubprojectRoot` built from that fallback is not a `ResolvedComponent`, so
the root-selector never sees it and `metadata.component` stays the repository —
matching the behaviour #914 reports. This is FR-006 working as designed, and it
means deriving the identity is cheap. **The cost of this feature is Phase 2,
not Phase 3.**

## Status

**Unanswered — awaiting the requester.** Recorded rather than assumed, per the
plan's Constitution Check note.
