// Reads the usage lines of the documentation pages (`docs/functions/<name>.mcn` at the repository root: the text
// cells between the title and the first section, each starting with the call in backticks) into a table built into
// the crate, so the help index always has the pages' current usage lines.
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/functions");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut pages: Vec<(String, Vec<(String, String)>)> = vec![];
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("mcn") {
                continue;
            }
            let (Some(name), Ok(text)) =
                (path.file_stem().and_then(|s| s.to_str()).map(str::to_string), std::fs::read_to_string(&path))
            else {
                continue;
            };
            pages.push((name, usages(&text)));
        }
    }
    pages.sort();
    let mut out = String::from("/// The pages: each function's name and its usage lines (call, text).\n");
    out.push_str("pub static PAGES: &[(&str, &[(&str, &str)])] = &[\n");
    for (name, us) in &pages {
        out.push_str(&format!("    ({name:?}, &["));
        for (call, text) in us {
            out.push_str(&format!("({call:?}, {text:?}), "));
        }
        out.push_str("]),\n");
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo")).join("pages.rs");
    std::fs::write(dest, out).expect("the build folder is writable");
}

/// The usage cells of a page: `text` cells after the title and before the first section, each a leading code span
/// (the call) and the text after it.
fn usages(text: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut cells: Vec<(String, Vec<String>)> = vec![];
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(kind) = line.strip_prefix("#% ") {
            cells.push((kind.split_whitespace().next().unwrap_or("").to_string(), vec![]));
        } else if let Some(c) = cells.last_mut() {
            c.1.push(line.strip_prefix("# ").or_else(|| line.strip_prefix('#')).unwrap_or(line).to_string());
        }
    }
    let mut started = false;
    for (kind, lines) in cells {
        match kind.as_str() {
            "title" => started = true,
            "section" => break,
            "text" if started => {
                let body = lines.join(" ");
                let body = body.trim();
                let Some(rest) = body.strip_prefix('`') else { continue };
                let Some(close) = rest.find('`') else { continue };
                out.push((rest[..close].to_string(), rest[close + 1..].trim().to_string()));
            }
            _ => {}
        }
    }
    out
}
