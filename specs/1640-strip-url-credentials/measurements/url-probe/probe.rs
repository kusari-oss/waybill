// Mirrors identifiers::sanitize::sanitize_userinfo's Owned path.
fn strip(u: &str) -> String {
    match url::Url::parse(u) {
        Ok(mut p) if !p.username().is_empty() || p.password().is_some() => {
            if p.set_username("").is_err() || p.set_password(None).is_err() { return u.into(); }
            p.to_string()
        }
        _ => u.into(),
    }
}
fn main() {
    for u in [
        "https://u:p@GitHub.com/Org/Repo.git",
        "https://u:p@github.com:443/org/repo.git",
        "https://u:p@github.com/a/../org/repo.git",
        "https://u:p@host/path%7Erepo with space.git",
        "git+https://u:p@GitHub.com/Org/Repo.git",
        "git://u:p@Host.Example/repo;protocol=https;branch=main",
        "ssh://git@Host.Example:22/org/repo.git",
        "https://u:p@host/x?b=2&a=1#frag",
    ] {
        println!("{u}\n  -> {}", strip(u));
    }
}
