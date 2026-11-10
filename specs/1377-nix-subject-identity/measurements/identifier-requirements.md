# Does a subject without a CPE meet the minimum-elements guidance? — checked 2026-11-09

Question (FR-012): when a Nix subject has neither a nixpkgs-declared nor a
curated CPE, may it carry no CPE at all, instead of the
`cpe:2.3:a:waybill:<name>:<version>` placeholder?

## CISA, "2026 Minimum Elements for a Software Bill of Materials (SBOM)"

Final, July 2026. Co-sealed by CISA, NSA, FBI, ASD's ACSC, Cyber Centre,
NÚKIB, ANSSI, BSI, CERT-In, ACN, METI, NCO, NIS/NCSC, KISA, NCSC-NL, NCSC-NZ,
NASK and NBU. Retrieved from
`https://www.cisa.gov/sites/default/files/2026-07/2026_cisa_sbom_minimum_elements_508c.pdf`
(the page `https://www.cisa.gov/resources-tools/resources/2026-minimum-elements-software-bill-materials-sbom`
links it).

Page 12, *Component Identifiers (Major Update)*:

> Definition: Identifiers used to identify a component or serve as a look-up
> key for relevant databases.
> The Component Identifiers element contains at least one software identifier
> associated with the target component. Machine-processable, unique software
> identifiers support automated analysis. This field should use common software
> identifiers, such as Common Platform Enumeration (CPE) and Package-URL (PURL).
> This field may also include universally unique identifiers (UUID),
> organization-specific identifiers, commit hashes, and intrinsic identifiers
> such as OmniBOR and Software Hash Identifier (SWHID). If there are multiple
> software identifiers, the SBOM author should include all of them.

Page 22, change note:

> Replace Other Unique Identifiers element with Component Identifiers element.
> Specify that at least one common software identifier should be included […]

Pages 13–14, *Explicitly Identifying Unknown Information (Major Update)*:

> If information required for any of the data fields is not provided, the SBOM
> author should explicitly state whether the information is unknown to the
> SBOM author or whether the SBOM author is withholding the information from
> the SBOM.

## ENISA, "SBOM Landscape Analysis — Towards an Implementation Guide"

Public Draft, v1.2, December 2025. Retrieved from
`https://www.enisa.europa.eu/sites/default/files/2025-12/SBOM%20Analysis%20-%20Towards%20an%20Implementation%20Guide_v1.20-Published.pdf`.
A landscape analysis, not a normative standard.

Section 2.2.2, Table 2 (printed page 16), *Baseline SBOM element requirements*:

> Software Identifiers — A consistent and unique ID (e.g., Package URL (PURL),
> SPDX identifier, SWHID etc).

mapped to NTIA's "Other Unique Identifiers" and CISA's "Software Identifiers".
The text after the table:

> Interoperability will be maintained through standardised format
> specifications and universal identifiers such as Package URLs (PURLs), Common
> Platform Enumeration (CPE) codes, and Software Heritage IDs.

## Reading

- Both require **at least one** identifier and name CPE as one example among
  several. Neither requires a CPE.
- Every component waybill emits carries a PURL (sbomnix comparison,
  2026-11-06: 0 waybill components without an identifier), so a Nix subject
  without a CPE still meets "at least one".
- A `waybill`-vendor CPE identifies no product and is a look-up key for no
  database, so it does not meet CISA's definition of an identifier in the
  first place. Omitting it removes nothing the guidance counts.
- "Include all of them" binds the other way: a real CPE, nixpkgs-declared or
  curated, must be emitted when one exists (FR-005, FR-006).
- The unknown-information practice applies when a required field is missing.
  Component Identifiers is still satisfied by the PURL, so no statement is
  owed for an absent CPE.
- The one case below "at least one" is an operator who passes `--no-root-purl`
  for a subject with no CPE. That is an explicit operator choice and existing
  behaviour, not introduced here.
