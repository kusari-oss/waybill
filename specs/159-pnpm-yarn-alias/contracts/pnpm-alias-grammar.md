# Contract: Pnpm alias-VALUE grammar

**Milestone 159** • The pnpm-lock.yaml alias-value syntax that `detect_pnpm_alias` MUST parse.

## Where it appears

Inside snapshot entries' `dependencies:`, `peerDependencies:`, or `optionalDependencies:` sub-mappings. Example location:

```yaml
snapshots:
  '@docusaurus/core@3.10.1(peers)':
    dependencies:
      react-helmet-async: '@slorber/react-helmet-async@1.3.0(react-dom@18.3.1(react@18.2.0))(react@18.2.0)'
      # ^^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      # local-name        raw-value (quoted-string form)
```

## Grammar (BNF)

```text
alias_value    ::= quoted_form | unquoted_form
quoted_form    ::= "'" aliased_name "@" version peer_suffix? "'"
unquoted_form  ::= aliased_name "@" version
aliased_name   ::= "@" scope "/" name | name
scope          ::= /[a-z0-9_-]+/
name           ::= /[a-z0-9._-]+/
version        ::= <semver string, per npm registry conventions>
peer_suffix    ::= "(" peer_expr ")"
peer_expr      ::= peer_binding ("(" peer_expr ")")*
peer_binding   ::= peer_name "@" peer_version
peer_name      ::= "@" scope "/" name | name
peer_version   ::= <semver string>
```

## Detection rule

1. Parse the raw-value using the existing `parse_pnpm_key` helper at `pnpm_lock.rs:129` (which handles both quoted and unquoted, and strips the peer suffix).
2. The result is `(canonical_name, canonical_version)`.
3. Compare `canonical_name` to `local_name` (the dep-key in the enclosing sub-mapping).
4. If `canonical_name != local_name` → ALIAS DETECTED. Emit `AliasResolution { local_name, aliased_name: canonical_name, aliased_version: canonical_version, pnpm_peer_suffix: extracted_suffix_or_none, ecosystem: Pnpm }`.
5. If `canonical_name == local_name` → NOT AN ALIAS (or self-referential value form like `foo: foo@1.0.0`). No `AliasResolution` emitted.

## Malformed-value handling (FR-010)

If `parse_pnpm_key` returns `None` (unparseable value string), OR if the value string doesn't fit the grammar above (e.g. YAML mapping instead of string, missing `@`), the detection function MUST return `None` AND emit a warn-level tracing log per FR-010:

```rust
tracing::warn!(
    lockfile = %source_path,
    local_name = %local_name,
    raw_value = %raw_value,
    "npm-alias parse failed (skipping entry)"
);
```

The enclosing `collect_pnpm_dep_names` / `build_snapshots_lookup` continues to the next entry (graceful degradation — no crash, no incorrect emission).

## Test coverage (SC-007 subset)

- **T1**: quoted-form alias, scoped aliased-name (`react-helmet-async: '@slorber/react-helmet-async@1.3.0(...)'`) — real test-podman-desktop shape.
- **T2**: unquoted-form alias (`string-width-cjs: string-width@4.2.3`).
- **T3**: quoted-form alias with scoped LOCAL name (`@some-scope/name: @other-scope/other@2.0.0`) — theoretical monorepo case.
- **T4**: no-alias case (`foo: foo@1.0.0`) — detection MUST return `None`.
- **T5**: malformed-value (`react-helmet-async: 'not-a-purl'`) — detection MUST return `None` AND emit the warn log.
