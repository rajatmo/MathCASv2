//! `parametric_n_dsolve_value(eqs, expr, [t, t0, t1], [p1, ..., pm])`: the equations reduced once, with the
//! parameters as symbols, into the term `parametric_function(params, expr, data)`; each call `f(v1, ..., vm)`
//! substitutes the values into the reduced right-hand side and the initial values and integrates (decision N3.12).
//!
//! The reduced system lives in the term itself (`data`), so nothing outlives a native call but terms the session
//! holds: `[t, start, a, b, [state symbols], [right-hand sides], [initial values], [[y, order, at], ...], options]`
//! (`options` is the call's trailing `options(...)` term, or `null`).
use crate::system::{System, Unknown};
use mc_api::names;
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::NativeCx;

/// The `parametric_function(params, expr, data)` term of a reduced system.
pub(crate) fn term<'h>(
    h: &'h Heap,
    ar: &Arith<'h, Heap>,
    sys: &System<'h>,
    params: T<'h>,
    expr: T<'h>,
    (a, b): (&Num, &Num),
    options: T<'h>,
) -> T<'h> {
    let list = |items: &[T<'h>]| h.call(h.sym(names::LIST), items);
    let state: Vec<T<'h>> = sys.state.iter().map(|&s| h.sym_term(s)).collect();
    let mut initial: Vec<T<'h>> = sys.initial.iter().map(|v| ar.number(v)).collect();
    for &(k, e) in &sys.initial_terms {
        initial[k] = e;
    }
    let unknowns: Vec<T<'h>> =
        sys.unknowns.iter().map(|u| list(&[h.sym_term(u.sym), h.int(u.order as i64), h.int(u.at as i64)])).collect();
    let data = list(&[
        h.sym_term(sys.t),
        sys.start,
        ar.number(a),
        ar.number(b),
        list(&state),
        sys.rhs,
        list(&initial),
        list(&unknowns),
        options,
    ]);
    h.call(h.sym("parametric_function"), &[params, expr, data])
}

/// The parts of a `parametric_function` term.
pub(crate) struct Parts<'h> {
    /// the parameters; `None` for one a rule has already replaced by a number (`rewrite(f(a), a -> 1.0)` also
    /// rewrites the `a` inside the term, as `find_root` does): its value is in the system already
    pub params: Vec<Option<Sym>>,
    pub t: Sym,
    pub start: T<'h>,
    pub a: Num,
    pub b: Num,
    pub state: Vec<Sym>,
    pub rhs: T<'h>,
    pub initial: Vec<T<'h>>,
    pub unknowns: Vec<Unknown>,
    pub options: Option<T<'h>>,
}

impl<'h> Parts<'h> {
    /// The parts of `parametric_function(params, _, data)`; `None` when the term was not made by
    /// `parametric_n_dsolve_value` (a hand-written one with the wrong shape).
    pub fn read(cx: &NativeCx<'_, '_, 'h>, params: T<'h>, data: T<'h>) -> Option<Parts<'h>> {
        let h = cx.heap();
        let ar = Arith::new(h);
        let sym = |u: T<'h>| match h.view(u) {
            View::Sym(s) => Some(s),
            _ => None,
        };
        let int = |u: T<'h>| match h.view(u) {
            View::Int(v @ 0..) => Some(v as usize),
            _ => None,
        };
        let param = |u: T<'h>| match sym(u) {
            Some(s) => Some(Some(s)),
            None => ar.num(u).map(|_| None),
        };
        let params = cx.as_list(params)?.iter().map(|&u| param(u)).collect::<Option<Vec<Option<Sym>>>>()?;
        let &[t, start, a, b, state, rhs, initial, unknowns, options] = cx.as_list(data)? else { return None };
        let state = cx.as_list(state)?.iter().map(|&u| sym(u)).collect::<Option<Vec<Sym>>>()?;
        let n = state.len();
        let initial = cx.as_list(initial)?.to_vec();
        if cx.as_list(rhs)?.len() != n || initial.len() != n || ar.num(start).is_none() {
            return None;
        }
        let mut us = vec![];
        for &u in cx.as_list(unknowns)? {
            let &[y, order, at] = cx.as_list(u)? else { return None };
            let (order, at) = (int(order)?, int(at)?);
            if order == 0 || at + order > n {
                return None;
            }
            us.push(Unknown { sym: sym(y)?, order, at });
        }
        let options = (options != cx.null()).then_some(options);
        Some(Parts { params, t: sym(t)?, start, a: ar.num(a)?, b: ar.num(b)?, state, rhs, initial, unknowns: us, options })
    }
}
