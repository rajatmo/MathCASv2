//! Notebooks written by earlier versions read with their cells, kinds and outputs, converted to the current names,
//! and print back as the current format (design 08-notebook 7.6). The fixtures in `tests/data/compat/<version>/`
//! are notebooks of the documentation and the examples as the tagged releases shipped them. Each is checked against
//! an independent reading of its lines; the converted code still parses; writing and reading again is a fixed point.
//! The version rule: a newer major version is refused with a message that says so, a newer minor version is read
//! with a warning and written back with its version.
use mc_nbfile::{Kind, Notebook, OutputKind, Version};
use std::path::{Path, PathBuf};

/// The kind words of the format (07-notebook 7.2), read from the marker lines.
const KINDS: [(&str, Kind); 22] = [
    ("cell", Kind::Input),
    ("input", Kind::Input),
    ("code", Kind::Code),
    ("text", Kind::Text),
    ("title", Kind::Title),
    ("subtitle", Kind::Subtitle),
    ("subsubtitle", Kind::Subsubtitle),
    ("chapter", Kind::Chapter),
    ("subchapter", Kind::Subchapter),
    ("section", Kind::Section),
    ("subsection", Kind::Subsection),
    ("subsubsection", Kind::Subsubsection),
    ("item", Kind::Item),
    ("item_paragraph", Kind::ItemParagraph),
    ("subitem", Kind::Subitem),
    ("subitem_paragraph", Kind::SubitemParagraph),
    ("subsubitem", Kind::Subsubitem),
    ("item_numbered", Kind::ItemNumbered),
    ("subitem_numbered", Kind::SubitemNumbered),
    ("subsubitem_numbered", Kind::SubsubitemNumbered),
    ("display_formula", Kind::DisplayFormula),
    ("program", Kind::Program),
];

/// The predicate names renamed before 1.0 that the fixtures use.
const OLD: [&str; 8] =
    ["possible_zero_q", "match_q", "palindrome_q", "value_q", "true_q", "polynomial_q", "string_match_q", "key_member_q"];

fn fixtures() -> Vec<(String, PathBuf)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/compat");
    let mut out = vec![];
    for v in std::fs::read_dir(&root).unwrap().flatten() {
        for f in std::fs::read_dir(v.path()).unwrap().flatten() {
            out.push((v.file_name().to_string_lossy().to_string(), f.path()));
        }
    }
    out.sort();
    out
}

/// What a plain reading of the lines finds: the kinds of the cells in order, and per cell its output lines by kind.
struct Scan {
    kinds: Vec<Kind>,
    outputs: Vec<Vec<OutputKind>>,
    /// the code lines of each code cell (before the outputs)
    code: Vec<Vec<String>>,
}

fn scan(text: &str) -> Scan {
    let mut s = Scan { kinds: vec![], outputs: vec![], code: vec![] };
    let mut in_output = false;
    for line in text.lines().skip(1) {
        if let Some(rest) = line.strip_prefix("#% ") {
            let word = rest.split(' ').next().unwrap_or("");
            if word == "output" {
                in_output = true;
                continue;
            }
            if let Some(&(_, k)) = KINDS.iter().find(|(w, _)| *w == word) {
                s.kinds.push(k);
                s.outputs.push(vec![]);
                s.code.push(vec![]);
                in_output = false;
                continue;
            }
        }
        if in_output {
            let kind = match line.get(..2) {
                Some("#|") => OutputKind::Print,
                Some("#=") => OutputKind::Value,
                Some("#!") => OutputKind::Message,
                Some("#?") => OutputKind::Note,
                Some("#~") => OutputKind::Data,
                _ => continue,
            };
            s.outputs.last_mut().unwrap().push(kind);
        } else if matches!(s.kinds.last(), Some(Kind::Input | Kind::Code)) {
            s.code.last_mut().unwrap().push(line.to_string());
        }
    }
    s
}

/// Whether `name(` occurs in `line` outside strings and comments.
fn calls(line: &str, name: &str) -> bool {
    let mut code = String::new();
    let mut in_str = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => in_str = !in_str,
            '\\' if in_str => {
                chars.next();
            }
            '#' if !in_str => break,
            _ if !in_str => code.push(c),
            _ => {}
        }
    }
    code.match_indices(&format!("{name}(")).any(|(i, _)| {
        code[..i].chars().last().is_none_or(|p| !(p.is_alphanumeric() || p == '_' || p == '.' || p == '`'))
    })
}

#[test]
fn notebooks_of_earlier_versions_read_with_their_cells_and_outputs() {
    let all = fixtures();
    let versions: std::collections::BTreeSet<&str> = all.iter().map(|(v, _)| v.as_str()).collect();
    assert_eq!(versions.into_iter().collect::<Vec<_>>(), ["v0.12.0", "v0.13.0", "v0.14.0", "v0.15.0"]);
    let (mut cells, mut outputs, mut renamed) = (0, 0, 0);
    for (version, path) in &all {
        let name = format!("{version}/{}", path.file_name().unwrap().to_string_lossy());
        let text = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
        let (nb, warnings) = Notebook::read(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(warnings.is_empty(), "{name}: {warnings:?}");
        assert_eq!(nb.version, Version { major: 2, minor: 0 }, "{name}");
        // cells and kinds, against the plain reading
        let s = scan(&text);
        assert_eq!(nb.cells.iter().map(|c| c.kind).collect::<Vec<_>>(), s.kinds, "{name}: kinds");
        // outputs: the same lines of the same kinds in the same cells
        for (i, c) in nb.cells.iter().enumerate() {
            let got: Vec<OutputKind> = c.output_lines().map(|o| o.kind).collect();
            assert_eq!(got, s.outputs[i], "{name}: outputs of cell {}", i + 1);
        }
        cells += nb.cells.len();
        outputs += nb.cells.iter().map(|c| c.output_lines().count()).sum::<usize>();
        // conversions: no old predicate name is left in the code; the code parses as it did
        for (i, c) in nb.cells.iter().enumerate().filter(|(_, c)| c.is_code()) {
            let new = c.source().unwrap();
            let old = s.code[i].join("\n");
            for n in OLD {
                assert!(!new.lines().any(|l| calls(l, n)), "{name}: cell {} still calls {n}", i + 1);
                if old.lines().any(|l| calls(l, n)) {
                    renamed += 1;
                    let is = format!("is_{}", n.strip_suffix("_q").unwrap());
                    assert!(new.lines().any(|l| calls(l, &is)), "{name}: cell {}: {n} not written {is}", i + 1);
                }
            }
            let (before, after) = (mc_syntax::parse(&old).errors.len(), mc_syntax::parse(&new).errors.len());
            assert_eq!(after, before, "{name}: cell {} parses differently after the conversion", i + 1);
        }
        // the current format: written with the current version, read again unchanged and without warnings
        let printed = nb.print();
        assert!(printed.starts_with("#! mathcas 2 notebook"), "{name}");
        let (again, w) = Notebook::read(&printed).unwrap();
        assert!(w.is_empty(), "{name}: {w:?}");
        assert_eq!(again.print(), printed, "{name}: printing is not a fixed point");
        assert_eq!(again.cells.len(), nb.cells.len());
    }
    // the fixed set as a whole (a fixture changed by accident shows here)
    assert_eq!((all.len(), cells, outputs, renamed), (19, 354, 133, 31), "files, cells, output lines, renamed calls");
}

#[test]
fn the_version_in_the_first_line() {
    let body = "\n#% cell\n1 + 1\n#% output\n#= 2\n";
    // the current version, also with extra spaces
    let (nb, w) = Notebook::read(&format!("#!  mathcas 2  notebook{body}")).unwrap();
    assert!(w.is_empty() && nb.version == Version::CURRENT);
    assert_eq!(Version::CURRENT.to_string(), "2");
    assert_eq!(Version::parse("2.0"), Some(Version::CURRENT));
    assert_eq!(Version::parse("2.13"), Some(Version { major: 2, minor: 13 }));
    assert_eq!(Version::parse("2."), None);
    assert_eq!(Version::parse("-2"), None);
    assert_eq!(Notebook::new().print(), "#! mathcas 2 notebook\n\n");
    // a newer minor version: read with a warning, everything kept, written back as that version
    let newer = "#! mathcas 2.3 notebook shading=on\n\n#% cell margin=wide\n1 + 1\n#% sidebar note\n#% output\n#= 2\n";
    let (nb, w) = Notebook::read(newer).unwrap();
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].line == 1 && w[0].message.contains("format 2.3"), "{}", w[0]);
    assert_eq!(nb.version, Version { major: 2, minor: 3 });
    assert_eq!(nb.print(), newer);
    // a newer major version: refused, and the message says why
    let e = Notebook::read(&format!("#! mathcas 3 notebook{body}")).unwrap_err();
    assert_eq!(e.line, 1);
    assert!(e.message.contains("format 3") && e.message.contains("newer version"), "{e}");
    let e = Notebook::read(&format!("#! mathcas 12.4 notebook{body}")).unwrap_err();
    assert!(e.message.contains("format 12.4"), "{e}");
    // older than the first format, and not a header at all
    let e = Notebook::read(&format!("#! mathcas 1 notebook{body}")).unwrap_err();
    assert!(e.message.contains("format 1 is not a notebook format"), "{e}");
    for bad in ["#! mathcas two notebook", "#! mathcas 2 script", "#! mathcas notebook", "# notebook"] {
        let e = Notebook::read(&format!("{bad}{body}")).unwrap_err();
        assert!(e.message.contains("starts with the line"), "{bad}: {e}");
    }
}
