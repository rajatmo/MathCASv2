//! Every notebook of the documentation reads without warnings and prints back to its own text; written with the
//! spellings of earlier versions (labels `label=N` and `n=N`, the style names `Title`, `ItemNumbered`, `Input`, the
//! kind words without underscores), it reads as the same notebook and prints in the current spelling.
use mc_nbfile::{Kind, Notebook};
use std::path::{Path, PathBuf};

fn notebooks() -> Vec<PathBuf> {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let mut out = vec![];
    for dir in ["functions", "guides"] {
        let Ok(rd) = std::fs::read_dir(docs.join(dir)) else { continue };
        out.extend(rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "mcn")));
    }
    out.sort();
    out
}

/// The kind word a notebook model before version 1.0 used: the style name in CamelCase.
fn camel(k: Kind) -> String {
    if k == Kind::Input {
        return "Input".into();
    }
    k.name()
        .split('_')
        .map(|w| {
            let mut cs = w.chars();
            cs.next().map(|c| c.to_ascii_uppercase().to_string() + cs.as_str()).unwrap_or_default()
        })
        .collect()
}

/// `text` with numbered labels on every code cell and output block, in the current spelling (`new`) and in the old
/// one, the kind words changed by `old_kind` (`old`).
fn labelled(text: &str, old_kind: &dyn Fn(Kind) -> String) -> (String, String) {
    let (mut new, mut old) = (String::new(), String::new());
    let mut n = 0;
    for line in text.lines() {
        let word = line.strip_prefix("#% ").map(|r| r.split(' ').next().unwrap_or(""));
        match word.and_then(|w| if w == "output" { None } else { Kind::from_name(w) }) {
            Some(k) => {
                let rest = &line[3 + word.unwrap_or("").len()..];
                if k.is_code() {
                    n += 1;
                    new.push_str(&format!("#% {}{rest} in={n}\n", k.name()));
                    old.push_str(&format!("#% {}{rest} label={n}\n", old_kind(k)));
                } else {
                    new.push_str(&format!("{line}\n"));
                    old.push_str(&format!("#% {}{rest}\n", old_kind(k)));
                }
            }
            None if word == Some("output") => {
                let rest = &line["#% output".len()..];
                new.push_str(&format!("#% output{rest} out={n}\n"));
                old.push_str(&format!("#% output{rest} n={n}\n"));
            }
            None => {
                new.push_str(&format!("{line}\n"));
                old.push_str(&format!("{line}\n"));
            }
        }
    }
    (new, old)
}

#[test]
fn every_documented_notebook_reads_and_prints_back() {
    let paths = notebooks();
    assert!(paths.len() > 1000, "the documentation's notebooks: {}", paths.len());
    let mut bad = vec![];
    for p in &paths {
        let text = std::fs::read_to_string(p).unwrap().replace("\r\n", "\n");
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let (nb, warnings) = match Notebook::read(&text) {
            Ok(r) => r,
            Err(e) => {
                bad.push(format!("{name}: {e}"));
                continue;
            }
        };
        if !warnings.is_empty() {
            bad.push(format!("{name}: {} warnings, the first {}", warnings.len(), warnings[0]));
        }
        if nb.print() != text {
            bad.push(format!("{name}: does not print back to its text"));
        }
        // labelled, in both spellings: the old reads as the new
        let old_kinds: [&dyn Fn(Kind) -> String; 2] = [&camel, &|k: Kind| k.name().replace('_', "")];
        for old_kind in old_kinds {
            let (new, old) = labelled(&text, old_kind);
            let a = Notebook::read(&new).map(|(nb, w)| (nb.print(), w.len()));
            let b = Notebook::read(&old).map(|(nb, w)| (nb.print(), w.len()));
            match (a, b) {
                (Ok((a, 0)), Ok((b, 0))) if a == b && a == new => {}
                (a, b) => {
                    let show = |r: &Result<(String, usize), _>| match r {
                        Ok((t, w)) => format!("{w} warnings, {} bytes", t.len()),
                        Err(e) => format!("{e}"),
                    };
                    bad.push(format!("{name}: labelled {} / old spelling {}", show(&a), show(&b)));
                }
            }
        }
        if bad.len() > 10 {
            break;
        }
    }
    assert!(bad.is_empty(), "{} of {} notebooks:\n{}", bad.len(), paths.len(), bad.join("\n"));
}

/// The predicates renamed before 1.0 are converted in the code cells of a notebook when it is read: whole names
/// outside strings, comments and backquotes; prose, qualified names and longer names are left alone. The second
/// route: the converted notebook prints as one written with the new names.
#[test]
fn old_predicate_names_in_code_are_converted() {
    let old = "#! mathcas 2 notebook\n\n#% text\n# possible_zero_q is renamed\n\n#% cell label=1\nis_ok = possible_zero_q(x - x) and match_q(1, _)\n\"possible_zero_q\" # value_q stays\n`true_q`(1) + M.true_q(2) + true_quux(3) + value_q(3)\n";
    let new = "#! mathcas 2 notebook\n\n#% text\n# possible_zero_q is renamed\n\n#% cell in=1\nis_ok = is_possible_zero(x - x) and is_match(1, _)\n\"possible_zero_q\" # value_q stays\n`true_q`(1) + M.true_q(2) + true_quux(3) + is_value(3)\n";
    let (nb, w) = Notebook::read(old).unwrap();
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(nb.print(), new);
    assert_eq!(Notebook::read(new).unwrap().0.print(), new);
    assert_eq!(mc_nbfile::renamed_predicates("is_value(x)"), None);
    assert_eq!(mc_nbfile::renamed_predicates("f(x) := upper_triangular_matrix_q(x)\n").as_deref(), Some("f(x) := is_upper_triangular_matrix(x)\n"));
}

/// The kind words are read in any case; other words after `#% ` are not markers.
#[test]
fn kind_words_in_any_case() {
    for k in Kind::ALL {
        assert_eq!(Kind::from_name(&camel(k)), Some(k), "{}", camel(k));
        assert_eq!(Kind::from_name(&k.name().to_ascii_uppercase()), Some(k));
    }
    assert_eq!(Kind::from_name("TODO"), None);
    assert_eq!(Kind::from_name("tıtle"), None);
    let (nb, w) = Notebook::read("#! mathcas 2 notebook\n\n#% Title\n# T\n\n#% Input label=1\n1 + 1\n#% Output n=1\n#= 2\n").unwrap();
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(nb.print(), "#! mathcas 2 notebook\n\n#% title\n# T\n\n#% cell in=1\n1 + 1\n#% output out=1\n#= 2\n");
}
