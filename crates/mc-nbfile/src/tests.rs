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

/// An SVG drawing as `mc-graphics` writes it (the class on the root element), `w` x `h` points, with `body`.
fn our_svg(w: u32, h: u32, body: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" class=\"mc-graphic\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n\
         <path class=\"mcf-bg\" d=\"M0 0L{w} 0L{w} {h}L0 {h}Z\" fill=\"#ffffff\"/>\n{body}</svg>\n"
    )
}

#[test]
fn plots_are_embedded_as_their_svg_deflated_and_shared_between_outputs() {
    use crate::embed::{attachments, embed, extract, is_ours};
    use crate::images::{image_path, image_value, picture_size};
    let root = scratch("svg-embed");
    let tmp = root.join("session");
    std::fs::create_dir_all(&tmp).unwrap();
    // a session plot: the PNG the kernel names, its SVG and PDF siblings, its description
    let png = tmp.join("plot-7-0.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\nnot shown").unwrap();
    let curve: String = (0..400).map(|k| format!("L{} {}", k, (k * k) % 300)).collect();
    let svg = our_svg(640, 400, &format!("<path d=\"M0 0{curve}\" fill=\"none\" stroke=\"#1f77b4\"/>\n"));
    assert!(is_ours(svg.as_bytes()) && !is_ours(b"<svg width=\"3\"/>"));
    std::fs::write(tmp.join("plot-7-0.svg"), &svg).unwrap();
    std::fs::write(tmp.join("plot-7-0.pdf"), b"%PDF-1.4").unwrap();
    std::fs::write(tmp.join("plot-7-0.json"), "{\"version\":1}").unwrap();
    let v = image_value(&png.to_string_lossy());
    let text = format!("{HEADER}\n\n#% cell label=1\nplot(x)\n#% output n=1\n#= {v}\n\n#% cell label=2\nagain\n#% output n=2\n#= {v}\n");
    let mut nb = Notebook::parse(&text).unwrap();
    let r = embed(&mut nb, None);
    assert_eq!((r.pictures, r.missing.len()), (2, 0));
    let saved = nb.print();
    let name = image_path(&nb.cells[0].outputs[0].lines[0].text).unwrap();
    assert!(name.starts_with("plot-") && name.ends_with(".svg"), "{name}");
    assert!(saved.contains(&format!("#~ @{name} deflate\n")), "{saved}");
    assert!(!saved.contains(".pdf") && !saved.contains("not shown"), "neither the PDF nor the PNG is stored");
    // the second output names the same file and carries nothing (one copy per notebook)
    assert_eq!(image_path(&nb.cells[1].outputs[0].lines[0].text).unwrap(), name);
    assert!(attachments(&nb.cells[1].outputs[0]).is_empty());
    assert_eq!(saved.matches("#~ @").count(), 2, "the drawing and its description, once");
    let files = attachments(&nb.cells[0].outputs[0]);
    assert_eq!(files[0].bytes, svg.as_bytes(), "the drawing byte for byte");
    // deflated: far smaller than the drawing's text
    assert!(r.stored * 2 < r.bytes, "{} stored of {}", r.stored, r.bytes);
    // opening: both outputs show the cached drawing; it has its natural size; saving again gives the same text
    let cache = root.join("cache");
    let mut opened = Notebook::parse(&saved).unwrap();
    assert_eq!(extract(&mut opened, &cache).unwrap(), 2);
    let shown: Vec<String> = opened.cells.iter().map(|c| image_path(&c.outputs[0].lines[0].text).unwrap()).collect();
    assert_eq!(shown[0], shown[1]);
    assert_eq!(std::fs::read_to_string(&shown[0]).unwrap(), svg);
    assert_eq!(picture_size(std::path::Path::new(&shown[0])), Some((640, 400)));
    let mut resaved = opened.clone();
    embed(&mut resaved, None);
    assert_eq!(resaved.print(), saved, "saving an opened notebook gives the same text");
    // saved without the cache (the runner rewriting other cells): the carried drawing stays, still once
    let mut carried = Notebook::parse(&saved).unwrap();
    let r = embed(&mut carried, Some(&root.join("elsewhere")));
    assert!(r.missing.is_empty(), "{r:?}");
    assert_eq!(carried.print(), saved);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_plots_column_twin_travels_with_its_svg() {
    use crate::embed::{attachments, embed, extract, COLUMN_TWIN};
    use crate::images::{image_path, image_value};
    let root = scratch("column-twin");
    let tmp = root.join("session");
    std::fs::create_dir_all(&tmp).unwrap();
    let png = tmp.join("plot-8-0.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\nnot shown").unwrap();
    let svg = our_svg(640, 400, "<path d=\"M0 0L640 400\" stroke=\"#1f77b4\"/>\n");
    let column = our_svg(569, 356, "<path d=\"M0 0L569 356\" stroke=\"#1f77b4\"/>\n");
    std::fs::write(tmp.join("plot-8-0.svg"), &svg).unwrap();
    std::fs::write(tmp.join("plot-8-0.column.svg"), &column).unwrap();
    // the print twins of `export_data` stay in the session
    std::fs::write(tmp.join("plot-8-0.print.svg"), &column).unwrap();
    std::fs::write(tmp.join("plot-8-0.json"), "{\"version\":1}").unwrap();
    // a second plot without a twin (a plot of its own size), and a twin that is not a drawing of ours
    let png2 = tmp.join("plot-8-1.png");
    std::fs::write(&png2, b"\x89PNG\r\n\x1a\nnot shown").unwrap();
    std::fs::write(tmp.join("plot-8-1.svg"), our_svg(500, 300, "")).unwrap();
    let png3 = tmp.join("plot-8-2.png");
    std::fs::write(&png3, b"\x89PNG\r\n\x1a\nnot shown").unwrap();
    std::fs::write(tmp.join("plot-8-2.svg"), our_svg(640, 400, "<g/>")).unwrap();
    std::fs::write(tmp.join("plot-8-2.column.svg"), "<svg width=\"3\"/>").unwrap();
    let text = format!(
        "{HEADER}\n\n#% cell label=1\nplot(x)\n#% output n=1\n#= {}\n\n#% cell label=2\nq\n#% output n=2\n#= {}\n\n\
         #% cell label=3\nr\n#% output n=3\n#= {}\n",
        image_value(&png.to_string_lossy()),
        image_value(&png2.to_string_lossy()),
        image_value(&png3.to_string_lossy())
    );
    let mut nb = Notebook::parse(&text).unwrap();
    embed(&mut nb, None);
    let saved = nb.print();
    let name = image_path(&nb.cells[0].outputs[0].lines[0].text).unwrap();
    let stem = name.trim_end_matches(".svg");
    let names: Vec<String> = attachments(&nb.cells[0].outputs[0]).into_iter().map(|a| a.name).collect();
    assert_eq!(names, [name.clone(), format!("{stem}.json"), format!("{stem}.{COLUMN_TWIN}")]);
    assert!(saved.contains(&format!("#~ @{stem}.column.svg deflate\n")), "{saved}");
    assert!(!saved.contains(".print."), "the print twins are not stored");
    assert_eq!(attachments(&nb.cells[1].outputs[0]).len(), 1, "no twin, nothing more");
    assert_eq!(attachments(&nb.cells[2].outputs[0]).len(), 1, "a twin not of ours is left out");
    // opening puts the twin beside the drawing, where the PDF export looks for it
    let cache = root.join("cache");
    let mut opened = Notebook::parse(&saved).unwrap();
    extract(&mut opened, &cache).unwrap();
    let shown = image_path(&opened.cells[0].outputs[0].lines[0].text).unwrap();
    assert_eq!(std::fs::read_to_string(std::path::Path::new(&shown).with_extension(COLUMN_TWIN)).unwrap(), column);
    let mut resaved = opened.clone();
    embed(&mut resaved, None);
    assert_eq!(resaved.print(), saved, "saving an opened notebook gives the same text");
    // saved without the cache: the carried twin stays with its drawing
    let mut carried = Notebook::parse(&saved).unwrap();
    let r = embed(&mut carried, Some(&root.join("elsewhere")));
    assert!(r.missing.is_empty(), "{r:?}");
    assert_eq!(carried.print(), saved);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_dense_plot_is_embedded_as_a_high_resolution_png_shown_at_its_size() {
    use crate::embed::{attachments, embed, SVG_LIMIT};
    use crate::images::{image_path, image_value, picture_size, png_density};
    let root = scratch("svg-dense");
    std::fs::create_dir_all(&root).unwrap();
    // 120 000 dots at scattered places: the drawing deflates to more than the limit
    let mut x: u64 = 12345;
    let mut d = String::with_capacity(2_000_000);
    for _ in 0..120_000 {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let (a, b) = ((x >> 33) % 64000, (x >> 13) % 40000);
        d.push_str(&format!("M{}.{:02} {}.{:02}h0", a / 100, a % 100, b / 100, b % 100));
    }
    let svg = our_svg(640, 400, &format!("<path d=\"{d}\" fill=\"none\" stroke=\"#ff0000\" stroke-width=\"1\" stroke-linecap=\"round\"/>\n"));
    assert!(miniz_oxide::deflate::compress_to_vec_zlib(svg.as_bytes(), 9).len() > SVG_LIMIT);
    let png = root.join("plot-9-0.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\nsmall").unwrap();
    std::fs::write(root.join("plot-9-0.svg"), &svg).unwrap();
    let text = format!("{HEADER}\n\n#% cell label=1\nlist_plot(p)\n#% output n=1\n#= {}\n", image_value(&png.to_string_lossy()));
    let mut nb = Notebook::parse(&text).unwrap();
    embed(&mut nb, None);
    let name = image_path(&nb.cells[0].outputs[0].lines[0].text).unwrap();
    assert!(name.ends_with(".png"), "{name}");
    let files = attachments(&nb.cells[0].outputs[0]);
    let hi = &files[0].bytes;
    assert_eq!(png_density(hi), 2);
    // twice the pixels, shown at the drawing's size
    let ihdr = |i: usize| u32::from_be_bytes([hi[16 + i], hi[17 + i], hi[18 + i], hi[19 + i]]);
    assert_eq!((ihdr(0), ihdr(4)), (1280, 800));
    let out = root.join(&name);
    std::fs::write(&out, hi).unwrap();
    assert_eq!(picture_size(&out), Some((640, 400)));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn density_chunks_are_written_and_read() {
    use crate::images::{png_density, with_density};
    // a 1 x 1 PNG (signature, IHDR, IDAT, IEND)
    let png: Vec<u8> = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0,
        0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 10, b'I', b'D', b'A', b'T', 0x78, 0x9C, 0x63, 0, 1, 0, 0, 5, 0, 1, 0x0D, 0x0A, 0x2D,
        0xB4, 0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
    ]
    .to_vec();
    assert_eq!(png_density(&png), 1);
    let two = with_density(&png, 2);
    assert_eq!(two.len(), png.len() + 21);
    assert_eq!(png_density(&two), 2);
    // the chunk as PNG readers check it (CRC from an independent implementation): pHYs, 5669 pixels a metre both
    // ways, unit metre
    assert_eq!(&two[33..54], &[0, 0, 0, 9, b'p', b'H', b'Y', b's', 0, 0, 0x16, 0x25, 0, 0, 0x16, 0x25, 1, 0x49, 0x52, 0x24, 0xF0][..]);
    // written again: one chunk, the new density
    assert_eq!(png_density(&with_density(&two, 3)), 3);
    assert_eq!(with_density(&two, 3).len(), two.len());
    assert_eq!(with_density(b"not a png", 2), b"not a png");
}
