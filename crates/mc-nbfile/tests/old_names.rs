//! A notebook saved before the cell kinds and labels were renamed (`itemnumbered`, `label=3`, `#% output n=3`)
//! opens with the same meaning and is written back with the current names (`item_numbered`, `in=3`, `out=3`).
use mc_nbfile::{Kind, Notebook};

fn data(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name);
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn an_old_notebook_is_converted_on_load() {
    let (nb, warnings) = Notebook::read(&data("old-names.mcn")).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(nb.print(), data("new-names.mcn"));
    use Kind::*;
    let kinds: Vec<_> = nb.cells.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [
            Title, Subtitle, Chapter, Section, Text, Input, Input, Code, Item, ItemParagraph, Subitem,
            SubitemParagraph, Subsubitem, ItemNumbered, SubitemNumbered, SubsubitemNumbered, DisplayFormula, Program,
            Input
        ]
    );
    // the labels: the input numbers and the output numbers, under their new names
    let ins: Vec<Option<u32>> = nb.cells.iter().map(|c| c.attrs.number("in")).collect();
    assert_eq!(ins.iter().flatten().copied().collect::<Vec<_>>(), [1, 2, 5, 7]);
    let outs: Vec<u32> = nb.cells.iter().flat_map(|c| c.outputs.iter().filter_map(|b| b.attrs.number("out"))).collect();
    assert_eq!(outs, [1, 3, 4, 7]);
    assert!(nb.cells.iter().all(|c| !c.attrs.has("label") && c.outputs.iter().all(|b| !b.attrs.has("n"))));
    // other attributes stay where they were
    assert!(nb.cells[5].attrs.has("typeset") && nb.cells[18].attrs.has("closed"));
    assert_eq!(nb.cells[6].outputs[2].attrs.get("size"), Some("360x225"));
}

#[test]
fn a_new_notebook_reads_back_unchanged() {
    let text = data("new-names.mcn");
    let (nb, warnings) = Notebook::read(&text).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(nb.print(), text);
    // every kind is written with its current name and read back from it
    for k in Kind::ALL {
        assert_eq!(Kind::from_name(k.name()), Some(k));
        assert!(k.name().chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{}", k.name());
    }
}

#[test]
fn the_old_kind_names_are_still_read() {
    for (old, kind) in [
        ("itemparagraph", Kind::ItemParagraph),
        ("subitemparagraph", Kind::SubitemParagraph),
        ("itemnumbered", Kind::ItemNumbered),
        ("subitemnumbered", Kind::SubitemNumbered),
        ("subsubitemnumbered", Kind::SubsubitemNumbered),
        ("displayformula", Kind::DisplayFormula),
        ("input", Kind::Input),
    ] {
        assert_eq!(Kind::from_name(old), Some(kind), "{old}");
        let text = format!("#! mathcas 2 notebook\n\n#% {old}\n# a\n");
        let nb = Notebook::parse(&text).unwrap();
        assert_eq!(nb.cells[0].kind, kind);
    }
    // a label already under the new name wins over an old one in the same marker
    let nb = Notebook::parse("#! mathcas 2 notebook\n\n#% cell in=4 label=2\nx\n#% output out=4 n=2\n#= x\n").unwrap();
    assert_eq!(nb.cells[0].attrs.number::<u32>("in"), Some(4));
    assert_eq!(nb.cells[0].outputs[0].attrs.number::<u32>("out"), Some(4));
}
