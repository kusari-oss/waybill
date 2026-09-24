//! Milestone 926 (#947) — resolve Haskell dependency versions through the
//! nixpkgs revision pinned in `flake.lock`.
//!
//! A Nix-built Haskell project declares ranges in `.cabal` and ships no
//! `cabal.project.freeze`, so every dependency lands at design tier with no
//! version. Those versions are not unknown: the pinned nixpkgs revision
//! determines them, and m925 already reads the lockfile without consulting
//! what it points at.
//!
//! Boundaries this module holds to:
//!
//! - It **enriches** components the Haskell reader already emitted. It never
//!   introduces one (Constitution Principle XII constraint 1, FR-001a). The
//!   transitive closure is deliberately out of scope — see #962.
//! - It never synthesises a version, hash or versioned identifier for a
//!   dependency it could not resolve (Principle IX, FR-005).
//! - A package supplied by the compiler has no version in the package set at
//!   all, and is reported as such rather than resolved from the default set
//!   (FR-014c).

pub(crate) mod boot_libraries;
pub(crate) mod cache;
pub(crate) mod fetch;
pub(crate) mod nix_base32;
pub(crate) mod package_set;
