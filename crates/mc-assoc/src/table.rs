//! The representation of datasets (package data, decision DATA.1): a dataset is the term
//! `dataset(assoc(c1 -> col1, ..., cm -> colm))`, an association from column names to columns, every column a list of
//! the same length (one element per row). Columns of machine integers or reals are packed arrays (semantics 11), so
//! a numeric column of a million rows is one node. A row is the association `assoc(c1 -> v1, ..., cm -> vm)`; it is
//! built when a function asks for it, never stored.
//!
//! [`Col`] reads a column's elements without unpacking it, [`Table`] is a dataset taken apart, and the builders
//! here make dataset terms from columns.
use mc_api::names;
use mc_core::{Heap, Packed, Store, Sym, View, T};

/// The symbols a dataset is made of. Natives keep one in their shared state; the public functions of the crate make
/// one for the call.
pub(crate) struct DataSyms {
    pub list: Sym,
    pub rule: Sym,
    pub assoc: Sym,
    pub dataset: Sym,
    /// `missing()`: an absent value (decision DATA.4)
    pub missing: Sym,
}

impl DataSyms {
    pub fn new(h: &Heap) -> DataSyms {
        DataSyms {
            list: h.sym(names::LIST),
            rule: h.sym(names::RULE),
            assoc: h.sym("assoc"),
            dataset: h.sym("dataset"),
            missing: h.sym("missing"),
        }
    }

    /// The absent value `missing()`.
    pub fn missing<'h>(&self, h: &'h Heap) -> T<'h> {
        h.call(self.missing, &[])
    }

    /// Is `t` an absent value `missing(...)` (with or without a reason)?
    pub fn is_missing(&self, h: &Heap, t: T<'_>) -> bool {
        h.packed(t).is_none() && matches!(h.view(t), View::App { head, .. } if head == h.sym_term(self.missing))
    }

    /// Is `t` a call `dataset(...)` (in normal form or not)?
    pub fn is_dataset_call(&self, h: &Heap, t: T<'_>) -> bool {
        h.packed(t).is_none() && matches!(h.view(t), View::App { head, .. } if head == h.sym_term(self.dataset))
    }

    /// The dataset with these columns: `names` distinct, `cols` lists of one length (not checked).
    pub fn dataset<'h>(&self, h: &'h Heap, names: &[T<'h>], cols: &[T<'h>]) -> T<'h> {
        let entries: Vec<T<'h>> = names.iter().zip(cols).map(|(&k, &c)| h.call(self.rule, &[k, c])).collect();
        h.call(self.dataset, &[h.call(self.assoc, &entries)])
    }
}

/// The elements of a dataset's column, read from a packed array's payload when it has one.
#[derive(Clone, Copy, Debug)]
pub enum Col<'h> {
    /// a packed vector of machine integers
    Ints(&'h [i64]),
    /// a packed vector of machine reals
    Reals(&'h [f64]),
    /// any other list
    Terms(&'h [T<'h>]),
}

impl<'h> Col<'h> {
    /// The elements of the list `t` (`None` when `t` is not a list).
    pub(crate) fn of(s: &DataSyms, h: &'h Heap, t: T<'h>) -> Option<Col<'h>> {
        match h.packed(t) {
            Some(Packed::Ints { dims: [_], data }) => return Some(Col::Ints(data)),
            Some(Packed::Reals { dims: [_], data }) => return Some(Col::Reals(data)),
            // an array of higher rank: its rows are the elements
            _ => {}
        }
        match h.view(t) {
            View::App { head, args } if head == h.sym_term(s.list) => Some(Col::Terms(args)),
            _ => None,
        }
    }

    /// The number of elements.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            Col::Ints(d) => d.len(),
            Col::Reals(d) => d.len(),
            Col::Terms(d) => d.len(),
        }
    }

    /// Element `i` (0-based).
    pub fn get(&self, h: &'h Heap, i: usize) -> T<'h> {
        match self {
            Col::Ints(d) => h.int(d[i]),
            Col::Reals(d) => h.real(d[i]),
            Col::Terms(d) => d[i],
        }
    }

    /// The list of the elements at the positions `ix`, in that order.
    pub(crate) fn gather(&self, s: &DataSyms, h: &'h Heap, ix: &[usize]) -> T<'h> {
        match self {
            Col::Ints(d) => h.ints(&ix.iter().map(|&i| d[i]).collect::<Vec<_>>()),
            Col::Reals(d) => h.reals(&ix.iter().map(|&i| d[i]).collect::<Vec<_>>()),
            Col::Terms(d) => h.call(s.list, &ix.iter().map(|&i| d[i]).collect::<Vec<_>>()),
        }
    }

    /// The list of the elements at the positions `ix`, `missing()` where a position is `None`.
    pub(crate) fn gather_or_missing(&self, s: &DataSyms, h: &'h Heap, ix: &[Option<usize>]) -> T<'h> {
        if ix.iter().all(Option::is_some) {
            let ix: Vec<usize> = ix.iter().flatten().copied().collect();
            return self.gather(s, h, &ix);
        }
        let m = s.missing(h);
        let v: Vec<T<'h>> = ix.iter().map(|i| i.map_or(m, |i| self.get(h, i))).collect();
        h.call(s.list, &v)
    }
}

/// A dataset in normal form, taken apart.
pub(crate) struct Table<'h> {
    /// the column names, distinct, in order
    pub names: Vec<T<'h>>,
    /// the columns
    pub cols: Vec<Col<'h>>,
    /// the column terms (lists)
    pub terms: Vec<T<'h>>,
    /// the number of rows
    pub n: usize,
}

impl<'h> Table<'h> {
    /// The table of `t` when it is a dataset in normal form: `dataset(a)` with `a` an association of lists of one
    /// length. The association's keys are distinct because it is in normal form.
    pub fn of(s: &DataSyms, h: &'h Heap, t: T<'h>) -> Option<Table<'h>> {
        if !s.is_dataset_call(h, t) {
            return None;
        }
        let View::App { args: &[a], .. } = h.view(t) else { return None };
        let View::App { head, args: entries } = h.view(a) else { return None };
        if head != h.sym_term(s.assoc) || h.packed(a).is_some() {
            return None;
        }
        let mut tab = Table { names: vec![], cols: vec![], terms: vec![], n: 0 };
        for &e in entries {
            let View::App { head, args: &[k, v] } = h.view(e) else { return None };
            if head != h.sym_term(s.rule) {
                return None;
            }
            let c = Col::of(s, h, v)?;
            if !tab.cols.is_empty() && c.len() != tab.n {
                return None;
            }
            tab.n = c.len();
            tab.names.push(k);
            tab.cols.push(c);
            tab.terms.push(v);
        }
        Some(tab)
    }

    /// The position of the column named `k` (compared by identity).
    pub fn column(&self, k: T<'h>) -> Option<usize> {
        self.names.iter().position(|&c| c == k)
    }

    /// Row `i` (0-based) as the association `assoc(c1 -> v1, ...)`.
    pub fn row(&self, s: &DataSyms, h: &'h Heap, i: usize) -> T<'h> {
        let entries: Vec<T<'h>> =
            self.names.iter().zip(&self.cols).map(|(&k, c)| h.call(s.rule, &[k, c.get(h, i)])).collect();
        h.call(s.assoc, &entries)
    }

    /// The dataset of the rows at the positions `ix`, in that order.
    pub fn gather(&self, s: &DataSyms, h: &'h Heap, ix: &[usize]) -> T<'h> {
        let cols: Vec<T<'h>> = self.cols.iter().map(|c| c.gather(s, h, ix)).collect();
        s.dataset(h, &self.names, &cols)
    }

    /// The dataset of the rows `lo..hi`.
    pub fn slice(&self, s: &DataSyms, h: &'h Heap, lo: usize, hi: usize) -> T<'h> {
        let ix: Vec<usize> = (lo..hi).collect();
        self.gather(s, h, &ix)
    }
}

/// Columns collected element by element, `missing()` where a row has no value.
pub(crate) struct Columns<'h> {
    pub names: Vec<T<'h>>,
    pub cols: Vec<Vec<T<'h>>>,
    /// the number of rows so far
    pub n: usize,
    at: rustc_hash::FxHashMap<T<'h>, usize>,
    missing: T<'h>,
}

impl<'h> Columns<'h> {
    pub fn new(s: &DataSyms, h: &'h Heap) -> Columns<'h> {
        Columns { names: vec![], cols: vec![], n: 0, at: Default::default(), missing: s.missing(h) }
    }

    /// The position of the column `k`, added (missing in every earlier row) when new.
    pub fn column(&mut self, k: T<'h>) -> usize {
        if let Some(&p) = self.at.get(&k) {
            return p;
        }
        self.at.insert(k, self.names.len());
        self.names.push(k);
        self.cols.push(vec![self.missing; self.n]);
        self.names.len() - 1
    }

    /// Start a new row (every column missing until set).
    pub fn new_row(&mut self) {
        self.n += 1;
        for c in &mut self.cols {
            c.push(self.missing);
        }
    }

    /// Set column `p` of the current row.
    pub fn set(&mut self, p: usize, v: T<'h>) {
        let last = self.n - 1;
        self.cols[p][last] = v;
    }

    /// The dataset.
    pub fn finish(self, s: &DataSyms, h: &'h Heap) -> T<'h> {
        let cols: Vec<T<'h>> = self.cols.iter().map(|c| h.call(s.list, c)).collect();
        s.dataset(h, &self.names, &cols)
    }
}
