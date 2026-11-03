//! The editor extension in `editors/vscode`: its manifest, grammar and language configuration parse and fit
//! together, the grammar covers every keyword of the language, and, when a JavaScript runtime is available,
//! `extension.js` passes a syntax check and drives the real server through `tests/vscode/smoke.js`.
//!
//! The runtime is `node` on PATH, or the program named by `MC_NODE` (any Node-compatible runtime; the variable
//! `ELECTRON_RUN_AS_NODE=1` is set for it, so an Electron binary works too). Without one the script checks are
//! skipped with a message.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn extension_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../editors/vscode")
}

fn json(file: &Path) -> Value {
    let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

/// Every `match`, `begin` and `end` regular expression of a grammar.
fn regexes(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                match (k.as_str(), x) {
                    ("match" | "begin" | "end", Value::String(s)) => out.push(s.clone()),
                    _ => regexes(x, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| regexes(x, out)),
        _ => {}
    }
}

#[test]
fn manifest_grammar_and_configuration() {
    let dir = extension_dir();
    let manifest = json(&dir.join("package.json"));
    assert!(dir.join(manifest["main"].as_str().unwrap()).is_file());
    let c = &manifest["contributes"];
    let lang = &c["languages"][0];
    assert_eq!(lang["id"], "mathcas");
    assert_eq!(lang["extensions"], serde_json::json!([".mc", ".mcn"]));
    let config = json(&dir.join(lang["configuration"].as_str().unwrap()));
    assert_eq!(config["comments"]["lineComment"], "#");
    assert_eq!(config["brackets"].as_array().unwrap().len(), 3);
    let grammar_ref = &c["grammars"][0];
    assert_eq!(grammar_ref["language"], "mathcas");
    let grammar = json(&dir.join(grammar_ref["path"].as_str().unwrap()));
    assert_eq!(grammar["scopeName"], grammar_ref["scopeName"]);
    let commands: Vec<&str> = c["commands"].as_array().unwrap().iter().map(|x| x["command"].as_str().unwrap()).collect();
    for want in ["mathcas.runFile", "mathcas.runSelection", "mathcas.restartServer"] {
        assert!(commands.contains(&want), "missing command {want}");
    }
    let props = &c["configuration"]["properties"];
    for want in ["mathcas.server.path", "mathcas.server.enable", "mathcas.repl.path"] {
        assert!(props.get(want).is_some(), "missing setting {want}");
    }
    assert!(dir.join("INSTALL.txt").is_file());
    // every rule the top level includes exists
    for p in grammar["patterns"].as_array().unwrap() {
        let name = p["include"].as_str().unwrap().trim_start_matches('#');
        assert!(grammar["repository"].get(name).is_some(), "grammar rule {name} is missing");
    }
}

#[test]
fn grammar_covers_the_keywords() {
    let grammar = json(&extension_dir().join("syntaxes/mathcas.tmLanguage.json"));
    let mut rs = vec![];
    regexes(&grammar, &mut rs);
    // a keyword appears as a whole alternative of a group: `(kw|`, `|kw|` or `|kw)`, or `(kw)`
    let covered = |kw: &str| {
        rs.iter().any(|r| {
            ["(", "|"].iter().any(|open| [")", "|"].iter().any(|close| r.contains(&format!("{open}{kw}{close}"))))
        })
    };
    let missing: Vec<&str> = mc_syntax::KEYWORDS.iter().copied().filter(|k| !covered(k)).collect();
    assert!(missing.is_empty(), "keywords without highlighting: {missing:?}");
    // ball literals and decorators have rules of their own
    assert!(rs.iter().any(|r| r.contains("(~)")));
    assert!(rs.iter().any(|r| r.starts_with("(@)")));
}

/// A JavaScript runtime, if there is one.
fn runtime() -> Option<String> {
    let program = std::env::var("MC_NODE").unwrap_or_else(|_| "node".to_string());
    let ok = Command::new(&program).env("ELECTRON_RUN_AS_NODE", "1").arg("--version").output().is_ok_and(|o| o.status.success());
    ok.then_some(program)
}

#[test]
fn extension_script_against_the_server() {
    let Some(node) = runtime() else {
        eprintln!("skipped: no JavaScript runtime (install node, or set MC_NODE)");
        return;
    };
    let script = extension_dir().join("extension.js");
    let check = Command::new(&node).env("ELECTRON_RUN_AS_NODE", "1").arg("--check").arg(&script).output().unwrap();
    assert!(check.status.success(), "{}", String::from_utf8_lossy(&check.stderr));

    let scratch = std::env::temp_dir().join(format!("mc-lsp-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let smoke = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vscode/smoke.js");
    // the script stops itself after 30 seconds
    let run = Command::new(&node)
        .env("ELECTRON_RUN_AS_NODE", "1")
        .arg(&smoke)
        .arg(&script)
        .arg(env!("CARGO_BIN_EXE_mc-lsp"))
        .arg(&scratch)
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success() && out.trim() == "ok", "{out}\n{}", String::from_utf8_lossy(&run.stderr));
}
