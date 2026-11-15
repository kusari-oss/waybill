//! Git submodules as components (spec 1530, issue #1515; milestone 105
//! User Story 6, finished).
//!
//! One component per *gitlink* — a tree entry of mode `160000` — pinned
//! to the commit the super-project records, whether or not the submodule
//! is initialised. `.gitmodules` supplies only the URL.
//!
//! The scanned repository is untrusted. Research R1 measured `git
//! ls-files` and `git status` running a program the repository names in
//! its own `core.fsmonitor`; `git ls-tree` does not. This reader runs
//! only commands measured not to load the index, and disables
//! `core.fsmonitor` and hooks on every invocation (`gitlinks.rs`).

pub(crate) mod gitlinks;
pub(crate) mod gitmodules;
pub(crate) mod identity;
