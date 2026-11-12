//! Datasets from and to files: type inference per column, missing values, quoting, the JSON shapes, round trips in
//! every text format, a large file on the packed path, and the failures.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// A folder of its own for one test, removed when dropped.
struct Dir(std::path::PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("mc-io-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    /// A file in the folder: written with `text`, and its path as the engine reads it.
    fn file(&self, name: &str, text: &str) -> String {
        let p = self.0.join(name);
        std::fs::write(&p, text).unwrap();
        p.to_string_lossy().replace('\\', "/")
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().replace('\\', "/")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// The term of the value of `src` as text (datasets are shown as tables).
fn term(e: &mut CoreEngine, src: &str) -> String {
    let s = eval(e, &format!("to_string({src})"));
    s.strip_prefix('"').and_then(|s| s.strip_suffix('"')).map_or(s.clone(), |s| s.replace("\\\"", "\"").replace("\\\\", "\\"))
}

#[test]
fn tables_with_inferred_types() {
    let d = Dir::new("ds-types");
    let mut e = CoreEngine::new();
    let csv = d.file(
        "a.csv",
        "id,name,score,ok,note,code\n1,Ann,2.5,true,\"a, b\",007\n2, Bob ,,false,,12\n3,\"Cy \"\"C\"\"\",4,true,\"two\nlines\",x\n",
    );
    assert_eq!(
        term(&mut e, &format!("import_data(\"{csv}\", \"dataset\")")),
        "dataset(assoc(\"id\" -> [1, 2, 3], \"name\" -> [\"Ann\", \"Bob\", \"Cy \\\"C\\\"\"], \"score\" -> [2.5, missing(), 4.0], \
         \"ok\" -> [true, false, true], \"note\" -> [\"a, b\", missing(), \"two\\nlines\"], \"code\" -> [\"007\", \"12\", \"x\"]))"
    );
    // a header with empty and repeated names; short records end in missing values; blank lines and CRLF
    let csv = d.file("b.csv", "a,,a\r\n1,2,3\r\n\r\n4\r\n");
    assert_eq!(
        term(&mut e, &format!("import_data(\"{csv}\", \"dataset\")")),
        "dataset(assoc(\"a\" -> [1, 4], \"column_2\" -> [2, missing()], \"a_2\" -> [3, missing()]))"
    );
    // TSV and whitespace tables (no quoting)
    let tsv = d.file("c.tsv", "x\ty\n1\t\"q\"\n2\t\n");
    assert_eq!(
        term(&mut e, &format!("import_data(\"{tsv}\", \"dataset\")")),
        "dataset(assoc(\"x\" -> [1, 2], \"y\" -> [\"\\\"q\\\"\", missing()]))"
    );
    let dat = d.file("d.dat", "t  v\n0  1.5\n1  -2e3\n");
    assert_eq!(term(&mut e, &format!("import_data(\"{dat}\", \"dataset\")")), "dataset(assoc(\"t\" -> [0, 1], \"v\" -> [1.5, -2000.0]))");
    // a format given before the element
    let out = d.file("e.out", "p,q\n1,2\n");
    assert_eq!(term(&mut e, &format!("import_data(\"{out}\", csv, \"dataset\")")), "dataset(assoc(\"p\" -> [1], \"q\" -> [2]))");
    // a header alone, an empty file
    let h = d.file("f.csv", "p,q\n");
    assert_eq!(term(&mut e, &format!("import_data(\"{h}\", \"dataset\")")), "dataset(assoc(\"p\" -> [], \"q\" -> []))");
    let empty = d.file("g.csv", "");
    assert_eq!(term(&mut e, &format!("import_data(\"{empty}\", \"dataset\")")), "dataset(assoc())");
    // the shown value is the table
    assert_eq!(eval(&mut e, &format!("import_data(\"{out}\", csv, \"dataset\")")), "  p |   q\n----+----\n  1 |   2\ndataset: 1 row, 2 columns");
}

#[test]
fn json_shapes() {
    let d = Dir::new("ds-json");
    let mut e = CoreEngine::new();
    // rows (keys in file order, absent keys and null missing), columns, and arrays under a header
    let rows = d.file("r.json", r#"[{"b": 1, "a": "x", "n": [1, 2]}, {"a": null, "c": 2.5}]"#);
    assert_eq!(
        term(&mut e, &format!("import_data(\"{rows}\", \"dataset\")")),
        "dataset(assoc(\"b\" -> [1, missing()], \"a\" -> [\"x\", missing()], \"n\" -> [[1, 2], missing()], \"c\" -> [missing(), 2.5]))"
    );
    let cols = d.file("c.json", r#"{"z": [1, 2], "y": ["s", null]}"#);
    assert_eq!(term(&mut e, &format!("import_data(\"{cols}\", \"dataset\")")), "dataset(assoc(\"z\" -> [1, 2], \"y\" -> [\"s\", missing()]))");
    let arrays = d.file("a.json", r#"[["u", "v"], [1, true], [2, null]]"#);
    assert_eq!(term(&mut e, &format!("import_data(\"{arrays}\", \"dataset\")")), "dataset(assoc(\"u\" -> [1, 2], \"v\" -> [true, missing()]))");
    assert_eq!(term(&mut e, &format!("import_data(\"{}\", \"dataset\")", d.file("e.json", "[]"))), "dataset(assoc())");
    // shapes that are no table, and columns of different lengths
    let bad = d.file("b.json", "[1, 2]");
    assert_eq!(
        eval(&mut e, &format!("import_data(\"{bad}\", \"dataset\")")),
        format!("failure(damaged, \"{bad}\", \"a dataset is an array of objects, an object of arrays or an array of arrays\")")
    );
    let uneven = d.file("u.json", r#"{"a": [1], "b": [1, 2]}"#);
    assert_eq!(eval(&mut e, &format!("import_data(\"{uneven}\", \"dataset\")")), "failure(shape_mismatch, dataset)");
    let broken = d.file("x.json", "[{");
    assert!(eval(&mut e, &format!("import_data(\"{broken}\", \"dataset\")")).starts_with("failure(io, "));
}

#[test]
fn round_trips() {
    let d = Dir::new("ds-round");
    let mut e = CoreEngine::new();
    eval(
        &mut e,
        "ds = dataset([assoc(\"i\" -> 1, \"x\" -> 0.5, \"s\" -> \"a,b\", \"t\" -> true), assoc(\"i\" -> 2, \"s\" -> \"say \\\"hi\\\"\", \"t\" -> false), \
         assoc(\"i\" -> 3, \"x\" -> -1.0e-12, \"s\" -> \"\", \"t\" -> true)]); null",
    );
    for ext in ["csv", "tsv", "json"] {
        let f = d.path(&format!("t.{ext}"));
        assert_eq!(eval(&mut e, &format!("export_data(\"{f}\", ds)")), format!("\"{f}\""), "{ext}");
        // an empty string is read back as missing in the delimited formats
        let back = term(&mut e, &format!("import_data(\"{f}\", \"dataset\")"));
        let s = if ext == "json" { "\"a,b\", \"say \\\"hi\\\"\", \"\"" } else { "\"a,b\", \"say \\\"hi\\\"\", missing()" };
        assert_eq!(
            back,
            format!("dataset(assoc(\"i\" -> [1, 2, 3], \"x\" -> [0.5, missing(), -1e-12], \"s\" -> [{s}], \"t\" -> [true, false, true]))"),
            "{ext}"
        );
    }
    let csv = std::fs::read_to_string(d.path("t.csv")).unwrap();
    assert_eq!(csv, "i,x,s,t\n1,0.5,\"a,b\",true\n2,,\"say \"\"hi\"\"\",false\n3,-1e-12,,true\n");
    let json = std::fs::read_to_string(d.path("t.json")).unwrap();
    assert!(json.starts_with("[\n  {\"i\": 1, \"x\": 0.5, \"s\": \"a,b\", \"t\": true},\n  {\"i\": 2, \"x\": null,"), "{json}");
    // non-string column names are written as their text; an empty dataset is a header or []
    let f = d.path("n.csv");
    eval(&mut e, &format!("export_data(\"{f}\", dataset(assoc(a -> [1], 2 -> [x])))"));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "a,2\n1,x\n");
    let f = d.path("z.json");
    eval(&mut e, &format!("export_data(\"{f}\", dataset([]))"));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "[]\n");
    // lists keep their former meaning
    let f = d.path("l.csv");
    eval(&mut e, &format!("export_data(\"{f}\", [[1, 2], [3, 4]])"));
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\")")), "[[1, 2], [3, 4]]");
}

#[test]
fn large_file_on_the_packed_path() {
    let d = Dir::new("ds-large");
    let mut e = CoreEngine::new();
    let n = 20_000;
    let mut text = String::from("k,v,w\n");
    for i in 0..n {
        text.push_str(&format!("{},{},{}\n", i % 7, i, i as f64 / 4.0));
    }
    let f = d.file("big.csv", &text);
    eval(&mut e, &format!("ds = import_data(\"{f}\", \"dataset\"); null"));
    assert_eq!(eval(&mut e, "length(ds)"), n.to_string());
    // numeric columns without missing values are packed arrays: equal to the lists built from numbers
    assert_eq!(eval(&mut e, &format!("column(ds, \"v\") === range(0, {})", n - 1)), "true");
    assert_eq!(eval(&mut e, "total(column(ds, \"w\"))"), format!("{:?}", (0..n).map(|i| i as f64 / 4.0).sum::<f64>()));
    assert_eq!(eval(&mut e, "column(aggregate(ds, \"k\", [count]), \"count\")"), "[2858, 2857, 2857, 2857, 2857, 2857, 2857]");
    let out = d.path("out.csv");
    eval(&mut e, &format!("export_data(\"{out}\", ds)"));
    assert_eq!(std::fs::read_to_string(&out).unwrap().replace(".0\n", "\n").replace(".0,", ","), text.replace(".0\n", "\n"));
}

#[test]
fn failures() {
    let d = Dir::new("ds-fail");
    let mut e = CoreEngine::new();
    let long = d.file("long.csv", "a,b\n1,2\n1,2,3\n");
    assert_eq!(
        eval(&mut e, &format!("import_data(\"{long}\", \"dataset\")")),
        format!("failure(damaged, \"{long}\", \"record 3 has 3 fields, the header 2\")")
    );
    // empty fields past the header are allowed
    let trailing = d.file("t.csv", "a,b\n1,2,\n");
    assert_eq!(term(&mut e, &format!("import_data(\"{trailing}\", \"dataset\")")), "dataset(assoc(\"a\" -> [1], \"b\" -> [2]))");
    let f = d.file("x.csv", "a\n1\n");
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", \"rows\")")), "failure(unknown_element, \"rows\", [\"dataset\"])");
    assert_eq!(eval(&mut e, &format!("import_data(\"{f}\", lines, \"dataset\")")), "failure(unknown_element, \"dataset\", [])");
    assert!(eval(&mut e, "import_data(\"no/such/file.csv\", \"dataset\")").starts_with("failure(io, "));
    let latin = d.0.join("latin.csv");
    std::fs::write(&latin, b"a\n\xe9\n").unwrap();
    let latin = latin.to_string_lossy().replace('\\', "/");
    assert!(eval(&mut e, &format!("import_data(\"{latin}\", \"dataset\")")).starts_with("failure(io, "));
}
