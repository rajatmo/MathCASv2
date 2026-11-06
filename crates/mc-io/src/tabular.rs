//! The text formats: CSV, TSV, whitespace tables, lines and JSON.

use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The non-empty lines of `text` as lists of fields.
pub(crate) fn rows<'h>(h: &'h Heap, text: &str, split: impl Fn(&str) -> Vec<String>) -> T<'h> {
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
pub(crate) fn split_csv(line: &str, sep: char) -> Vec<String> {
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

pub(crate) fn from_json<'h>(h: &'h Heap, j: &serde_json::Value) -> T<'h> {
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

/// A value as text: numbers directly (reals keep a point or an exponent), strings as they are, anything else
/// through `to_string` (its printed form).
pub(crate) fn text<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<String> {
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
pub(crate) fn delimited<'h>(cx: &mut NativeCx<'_, '_, 'h>, data: T<'h>, sep: &str, quote: bool) -> Res<Option<String>> {
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

pub(crate) fn to_json<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<serde_json::Value> {
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
