//! Datasets (`mc-assoc`) from and to text files: `import_data(file, "dataset")` for CSV, TSV, whitespace tables and
//! JSON, and `export_data` of a dataset to those formats.
//!
//! **Tables** (decision DATA.11): the first non-blank record names the columns (an empty name becomes `column_k`, a
//! repeated one gets `_2`, `_3`, ...); each column's type is inferred from all its fields: integers if every field
//! is one (a packed column when none is missing), else reals if every field is a number, else `true`/`false` if every
//! field is one of those, else strings (every field as text, numbers included). Fields are trimmed; an empty field is
//! `missing()`. A record shorter than the header ends in missing values; a longer one (beyond empty trailing fields)
//! is `failure(damaged, file, message)`. CSV fields may be quoted (`""` is a quote inside quotes, and quoted fields
//! may span lines); TSV fields are the text between tabs.
//!
//! **JSON**: an array of objects (rows; a key absent from an object is missing), an object of arrays (columns), or
//! an array of arrays whose first array names the columns; `null` cells are missing.
use crate::tabular::{text, to_json};
use crate::{failure, Format};
use mc_api::names;
use mc_assoc::Column;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Res};
use std::borrow::Cow;
use std::fmt::Write as _;

/// The records of a text: every field in one vector (no allocation per record), and where each record ends.
#[derive(Default)]
struct Records<'a> {
    fields: Vec<Cow<'a, str>>,
    ends: Vec<usize>,
}

impl<'a> Records<'a> {
    fn len(&self) -> usize {
        self.ends.len()
    }

    /// The fields of record `r`.
    fn get(&self, r: usize) -> &[Cow<'a, str>] {
        let start = if r == 0 { 0 } else { self.ends[r - 1] };
        &self.fields[start..self.ends[r]]
    }

    /// End the record whose fields were pushed since the last one; a blank one (a single empty field) is dropped.
    fn end(&mut self) {
        let start = self.ends.last().copied().unwrap_or(0);
        let blank = self.fields.len() == start + 1 && self.fields[start].trim().is_empty();
        if blank {
            self.fields.truncate(start);
        } else if self.fields.len() > start {
            self.ends.push(self.fields.len());
        }
    }
}

/// The records of a CSV text: fields separated by `sep`, double quotes around a field let it hold separators,
/// quotes (`""`) and line breaks. Blank lines give no record.
fn csv_records(text: &str, sep: u8) -> Records<'_> {
    let b = text.as_bytes();
    let mut out = Records::default();
    let mut i = 0;
    let n = b.len();
    while i < n {
        // one field starting at i
        if b[i] == b'"' {
            let mut s = String::new();
            let mut j = i + 1;
            let mut start = j;
            loop {
                match b[j..].iter().position(|&c| c == b'"') {
                    None => {
                        s.push_str(&text[start..]);
                        j = n;
                        break;
                    }
                    Some(p) => {
                        s.push_str(&text[start..j + p]);
                        j += p + 1;
                        if j < n && b[j] == b'"' {
                            s.push('"');
                            j += 1;
                            start = j;
                        } else {
                            break;
                        }
                    }
                }
            }
            // anything after the closing quote up to the separator belongs to the field
            let end = b[j..].iter().position(|&c| c == sep || c == b'\n').map_or(n, |p| j + p);
            s.push_str(text[j..end].trim_end_matches('\r'));
            out.fields.push(Cow::Owned(s));
            i = end;
        } else {
            let end = b[i..].iter().position(|&c| c == sep || c == b'\n').map_or(n, |p| i + p);
            out.fields.push(Cow::Borrowed(text[i..end].trim_end_matches('\r')));
            i = end;
        }
        if i < n && b[i] == sep {
            i += 1;
            if i == n {
                out.fields.push(Cow::Borrowed(""));
                out.end();
            }
            continue;
        }
        // the end of a record
        i += 1;
        out.end();
    }
    out
}

/// The records of a text split at line breaks and then into fields by `split`; blank lines give none.
fn line_records<'a, I: Iterator<Item = &'a str>>(text: &'a str, split: impl Fn(&'a str) -> I) -> Records<'a> {
    let mut out = Records::default();
    for l in text.lines().filter(|l| !l.trim().is_empty()) {
        out.fields.extend(split(l).map(Cow::Borrowed));
        out.ends.push(out.fields.len());
    }
    out
}

/// The type of a column, inferred from its fields.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Int,
    Real,
    Bool,
    Str,
}

fn parse_real(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|x| x.is_finite() && s.bytes().any(|c| c.is_ascii_digit()))
}

/// The type of the fields `fs` (empty ones skipped).
fn kind_of(fs: &[&str]) -> Kind {
    let (mut int, mut num, mut boolean) = (true, true, true);
    for f in fs.iter().filter(|f| !f.is_empty()) {
        if int && f.parse::<i64>().is_err() {
            int = false;
        }
        if !int && num && parse_real(f).is_none() {
            num = false;
        }
        if boolean && *f != "true" && *f != "false" {
            boolean = false;
        }
        if !(int || num || boolean) {
            return Kind::Str;
        }
    }
    match (int, num, boolean) {
        (true, ..) => Kind::Int,
        (_, true, _) => Kind::Real,
        (_, _, true) => Kind::Bool,
        _ => Kind::Str,
    }
}

/// The column of the fields `fs` as a list of values of the inferred type.
fn column<'h>(h: &'h Heap, fs: &[&str]) -> T<'h> {
    let missing = mc_assoc::missing_value(h);
    let list = h.sym(names::LIST);
    let any_missing = fs.iter().any(|f| f.is_empty());
    match kind_of(fs) {
        Kind::Int if !any_missing => h.ints(&fs.iter().map(|f| f.parse::<i64>().unwrap_or(0)).collect::<Vec<_>>()),
        Kind::Real if !any_missing => h.reals(&fs.iter().map(|f| parse_real(f).unwrap_or(0.0)).collect::<Vec<_>>()),
        kind => {
            let (t, f) = (h.sym_term(h.sym(names::TRUE)), h.sym_term(h.sym(names::FALSE)));
            let v: Vec<T<'h>> = fs
                .iter()
                .map(|s| match kind {
                    _ if s.is_empty() => missing,
                    Kind::Int => h.int(s.parse::<i64>().unwrap_or(0)),
                    Kind::Real => h.real(parse_real(s).unwrap_or(0.0)),
                    Kind::Bool => {
                        if *s == "true" {
                            t
                        } else {
                            f
                        }
                    }
                    Kind::Str => h.string(s),
                })
                .collect();
            h.call(list, &v)
        }
    }
}

/// Column names from a header: trimmed, empty ones named by position, repeated ones numbered.
fn header_names(header: &[Cow<'_, str>]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(header.len());
    for (k, f) in header.iter().enumerate() {
        let base = match f.trim() {
            "" => format!("column_{}", k + 1),
            s => s.to_string(),
        };
        let mut name = base.clone();
        let mut i = 2;
        while out.contains(&name) {
            name = format!("{base}_{i}");
            i += 1;
        }
        out.push(name);
    }
    out
}

/// The dataset of the text `text` in a table format, or the failure for it.
pub(crate) fn import_table<'h>(cx: &mut NativeCx<'_, '_, 'h>, file: T<'h>, text: &str, format: Format) -> Res<T<'h>> {
    let h = cx.heap();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let records = match format {
        Format::Csv => csv_records(text, b','),
        Format::Tsv => line_records(text, |l| l.split('\t').map(|f| f.trim_end_matches('\r'))),
        _ => line_records(text, str::split_whitespace),
    };
    cx.tick(1 + records.len() as u64 / 64)?;
    if records.len() == 0 {
        return Ok(mc_assoc::dataset_of_columns(h, &[], &[]));
    }
    let names = header_names(records.get(0));
    let rest = (1..records.len()).map(|r| records.get(r));
    let m = names.len();
    // the fields column by column
    let mut cols: Vec<Vec<&str>> = (0..m).map(|_| Vec::with_capacity(records.len())).collect();
    for (r, rec) in rest.enumerate() {
        if r % 4096 == 0 {
            cx.tick(1 + (4096 * m as u64) / 64)?;
        }
        if rec.len() > m && rec[m..].iter().any(|f| !f.trim().is_empty()) {
            let msg = format!("record {} has {} fields, the header {}", r + 2, rec.len(), m);
            return Ok(failure(h, "damaged", &[file, h.string(&msg)]));
        }
        for (j, c) in cols.iter_mut().enumerate() {
            c.push(rec.get(j).map_or("", |f| f.trim()));
        }
    }
    let mut terms = Vec::with_capacity(m);
    for c in &cols {
        cx.tick(1 + c.len() as u64 / 16)?;
        terms.push(column(h, c));
    }
    let names: Vec<T<'h>> = names.iter().map(|n| h.string(n)).collect();
    Ok(mc_assoc::dataset_of_columns(h, &names, &terms))
}

/// A JSON value whose objects keep their keys in file order (`serde_json`'s map sorts them), so that a dataset's
/// columns come in the order the file names them.
enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl<'de> serde::Deserialize<'de> for Json {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Json, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Json;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON value")
            }
            fn visit_unit<E>(self) -> Result<Json, E> {
                Ok(Json::Null)
            }
            fn visit_bool<E>(self, b: bool) -> Result<Json, E> {
                Ok(Json::Bool(b))
            }
            fn visit_i64<E>(self, i: i64) -> Result<Json, E> {
                Ok(Json::Int(i))
            }
            fn visit_u64<E>(self, u: u64) -> Result<Json, E> {
                Ok(i64::try_from(u).map_or(Json::Real(u as f64), Json::Int))
            }
            fn visit_f64<E>(self, x: f64) -> Result<Json, E> {
                Ok(Json::Real(x))
            }
            fn visit_str<E>(self, s: &str) -> Result<Json, E> {
                Ok(Json::Str(s.to_string()))
            }
            fn visit_string<E>(self, s: String) -> Result<Json, E> {
                Ok(Json::Str(s))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Json, A::Error> {
                let mut v = Vec::new();
                while let Some(x) = a.next_element()? {
                    v.push(x);
                }
                Ok(Json::Arr(v))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Json, A::Error> {
                let mut v = Vec::new();
                while let Some(kv) = a.next_entry()? {
                    v.push(kv);
                }
                Ok(Json::Obj(v))
            }
        }
        d.deserialize_any(V)
    }
}

/// A JSON value as a term, as `import_data` reads JSON (objects as lists of rules); a `null` cell is missing.
fn json_term<'h>(h: &'h Heap, j: &Json, cell: bool) -> T<'h> {
    match j {
        Json::Null if cell => mc_assoc::missing_value(h),
        Json::Null => h.sym_term(h.sym(names::NULL)),
        Json::Bool(b) => h.sym_term(h.sym(if *b { names::TRUE } else { names::FALSE })),
        Json::Int(i) => h.int(*i),
        Json::Real(x) => h.real(*x),
        Json::Str(s) => h.string(s),
        Json::Arr(a) => h.call(h.sym(names::LIST), &a.iter().map(|x| json_term(h, x, false)).collect::<Vec<_>>()),
        Json::Obj(o) => {
            let rule = h.sym(names::RULE);
            let v: Vec<T<'h>> = o.iter().map(|(k, x)| h.call(rule, &[h.string(k), json_term(h, x, false)])).collect();
            h.call(h.sym(names::LIST), &v)
        }
    }
}

/// A JSON value as a cell: `null` is missing.
fn json_cell<'h>(h: &'h Heap, j: &Json) -> T<'h> {
    json_term(h, j, true)
}

/// The dataset of a JSON text: rows (an array of objects), columns (an object of arrays), or an array of arrays
/// under a header; the term `dataset(...)` is evaluated, so its normal form and failures are `dataset`'s.
pub(crate) fn import_json<'h>(cx: &mut NativeCx<'_, '_, 'h>, file: T<'h>, text: &str) -> Res<T<'h>> {
    use Json as J;
    let h = cx.heap();
    let j = match serde_json::from_str::<J>(text) {
        Ok(j) => j,
        Err(e) => return Ok(failure(h, "io", &[file, h.string(&e.to_string())])),
    };
    cx.tick(1 + text.len() as u64 / 4096)?;
    let (list, rule, assoc) = (h.sym(names::LIST), h.sym(names::RULE), h.sym("assoc"));
    let pairs = |o: &[(String, J)], cell: &dyn Fn(&J) -> T<'h>| -> Vec<T<'h>> {
        o.iter().map(|(k, v)| h.call(rule, &[h.string(k), cell(v)])).collect()
    };
    let cells = |a: &[J]| h.call(list, &a.iter().map(|x| json_cell(h, x)).collect::<Vec<_>>());
    let arg = match &j {
        J::Arr(rows) if rows.iter().all(|r| matches!(r, J::Obj(_))) => {
            let v: Vec<T<'h>> = rows
                .iter()
                .filter_map(|r| if let J::Obj(o) = r { Some(h.call(assoc, &pairs(o, &|x| json_cell(h, x)))) } else { None })
                .collect();
            h.call(list, &v)
        }
        J::Arr(rows) if rows.iter().all(|r| matches!(r, J::Arr(_))) => {
            let v: Vec<T<'h>> = rows.iter().filter_map(|r| if let J::Arr(a) = r { Some(cells(a)) } else { None }).collect();
            h.call(list, &v)
        }
        J::Obj(o) if o.iter().all(|(_, v)| matches!(v, J::Arr(_))) => {
            let column = |x: &J| match x {
                J::Arr(a) => cells(a),
                _ => json_cell(h, x),
            };
            h.call(assoc, &pairs(o, &column))
        }
        _ => {
            let msg = "a dataset is an array of objects, an object of arrays or an array of arrays";
            return Ok(failure(h, "damaged", &[file, h.string(msg)]));
        }
    };
    cx.eval(h.call(h.sym("dataset"), &[arg]))
}

/// The text of a column name: a string as it is, anything else printed.
fn name_text<'h>(cx: &mut NativeCx<'_, '_, 'h>, name: T<'h>) -> Res<String> {
    text(cx, name)
}

/// A field of a delimited file, quoted when it holds the separator, a quote or a line break (CSV only).
fn push_field(out: &mut String, s: &str, sep: &str, quote: bool) {
    if quote && (s.contains(sep) || s.contains('"') || s.contains('\n') || s.contains('\r')) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

/// A dataset as delimited text: a header line, then a line per row; missing values are empty fields. `None` when
/// `data` is not a dataset.
pub(crate) fn delimited<'h>(cx: &mut NativeCx<'_, '_, 'h>, data: T<'h>, sep: &str, quote: bool) -> Res<Option<String>> {
    let h = cx.heap();
    let Some((names, cols)) = mc_assoc::dataset_columns(h, data) else { return Ok(None) };
    let n = cols.first().map_or(0, |c| c.len());
    let mut out = String::with_capacity(16 * n * cols.len().max(1));
    for (j, &c) in names.iter().enumerate() {
        if j > 0 {
            out.push_str(sep);
        }
        let s = name_text(cx, c)?;
        push_field(&mut out, &s, sep, quote);
    }
    out.push('\n');
    for i in 0..n {
        if i % 4096 == 0 {
            cx.tick(1 + (4096 * cols.len() as u64) / 64)?;
        }
        for (j, c) in cols.iter().enumerate() {
            if j > 0 {
                out.push_str(sep);
            }
            match c {
                Column::Ints(d) => {
                    let _ = write!(out, "{}", d[i]);
                }
                Column::Reals(d) => {
                    let _ = write!(out, "{:?}", d[i]);
                }
                Column::Terms(d) if mc_assoc::is_missing_value(h, d[i]) => {}
                Column::Terms(d) => {
                    let s = text(cx, d[i])?;
                    push_field(&mut out, &s, sep, quote);
                }
            }
        }
        out.push('\n');
    }
    Ok(Some(out))
}

/// A dataset as a JSON array of objects, one per row and line (missing values are `null`). `None` when `data` is not
/// a dataset.
pub(crate) fn json<'h>(cx: &mut NativeCx<'_, '_, 'h>, data: T<'h>) -> Res<Option<String>> {
    let h = cx.heap();
    let Some((names, cols)) = mc_assoc::dataset_columns(h, data) else { return Ok(None) };
    let n = cols.first().map_or(0, |c| c.len());
    let mut keys = Vec::with_capacity(names.len());
    for &c in &names {
        let s = name_text(cx, c)?;
        keys.push(serde_json::Value::String(s).to_string());
    }
    let mut out = String::from("[");
    for i in 0..n {
        if i % 4096 == 0 {
            cx.tick(1 + (4096 * cols.len() as u64) / 64)?;
        }
        out.push_str(if i == 0 { "\n  {" } else { ",\n  {" });
        for (j, c) in cols.iter().enumerate() {
            if j > 0 {
                out.push_str(", ");
            }
            out.push_str(&keys[j]);
            out.push_str(": ");
            match c {
                Column::Ints(d) => {
                    let _ = write!(out, "{}", d[i]);
                }
                Column::Reals(d) if d[i].is_finite() => {
                    let _ = write!(out, "{:?}", d[i]);
                }
                Column::Reals(_) => out.push_str("null"),
                Column::Terms(d) if mc_assoc::is_missing_value(h, d[i]) => out.push_str("null"),
                Column::Terms(d) => {
                    let v = to_json(cx, d[i])?;
                    out.push_str(&v.to_string());
                }
            }
        }
        out.push('}');
    }
    out.push_str(if n == 0 { "]\n" } else { "\n]\n" });
    Ok(Some(out))
}

/// Whether `t` is a dataset (its `View` is not needed).
pub(crate) fn is_dataset<'h>(h: &'h Heap, t: T<'h>) -> bool {
    h.packed(t).is_none() && matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym("dataset")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(text: &str) -> Vec<Vec<String>> {
        let r = csv_records(text, b',');
        (0..r.len()).map(|k| r.get(k).iter().map(|f| f.to_string()).collect()).collect()
    }

    #[test]
    fn csv_records_quotes_and_lines() {
        assert_eq!(fields("a,b\n1,2\n"), [["a", "b"], ["1", "2"]]);
        assert_eq!(fields("a,b\r\n1,2"), [["a", "b"], ["1", "2"]]);
        assert_eq!(fields("\"x,y\",\"say \"\"hi\"\"\"\n"), [["x,y", "say \"hi\""]]);
        assert_eq!(fields("\"two\nlines\",3\n"), [["two\nlines", "3"]]);
        assert_eq!(fields("a,\n,b\n\n\nc"), vec![vec!["a", ""], vec!["", "b"], vec!["c"]]);
        assert_eq!(fields(""), Vec::<Vec<String>>::new());
        assert_eq!(fields("\"open"), [["open"]]);
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(&["1", "", "-3"]), Kind::Int);
        assert_eq!(kind_of(&["1", "2.5", "1e3"]), Kind::Real);
        assert_eq!(kind_of(&["true", "false", ""]), Kind::Bool);
        assert_eq!(kind_of(&["1", "x"]), Kind::Str);
        assert_eq!(kind_of(&["1", "true"]), Kind::Str);
        assert_eq!(kind_of(&["nan", "1"]), Kind::Str);
        assert_eq!(kind_of(&["", ""]), Kind::Int);
    }

    #[test]
    fn header() {
        let h: Vec<Cow<'_, str>> = ["a", "", "a", " b ", "a"].into_iter().map(Cow::Borrowed).collect();
        assert_eq!(header_names(&h), ["a", "column_2", "a_2", "b", "a_3"]);
    }
}
