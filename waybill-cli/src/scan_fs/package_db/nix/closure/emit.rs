//! Turning a classified closure into components.
//!
//! Closure components **supplement** the manifest-derived set; they never
//! replace it (spec FR-003a). The two answer different questions: a manifest
//! covers every stanza a project declares, while a closure covers only what
//! the selected attribute builds. Measured — one project's executable-stanza
//! dependencies and the whole GHC boot set are absent from its library's
//! closure, so treating closure-absence as evidence of spuriousness would
//! discard the Haskell standard distribution.

use waybill_common::resolution::{ResolutionEvidence, ResolutionTechnique, ResolvedComponent};
use waybill_common::types::purl::Purl;

use super::classify::DerivationRole;
use super::derivation::RawDerivation;
use super::member::{self, MemberRole};
use super::ClassifiedClosure;

/// C182 — how nix referenced this member.
pub(crate) const ANN_CLOSURE_ROLE: &str = "waybill:closure-role";
/// Milestone 1372 (#1366) — C211. Present only where the build graph alone
/// put a derivation in its role, so presence is the signal and a component
/// admitted the old way gains no annotation it did not have.
pub(crate) const ANN_CLOSURE_ADMISSION: &str = "waybill:closure-admission";

/// C185 — the patches this member applies.
///
/// CycloneDX carries these natively in `pedigree.patches[]` and suppresses
/// this property; SPDX 2.3 and SPDX 3 have no equivalent and carry the
/// annotation. That is the one place the three formats differ in capability
/// rather than in spelling. The value is the JSON array encoded as a string,
/// matching the m134 / m147 / m173 convention for array-valued annotations.
pub(crate) const ANN_CLOSURE_PATCHES: &str = "waybill:closure-patches";

/// C205 (m1279, #1346) — dependency attributes that name this member and that
/// role classification does not read.
///
/// A sorted JSON array of attribute names, encoded as a string, matching the
/// m134 / m147 / m173 convention this file already follows for C185.
///
/// It is what stops `unreferenced` meaning two things. Without it, "nothing
/// in this closure depends on this member" and "the only thing that depends
/// on it is an attribute waybill does not classify" are the same output — the
/// first a fact about the closure, the second a fact about waybill — and a
/// consumer has no way to tell which they are looking at.
///
/// **It names the attributes** (FR-013). "Referenced by something waybill does
/// not classify" without saying what is a smaller version of the same defect:
/// it tells a consumer a gap exists and not where.
///
/// Present whenever an unclassified attribute names the member, **whatever its
/// role** — a reader comparing two `artifact-input` members should be able to
/// tell that one of them also has an unread reference. Absence therefore means
/// "no unclassified attribute names this member", never "not checked", and
/// that is only trustworthy because the FR-007 guard reports an attribute the
/// classifier does not recognise rather than ignoring it.
///
/// **Reach, stated rather than assumed.** This can only land on a component,
/// and [`emits_as_component`] drops an `Unreferenced` member that applies no
/// patch. So the gating table's "`unreferenced` + named" row materialises
/// only for patch-appliers, which is also the only place `unreferenced` is
/// observable in output at all. The other half of #1346's fix is the widening
/// in `classify.rs`: a member the four newly-consulted attributes reach stops
/// being `Unreferenced`, which is what makes it emitted in the first place.
pub(crate) const ANN_CLOSURE_UNCLASSIFIED_REFS: &str = "waybill:closure-unclassified-refs";

/// Milestone 1436 — internal: the build identity of a component whose
/// patches resolve a CVE, and which of those CVEs a VEX claim may be made
/// for.
///
/// An object `{"nix_drv": "<h1>[,<h2>…]", "claims": [cve…], "unscoped":
/// [cve…], "variants_without_patches": n}`. Filtered from every format by `is_internal_emission_key`: the
/// identity is already in the PURL, the claims are in the VEX, and the
/// unscoped count is C220, so emitting this as well would publish each fact
/// twice. It rides on the component, rather than in a side table, so that a
/// merge into another reader's component (`merge_closure_components` folds
/// closure annotations in) and every projection carry it for free.
pub(crate) const ANN_BACKPORT_IDENTITY: &str = "waybill:nix-backport-identity";

/// The PURL qualifier key that scopes a backport claim to this build
/// (research R1).
pub(crate) const NIX_DRV_QUALIFIER: &str = "nix_drv";

/// The `nix_drv` value a component's internal backport identity records.
fn recorded_nix_drv(c: &ResolvedComponent) -> Option<&str> {
    c.extra_annotations
        .get(ANN_BACKPORT_IDENTITY)
        .and_then(|v| v.get(NIX_DRV_QUALIFIER))
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
}

/// `purl` with every qualifier and any subpath removed. A backport claim and
/// a nixpkgs declaration are compared on this (research R10), so a claim is
/// matched to its component whichever passes added or kept the qualifier.
pub(crate) fn without_qualifiers(purl: &str) -> &str {
    let end = purl.find(['?', '#']).unwrap_or(purl.len());
    &purl[..end]
}

/// `purl` with `nix_drv=<value>` among its qualifiers, any previous
/// `nix_drv` replaced. `Purl::new` validates the result and sorts the
/// qualifiers, so the value is canonical whichever qualifiers were present.
fn with_nix_drv(purl: &Purl, value: &str) -> Option<Purl> {
    let raw = purl.as_str();
    let (head, subpath) = match raw.find('#') {
        Some(i) => (&raw[..i], &raw[i..]),
        None => (raw, ""),
    };
    let (base, quals) = match head.split_once('?') {
        Some((b, q)) => (b, q),
        None => (head, ""),
    };
    let mut pairs: Vec<String> = quals
        .split('&')
        .filter(|p| !p.is_empty())
        .filter(|p| p.split_once('=').map(|(k, _)| k) != Some(NIX_DRV_QUALIFIER))
        .map(str::to_string)
        .collect();
    pairs.push(format!("{NIX_DRV_QUALIFIER}={value}"));
    Purl::new(&format!("{base}?{}{subpath}", pairs.join("&"))).ok()
}

/// Give every component that records a backport identity its `nix_drv`
/// qualifier (spec FR-001), and return the `(old, new)` PURL of each one
/// changed.
///
/// Idempotent. Called by [`components`] as it emits, and again by the scan
/// immediately before the closure's edges are built, because three passes
/// between the two rebuild a component's PURL from its parts — the
/// milestone-1275, -1366 and -1378 identity passes — and a merge into
/// another reader's component keeps that component's PURL. The caller
/// rewrites relationship endpoints with the returned pairs through
/// `apply_renames` (#980): a PURL is identity, and changing one an edge
/// already names would orphan the edge.
pub(crate) fn qualify_backport_identities(
    components: &mut [ResolvedComponent],
) -> Vec<(String, String)> {
    let mut renames = Vec::new();
    for c in components.iter_mut() {
        let Some(value) = recorded_nix_drv(c).map(str::to_string) else {
            continue;
        };
        let Some(qualified) = with_nix_drv(&c.purl, &value) else {
            tracing::warn!(
                purl = %c.purl.as_str(),
                "nix-closure: a backport identity yields no valid PURL; leaving it unqualified"
            );
            continue;
        };
        if qualified != c.purl {
            renames.push((c.purl.as_str().to_string(), qualified.as_str().to_string()));
            c.purl = qualified;
        }
    }
    renames
}

/// Roles that become components on the strength of the role alone.
///
/// `Unreferenced` is excluded here, but that is not the whole rule — see
/// [`emits_as_component`], which readmits the patch-appliers among them.
fn role_alone_emits(role: DerivationRole) -> bool {
    matches!(
        role,
        DerivationRole::ArtifactInput | DerivationRole::BuildTooling | DerivationRole::Both
    )
}

/// Whether a closure member becomes a component.
///
/// Role is the usual test, with one exception that spec FR-004a makes
/// load-bearing: an `Unreferenced` member that applies a patch is in scope.
/// Measured, `jq` and `lua` are `Unreferenced` in one project's closure and
/// carry 6 of its 18 CVEs. Dropping them would leave those patches with no
/// component to hang a `pedigree` on, and the CVEs would simply vanish —
/// silently, since nothing downstream can tell an absent component from a
/// component with nothing to say.
///
/// The test is local to the derivation: applying a patch means a non-empty
/// `patches` field, which needs no cross-reference to the attribution pass.
/// Visible to the crate so `waybill debug closure-dump` can report the real
/// answer rather than a second implementation of this rule. Four probe
/// errors in one session came from modelling this logic in Python instead of
/// asking the binary; a debug surface that calls the same function cannot
/// drift from it.
/// Would this derivation emit even if its role admitted nothing?
///
/// Milestone 1372 (#1366) uses this to split C210's two counts: a derivation
/// the role alone admits is **newly emitted**, one that emits anyway is merely
/// **relabelled**. The two clauses here are exactly the non-role arms of
/// [`emits_as_component`], named once so the split cannot drift from the gate
/// it is splitting.
pub(crate) fn emits_without_role(drv: &RawDerivation) -> bool {
    drv.paths_in("patches").next().is_some() || drv.declared_pname().is_none()
}

pub(crate) fn emits_as_component(role: DerivationRole, drv: &RawDerivation) -> bool {
    role_alone_emits(role)
        || drv.paths_in("patches").next().is_some()
        // Milestone 1282 FR-001 — a member declaring no `pname` is emitted
        // whatever its C182 role.
        //
        // **This is the line that closes Gap A, and it is not a `pname`
        // test.** `pname()` falls back to the derivation's own `name`, so it
        // returns `Some` for all 1,145 and gates nothing; the clause that
        // hid them is `role_alone_emits`, because every one of them is
        // `Unreferenced` and applies no patch. Read from the golden: 1,315
        // `unreferenced` derivations yielded 11 components (research R8a).
        //
        // Deliberately asymmetric: a `pname`-**bearing** `Unreferenced`
        // member is still suppressed unless it applies a patch. Roughly 170
        // sit on that side, and emitting them is a question about a
        // population this milestone did not measure (spec Assumptions).
        || drv.declared_pname().is_none()
}

/// Is this a member milestone 1282 added, rather than one emitted before it?
///
/// The test is on the **declaration**, matching [`emits_as_component`]'s new
/// clause, so the two cannot drift into disagreeing about which population
/// they are talking about.
fn is_1282_member(drv: &RawDerivation) -> bool {
    drv.declared_pname().is_none()
}

/// The `.drv` keys that will become components, for the milestone-1280
/// projection.
///
/// Derived from [`emits_as_component`] rather than from a second copy of the
/// rule: the projection must emit an edge to exactly the nodes the emitter
/// emits, and two predicates that have to agree is the drift this project
/// keeps paying for. A derivation yielding no name is excluded here for the
/// same reason `components` skips it.
pub(crate) fn emitted_drv_keys<'a>(
    raw: &'a super::derivation::RawClosure,
    roles: &std::collections::BTreeMap<String, DerivationRole>,
) -> std::collections::HashSet<&'a str> {
    raw.derivations
        .iter()
        .filter(|(key, drv)| {
            roles
                .get(*key)
                .copied()
                .is_some_and(|role| emits_as_component(role, drv))
                && drv.pname().is_some()
                // Milestone 1282 FR-013 — of the members this feature adds,
                // only `source` and `package` may be edge endpoints. The
                // other two are emitted and walked *through*, so the edge
                // names the source a package was built from rather than the
                // build script that stood between them.
                //
                // Derived from the same classification the emitter uses, not
                // from a second copy of the rule: emission is now a strict
                // **superset** of edge participation, and
                // [`edge_endpoint_keys_are_a_subset_of_emitted`] holds the
                // two in that relation rather than trusting them to agree.
                && (!is_1282_member(drv) || member::classify(drv).is_edge_endpoint())
        })
        .map(|(key, _)| key.as_str())
        .collect()
}

/// The members the nixpkgs metadata lookup is counted over: every
/// [`emitted_drv_keys`] member, plus the evaluated package.
///
/// Milestone 1377 — the evaluated package is emitted whatever its role (it
/// is the subject), and its metadata is looked up like any member's, so it
/// is counted. Without it, a root emitted only because it is the root was
/// looked up but appeared in neither `wanted` nor `received`. Kept apart
/// from `emitted_drv_keys` because that set is also the edge projection's,
/// and the root is never an edge target.
///
/// `debug closure-dump` reports membership of this set per row, so an
/// independent count (`specs/1377-…/measurements/meta-eval-cost/reach.py`)
/// takes its population from the binary rather than modelling it.
pub(crate) fn coverage_drv_keys<'a>(
    raw: &'a super::derivation::RawClosure,
    roles: &std::collections::BTreeMap<String, DerivationRole>,
    evaluated: Option<&str>,
) -> std::collections::HashSet<&'a str> {
    let mut keys = emitted_drv_keys(raw, roles);
    if let Some((key, drv)) = evaluated.and_then(|k| raw.derivations.get_key_value(k)) {
        if drv.pname().is_some() {
            keys.insert(key.as_str());
        }
    }
    keys
}

/// Every `.drv` key this scan emits as a component.
///
/// Test-only: production derives the same population by building the
/// components themselves, and a second production caller would be the second
/// copy of the rule `emitted_drv_keys`' comment warns about. The set form
/// exists so the subset relation can be asserted.
///
/// Milestone 1282 — the superset of [`emitted_drv_keys`]. Before this feature
/// the two sets were one set, and `emitted_drv_keys` carries a comment saying
/// why that mattered: *two predicates that have to agree is the drift this
/// project keeps paying for*. The divergence here is deliberate, so it is
/// asserted rather than assumed.
#[cfg(test)]
pub(crate) fn component_drv_keys<'a>(
    raw: &'a super::derivation::RawClosure,
    roles: &std::collections::BTreeMap<String, DerivationRole>,
) -> std::collections::HashSet<&'a str> {
    raw.derivations
        .iter()
        .filter(|(key, drv)| {
            roles
                .get(*key)
                .copied()
                .is_some_and(|role| emits_as_component(role, drv))
                && drv.pname().is_some()
        })
        .map(|(key, _)| key.as_str())
        .collect()
}

/// What `components` merges on: `(name, version, identity-hash)`.
///
/// The third element is empty for every member emitted before milestone 1282,
/// so those keys sort and behave exactly as the `(name, version)` pair they
/// replaced.
type MergeKey = (String, String, String);

/// A content hash, made safe for a PURL name segment **without losing
/// distinctness**.
///
/// nix writes `sha256-<base64>` or a bare base32 digest, so the characters to
/// deal with are `+`, `/`, `=` and the algorithm separator `-`.
/// `encode_purl_segment` normalises only `+`, which is not enough.
///
/// **The obvious encoding is wrong, and it was written here first.** Mapping
/// `+`→`p`, `/`→`s`, `=`→`e` is not injective: `p`, `s` and `e` are
/// themselves base64 characters, so the hashes `a+b` and `apb` both encode to
/// `apb` and two distinct artifacts compare equal — the exact collision
/// FR-002 exists to prevent, reintroduced by the function meant to preserve
/// identity.
///
/// `.` is the escape introducer because it appears in **no** hash alphabet —
/// not base64, not base32, not the algorithm prefix — and so is never passed
/// through. Every escape is therefore unambiguous and the mapping is
/// injective, which is what the identity requires.
fn encode_hash_segment(hash: &str) -> String {
    let mut out = String::with_capacity(hash.len() + 8);
    for c in hash.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' => out.push(c),
            '+' => out.push_str(".p"),
            '/' => out.push_str(".s"),
            '=' => out.push_str(".e"),
            '-' => out.push_str(".h"),
            '.' => out.push_str(".d"),
            // Unreachable for every hash nix emits, and encoded rather than
            // replaced anyway: a catch-all that collapsed unknown characters
            // to one symbol would be the same defect as the mapping above.
            other => out.push_str(&format!(".x{:04x}", other as u32)),
        }
    }
    out
}

/// The URL a fixed-output derivation fetched from (FR-008).
///
/// `urls` is the plural form `fetchurl` sets and may hold several
/// whitespace-separated mirrors; `url` is the singular older spelling. The
/// first is taken — the mirrors are alternative routes to one artifact whose
/// identity is already the content hash, so listing them all would suggest
/// several sources where there is one.
///
/// Carried as a CycloneDX-native `externalReference` of type `distribution`
/// rather than a `waybill:` property, per Constitution Principle V: a
/// download location is a thing every format already has a slot for.
/// Measured: 138 of 1,145 members record one.
///
/// #1046 — read through [`RawDerivation::attr_str`], so a fetch that keeps
/// its URL under `structuredAttrs` is read too. At nixpkgs `a799d3e3`,
/// `fetchgit` does exactly that, and an `env`-only read lost every git
/// fetch's URL. Under structured attributes `urls` can be a JSON list, whose
/// first string is taken for the same reason as the whitespace form's.
pub(crate) fn source_url(drv: &RawDerivation) -> Option<&str> {
    drv.attr_str("urls")
        .and_then(|v| v.split_whitespace().next())
        .or_else(|| {
            drv.structured_attrs
                .as_ref()
                .and_then(|sa| sa.get("urls"))
                .and_then(serde_json::Value::as_array)
                .and_then(|list| list.iter().find_map(serde_json::Value::as_str))
        })
        .or_else(|| drv.attr_str("url"))
        .filter(|u| !u.is_empty())
}

/// The external references a closure component carries for where it came
/// from.
///
/// - A nameless member (`is_member`) that is a VCS checkout carries a `vcs`
///   reference (#1046). One that records only a URL carries the `distribution`
///   reference FR-008 of m1282 introduced. A repository URL is not a download,
///   so a checkout never carries `distribution`.
/// - A package carries the `vcs` reference of the checkout its `src` is
///   (#1046 FR-009). Through `src` only: its other inputs include the sources
///   of its build tools, which it was not built from.
fn source_references(
    drv: &RawDerivation,
    is_member: bool,
    out_index: &std::collections::BTreeMap<&str, &str>,
    raw: &super::derivation::RawClosure,
) -> Vec<waybill_common::resolution::ExternalReference> {
    if is_member {
        if let Some(v) = vcs_source(drv) {
            return vec![v.reference()];
        }
        return source_url(drv)
            .map(|url| {
                vec![waybill_common::resolution::ExternalReference {
                    ref_type: "distribution".to_string(),
                    url: url.to_string(),
                }]
            })
            .unwrap_or_default();
    }
    super::derivation::src_derivation(drv, out_index, &raw.derivations)
        .and_then(|k| raw.derivations.get(k))
        .and_then(vcs_source)
        .map(|v| vec![v.reference()])
        .unwrap_or_default()
}

/// #1046 — a fetch that records a repository URL **and** a revision: a VCS
/// checkout, as `fetchgit` produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VcsSource {
    pub(crate) url: String,
    pub(crate) rev: String,
}

impl VcsSource {
    /// The SPDX VCS-location form, `git+<url>@<rev>`, which is also the
    /// purl-spec `vcs_url` value and A10's `vcs` reference.
    pub(crate) fn location(&self) -> String {
        format!("git+{}@{}", self.url, self.rev)
    }

    /// The A10 external reference: CycloneDX `vcs`, SPDX 2.3 `externalRefs`
    /// `OTHER`/`vcs`, SPDX 3 `software_sourceInfo`.
    pub(crate) fn reference(&self) -> waybill_common::resolution::ExternalReference {
        waybill_common::resolution::ExternalReference {
            ref_type: "vcs".to_string(),
            url: self.location(),
        }
    }
}

/// The VCS source a derivation fetched, when it records both a URL and a
/// revision in either carrier. A URL without a revision is a download, and
/// stays [`source_url`]'s `distribution` reference.
pub(crate) fn vcs_source(drv: &RawDerivation) -> Option<VcsSource> {
    let url = drv.attr_str("url").filter(|u| !u.is_empty())?;
    let rev = drv.attr_str("rev").filter(|r| !r.is_empty())?;
    Some(VcsSource {
        url: url.to_string(),
        rev: rev.to_string(),
    })
}

/// The PURL coordinate a derivation is emitted under, and the identity hash
/// that distinguishes it (empty for every member that declares a `pname`).
///
/// `None` when the derivation yields no name.
///
/// Milestone 1378 (#1396) lifted this out of [`components`] so the contents
/// pass can find the component a derivation became. A `pname`-less member is
/// keyed by its hash, not by `(name, version)` — 56 members named `source`
/// share that pair — and a second copy of this rule in the caller would be
/// two predicates that have to agree.
pub(crate) fn coordinate(drv: &RawDerivation, key: &str) -> Option<(String, String)> {
    let name = drv.pname()?;
    let version = drv.version().unwrap_or_default();
    let basis = is_1282_member(drv).then(|| member::identity(drv, key));

    // `pkg:nix` is not a purl-spec type, so these carry `pkg:generic`
    // with the name and version nix recorded. A wrong-but-well-formed
    // PURL would be worse than a generic one: it would collide with
    // whatever really owns that coordinate.
    let coordinate = match &basis {
        // FR-002 — the hash occupies the **name segment**. A qualifier
        // does not satisfy the requirement: PURL equality is how
        // downstream tooling matches components, some tools compare
        // without qualifiers, and under such a comparison the 56 distinct
        // artifacts named `source` would collapse to one. That is
        // precisely the collision this identity exists to prevent.
        Some(b) => format!("pkg:generic/{}-{}", name, encode_hash_segment(b.hash())),
        None if version.is_empty() => format!("pkg:generic/{name}"),
        None => format!("pkg:generic/{name}@{version}"),
    };
    let identity_key = basis.as_ref().map(|b| b.hash().to_string()).unwrap_or_default();
    Some((coordinate, identity_key))
}

/// Is this a member milestone 1282 added? Crate-visible for the contents
/// pass, which keys such members by hash.
pub(crate) fn is_hash_keyed_member(drv: &RawDerivation) -> bool {
    is_1282_member(drv)
}

/// Build the components a classified closure contributes.
///
/// Returns them rather than mutating a caller's vector, so the supplementing
/// relationship is visible at the call site instead of buried here.
pub(crate) fn components(closure: &ClassifiedClosure) -> Vec<ResolvedComponent> {
    // Keyed by the identity a consumer sees, not by derivation, because a
    // closure holds several derivations per `(pname, version)` — build
    // variants. Measured on a real closure: 497 derivations emitted 348
    // distinct identities, so without this a third of the components were
    // duplicates carrying the same PURL, and `perl 5.42.0` appeared seven
    // times. A BTreeMap also fixes the output order independently of the
    // derivation hashes, which vary between machines.
    // The role travels beside the component rather than being read back out
    // of its own annotation: a round-trip through the wire string would make
    // merging depend on parsing what emission just wrote.
    // Milestone 1282 — the key gained a third element, and that is the whole
    // of research R8c. It was `(name, version)`; the members this feature
    // adds declare no `version` and keep their `name` unmodified (FR-016), so
    // all 56 members named `source` shared one key and merged into **one**
    // component — the collision FR-002, SC-014 and User Story 1 scenario 3
    // each forbid separately. The third element is the identity hash, and it
    // is empty for every member emitted before this feature, so their keys,
    // their ordering and their output are unchanged.
    let mut merged: std::collections::BTreeMap<
        MergeKey,
        (DerivationRole, Option<MemberRole>, ResolvedComponent),
    > = std::collections::BTreeMap::new();
    // C205 — unioned over build variants for the same reason the role is:
    // the component a consumer sees is one identity, and a reference through
    // any variant of it is a reference. Keyed by that identity rather than by
    // `.drv` path, which is what the index carries.
    let mut unclassified: std::collections::BTreeMap<
        MergeKey,
        std::collections::BTreeSet<String>,
    > = std::collections::BTreeMap::new();
    // Milestone 1281 — unioned over build variants for the same reason the
    // role and C205 are: the component a consumer sees is one identity, while
    // the license index is keyed by `.drv` path. Order is first-seen rather
    // than sorted, because FR-005 carries nixpkgs' stated order.
    let mut licenses_for: std::collections::BTreeMap<
        MergeKey,
        Vec<waybill_common::types::license::SpdxExpression>,
    > = std::collections::BTreeMap::new();
    // Milestone 1372 (#1366) — C211, keyed by identity because that is what a
    // consumer reads. `true` if any derivation behind the identity was put in
    // its role by the build graph alone.
    let mut admitted_for: std::collections::BTreeMap<MergeKey, bool> =
        std::collections::BTreeMap::new();
    // Milestone 1377 (#1394) — the CPEs nixpkgs declares for the derivations
    // behind each identity. Keyed by identity because that is what is
    // emitted; the index is keyed by `.drv`, as the license index is.
    let mut declared_for: std::collections::BTreeMap<
        MergeKey,
        Vec<&super::identifiers::Declared>,
    > = std::collections::BTreeMap::new();
    // Milestone 1377 (#1398) — the derivation the evaluated attribute built.
    // It is emitted whatever its role, because it is the document's subject:
    // nothing consumes it, so it classifies `Unreferenced`, and before this
    // it reached the document only if it happened to apply a patch (openssh)
    // or another reader emitted it (moat's cabal package).
    //
    // Deliberately not added to `emitted_drv_keys`. The root is never an
    // edge target (nothing consumes it) and `graph::project` already treats
    // it as an edge source, so the projection is unchanged. The license
    // index is keyed by `.drv` over every evaluated row, so the root still
    // receives its declared licenses here; what it does not do is join the
    // `wanted` population milestone 1281 counts coverage over.
    //
    // `evaluated`, not `root`: with several unconsumed derivations the
    // closure does not say which one was evaluated, and no subject is named.
    let root_key = closure.graph.evaluated.as_deref();
    let mut root_identity: Option<MergeKey> = None;
    // Milestone 1436 — every derivation merged into each identity, so the
    // backport census below can say how many build variants a component
    // stands for, including ones that apply no patch.
    let mut drvs_for: std::collections::BTreeMap<MergeKey, std::collections::BTreeSet<&str>> =
        std::collections::BTreeMap::new();
    // #1046 — output basename → `.drv`, so a package's `src` resolves to the
    // fetch it was built from. Built once, for every derivation.
    let out_index = closure.raw.output_index();
    for (key, drv) in &closure.raw.derivations {
        let Some(role) = closure.roles.get(key).copied() else {
            continue;
        };
        let is_root = root_key == Some(key.as_str());
        if !is_root && !emits_as_component(role, drv) {
            continue;
        }
        let Some(name) = drv.pname() else {
            continue;
        };
        let version = drv.version().unwrap_or_default();

        // Milestone 1282 — `None` for every member emitted before this
        // feature, which is what keeps their identity and their key exactly
        // as they were.
        let member_role = is_1282_member(drv).then(|| member::classify(drv));
        let Some((coordinate, identity_key)) = coordinate(drv, key) else {
            continue;
        };
        let merge_key: MergeKey = (name.to_string(), version.to_string(), identity_key);
        if is_root {
            root_identity = Some(merge_key.clone());
        }
        if closure.graph_admitted.contains(key) {
            admitted_for.insert(merge_key.clone(), true);
        }
        let Ok(purl) = Purl::new(&coordinate) else {
            tracing::debug!(name, "nix-closure: skipping a name that yields no valid PURL");
            continue;
        };

        let component = ResolvedComponent {
            build_inclusion: None,
            artifact_presence: None,
            scope_evidence: None,
            name: name.to_string(),
            version: version.to_string(),
            purl,
            evidence: ResolutionEvidence {
                technique: ResolutionTechnique::UrlPattern,
                confidence: 0.95,
                source_connection_ids: vec![],
                source_file_paths: vec![],
                deps_dev_match: None,
            },
            licenses: vec![],
            concluded_licenses: vec![],
            hashes: vec![],
            supplier: None,
            cpes: vec![],
            advisories: vec![],
            occurrences: vec![],
            lifecycle_scope: None,
            requirement_ranges: Vec::new(),
            source_type: None,
            sbom_tier: Some("build".to_string()),
            buildinfo_status: None,
            evidence_kind: None,
            binary_class: None,
            binary_stripped: None,
            linkage_kind: None,
            detected_go: None,
            confidence: None,
            binary_packed: None,
            npm_role: None,
            raw_version: None,
            parent_purl: None,
            co_owned_by: None,
            shade_relocation: None,
            external_references: source_references(drv, member_role.is_some(), &out_index, &closure.raw),
            extra_annotations: Default::default(),
            binary_role: None,
        };
        if let Some(found) = closure.licenses.get(key) {
            let slot = licenses_for.entry(merge_key.clone()).or_default();
            for expr in found {
                if !slot.iter().any(|e| e.as_str() == expr.as_str()) {
                    slot.push(expr.clone());
                }
            }
        }
        // Not for milestone-1282 members: a `bash-5.3.tar.gz` is named after
        // bash and is not bash (m1367 FR-002). None of them can join in
        // practice — they carry no version and their `.drv` is a fetch — so
        // this states the rule rather than relying on that.
        if member_role.is_none() {
            if let Some(found) = closure.identifiers.index.get(key) {
                declared_for.entry(merge_key.clone()).or_default().push(found);
            }
        }
        if let Some(attributes) = closure.read_outcome.unclassified_refs.get(key) {
            unclassified
                .entry(merge_key.clone())
                .or_default()
                .extend(attributes.iter().cloned());
        }
        drvs_for.entry(merge_key.clone()).or_default().insert(key.as_str());
        merged
            .entry(merge_key)
            // The variants are otherwise identical — same name, version and
            // PURL — so the role is the only thing that can differ, and it
            // does: measured, 13 of 97 duplicated identities disagree.
            //
            // FR-003 — this is also where the content-hash merge happens for
            // milestone 1282: two members sharing a name and a content hash
            // share a key, so they land here as one component. Measured, 549
            // fixed-output members become 502 components, and the merge is
            // correct — the same content fetched twice is one artifact.
            .and_modify(|(seen, _, _)| *seen = seen.union(role))
            .or_insert((role, member_role, component));
    }
    // Attribution runs here so the patch facts ride on the component itself.
    // One source of truth: CycloneDX translates it to native `pedigree`, the
    // two SPDX emitters carry it as an annotation, and nothing has to join
    // the two sides back together at emission time.
    let patched = super::patches::attribute(&closure.raw);
    let index: std::collections::HashMap<(&str, &str), &super::patches::ComponentPatches> =
        patched
            .iter()
            .map(|r| ((r.component.as_str(), r.version.as_deref().unwrap_or("")), r))
            .collect();

    let mut out = merged
        .into_iter()
        .map(|(identity, (role, member_role, mut component))| {
            // FR-001 / FR-009 — the licenses land in the component's own
            // `licenses` field, which every emitter already carries to its
            // format's native license construct. `concluded_licenses` stays
            // empty: a nixpkgs `meta.license` is what the packager declares,
            // not a conclusion drawn from inspecting the artifact.
            if let Some(found) = licenses_for.remove(&identity) {
                component.licenses = found;
            }
            // FR-005, FR-007 — the declared CPE is the component's only CPE.
            // Build variants of one identity that state different CPEs leave
            // it with none here, so the curated fallback applies; a CPE names
            // a vendor, product and version, so they never have been seen to.
            if let Some(found) = declared_for.remove(&identity) {
                let cpes: std::collections::BTreeSet<&super::identifiers::DeclaredCpe> =
                    found.iter().map(|d| &d.cpe).collect();
                if let (Some(cpe), 1) = (cpes.first(), cpes.len()) {
                    let basis = found
                        .iter()
                        .map(|d| d.basis)
                        .min()
                        .unwrap_or(super::identifiers::JoinBasis::NameVersion);
                    // What the curated table would have given this
                    // component, recorded when the declaration differs:
                    // bash's `5.3p9` against nixpkgs' `5.3` update `9`.
                    let curated = crate::generate::cpe::synthesize_cpes(&component);
                    let overrode = curated.first().map(String::as_str).filter(|c| *c != cpe.as_str());
                    component.extra_annotations.insert(
                        super::identifiers::ANN_DECLARED_CPE.to_string(),
                        super::identifiers::provenance(basis, overrode),
                    );
                    component.cpes = vec![(*cpe).clone().into_string()];
                } else {
                    tracing::debug!(
                        name = %component.name,
                        version = %component.version,
                        "nix-closure-identifiers: build variants declare different CPEs; \
                         emitting none from nixpkgs"
                    );
                }
            }
            component.extra_annotations.insert(
                ANN_CLOSURE_ROLE.to_string(),
                serde_json::Value::String(role.wire().to_string()),
            );
            // Internal: read by the root selector's evaluated-package rung,
            // filtered from every format by `is_internal_emission_key`.
            // `merge_closure_components` folds closure annotations into an
            // existing component with `or_insert`, which is how the tag
            // reaches a reader's component the root merged into.
            if root_identity.as_ref() == Some(&identity) {
                component.extra_annotations.insert(
                    crate::generate::root_selector::NIX_EVALUATED_PACKAGE_KEY.to_string(),
                    serde_json::Value::Bool(true),
                );
            }
            // C211 — how this identity earned that role. Only `build-graph`
            // is disclosed: a declared dependency attribute is the ordinary
            // case and says nothing a reader needs told.
            //
            // Unioned over build variants for the same reason the role is: a
            // component is one identity, and if ANY derivation behind it was
            // admitted by the graph alone, the identity owes the disclosure.
            if admitted_for.remove(&identity).unwrap_or(false) {
                component.extra_annotations.insert(
                    ANN_CLOSURE_ADMISSION.to_string(),
                    serde_json::Value::String("build-graph".to_string()),
                );
            }
            // FR-005 — exactly one role, on exactly the members this feature
            // adds. A component emitted before milestone 1282 carries none,
            // which is what lets a consumer tell the two populations apart
            // and what FR-007's filter relies on (a filter keeping only
            // `source` and `package` would otherwise discard all 381 named
            // packages).
            if let Some(m) = member_role {
                component.extra_annotations.insert(
                    member::ANN_MEMBER_ROLE.to_string(),
                    serde_json::Value::String(m.wire().to_string()),
                );
            }
            if let Some(attributes) = unclassified.get(&identity) {
                // Non-empty by construction: the index only records a member
                // an attribute actually named. Guarded anyway, because an
                // empty array would assert "checked, and nothing" in the one
                // slot whose absence already says that.
                if !attributes.is_empty() {
                    component.extra_annotations.insert(
                        ANN_CLOSURE_UNCLASSIFIED_REFS.to_string(),
                        serde_json::Value::String(
                            serde_json::Value::Array(
                                attributes
                                    .iter()
                                    .map(|a| serde_json::Value::String(a.clone()))
                                    .collect(),
                            )
                            .to_string(),
                        ),
                    );
                }
            }
            // Only for members that predate milestone 1282. A new member
            // has an empty `version`, so an unguarded lookup could attach a
            // named component's patch record to an unrelated source that
            // happens to share its name.
            if let Some(record) = (member_role.is_none())
                .then(|| index.get(&(component.name.as_str(), component.version.as_str())))
                .flatten()
            {
                if !record.patches.is_empty() {
                    component.extra_annotations.insert(
                        ANN_CLOSURE_PATCHES.to_string(),
                        serde_json::Value::String(
                            super::patches::patches_json(record).to_string(),
                        ),
                    );
                }
                // Milestone 1436 (FR-001, research R4) — a component whose
                // patches resolve a CVE carries the identity of every
                // patching build it stands for. One with patches that name
                // no CVE keeps its PURL byte-identical (clarification 2).
                if !record.distinct_cves().is_empty() {
                    // Emitted derivations merged here that apply no patch.
                    // Recorded so the census can report them: measured 0 on
                    // five nixpkgs targets (#1436 T001), and a claim is not
                    // about a build no patch reached.
                    let unpatched = drvs_for.get(&identity).map_or(0, |ds| {
                        ds.iter().filter(|d| !record.variants.contains_key(**d)).count()
                    });
                    component.extra_annotations.insert(
                        ANN_BACKPORT_IDENTITY.to_string(),
                        serde_json::json!({
                            NIX_DRV_QUALIFIER: record.drv_hashes().join(","),
                            "claims": record.every_variant_resolves(),
                            "unscoped": record.unscoped_cves(),
                            "variants_without_patches": unpatched,
                        }),
                    );
                }
            }
            component
        })
        .collect::<Vec<_>>();
    qualify_backport_identities(&mut out);
    out
}

/// The claims a component's backport identity records (CVEs every patching
/// variant resolves), or none.
pub(crate) fn backport_claims(c: &ResolvedComponent) -> Vec<&str> {
    backport_list(c, "claims")
}

/// The CVEs only some of a component's patching variants resolve: C220's
/// population on this component.
pub(crate) fn backport_unscoped(c: &ResolvedComponent) -> Vec<&str> {
    backport_list(c, "unscoped")
}

fn backport_list<'a>(c: &'a ResolvedComponent, field: &str) -> Vec<&'a str> {
    c.extra_annotations
        .get(ANN_BACKPORT_IDENTITY)
        .and_then(|v| v.get(field))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .collect()
}

/// Whether `c`'s PURL carries the `nix_drv` value its backport identity
/// records. A claim is made only then (FR-002): a PURL that could not be
/// qualified would put a build-scoped claim on every build of the version.
pub(crate) fn carries_build_identity(c: &ResolvedComponent) -> bool {
    recorded_nix_drv(c).is_some_and(|value| {
        let p = c.purl.as_str();
        let quals = p.split_once('?').map_or("", |(_, q)| q);
        let quals = quals.split('#').next().unwrap_or("");
        quals
            .split('&')
            .any(|kv| kv.split_once('=') == Some((NIX_DRV_QUALIFIER, value)))
    })
}

/// #1436 T001 — the census research R4 left to measure, over the components
/// a scan will emit: (a) components with a resolved CVE, (b) how many of
/// those merge more than one patching derivation, and (c) the (component,
/// CVE) pairs not every variant resolves, which is C220. Silent when (a) is
/// zero, so a scan without a backport logs nothing new.
pub(crate) fn log_backport_census(components: &[ResolvedComponent]) {
    let (mut a, mut b, mut pairs, mut c, mut unpatched) = (0usize, 0usize, 0usize, 0usize, 0u64);
    for comp in components {
        let Some(identity) = comp.extra_annotations.get(ANN_BACKPORT_IDENTITY) else {
            continue;
        };
        a += 1;
        if recorded_nix_drv(comp).is_some_and(|v| v.contains(',')) {
            b += 1;
        }
        let claims = backport_claims(comp).len();
        let unscoped = backport_unscoped(comp).len();
        pairs += claims + unscoped;
        c += unscoped;
        unpatched += identity
            .get("variants_without_patches")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
    }
    if a > 0 {
        tracing::info!(
            components = a,
            multi_variant = b,
            pairs,
            unscoped_pairs = c,
            variants_without_patches = unpatched,
            "nix-closure: backport identity"
        );
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::super::classify::classify;
    use super::super::derivation::RawClosure;
    use super::*;

    const CLOSURE: &str = r#"{
      "derivations": {
        "d-lib.drv":  { "env": {"pname":"lib","version":"1.0"}, "outputs":{"out":{"path":"h1-lib"}} },
        "d-cc.drv":   { "env": {"pname":"cc","version":"13"},   "outputs":{"out":{"path":"h2-cc"}} },
        "d-orph.drv": { "env": {"pname":"orph","version":"9"},  "outputs":{"out":{"path":"h4-orph"}} },
        "d-app.drv": {
          "env": {"pname":"app","version":"2.0",
                  "buildInputs":"/nix/store/h1-lib",
                  "nativeBuildInputs":"/nix/store/h2-cc"},
          "outputs":{"out":{"path":"h5-app"}}
        }
      },
      "version": 3
    }"#;

    fn built() -> Vec<ResolvedComponent> {
        let raw = RawClosure::parse(CLOSURE).unwrap();
        let roles = classify(&raw);
        components(&ClassifiedClosure {
            runtime: None,
            attribute: "default".into(),
            raw,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
            identifiers: Default::default(),
        })
    }

    #[test]
    fn emits_artifact_inputs_and_tooling_each_marked_with_its_role() {
        let c = built();
        // CONTROL: the fixture yields components at all, so the role
        // assertions below are not passing over an empty vector.
        assert_eq!(c.len(), 2, "expected lib and cc, got {:?}",
                   c.iter().map(|x| &x.name).collect::<Vec<_>>());

        let role_of = |name: &str| {
            c.iter()
                .find(|x| x.name == name)
                .and_then(|x| x.extra_annotations.get(ANN_CLOSURE_ROLE))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };
        assert_eq!(role_of("lib").as_deref(), Some("artifact-input"));
        assert_eq!(role_of("cc").as_deref(), Some("build-tooling"));
    }

    #[test]
    fn build_tooling_is_emitted_rather_than_dropped() {
        // The clarified decision, and load-bearing beyond tidiness: the
        // richest vulnerability signal in both measured closures — `unzip`
        // with 11 CVEs — is build tooling. Dropping tooling would discard it.
        assert!(built().iter().any(|c| c.name == "cc"));
    }

    #[test]
    fn m1372_t034_one_name_at_two_versions_survives_as_two_components() {
        // The merge key is (name, version, identity), so a name admitted by
        // the build graph at two versions must stay two components. The real
        // closure contains the case and the document gets it right -- both
        // `cctools-binutils-darwin` at `1010.6` and `boot` and `macOS-SDK` at
        // `14.4` and `26.4` survive as pairs -- but a merge keyed on the name
        // alone would collapse each pair into one, and no other test here
        // would notice: every other fixture uses a single version per name.
        //
        // Both derivations below are reachable only through `inputs.drvs`,
        // which is exactly the population milestone 1372 admits, so this
        // covers the merge on the *new* path rather than the declared one.
        const TWO_VERSIONS: &str = r#"{
          "derivations": {
            "d-sdk-a.drv": { "env": {"pname":"sdk","version":"14.4"},
                             "outputs":{"out":{"path":"h1-sdk-a"}} },
            "d-sdk-b.drv": { "env": {"pname":"sdk","version":"26.4"},
                             "outputs":{"out":{"path":"h2-sdk-b"}} },
            "d-app.drv": {
              "env": {"pname":"app","version":"1.0"},
              "inputs": {"drvs": {"d-sdk-a.drv": {}, "d-sdk-b.drv": {}}},
              "outputs":{"out":{"path":"h3-app"}}
            }
          },
          "version": 3
        }"#;

        let raw = RawClosure::parse(TWO_VERSIONS).unwrap();
        let roles = classify(&raw);
        let graph_admitted = super::super::classify::graph_admitted(&raw);

        // CONTROL: both versions must actually be graph-admitted, or the
        // assertion below passes over a population this test never created.
        assert_eq!(
            graph_admitted.len(),
            2,
            "expected both sdk derivations admitted by the graph, got {graph_admitted:?}"
        );

        let built = components(&ClassifiedClosure {
            runtime: None,
            attribute: "default".into(),
            raw,
            roles,
            graph_admitted,
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
            identifiers: Default::default(),
        });

        let mut versions: Vec<&str> = built
            .iter()
            .filter(|c| c.name == "sdk")
            .map(|c| c.version.as_str())
            .collect();
        versions.sort_unstable();
        assert_eq!(
            versions,
            vec!["14.4", "26.4"],
            "one name at two versions collapsed; got {:?}",
            built
                .iter()
                .map(|c| (&c.name, &c.version))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn build_variants_merge_into_one_component_and_union_their_roles() {
        // A closure holds several derivations per (pname, version).
        // Measured on a real closure: 497 derivations, 348 distinct
        // identities, `perl 5.42.0` seven times — and 13 of 97 duplicated
        // identities disagreed about their role, so the union is not a
        // formality.
        const VARIANTS: &str = r#"{
          "derivations": {
            "v1.drv":  { "env": {"pname":"dup","version":"1.0"},
                         "outputs":{"out":{"path":"o1"}} },
            "v2.drv":  { "env": {"pname":"dup","version":"1.0"},
                         "outputs":{"out":{"path":"o2"}} },
            "app.drv": { "env": {"pname":"app","version":"2",
                                 "buildInputs":"/nix/store/o1",
                                 "nativeBuildInputs":"/nix/store/o2"},
                         "outputs":{"out":{"path":"o3"}} }
          },
          "version": 3
        }"#;
        let raw = RawClosure::parse(VARIANTS).unwrap();
        let roles = classify(&raw);
        // CONTROL: the two variants really do carry different roles, so the
        // union below is exercised rather than trivially satisfied.
        assert_eq!(roles["v1.drv"], DerivationRole::ArtifactInput);
        assert_eq!(roles["v2.drv"], DerivationRole::BuildTooling);

        let c = components(&ClassifiedClosure {
            runtime: None,
            attribute: "default".into(),
            raw,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
            identifiers: Default::default(),
        });
        let dups: Vec<_> = c.iter().filter(|x| x.name == "dup").collect();
        assert_eq!(dups.len(), 1, "two variants, one component");
        assert_eq!(
            dups[0].extra_annotations[ANN_CLOSURE_ROLE],
            serde_json::Value::String("both".into()),
            "artifact-input unioned with build-tooling is both"
        );
    }

    #[test]
    fn unreferenced_is_absorbed_rather_than_winning_a_union() {
        // A component referenced through any variant is referenced.
        assert_eq!(
            DerivationRole::Both.union(DerivationRole::Unreferenced),
            DerivationRole::Both
        );
        assert_eq!(
            DerivationRole::Unreferenced.union(DerivationRole::Unreferenced),
            DerivationRole::Unreferenced
        );
    }

    #[test]
    fn an_unreferenced_member_that_applies_a_patch_is_still_a_component() {
        // FR-004a, and the reason it exists: measured, `jq` and `lua` are
        // Unreferenced in a real closure and carry 6 of its 18 CVEs. Without
        // this exception those patches have no component to attach to.
        const WITH_PATCHER: &str = r#"{
          "derivations": {
            "d-orph.drv": { "env": {"pname":"orph","version":"9"},
                            "outputs":{"out":{"path":"h4-orph"}} },
            "d-fix.drv":  { "env": {"pname":"fix","version":"1",
                                    "patches":"/nix/store/h-CVE-2020-1.patch"},
                            "outputs":{"out":{"path":"h6-fix"}} }
          },
          "version": 3
        }"#;
        let raw = RawClosure::parse(WITH_PATCHER).unwrap();
        let roles = classify(&raw);
        // CONTROL: both are Unreferenced, so the assertions below turn on the
        // patch field and not on some role difference between the two.
        assert_eq!(roles["d-orph.drv"], DerivationRole::Unreferenced);
        assert_eq!(roles["d-fix.drv"], DerivationRole::Unreferenced);

        let names: Vec<&str> = components(&ClassifiedClosure {
            runtime: None,
            attribute: "default".into(),
            raw,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
            identifiers: Default::default(),
        })
        .iter()
        .map(|c| c.name.clone())
        .collect::<Vec<_>>()
        .leak()
        .iter()
        .map(String::as_str)
        .collect();
        assert_eq!(names, vec!["fix"], "the patch-applier is readmitted, the other is not");
    }

    #[test]
    fn unreferenced_members_are_not_components_here() {
        // Including the root: nothing depends on the thing being built. The
        // patch path handles the Unreferenced members that are in scope.
        let built = built();
        let named = |n: &str| built.iter().any(|c| c.name == n);
        assert!(!named("orph"));
        assert!(!named("app"));
    }

    /// #1436 — two build variants of one identity, both patching a CVE, a
    /// component whose patch names none, and an unpatched component.
    const H1: &str = "0b1d6zkr6zq8m4rv7ah3ylnrwfkdsxfh";
    const H2: &str = "9s0k1rfm4y0gn1p6dq8jrx5fhkw2lc3v";

    fn backport_closure() -> String {
        format!(
            r#"{{
          "derivations": {{
            "/nix/store/{H2}-waybill-fixture-zip-6.0.drv": {{ "env": {{"pname":"waybill-fixture-zip","version":"6.0",
                    "patches":"/nix/store/h-CVE-2099-0001.patch"}}, "outputs":{{"out":{{"path":"/nix/store/o2-zip"}}}} }},
            "/nix/store/{H1}-waybill-fixture-zip-6.0.drv": {{ "env": {{"pname":"waybill-fixture-zip","version":"6.0",
                    "patches":"/nix/store/h-CVE-2099-0001.patch"}}, "outputs":{{"out":{{"path":"/nix/store/o1-zip"}}}} }},
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-waybill-fixture-plain-1.0.drv": {{ "env": {{"pname":"waybill-fixture-plain","version":"1.0",
                    "patches":"/nix/store/h-gcc-15.patch"}}, "outputs":{{"out":{{"path":"/nix/store/o3-plain"}}}} }},
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-waybill-fixture-lib-1.0.drv": {{ "env": {{"pname":"waybill-fixture-lib","version":"1.0"}},
                    "outputs":{{"out":{{"path":"/nix/store/o4-lib"}}}} }},
            "/nix/store/cccccccccccccccccccccccccccccccc-waybill-fixture-app-1.0.drv": {{ "env": {{"pname":"waybill-fixture-app","version":"1.0",
                    "buildInputs":"/nix/store/o4-lib"}}, "outputs":{{"out":{{"path":"/nix/store/o5-app"}}}} }}
          }},
          "version": 3
        }}"#
        )
    }

    fn backport_components() -> Vec<ResolvedComponent> {
        let raw = RawClosure::parse(&backport_closure()).unwrap();
        let roles = classify(&raw);
        components(&ClassifiedClosure {
            runtime: None,
            attribute: "default".into(),
            raw,
            roles,
            graph_admitted: Default::default(),
            graph_admission: Default::default(),
            read_outcome: Default::default(),
            graph: Default::default(),
            licenses: Default::default(),
            degraded_licenses: None,
            license_coverage: Default::default(),
            identifiers: Default::default(),
        })
    }

    fn purl_of(c: &[ResolvedComponent], name: &str) -> String {
        c.iter()
            .find(|x| x.name == name)
            .map(|x| x.purl.as_str().to_string())
            .unwrap_or_else(|| panic!("no {name}: {:?}", c.iter().map(|x| &x.name).collect::<Vec<_>>()))
    }

    #[test]
    fn m1436_a_cve_resolving_component_names_every_patching_variant() {
        let c = backport_components();
        assert_eq!(
            purl_of(&c, "waybill-fixture-zip"),
            format!("pkg:generic/waybill-fixture-zip@6.0?nix_drv={H1},{H2}"),
            "sorted, comma-joined, one per patching variant"
        );
        let zip = c.iter().find(|x| x.name == "waybill-fixture-zip").unwrap();
        assert!(carries_build_identity(zip));
        assert_eq!(backport_claims(zip), ["CVE-2099-0001"]);
        assert!(backport_unscoped(zip).is_empty());
    }

    #[test]
    fn m1436_patches_naming_no_cve_leave_the_purl_byte_identical() {
        // Clarification 2: the qualifier exists only to scope a claim, so a
        // component with no claim to scope keeps its PURL.
        let c = backport_components();
        // CONTROL: the component applies a patch, so it is not unqualified
        // merely for having none.
        let plain = c.iter().find(|x| x.name == "waybill-fixture-plain").unwrap();
        assert!(plain.extra_annotations.contains_key(ANN_CLOSURE_PATCHES));
        assert_eq!(plain.purl.as_str(), "pkg:generic/waybill-fixture-plain@1.0");
        assert!(!plain.extra_annotations.contains_key(ANN_BACKPORT_IDENTITY));
        assert_eq!(purl_of(&c, "waybill-fixture-lib"), "pkg:generic/waybill-fixture-lib@1.0");
    }

    #[test]
    fn m1436_the_same_closure_gives_the_same_purl() {
        // US3-1: the identity is a function of the closure alone.
        let a: Vec<String> = backport_components().iter().map(|c| c.purl.as_str().to_string()).collect();
        let b: Vec<String> = backport_components().iter().map(|c| c.purl.as_str().to_string()).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn m1436_qualifying_is_idempotent_and_survives_a_rename() {
        // A later identity pass rebuilds the PURL from its parts, dropping
        // the qualifier; the scan restores it, and the pair it returns is
        // what moves the edges (#980).
        let mut c = backport_components();
        assert!(qualify_backport_identities(&mut c).is_empty(), "already qualified");
        let zip = c.iter_mut().find(|x| x.name == "waybill-fixture-zip").unwrap();
        zip.purl = Purl::new("pkg:hackage/waybill-fixture-zip@6.0?arch=x").unwrap();
        let renames = qualify_backport_identities(&mut c);
        let expected = format!("pkg:hackage/waybill-fixture-zip@6.0?arch=x&nix_drv={H1},{H2}");
        assert_eq!(
            renames,
            vec![("pkg:hackage/waybill-fixture-zip@6.0?arch=x".to_string(), expected.clone())]
        );
        assert_eq!(purl_of(&c, "waybill-fixture-zip"), expected);
        assert!(qualify_backport_identities(&mut c).is_empty());
    }

    #[test]
    fn m1436_without_qualifiers_strips_every_qualifier_and_subpath() {
        assert_eq!(without_qualifiers("pkg:generic/a@1?nix_drv=x,y&b=c#sub"), "pkg:generic/a@1");
        assert_eq!(without_qualifiers("pkg:generic/a@1"), "pkg:generic/a@1");
    }

    #[test]
    fn every_component_carries_a_version_and_a_valid_purl() {
        for c in built() {
            assert!(!c.version.is_empty(), "{} has no version", c.name);
            assert!(c.purl.as_str().starts_with("pkg:generic/"));
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod m1282_tests {
    use super::super::classify::classify;
    use super::super::derivation::RawClosure;
    use super::*;

    fn fixture() -> RawClosure {
        RawClosure::parse(include_str!(
            "../../../../../tests/fixtures/nix_closure_members/closure.json"
        ))
        .unwrap()
    }

    #[test]
    fn m1282_t031_edge_endpoints_are_a_subset_of_emitted_components() {
        // **The assertion this milestone exists to keep honest.**
        //
        // `emitted_drv_keys` carries a comment warning that *two predicates
        // that have to agree is the drift this project keeps paying for*, and
        // milestone 1282 breaks that identity deliberately: after FR-001 the
        // emitted set is every member, and after FR-013 the endpoint set is
        // only the `pname`-bearing members plus `source` and `package`.
        //
        // Emission is therefore a strict **superset** of edge participation.
        // That relation is what keeps the divergence from becoming a
        // disagreement, so it is asserted rather than trusted. An endpoint
        // outside the emitted set is an edge naming a component the document
        // does not contain — the thing FR-013's last sentence forbids.
        let raw = fixture();
        let roles = classify(&raw);
        let endpoints = emitted_drv_keys(&raw, &roles);
        let components = component_drv_keys(&raw, &roles);

        for key in &endpoints {
            assert!(
                components.contains(key),
                "edge endpoint {key} is not an emitted component"
            );
        }

        // CONTROL: the two sets must actually DIFFER on this fixture, or the
        // subset assertion above holds vacuously and would keep passing if
        // the feature stopped emitting the 513.
        assert!(
            components.len() > endpoints.len(),
            "emission must be a STRICT superset here; \
             components={} endpoints={} — if they are equal this test proves nothing",
            components.len(),
            endpoints.len()
        );
    }

    #[test]
    fn m1282_t016_identical_content_merges_and_differing_content_does_not() {
        let raw = fixture();
        let roles = classify(&raw);
        let keys = component_drv_keys(&raw, &roles);

        // Both members named `source` carry different hashes: two components.
        assert!(keys.contains("/nix/store/cccccccc-source.drv"));
        assert!(keys.contains("/nix/store/eeeeeeee-source.drv"));
        // Both `waybill-fixture-tarball` members carry the SAME hash, under
        // the two different spellings: one component after the merge, which
        // the component-level test in `closure_members.rs` asserts.
        assert!(keys.contains("/nix/store/ffffffff-waybill-fixture-tarball.drv"));
        assert!(keys.contains("/nix/store/11111111-waybill-fixture-tarball.drv"));
    }

    #[test]
    fn m1282_the_hash_segment_encoding_is_injective() {
        // The obvious encoding (`+`->`p`, `/`->`s`, `=`->`e`) is NOT, because
        // those letters are themselves base64 characters: `a+b` and `apb`
        // both become `apb`. That version was written here first and would
        // have re-created the collision FR-002 exists to prevent, inside the
        // function meant to preserve identity.
        //
        // Exhaustive over every string up to length 4 drawn from the full
        // character set a nix hash can contain.
        let alphabet = ['A', 'B', 'Z', 'a', 'z', '0', '9', '+', '/', '=', '-', '.'];
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut count = 0usize;
        for n in 1..=4 {
            let mut idx = vec![0usize; n];
            loop {
                let s: String = idx.iter().map(|&i| alphabet[i]).collect();
                let e = encode_hash_segment(&s);
                if let Some(prev) = seen.insert(e.clone(), s.clone()) {
                    assert_eq!(prev, s, "collision: {prev:?} and {s:?} both encode to {e:?}");
                }
                count += 1;
                let mut k = n;
                loop {
                    if k == 0 {
                        break;
                    }
                    k -= 1;
                    idx[k] += 1;
                    if idx[k] < alphabet.len() {
                        break;
                    }
                    idx[k] = 0;
                    if k == 0 {
                        break;
                    }
                }
                if idx.iter().all(|&i| i == 0) {
                    break;
                }
            }
        }
        assert!(count > 20_000, "expected an exhaustive sweep, ran {count}");

        // And the output is safe in a PURL name segment.
        for encoded in seen.keys() {
            assert!(
                encoded
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.'),
                "unsafe character in {encoded:?}"
            );
        }
    }


    // ---------------------------------------------------------------
    // #1046 — a fetch's VCS source, and its URL, from either carrier
    // ---------------------------------------------------------------

    fn one_drv(json: &str) -> super::super::derivation::RawDerivation {
        serde_json::from_str(json).unwrap()
    }

    const REV: &str = "1111111111111111111111111111111111111111";

    /// The `fetchgit` shape measured at nixpkgs `a799d3e3`: `env` holds only
    /// `out`, and `url`/`rev` live under `structuredAttrs`.
    #[test]
    fn m1046_vcs_source_reads_structured_attributes() {
        let d = one_drv(&format!(
            r#"{{"name":"src","env":{{"out":"/nix/store/x"}},
                "structuredAttrs":{{"url":"https://example.invalid/waybill-fixture.git","rev":"{REV}"}},
                "outputs":{{"out":{{"path":"h-src","hash":"sha256-AAAA"}}}}}}"#
        ));
        let v = vcs_source(&d).expect("structured url+rev is a VCS source");
        assert_eq!(v.url, "https://example.invalid/waybill-fixture.git");
        assert_eq!(v.rev, REV);
        assert_eq!(
            v.reference().url,
            format!("git+https://example.invalid/waybill-fixture.git@{REV}")
        );
        assert_eq!(v.reference().ref_type, "vcs");
    }

    #[test]
    fn m1046_vcs_source_reads_the_environment_too() {
        let d = one_drv(&format!(
            r#"{{"env":{{"url":"https://example.invalid/waybill-fixture.git","rev":"{REV}"}}}}"#
        ));
        assert!(vcs_source(&d).is_some());
    }

    /// CONTROL: a URL without a revision is a download, not a checkout; a
    /// revision without a URL names nothing; an empty value is absent.
    #[test]
    fn m1046_vcs_source_needs_both_and_neither_empty() {
        let no_rev = one_drv(r#"{"env":{"url":"https://example.invalid/waybill-fixture.tar.gz"}}"#);
        assert!(vcs_source(&no_rev).is_none());
        let no_url = one_drv(&format!(r#"{{"structuredAttrs":{{"rev":"{REV}"}}}}"#));
        assert!(vcs_source(&no_url).is_none());
        let empty = one_drv(r#"{"structuredAttrs":{"url":"","rev":""}}"#);
        assert!(vcs_source(&empty).is_none());
    }

    /// `source_url` reads `url` from structured attributes, and a structured
    /// `urls` list's first entry; the `env` forms are unchanged.
    #[test]
    fn m1046_source_url_reads_both_carriers() {
        let structured_url =
            one_drv(r#"{"structuredAttrs":{"url":"https://example.invalid/a.tar.gz"}}"#);
        assert_eq!(source_url(&structured_url), Some("https://example.invalid/a.tar.gz"));
        let structured_list = one_drv(
            r#"{"structuredAttrs":{"urls":["https://example.invalid/b.tar.gz","https://mirror.invalid/b.tar.gz"]}}"#,
        );
        assert_eq!(source_url(&structured_list), Some("https://example.invalid/b.tar.gz"));
        // CONTROL: the env forms, as before.
        let env_urls = one_drv(
            r#"{"env":{"urls":"https://example.invalid/c.tar.gz https://mirror.invalid/c.tar.gz"}}"#,
        );
        assert_eq!(source_url(&env_urls), Some("https://example.invalid/c.tar.gz"));
        let env_url = one_drv(r#"{"env":{"url":"https://example.invalid/d.tar.gz"}}"#);
        assert_eq!(source_url(&env_url), Some("https://example.invalid/d.tar.gz"));
        assert_eq!(source_url(&one_drv(r#"{"env":{"url":""}}"#)), None);
    }
}
