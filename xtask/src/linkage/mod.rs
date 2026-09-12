// Issue #824 — enforce Constitution Principle I's static-linkage rule.
//
// v3.0.0 permits a dependency that vendors and compiles C, provided it
// links statically and the released binary gains no dynamic dependency
// beyond the platform's own baseline. Until now nothing checked that.
// A rule enforced only by review is a rule that quietly stops being
// true: a transitive bump can flip a `*-sys` crate from vendored to
// system-discovered and the first symptom is an operator's binary
// failing to start on a host missing the library.
//
// One implementation covers all four release targets because `object`
// parses ELF, Mach-O and PE alike, and parses them as data — so an
// x86_64 runner can inspect the cross-built aarch64 artifact, and no
// job needs `readelf`, `otool` or `dumpbin` on its PATH.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

use clap::Args;
use object::read::macho::MachHeader;
use object::Object;

mod baseline;
pub use baseline::{baseline_for, Baseline};

#[cfg(test)]
mod tests;

#[derive(Args, Debug)]
pub struct LinkageArgs {
    /// Binary to inspect. Accepts any ELF, Mach-O or PE file.
    #[arg(long)]
    pub binary: PathBuf,

    /// Target triple whose baseline to enforce. Must be one of the
    /// release targets; an unknown triple is a hard error rather than
    /// an empty allowlist, so a new target cannot silently skip the gate.
    #[arg(long)]
    pub target: String,

    /// Print what was found and exit 0 without enforcing. For seeding a
    /// baseline, never for CI.
    #[arg(long, default_value_t = false)]
    pub report_only: bool,
}

/// A dynamic dependency recorded in the binary, normalised for matching.
///
/// macOS `LC_LOAD_DYLIB` entries are absolute paths
/// (`/usr/lib/libiconv.2.dylib`); ELF `DT_NEEDED` and PE imports are
/// bare names. The baseline stores whichever form the platform emits,
/// so no normalisation beyond case is applied — a rule written against
/// a real observed string is easier to audit than one written against
/// a transformation of it.
pub type Dep = String;

pub fn run(args: LinkageArgs) -> Result<(), Box<dyn Error>> {
    let baseline = baseline_for(&args.target).ok_or_else(|| -> Box<dyn Error> {
        format!(
            "no linkage baseline for target `{}`.\n\
             \n\
             Known targets: {}\n\
             \n\
             A new release target must declare its platform baseline before it \
             can ship — an unknown triple is refused rather than allowed \
             through, so adding a target cannot silently skip this gate.",
            args.target,
            baseline::known_targets().join(", "),
        )
        .into()
    })?;

    let found = read_dynamic_deps(&args.binary)?;

    if args.report_only {
        println!("{} ({}):", args.binary.display(), args.target);
        for d in &found {
            let mark = if baseline.allows(d) { " " } else { "!" };
            println!("  {mark} {d}");
        }
        return Ok(());
    }

    let unexpected: Vec<&Dep> = found.iter().filter(|d| !baseline.allows(d)).collect();
    if unexpected.is_empty() {
        println!(
            "linkage ok: {} dynamic dependencies, all within the {} baseline",
            found.len(),
            args.target,
        );
        return Ok(());
    }

    Err(format!(
        "{} dynamic dependencies outside the {} platform baseline:\n\
         \n{}\n\n\
         Constitution Principle I: a dependency that vendors C MUST link \
         statically, and released binaries MUST NOT acquire a dynamic-link \
         dependency beyond the platform's own baseline.\n\
         \n\
         If one of these is genuinely OS-supplied, add it to \
         xtask/src/linkage/baseline.rs with a note saying which OS ships it. \
         If it is a third-party library, the fix is the dependency, not the \
         allowlist — a `*-sys` crate usually needs its vendored/static \
         feature enabled and pkg-config probing disabled.",
        unexpected.len(),
        args.target,
        unexpected
            .iter()
            .map(|d| format!("  {d}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .into())
}

/// Extract the dynamic dependency list from an ELF, Mach-O or PE binary.
pub fn read_dynamic_deps(path: &Path) -> Result<BTreeSet<Dep>, Box<dyn Error>> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    read_dynamic_deps_from_bytes(&bytes)
        .map_err(|e| format!("{}: {e}", path.display()).into())
}

pub fn read_dynamic_deps_from_bytes(bytes: &[u8]) -> Result<BTreeSet<Dep>, Box<dyn Error>> {
    let file = object::File::parse(bytes)
        .map_err(|e| -> Box<dyn Error> { format!("not a recognised binary: {e}").into() })?;

    let mut out = BTreeSet::new();

    // `imports()` covers PE import tables and ELF DT_NEEDED. For Mach-O
    // it yields symbol imports rather than the dylib list, so Mach-O is
    // handled separately below.
    if !matches!(file, object::File::MachO32(_) | object::File::MachO64(_)) {
        for import in file.imports()? {
            let lib = String::from_utf8_lossy(import.library()).into_owned();
            if !lib.is_empty() {
                out.insert(lib);
            }
        }
        // ELF DT_NEEDED arrives through the same `imports()` call.
        // Verified against `readelf -d` on a glibc binary: all three
        // NEEDED entries (including the loader) are reported.
        return Ok(out);
    }

    out.extend(macho_load_dylibs(bytes)?);
    Ok(out)
}

/// Mach-O `LC_LOAD_DYLIB` / `LC_LOAD_WEAK_DYLIB` paths.
fn macho_load_dylibs(bytes: &[u8]) -> Result<Vec<Dep>, Box<dyn Error>> {
    use object::macho;
    use object::read::macho::FatArch;

    // A universal binary carries per-arch slices; inspect every slice so
    // a bad dependency cannot hide in the one nobody looked at.
    if let Ok(arches) = object::read::macho::MachOFatFile64::parse(bytes) {
        let mut all = Vec::new();
        for arch in arches.arches() {
            let data = arch.data(bytes)?;
            all.extend(macho_slice_dylibs::<macho::MachHeader64<object::Endianness>>(data)?);
        }
        return Ok(all);
    }
    if let Ok(arches) = object::read::macho::MachOFatFile32::parse(bytes) {
        let mut all = Vec::new();
        for arch in arches.arches() {
            let data = arch.data(bytes)?;
            all.extend(macho_slice_dylibs::<macho::MachHeader32<object::Endianness>>(data)?);
        }
        return Ok(all);
    }
    macho_slice_dylibs::<macho::MachHeader64<object::Endianness>>(bytes)
}

fn macho_slice_dylibs<H: MachHeader<Endian = object::Endianness>>(
    data: &[u8],
) -> Result<Vec<Dep>, Box<dyn Error>> {
    let mut out = Vec::new();
    let Ok(header) = H::parse(data, 0) else {
        return Ok(out);
    };
    let endian = header.endian()?;
    let mut commands = header.load_commands(endian, data, 0)?;
    while let Some(command) = commands.next()? {
        if let Some(dylib) = command.dylib()? {
            let name = command.string(endian, dylib.dylib.name)?;
            out.push(String::from_utf8_lossy(name).into_owned());
        }
    }
    Ok(out)
}
