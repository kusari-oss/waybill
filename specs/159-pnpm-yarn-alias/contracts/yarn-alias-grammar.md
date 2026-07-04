# Contract: Yarn v1 key-side alias grammar

**Milestone 159** • The yarn.lock v1 alias-key syntax that `detect_yarn_v1_alias` MUST parse.

## Where it appears

At the top-level entry key line in yarn.lock (v1). Comma-joined specs sharing a single entry:

```
"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":
  version "2.6.4"
  resolved "https://registry.yarnpkg.com/@cosmos.gl/graph/-/graph-2.6.4.tgz#..."
  integrity sha512-...
  dependencies:
    d3-array "^3.2.0"
    ...
```

The alias marker `npm:` appears INSIDE the version-spec portion of any spec on the key line.

## Grammar (BNF)

```text
yarn_v1_key    ::= spec ("," WS* spec)*
spec           ::= '"' spec_body '"' | spec_body
spec_body      ::= local_name "@" version_spec
local_name     ::= "@" scope "/" name | name
version_spec   ::= npm_alias | range_spec
npm_alias      ::= "npm:" aliased_name
aliased_name   ::= "@" scope "/" name | name
range_spec     ::= <semver range | git URL | file: path | link: path | ...>
scope          ::= /[a-z0-9_-]+/
name           ::= /[a-z0-9._-]+/
WS             ::= " " | "\t"
```

## Detection rule

1. Split the raw key line on `,` to get individual specs.
2. For each spec:
   a. `unquote()` strip surrounding double-quotes.
   b. Extract the local-name via the existing `descriptor_to_name` at `yarn_lock.rs:239` (which handles the LAST-`@`-after-scope-prefix convention).
   c. Everything AFTER the local-name's terminal `@` is the version-spec.
   d. If the version-spec starts with `npm:` → ALIAS DETECTED.
3. If ANY spec detected an alias, emit `AliasResolution { local_name, aliased_name: <after npm: prefix>, aliased_version: <from entry's `version:` line>, pnpm_peer_suffix: None, ecosystem: YarnV1 }`.

Per Q2 clarification (2026-07-04): emit ONCE per unique local-name across all specs in the key. If two specs share the same local-name (as in the example above where both specs use `@cosmograph/cosmos`), emit ONE `AliasResolution`. If two specs have different local-names (theoretical monorepo case), emit multiple.

## Aliased-version source

Unlike pnpm where the alias VALUE includes the version, yarn v1's key spec contains only the RANGE (`^1.1.1`) or the alias-marker (`npm:@cosmos.gl/graph`) — never a resolved version. The resolved version MUST be read from the entry BODY's `version: "..."` line:

```
"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":
  version "2.6.4"        # <-- aliased_version comes from here
```

## Malformed-key handling (FR-010)

If the key line fails to split into valid specs, OR if the `npm:` marker appears without a following aliased-name (e.g. `foo@npm:`), OR if the entry has no `version:` line, detection MUST return `None` AND emit the warn log per FR-010:

```rust
tracing::warn!(
    lockfile = %source_path,
    local_name = %local_name_or_raw_key,
    raw_value = %raw_key,
    "npm-alias parse failed (skipping entry)"
);
```

The enclosing `parse_v1` continues to the next entry.

## Test coverage (SC-007 subset)

- **T6**: yarn v1 key with `npm:` alias, both scoped (`"@cosmograph/cosmos@^1.1.1", "@cosmograph/cosmos@npm:@cosmos.gl/graph":`) — real test-guac-visualizer shape.
- **T7**: yarn v1 key with `npm:` alias, unscoped local-name, scoped aliased-name (`"string-width@^4.0.0", "string-width@npm:@shim/string-width":`).
- **T8**: yarn v1 key with `npm:` alias, scoped local-name, unscoped aliased-name (`"@scope/name@^1.0.0", "@scope/name@npm:real-name":`).
- **T9**: no-alias case (`"lodash@^4.17.21":`) — detection MUST return `None`.
- **T10**: malformed-key (`foo@npm:` with no aliased-name after `:`) — detection MUST return `None` AND emit the warn log.
