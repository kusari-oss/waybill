# Contract: root override component policy

Feature `860-multi-main-module-override` · satisfies FR-001 … FR-011

Governs what an operator-supplied root (`--root-name` / `--root-version`
/ `--root-purl`) does to components carrying the main-module role.

## C-1 — One policy at every N

- **C-1.1** Behaviour MUST NOT branch on the number of main-module
  components. N=0, N=1 and N>1 follow the same rule.
- **C-1.2** This supersedes the milestone-077 clean-replacement default
  and the milestone-149 N>1 fall-through (`root_selector.rs:525`).

## C-2 — Retention

- **C-2.1** A main-module component MUST be retained in `components[]`
  under an active override, demoted per the milestone-149 shape: role
  annotation removed, `waybill:demoted-from-main-module = "true"` added,
  emitted type `library`.
- **C-2.2** Retention MUST NOT alter the component's PURL, name,
  version, licenses or hashes.
- **C-2.3** An override MUST NOT reduce the component set. For any
  scan, the components emitted with an override are a superset of those
  emitted without it, minus at most one component absorbed under C-4.

## C-3 — Edges

- **C-3.1** A retained module MUST keep its own outbound edges. They
  MUST NOT be re-anchored onto the root. *(Supersedes the milestone-149
  US1 Option A decision recorded 2026-06-29, and the description of it
  in catalog row C102.)*
- **C-3.2** The root MUST declare a dependency on **every** retained
  module, independent of inter-module edges.
- **C-3.3** Every dependency reference MUST resolve to a component
  present in the same document.

## C-4 — Identity collision

- **C-4.1** When a retained module's PURL equals the root's PURL, the
  module MUST NOT be emitted separately; its outbound edges attach to
  the root, and no root→module edge is emitted for it.

## C-5 — Format parity

- **C-5.1** The policy MUST be implemented once, in
  `apply_main_module_drop_or_demote`, and MUST produce equivalent shape
  in CycloneDX, SPDX 2.3 and SPDX 3.
- **C-5.2** The C102 annotation VALUE MUST be byte-identical across all
  three formats. Whether its SPDX 3 *subject* aligns with the other two
  depends on the `package_iri_by_purl` alias — see C-6.

## C-6 — The SPDX 3 alias

- **C-6.1** The alias at `v3_document.rs:318-324` exists to serve edge
  re-anchoring. C-3.1 removes re-anchoring for this path.
- **C-6.2** Whether the alias is still required MUST be **verified**,
  not assumed. If it serves no remaining purpose it SHOULD be removed,
  which aligns the SPDX 3 annotation subject with CDX and SPDX 2.3 and
  closes the divergence milestone 149 deferred.
- **C-6.3** If the alias is still required, the divergence MUST be
  re-documented in C102 rather than silently carried.

## C-7 — Flag compatibility

- **C-7.1** `--preserve-manifest-main-module` MUST continue to be
  accepted and MUST NOT error.
- **C-7.2** It becomes a no-op. Help text MUST say so.

## C-8 — What this must not become

- **C-8.1** Not a change to root *selection*. How waybill picks a main
  module absent an override is milestones 127/201 and is untouched.
- **C-8.2** Not a second root. Retention MUST NOT produce a document
  with two subjects.
