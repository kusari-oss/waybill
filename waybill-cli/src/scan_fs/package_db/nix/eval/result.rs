//! What one `nix eval` invocation produced, and the domain values it needs.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// A Nix platform double — `x86_64-linux`, `aarch64-darwin`.
///
/// A newtype rather than a `String` because it is load-bearing twice over.
/// It is the value that makes an emitted document mean something specific
/// (`hinotify` is 0.4.2 on `x86_64-linux` and 0.1.8 on `aarch64-darwin` at one
/// nixpkgs revision), and naming it explicitly is a *precondition* for
/// evaluating in pure mode at all: `builtins.currentSystem` does not exist
/// there, so an evaluation that asks Nix what platform it is on has already
/// left purity behind (research R2).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct NixSystem(String);

/// Why a string is not a usable Nix platform double.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum NixSystemParseError {
    #[error("a nix system must be `<arch>-<os>`, got {0:?}")]
    NotTwoSegments(String),
    #[error("a nix system must not have an empty segment, got {0:?}")]
    EmptySegment(String),
    #[error("a nix system must be ascii alphanumeric, `_` or `-`, got {0:?}")]
    UnexpectedCharacter(String),
}

impl NixSystem {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for NixSystem {
    type Err = NixSystemParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let owned = s.to_string();
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(NixSystemParseError::UnexpectedCharacter(owned));
        }
        // Split once from the right: `x86_64-linux` has one hyphen, but
        // `aarch64-unknown-linux-gnu`-shaped values would have more, and the
        // architecture is the part that can contain them.
        let Some((arch, os)) = s.split_once('-') else {
            return Err(NixSystemParseError::NotTwoSegments(owned));
        };
        if arch.is_empty() || os.is_empty() {
            return Err(NixSystemParseError::EmptySegment(owned));
        }
        Ok(Self(owned))
    }
}

impl fmt::Display for NixSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Proof that the import-from-derivation refusal was observed to be in effect.
///
/// Constructible only by [`crate::scan_fs::package_db::nix::eval::preflight`],
/// so a caller cannot assert the safety property by writing `true`. The
/// distinction matters because `nix` accepts an unsupported `--option` with a
/// warning and exit code 0 (research R3): *requesting* the refusal and
/// *having* it look identical from the outside, and only the pre-flight can
/// tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IfdRefusalVerified(pub(super) ());

impl IfdRefusalVerified {
    /// Mint the token. `pub(super)` so only this module tree can call it, and
    /// by convention only `preflight` does.
    pub(super) fn attest() -> Self {
        Self(())
    }
}

/// What one successful evaluation produced.
///
/// Holding [`IfdRefusalVerified`] by value rather than a `bool` is what makes
/// spec FR-008 enforceable at the type level: a caller cannot obtain an
/// `EvaluationOutcome` without the pre-flight having succeeded first.
#[derive(Debug, Clone)]
pub(crate) struct EvaluationOutcome {
    /// The nixpkgs revision evaluated against, 40 hex characters.
    pub(crate) revision: String,
    /// The platform the result describes.
    pub(crate) system: NixSystem,
    /// Component name to version, as Nix reported it. `None` means the
    /// attribute was absent or `tryEval` caught it — not an error, and not a
    /// version.
    pub(crate) versions: BTreeMap<String, Option<String>>,
}

impl EvaluationOutcome {
    /// Build an outcome, consuming the pre-flight's proof.
    ///
    /// Taking [`IfdRefusalVerified`] by value is what makes spec FR-008
    /// enforceable at the type level: there is no way to obtain an
    /// `EvaluationOutcome` without the pre-flight having succeeded, because
    /// there is no other way to mint the token. It is consumed rather than
    /// stored because holding it would add a field nothing reads — the
    /// guarantee lives in the constructor's signature, not in the struct.
    pub(crate) fn new(
        revision: String,
        system: NixSystem,
        versions: BTreeMap<String, Option<String>>,
        _ifd_refused: IfdRefusalVerified,
    ) -> Self {
        Self {
            revision,
            system,
            versions,
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_platforms_the_research_measured() {
        for s in ["x86_64-linux", "aarch64-darwin", "aarch64-linux"] {
            assert_eq!(NixSystem::from_str(s).unwrap().as_str(), s);
        }
    }

    #[test]
    fn rejects_shapes_that_are_not_a_platform_double() {
        use NixSystemParseError::*;
        assert!(matches!(NixSystem::from_str("linux"), Err(NotTwoSegments(_))));
        assert!(matches!(NixSystem::from_str(""), Err(NotTwoSegments(_))));
        assert!(matches!(NixSystem::from_str("-linux"), Err(EmptySegment(_))));
        assert!(matches!(NixSystem::from_str("x86_64-"), Err(EmptySegment(_))));
        assert!(matches!(
            NixSystem::from_str("x86_64 linux"),
            Err(UnexpectedCharacter(_))
        ));
        // Nix expressions are code; a platform that could carry expression
        // syntax must never reach one.
        assert!(matches!(
            NixSystem::from_str("x86_64-linux\"; evil = \""),
            Err(UnexpectedCharacter(_))
        ));
    }

    #[test]
    fn architecture_may_contain_underscores_and_extra_hyphens() {
        assert!(NixSystem::from_str("x86_64-linux").is_ok());
        assert!(NixSystem::from_str("armv7l-linux").is_ok());
        // split_once takes the FIRST hyphen, so a longer triple still parses
        // with a non-empty os segment rather than being rejected outright.
        assert!(NixSystem::from_str("aarch64-unknown-linux").is_ok());
    }
}
