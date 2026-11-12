//! `mc-notices`: write THIRD-PARTY-NOTICES.txt for the shipped programs and check their dependencies' licences.
//!
//! ```text
//! mc-notices [--out FILE] [--target TRIPLE] [--manifest-path Cargo.toml] [--bins a,b,c] [--offline] [--check]
//! ```
//!
//! Exit status 0 when every licence is acceptable (and the notices were written, unless `--check`), 1 on a
//! licence violation (listed on standard error, nothing written), 2 on a usage or I/O error.
use mc_bench::notices::{self, RenderOptions};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

struct Options {
    out: Option<PathBuf>,
    target: String,
    manifest: PathBuf,
    bins: Vec<String>,
    offline: bool,
    check_only: bool,
}

const USAGE: &str = "usage: mc-notices [--out FILE] [--target TRIPLE] [--manifest-path Cargo.toml] [--bins a,b,c] \
                     [--offline] [--check]";

fn parse(args: Vec<String>) -> Result<Options, String> {
    let mut o = Options {
        out: None,
        target: env!("MC_BENCH_TARGET").to_string(),
        manifest: PathBuf::from("Cargo.toml"),
        bins: notices::SHIPPED_BINARIES.iter().map(|s| s.to_string()).collect(),
        offline: false,
        check_only: false,
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} needs a value\n{USAGE}"));
        match a.as_str() {
            "--out" => o.out = Some(PathBuf::from(value()?)),
            "--target" => o.target = value()?,
            "--manifest-path" => o.manifest = PathBuf::from(value()?),
            "--bins" => o.bins = value()?.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect(),
            "--offline" => o.offline = true,
            "--check" => o.check_only = true,
            "-h" | "--help" => return Err(USAGE.to_string()),
            _ => return Err(format!("unknown argument '{a}'\n{USAGE}")),
        }
    }
    Ok(o)
}

fn run(o: &Options) -> Result<bool, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let metadata = notices::cargo_metadata(&cargo, &o.manifest, &o.target, o.offline)?;
    let bins: Vec<&str> = o.bins.iter().map(String::as_str).collect();
    let shipped = notices::shipped(&metadata, &bins)?;
    let problems = notices::violations(&shipped.dependencies);
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("licence: {p}");
        }
        eprintln!("{} of {} crates fail the licence check", problems.len(), shipped.dependencies.len());
        return Ok(false);
    }
    eprintln!("licences: {} crates for {}, all acceptable", shipped.dependencies.len(), o.target);
    if o.check_only {
        return Ok(true);
    }
    let version = shipped.members.iter().find(|m| m.name == "mc-repl").or(shipped.members.first());
    let version = version.map_or_else(|| "dev".to_string(), |m| m.version.clone());
    let text = notices::render(&shipped, &RenderOptions { version, target: o.target.clone() })?;
    match &o.out {
        Some(path) => {
            std::fs::write(path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
            eprintln!("wrote {} ({} KB)", path.display(), text.len() / 1024);
        }
        None => print!("{text}"),
    }
    Ok(true)
}

fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1).collect()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match run(&options) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("mc-notices: {e}");
            ExitCode::from(2)
        }
    }
}
