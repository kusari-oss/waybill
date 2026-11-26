//! Prints one temporary-file name per process.
//! No argument: create a NamedTempFile and drop it, as the tests do.
//! `--fixed-seed`: build a name the way tempfile does, from a constant seed.
//! That is the control: every process must collide.
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--fixed-seed") {
        let mut rng = fastrand::Rng::with_seed(42);
        let name: String = std::iter::repeat_with(|| rng.alphanumeric()).take(6).collect();
        println!(".tmp{name}");
        return;
    }
    let f = tempfile::NamedTempFile::new().expect("tempfile");
    println!("{}", f.path().display());
}
