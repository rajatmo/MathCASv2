//! Datasets (package data): `dataset` (construction and normal form), `is_dataset`, `missing`, `is_missing`,
//! `column`, `columns`, `rows`, `add_column`, `join_on`, and the list functions on a dataset's rows (`length`,
//! `part`, `first`, `last`, `rest`, `most`, `take`, `drop`, `reverse`, `join`, `select`, `sort_by`), which reach
//! [`rows_op`] through the dataset's argument-head rules (decision DATA.2).
use crate::ctx::{charge_copy, map_calls, take, Lib, Meter};
use crate::table::{Col, Columns, Table};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;

/// The functions of lists that act on a dataset's rows (`mc_lists::DATASET_ROWS`, which declines them).
pub(crate) const ROW_FUNCTIONS: &[&str] =
    &["length", "part", "first", "last", "rest", "most", "take", "drop", "reverse", "join", "select", "sort_by"];

/// The internal native the argument-head rules call: `dataset_rows(f, ds, args...)` is `f(ds, args...)` on rows.
pub(crate) const ROWS_OP: &str = "dataset_rows";

/// `failure(argument, f, k)` for the function named `f`.
fn bad<'h>(lib: &Lib, h: &'h Heap, f: T<'h>, k: usize) -> T<'h> {
    lib.fail(h, lib.s.argument, &[f, h.int(k as i64)])
}

/// The 0-based position of the 1-based index `k` among `n` (negative from the end), if inside.
pub(crate) fn position(k: i64, n: usize) -> Option<usize> {
    let n = n as i64;
    match k {
        1.. if k <= n => Some(k as usize - 1),
        ..=-1 if -k <= n => Some((n + k) as usize),
        _ => None,
    }
}

/// `dataset(assoc(c -> col, ...))` (the normal form: columns of one length), `dataset([row, ...])` (rows as
/// associations; a key absent from a row is `missing()`), `dataset([[c1, c2, ...], [v1, v2, ...], ...])` (a table
/// whose first row names the columns, as `import_data` gives one), `dataset(rows, [c1, c2, ...])` (rows as lists).
pub(crate) fn dataset<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let s = &lib.d;
    match *args {
        [x] => {
            if Table::of(s, h, t).is_some() {
                return Ok(Reply::Done(t));
            }
            if Table::of(s, h, x).is_some() {
                return Ok(Reply::Done(x));
            }
            if let Some(entries) = lib.as_assoc(h, x) {
                // an association of rows (each an association): the rows, their keys dropped (decision io-core.12)
                let rows: Option<Vec<T<'h>>> = entries
                    .iter()
                    .map(|&e| lib.entry(h, e).map(|(_, v)| v).filter(|&v| lib.as_assoc(h, v).is_some()))
                    .collect();
                if let Some(rows) = rows.filter(|r| !r.is_empty()) {
                    return from_rows(lib, cx, &rows);
                }
                // an association of single values: a dataset of one row (G57)
                if !entries.is_empty() && entries.iter().all(|&e| Col::of(s, h, lib.value(h, e)).is_none()) {
                    return from_rows(lib, cx, &[x]);
                }
                return Ok(Reply::Done(columns_failure(lib, h, t, entries)));
            }
            if let View::Sym(_) = h.view(x) {
                return Ok(Reply::Decline);
            }
            let Some(xs) = cx.as_list(x) else { return Ok(Reply::Done(lib.bad_arg(h, t, 1))) };
            if xs.is_empty() {
                return Ok(Reply::Done(s.dataset(h, &[], &[])));
            }
            if xs.iter().all(|&r| lib.as_assoc(h, r).is_some()) {
                return from_rows(lib, cx, xs);
            }
            match cx.as_list(xs[0]) {
                Some(header) if xs.iter().all(|&r| cx.as_list(r).is_some()) => from_lists(lib, cx, t, header, &xs[1..]),
                _ => Ok(Reply::Done(lib.bad_arg(h, t, 1))),
            }
        }
        [x, names] => {
            let names = match h.view(names) {
                View::Sym(_) => return Ok(Reply::Decline),
                _ => match cx.as_list(names) {
                    Some(ns) => ns,
                    None => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
                },
            };
            let rows = match h.view(x) {
                View::Sym(_) => return Ok(Reply::Decline),
                _ => match cx.as_list(x) {
                    Some(rs) if rs.iter().all(|&r| cx.as_list(r).is_some()) => rs,
                    _ => return Ok(Reply::Done(lib.bad_arg(h, t, 1))),
                },
            };
            from_lists(lib, cx, t, names, rows)
        }
        _ => Ok(Reply::Decline),
    }
}

/// Why `dataset(assoc(...))` is not a dataset: a value that is not a list, or columns of different lengths.
fn columns_failure<'h>(lib: &Lib, h: &'h Heap, t: T<'h>, entries: &[T<'h>]) -> T<'h> {
    let mut n = None;
    for &e in entries {
        match Col::of(&lib.d, h, lib.value(h, e)) {
            None => return lib.bad_arg(h, t, 1),
            Some(c) if n.is_some_and(|n| n != c.len()) => {
                return lib.fail_named(h, "shape_mismatch", &[h.sym_term(lib.d.dataset)]);
            }
            Some(c) => n = Some(c.len()),
        }
    }
    lib.bad_arg(h, t, 1)
}

/// The dataset of rows given as associations: the columns are every key, in order of first occurrence.
fn from_rows<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, rows: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut cols = Columns::new(&lib.d, h);
    let mut meter = Meter::default();
    for &r in rows {
        cols.new_row();
        for &e in lib.as_assoc(h, r).unwrap_or(&[]) {
            meter.step(cx)?;
            let (k, v) = (lib.key(h, e), lib.value(h, e));
            let p = cols.column(k);
            cols.set(p, v);
        }
    }
    Ok(Reply::Done(cols.finish(&lib.d, h)))
}

/// The dataset of rows given as lists under the column names `names`: a short row ends in missing values, a row
/// longer than `names` is `failure(shape_mismatch, dataset)`, a repeated name `failure(duplicate_column, c)`.
fn from_lists<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    _t: T<'h>,
    names: &[T<'h>],
    rows: &[T<'h>],
) -> Res<Reply<'h>> {
    let h = cx.heap();
    if let Some(dup) = first_repeated(names) {
        return Ok(Reply::Done(lib.fail_named(h, "duplicate_column", &[dup])));
    }
    let m = names.len();
    let missing = lib.d.missing(h);
    let mut cols: Vec<Vec<T<'h>>> = (0..m).map(|_| Vec::with_capacity(rows.len())).collect();
    let mut meter = Meter::default();
    for &r in rows {
        meter.step(cx)?;
        let fields = cx.as_list(r).unwrap_or(&[]);
        if fields.len() > m {
            return Ok(Reply::Done(lib.fail_named(h, "shape_mismatch", &[h.sym_term(lib.d.dataset)])));
        }
        for (j, c) in cols.iter_mut().enumerate() {
            c.push(fields.get(j).copied().unwrap_or(missing));
        }
    }
    let cols: Vec<T<'h>> = cols.iter().map(|c| cx.list(c)).collect();
    Ok(Reply::Done(lib.d.dataset(h, names, &cols)))
}

/// The first element of `xs` that occurs earlier too (identity).
fn first_repeated<'h>(xs: &[T<'h>]) -> Option<T<'h>> {
    let mut seen = rustc_hash::FxHashSet::default();
    xs.iter().copied().find(|&x| !seen.insert(x))
}

/// Argument `k` of the call `t` as a dataset: a symbol declines, anything else is `failure(argument, f, k)`.
pub(crate) fn table_operand<'h>(lib: &Lib, h: &'h Heap, t: T<'h>, x: T<'h>, k: usize) -> Result<Table<'h>, Reply<'h>> {
    match Table::of(&lib.d, h, x) {
        Some(tab) => Ok(tab),
        None if matches!(h.view(x), View::Sym(_)) => Err(Reply::Decline),
        None => Err(Reply::Done(lib.bad_arg(h, t, k))),
    }
}

/// `is_dataset(x)`: whether `x` is a dataset in normal form.
pub(crate) fn is_dataset<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [x] = *args else { return Ok(Reply::Decline) };
    Ok(Reply::Done(cx.bool(Table::of(&lib.d, cx.heap(), x).is_some())))
}

/// `missing()`, `missing(reason)`: an absent value; it stays as it is.
pub(crate) fn missing<'h>(_lib: &Lib, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(Reply::Decline)
}

/// `is_missing(x)`: whether `x` is `missing(...)`.
pub(crate) fn is_missing<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let [x] = *args else { return Ok(Reply::Decline) };
    Ok(Reply::Done(cx.bool(lib.d.is_missing(cx.heap(), x))))
}

/// `column(ds, c)`: the list of the values of column `c` (`failure(missing_key, c)` if there is none).
pub(crate) fn column<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `column([x, ...], options...)` is the display form (a column of items), not a dataset column
    if args.first().is_some_and(|&x| cx.as_list(x).is_some()) {
        return Ok(Reply::Decline);
    }
    let [d, c] = *args else { return Ok(Reply::Decline) };
    let tab = take!(table_operand(lib, h, t, d, 1));
    Ok(Reply::Done(match tab.column(c) {
        Some(j) => tab.terms[j],
        None => lib.fail(h, lib.s.missing_key, &[c]),
    }))
}

/// `columns(ds)`: the list of the column names; `columns(ds, [c, ...])`: the dataset of those columns, in that
/// order.
pub(crate) fn columns<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    match *args {
        [d] => {
            let tab = take!(table_operand(lib, h, t, d, 1));
            Ok(Reply::Done(cx.list(&tab.names)))
        }
        [d, cs] => {
            let tab = take!(table_operand(lib, h, t, d, 1));
            let cs = lib.key_list(h, cs);
            if let Some(dup) = first_repeated(&cs) {
                return Ok(Reply::Done(lib.fail_named(h, "duplicate_column", &[dup])));
            }
            let mut cols = Vec::with_capacity(cs.len());
            for &c in &cs {
                match tab.column(c) {
                    Some(j) => cols.push(tab.terms[j]),
                    None => return Ok(Reply::Done(lib.fail(h, lib.s.missing_key, &[c]))),
                }
            }
            Ok(Reply::Done(lib.d.dataset(h, &cs, &cols)))
        }
        _ => Ok(Reply::Decline),
    }
}

/// The rows of `tab` as associations, charging fuel per row.
fn all_rows<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, tab: &Table<'h>) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let mut meter = Meter::default();
    let mut rows = Vec::with_capacity(tab.n);
    for i in 0..tab.n {
        meter.step(cx)?;
        rows.push(tab.row(&lib.d, h, i));
    }
    Ok(rows)
}

/// `rows(ds)`: the list of the rows, each an association from column names to values.
pub(crate) fn rows<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d] = *args else { return Ok(Reply::Decline) };
    let tab = take!(table_operand(lib, h, t, d, 1));
    let rows = all_rows(lib, cx, &tab)?;
    Ok(Reply::Done(cx.list(&rows)))
}

/// `f(row)` for every row, or the first failure among the values.
pub(crate) fn row_calls<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    tab: &Table<'h>,
    f: T<'h>,
) -> Res<Result<Vec<T<'h>>, T<'h>>> {
    let rows = all_rows(lib, cx, tab)?;
    let v = map_calls(cx, f, &rows)?;
    let h = cx.heap();
    Ok(match v.iter().find(|&&x| lib.is_failure(h, x)) {
        Some(&fail) => Err(fail),
        None => Ok(v),
    })
}

/// `add_column(ds, c, f)`: the dataset with column `c` set to `f(row)` for every row (replacing a column `c` in
/// place, else added last); `add_column(ds, c, values)`: set to a list of one value per row.
pub(crate) fn add_column<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [d, c, f] = *args else { return Ok(Reply::Decline) };
    let tab = take!(table_operand(lib, h, t, d, 1));
    let col = match Col::of(&lib.d, h, f) {
        Some(v) if v.len() == tab.n || tab.names.is_empty() => f,
        Some(_) => return Ok(Reply::Done(lib.fail_named(h, "shape_mismatch", &[h.sym_term(h.sym("add_column"))]))),
        None => match row_calls(lib, cx, &tab, f)? {
            Ok(v) => cx.list(&v),
            Err(fail) => return Ok(Reply::Done(fail)),
        },
    };
    let (mut names, mut cols) = (tab.names.clone(), tab.terms.clone());
    match tab.column(c) {
        Some(j) => cols[j] = col,
        None => {
            names.push(c);
            cols.push(col);
        }
    }
    Ok(Reply::Done(lib.d.dataset(h, &names, &cols)))
}

/// What names each row's key: columns, or a function of the row (decision DATA.6: a string or a list of strings
/// names columns, anything else is a function).
pub(crate) enum By<'h> {
    Cols(Vec<usize>),
    Fun(T<'h>),
}

/// The key `by` names in `tab`, or `failure(missing_key, c)` for a column it does not have.
pub(crate) fn by_of<'h>(lib: &Lib, h: &'h Heap, tab: &Table<'h>, by: T<'h>) -> Result<By<'h>, T<'h>> {
    let names: Vec<T<'h>> = match h.view(by) {
        View::Str(_) => vec![by],
        View::App { head, args } if head == h.sym_term(lib.s.list) && !args.is_empty() => args.to_vec(),
        _ => return Ok(By::Fun(by)),
    };
    let mut cols = Vec::with_capacity(names.len());
    for c in names {
        match tab.column(c) {
            Some(j) => cols.push(j),
            None => return Err(lib.fail(h, lib.s.missing_key, &[c])),
        }
    }
    Ok(By::Cols(cols))
}

/// The key of every row: a column's value, the list of several columns' values, or `f(row)`.
pub(crate) fn row_keys<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    tab: &Table<'h>,
    by: &By<'h>,
) -> Res<Result<Vec<T<'h>>, T<'h>>> {
    let h = cx.heap();
    match by {
        By::Fun(f) => row_calls(lib, cx, tab, *f),
        By::Cols(cs) => {
            let mut meter = Meter::default();
            let mut keys = Vec::with_capacity(tab.n);
            for i in 0..tab.n {
                meter.step(cx)?;
                keys.push(match cs.as_slice() {
                    [j] => tab.cols[*j].get(h, i),
                    _ => {
                        let v: Vec<T<'h>> = cs.iter().map(|&j| tab.cols[j].get(h, i)).collect();
                        cx.list(&v)
                    }
                });
            }
            Ok(Ok(keys))
        }
    }
}

/// `f(ds, args...)` for the list functions of [`ROW_FUNCTIONS`] on a dataset `ds`: the rows are the elements. Called
/// as `dataset_rows(f, ds, args...)` by the dataset's argument-head rules; a call it cannot evaluate is given back
/// unevaluated as `f(ds, args...)`.
pub(crate) fn rows_op<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [f, d, ref rest @ ..] = *args else { return Ok(Reply::Decline) };
    let unevaluated = || {
        let mut a = vec![d];
        a.extend_from_slice(rest);
        Reply::Done(h.app(f, &a))
    };
    let (View::Sym(name), Some(tab)) = (h.view(f), Table::of(&lib.d, h, d)) else { return Ok(unevaluated()) };
    let s = &lib.d;
    let n = tab.n;
    let index = |k: i64| lib.fail_named(h, "index", &[h.int(k), h.int(n as i64)]);
    Ok(match (h.sym_name(name).as_str(), rest) {
        ("length", []) => Reply::Done(h.int(n as i64)),
        ("first", []) if n == 0 => Reply::Done(index(1)),
        ("last", []) if n == 0 => Reply::Done(index(-1)),
        ("first", []) => Reply::Done(tab.row(s, h, 0)),
        ("last", []) => Reply::Done(tab.row(s, h, n - 1)),
        ("rest" | "most", []) if n == 0 => Reply::Done(index(1)),
        ("rest", []) => Reply::Done(tab.slice(s, h, 1, n)),
        ("most", []) => Reply::Done(tab.slice(s, h, 0, n - 1)),
        ("reverse", []) => {
            charge_copy(cx, n * tab.cols.len())?;
            let ix: Vec<usize> = (0..n).rev().collect();
            Reply::Done(tab.gather(s, h, &ix))
        }
        (op @ ("take" | "drop"), &[k]) => match h.view(k) {
            View::Int(k) => match usize::try_from(k.unsigned_abs()).ok().filter(|&a| a <= n) {
                None => Reply::Done(index(k)),
                Some(a) => {
                    let (lo, hi) = if k >= 0 { (0, a) } else { (n - a, n) };
                    charge_copy(cx, n * tab.cols.len())?;
                    let ix: Vec<usize> = if op == "take" { (lo..hi).collect() } else { (0..lo).chain(hi..n).collect() };
                    Reply::Done(tab.gather(s, h, &ix))
                }
            },
            View::Sym(_) => unevaluated(),
            _ => Reply::Done(bad(lib, h, f, 2)),
        },
        ("part", [i, more @ ..]) => part(lib, cx, &tab, *i, more)?,
        ("select", &[p]) => match row_calls(lib, cx, &tab, p)? {
            Ok(v) => {
                let ix: Vec<usize> = (0..n).filter(|&i| cx.is_true(v[i])).collect();
                Reply::Done(tab.gather(s, h, &ix))
            }
            Err(fail) => Reply::Done(fail),
        },
        ("sort_by", &[by]) => sort_rows(lib, cx, &tab, by)?,
        ("join", others) => join_rows(lib, cx, f, &tab, others)?,
        _ => unevaluated(),
    })
}

/// `part(ds, i, ...)`: an integer index is a row (an association; further indices go to it), a span or a list of
/// integers a dataset of rows (further indices go to each row, giving a list), anything else a column name (the
/// column; further indices go to it). A symbol that names no column leaves the call unevaluated.
fn part<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, tab: &Table<'h>, i: T<'h>, more: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let s = &lib.d;
    let part = h.sym("part");
    let n = tab.n;
    let index = |k: T<'h>| lib.fail_named(h, "index", &[k, h.int(n as i64)]);
    let then = |x: T<'h>| -> Reply<'h> {
        if more.is_empty() {
            return Reply::Done(x);
        }
        let mut a = vec![x];
        a.extend_from_slice(more);
        Reply::Again(h.call(part, &a))
    };
    // the rows an index selects
    let ix: Vec<usize> = match h.view(i) {
        View::Int(k) => {
            return Ok(match position(k, n) {
                Some(p) => then(tab.row(s, h, p)),
                None => Reply::Done(index(i)),
            })
        }
        View::App { head, args: &[a, b] } if head == h.sym_term(h.sym("span")) => {
            let (View::Int(a), View::Int(b)) = (h.view(a), h.view(b)) else {
                return Ok(Reply::Done(lib.fail(h, lib.s.argument, &[h.sym_term(part), h.int(2)])));
            };
            let lo = if a == n as i64 + 1 { Some(n) } else { position(a, n) };
            match (lo, position(b, n)) {
                (None, _) => return Ok(Reply::Done(index(h.int(a)))),
                (Some(lo), Some(hi)) if hi + 1 >= lo => (lo..hi + 1).collect(),
                _ => return Ok(Reply::Done(index(h.int(b)))),
            }
        }
        // `all`: every row, so `ds[all, "y"]` is the column `y` as a list (G63)
        View::Sym(a) if h.sym_name(a) == "all" && tab.column(i).is_none() && more.first().is_some_and(|&c| tab.column(c).is_some()) => {
            (0..n).collect()
        }
        _ => match (cx.as_list(i), tab.column(i)) {
            (_, Some(j)) => return Ok(then(tab.terms[j])),
            (Some(ks), None) => {
                let mut ix = Vec::with_capacity(ks.len());
                for &k in ks {
                    match h.view(k) {
                        View::Int(v) => match position(v, n) {
                            Some(p) => ix.push(p),
                            None => return Ok(Reply::Done(index(k))),
                        },
                        _ => return Ok(Reply::Done(lib.fail(h, lib.s.missing_key, &[i]))),
                    }
                }
                ix
            }
            // a symbol that names no column is an unknown index: the call stays as it is
            (None, None) if matches!(h.view(i), View::Sym(_)) => {
                let mut a = vec![s.dataset(h, &tab.names, &tab.terms), i];
                a.extend_from_slice(more);
                return Ok(Reply::Done(h.call(part, &a)));
            }
            (None, None) => return Ok(Reply::Done(lib.fail(h, lib.s.missing_key, &[i]))),
        },
    };
    charge_copy(cx, ix.len() * tab.cols.len().max(1))?;
    if more.is_empty() {
        return Ok(Reply::Done(tab.gather(s, h, &ix)));
    }
    let parts: Vec<T<'h>> = ix
        .iter()
        .map(|&p| {
            let mut a = vec![tab.row(s, h, p)];
            a.extend_from_slice(more);
            h.call(part, &a)
        })
        .collect();
    Ok(Reply::Again(cx.list(&parts)))
}

/// Compare two values of a key column: numbers of a packed column directly, other values in the kernel's order,
/// `missing(...)` after everything else.
fn cmp_cells<'h>(lib: &Lib, cx: &NativeCx<'_, '_, 'h>, c: &Col<'h>, a: usize, b: usize) -> Ordering {
    match c {
        Col::Ints(d) => d[a].cmp(&d[b]),
        Col::Reals(d) => d[a].total_cmp(&d[b]),
        Col::Terms(d) => {
            let h = cx.heap();
            match (lib.d.is_missing(h, d[a]), lib.d.is_missing(h, d[b])) {
                (false, false) => cx.cmp(d[a], d[b]),
                (x, y) => x.cmp(&y),
            }
        }
    }
}

/// `sort_by(ds, by)`: the rows ordered by their keys (stable; missing keys last).
fn sort_rows<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, tab: &Table<'h>, by: T<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let keys: Vec<Col<'h>> = match by_of(lib, h, tab, by) {
        Err(fail) => return Ok(Reply::Done(fail)),
        Ok(By::Cols(cs)) => cs.iter().map(|&j| tab.cols[j]).collect(),
        Ok(by @ By::Fun(_)) => match row_keys(lib, cx, tab, &by)? {
            // the keys as one list: packed (and compared as numbers) when they are machine numbers
            Ok(v) => vec![Col::of(&lib.d, h, cx.list(&v)).expect("a list")],
            Err(fail) => return Ok(Reply::Done(fail)),
        },
    };
    let n = tab.n as u64;
    cx.tick(n * (64 - n.leading_zeros() as u64) / 16)?;
    let mut ix: Vec<usize> = (0..tab.n).collect();
    let cx_ref: &NativeCx<'_, '_, 'h> = cx;
    ix.sort_by(|&a, &b| {
        keys.iter().map(|c| cmp_cells(lib, cx_ref, c, a, b)).find(|o| o.is_ne()).unwrap_or(Ordering::Equal)
    });
    Ok(Reply::Done(tab.gather(&lib.d, h, &ix)))
}

/// `join(ds1, ds2, ...)`: the rows of every dataset in order; the columns of all, in order of first occurrence,
/// missing where a dataset lacks one.
fn join_rows<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    first: &Table<'h>,
    others: &[T<'h>],
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let mut tabs = Vec::with_capacity(others.len());
    for (k, &o) in others.iter().enumerate() {
        match Table::of(&lib.d, h, o) {
            Some(t) => tabs.push(t),
            None if matches!(h.view(o), View::Sym(_)) => {
                let mut a = vec![lib.d.dataset(h, &first.names, &first.terms)];
                a.extend_from_slice(others);
                return Ok(Reply::Done(h.app(f, &a)));
            }
            None => return Ok(Reply::Done(bad(lib, h, f, k + 2))),
        }
    }
    let all: Vec<&Table<'h>> = std::iter::once(first).chain(tabs.iter()).collect();
    let mut names: Vec<T<'h>> = vec![];
    for t in &all {
        for &c in &t.names {
            if !names.contains(&c) {
                names.push(c);
            }
        }
    }
    let total: usize = all.iter().map(|t| t.n).sum();
    charge_copy(cx, total * names.len())?;
    let missing = lib.d.missing(h);
    let mut cols = Vec::with_capacity(names.len());
    for &c in &names {
        let mut v = Vec::with_capacity(total);
        for t in &all {
            match t.column(c) {
                Some(j) => v.extend((0..t.n).map(|i| t.cols[j].get(h, i))),
                None => v.extend(std::iter::repeat_n(missing, t.n)),
            }
        }
        cols.push(cx.list(&v));
    }
    Ok(Reply::Done(lib.d.dataset(h, &names, &cols)))
}

/// How `join_on` keeps rows without a partner.
#[derive(Clone, Copy, PartialEq)]
enum How {
    Inner,
    Left,
    Right,
    Outer,
}

/// `join_on(a, b, key)`: the rows of `a` and `b` whose key columns agree, one row per matching pair (in the order of
/// `a`, then of `b`); `how: left` also keeps the rows of `a` without a partner, `how: right` those of `b`,
/// `how: outer` both. The key columns come first, then the other columns of `a`, then those of `b`; a name of `b`
/// that `a` has too is suffixed `_right` (decision DATA.9). A missing key matches nothing.
pub(crate) fn join_on<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, how) = match args.split_last() {
        Some((&o, rest)) if matches!(h.view(o), View::App { head, .. } if head == h.sym_term(h.sym("options"))) => {
            match how_of(lib, h, o) {
                Ok(how) => (rest, how),
                Err(fail) => return Ok(Reply::Done(fail)),
            }
        }
        _ => (args, How::Inner),
    };
    let [a, b, key] = *args else { return Ok(Reply::Decline) };
    let ta = take!(table_operand(lib, h, t, a, 1));
    let tb = take!(table_operand(lib, h, t, b, 2));
    let keys = lib.key_list(h, key);
    let mut ka = Vec::with_capacity(keys.len());
    let mut kb = Vec::with_capacity(keys.len());
    for &k in &keys {
        match (ta.column(k), tb.column(k)) {
            (Some(i), Some(j)) => {
                ka.push(i);
                kb.push(j);
            }
            _ => return Ok(Reply::Done(lib.fail(h, lib.s.missing_key, &[k]))),
        }
    }
    let key_of = |tab: &Table<'h>, cs: &[usize], i: usize| -> Option<T<'h>> {
        let v: Vec<T<'h>> = cs.iter().map(|&j| tab.cols[j].get(h, i)).collect();
        if v.iter().any(|&x| lib.d.is_missing(h, x)) {
            return None;
        }
        Some(if v.len() == 1 { v[0] } else { h.call(lib.s.list, &v) })
    };
    // the rows of b by key
    let mut at: FxHashMap<T<'h>, Vec<usize>> = FxHashMap::default();
    let mut meter = Meter::default();
    for i in 0..tb.n {
        meter.step(cx)?;
        if let Some(k) = key_of(&tb, &kb, i) {
            at.entry(k).or_default().push(i);
        }
    }
    let mut pairs: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let mut matched_b = vec![false; tb.n];
    for i in 0..ta.n {
        meter.step(cx)?;
        match key_of(&ta, &ka, i).and_then(|k| at.get(&k)) {
            Some(js) => {
                for &j in js {
                    meter.step(cx)?;
                    pairs.push((Some(i), Some(j)));
                    matched_b[j] = true;
                }
            }
            None if matches!(how, How::Left | How::Outer) => pairs.push((Some(i), None)),
            None => {}
        }
    }
    if how == How::Right {
        // every row of b, in the order of b, with its partners
        let mut by_b: Vec<Vec<usize>> = vec![vec![]; tb.n];
        for &(i, j) in &pairs {
            if let (Some(i), Some(j)) = (i, j) {
                by_b[j].push(i);
            }
        }
        pairs = (0..tb.n)
            .flat_map(|j| match by_b[j].as_slice() {
                [] => vec![(None, Some(j))],
                is => is.iter().map(|&i| (Some(i), Some(j))).collect(),
            })
            .collect();
    } else if how == How::Outer {
        pairs.extend((0..tb.n).filter(|&j| !matched_b[j]).map(|j| (None, Some(j))));
    }
    let li: Vec<Option<usize>> = pairs.iter().map(|p| p.0).collect();
    let ri: Vec<Option<usize>> = pairs.iter().map(|p| p.1).collect();
    charge_copy(cx, pairs.len() * (ta.cols.len() + tb.cols.len()))?;
    let (mut names, mut cols) = (Vec::new(), Vec::new());
    for (&ja, &jb) in ka.iter().zip(&kb) {
        names.push(ta.names[ja]);
        // a key value comes from whichever side the row has
        let v: Vec<T<'h>> = pairs
            .iter()
            .map(|&(i, j)| match (i, j) {
                (Some(i), _) => ta.cols[ja].get(h, i),
                (None, Some(j)) => tb.cols[jb].get(h, j),
                (None, None) => lib.d.missing(h),
            })
            .collect();
        cols.push(cx.list(&v));
    }
    for (j, &c) in ta.names.iter().enumerate().filter(|(j, _)| !ka.contains(j)) {
        names.push(c);
        cols.push(ta.cols[j].gather_or_missing(&lib.d, h, &li));
    }
    for (j, &c) in tb.names.iter().enumerate().filter(|(j, _)| !kb.contains(j)) {
        let name = if names.contains(&c) {
            match h.view(c) {
                View::Str(s) => h.string(&format!("{s}_right")),
                _ => return Ok(Reply::Done(lib.fail_named(h, "duplicate_column", &[c]))),
            }
        } else {
            c
        };
        if names.contains(&name) {
            return Ok(Reply::Done(lib.fail_named(h, "duplicate_column", &[name])));
        }
        names.push(name);
        cols.push(tb.cols[j].gather_or_missing(&lib.d, h, &ri));
    }
    Ok(Reply::Done(lib.d.dataset(h, &names, &cols)))
}

/// The `how` option of `join_on`: `inner`, `left`, `right` or `outer` (a symbol or a string).
fn how_of<'h>(lib: &Lib, h: &'h Heap, o: T<'h>) -> Result<How, T<'h>> {
    let View::App { args, .. } = h.view(o) else { return Ok(How::Inner) };
    let mut how = How::Inner;
    for &r in args {
        let (k, v) = match lib.entry(h, r) {
            Some(kv) => kv,
            None => return Err(lib.fail_named(h, "bad_option", &[r])),
        };
        if k != h.sym_term(h.sym("how")) {
            return Err(lib.fail_named(h, "unknown_option", &[k]));
        }
        let word = match h.view(v) {
            View::Sym(s) => h.sym_name(s),
            View::Str(s) => s.to_string(),
            _ => String::new(),
        };
        how = match word.as_str() {
            "inner" => How::Inner,
            "left" => How::Left,
            "right" => How::Right,
            "outer" => How::Outer,
            _ => return Err(lib.fail_named(h, "bad_option", &[k, v])),
        };
    }
    Ok(how)
}
