// Milestone 055 — Go transitive dependency edges (anchored on go.sum).
//
// This module was promoted from a single `golang.rs` file to a directory
// module in milestone 055 (T008). The original ~2018-line implementation
// from milestones 049/053/054 lives unchanged in `legacy.rs`; the new
// resolver code (4-step ladder per spec FR-002) is split across the
// sibling submodule files.
//
// Module map:
//   - `legacy`         — milestone 049/053/054 implementation: go.mod /
//                        go.sum parsers, GoModCache, build_entries_from_go_module,
//                        build_main_module_entry, read(). Unchanged in 055
//                        except for the call into the new resolver from `read()`.
//   - `module_id`      — `ModuleId` newtype, used everywhere the resolver
//                        traffics in (path, version) pairs.
//   - `graph_resolver` — 4-step ladder orchestrator + all resolver types
//                        (`ResolutionStep`, `ModuleGraphMap`, `LadderSummary`,
//                        `WorkspaceContext`, `GraphResolverConfig`,
//                        `GraphResolverError`, `StepResult`, `StepError`,
//                        `ErrorClass`).
//   - `go_mod_graph`   — step 1: `go mod graph` subprocess + output parser.
//   - `proxy_fetch`    — step 3: HTTP client + Go module-path escape.
//   - `goprivate`      — `$GOPROXY` and `$GOPRIVATE` env-var parsers.
//   - `mod_why`        — milestone 112: `go mod why -m -vendor`
//                        build-graph classification (runner + parser).

pub mod legacy;
pub mod module_id;
pub mod graph_resolver;
pub mod go_mod_graph;
pub mod mod_why;
pub mod proxy_fetch;
pub mod goprivate;
// Milestone 161 (T001): go.work parser + workspace-mode types +
// Q1 hybrid edge-disposition classifier.
pub mod gowork;

// Preserve the pre-T008 import surface — callers say
// `crate::scan_fs::package_db::golang::read(...)`,
// `crate::scan_fs::package_db::golang::build_main_module_entry(...)`,
// etc. The glob re-export keeps those paths working transparently.
pub use legacy::*;

// Public re-exports for the new resolver API. Marked `#[allow(unused_imports)]`
// because external consumers (callers in legacy.rs::read() once T025 wires
// it up, plus the integration test in tests/go_transitive_edges.rs) won't
// be live until the US1 phase tasks land. The re-exports define the
// public-API surface of the resolver per
// specs/055-go-transitive-edges/contracts/resolver-api.md.
#[allow(unused_imports)]
pub use graph_resolver::{
    GraphResolver, GraphResolverConfig, GraphResolverError, LadderSummary, ModuleGraphEntry,
    ModuleGraphMap, ResolutionStep, StepError, StepResult, WorkspaceContext,
};
#[allow(unused_imports)]
pub use module_id::ModuleId;
