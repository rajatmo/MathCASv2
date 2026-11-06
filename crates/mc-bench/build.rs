//! Records the compiler version for the machine description in results files (02-benchmarks section 1), and the
//! target triple for mc-notices.
use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=MC_BENCH_RUSTC={version}");
    // The default target of mc-notices: the platform this build is for.
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".into());
    println!("cargo:rustc-env=MC_BENCH_TARGET={target}");
    println!("cargo:rerun-if-env-changed=RUSTC");
}
