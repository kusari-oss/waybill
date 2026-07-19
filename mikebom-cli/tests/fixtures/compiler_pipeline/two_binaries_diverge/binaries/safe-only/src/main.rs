//! m210 SC-001 fixture — the safe binary. Depends ONLY on libsafe.
//! Its source-read-set MUST NOT contain any libvuln source path.
fn main() {
    println!("{}", libsafe::safe_computation());
}
