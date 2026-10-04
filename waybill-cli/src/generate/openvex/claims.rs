//! Milestone 1068 (#1039) — which VEX statements a document can carry.
//!
//! A statement can name several components, and a document (a split, a
//! root-overridden or tier-filtered projection) may contain only some of
//! them. So presence is decided per **claim**: one statement applied to one
//! component. CycloneDX carries the claims whose component it contains; every
//! format counts the rest as C193 `waybill:vex-claims-omitted`.

use std::collections::HashSet;

use waybill_common::resolution::ResolvedComponent;

use super::statements::OpenVexStatement;

/// One statement applied to one component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Claim<'a> {
    /// Index into the statement slice the claim came from.
    pub statement: usize,
    pub component_purl: &'a str,
    /// A build claim: the statement's product is the build, and this
    /// component is a subcomponent of it. Otherwise the component is the
    /// product itself (a version claim).
    pub build: bool,
}

/// Expand statements into claims, in statement order.
///
/// A version statement makes one claim per product; a build statement (its
/// product carries subcomponents) one per subcomponent. The build itself is
/// always in the document — it is the document's root, and the main-module
/// PURL that names it is aliased to the root even when a root override drops
/// that component — so it never decides presence.
pub(crate) fn claims(statements: &[OpenVexStatement]) -> Vec<Claim<'_>> {
    let mut out = Vec::new();
    for (i, st) in statements.iter().enumerate() {
        for p in &st.products {
            if p.subcomponents.is_empty() {
                out.push(Claim { statement: i, component_purl: &p.id, build: false });
            } else {
                for sc in &p.subcomponents {
                    out.push(Claim { statement: i, component_purl: &sc.id, build: true });
                }
            }
        }
    }
    out
}

/// C193 — claims whose component is not among `components`.
pub(crate) fn omitted_claims(
    statements: &[OpenVexStatement],
    components: &[ResolvedComponent],
) -> usize {
    let present: HashSet<&str> = components.iter().map(|c| c.purl.as_str()).collect();
    claims(statements)
        .iter()
        .filter(|c| !present.contains(c.component_purl))
        .count()
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::super::statements::{
        OpenVexJustification, OpenVexProduct, OpenVexStatus, OpenVexVulnerability,
    };
    use super::*;

    fn product(purl: &str, subs: &[&str]) -> OpenVexProduct {
        OpenVexProduct {
            id: purl.to_string(),
            identifiers: Default::default(),
            subcomponents: subs.iter().map(|s| product(s, &[])).collect(),
        }
    }
    fn statement(status: OpenVexStatus, products: Vec<OpenVexProduct>) -> OpenVexStatement {
        OpenVexStatement {
            vulnerability: OpenVexVulnerability {
                name: "CVE-2019-13232".to_string(),
                description: None,
                aliases: Vec::new(),
            },
            products,
            status,
            justification: (status == OpenVexStatus::NotAffected)
                .then_some(OpenVexJustification::VulnerableCodeNotPresent),
            impact_statement: None,
            action_statement: None,
        }
    }
    fn component(purl: &str) -> ResolvedComponent {
        crate::enrich::depsdev_source::tests::make_component(purl)
    }

    /// A backport: `affected` on the version, `not_affected` on the build.
    fn backport() -> Vec<OpenVexStatement> {
        vec![
            statement(OpenVexStatus::Affected, vec![product("pkg:generic/unzip@6.0", &[])]),
            statement(
                OpenVexStatus::NotAffected,
                vec![product("pkg:hackage/moat@0.1", &["pkg:generic/unzip@6.0"])],
            ),
        ]
    }

    #[test]
    fn a_backport_is_two_statements_and_two_claims() {
        let st = backport();
        let c = claims(&st);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].statement, c[0].build), (0, false));
        assert_eq!((c[1].statement, c[1].build), (1, true));
        assert!(c.iter().all(|c| c.component_purl == "pkg:generic/unzip@6.0"));
    }

    #[test]
    fn a_partly_present_statement_omits_only_its_absent_claims() {
        let st = vec![statement(
            OpenVexStatus::Affected,
            vec![
                product("pkg:generic/a@1", &[]),
                product("pkg:generic/b@1", &[]),
                product("pkg:generic/c@1", &[]),
            ],
        )];
        let comps = [component("pkg:generic/a@1"), component("pkg:generic/c@1")];
        assert_eq!(omitted_claims(&st, &comps), 1);
    }

    #[test]
    fn a_build_claim_whose_subcomponent_is_absent_is_omitted() {
        let st = backport();
        assert_eq!(omitted_claims(&st, &[component("pkg:hackage/moat@0.1")]), 2);
    }

    /// Under a root override the main module is dropped from the document,
    /// but the build is still the document's root: only the subcomponent
    /// decides presence.
    #[test]
    fn a_build_claim_survives_the_main_module_being_dropped() {
        let st = backport();
        assert_eq!(omitted_claims(&st, &[component("pkg:generic/unzip@6.0")]), 0);
    }
}
