# Data model: CMake fetches made through user-defined wrappers

Everything is in process for one scan. Nothing persists beyond the emitted document. Files are under `waybill-cli/src/scan_fs/package_db/cmake/`.

## WrapperDef (`wrappers.rs`)

One `function()` or `macro()` definition whose body calls a fetch built-in or another wrapper and forwards the caller's arguments (R1).

| Field | Type | Meaning |
|---|---|---|
| `name` | `String` | As written. Lookup is by the lowercased name, because CMake command names are case-insensitive. |
| `kind` | `Function \| Macro` | Probe W7: both expose the same argument variables, so binding does not depend on it. |
| `file` | `PathBuf` | Relative to the scan root. |
| `ordinal` | `usize` | The definition's index among the `function()`/`macro()` definitions in its file. With `file`, it identifies the definition, because tokenised commands carry no line numbers (spec 1538 R15). |
| `params` | `Vec<String>` | Named positional parameters. |
| `parses` | `Vec<ParseArgs>` | The body's `cmake_parse_arguments` calls, in body order. |
| `defaults` | `Vec<Default>` | Literal values the body sets for a parsed variable when the call did not set it (R4). |
| `locals` | `BTreeMap<String, Option<String>>` | Other body-local `set(<var> <literal>)`s. A variable set to different values in the body maps to `None`, as `collect_set_vars` does today. |
| `call` | `BodyCall` | The single fetch or wrapper call in the body. A body with more than one is not a wrapper (R1). |
| `conditions` | `Vec<String>` | `if()` conditions enclosing `call` inside the body, outermost first. They are appended to the call site's (FR-006). |
| `shapes` | `BTreeSet<ForwardingShape>` | How the caller's arguments reach `call` (R1). Never empty for a `WrapperDef`. |

```rust
enum BodyTarget { Builtin(FetchKind), Wrapper(String /* lowercased */) }
struct BodyCall { target: BodyTarget, args: Vec<Arg> }   // Arg: text + quoted, from the tokeniser
struct ParseArgs { prefix: String, parse_argv: Option<usize>, options: Vec<String>, one: Vec<String>, multi: Vec<String> }
struct Default { var: String, value: String }

enum ForwardingShape {   // closed; the census (SC-001) fixed this list
    Argn,                // ${ARGN}
    Argv,                // ${ARGV}
    Unparsed,            // ${<P>_UNPARSED_ARGUMENTS}
    ParsedVariable,      // any ${<P>_<KW>}: URL ${<P>_URL}, URL ${<P>_SOURCE_URL}
    NameParameter,       // the call's first argument is a parameter or ${ARGV<n>}
}
```

**Validation (R2).** A definition is a `WrapperDef` only when its body call, read on its own as spec 1538 reads it, has **no identity**: `fetch_entry` gives no PURL, or a PURL containing `${…}`. Otherwise the call is a direct fetch in a function body, spec 1538's `cmake-function-body`, unchanged, and calls to the function are not read. That covers a body with only literal arguments (spec edge case) and spec 1538's C-13b, a parameterised name with a literal GitHub `GIT_REPOSITORY`.

A body whose call targets **another wrapper** (`BodyTarget::Wrapper`) is a wrapper whenever it forwards by a recognised shape. The own-identity check applies only to a body that calls a built-in, because a wrapper-to-wrapper call has no `fetch_entry` reading of its own. On TheRock this covers LLVM's four `builtin_*` and `runtime_*` functions.

## Definitions

`BTreeMap<String /* lowercased name */, Vec<WrapperDef>>`, built in the pre-pass over every walked CMake file, before any call is read. Every definition of a name is kept: FR-002 binds a call against all of them.

A name defined both as a wrapper and as a function that is not a wrapper keeps both definitions. The non-wrapper one binds to "no fetch", so the call is ambiguous, never silently bound through the wrapper (spec edge case "Defined twice").

## WrapperCall

One call site of a name in `Definitions`.

| Field | Type | Meaning |
|---|---|---|
| `file` | `PathBuf` | The call site. It becomes C18, and ownership (FR-006). |
| `args` | `Vec<Arg>` | The call's arguments, expanded with the call-site file's literal `set()`s, as direct calls are today. |
| `blocks` | `BlockStack` snapshot | `if_conditions()`, `function()`, loop membership at the call. |

A call inside the body of a `WrapperDef` is part of that definition (`BodyTarget::Wrapper`), not a call site. A call inside a function that is **not** a wrapper is a call site, and gets `cmake-function-body` from that enclosing function, as a direct fetch would.

## Binding (`bind`)

`bind(call: &[Arg], def: &WrapperDef, defs: &Definitions, depth: u8) -> Binding`

```rust
enum Binding {
    Bound { kind: FetchKind, args: Vec<Arg>, conditions: Vec<String>, chain: Vec<String> },
    Unresolved(UnboundReason),
}
enum UnboundReason { Recursion, DepthExceeded, TooFewArguments, AmbiguousDefinitions }
```

Steps, each measured with CMake 4.4.5 (`measurements/cmake-semantics/`):
1. `ARGV` is the call's arguments. `ARGV<n>` is the n-th. Named parameters take `ARGV0…` in order, and `ARGN` is the rest (W7). Too few arguments for the named parameters means `TooFewArguments`.
2. Each `cmake_parse_arguments` in body order (W1, W1b, W4):
   - its input is `ARGV[n..]` with `PARSE_ARGV <n>`, or the expansion of its trailing arguments otherwise;
   - an option is `TRUE` if present, otherwise `FALSE`;
   - a one-value keyword takes the word after it, and **the last occurrence wins** (W4);
   - a multi-value keyword takes every word up to the next keyword **of this call**, so a `URL` that the wrapper does not parse is swallowed by a preceding multi-value keyword (W1);
   - everything else, in order, is `<P>_UNPARSED_ARGUMENTS`.
3. Defaults: each `Default` whose variable is still unset gets its literal value (W8).
4. The body call's arguments are substituted: parameters, `ARGN`, `ARGV`, `ARGV<n>`, `<P>_*`, then `locals`. A substituted list expands to one argument per element, quoted or not (W6). An unknown variable stays as `${…}`, and decides nothing unless the identity depends on it.
5. `BodyTarget::Wrapper(w)`: recurse on `w` with the substituted arguments, `depth + 1`. A name already on the chain is `Recursion`. Depth over 8 is `DepthExceeded`.
6. `BodyTarget::Builtin(kind)`: `Bound`. The arguments go to the existing `fetch_entry` unchanged, so a repeated `URL` is read as CMake reads it: the first occurrence is the first mirror tried, and is what is downloaded (W2, W2b, W3).

**Several definitions (FR-002).** `bind` runs once per definition of the called name. If every result is `Bound` to the same PURL and the same declared hash, the first one in walk order is used, and its chain is the row's. Otherwise the call is `AmbiguousDefinitions`. Probe W5/W5b shows why: the definition that runs is the last one *evaluated* before the call, across `include()` and `add_subdirectory()`, which static reading cannot know.

## BoundDeclaration

A `Bound` binding becomes the same `Declaration` a direct call produces (spec 1538 data model), with:
- `name`: the bound first argument, lowercased, for first-declaration-wins (FR-006);
- `file`: the call site;
- `conditions`: the call site's, then the body's (FR-006);
- `chain`: outermost wrapper first, for C234.

Then FR-005 applies to the entry:
- a `URL` fetch whose archive filename, without its archive extension, is `<name>[-_]v?<version>`, where `<version>` is the version `parse_version_from_url` gave and `<name>` is non-empty, gets PURL `pkg:generic/<name>@<version>` and that name;
- any other wrapped fetch keeps the name a direct call would give it.

## FetchCoverage (C231) additions

| Key | Meaning | In V1? |
|---|---|---|
| `wrappers.definitions` | wrapper definitions read. Each one's body call is a template, not a declaration. | no |
| `wrappers.calls` | wrapper call sites read. Each is one declaration in `read`. | (counted in `read`) |
| `wrappers.ambiguous-calls` | calls left unresolved because their definitions disagree. Each is also in `unresolved.identity`. | no |

V1 is unchanged: `read = emitted + merged.first-declaration-wins + merged.existing-component + unresolved.identity`. A wrapper call that stays unresolved for any reason counts in `unresolved.identity`.

## Wire row C234 `waybill:cmake-fetch-wrapper`

A per-component sorted JSON array of chains. Each chain is a JSON array of wrapper names as defined, outermost first: `[["therock_subproject_fetch"]]`, `[["wf_outer","wf_argn"]]`. A component that stands for several wrapped declarations (R4 of spec 1538: one component per PURL) carries the union. A component that only direct declarations stand for carries no row.

A declaration merged into another component (first-declaration-wins, R9) is recorded in C233 as today, and its chain is not carried: C234 describes what the component's own declarations were.
