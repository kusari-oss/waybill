//! Parsing `nix derivation show -r` output.

use std::collections::BTreeMap;

use serde::Deserialize;

/// One derivation from the closure.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawDerivation {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) outputs: BTreeMap<String, RawOutput>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawOutput {
    #[serde(default)]
    pub(crate) path: Option<String>,
}

/// The top-level shape: `{"derivations": {...}, "version": N}`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawClosure {
    pub(crate) derivations: BTreeMap<String, RawDerivation>,
}

impl RawDerivation {
    /// The package name, preferring `pname` over the derivation's own name.
    pub(crate) fn pname(&self) -> Option<&str> {
        self.env
            .get("pname")
            .map(String::as_str)
            .or(self.name.as_deref())
    }

    pub(crate) fn version(&self) -> Option<&str> {
        self.env.get("version").map(String::as_str)
    }

    /// Store paths listed in a whitespace-separated `env` field.
    pub(crate) fn paths_in(&self, field: &str) -> impl Iterator<Item = &str> {
        self.env
            .get(field)
            .map(String::as_str)
            .unwrap_or_default()
            .split_whitespace()
    }
}

/// The basename of a store path, which is the only form in which two sides of
/// the closure can be compared.
///
/// **This is the join that fails silently.** Output paths under
/// `outputs.*.path` are stored WITHOUT the `/nix/store/` prefix, while the
/// `env` fields that reference them carry it. Comparing the raw strings
/// matches nothing — and matches nothing without erroring, producing a
/// classification in which every member is unreferenced. The first draft of
/// the analysis script did exactly that and reported 1,275 of 1,275
/// unreferenced, which looked like a finding rather than a bug.
pub(crate) fn store_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl RawClosure {
    pub(crate) fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Map every output basename to the `.drv` key that produces it.
    pub(crate) fn output_index(&self) -> BTreeMap<&str, &str> {
        let mut index = BTreeMap::new();
        for (key, drv) in &self.derivations {
            for out in drv.outputs.values() {
                if let Some(path) = out.path.as_deref() {
                    index.insert(store_basename(path), key.as_str());
                }
            }
        }
        index
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;

    /// Shaped like the real thing: outputs carry a bare basename, env fields
    /// carry a full store path to the same output.
    const CLOSURE: &str = r#"{
      "derivations": {
        "aaa-lib-1.0.drv": {
          "name": "lib-1.0",
          "env": { "pname": "lib", "version": "1.0" },
          "outputs": { "out": { "path": "hash1-lib-1.0" } }
        },
        "bbb-app-2.0.drv": {
          "name": "app-2.0",
          "env": { "pname": "app", "version": "2.0",
                   "buildInputs": "/nix/store/hash1-lib-1.0" },
          "outputs": { "out": { "path": "hash2-app-2.0" } }
        }
      },
      "version": 3
    }"#;

    #[test]
    fn basename_join_resolves_across_the_prefix_mismatch() {
        let c = RawClosure::parse(CLOSURE).unwrap();
        let index = c.output_index();

        // CONTROL: the fixture really does hold two derivations. A parse that
        // silently produced nothing would make the assertion below vacuous.
        assert_eq!(c.derivations.len(), 2);
        assert_eq!(index.len(), 2);

        let app = &c.derivations["bbb-app-2.0.drv"];
        let referenced: Vec<&str> = app
            .paths_in("buildInputs")
            .filter_map(|p| index.get(store_basename(p)).copied())
            .collect();

        assert_eq!(
            referenced,
            vec!["aaa-lib-1.0.drv"],
            "the env field carries /nix/store/hash1-lib-1.0 and the output \
             carries hash1-lib-1.0; comparing them unstripped resolves nothing"
        );
    }

    #[test]
    fn comparing_unstripped_paths_resolves_nothing() {
        // The bug this join exists to avoid, asserted directly so the reason
        // for `store_basename` cannot be optimised away by a later reader.
        let c = RawClosure::parse(CLOSURE).unwrap();
        let index = c.output_index();
        let app = &c.derivations["bbb-app-2.0.drv"];
        let naive: Vec<&str> = app
            .paths_in("buildInputs")
            .filter_map(|p| index.get(p).copied())
            .collect();
        assert!(
            naive.is_empty(),
            "if this ever resolves, the prefix mismatch is gone and \
             store_basename may be simplified"
        );
    }

    #[test]
    fn pname_is_preferred_over_the_derivation_name() {
        let c = RawClosure::parse(CLOSURE).unwrap();
        assert_eq!(c.derivations["aaa-lib-1.0.drv"].pname(), Some("lib"));
        assert_eq!(c.derivations["aaa-lib-1.0.drv"].version(), Some("1.0"));
    }

    #[test]
    fn a_derivation_without_the_field_yields_no_paths() {
        let c = RawClosure::parse(CLOSURE).unwrap();
        assert_eq!(c.derivations["aaa-lib-1.0.drv"].paths_in("buildInputs").count(), 0);
    }
}
