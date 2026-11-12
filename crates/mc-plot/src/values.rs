//! Values of the rest of the system read as plot data: quantities (the magnitude, the unit kept for the axis
//! label), `around(v, e)` (the value with an error bar), `interval([a, b])` (its middle, with a bar over the
//! interval), datasets (their columns) and associations (their values, the keys as names).
use crate::real;
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// A number read from a data value, with what goes with it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Reading<'h> {
    /// the value (`None`: not a finite real)
    pub(crate) v: Option<f64>,
    /// the ends of an error bar or of an interval
    pub(crate) bar: Option<(f64, f64)>,
    /// the unit of a quantity, as text, and as the term
    pub(crate) unit: Option<String>,
    pub(crate) unit_term: Option<T<'h>>,
}

/// The head name and arguments of `t` when it is a call of a symbol.
pub(crate) fn call_of<'h, S: Store + ?Sized>(h: &'h S, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    let View::App { head, args } = h.view(t) else { return None };
    let View::Sym(s) = h.view(head) else { return None };
    Some((h.sym_name(s).to_string(), args))
}

/// The unit of a quantity as label text: a string as it is, an expression printed.
pub(crate) fn unit_text<'h>(cx: &NativeCx<'_, '_, 'h>, u: T<'h>) -> String {
    let h = cx.heap();
    match h.view(u) {
        View::Str(s) => s.to_string(),
        _ => mc_syntax::Printer::new(h).expr(u),
    }
}

/// `t` read as a number: a real, a numeric expression, `quantity(m, u)`, `around(v, e)`, `around(v, [e-, e+])`
/// or `interval([a, b])`.
pub(crate) fn read<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Reading<'h>> {
    read_in(cx, t, None)
}

/// [`read`], a quantity in another unit than `target` converted to `target` (`quantity(30, "cm")` read in metres is
/// 0.3).
pub(crate) fn read_in<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, target: Option<T<'h>>) -> Res<Reading<'h>> {
    let h = cx.heap();
    match h.view(t) {
        View::Int(i) => return Ok(Reading { v: Some(i as f64), ..Reading::default() }),
        View::Real(x) => return Ok(Reading { v: Some(x).filter(|x| x.is_finite()), ..Reading::default() }),
        _ => {}
    }
    match call_of(h, t) {
        Some((n, &[m, u])) if n == "quantity" => {
            let mut r = match target {
                Some(to) if to != u => {
                    let v = cx.eval(h.call(h.sym("quantity_magnitude"), &[t, to]))?;
                    let mut r = read(cx, v)?;
                    if r.v.is_none() {
                        // not convertible: the magnitude as it is
                        r = read(cx, m)?;
                    }
                    r
                }
                _ => read(cx, m)?,
            };
            r.unit = Some(unit_text(cx, target.unwrap_or(u)));
            r.unit_term = Some(target.unwrap_or(u));
            Ok(r)
        }
        Some((n, &[v, e])) if n == "around" => {
            let v = real(cx, v)?;
            let (lo, hi) = match cx.as_list(e) {
                Some(&[a, b]) => (real(cx, a)?, real(cx, b)?),
                _ => {
                    let e = real(cx, e)?;
                    (e, e)
                }
            };
            let bar = match (v, lo, hi) {
                (Some(v), Some(lo), Some(hi)) => Some((v - lo.abs(), v + hi.abs())),
                _ => None,
            };
            Ok(Reading { v, bar, ..Reading::default() })
        }
        Some((n, &[i])) if n == "interval" => {
            let ends = match cx.as_list(i) {
                Some(&[a, b]) => (real(cx, a)?, real(cx, b)?),
                _ => (None, None),
            };
            Ok(match ends {
                (Some(a), Some(b)) => Reading { v: Some(0.5 * (a + b)), bar: Some((a.min(b), a.max(b))), ..Reading::default() },
                _ => Reading::default(),
            })
        }
        _ => Ok(Reading { v: real(cx, t)?, ..Reading::default() }),
    }
}

/// Lists with `quantity(list, unit)` in them as lists of quantities (`quantity([1, 2], "m")` is `[quantity(1, "m"),
/// quantity(2, "m")]`), through nested lists (at most 3 levels).
pub(crate) fn spread_quantities<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, depth: usize) -> T<'h> {
    let h = cx.heap();
    if depth > 3 || h.packed(t).is_some() {
        return t;
    }
    if let Some((n, &[m, u])) = call_of(h, t) {
        if n == "quantity" {
            if let Some(items) = cx.as_list(m) {
                let items: Vec<T<'h>> = items.iter().map(|&e| spread_quantities(cx, h.call(h.sym("quantity"), &[e, u]), depth + 1)).collect();
                return cx.list(&items);
            }
        }
        return t;
    }
    match cx.as_list(t) {
        Some(items) if items.iter().any(|&e| !matches!(h.view(e), View::Int(_) | View::Real(_))) => {
            let new: Vec<T<'h>> = items.iter().map(|&e| spread_quantities(cx, e, depth + 1)).collect();
            if new.as_slice() == items {
                t
            } else {
                cx.list(&new)
            }
        }
        _ => t,
    }
}

/// The columns of a dataset `dataset(assoc(name -> column, ...))`, with their names; `None` for anything else.
pub(crate) fn dataset_columns<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<(String, T<'h>)>>> {
    let h = cx.heap();
    let Some((n, &[a])) = call_of(h, t) else { return Ok(None) };
    if n != "dataset" {
        return Ok(None);
    }
    assoc_entries(cx, a)
}

/// The entries of an association as (key as text, value); `None` when `t` is not an association.
pub(crate) fn assoc_entries<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<(String, T<'h>)>>> {
    let h = cx.heap();
    let Some((n, rules)) = call_of(h, t) else { return Ok(None) };
    if n != "assoc" {
        return Ok(None);
    }
    let mut out = Vec::with_capacity(rules.len());
    for &r in rules {
        let View::App { args: &[k, v], .. } = h.view(r) else { return Ok(None) };
        out.push((unit_text(cx, k), v));
    }
    Ok(Some(out))
}

/// Data given as a dataset or an association, as plain list data with names for the data sets: a dataset with one
/// column is that column; with two, the points (first column, second column); with more, a data set per column
/// after the first, against the first (named by the columns). An association of lists is a data set per key; an
/// association of numbers is one data set. `None` when `data`
/// is neither.
pub(crate) fn tabular<'h>(cx: &mut NativeCx<'_, '_, 'h>, data: T<'h>) -> Res<Option<Tabular<'h>>> {
    if let Some(cols) = dataset_columns(cx, data)? {
        if cols.is_empty() {
            return Ok(None);
        }
        let h = cx.heap();
        let pair = |cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, y: T<'h>| -> Res<T<'h>> {
            cx.eval(h.call(h.sym("transpose"), &[cx.list(&[x, y])]))
        };
        return Ok(Some(match cols.len() {
            1 => Tabular { data: cols[0].1, names: vec![], x_name: None, y_name: Some(cols[0].0.clone()) },
            2 => Tabular { data: pair(cx, cols[0].1, cols[1].1)?, names: vec![], x_name: Some(cols[0].0.clone()), y_name: Some(cols[1].0.clone()) },
            _ => {
                let mut sets = Vec::with_capacity(cols.len() - 1);
                for c in &cols[1..] {
                    sets.push(pair(cx, cols[0].1, c.1)?);
                }
                Tabular { data: cx.list(&sets), names: cols[1..].iter().map(|c| c.0.clone()).collect(), x_name: Some(cols[0].0.clone()), y_name: None }
            }
        }));
    }
    let Some(entries) = assoc_entries(cx, data)? else { return Ok(None) };
    let lists = !entries.is_empty() && entries.iter().all(|e| cx.as_list(e.1).is_some());
    let values: Vec<T<'h>> = entries.iter().map(|e| e.1).collect();
    let names: Vec<String> = entries.into_iter().map(|e| e.0).collect();
    Ok(Some(if lists {
        Tabular { data: cx.list(&values), names, x_name: None, y_name: None }
    } else {
        Tabular { data: cx.list(&values), names: vec![], x_name: None, y_name: None }
    }))
}

/// Plain list data read from a dataset or an association.
pub(crate) struct Tabular<'h> {
    pub(crate) data: T<'h>,
    /// a name per data set (legend entries)
    pub(crate) names: Vec<String>,
    /// axis labels from column names
    pub(crate) x_name: Option<String>,
    pub(crate) y_name: Option<String>,
}
