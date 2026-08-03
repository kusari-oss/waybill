# Milestone 225 US1 fixture — minimal Pants shell backend layout.
# Uses synthetic waybill-fixture-* script names per memory
# feedback_fixture_synthetic_package_names.

shell_source(
    name="deploy",
    source="waybill-fixture-deploy.sh",
)

# shell_sources glob demonstrating the same-file multi-owner case
# — waybill-fixture-deploy.sh is owned by BOTH `deploy` (explicit)
# AND `utils` (glob), which the reader's SC-006 dedup pass merges
# into ONE component with both addresses in waybill:pants-target.
shell_sources(
    name="utils",
    sources=["waybill-fixture-*.sh"],
)
