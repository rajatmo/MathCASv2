//! The licence check and the notices on the real workspace, for the platform under test: CI fails when a shipped
//! dependency's licence is not on the allowed list.
use mc_bench::notices::{self, RenderOptions};
use std::ffi::OsString;
use std::path::Path;

fn workspace_shipped() -> notices::Shipped {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    // The host's own target: every package it needs is already in the local registry after the build.
    let target = host_target();
    let metadata = notices::cargo_metadata(&cargo, &manifest, &target, false).expect("cargo metadata");
    notices::shipped(&metadata, notices::SHIPPED_BINARIES).expect("dependency graph")
}

fn host_target() -> String {
    let cargo = std::env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let out = std::process::Command::new(cargo).arg("-vV").output().expect("rustc -vV");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines().find_map(|l| l.strip_prefix("host: ")).expect("host line").trim().to_string()
}

#[test]
fn every_shipped_dependency_has_an_allowed_licence() {
    let shipped = workspace_shipped();
    assert!(shipped.dependencies.len() > 100, "{} dependencies", shipped.dependencies.len());
    assert!(shipped.members.iter().any(|m| m.name == "mc-repl"));
    let problems = notices::violations(&shipped.dependencies);
    assert!(problems.is_empty(), "licence check failed:\n{}", problems.join("\n"));
    // Each exception is still needed: when its crate leaves the graph, the exception goes too.
    for e in notices::EXCEPTIONS {
        assert!(
            shipped.dependencies.iter().any(|p| p.name == e.krate),
            "exception for {} is no longer needed: remove it",
            e.krate
        );
    }
}

#[test]
fn notices_render_for_the_workspace() {
    let shipped = workspace_shipped();
    let text =
        notices::render(&shipped, &RenderOptions { version: "test".into(), target: host_target() }).expect("render");
    for needle in
        ["1. Components", "Apache License", "Fonts compiled into mc-ui", "Fonts compiled into mc-mathfont", "LAB.icc"]
    {
        assert!(text.contains(needle), "notices lack '{needle}'");
    }
    // Every crate appears in the table and among the licence texts.
    for p in &shipped.dependencies {
        assert!(text.contains(&format!("  {} ", p.label())), "{} missing from the notices", p.label());
    }
}

/// hayro-interpret embeds `assets/LAB.icc` without a licence statement of its own. The same author ships the
/// byte-identical profile in hayro-jpeg2000, whose assets README puts all its ICC profiles under CC0 1.0
/// (packaging/licenses/embedded-data.txt). If either crate changes the file, this fails: check the licence again.
#[test]
fn the_embedded_lab_profile_is_the_cc0_one() {
    let shipped = workspace_shipped();
    let dir = |name: &str| {
        shipped
            .dependencies
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.dir.clone())
            .unwrap_or_else(|| panic!("{name} not shipped"))
    };
    let embedded = std::fs::read(dir("hayro-interpret").join("assets/LAB.icc")).expect("hayro-interpret LAB.icc");
    let licensed_dir = dir("hayro-jpeg2000").join("assets");
    let licensed = std::fs::read(licensed_dir.join("LAB.icc")).expect("hayro-jpeg2000 LAB.icc");
    assert_eq!(embedded, licensed, "the two LAB.icc files differ: review the licence of hayro-interpret's copy");
    let readme = std::fs::read_to_string(licensed_dir.join("README.md")).expect("assets README");
    assert!(readme.contains("CC0 1.0"), "hayro-jpeg2000's assets README no longer states CC0 1.0");
    assert_eq!(embedded.len(), 484);
    // The profile's own copyright tag (UTF-16BE in an `mluc` record) says it is free to use.
    let tag: Vec<u8> = "No copyright, use freely".encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
    assert!(embedded.windows(tag.len()).any(|w| w == tag.as_slice()));
}
