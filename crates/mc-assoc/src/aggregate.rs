//! Grouping and aggregation of datasets: `group_by` on a dataset, `aggregate`, `pivot`, and the aggregate functions
//! `total`, `mean`, `count`, `min`, `max` computed from a column's payload where it is packed. Every aggregate skips
//! missing values (decision DATA.7).
use crate::ctx::{take, Lib, Meter};
use crate::dataset::{by_of, row_keys, table_operand, By};
use crate::group::classes;
use crate::table::{Col, Table};
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// An aggregate function.
#[derive(Clone, Copy)]
enum Agg<'h> {
    Total,
    Mean,
    Count,
    Min,
    Max,
    /// any other function, applied to the list of the values that are not missing
    Custom(T<'h>),
}

impl<'h> Agg<'h> {
    fn of(h: &'h Heap, f: T<'h>) -> Agg<'h> {
        match h.view(f) {
            View::Sym(s) => match h.sym_name(s).as_str() {
                "total" => Agg::Total,
                "mean" => Agg::Mean,
                "count" => Agg::Count,
                "min" => Agg::Min,
                "max" => Agg::Max,
                _ => Agg::Custom(f),
            },
            _ => Agg::Custom(f),
        }
    }
}

/// An integer as a term (a big integer beyond 64 bits).
fn int128<'h>(h: &'h Heap, v: i128) -> T<'h> {
    match i64::try_from(v) {
        Ok(i) => h.int(i),
        Err(_) => {
            let m = v.unsigned_abs();
            h.big_int(v < 0, &[m as u64, (m >> 64) as u64])
        }
    }
}

/// A sum of reals with Neumaier's compensation (close to the correctly rounded sum).
fn real_sum(xs: impl Iterator<Item = f64>) -> f64 {
    let (mut s, mut c) = (0.0f64, 0.0f64);
    for x in xs {
        let t = s + x;
        c += if s.abs() >= x.abs() { (s - t) + x } else { (x - t) + s };
        s = t;
    }
    s + c
}

/// The values of column `c` at rows `ix` that are not missing, as the numbers they are when they all are machine
/// numbers.
enum Values<'h> {
    Ints(Vec<i64>),
    /// machine reals, with the terms they came from when some were integers (which `min`, `max` and custom
    /// functions give back as they are)
    Reals(Vec<f64>, Option<Vec<T<'h>>>),
    Terms(Vec<T<'h>>),
}

fn values<'h>(lib: &Lib, h: &'h Heap, c: &Col<'h>, ix: &[usize]) -> Values<'h> {
    match c {
        Col::Ints(d) => Values::Ints(ix.iter().map(|&i| d[i]).collect()),
        Col::Reals(d) => Values::Reals(ix.iter().map(|&i| d[i]).collect(), None),
        Col::Terms(d) => {
            let v: Vec<T<'h>> = ix.iter().map(|&i| d[i]).filter(|&x| !lib.d.is_missing(h, x)).collect();
            if v.iter().all(|&x| matches!(h.view(x), View::Int(_))) {
                Values::Ints(v.iter().map(|&x| if let View::Int(i) = h.view(x) { i } else { 0 }).collect())
            } else if v.iter().all(|&x| matches!(h.view(x), View::Int(_) | View::Real(_))) {
                let as_real = |x| match h.view(x) {
                    View::Int(i) => i as f64,
                    View::Real(r) => r,
                    _ => 0.0,
                };
                Values::Reals(v.iter().map(|&x| as_real(x)).collect(), Some(v))
            } else {
                Values::Terms(v)
            }
        }
    }
}

/// `f` applied to the values of column `c` at the rows `ix` (missing values skipped). An empty `total` is 0, an
/// empty `count` 0, an empty `mean`, `min` or `max` is `missing()`.
fn aggregate_values<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    c: &Col<'h>,
    ix: &[usize],
    f: Agg<'h>,
) -> Res<T<'h>> {
    let h = cx.heap();
    cx.tick(ix.len() as u64 / 16)?;
    let vals = values(lib, h, c, ix);
    let len = match &vals {
        Values::Ints(v) => v.len(),
        Values::Reals(v, _) => v.len(),
        Values::Terms(v) => v.len(),
    };
    if len == 0 && matches!(f, Agg::Mean | Agg::Min | Agg::Max) {
        return Ok(lib.d.missing(h));
    }
    let call = |name: &str, args: &[T<'h>]| h.call(h.sym(name), args);
    let list = |cx: &NativeCx<'_, '_, 'h>| -> T<'h> {
        match &vals {
            Values::Ints(v) => h.ints(v),
            Values::Reals(_, Some(v)) | Values::Terms(v) => cx.list(v),
            Values::Reals(v, None) => h.reals(v),
        }
    };
    let total = |cx: &mut NativeCx<'_, '_, 'h>| -> Res<T<'h>> {
        Ok(match &vals {
            Values::Ints(v) => int128(h, v.iter().map(|&x| x as i128).sum()),
            Values::Reals(v, _) => h.real(real_sum(v.iter().copied())),
            Values::Terms(_) => {
                let l = list(cx);
                cx.eval(call("total", &[l]))?
            }
        })
    };
    Ok(match f {
        Agg::Count => h.int(len as i64),
        Agg::Total => total(cx)?,
        Agg::Mean => {
            let s = total(cx)?;
            let inv = call(names::POW, &[h.int(len as i64), h.int(-1)]);
            cx.eval(call(names::MUL, &[s, inv]))?
        }
        Agg::Min | Agg::Max => {
            let max = matches!(f, Agg::Max);
            match &vals {
                Values::Ints(v) => h.int(if max { *v.iter().max().expect("not empty") } else { *v.iter().min().expect("not empty") }),
                Values::Reals(v, terms) => {
                    // the first extreme element (NaN never is one)
                    let better = |a: f64, b: f64| if max { a > b } else { a < b };
                    let k = (1..v.len()).fold(0, |k, i| if better(v[i], v[k]) || v[k].is_nan() { i } else { k });
                    terms.as_ref().map_or_else(|| h.real(v[k]), |t| t[k])
                }
                Values::Terms(_) => {
                    let l = list(cx);
                    cx.eval(call(if max { "max" } else { "min" }, &[l]))?
                }
            }
        }
        Agg::Custom(g) => {
            let l = list(cx);
            cx.eval(h.app(g, &[l]))?
        }
    })
}

/// `group_by(ds, by)`: the association from each key (a column's value, a list of several columns' values, or
/// `f(row)`) to the dataset of the rows with that key, keys in order of first occurrence; `group_by(ds, by, g)`:
/// each dataset replaced by `g` of it.
pub(crate) fn group_dataset<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    tab: &Table<'h>,
    by: T<'h>,
    g: Option<T<'h>>,
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (keys, members) = match groups(lib, cx, tab, by)? {
        Ok(km) => km,
        Err(fail) => return Ok(Reply::Done(fail)),
    };
    let mut parts: Vec<T<'h>> = Vec::with_capacity(keys.len());
    for m in &members {
        cx.tick((m.len() * tab.cols.len().max(1)) as u64 / 16)?;
        parts.push(tab.gather(&lib.d, h, m));
    }
    if let Some(g) = g {
        parts = crate::ctx::map_calls(cx, g, &parts)?;
        if let Some(&fail) = parts.iter().find(|&&v| lib.is_failure(h, v)) {
            return Ok(Reply::Done(fail));
        }
    }
    let entries: Vec<T<'h>> = keys.iter().zip(&parts).map(|(&k, &v)| lib.rule(h, k, v)).collect();
    Ok(Reply::Done(crate::assoc::from_distinct(lib, h, &keys, &entries)))
}

/// The distinct keys of the rows and the rows of each.
type Groups<'h> = (Vec<T<'h>>, Vec<Vec<usize>>);

fn groups<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, tab: &Table<'h>, by: T<'h>) -> Res<Result<Groups<'h>, T<'h>>> {
    let h = cx.heap();
    let by = match by_of(lib, h, tab, by) {
        Ok(b) => b,
        Err(fail) => return Ok(Err(fail)),
    };
    let keys = match row_keys(lib, cx, tab, &by)? {
        Ok(k) => k,
        Err(fail) => return Ok(Err(fail)),
    };
    Ok(Ok(classes(cx, &keys)?))
}

/// One output column of `aggregate`: its name, the column it reads (none for the row count) and the function.
struct Spec<'h> {
    name: T<'h>,
    column: Option<usize>,
    f: Agg<'h>,
}

/// The specifications of `aggregate`: a list (or one) of `count` (the number of rows, named `"count"`), `c -> f`
/// (`f` of column `c`, named `c`) and `name -> [c, f]` (`f` of column `c`, named `name`).
fn specs_of<'h>(lib: &Lib, h: &'h Heap, tab: &Table<'h>, specs: T<'h>, at: usize, t: T<'h>) -> Result<Vec<Spec<'h>>, T<'h>> {
    let items = match h.view(specs) {
        View::App { head, args } if head == h.sym_term(lib.s.list) => args.to_vec(),
        _ => vec![specs],
    };
    let column = |c: T<'h>| tab.column(c).ok_or_else(|| lib.fail(h, lib.s.missing_key, &[c]));
    let mut out = Vec::with_capacity(items.len());
    for x in items {
        if let View::Sym(s) = h.view(x) {
            if h.sym_name(s) == "count" {
                out.push(Spec { name: h.string("count"), column: None, f: Agg::Count });
                continue;
            }
        }
        let Some((k, v)) = lib.entry(h, x) else { return Err(lib.bad_arg(h, t, at)) };
        match h.view(v) {
            View::App { head, args: &[c, f] } if head == h.sym_term(lib.s.list) => {
                out.push(Spec { name: k, column: Some(column(c)?), f: Agg::of(h, f) })
            }
            _ => out.push(Spec { name: k, column: Some(column(k)?), f: Agg::of(h, v) }),
        }
    }
    Ok(out)
}

/// The aggregated columns: for every spec, its value over each group of rows.
fn aggregate_groups<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    tab: &Table<'h>,
    specs: &[Spec<'h>],
    members: &[Vec<usize>],
) -> Res<Result<Vec<T<'h>>, T<'h>>> {
    let h = cx.heap();
    let mut cols = Vec::with_capacity(specs.len());
    let mut meter = Meter::default();
    for s in specs {
        let mut v = Vec::with_capacity(members.len());
        for m in members {
            meter.step(cx)?;
            let x = match s.column {
                None => h.int(m.len() as i64),
                Some(j) => aggregate_values(lib, cx, &tab.cols[j], m, s.f)?,
            };
            if lib.is_failure(h, x) {
                return Ok(Err(x));
            }
            v.push(x);
        }
        cols.push(cx.list(&v));
    }
    Ok(Ok(cols))
}

/// `aggregate(ds, specs)`: a dataset of one row aggregating every row; `aggregate(ds, by, specs)`: one row per
/// group of rows with the same key (`by` names a column, a list of columns, or is a function of the row: the key
/// column is then named `"key"`), key columns first; `aggregate(groups, specs)`: one row per entry of an
/// association of datasets (as `group_by` gives), the key column named `"key"`.
pub(crate) fn aggregate<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let key_name = h.string("key");
    match *args {
        [g, specs] if lib.as_assoc(h, g).is_some() => {
            // an association of datasets: aggregate each, then stack them
            let entries = lib.as_assoc(h, g).unwrap_or(&[]);
            let (mut keys, mut rows) = (Vec::with_capacity(entries.len()), Vec::with_capacity(entries.len()));
            let mut out_names: Option<Vec<T<'h>>> = None;
            for &e in entries {
                let tab = match Table::of(&lib.d, h, lib.value(h, e)) {
                    Some(tab) => tab,
                    None => return Ok(Reply::Done(lib.bad_arg(h, t, 1))),
                };
                let sp = match specs_of(lib, h, &tab, specs, 2, t) {
                    Ok(s) => s,
                    Err(f) => return Ok(Reply::Done(f)),
                };
                let all: Vec<usize> = (0..tab.n).collect();
                let cols = match aggregate_groups(lib, cx, &tab, &sp, &[all])? {
                    Ok(c) => c,
                    Err(f) => return Ok(Reply::Done(f)),
                };
                out_names.get_or_insert_with(|| sp.iter().map(|s| s.name).collect());
                keys.push(lib.key(h, e));
                rows.push(cols.iter().map(|&c| cx.as_list(c).map_or(c, |v| v[0])).collect::<Vec<_>>());
            }
            let out_names = out_names.unwrap_or_default();
            let mut names = vec![key_name];
            names.extend_from_slice(&out_names);
            if let Some(dup) = repeated(&names) {
                return Ok(Reply::Done(lib.fail_named(h, "duplicate_column", &[dup])));
            }
            let mut cols = vec![cx.list(&keys)];
            for j in 0..out_names.len() {
                let v: Vec<T<'h>> = rows.iter().map(|r| r[j]).collect();
                cols.push(cx.list(&v));
            }
            Ok(Reply::Done(lib.d.dataset(h, &names, &cols)))
        }
        [d, specs] => {
            let tab = take!(table_operand(lib, h, t, d, 1));
            let sp = match specs_of(lib, h, &tab, specs, 2, t) {
                Ok(s) => s,
                Err(f) => return Ok(Reply::Done(f)),
            };
            let all: Vec<usize> = (0..tab.n).collect();
            Ok(Reply::Done(match aggregate_groups(lib, cx, &tab, &sp, &[all])? {
                Ok(cols) => finish(lib, h, sp.iter().map(|s| s.name).collect(), cols),
                Err(f) => f,
            }))
        }
        [d, by, specs] => {
            let tab = take!(table_operand(lib, h, t, d, 1));
            let sp = match specs_of(lib, h, &tab, specs, 3, t) {
                Ok(s) => s,
                Err(f) => return Ok(Reply::Done(f)),
            };
            let by_cols = match by_of(lib, h, &tab, by) {
                Ok(b) => b,
                Err(f) => return Ok(Reply::Done(f)),
            };
            let (keys, members) = match groups(lib, cx, &tab, by)? {
                Ok(km) => km,
                Err(f) => return Ok(Reply::Done(f)),
            };
            let (mut names, mut cols) = (vec![], vec![]);
            match by_cols {
                By::Cols(cs) => {
                    for (k, &j) in cs.iter().enumerate() {
                        names.push(tab.names[j]);
                        let v: Vec<T<'h>> = if cs.len() == 1 {
                            keys.clone()
                        } else {
                            keys.iter().map(|&key| cx.as_list(key).map_or(key, |p| p[k])).collect()
                        };
                        cols.push(cx.list(&v));
                    }
                }
                By::Fun(_) => {
                    names.push(key_name);
                    cols.push(cx.list(&keys));
                }
            }
            match aggregate_groups(lib, cx, &tab, &sp, &members)? {
                Ok(c) => {
                    names.extend(sp.iter().map(|s| s.name));
                    cols.extend(c);
                    Ok(Reply::Done(finish(lib, h, names, cols)))
                }
                Err(f) => Ok(Reply::Done(f)),
            }
        }
        _ => Ok(Reply::Decline),
    }
}

/// The first name that occurs twice.
fn repeated<'h>(names: &[T<'h>]) -> Option<T<'h>> {
    let mut seen = rustc_hash::FxHashSet::default();
    names.iter().copied().find(|&x| !seen.insert(x))
}

/// The dataset of these columns, or `failure(duplicate_column, c)`.
fn finish<'h>(lib: &Lib, h: &'h Heap, names: Vec<T<'h>>, cols: Vec<T<'h>>) -> T<'h> {
    match repeated(&names) {
        Some(dup) => lib.fail_named(h, "duplicate_column", &[dup]),
        None => lib.d.dataset(h, &names, &cols),
    }
}

/// `pivot(ds, r, c, v)`: a row for each distinct value of column `r` and a column for each distinct value of column
/// `c` (both in order of first occurrence), holding the value of column `v` of the row with those two values
/// (`missing()` if none, `failure(duplicate_entry, [rv, cv])` if several); `pivot(ds, r, c, v, f)`: `f` of the
/// values instead (an aggregate function, as in `aggregate`).
pub(crate) fn pivot<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (d, r, c, v, f) = match *args {
        [d, r, c, v] => (d, r, c, v, None),
        [d, r, c, v, f] => (d, r, c, v, Some(Agg::of(h, f))),
        _ => return Ok(Reply::Decline),
    };
    let tab = take!(table_operand(lib, h, t, d, 1));
    let mut cols = [0usize; 3];
    for (slot, &name) in cols.iter_mut().zip(&[r, c, v]) {
        match tab.column(name) {
            Some(j) => *slot = j,
            None => return Ok(Reply::Done(lib.fail(h, lib.s.missing_key, &[name]))),
        }
    }
    let [jr, jc, jv] = cols;
    let rk: Vec<T<'h>> = (0..tab.n).map(|i| tab.cols[jr].get(h, i)).collect();
    let ck: Vec<T<'h>> = (0..tab.n).map(|i| tab.cols[jc].get(h, i)).collect();
    let (rows, row_members) = classes(cx, &rk)?;
    let (heads, col_members) = classes(cx, &ck)?;
    // the column class of every row
    let mut class_of = vec![0usize; tab.n];
    for (k, m) in col_members.iter().enumerate() {
        for &i in m {
            class_of[i] = k;
        }
    }
    let mut out: Vec<Vec<T<'h>>> = vec![Vec::with_capacity(rows.len()); heads.len()];
    let mut meter = Meter::default();
    for (ri, m) in row_members.iter().enumerate() {
        let mut cells: Vec<Vec<usize>> = vec![vec![]; heads.len()];
        for &i in m {
            meter.step(cx)?;
            cells[class_of[i]].push(i);
        }
        for (k, cell) in cells.iter().enumerate() {
            let x = match f {
                Some(f) => aggregate_values(lib, cx, &tab.cols[jv], cell, f)?,
                None => match cell.as_slice() {
                    [] => lib.d.missing(h),
                    [i] => tab.cols[jv].get(h, *i),
                    _ => {
                        let at = h.call(lib.s.list, &[rows[ri], heads[k]]);
                        return Ok(Reply::Done(lib.fail_named(h, "duplicate_entry", &[at])));
                    }
                },
            };
            if lib.is_failure(h, x) {
                return Ok(Reply::Done(x));
            }
            out[k].push(x);
        }
    }
    let mut names = vec![tab.names[jr]];
    names.extend_from_slice(&heads);
    let mut columns = vec![cx.list(&rows)];
    columns.extend(out.iter().map(|v| cx.list(v)));
    Ok(Reply::Done(finish(lib, h, names, columns)))
}
