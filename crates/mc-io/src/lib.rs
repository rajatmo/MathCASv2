//! Import and export of data files, first version: lists as CSV, TSV, whitespace-separated text and JSON, and PNG
//! images (the `image(path)` values of `plot` and `list_plot`).
//!
//! The format comes from the file extension, or from a format symbol as the last argument
//! (`import_data("data.out", csv)`). Paths are relative to the current directory.
//!
//! | format | `import_data` gives | `export_data` takes |
//! |---|---|---|
//! | `csv`, `tsv` | a list of rows, each a list of fields | a list of rows (lists), or of values (one per line) |
//! | `txt`, `dat` (`table`) | a list of lines, each a list of whitespace-separated fields | as csv, fields separated by spaces |
//! | `lines` | a list of strings, one per line | a list of values, one per line |
//! | `json` | lists, numbers, strings, `true`/`false`, `null`; objects as lists of rules `key -> value` | the same |
//! | `png` | `image(path)` | an `image(path)` value: the PNG is copied |
//!
//! Fields that read as integers become integers, then machine reals, else strings (quotes in CSV removed).
//! Exported values: integers and machine reals as numbers (reals keep a decimal point or exponent, so they read
//! back as reals), strings as text, anything else as its printed form. Both functions are effects (they run in
//! program order), and imported values are never memoised.
#![warn(missing_docs)]

use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "import_data",
        "import_data(file), import_data(file, format): the data of a file: csv, tsv, txt/dat (lists of rows), lines, json, png (image(path)); the format from the extension unless given",
    ),
    (
        "export_data",
        "export_data(file, data), export_data(file, data, format): write data (lists of rows or values; json; an image(path) as png); gives the file name",
    ),
];

/// Register the natives with `k`.
pub fn install(k: &mut Kernel, h: &Heap) {
    k.register(h.sym("import_data"), Attributes::default(), Box::new(Import));
    k.register(h.sym("export_data"), Attributes::default(), Box::new(Export));
}

fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Format {
    Csv,
    Tsv,
    Table,
    Lines,
    Json,
    Png,
}

impl Format {
    fn named(s: &str) -> Option<Format> {
        Some(match s.to_ascii_lowercase().as_str() {
            "csv" => Format::Csv,
            "tsv" => Format::Tsv,
            "txt" | "dat" | "table" => Format::Table,
            "lines" => Format::Lines,
            "json" => Format::Json,
            "png" => Format::Png,
            _ => return None,
        })
    }
}

/// The file name and format of `args` (`[file]` or `[file, format]` after `skip` data arguments).
fn target<'h>(h: &'h Heap, f: &str, file: T<'h>, fmt: Option<T<'h>>) -> Result<(String, Format), T<'h>> {
    let View::Str(path) = h.view(file) else {
        return Err(failure(h, "argument", &[h.sym_term(h.sym(f)), h.int(1)]));
    };
    let format = match fmt.map(|t| h.view(t)) {
        Some(View::Sym(s)) => Format::named(&h.sym_name(s)),
        Some(View::Str(s)) => Format::named(s),
        Some(_) => None,
        None => std::path::Path::new(path).extension().and_then(|e| e.to_str()).and_then(Format::named),
    };
    match format {
        Some(fm) => Ok((path.to_string(), fm)),
        None => Err(failure(h, "unknown_format", &[file])),
    }
}

/// `import_data(file)`, `import_data(file, format)`.
struct Import;

impl Native for Import {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (file, fmt) = match args {
            [f] => (*f, None),
            [f, m] => (*f, Some(*m)),
            _ => return Ok(Reply::Decline),
        };
        let (path, format) = match target(h, "import_data", file, fmt) {
            Ok(v) => v,
            Err(f) => return Ok(Reply::Done(f)),
        };
        // reading a file depends on the world: in program order, never memoised
        cx.effect()?;
        cx.volatile();
        if format == Format::Png {
            let ok = std::fs::read(&path).map(|b| b.starts_with(b"\x89PNG")).unwrap_or(false);
            return Ok(Reply::Done(if ok {
                let abs = std::fs::canonicalize(&path).map(|p| p.to_string_lossy().into_owned()).unwrap_or(path);
                h.call(h.sym("image"), &[h.string(abs.trim_start_matches(r"\\?\").replace('\\', "/").as_str())])
            } else {
                failure(h, "io", &[file, h.string("not a PNG file")])
            }));
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return Ok(Reply::Done(failure(h, "io", &[file, h.string(&e.to_string())]))),
        };
        cx.tick(1 + text.len() as u64 / 4096)?;
        let v = match format {
            Format::Csv => rows(h, &text, |l| split_csv(l, ',')),
            // TSV has no quoting: fields are the text between tabs
            Format::Tsv => rows(h, &text, |l| l.split('\t').map(str::to_string).collect()),
            Format::Table => rows(h, &text, |l| l.split_whitespace().map(str::to_string).collect()),
            Format::Lines => {
                let ls: Vec<T<'h>> = text.lines().map(|l| h.string(l)).collect();
                h.call(h.sym(names::LIST), &ls)
            }
            Format::Json => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(j) => from_json(h, &j),
                Err(e) => failure(h, "io", &[file, h.string(&e.to_string())]),
            },
            Format::Png => unreachable!("handled above"),
        };
        Ok(Reply::Done(v))
    }
}

/// The non-empty lines of `text` as lists of fields.
fn rows<'h>(h: &'h Heap, text: &str, split: impl Fn(&str) -> Vec<String>) -> T<'h> {
    let list = h.sym(names::LIST);
    let rs: Vec<T<'h>> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let fs: Vec<T<'h>> = split(l).iter().map(|f| field(h, f)).collect();
            h.call(list, &fs)
        })
        .collect();
    h.call(list, &rs)
}

/// One CSV line split at `sep`, with double-quoted fields ("" inside quotes is a quote).
fn split_csv(line: &str, sep: char) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (vec![], String::new(), false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == sep && !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// A field: an integer, a machine real, or a string.
fn field<'h>(h: &'h Heap, f: &str) -> T<'h> {
    let s = f.trim();
    if let Ok(i) = s.parse::<i64>() {
        return h.int(i);
    }
    if let Ok(x) = s.parse::<f64>() {
        if x.is_finite() && s.chars().any(|c| c.is_ascii_digit()) {
            return h.real(x);
        }
    }
    h.string(s)
}

fn from_json<'h>(h: &'h Heap, j: &serde_json::Value) -> T<'h> {
    use serde_json::Value as J;
    match j {
        J::Null => h.sym_term(h.sym(names::NULL)),
        J::Bool(b) => h.sym_term(h.sym(if *b { names::TRUE } else { names::FALSE })),
        J::Number(n) => match n.as_i64() {
            Some(i) => h.int(i),
            None => h.real(n.as_f64().unwrap_or(f64::NAN)),
        },
        J::String(s) => h.string(s),
        J::Array(a) => {
            let v: Vec<T<'h>> = a.iter().map(|x| from_json(h, x)).collect();
            h.call(h.sym(names::LIST), &v)
        }
        J::Object(o) => {
            let rule = h.sym(names::RULE);
            let v: Vec<T<'h>> = o.iter().map(|(k, x)| h.call(rule, &[h.string(k), from_json(h, x)])).collect();
            h.call(h.sym(names::LIST), &v)
        }
    }
}

/// `export_data(file, data)`, `export_data(file, data, format)`.
struct Export;

impl Native for Export {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (file, data, fmt) = match args {
            [f, d] => (*f, *d, None),
            [f, d, m] => (*f, *d, Some(*m)),
            _ => return Ok(Reply::Decline),
        };
        let (path, format) = match target(h, "export_data", file, fmt) {
            Ok(v) => v,
            Err(f) => return Ok(Reply::Done(f)),
        };
        cx.effect()?;
        let written: Result<(), String> = match format {
            Format::Png => match image_path(h, data) {
                Some(src) => std::fs::copy(src, &path).map(|_| ()).map_err(|e| e.to_string()),
                None => return Ok(Reply::Done(failure(h, "argument", &[h.sym_term(h.sym("export_data")), h.int(2)]))),
            },
            Format::Json => {
                let j = to_json(cx, data)?;
                serde_json::to_string_pretty(&j).map_err(|e| e.to_string()).and_then(|s| write(&path, s))
            }
            _ => {
                let sep = match format {
                    Format::Csv => ",",
                    Format::Tsv => "\t",
                    _ => " ",
                };
                let text = delimited(cx, data, sep, format == Format::Csv)?;
                match text {
                    Some(t) => write(&path, t),
                    None => {
                        return Ok(Reply::Done(failure(h, "argument", &[h.sym_term(h.sym("export_data")), h.int(2)])))
                    }
                }
            }
        };
        Ok(Reply::Done(match written {
            Ok(()) => file,
            Err(e) => failure(h, "io", &[file, h.string(&e)]),
        }))
    }
}

fn write(path: &str, s: String) -> Result<(), String> {
    std::fs::write(path, s).map_err(|e| e.to_string())
}

/// The file of an `image(path)` value.
fn image_path<'h>(h: &'h Heap, t: T<'h>) -> Option<&'h str> {
    let View::App { head, args: [p] } = h.view(t) else { return None };
    if head != h.sym_term(h.sym("image")) {
        return None;
    }
    match h.view(*p) {
        View::Str(s) => Some(s),
        _ => None,
    }
}

/// A value as text: numbers directly (reals keep a point or an exponent), strings as they are, anything else
/// through `to_string` (its printed form).
fn text<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<String> {
    let h = cx.heap();
    Ok(match h.view(t) {
        View::Int(i) => i.to_string(),
        View::Real(x) => format!("{x:?}"),
        View::Str(s) => s.to_string(),
        _ => match h.view(cx.eval(h.call(h.sym("to_string"), &[t]))?) {
            View::Str(s) => s.to_string(),
            _ => String::from("?"),
        },
    })
}

/// Rows (a list of lists) or values (a list), one per line; `None` when `data` is not a list.
fn delimited<'h>(cx: &mut NativeCx<'_, '_, 'h>, data: T<'h>, sep: &str, quote: bool) -> Res<Option<String>> {
    let Some(rows) = cx.as_list(data) else { return Ok(None) };
    let mut out = String::new();
    for (i, &r) in rows.iter().enumerate() {
        if i % 1024 == 0 {
            cx.tick(1)?;
        }
        let fields: Vec<T<'h>> = cx.as_list(r).map(|f| f.to_vec()).unwrap_or_else(|| vec![r]);
        for (k, &f) in fields.iter().enumerate() {
            if k > 0 {
                out.push_str(sep);
            }
            let s = text(cx, f)?;
            if quote && (s.contains(',') || s.contains('"') || s.contains('\n')) {
                out.push('"');
                out.push_str(&s.replace('"', "\"\""));
                out.push('"');
            } else {
                out.push_str(&s);
            }
        }
        out.push('\n');
    }
    Ok(Some(out))
}

fn to_json<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<serde_json::Value> {
    use serde_json::Value as J;
    let h = cx.heap();
    cx.tick(1)?;
    Ok(match h.view(t) {
        View::Int(i) => J::from(i),
        View::Real(x) => serde_json::Number::from_f64(x).map(J::Number).unwrap_or(J::Null),
        View::Str(s) => J::String(s.to_string()),
        View::Sym(s) if h.sym_name(s) == names::TRUE => J::Bool(true),
        View::Sym(s) if h.sym_name(s) == names::FALSE => J::Bool(false),
        View::Sym(s) if h.sym_name(s) == names::NULL => J::Null,
        View::App { head, args } if head == h.sym_term(h.sym(names::LIST)) => {
            // a list of rules with string keys is an object
            let rule = h.sym_term(h.sym(names::RULE));
            let pairs: Option<Vec<(String, T<'h>)>> = args
                .iter()
                .map(|&a| match h.view(a) {
                    View::App { head, args: [k, v] } if head == rule => match h.view(*k) {
                        View::Str(k) => Some((k.to_string(), *v)),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            match pairs {
                Some(ps) if !ps.is_empty() => {
                    let mut o = serde_json::Map::new();
                    for (k, v) in ps {
                        o.insert(k, to_json(cx, v)?);
                    }
                    J::Object(o)
                }
                _ => {
                    let mut v = Vec::with_capacity(args.len());
                    for &a in args {
                        v.push(to_json(cx, a)?);
                    }
                    J::Array(v)
                }
            }
        }
        _ => J::String(text(cx, t)?),
    })
}
