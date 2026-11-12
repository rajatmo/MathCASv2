//! The reduction of the user's equations to a first-order system `Y' = F(t, Y)` with initial values (section O of
//! the phase 4A plan).
//!
//! - Every equation is either a **differential equation**, in which the unknowns occur only as `y(t)` and
//!   `derivative(k)(y)(t)` (prime notation `y'(t)` lowers to the latter), or an **initial condition**
//!   `y(s) == v` / `derivative(k)(y)(s) == v` at a numeric point s.
//! - An unknown whose highest derivative is of order m becomes m state components `y, y', ..., y^(m-1)`.
//! - The equations are solved for the highest derivatives when they are linear in them (decision N3.1): each
//!   equation `lhs == rhs` gives the residual `r = lhs - rhs = A h + b` in the highest derivatives `h` (the
//!   coefficients `A = d(r, h)` through `mc-calculus`, which must be free of `h`). A constant numeric `A` is
//!   inverted exactly; otherwise each equation must hold exactly one highest derivative (a diagonal `A` up to the
//!   order of the equations). Anything else is `failure(argument, n_dsolve, "implicit equation")`.
use crate::util::{self, mentions};
use mc_api::names;
use mc_arith::{Arith, Num};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Res};

/// An unknown function of the system.
#[derive(Clone, Debug)]
pub(crate) struct Unknown {
    /// its symbol
    pub sym: Sym,
    /// the order of its highest derivative
    pub order: usize,
    /// the state index of the function itself (its derivatives follow)
    pub at: usize,
}

/// The first-order system.
pub(crate) struct System<'h> {
    /// the independent variable
    pub t: Sym,
    /// the unknowns, in the order given
    pub unknowns: Vec<Unknown>,
    /// the state symbols `s1, ..., sn` the right-hand side is written in
    pub state: Vec<Sym>,
    /// the right-hand side: a list of n expressions in `t` and the state symbols
    pub rhs: T<'h>,
    /// the point of the initial conditions (a number term)
    pub start: T<'h>,
    /// the initial state (numbers, exact where given exactly)
    pub initial: Vec<Num>,
    /// initial values that depend on the parameters of `parametric_n_dsolve_value`: (component, expression); the
    /// component's entry of `initial` is a placeholder until the parameters have values
    pub initial_terms: Vec<(usize, T<'h>)>,
    /// a boundary value problem (decision N3.20): the components of the initial state not given at the start
    /// (placeholders in `initial`), found by shooting so that the conditions `(component, point, value)` at the
    /// other point hold; both empty for an initial value problem
    pub free: Vec<usize>,
    /// see [`System::free`]
    pub targets: Vec<(usize, Num, Num)>,
}

impl<'h> System<'h> {
    /// `expr` in the variable and the state symbols: `y^(d)(t)` becomes the state symbol for d below the order
    /// and the right-hand side's expression for the highest derivative. `Err` for other uses of the unknowns.
    pub fn in_state(&self, cx: &mut NativeCx<'_, '_, 'h>, expr: T<'h>) -> Res<Result<T<'h>, Refusal>> {
        let h = cx.heap();
        let ys: Vec<Sym> = self.unknowns.iter().map(|u| u.sym).collect();
        let derivative = h.sym_term(h.sym("derivative"));
        let tt = h.sym_term(self.t);
        let rhs = cx.as_list(self.rhs).expect("the right-hand side is a list");
        let mut refused = None;
        let swapped = mc_numfn::replace(h, expr, &mut |u| {
            let (i, d, x) = occurrence(h, &ys, derivative, u)?;
            let un = &self.unknowns[i];
            if x != tt || d > un.order {
                refused = Some(format!("{} must be applied to {}", printed_name(h, un.sym, d), h.sym_name(self.t)));
                return None;
            }
            Some(if d < un.order { h.sym_term(self.state[un.at + d]) } else { rhs[un.at + un.order - 1] })
        });
        if let Some(m) = refused {
            return Ok(Err(m));
        }
        if mentions(cx, swapped, &ys)? {
            return Ok(Err("an unknown must be applied to the variable".into()));
        }
        Ok(Ok(cx.eval(swapped)?))
    }

    /// The size of the state.
    pub fn n(&self) -> usize {
        self.state.len()
    }

    /// The unknown and derivative order of state component `i`.
    pub fn component(&self, i: usize) -> (Sym, usize) {
        let u = self.unknowns.iter().rev().find(|u| u.at <= i).expect("every component belongs to an unknown");
        (u.sym, i - u.at)
    }
}

/// Why the equations were refused: the message of `failure(argument, n_dsolve, message)`.
pub(crate) type Refusal = String;

/// One occurrence of an unknown: `(unknown index, derivative order, argument)`.
type Occurrence<'h> = (usize, usize, T<'h>);

struct Reader<'h> {
    h: &'h Heap,
    unknowns: Vec<Sym>,
    derivative: T<'h>,
}

impl<'h> Reader<'h> {
    /// `y(x)` or `derivative(k)(y)(x)` for an unknown `y`.
    fn occurrence(&self, u: T<'h>) -> Option<Occurrence<'h>> {
        occurrence(self.h, &self.unknowns, self.derivative, u)
    }

    /// Every occurrence of an unknown in `t`; `Err` when an unknown is used otherwise (bare, or with another
    /// number of arguments).
    fn occurrences(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Result<Vec<Occurrence<'h>>, Refusal>> {
        let h = self.h;
        let mut out = vec![];
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if !seen.insert(u) {
                continue;
            }
            if seen.len().is_multiple_of(1024) {
                cx.tick(1)?;
            }
            if let Some(o) = self.occurrence(u) {
                out.push(o);
                continue;
            }
            match h.view(u) {
                View::Sym(s) if self.unknowns.contains(&s) => {
                    return Ok(Err(format!("{} must be applied to the variable", h.sym_name(s))));
                }
                View::App { head, args } => {
                    stack.push(head);
                    stack.extend_from_slice(args);
                }
                _ => {}
            }
        }
        Ok(Ok(out))
    }
}

/// `y(x)` or `derivative(k)(y)(x)` for one of the unknowns `ys` (`derivative` is the term of that symbol): the
/// unknown's index, the order and the argument.
pub(crate) fn occurrence<'h>(h: &'h Heap, ys: &[Sym], derivative: T<'h>, u: T<'h>) -> Option<Occurrence<'h>> {
    let View::App { head, args: &[x] } = h.view(u) else { return None };
    match h.view(head) {
        View::Sym(s) => ys.iter().position(|&y| y == s).map(|i| (i, 0, x)),
        View::App { head: op, args: &[f] } => {
            let View::App { head: d, args: &[k] } = h.view(op) else { return None };
            let (View::Int(k @ 0..), View::Sym(s)) = (h.view(k), h.view(f)) else { return None };
            if d != derivative {
                return None;
            }
            ys.iter().position(|&y| y == s).map(|i| (i, k as usize, x))
        }
        _ => None,
    }
}

/// The unknowns of equations given without them (`n_dsolve_value`): every symbol `y` of a derivative
/// `derivative(k)(y)(x)` with k >= 1, in the order of first appearance (depth first, left to right).
pub(crate) fn inferred_unknowns<'h>(cx: &mut NativeCx<'_, '_, 'h>, eqs: T<'h>) -> Res<Vec<Sym>> {
    let h = cx.heap();
    let derivative = h.sym_term(h.sym("derivative"));
    let mut out: Vec<Sym> = vec![];
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![eqs];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if seen.len().is_multiple_of(1024) {
            cx.tick(1)?;
        }
        let View::App { head, args } = h.view(u) else { continue };
        if let View::App { head: op, args: &[f] } = h.view(head) {
            if let (View::App { head: d, args: &[k] }, View::Sym(s)) = (h.view(op), h.view(f)) {
                if d == derivative && matches!(h.view(k), View::Int(1..)) && !out.contains(&s) {
                    out.push(s);
                }
            }
        }
        stack.extend(args.iter().rev());
        stack.push(head);
    }
    Ok(out)
}

/// The unknowns: a symbol `y` (or `y(t)`), or a list of them.
pub(crate) fn unknowns<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<(Vec<Sym>, bool)> {
    let h = cx.heap();
    let one = |u: T<'h>| match h.view(u) {
        View::Sym(s) => Some(s),
        View::App { head, args: [_] } => match h.view(head) {
            View::Sym(s) => Some(s),
            _ => None,
        },
        _ => None,
    };
    let (syms, list) = match cx.as_list(t) {
        Some(items) => (items.iter().map(|&u| one(u)).collect::<Option<Vec<Sym>>>()?, true),
        None => (vec![one(t)?], false),
    };
    let distinct: std::collections::HashSet<Sym> = syms.iter().copied().collect();
    (!syms.is_empty() && distinct.len() == syms.len()).then_some((syms, list))
}

/// The equations: a list (nested lists flattened) or a single equation; a conjunction `e1 and e2` (as a chain
/// `x(0) == y(0) == 1` is written) gives its equations.
fn equations<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let and = h.sym_term(h.sym("and"));
    let mut out = vec![];
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        cx.tick(1)?;
        match cx.as_list(u) {
            Some(items) => stack.extend(items.iter().rev()),
            None => match h.view(u) {
                View::App { head, args } if head == and => stack.extend(args.iter().rev()),
                _ => out.push(u),
            },
        }
    }
    Ok(out)
}

/// Reduce `eqs` in the unknowns `ys` and the independent variable `t` to a first-order system; closed initial
/// values and points are evaluated by `n` to `digits` (machine precision for `None`); an initial value that
/// mentions one of the parameters `params` is kept as an expression (`System::initial_terms`). `Ok(Err(message))`
/// when the equations do not have a supported form.
pub(crate) fn reduce<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    eqs: T<'h>,
    ys: &[Sym],
    t: Sym,
    digits: Option<u64>,
    params: &[Sym],
) -> Res<Result<System<'h>, Refusal>> {
    let h = cx.heap();
    if ys.contains(&t) {
        return Ok(Err("the variable is also an unknown".into()));
    }
    let equal = h.sym_term(h.sym("equal"));
    let reader = Reader { h, unknowns: ys.to_vec(), derivative: h.sym_term(h.sym("derivative")) };
    let tt = h.sym_term(t);
    let mut differential: Vec<T<'h>> = vec![];
    // (unknown, order, point, value), the value a number or (with parameters) an expression
    let mut conditions: Vec<(usize, usize, Num, Result<Num, T<'h>>)> = vec![];
    let mut orders = vec![0usize; ys.len()];
    let mut seen_derivative = vec![false; ys.len()];
    // conditions equating two occurrences: (equation index, occurrence, occurrence)
    let mut linked = vec![];
    for (k, e) in equations(cx, eqs)?.into_iter().enumerate() {
        let View::App { head, args: &[l, r] } = h.view(e) else {
            return Ok(Err(format!("equation {} is not an equation lhs == rhs", k + 1)));
        };
        if head != equal {
            return Ok(Err(format!("equation {} is not an equation lhs == rhs", k + 1)));
        }
        let occ = match reader.occurrences(cx, e)? {
            Ok(o) => o,
            Err(m) => return Ok(Err(m)),
        };
        if occ.is_empty() {
            return Ok(Err(format!("equation {} has no unknown", k + 1)));
        }
        if occ.iter().all(|o| o.2 == tt) {
            for &(i, d, _) in &occ {
                orders[i] = orders[i].max(d);
                seen_derivative[i] |= d > 0;
            }
            differential.push(e);
            continue;
        }
        // an initial condition: one side an occurrence at a numeric point, the other a number
        let cond = match (reader.occurrence(l), reader.occurrence(r)) {
            (Some(o), _) if occ.len() == 1 => Some((o, r)),
            (_, Some(o)) if occ.len() == 1 => Some((o, l)),
            _ => None,
        };
        // `x(0) == y(0)`: two occurrences at points; resolved below from the other conditions
        if let (Some(o1), Some(o2), 2) = (reader.occurrence(l), reader.occurrence(r), occ.len()) {
            if o1.2 != tt && o2.2 != tt {
                linked.push((k, o1, o2));
                continue;
            }
        }
        let Some(((i, d, x), v)) = cond else {
            return Ok(Err(format!("equation {} is neither a differential equation in {} nor an initial condition", k + 1, h.sym_name(t))));
        };
        let Some(x) = util::number(cx, ar, x, digits)? else {
            return Ok(Err(format!("initial condition {} is not numeric", k + 1)));
        };
        let v = match util::number(cx, ar, v, digits)? {
            Some(n) => Ok(n),
            None if !params.is_empty() && mentions(cx, v, params)? => Err(v),
            None => return Ok(Err(format!("initial condition {} is not numeric", k + 1))),
        };
        conditions.push((i, d, x, v));
    }
    // the linked conditions take the value of a condition at the same unknown, order and point
    while !linked.is_empty() {
        let before = linked.len();
        let mut rest = vec![];
        for (k, (i1, d1, x1), (i2, d2, x2)) in linked {
            let (Some(p1), Some(p2)) = (util::number(cx, ar, x1, digits)?, util::number(cx, ar, x2, digits)?) else {
                return Ok(Err(format!("initial condition {} is not numeric", k + 1)));
            };
            let find = |i: usize, d: usize, p: &Num| {
                conditions.iter().find(|c| c.0 == i && c.1 == d && util::real(&c.2) == util::real(p)).map(|c| c.3.clone())
            };
            match (find(i1, d1, &p1), find(i2, d2, &p2)) {
                (Some(v), None) => conditions.push((i2, d2, p2, v)),
                (None, Some(v)) => conditions.push((i1, d1, p1, v)),
                (Some(_), Some(_)) => {}
                (None, None) => rest.push((k, (i1, d1, x1), (i2, d2, x2))),
            }
        }
        if rest.len() == before {
            let k = rest[0].0;
            return Ok(Err(format!("initial condition {} is not numeric", k + 1)));
        }
        linked = rest;
    }
    for (i, &y) in ys.iter().enumerate() {
        if !seen_derivative[i] {
            return Ok(Err(format!("no derivative of {} occurs (algebraic equations are not supported)", h.sym_name(y))));
        }
    }
    if differential.len() != ys.len() {
        return Ok(Err(format!("{} differential equations for {} unknowns", differential.len(), ys.len())));
    }
    // the state
    let mut unknowns = Vec::with_capacity(ys.len());
    let mut n = 0;
    for (i, &sym) in ys.iter().enumerate() {
        unknowns.push(Unknown { sym, order: orders[i], at: n });
        n += orders[i];
    }
    let state: Vec<Sym> = (1..=n).map(|k| h.sym(&format!("ode·s{k}"))).collect();
    let highest: Vec<Sym> = (1..=ys.len()).map(|k| h.sym(&format!("ode·h{k}"))).collect();
    // the residuals in the state and the highest derivatives
    let mut residuals = Vec::with_capacity(ys.len());
    for &e in &differential {
        let View::App { args: &[l, r], .. } = h.view(e) else { unreachable!("checked above") };
        let swapped = mc_numfn::replace(h, ar.sub(l, r), &mut |u| {
            let (i, d, x) = reader.occurrence(u)?;
            if x != tt {
                return None;
            }
            let un = &unknowns[i];
            Some(h.sym_term(if d == un.order { highest[i] } else { state[un.at + d] }))
        });
        residuals.push(cx.eval(swapped)?);
    }
    let rhs_highest = match solve_highest(cx, ar, &residuals, &highest)? {
        Ok(v) => v,
        Err(m) => return Ok(Err(m)),
    };
    let mut rhs = Vec::with_capacity(n);
    for (u, &hi) in unknowns.iter().zip(&rhs_highest) {
        for k in 0..u.order {
            rhs.push(if k + 1 < u.order { h.sym_term(state[u.at + k + 1]) } else { hi });
        }
    }
    let rhs = cx.eval(cx.list(&rhs))?;
    // the initial conditions: one per component at the start, the point with the most conditions; conditions at
    // one other point make a boundary value problem
    let mut initial: Vec<Option<Result<Num, T<'h>>>> = vec![None; n];
    let mut points: Vec<(Num, usize)> = vec![];
    for (_, _, x, _) in &conditions {
        match points.iter_mut().find(|(p, _)| mc_arith::number::equal(p, x) == Some(true)) {
            Some(e) => e.1 += 1,
            None => points.push((x.clone(), 1)),
        }
    }
    if points.len() > 2 {
        return Ok(Err("the conditions are at more than two points".into()));
    }
    // the first point with the most conditions
    let mut start: Option<Num> = None;
    let mut best = 0;
    for (p, c) in &points {
        if *c > best {
            best = *c;
            start = Some(p.clone());
        }
    }
    let mut targets = vec![];
    for (i, d, x, v) in conditions {
        let u = &unknowns[i];
        if d >= u.order {
            return Ok(Err(format!("the initial condition {} is of the highest order", printed_name(h, u.sym, d))));
        }
        if start.as_ref().is_some_and(|s| mc_arith::number::equal(s, &x) != Some(true)) {
            let Ok(v) = v else { return Ok(Err("a boundary condition depends on a parameter".into())) };
            targets.push((u.at + d, x, v));
            continue;
        }
        let slot = &mut initial[u.at + d];
        if let Some(old) = slot {
            let same = match (&*old, &v) {
                (Ok(a), Ok(b)) => mc_arith::number::equal(a, b) == Some(true),
                (Err(a), Err(b)) => a == b,
                _ => false,
            };
            if !same {
                return Ok(Err(format!("conflicting initial conditions for {}", printed_name(h, u.sym, d))));
            }
        }
        *slot = Some(v);
    }
    let Some(start) = start else { return Ok(Err("no initial conditions".into())) };
    let mut init = Vec::with_capacity(n);
    let mut initial_terms = vec![];
    let mut free = vec![];
    for (k, v) in initial.into_iter().enumerate() {
        match v {
            Some(Ok(v)) => init.push(v),
            Some(Err(e)) => {
                init.push(Num::int(0));
                initial_terms.push((k, e));
            }
            None if free.len() < targets.len() => {
                init.push(Num::int(0));
                free.push(k);
            }
            None => {
                let u = unknowns.iter().rev().find(|u| u.at <= k).expect("a component's unknown");
                return Ok(Err(format!("missing initial condition for {}", printed_name(h, u.sym, k - u.at))));
            }
        }
    }
    if free.len() != targets.len() {
        return Ok(Err(format!("{} boundary conditions for {} unknown initial values", targets.len(), free.len())));
    }
    Ok(Ok(System { t, unknowns, state, rhs, start: ar.number(&start), initial: init, initial_terms, free, targets }))
}

/// `y`, `y'`, `y''`, ... for messages.
pub(crate) fn printed_name(h: &Heap, s: Sym, d: usize) -> String {
    format!("{}{}", h.sym_name(s), "'".repeat(d))
}

/// The highest derivatives solved from the residuals `r_e = A h + b` (decision N3.1).
fn solve_highest<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    residuals: &[T<'h>],
    highest: &[Sym],
) -> Res<Result<Vec<T<'h>>, Refusal>> {
    let h = cx.heap();
    let m = highest.len();
    if let Some(out) = explicit_highest(cx, ar, residuals, highest)? {
        return Ok(Ok(out));
    }
    let implicit = || Err("implicit equation".to_string());
    let hs: Vec<T<'h>> = highest.iter().map(|&s| h.sym_term(s)).collect();
    let spec = cx.list(&hs);
    let mut a = Vec::with_capacity(m * m);
    let mut b = Vec::with_capacity(m);
    for &r in residuals {
        let row = cx.eval(h.call(h.sym("d"), &[r, spec]))?;
        let Some(items) = cx.as_list(row) else { return Ok(implicit()) };
        if items.len() != m || mentions(cx, row, highest)? || mentions(cx, row, &[h.sym("d"), h.sym("derivative")])? {
            return Ok(implicit());
        }
        a.extend_from_slice(items);
        let zeroed = mc_numfn::replace(h, r, &mut |u| match h.view(u) {
            View::Sym(s) if highest.contains(&s) => Some(h.int(0)),
            _ => None,
        });
        b.push(cx.eval(zeroed)?);
    }
    let neg_b: Vec<T<'h>> = b.iter().map(|&v| ar.neg(v)).collect();
    // a constant numeric matrix: invert it exactly
    let nums: Option<Vec<Num>> = a.iter().map(|&e| ar.num(e)).collect();
    if let Some(nums) = nums {
        let Some(inv) = invert(m, &nums) else { return Ok(implicit()) };
        let mut out = Vec::with_capacity(m);
        for j in 0..m {
            let terms: Vec<T<'h>> = (0..m)
                .filter(|&e| !inv[j * m + e].is_exact_zero())
                .map(|e| h.call(h.sym(names::MUL), &[ar.number(&inv[j * m + e]), neg_b[e]]))
                .collect();
            let sum = if terms.is_empty() { h.int(0) } else { h.call(h.sym(names::ADD), &terms) };
            out.push(cx.eval(sum)?);
        }
        return Ok(Ok(out));
    }
    // otherwise one highest derivative per equation
    let zero = |t: T<'h>| ar.num(t).is_some_and(|n| n.is_exact_zero());
    let mut out: Vec<Option<T<'h>>> = vec![None; m];
    for e in 0..m {
        let nonzero: Vec<usize> = (0..m).filter(|&j| !zero(a[e * m + j])).collect();
        let &[j] = nonzero.as_slice() else { return Ok(implicit()) };
        if out[j].is_some() {
            return Ok(implicit());
        }
        out[j] = Some(cx.eval(h.call(h.sym(names::MUL), &[neg_b[e], h.call(h.sym(names::POW), &[a[e * m + j], h.int(-1)])]))?);
    }
    Ok(Ok(out.into_iter().map(|v| v.expect("a bijection")).collect()))
}

/// The common explicit form without differentiating: every residual is a sum with one term `h_j` or `-h_j` and no
/// other occurrence of a highest derivative, each `h_j` in one residual. Then `A` is a signed permutation and
/// `h_j = -(r - h_j)` (or `r + h_j`): the result of the general path, as `A` is inverted exactly. `None` for any
/// other form.
fn explicit_highest<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    ar: &Arith<'h, Heap>,
    residuals: &[T<'h>],
    highest: &[Sym],
) -> Res<Option<Vec<T<'h>>>> {
    let h = cx.heap();
    let add = h.sym_term(h.sym(names::ADD));
    let mul = h.sym_term(h.sym(names::MUL));
    let minus_one = h.int(-1);
    let mut out: Vec<Option<T<'h>>> = vec![None; highest.len()];
    for &r in residuals {
        let View::App { head, args } = h.view(r) else { return Ok(None) };
        if head != add {
            return Ok(None);
        }
        // the one argument that is +-h_j
        let mut found = None;
        for (i, &a) in args.iter().enumerate() {
            let (sign, s) = match h.view(a) {
                View::Sym(s) => (1, s),
                View::App { head, args: &[c, x] } if head == mul && c == minus_one => match h.view(x) {
                    View::Sym(s) => (-1, s),
                    _ => continue,
                },
                _ => continue,
            };
            if let Some(j) = highest.iter().position(|&q| q == s) {
                if found.is_some() {
                    return Ok(None);
                }
                found = Some((i, j, sign));
            }
        }
        let Some((i, j, sign)) = found else { return Ok(None) };
        if out[j].is_some() {
            return Ok(None);
        }
        let rest: Vec<T<'h>> = args.iter().enumerate().filter(|&(k, _)| k != i).map(|(_, &a)| a).collect();
        let b = ar.add(&rest);
        if mentions(cx, b, highest)? {
            return Ok(None);
        }
        out[j] = Some(if sign == 1 { cx.eval(ar.neg(b))? } else { cx.eval(b)? });
    }
    Ok(out.into_iter().collect())
}

/// The inverse of an m by m matrix of numbers by Gauss–Jordan elimination (exact for exact entries); `None` when
/// it is singular.
fn invert(m: usize, a: &[Num]) -> Option<Vec<Num>> {
    let mut w = a.to_vec();
    let mut inv: Vec<Num> = (0..m * m).map(|k| Num::int(i64::from(k / m == k % m))).collect();
    let mag = |v: &Num| match v {
        Num::Real(r) => r.to_f64().abs(),
        Num::Complex(re, im) => re.to_f64().hypot(im.to_f64()),
    };
    for c in 0..m {
        let p = (c..m).filter(|&r| !w[r * m + c].is_exact_zero()).max_by(|&i, &j| mag(&w[i * m + c]).total_cmp(&mag(&w[j * m + c])))?;
        for k in 0..m {
            w.swap(c * m + k, p * m + k);
            inv.swap(c * m + k, p * m + k);
        }
        let r = w[c * m + c].recip().ok()?;
        for k in 0..m {
            w[c * m + k] = w[c * m + k].mul(&r);
            inv[c * m + k] = inv[c * m + k].mul(&r);
        }
        for i in 0..m {
            if i == c || w[i * m + c].is_exact_zero() {
                continue;
            }
            let f = w[i * m + c].clone();
            for k in 0..m {
                w[i * m + k] = w[i * m + k].add(&f.mul(&w[c * m + k]).neg());
                inv[i * m + k] = inv[i * m + k].add(&f.mul(&inv[c * m + k]).neg());
            }
        }
    }
    Some(inv)
}
