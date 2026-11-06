//! Third-party notices for the shipped programs: which crates go into the binaries, under which licences, and the
//! licence texts they carry, collected from `cargo metadata` and the local cargo registry.
//!
//! - [`shipped`] walks the resolved dependency graph from the packages that build the shipped binaries, following
//!   normal dependencies only (no dev- or build-dependencies: their code does not end up in the programs);
//! - [`violations`] checks every crate's licence expression against [`spdx::ALLOWED`] (plus the per-crate
//!   [`EXCEPTIONS`]);
//! - [`render`] writes `THIRD-PARTY-NOTICES.txt`: a table of the crates, then each distinct licence text once with
//!   the crates that carry it, then the bundled fonts and notes on embedded data.
//!
//! Licence files are the package's own (`LICENSE*`, `COPYING*`, `NOTICE*`, ... at its root, its `license-file`, and
//! licence files in its asset folders; every `.txt` in a `fonts` folder). A crate published without any licence file
//! at its root gets the standard text of the licence chosen from its expression, from `packaging/licenses/`.
pub mod json;
pub mod spdx;

use json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The programs the distributions ship.
pub const SHIPPED_BINARIES: &[&str] =
    &["mc-repl", "mc-kernel", "mc-lsp", "mc-notebook", "mc-docs", "mc-docs-pages", "mc-bench"];

/// A licence accepted for one crate only, outside [`spdx::ALLOWED`], with the reason.
#[derive(Clone, Copy, Debug)]
pub struct Exception {
    pub krate: &'static str,
    pub licence: &'static str,
    pub reason: &'static str,
}

/// The accepted exceptions. Each is scoped to one crate, so a new crate under the same licence still fails the check.
pub const EXCEPTIONS: &[Exception] = &[Exception {
    krate: "epaint_default_fonts",
    licence: "Ubuntu-font-1.0",
    reason: "the user-interface library's built-in Ubuntu-Light font, compiled in through eframe's default \
             `default_fonts` feature; a free font licence (redistribution and embedding allowed). The windows use \
             their own bundled fonts, so the feature can be turned off in the crates that depend on eframe, which \
             removes this crate",
}];

/// One package of the dependency graph.
#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    pub version: String,
    /// The SPDX expression from the manifest, if any.
    pub license: Option<String>,
    /// The manifest's `license-file`, relative to the package folder.
    pub license_file: Option<String>,
    pub authors: Vec<String>,
    pub repository: Option<String>,
    /// The package folder (in the cargo registry for dependencies).
    pub dir: PathBuf,
}

impl Package {
    /// `name version`.
    pub fn label(&self) -> String {
        format!("{} {}", self.name, self.version)
    }

    fn from_json(v: &Value) -> Result<Package, String> {
        let field = |k: &str| v.str_at(k).map(str::to_string);
        let manifest = field("manifest_path").ok_or("package without manifest_path")?;
        Ok(Package {
            name: field("name").ok_or("package without name")?,
            version: field("version").ok_or("package without version")?,
            license: field("license"),
            license_file: field("license_file"),
            authors: v
                .get("authors")
                .map_or(&[][..], Value::items)
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            repository: field("repository"),
            dir: Path::new(&manifest).parent().map(Path::to_path_buf).unwrap_or_default(),
        })
    }
}

/// What the shipped binaries are built from.
#[derive(Clone, Debug)]
pub struct Shipped {
    pub workspace_root: PathBuf,
    /// The workspace's own packages that are reached (licensed under the workspace licence).
    pub members: Vec<Package>,
    /// Third-party packages, sorted by name and version.
    pub dependencies: Vec<Package>,
}

/// Run `cargo metadata` for the workspace of `manifest`, resolved for `target`.
pub fn cargo_metadata(cargo: &OsStr, manifest: &Path, target: &str, offline: bool) -> Result<Value, String> {
    let mut cmd = Command::new(cargo);
    cmd.args(["metadata", "--format-version", "1", "--filter-platform", target, "--manifest-path"]).arg(manifest);
    if offline {
        cmd.arg("--offline");
    }
    let out = cmd.output().map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let text = String::from_utf8(out.stdout).map_err(|_| "cargo metadata printed non-UTF-8 output".to_string())?;
    json::parse(&text).map_err(|e| e.to_string())
}

/// The packages reached from the workspace packages that build `binaries`, through normal dependencies.
pub fn shipped(metadata: &Value, binaries: &[&str]) -> Result<Shipped, String> {
    let workspace_root = PathBuf::from(metadata.str_at("workspace_root").ok_or("metadata without workspace_root")?);
    let members: BTreeSet<&str> =
        metadata.get("workspace_members").map_or(&[][..], Value::items).iter().filter_map(Value::as_str).collect();
    let packages: HashMap<&str, &Value> = metadata
        .get("packages")
        .map_or(&[][..], Value::items)
        .iter()
        .filter_map(|p| Some((p.str_at("id")?, p)))
        .collect();

    let mut roots = Vec::new();
    for bin in binaries {
        let owner = members.iter().find(|id| {
            packages.get(*id).is_some_and(|p| {
                p.get("targets").map_or(&[][..], Value::items).iter().any(|t| {
                    t.str_at("name") == Some(bin)
                        && t.get("kind").is_some_and(|k| k.items().iter().any(|k| k.as_str() == Some("bin")))
                })
            })
        });
        roots.push(*owner.ok_or_else(|| format!("no workspace package builds the binary '{bin}'"))?);
    }

    let nodes: HashMap<&str, &Value> = metadata
        .get("resolve")
        .and_then(|r| r.get("nodes"))
        .ok_or("metadata without a resolve graph")?
        .items()
        .iter()
        .filter_map(|n| Some((n.str_at("id")?, n)))
        .collect();
    let mut seen = BTreeSet::new();
    let mut stack = roots;
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let node = nodes.get(id).ok_or_else(|| format!("package {id} missing from the resolve graph"))?;
        for dep in node.get("deps").map_or(&[][..], Value::items) {
            // `kind` is null for normal dependencies, "dev" or "build" otherwise.
            let normal =
                dep.get("dep_kinds").map_or(&[][..], Value::items).iter().any(|k| k.get("kind") == Some(&Value::Null));
            if let (true, Some(pkg)) = (normal, dep.str_at("pkg")) {
                stack.push(pkg);
            }
        }
    }

    let mut shipped = Shipped { workspace_root, members: Vec::new(), dependencies: Vec::new() };
    for id in seen {
        let package = Package::from_json(packages.get(id).ok_or_else(|| format!("package {id} missing"))?)?;
        if members.contains(id) {
            shipped.members.push(package);
        } else {
            shipped.dependencies.push(package);
        }
    }
    let key = |p: &Package| (p.name.clone(), p.version.clone());
    shipped.members.sort_by_key(key);
    shipped.dependencies.sort_by_key(key);
    Ok(shipped)
}

/// Whether `licence` is acceptable for the crate `krate`.
pub fn accepted(krate: &str, licence: &str) -> bool {
    spdx::is_allowed(licence) || EXCEPTIONS.iter().any(|e| e.krate == krate && e.licence == licence)
}

/// One message per dependency whose licence is missing, malformed or not acceptable.
pub fn violations(dependencies: &[Package]) -> Vec<String> {
    let mut out = Vec::new();
    for p in dependencies {
        let Some(expr) = &p.license else {
            out.push(format!("{}: no licence expression in the manifest (review its licence file by hand)", p.label()));
            continue;
        };
        match spdx::parse(expr) {
            Err(e) => out.push(format!("{}: {e}", p.label())),
            Ok(e) if !e.satisfiable(&|l| accepted(&p.name, l)) => {
                out.push(format!("{}: licence '{expr}' is not on the allowed list", p.label()))
            }
            Ok(_) => {}
        }
    }
    out
}

/// A licence file of a package: its path relative to the package folder, and its text.
#[derive(Clone, Debug)]
pub struct LicenceFile {
    pub path: String,
    pub text: String,
}

/// Folders of a package never searched for licence files (source and tests).
const SKIPPED_FOLDERS: &[&str] = &["src", "tests", "benches", "examples", "target", ".git", ".github"];

fn is_licence_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let code = [".RS", ".TOML", ".JSON", ".HTML", ".PY", ".SH"].iter().any(|ext| upper.ends_with(ext));
    !code
        && (upper.contains("LICEN")
            || ["COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE", "OFL", "UFL"].iter().any(|p| upper.starts_with(p)))
}

/// Normalise a licence text for comparison and output: LF line ends, no trailing spaces, no blank edges.
fn normalise(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    lines.join("\n").trim_matches('\n').to_string()
}

fn sorted_entries(dir: &Path) -> Vec<(String, PathBuf, bool)> {
    let mut entries: Vec<(String, PathBuf, bool)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path(), e.file_type().is_ok_and(|t| t.is_dir())))
        .collect();
    entries.sort();
    entries
}

/// The licence files of `package`: at its root and its `license-file`, then in its first-level folders.
pub fn licence_files(package: &Package) -> Vec<LicenceFile> {
    let mut found: BTreeMap<String, String> = BTreeMap::new();
    let mut add = |rel: String, path: &Path| {
        if let Ok(bytes) = fs::read(path) {
            found.entry(rel).or_insert_with(|| normalise(&String::from_utf8_lossy(&bytes)));
        }
    };
    for (name, path, is_dir) in sorted_entries(&package.dir) {
        if !is_dir && is_licence_name(&name) {
            add(name, &path);
        } else if is_dir && !SKIPPED_FOLDERS.contains(&name.as_str()) {
            let fonts = name.eq_ignore_ascii_case("fonts");
            for (inner, inner_path, inner_dir) in sorted_entries(&path) {
                let text = inner.to_ascii_lowercase().ends_with(".txt");
                if !inner_dir && (is_licence_name(&inner) || (fonts && text)) {
                    add(format!("{name}/{inner}"), &inner_path);
                }
            }
        }
    }
    if let Some(file) = &package.license_file {
        add(file.replace('\\', "/"), &package.dir.join(file));
    }
    found.into_iter().map(|(path, text)| LicenceFile { path, text }).collect()
}

/// Whether a package has a licence file of its own (at its root or named by `license-file`), not only in asset
/// folders.
fn has_root_licence(files: &[LicenceFile]) -> bool {
    files.iter().any(|f| !f.path.contains('/'))
}

/// The file name of a standard licence text in `packaging/licenses/`.
pub fn template_name(licence: &str) -> String {
    format!("{}.txt", licence.replace(' ', "-"))
}

/// What [`render`] needs besides the graph.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// The product version for the heading.
    pub version: String,
    /// The target triple the graph was resolved for.
    pub target: String,
}

const RULE: &str = "--------------------------------------------------------------------------------";

/// Write the notices. Fails when a crate has no licence text at all (neither its own files nor a standard text in
/// `packaging/licenses/`), or when a bundled font folder has no licence text.
pub fn render(shipped: &Shipped, options: &RenderOptions) -> Result<String, String> {
    let templates = shipped.workspace_root.join("packaging").join("licenses");
    // text -> the crates (with the file they carry it in) that use it
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut table = Vec::new();
    for p in &shipped.dependencies {
        let expr = p.license.clone().unwrap_or_else(|| "see licence file".into());
        table.push((p.name.clone(), p.version.clone(), expr.clone()));
        let files = licence_files(p);
        for f in &files {
            groups.entry(f.text.clone()).or_default().push(format!("{} ({})", p.label(), f.path));
        }
        if has_root_licence(&files) {
            continue;
        }
        let parsed = spdx::parse(&expr).map_err(|e| format!("{}: {e}", p.label()))?;
        let chosen = parsed
            .choose(&|l| accepted(&p.name, l))
            .ok_or_else(|| format!("{}: licence '{expr}' is not on the allowed list", p.label()))?;
        for licence in chosen {
            let path = templates.join(template_name(&licence));
            match fs::read_to_string(&path) {
                Ok(text) => {
                    let who =
                        if p.authors.is_empty() { format!("the {} authors", p.name) } else { p.authors.join(", ") };
                    groups.entry(normalise(&text)).or_default().push(format!(
                        "{} (no licence file in the package; standard {licence} text; copyright {who})",
                        p.label()
                    ));
                }
                // Asset licences (fonts) may be the only text for parts of the expression; code licences may not.
                Err(_) if licence_in_assets(&licence, &files) => {}
                Err(_) => {
                    return Err(format!(
                        "{}: no licence file in the package and no standard text {} for '{licence}'",
                        p.label(),
                        template_name(&licence)
                    ))
                }
            }
        }
    }

    let mut out = String::new();
    let line = |out: &mut String, s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    line(&mut out, &format!("MathCAS v2 {}: third-party notices", options.version));
    line(&mut out, &"=".repeat(40));
    line(&mut out, "");
    line(&mut out, "MathCAS v2 is free software under the GNU General Public License, version 3 or (at your option)");
    line(&mut out, "any later version (LICENSE.txt). Its programs are built with the third-party components listed");
    line(&mut out, "below, each under its own licence, whose texts follow. Where a component offers a choice of");
    line(&mut out, "licences, MathCAS uses it under any of them.");
    line(&mut out, "");
    line(
        &mut out,
        &format!("Built for {}. Generated by mc-notices from the dependency graph of Cargo.lock.", options.target),
    );
    line(&mut out, "");
    line(&mut out, &format!("1. Components ({} crates)", table.len()));
    line(&mut out, "");
    let width = table.iter().map(|(n, v, _)| n.len() + v.len() + 1).max().unwrap_or(0);
    for (name, version, expr) in &table {
        line(&mut out, &format!("  {:<width$}  {expr}", format!("{name} {version}")));
    }

    let mut ordered: Vec<(String, Vec<String>)> = groups.into_iter().collect();
    ordered.sort_by(|a, b| a.1.first().cmp(&b.1.first()));
    line(&mut out, "");
    line(&mut out, &format!("2. Licence texts ({} distinct)", ordered.len()));
    for (i, (text, users)) in ordered.iter().enumerate() {
        line(&mut out, "");
        line(&mut out, RULE);
        line(&mut out, &format!("2.{} Used by:", i + 1));
        for u in users {
            line(&mut out, &format!("  {u}"));
        }
        line(&mut out, RULE);
        line(&mut out, "");
        line(&mut out, text);
    }

    line(&mut out, "");
    line(&mut out, "3. Bundled fonts");
    for member in &shipped.members {
        let dir = member.dir.join("fonts");
        if !dir.is_dir() {
            continue;
        }
        let entries = sorted_entries(&dir);
        let fonts: Vec<&str> = entries
            .iter()
            .filter(|(n, _, d)| !d && [".ttf", ".otf"].iter().any(|x| n.to_ascii_lowercase().ends_with(x)))
            .map(|(n, _, _)| n.as_str())
            .collect();
        let texts: Vec<&(String, PathBuf, bool)> =
            entries.iter().filter(|(n, _, d)| !d && n.to_ascii_lowercase().ends_with(".txt")).collect();
        if fonts.is_empty() {
            continue;
        }
        if texts.is_empty() {
            return Err(format!("{}: fonts without a licence text in {}", member.name, dir.display()));
        }
        line(&mut out, "");
        line(&mut out, RULE);
        line(&mut out, &format!("Fonts compiled into {}: {}", member.name, fonts.join(", ")));
        line(&mut out, RULE);
        for (name, path, _) in texts {
            let text = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            line(&mut out, "");
            line(&mut out, &format!("[{}/fonts/{name}]", member.name));
            line(&mut out, "");
            line(&mut out, &normalise(&String::from_utf8_lossy(&text)));
        }
    }

    let notes = templates.join("embedded-data.txt");
    if let Ok(text) = fs::read_to_string(&notes) {
        line(&mut out, "");
        line(&mut out, "4. Notes on embedded data");
        line(&mut out, "");
        line(&mut out, &normalise(&text));
    }
    Ok(out)
}

/// Whether a font licence such as `OFL-1.1` or `Ubuntu-font-1.0` is carried by the package's asset files (their
/// texts do not contain the SPDX identifier itself).
fn licence_in_assets(licence: &str, files: &[LicenceFile]) -> bool {
    let needle = match licence {
        "OFL-1.1" => "SIL OPEN FONT LICENSE",
        "Ubuntu-font-1.0" => "UBUNTU FONT LICENCE",
        _ => return false,
    };
    files.iter().any(|f| f.path.contains('/') && f.text.to_ascii_uppercase().contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mc-notices-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn package(name: &str, license: Option<&str>, dir: &Path) -> Package {
        Package {
            name: name.into(),
            version: "1.0.0".into(),
            license: license.map(str::to_string),
            license_file: None,
            authors: vec!["A. Author".into()],
            repository: None,
            dir: dir.to_path_buf(),
        }
    }

    #[test]
    fn licence_names() {
        for yes in ["LICENSE", "LICENSE-MIT", "license.txt", "COPYING", "NOTICE", "UNLICENSE", "OFL.txt", "LICENCE"] {
            assert!(is_licence_name(yes), "{yes}");
        }
        for no in ["README.md", "license.rs", "Cargo.toml", "build.rs"] {
            assert!(!is_licence_name(no), "{no}");
        }
    }

    #[test]
    fn violations_name_the_crate() {
        let dir = temp("violations");
        let deps = vec![
            package("ok", Some("MIT OR Apache-2.0"), &dir),
            package("gpl", Some("GPL-3.0-only"), &dir),
            package("none", None, &dir),
            package("broken", Some("MIT OR"), &dir),
            package("epaint_default_fonts", Some("(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0"), &dir),
            package("other_fonts", Some("Ubuntu-font-1.0"), &dir),
        ];
        let v = violations(&deps);
        assert_eq!(v.len(), 4, "{v:?}");
        assert!(v[0].starts_with("gpl 1.0.0: licence 'GPL-3.0-only'"));
        assert!(v[1].starts_with("none 1.0.0: no licence expression"));
        assert!(v[2].starts_with("broken 1.0.0:"));
        assert!(v[3].starts_with("other_fonts 1.0.0:"), "the exception is scoped to one crate: {v:?}");
    }

    #[test]
    fn renders_groups_templates_and_fonts() {
        let root = temp("render");
        fs::create_dir_all(root.join("packaging/licenses")).unwrap();
        fs::write(root.join("packaging/licenses/MIT.txt"), "Standard MIT text.\r\n").unwrap();
        fs::write(root.join("packaging/licenses/embedded-data.txt"), "About the data.").unwrap();
        let a = root.join("a");
        let b = root.join("b");
        let c = root.join("c");
        let member = root.join("member");
        for d in [&a, &b, &c, &member.join("fonts"), &a.join("assets"), &a.join("src")] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(a.join("LICENSE-MIT"), "Copyright X\n\nSame text.   \n").unwrap();
        fs::write(a.join("assets/LICENSE.txt"), "Asset licence.").unwrap();
        fs::write(a.join("src/license.txt"), "never read").unwrap();
        fs::write(b.join("COPYING"), "Copyright X\r\n\r\nSame text.\r\n").unwrap();
        fs::write(member.join("fonts/Face.ttf"), b"font").unwrap();
        fs::write(member.join("fonts/OFL.txt"), "Font licence.").unwrap();
        let shipped = Shipped {
            workspace_root: root.clone(),
            members: vec![package("mc-ui", None, &member)],
            dependencies: vec![
                package("a", Some("MIT"), &a),
                package("b", Some("MIT"), &b),
                package("c", Some("Apache-2.0 OR MIT"), &c),
            ],
        };
        let text = render(&shipped, &RenderOptions { version: "9.9.9".into(), target: "test-target".into() }).unwrap();
        assert!(text.starts_with("MathCAS v2 9.9.9: third-party notices"));
        assert!(text.contains("1. Components (3 crates)"));
        assert!(text.contains("2. Licence texts (3 distinct)"), "{text}");
        // a and b carry the same text (line ends and trailing spaces aside): one group.
        assert!(text.contains("  a 1.0.0 (LICENSE-MIT)\n  b 1.0.0 (COPYING)\n"), "{text}");
        assert!(text.contains("a 1.0.0 (assets/LICENSE.txt)"));
        assert!(!text.contains("never read"));
        assert!(text.contains("c 1.0.0 (no licence file in the package; standard MIT text; copyright A. Author)"));
        assert!(text.contains("Fonts compiled into mc-ui: Face.ttf") && text.contains("Font licence."));
        assert!(text.contains("4. Notes on embedded data\n\nAbout the data."));

        // A crate without files and without a standard text for its licence is an error.
        let missing = Shipped { dependencies: vec![package("d", Some("Zlib"), &c)], ..shipped.clone() };
        let err = render(&missing, &RenderOptions { version: "1".into(), target: "t".into() }).unwrap_err();
        assert!(err.contains("no standard text Zlib.txt"), "{err}");
    }

    #[test]
    fn graph_walk_follows_normal_dependencies_only() {
        let meta = json::parse(
            r#"{
              "workspace_root": "/ws",
              "workspace_members": ["app", "lib"],
              "packages": [
                {"id": "app", "name": "app", "version": "0.1.0", "manifest_path": "/ws/app/Cargo.toml",
                 "targets": [{"name": "app-bin", "kind": ["bin"]}]},
                {"id": "lib", "name": "lib", "version": "0.1.0", "manifest_path": "/ws/lib/Cargo.toml", "targets": []},
                {"id": "dep", "name": "dep", "version": "1.2.0", "license": "MIT", "authors": ["Z"],
                 "manifest_path": "/reg/dep/Cargo.toml", "targets": []},
                {"id": "devdep", "name": "devdep", "version": "1.0.0", "manifest_path": "/reg/d/Cargo.toml",
                 "targets": []},
                {"id": "builddep", "name": "builddep", "version": "1.0.0", "manifest_path": "/reg/b/Cargo.toml",
                 "targets": []}
              ],
              "resolve": {"nodes": [
                {"id": "app", "deps": [
                  {"pkg": "lib", "dep_kinds": [{"kind": null, "target": null}]},
                  {"pkg": "devdep", "dep_kinds": [{"kind": "dev", "target": null}]},
                  {"pkg": "builddep", "dep_kinds": [{"kind": "build", "target": null}]}]},
                {"id": "lib", "deps": [{"pkg": "dep", "dep_kinds": [{"kind": "dev"}, {"kind": null}]}]},
                {"id": "dep", "deps": []},
                {"id": "devdep", "deps": []},
                {"id": "builddep", "deps": []}
              ]}
            }"#,
        )
        .unwrap();
        let s = shipped(&meta, &["app-bin"]).unwrap();
        assert_eq!(s.members.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["app", "lib"]);
        assert_eq!(s.dependencies.len(), 1);
        assert_eq!(
            (s.dependencies[0].label(), s.dependencies[0].authors.clone()),
            ("dep 1.2.0".into(), vec!["Z".into()])
        );
        assert!(shipped(&meta, &["nope"]).unwrap_err().contains("'nope'"));
    }
}
