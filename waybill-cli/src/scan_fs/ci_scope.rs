//! Spec 1613 (#1610): the scope of what is declared only in CI and docs
//! dot-directories.
//!
//! - **R5:** a runtime component (no lifecycle scope, or `Runtime`) whose every evidence source
//!   file lies under a listed CI or docs dot-directory (`project_roots::
//!   LISTED_DOT_DIRECTORIES`) is `Development`, with C62 `ci-directory`, or
//!   `docs-directory` when every file lies under `.sphinx`. A pip
//!   requirements file there is labelled earlier, through C155 and
//!   `Optional`; this pass leaves a scoped component alone.
//! - **R8:** a component whose final scope is runtime carries no C155. A
//!   package declared in a CI or dev requirements file and in a runtime one
//!   keeps the runtime scope (the deduplicator's merge) and used to keep the
//!   scoped file's label too.

use std::path::{Path, PathBuf};

use waybill_common::resolution::{LifecycleScope, ResolvedComponent};

use crate::resolve::scope_propagation::ANN_SCOPE_DERIVATION;
use crate::scan_fs::package_db::project_roots::{listed_ancestor_class, DotDirClass};

/// C155, emitted by the pip requirements reader.
const ANN_REQ_FILE_SCOPE: &str = "waybill:python-req-file-scope";
pub(crate) const DERIVATION_CI_DIRECTORY: &str = "ci-directory";
pub(crate) const DERIVATION_DOCS_DIRECTORY: &str = "docs-directory";

/// What the pass changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct CiScopeSummary {
    pub(crate) ci: usize,
    pub(crate) docs: usize,
    pub(crate) labels_removed: usize,
}

/// The listed class of `path`'s nearest listed ancestor, the path taken
/// relative to the first of `roots` it lies under. A path under no root is
/// outside the scan and has no class.
fn class_of(path: &str, roots: &[PathBuf]) -> Option<DotDirClass> {
    let p = Path::new(path);
    let rel = if p.is_relative() { p } else { roots.iter().find_map(|r| p.strip_prefix(r).ok())? };
    listed_ancestor_class(rel, Path::new(""))
}

pub(crate) fn apply(components: &mut [ResolvedComponent], roots: &[PathBuf]) -> CiScopeSummary {
    let mut summary = CiScopeSummary::default();
    for c in components.iter_mut() {
        let runtime = matches!(c.lifecycle_scope, None | Some(LifecycleScope::Runtime));
        if runtime && !c.evidence.source_file_paths.is_empty() {
            let classes: Option<Vec<DotDirClass>> =
                c.evidence.source_file_paths.iter().map(|p| class_of(p, roots)).collect();
            if let Some(classes) = classes {
                let docs = classes.iter().all(|k| *k == DotDirClass::Docs);
                c.lifecycle_scope = Some(LifecycleScope::Development);
                let value = if docs { DERIVATION_DOCS_DIRECTORY } else { DERIVATION_CI_DIRECTORY };
                c.extra_annotations.insert(ANN_SCOPE_DERIVATION.to_string(), serde_json::json!(value));
                if docs {
                    summary.docs += 1;
                } else {
                    summary.ci += 1;
                }
            }
        }
        if matches!(c.lifecycle_scope, None | Some(LifecycleScope::Runtime))
            && c.extra_annotations.remove(ANN_REQ_FILE_SCOPE).is_some()
        {
            summary.labels_removed += 1;
        }
    }
    if summary != CiScopeSummary::default() {
        tracing::info!(
            ci = summary.ci,
            docs = summary.docs,
            labels_removed = summary.labels_removed,
            "ci scope: components declared only in CI or docs dot-directories (spec 1613)"
        );
    }
    summary
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use crate::generate::openvex::tests::mk_component;

    const ROOT: &str = "/scan-root";

    fn roots() -> Vec<PathBuf> {
        vec![PathBuf::from(ROOT)]
    }

    fn at(purl: &str, rels: &[&str]) -> ResolvedComponent {
        let mut c = mk_component(purl);
        c.evidence.source_file_paths = rels.iter().map(|r| format!("{ROOT}/{r}")).collect();
        c
    }

    fn c62(c: &ResolvedComponent) -> Option<&str> {
        c.extra_annotations.get(ANN_SCOPE_DERIVATION).and_then(|v| v.as_str())
    }

    #[test]
    fn only_under_listed_directories_is_development_with_its_class() {
        let mut cs = vec![
            at("pkg:npm/waybill-fixture-a@1.0.0", &[".github/actions/x/package.json"]),
            at("pkg:npm/waybill-fixture-b@1.0.0", &["docs/.sphinx/package.json"]),
            at("pkg:npm/waybill-fixture-c@1.0.0", &[".ci/a/package.json", "x/.sphinx/package.json"]),
        ];
        let s = apply(&mut cs, &roots());
        assert_eq!(s, CiScopeSummary { ci: 2, docs: 1, labels_removed: 0 });
        assert_eq!(cs[0].lifecycle_scope, Some(LifecycleScope::Development));
        assert_eq!(c62(&cs[0]), Some(DERIVATION_CI_DIRECTORY));
        assert_eq!(c62(&cs[1]), Some(DERIVATION_DOCS_DIRECTORY));
        assert_eq!(c62(&cs[2]), Some(DERIVATION_CI_DIRECTORY), "a mixed CI and docs component is CI");
    }

    #[test]
    fn an_explicit_runtime_scope_under_listed_directories_is_development() {
        // A manifest-declared dependency of a project under `.ci/` carries
        // an explicit `Runtime`, not `None` (seen on the quality corpus).
        let mut c = at("pkg:pypi/waybill-fixture-r@unresolved", &[".ci/tool"]);
        c.lifecycle_scope = Some(LifecycleScope::Runtime);
        let mut cs = vec![c];
        assert_eq!(apply(&mut cs, &roots()).ci, 1);
        assert_eq!(cs[0].lifecycle_scope, Some(LifecycleScope::Development));
        assert_eq!(c62(&cs[0]), Some(DERIVATION_CI_DIRECTORY));
    }

    #[test]
    fn mixed_locations_scoped_components_and_no_paths_are_left_alone() {
        let mixed = at("pkg:npm/waybill-fixture-m@1.0.0", &[".github/package.json", "package.json"]);
        let mut scoped = at("pkg:npm/waybill-fixture-s@1.0.0", &[".github/package.json"]);
        scoped.lifecycle_scope = Some(LifecycleScope::Optional);
        let none = mk_component("pkg:npm/waybill-fixture-n@1.0.0");
        let mut outside = mk_component("pkg:npm/waybill-fixture-o@1.0.0");
        outside.evidence.source_file_paths = vec!["/elsewhere/.ci/package.json".into()];
        let mut cs = vec![mixed, scoped, none, outside];
        assert_eq!(apply(&mut cs, &roots()), CiScopeSummary::default());
        assert_eq!(cs[0].lifecycle_scope, None);
        assert_eq!(cs[1].lifecycle_scope, Some(LifecycleScope::Optional));
        assert_eq!(c62(&cs[1]), None);
        assert_eq!(cs[2].lifecycle_scope, None);
        assert_eq!(cs[3].lifecycle_scope, None, "a path outside the scan roots is not under a listed directory");
    }

    #[test]
    fn a_runtime_component_loses_its_requirements_label_and_a_scoped_one_keeps_it() {
        let label = |c: &mut ResolvedComponent| {
            c.extra_annotations.insert(ANN_REQ_FILE_SCOPE.into(), serde_json::json!("ci"));
        };
        let mut runtime = at("pkg:pypi/waybill-fixture-both@1.0", &[".ci/requirements.txt", "requirements.txt"]);
        label(&mut runtime);
        let mut explicit = runtime.clone();
        explicit.lifecycle_scope = Some(LifecycleScope::Runtime);
        let mut optional = at("pkg:pypi/waybill-fixture-cionly@1.0", &[".ci/requirements.txt"]);
        optional.lifecycle_scope = Some(LifecycleScope::Optional);
        label(&mut optional);
        let mut cs = vec![runtime, explicit, optional];
        assert_eq!(apply(&mut cs, &roots()).labels_removed, 2);
        assert!(!cs[0].extra_annotations.contains_key(ANN_REQ_FILE_SCOPE));
        assert!(!cs[1].extra_annotations.contains_key(ANN_REQ_FILE_SCOPE));
        assert_eq!(cs[2].extra_annotations.get(ANN_REQ_FILE_SCOPE), Some(&serde_json::json!("ci")));
    }
}
