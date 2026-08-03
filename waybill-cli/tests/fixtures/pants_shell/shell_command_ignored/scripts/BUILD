# FR-012 regression fixture: shell_command targets are legal Pants
# syntax but out-of-scope for the milestone-225 reader (they describe
# actions, not artifacts). This BUILD file exercises both target types
# in one place; the reader must emit ONE component (the wrapper) and
# ZERO components attributable to the shell_command.

shell_command(
    name="build",
    command="make all",
    tools=["make"],
    outputs=["target/binary"],
)

shell_source(
    name="wrapper",
    source="waybill-fixture-wrapper.sh",
)
