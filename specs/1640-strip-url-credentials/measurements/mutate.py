"""Spec 1640 T022 mutations (SC-005). Each: back up with cp, apply one exact
replacement, run its killing test, classify, restore with cp.

usage: python3 -I -B mutate.py REPO OUTDIR
"""
import pathlib
import shutil
import subprocess
import sys

REPO = pathlib.Path(sys.argv[1])
OUT = pathlib.Path(sys.argv[2])
SRC = REPO / "waybill-cli/src"
SZ = "waybill::identifiers::sanitize::"
B = "std::borrow::Cow::<str>::Borrowed"
IT = ["--test", "declared_url_credentials"]
BIN = ["--bin", "waybill"]

M = [
    ("M01 cmake fetch", "scan_fs/package_db/cmake.rs",
     f"v.map(|u| {SZ}sanitize_declared_url(&u, source_path).into_owned())", "v", IT + ["d01_"]),
    ("M02 cmake dataset", "scan_fs/package_db/cmake.rs",
     f"let url = {SZ}sanitize_declared_url(&d.url, &first);", "let url = d.url.clone();", IT + ["d02_"]),
    ("M03 bazel archive", "scan_fs/package_db/bazel.rs",
     f"let archive_url = extract_first_url(body)\n                .map(|u| {SZ}sanitize_declared_url(&u, &source_path).into_owned());",
     "let archive_url = extract_first_url(body);", IT + ["d04_"]),
    ("M04 bazel remote", "scan_fs/package_db/bazel.rs",
     f"let remote_url = extract_simple_string(body, \"remote\")\n                .map(|u| {SZ}sanitize_declared_url(&u, &source_path).into_owned());",
     "let remote_url = extract_simple_string(body, \"remote\");", IT + ["d04g_"]),
    ("M05 uv sources", "scan_fs/package_db/pip/uv_lock.rs",
     f"let declared = |u: &str| {SZ}sanitize_declared_url(u, source_path).into_owned();",
     "let declared = |u: &str| u.to_string();", IT + ["d06u_"]),
    ("M06 uv artifact (S3)", "scan_fs/package_db/pip/uv_lock.rs",
     f"let cleaned = {SZ}sanitize_declared_url(&raw, source_path);", f"let cleaned = {B}(&raw);",
     BIN + ["m800_private_index_credentials_are_stripped"]),
    ("M07 flake", "scan_fs/package_db/nix/mod.rs",
     f"let url = {SZ}sanitize_declared_url(url, &source_path);", f"let url = {B}(url);", IT + ["d07_"]),
    ("M08 closure source_url", "scan_fs/package_db/nix/closure/emit.rs",
     f"Some({SZ}sanitize_declared_url(raw, drv_label(drv)).into_owned())", "Some(raw.to_string())",
     BIN + ["s1640_a_credentialled_derivation_source_is_stripped"]),
    ("M09 closure vcs_source", "scan_fs/package_db/nix/closure/emit.rs",
     f"url: {SZ}sanitize_declared_url(url, drv_label(drv)).into_owned(),", "url: url.to_string(),",
     BIN + ["s1640_a_credentialled_derivation_source_is_stripped"]),
    ("M10 pants pex", "scan_fs/package_db/pants/lockfile.rs",
     f".map(|a| {SZ}sanitize_declared_url(&a.url, &lockfile_display).into_owned());", ".map(|a| a.url.clone());",
     IT + ["d08_"]),
    ("M11 pants coursier", "scan_fs/package_db/pants_jvm/lockfile.rs",
     f"let url = {SZ}sanitize_declared_url(url, &lockfile_path.display().to_string());", f"let url = {B}(url);",
     IT + ["d09_"]),
    ("M12 cocoapods", "scan_fs/package_db/cocoapods.rs",
     f"let url = {SZ}sanitize_declared_url(url, manifest);", f"let url = {B}(url);", IT + ["d12_"]),
    ("M13 dart", "scan_fs/package_db/dart.rs",
     f"let declared = |u: &str| {SZ}sanitize_declared_url(u, manifest).into_owned();",
     "let declared = |u: &str| u.to_string();", IT + ["d13g_"]),
    ("M14 composer", "scan_fs/package_db/composer.rs",
     f"let declared = |u: &str| {SZ}sanitize_declared_url(u, manifest).into_owned();",
     "let declared = |u: &str| u.to_string();", IT + ["d14_"]),
    ("M15 elixir lock", "scan_fs/package_db/elixir.rs",
     f"let url = {SZ}sanitize_declared_url(url, manifest);", f"let url = {B}(url);", IT + ["d15_"]),
    ("M16 elixir declared", "scan_fs/package_db/elixir.rs",
     f"let url = {SZ}sanitize_declared_url(url, &source_path);", f"let url = {B}(url);", IT + ["d15d_"]),
    ("M17 erlang lock", "scan_fs/package_db/erlang.rs",
     f"let url = {SZ}sanitize_declared_url(url, manifest);", f"let url = {B}(url);", IT + ["d16_"]),
    ("M18 erlang declared", "scan_fs/package_db/erlang.rs",
     f"let url = {SZ}sanitize_declared_url(url, &manifest);", f"let url = {B}(url);", IT + ["d16d_"]),
    ("M19 pip direct url", "scan_fs/package_db/pip/requirements_txt.rs",
     f"let url = {SZ}sanitize_declared_url(&direct.url, source_path);", f"let url = {B}(&direct.url);",
     IT + ["d17_"]),
    ("M20 cargo-auditable", "scan_fs/binary/entry.rs",
     f"let source = {SZ}sanitize_declared_url(&pkg.source, &path.display().to_string()).into_owned();",
     "let source = pkg.source.clone();", BIN + ["s1640_a_credentialled_git_source_is_stripped"]),
    ("M21 yocto", "scan_fs/package_db/yocto/recipe.rs",
     f"*uri = {SZ}sanitize_declared_url(uri, &manifest).into_owned();", "let _ = &manifest;", IT + ["d19_"]),
    ("M22 swift", "scan_fs/package_db/swift/lockfile.rs",
     f"let location = {SZ}sanitize_declared_url(&entry.location, &path.display().to_string()).into_owned();",
     "let location = entry.location.clone();", IT + ["d22_"]),
    ("M23 requirement ranges", "scan_fs/mod.rs",
     f".map(|r| {SZ}sanitize_declared_text(r, &entry.source_path).into_owned())", ".map(|r| r.clone())",
     IT + ["d20_"]),
    ("M24 pyproject constraint", "scan_fs/package_db/pip/mod.rs",
     f"let constraint = {SZ}sanitize_declared_text(\n                &take_constraint(raw_dep),\n                &manifest_path.display().to_string(),\n            )\n            .into_owned();",
     "let constraint = take_constraint(raw_dep);", IT + ["d21_"]),
    ("M25 splice reverted", "identifiers/sanitize.rs",
     "if let Some(spliced) = without_userinfo_span(url) {", "if let Some(spliced) = None::<String> {",
     IT + ["i3_"]),
    ("M26 ssh exception removed", "identifiers/sanitize.rs",
     "if matches!(out, Cow::Owned(_)) && !is_ssh_git_user(raw) {", "if matches!(out, Cow::Owned(_)) {",
     IT + ["s1_"]),
    ("M27 warning removed", "identifiers/sanitize.rs",
     "            \"stripped credentials from URL\"\n        );", "            \"credentials\"\n        );",
     IT + ["d05_"]),
    ("M28 text sanitiser inert", "identifiers/sanitize.rs",
     "let clean = sanitize_declared_url(&rest[start..end], manifest);",
     "let clean = Cow::Borrowed(&rest[start..end]);", IT + ["d21_"]),
]


def run(mid, rel, old, new, test):
    path = SRC / rel
    text = path.read_text()
    if text.count(old) != 1:
        return f"{mid}: NOT APPLIED (anchor count {text.count(old)})"
    backup = OUT / (mid.split()[0] + ".orig")
    shutil.copyfile(path, backup)
    try:
        path.write_text(text.replace(old, new))
        r = subprocess.run(["cargo", "test", "-p", "waybill", *test], cwd=REPO, capture_output=True, text=True)
        log = r.stdout + r.stderr
        (OUT / (mid.split()[0] + ".log")).write_text(log)
        if any(line.startswith("error[E") for line in log.splitlines()):
            return f"{mid}: COMPILE ERROR (not a kill)"
        failed = [l for l in log.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
        ran = [l for l in log.splitlines() if l.startswith("test result:")]
        if r.returncode != 0 and failed:
            return f"{mid}: KILLED by {', '.join(f.split()[1] for f in failed)}"
        return f"{mid}: SURVIVED ({ran[-1] if ran else 'no test result line'})"
    finally:
        shutil.copyfile(backup, path)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    results = [run(*m) for m in M]
    (OUT / "results.txt").write_text("\n".join(results) + "\n")
    print("\n".join(results))


if __name__ == "__main__":
    main()
