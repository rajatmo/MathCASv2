use super::*;

const SAMPLE: &str = "#! mathcas 2 notebook\n\n#% text\n# Title\n#\n# More prose.\n\n#% cell\nx = 1\nprint(x)\n\
                      #% output\n#| 1\n#= 1\n\n#% cell\n# a comment\n#% not a marker\ny = 2\n\n\n#% cell\n1/0\n\
                      #% output\n#! failure(division_by_zero)\n#|\n";

fn line(kind: OutputKind, text: &str) -> OutputLine {
    OutputLine { kind, text: text.into() }
}

#[test]
fn round_trip_of_the_format_before_the_extensions() {
    let nb = Notebook::parse(SAMPLE).unwrap();
    assert_eq!(nb.print(), SAMPLE);
    assert_eq!(nb.cells.len(), 4);
    assert_eq!(nb.cells[0].prose().unwrap(), "Title\n\nMore prose.");
    assert_eq!(nb.cells[1].source().unwrap(), "x = 1\nprint(x)");
    let out: Vec<_> = nb.cells[1].output_lines().cloned().collect();
    assert_eq!(out[0], line(OutputKind::Print, "1"));
    assert_eq!(out[1].kind, OutputKind::Value);
    assert_eq!(nb.cells[2].source().unwrap(), "# a comment\n#% not a marker\ny = 2");
    assert_eq!((nb.cells[2].gap, nb.cells[2].outputs.len()), (2, 0));
    assert_eq!(nb.cells[3].output_lines().nth(1).unwrap(), &line(OutputKind::Print, ""));
    assert_eq!(nb.first_lines(), [4, 9, 16, 22]);
    // CR LF line ends are kept
    let crlf = SAMPLE.replace('\n', "\r\n");
    let nb2 = Notebook::parse(&crlf).unwrap();
    assert!(nb2.crlf);
    assert_eq!(nb2.print(), crlf);
    assert_eq!(nb2.cells, nb.cells);
}

#[test]
fn built_notebooks_print_in_the_old_format() {
    let mut nb = Notebook::new();
    nb.cells.push(Cell::text("Hello\n\nworld"));
    nb.cells.push(Cell::code("1 + 1\n"));
    nb.cells.push(Cell::code("2"));
    nb.cells[1].set_output(Some(vec![line(OutputKind::Value, "2")]));
    let text = nb.print();
    assert_eq!(
        text,
        "#! mathcas 2 notebook\n\n#% text\n# Hello\n#\n# world\n\n#% cell\n1 + 1\n#% output\n#= 2\n\n#% cell\n2\n\n"
    );
    assert_eq!(Notebook::parse(&text).unwrap(), nb);
    nb.cells[1].set_output(None);
    assert!(nb.cells[1].outputs.is_empty());
}

#[test]
fn layouts_that_print_differently() {
    // source before the first marker becomes a code cell; a missing final line end is added
    let nb = Notebook::parse("#! mathcas 2 notebook\n1 + 1\n\n#% cell\n2").unwrap();
    assert_eq!(nb.cells.len(), 2);
    assert_eq!(nb.print(), "#! mathcas 2 notebook\n#% cell\n1 + 1\n\n#% cell\n2\n");
    // `#|x` reads as `#| x`
    let nb = Notebook::parse("#!  mathcas  2  notebook\n#% cell\n1\n#% output\n#=1\n").unwrap();
    assert_eq!(nb.print(), "#! mathcas 2 notebook\n#% cell\n1\n#% output\n#= 1\n");
    assert_eq!(Notebook::parse("\u{feff}#! mathcas 2 notebook\n").unwrap(), Notebook::default());
    // `input` is `cell`; extra spaces between attributes are dropped
    let nb = Notebook::parse("#! mathcas 2 notebook\n#% input  label=2\nx\n").unwrap();
    assert_eq!(nb.print(), "#! mathcas 2 notebook\n#% cell label=2\nx\n");
}

#[test]
fn errors() {
    let e = |t: &str| Notebook::parse(t).unwrap_err();
    assert_eq!(e("").line, 1);
    assert_eq!(e("#! mathcas 2\n#% cell\n1\n").line, 1);
    assert!(e("x").to_string().starts_with("line 1: a notebook starts with"));
    let err = e("#! mathcas 2 notebook\n#% cell\n1\n#% output\n#= 1\n2\n");
    assert_eq!(err.line, 6);
    assert!(err.message.contains("start a new cell"), "{err}");
    assert_eq!(e("#! mathcas 2 notebook\n#% text\n# t\n#% output\n").line, 4);
    // several output blocks are one cell's (blank lines between them are dropped)
    let nb = Notebook::parse("#! mathcas 2 notebook\n#% cell\n#% output\n#=\n\n#% output\n#% output\n").unwrap();
    assert_eq!((nb.cells.len(), nb.cells[0].outputs.len()), (1, 3));
    assert_eq!(e("#! mathcas 2 notebook\n#% section\n# S\n#% output\n#= 1\n").line, 4);
}

const EXTENDED: &str = "#! mathcas 2 notebook grouping=manual magnification=1.25

#% title
# Cooling of a sphere

#% section closed
# Model

#% cell label=1 typeset
k = ((3)/(4))
T(t) = T0*exp(-k t)
#% output n=1
#= 3/4

#% cell label=3
print(\"a\")
1 + 1
2 + 2
#% output
#| a
#% output n=4
#= 2
#% output n=5 size=360x225
#= image(\"cooling.files/plot-3f2a91.png\")
#? warning: something

#% code label=6 future=thing
fn helper(x) = x^2

#% item
# first
#% subitem
# second

#% program
# while true
#   x
";

#[test]
fn the_extended_grammar() {
    let (nb, warnings) = Notebook::read(EXTENDED).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(nb.print(), EXTENDED);
    assert_eq!(nb.header.get("grouping"), Some("manual"));
    assert_eq!(nb.header.number::<f64>("magnification"), Some(1.25));
    let kinds: Vec<_> = nb.cells.iter().map(|c| c.kind).collect();
    use Kind::*;
    assert_eq!(kinds, [Title, Section, Input, Input, Code, Item, Subitem, Program]);
    assert!(nb.cells[1].attrs.has("closed"));
    assert_eq!(nb.cells[2].attrs.number::<u32>("label"), Some(1));
    assert!(nb.cells[2].attrs.has("typeset"));
    let c = &nb.cells[3];
    assert_eq!(c.outputs.len(), 3);
    assert!(c.outputs[0].attrs.is_empty());
    assert_eq!(c.outputs[1].attrs.get("n"), Some("4"));
    assert_eq!(c.outputs[2].attrs.get("size"), Some("360x225"));
    assert_eq!(c.outputs[2].lines[1], line(OutputKind::Note, "warning: something"));
    // unknown attributes are kept
    assert_eq!(nb.cells[4].attrs.get("future"), Some("thing"));
    assert_eq!(nb.cells[7].prose().unwrap(), "while true\n  x");
    assert_eq!(nb.cells[7].source(), None);
    // a notebook reads as a script: only code cells hold non-comment lines
    let code: Vec<&str> = EXTENDED.lines().skip(1).filter(|l| !l.starts_with('#') && !l.trim().is_empty()).collect();
    assert_eq!(code, ["k = ((3)/(4))", "T(t) = T0*exp(-k t)", "print(\"a\")", "1 + 1", "2 + 2", "fn helper(x) = x^2"]);
}

#[test]
fn warnings_keep_content() {
    let text = "#! mathcas 2 notebook\n#% cell\nx\n#% sectoin\n# Heading\n#% cell label=a!b\ny\n";
    let (nb, warnings) = Notebook::read(text).unwrap();
    assert_eq!(nb.print(), text);
    assert_eq!(nb.cells.len(), 2);
    assert_eq!(nb.cells[0].source().unwrap(), "x\n#% sectoin\n# Heading");
    let lines: Vec<_> = warnings.iter().map(|w| w.line).collect();
    assert_eq!(lines, [4, 6]);
    assert!(warnings[0].message.contains("did you mean `#% section`"), "{}", warnings[0]);
    assert!(warnings[1].message.contains("malformed attribute `label=a!b`"));
    // ordinary `#%` comments are not warned about
    let (_, w) = Notebook::read("#! mathcas 2 notebook\n#% cell\n#% not a marker\n#% TODO later\n").unwrap();
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn attributes() {
    let mut a = Attrs::new();
    a.set("label", Some("3".into()));
    a.flag("closed", true);
    a.flag("closed", true);
    a.set("label", Some("4".into()));
    assert_eq!(a.to_string(), " label=4 closed");
    a.flag("closed", false);
    a.remove("missing");
    assert_eq!(a.to_string(), " label=4");
    assert_eq!(a.number::<u32>("label"), Some(4));
    assert_eq!(a.number::<u32>("closed"), None);
    let (p, bad) = Attrs::parse("n=1 size=360x225 x:y=1");
    assert_eq!(p.0.len(), 3);
    assert_eq!(bad, ["malformed attribute `x:y=1`"]);
}

#[test]
fn fragments() {
    let nb = Notebook::parse(EXTENDED).unwrap();
    let frag = print_cells(&nb.cells[2..4]);
    assert!(frag.starts_with("#% cells\n#% cell label=1 typeset\n"));
    assert_eq!(parse_cells(&frag).unwrap(), nb.cells[2..4]);
    assert_eq!(parse_cells(&frag.replace('\n', "\r\n")).unwrap(), nb.cells[2..4]);
    assert_eq!(parse_cells("x = 1\n"), None);
    assert_eq!(parse_cells("#% cells\nstray\n"), None);
    assert_eq!(parse_cells("#% cells\n"), Some(vec![]));
}

/// A small deterministic generator (no external crates): xorshift.
struct Gen(u64);

impl Gen {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn word(&mut self) -> String {
        let words = ["x", "f(x)", "1 + 2", "  indented", "#% not a marker", "a=b", "\"s\"", "é", "#", "#| no"];
        words[self.below(words.len())].to_string()
    }
}

fn generated(g: &mut Gen) -> Notebook {
    let mut nb = Notebook::new();
    if g.below(3) == 0 {
        nb.header.set("grouping", Some("manual".into()));
    }
    for _ in 0..g.below(12) {
        let kind = Kind::ALL[g.below(Kind::ALL.len())];
        let content: Vec<String> = (0..1 + g.below(4)).map(|_| g.word()).collect();
        let mut cell = Cell::new(kind, &content.join("\n"));
        if g.below(2) == 0 {
            cell.attrs.set("label", Some(g.below(100).to_string()));
        }
        if g.below(4) == 0 {
            cell.attrs.flag("closed", true);
        }
        if kind.is_code() {
            for _ in 0..g.below(3) {
                let mut b = OutputBlock::default();
                if g.below(2) == 0 {
                    b.attrs.set("n", Some(g.below(50).to_string()));
                }
                for _ in 0..1 + g.below(3) {
                    let k = [OutputKind::Print, OutputKind::Value, OutputKind::Message, OutputKind::Note][g.below(4)];
                    b.lines.push(OutputLine { kind: k, text: g.word() });
                }
                cell.outputs.push(b);
            }
        }
        cell.gap = 1 + g.below(2);
        nb.cells.push(cell);
    }
    nb
}

#[test]
fn generated_notebooks_round_trip() {
    let mut g = Gen(0x9e37_79b9_7f4a_7c15);
    for _ in 0..500 {
        let nb = generated(&mut g);
        let text = nb.print();
        let back = Notebook::parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(back, nb, "{text}");
        assert_eq!(back.print(), text);
        let firsts = nb.first_lines();
        let lines: Vec<&str> = text.lines().collect();
        for (cell, &at) in nb.cells.iter().zip(&firsts) {
            if let Some(first) = cell.lines.first() {
                assert_eq!(lines[at as usize - 1], first);
            }
        }
        assert_eq!(parse_cells(&print_cells(&nb.cells)).unwrap(), nb.cells);
    }
}

#[test]
fn every_kind_has_a_name() {
    for k in Kind::ALL {
        assert_eq!(Kind::from_name(k.name()), Some(k));
    }
    assert_eq!(Kind::from_name("input"), Some(Kind::Input));
    assert_eq!(Kind::from_name("Output"), None);
}

#[test]
fn the_example_notebook_round_trips() {
    let text = include_str!("../../../examples/notebook.mcn");
    let (nb, warnings) = Notebook::read(text).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(nb.print(), text);
}

#[test]
fn atomic_writes() {
    let dir = std::env::temp_dir().join(format!("mc-nbfile-atomic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("n.mcn");
    write_atomic(&path, "one\n").unwrap();
    write_atomic(&path, "two\n").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "two\n");
    let left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(left, ["n.mcn"], "no temporary file is left");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A fresh folder for a file-system test.
fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("mc-nbfile-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn pictures_are_embedded_in_the_file_and_read_back() {
    use crate::embed::{attachments, embed, extract};
    use crate::images::{image_path, image_value};
    let root = scratch("embed");
    let tmp = root.join("session");
    std::fs::create_dir_all(&tmp).unwrap();
    // a plot of the session: the picture, its description and a 3D scene (an SVG sibling stays behind)
    let png = tmp.join("plot-11-0.png");
    let picture: Vec<u8> =
        b"\x89PNG\r\n\x1a\n".iter().copied().chain((0..3000u32).map(|k| (k * 7 % 251) as u8)).collect();
    std::fs::write(&png, &picture).unwrap();
    let series: Vec<String> = (0..2000).map(|k| k.to_string()).collect();
    let json = format!("{{\"version\":1,\"series\":[{}]}}", series.join(","));
    std::fs::write(tmp.join("plot-11-0.json"), &json).unwrap();
    std::fs::write(tmp.join("plot-11-0.scene3d"), "mathcas-scene3d 1\ngraphics3d(sphere())\n").unwrap();
    std::fs::write(tmp.join("plot-11-0.svg"), "<svg/>").unwrap();
    let text = format!(
        "{HEADER}\n\n#% cell label=1\nplot(x)\n#% output n=1 size=300x200\n#= {}\n\n#% cell label=2\nq\n#% output n=2\n\
         #= {}\n#% output n=3\n#= image(\"gone.png\")\n",
        image_value(&png.to_string_lossy()),
        image_value(&png.to_string_lossy()),
    );
    let mut nb = Notebook::parse(&text).unwrap();
    let r = embed(&mut nb, None);
    assert_eq!(r.pictures, 2);
    assert_eq!(r.missing, vec!["gone.png".to_string()]);
    let saved = nb.print();
    // one file: the value names the embedded picture, not the session's plots folder
    assert!(!saved.contains(&tmp.display().to_string()), "{saved}");
    let value = image_path(&nb.cells[0].outputs[0].lines[0].text).unwrap();
    assert!(value.starts_with("plot-") && value.ends_with(".png") && !value.contains('/'), "{value}");
    let files = attachments(&nb.cells[0].outputs[0]);
    let names: Vec<&str> = files.iter().map(|a| a.name.as_str()).collect();
    let stem = value.trim_end_matches(".png");
    assert_eq!(names, [value.clone(), format!("{stem}.json"), format!("{stem}.scene3d")]);
    assert_eq!(files[0].bytes, picture);
    assert_eq!(files[1].bytes, json.as_bytes(), "the description is kept byte for byte (deflated in the file)");
    assert!(saved.contains(&format!("#~ @{stem}.json deflate\n")));
    assert!(saved.lines().all(|l| l.len() <= 110), "data lines are short");
    // the deflated description is far smaller than its text
    let block = &nb.cells[0].outputs[0].lines;
    let data: usize = block.iter().filter(|l| l.kind == OutputKind::Data).map(|l| l.text.len()).sum();
    assert!(data < picture.len() * 4 / 3 + json.len(), "{data}");
    // the size stays on its block; the file reads back unchanged
    assert_eq!(nb.cells[0].outputs[0].attrs.get("size"), Some("300x200"));
    let again = Notebook::parse(&saved).unwrap();
    assert_eq!(again.print(), saved);
    // opening: the files go to the cache and the values name them there; saving again gives the same text
    let cache = root.join("cache");
    let mut opened = again.clone();
    assert_eq!(extract(&mut opened, &cache).unwrap(), 3);
    assert!(opened.cells.iter().flat_map(|c| c.output_lines()).all(|l| l.kind != OutputKind::Data));
    let shown = image_path(&opened.cells[0].outputs[0].lines[0].text).unwrap();
    assert_eq!(std::fs::read(&shown).unwrap(), picture);
    assert_eq!(std::fs::read_to_string(std::path::Path::new(&shown).with_extension("json")).unwrap(), json);
    assert_eq!(extract(&mut again.clone(), &cache).unwrap(), 0, "cached once");
    let mut resaved = opened.clone();
    embed(&mut resaved, None);
    assert_eq!(resaved.print(), saved, "saving an opened notebook gives the same text");
    // saving the file as read (no cache, e.g. the runner rewriting other cells) keeps the carried pictures
    let mut carried = again.clone();
    let r = embed(&mut carried, Some(&root.join("elsewhere")));
    assert!(r.missing.iter().all(|m| m == "gone.png"), "{r:?}");
    assert_eq!(carried.print(), saved);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_older_notebook_with_a_sidecar_folder_is_embedded_on_save_and_the_folder_stays() {
    use crate::embed::{attachments, embed};
    use crate::images::image_path;
    let root = scratch("older");
    let side = root.join("cooling.files");
    std::fs::create_dir_all(&side).unwrap();
    std::fs::write(side.join("plot-3f2a91c04b7e.png"), b"\x89PNG\r\n\x1a\nold").unwrap();
    std::fs::write(side.join("plot-3f2a91c04b7e.json"), "{}").unwrap();
    let text = format!(
        "{HEADER}\n\n#% cell label=1\nplot(x)\n#% output n=1\n#= image(\"cooling.files/plot-3f2a91c04b7e.png\")\n"
    );
    // reading the old layout is unchanged
    let mut nb = Notebook::parse(&text).unwrap();
    assert_eq!(nb.print(), text);
    let r = embed(&mut nb, Some(&root));
    assert_eq!((r.pictures, r.missing.len()), (1, 0));
    let value = image_path(&nb.cells[0].outputs[0].lines[0].text).unwrap();
    assert!(!value.contains("cooling.files"), "{value}");
    assert_eq!(attachments(&nb.cells[0].outputs[0])[0].bytes, b"\x89PNG\r\n\x1a\nold");
    assert!(side.join("plot-3f2a91c04b7e.png").is_file(), "the old folder is left for the user");
    let _ = std::fs::remove_dir_all(&root);
}
