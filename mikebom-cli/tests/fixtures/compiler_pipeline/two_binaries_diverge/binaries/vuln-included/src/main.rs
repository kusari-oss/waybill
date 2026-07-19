//! m210 SC-001 fixture — the vulnerable binary. Depends on
//! libsafe + libvuln. Its source-read-set MUST contain paths from
//! BOTH libsafe and libvuln.
fn main() {
    println!(
        "safe={} vuln={}",
        libsafe::safe_computation(),
        libvuln::vulnerable_computation(),
    );
}
