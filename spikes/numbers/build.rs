//! With the `v1` feature, copies v1's number modules (`bigint.rs`, `bigfloat.rs`, `interrupt.rs`) from the v1
//! repository into OUT_DIR so the crate can compile them as its own modules. The v1 files are only read. Their
//! inner doc comments (`//!`) are dropped because `include!` does not accept inner attributes.
//!
//! The v1 source directory is `MATHCAS_V1_SRC` if set, else the first `MathCAS/crates/kernel/src` found in an
//! ancestor of this crate (checkouts can sit at different depths).
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=MATHCAS_V1_SRC");
    if std::env::var_os("CARGO_FEATURE_V1").is_none() {
        return;
    }
    let src = match std::env::var_os("MATHCAS_V1_SRC") {
        Some(p) => PathBuf::from(p),
        None => {
            let here = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
            here.ancestors()
                .map(|a| a.join("MathCAS").join("crates").join("kernel").join("src"))
                .find(|p| p.join("bigint.rs").is_file())
                .expect("feature v1: v1 sources not found; set MATHCAS_V1_SRC to MathCAS/crates/kernel/src")
        }
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for name in ["bigint.rs", "bigfloat.rs", "interrupt.rs"] {
        let path = src.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let body: String = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("//!"))
            .map(|l| format!("{l}\n"))
            .collect();
        std::fs::write(out.join(format!("v1_{name}")), body).unwrap();
    }
}
