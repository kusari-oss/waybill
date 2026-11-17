# Data model: CMake fetches in nested projects

In process only. Nothing persists beyond the emitted document.

## Command (`cmake/commands.rs`)

One CMake command invocation.

| Field | Type | Meaning |
|---|---|---|
| `name` | `String` | Command name, lowercased. CMake command names are case-insensitive. |
| `args` | `String` | Raw argument text between the outer parentheses. |
| `line` | `u32` | 1-based line where the command starts. |

**Tokenising rules:**
- A `#` line comment and a `#[[ … ]]` / `#[=[ … ]=]` bracket comment are skipped.
- Inside arguments, quoted strings (with `\` escapes), bracket arguments and nested parentheses are respected.

## Block stack (`cmake/blocks.rs`)

```rust
enum Block {
    If { condition: String },  // replaced by elseif(); prefixed "else: " by else()
    Function { name: String }, // function() and macro()
    Loop,                      // foreach() and while()
}
```

An unbalanced `end*` is ignored, and a file that ends with open blocks is not an error. Both are counted, never fatal.

## FetchDeclaration

| Field | Type | Meaning |
|---|---|---|
| `kind` | `FetchContent \| ExternalProject` | |
| `name` | `String` | First argument. Unresolvable if it contains `${…}` with no same-file literal `set()`. |
| `file` | `PathBuf` | Relative to the scan root. |
| `line` | `u32` | |
| `conditions` | `Vec<String>` | Enclosing `if` conditions outermost first, plus `function: <name>`. Becomes C232. |
| `shape` | `Option<FallbackShape>` | The first applicable shape (R2 order). `None` means unconditional. |
| `test` | `Option<TestDerivation>` | `SourceTestDirectory` or `CmakeBuildTesting` (R7). |
| `identity` | `Option<String>` | The PURL the existing reader builds from URL / `GIT_REPOSITORY` / `GIT_TAG`. `None` means unresolved. |

## FallbackShape (`cmake/fallback.rs`)

A closed enum. `as_str()` gives the C122 value.

| variant | C122 value | applies to |
|---|---|---|
| `FindPackageArgs` | `cmake-find-package-args` | FetchContent, unless `NEVER` is in force in the declaring directory or an enclosing one |
| `NotTarget` | `cmake-not-target` | both kinds |
| `FindPackageFallback` | `cmake-find-package-fallback` | both kinds (`NOT <X>_FOUND`) |
| `TryFindPackageAlways` | `cmake-try-find-package-always` | FetchContent |
| `DependencyProvider` | `cmake-dependency-provider` | FetchContent, only if the tree defines a provider listing `FETCHCONTENT_MAKEAVAILABLE_SERIAL` |
| `FunctionBody` | `cmake-function-body` | both kinds |
| `Conditional` | `cmake-conditional` | both kinds, for any other enclosing `if()` |

**Scope of the mode setting.** `TRY_FIND_PACKAGE_MODE` takes the value of the nearest `set(FETCHCONTENT_TRY_FIND_PACKAGE_MODE …)` in the declaring directory or an ancestor directory. When one directory sets it several times, the last set in file order wins. That is static and approximate, and C122's value makes it visible.

## Merge decisions (`scan_fs/cmake_fetch_attach.rs`)

**First declaration wins (R5).** This applies to FetchContent only. When a declaration's lowercased name is also declared by a file in an **ancestor directory**, that declaration is the winner.
- The loser is not emitted.
- The winner's C18 gains the loser's file, and its C233 gains `{file, version}`.
- C231 counts it under `merged.first-declaration-wins`.

**Existing component (R9).** This applies to conditional fetches only. When the fetch's identity, without version (`pkg:github/<owner>/<repo>`, or the URL with its version segment dropped), equals a component another reader emitted:
- the fetch is merged there: C56 gets the CMake mechanism if it differs, and C233 gets `{file, version}`;
- C231 counts it under `merged.existing-component`.

**Same PURL.** The existing PURL dedup handles this. C18 is the union of files. The merged component is Optional only if **every** declaration is conditional, and C232 is the sorted union of conditions (R4).

## Ownership

`OwnerIndex::owner_of(file)` from spec 1537 decides who declares a fetch, made reusable:
- the edge is `owner DependsOn fetch`;
- with no owner, the fetch is a declared root anchor.

Scope typing comes from `apply_lifecycle_scope_to_edges`.

## Wire rows

**C231 `waybill:cmake-fetch-coverage`** is document-scope, present iff `read > 0`.

```json
{"conditional":N,"emitted":N,"merged":{"existing-component":N,"first-declaration-wins":N},
 "read":N,"test":N,"unparseable-files":N,"unresolved":{"identity":N}}
```

**Invariants:**
- **V1:** `read == emitted + merged.existing-component + merged.first-declaration-wins + unresolved.identity`.
- **V2:** `conditional <= emitted` and `test <= emitted`. Both are counts of emitted components.

**C232 `waybill:cmake-fetch-conditions`** is per-component: a JSON array of strings, sorted, present iff the component is conditional or test-scoped through a condition.

**C233 `waybill:superseded-declarations`** is per-component: a JSON array of `{"file": …, "version": …}`, sorted by file then version, present iff any declaration was merged into the component.

**C122 `waybill:optional-derivation`** gets new values `cmake-*` (FallbackShape).

**C62 `waybill:lifecycle-scope-derivation`** gets new values `source-test-directory` and `cmake-build-testing`.
