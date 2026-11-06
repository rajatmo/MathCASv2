//! Exact global optimization and instances (decision 2C.14): `minimize`, `maximize`, `min_value`, `max_value`,
//! `arg_min`, `arg_max`, `find_instance`.
//!
//! `minimize(f, x)`, `minimize([f, cond], x)`, `minimize(f, [x, y])`: the global minimum `[value, [x -> point]]`.
//!
//! - **One variable**: the candidates are the real critical points (`solve(d(f, x) == 0, x)` over the reals), the
//!   ends of the intervals of `reduce(cond, x, reals)` (the closed ones; an open end or an infinity contributes its
//!   limit, which bounds the values without being attained), and the points where the domain's pieces are single
//!   points. The smallest value attained wins; when a limit is smaller (or equal and not attained) the minimum is
//!   not attained: an infinite limit gives `[-inf, [x -> -inf]]` (the side where it is approached), a finite one
//!   leaves the call unevaluated.
//! - **Several variables**: the real critical points of the gradient (`solve`), when the function is bounded below
//!   by the smallest critical value along every coordinate direction is not checked; only polynomials of even
//!   total degree with a positive definite leading form are treated (a convex quadratic, a sum of even powers).
//!
//! Values are compared numerically (`n`), results are exact. `maximize` is `minimize` of `-f`.
//!
//! `find_instance(cond, x)`, `find_instance(cond, [x, y])`: one point satisfying `cond`, `[[x -> v]]`: for one
//! variable from `reduce` (an equation's root, else an integer or simple rational inside an interval); for several,
//! from `solve` with free unknowns set to 0; `[]` when `cond` is false.
use crate::integrate::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The functions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `minimize`
    Minimize,
    /// `maximize`
    Maximize,
    /// `min_value`
    MinValue,
    /// `max_value`
    MaxValue,
    /// `arg_min`
    ArgMin,
    /// `arg_max`
    ArgMax,
    /// `find_instance`
    FindInstance,
    /// `function_range`
    Range,
}

/// The native.
pub struct OptNative {
    /// which function
    pub kind: Kind,
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

struct C<'a, 'b, 'c, 'k, 'h> {
    cx: &'a mut NativeCx<'c, 'k, 'h>,
    b: &'b Build<'h>,
}

impl<'h> C<'_, '_, '_, '_, 'h> {
    fn ev(&mut self, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
        let t = self.b.call(name, args);
        self.cx.eval(t)
    }
    fn list(&self, xs: &[T<'h>]) -> T<'h> {
        self.b.heap().call(self.b.hd.list, xs)
    }
    fn rule(&self, a: T<'h>, v: T<'h>) -> T<'h> {
        self.b.call(names::RULE, &[a, v])
    }
    fn at(&mut self, f: T<'h>, x: T<'h>, v: T<'h>) -> Res<T<'h>> {
        let r = self.rule(x, v);
        self.ev("rewrite", &[f, r])
    }
    /// The machine value of a closed real term.
    fn num(&mut self, t: T<'h>) -> Res<Option<f64>> {
        let v = self.ev("n", &[t])?;
        Ok(match self.b.heap().view(v) {
            View::Real(x) if x.is_finite() => Some(x),
            View::Int(k) => Some(k as f64),
            _ => None,
        })
    }
    fn inf(&self) -> T<'h> {
        let h = self.b.heap();
        h.sym_term(h.sym("inf"))
    }
    fn is_call(&self, t: T<'h>, name: &str) -> bool {
        let h = self.b.heap();
        matches!(h.view(t), View::App { head, .. } if self.b.sym_of(head) == Some(h.sym(name)))
    }
}

impl Native for OptNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        if self.kind == Kind::Range {
            return range(cx, &self.stop, args);
        }
        let [fc, xs] = args else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let mut c = C { cx, b: &b };
        let vars: Vec<T<'h>> = match c.cx.as_list(*xs) {
            Some(v) => v.to_vec(),
            None => vec![*xs],
        };
        if vars.iter().any(|v| !matches!(h.view(*v), View::Sym(_))) {
            return Ok(Reply::Decline);
        }
        if self.kind == Kind::FindInstance {
            return Ok(find_instance(&mut c, *fc, &vars)?.map_or(Reply::Decline, Reply::Done));
        }
        let (f, cond) = match c.cx.as_list(*fc) {
            Some([f, k]) => (*f, Some(*k)),
            Some(_) => return Ok(Reply::Decline),
            None => (*fc, None),
        };
        let max = matches!(self.kind, Kind::Maximize | Kind::MaxValue | Kind::ArgMax);
        let g = if max { b.neg(f) } else { f };
        let r = match vars.as_slice() {
            [x] => minimize1(&mut c, g, cond, *x)?,
            _ => minimize_n(&mut c, g, cond, &vars)?,
        };
        let Some((v, point)) = r else { return Ok(Reply::Decline) };
        let v = if max { c.cx.eval(b.neg(v))? } else { v };
        let rules: Vec<T<'h>> = vars.iter().zip(&point).map(|(&x, &p)| c.rule(x, p)).collect();
        Ok(Reply::Done(match self.kind {
            Kind::Minimize | Kind::Maximize => c.list(&[v, c.list(&rules)]),
            Kind::MinValue | Kind::MaxValue => v,
            _ => {
                if point.len() == 1 {
                    point[0]
                } else {
                    c.list(&point)
                }
            }
        }))
    }
}

/// The pieces of `reduce(cond, x, reals)`: closed and open ends and single points. `None` when not reduced.
#[allow(clippy::type_complexity)]
fn pieces<'h>(c: &mut C<'_, '_, '_, '_, 'h>, cond: T<'h>, x: T<'h>) -> Res<Option<(Vec<(T<'h>, bool)>, Vec<T<'h>>)>> {
    let h = c.b.heap();
    let reals = h.sym_term(h.sym("reals"));
    let r = c.ev("reduce", &[cond, x, reals])?;
    if c.is_call(r, "reduce") || c.is_call(r, names::FAILURE) || c.cx.is_false(r) {
        return Ok(None);
    }
    // collect the atoms x op a: ends (with closedness) and points x == a
    let mut ends = vec![];
    let mut points = vec![];
    let mut stack = vec![r];
    while let Some(u) = stack.pop() {
        let View::App { head, args } = h.view(u) else { continue };
        let Some(s) = c.b.sym_of(head) else { continue };
        let name = h.sym_name(s);
        match (name.as_str(), args) {
            ("and" | "or", _) => stack.extend(args.iter().copied()),
            (op @ ("less" | "less_equal" | "greater" | "greater_equal" | "equal"), &[l, rr]) => {
                let (other, ok) = if l == x { (rr, true) } else if rr == x { (l, true) } else { (l, false) };
                if !ok {
                    return Ok(None);
                }
                if op == "equal" {
                    points.push(other);
                } else {
                    ends.push((other, op.ends_with("equal")));
                }
            }
            _ => return Ok(None),
        }
    }
    Ok(Some((ends, points)))
}

/// The minimum of `g` in one variable.
#[allow(clippy::type_complexity)]
fn minimize1<'h>(c: &mut C<'_, '_, '_, '_, 'h>, g: T<'h>, cond: Option<T<'h>>, x: T<'h>) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let b = c.b;
    let h = b.heap();
    // no other symbols
    let mut stack = vec![g];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) if u != x && !matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma") => return Ok(None),
            View::App { args, .. } => stack.extend(args.iter().copied()),
            _ => {}
        }
    }
    let inside = |c: &mut C<'_, '_, '_, '_, 'h>, v: T<'h>| -> Res<bool> {
        Ok(match cond {
            None => true,
            Some(k) => {
                let r = c.at(k, x, v)?;
                c.cx.is_true(r)
            }
        })
    };
    let mut cands: Vec<T<'h>> = vec![];
    // critical points
    let dg = c.ev("d", &[g, x])?;
    let reals = h.sym_term(h.sym("reals"));
    let eq = b.call("equal", &[dg, b.zero()]);
    let s = c.ev("solve", &[eq, x, reals])?;
    if let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) {
        for it in items {
            if let Some(&[r]) = c.cx.as_list(it) {
                if let View::App { args: &[_, v], .. } = h.view(r) {
                    if is_free(b, v, x) {
                        cands.push(v);
                    }
                }
            }
        }
    } else if !c.is_call(s, "solve") {
        // no answer from solve that can be read
        return Ok(None);
    } else {
        return Ok(None);
    }
    // ends and points of the domain
    let mut limits: Vec<(T<'h>, i8)> = vec![];
    match cond {
        None => {
            limits.push((c.inf(), 1));
            limits.push((b.neg(c.inf()), -1));
        }
        Some(k) => {
            let Some((ends, points)) = pieces(c, k, x)? else { return Ok(None) };
            cands.extend(points);
            for (e, closed) in ends {
                if closed {
                    cands.push(e);
                } else {
                    limits.push((e, 0));
                }
            }
            // unbounded sides
            for (inf, sign) in [(c.inf(), 1i8), (b.neg(c.inf()), -1)] {
                if inside(c, b.mul(&[b.int(sign as i64), b.int(1_000_000_000)]))? {
                    limits.push((inf, sign));
                }
            }
        }
    }
    // the smallest attained value
    let mut best: Option<(f64, T<'h>, T<'h>)> = None;
    for p in cands {
        c.cx.tick(1)?;
        if !inside(c, p)? {
            continue;
        }
        let v = c.at(g, x, p)?;
        let Some(nv) = c.num(v)? else { continue };
        if best.as_ref().is_none_or(|bst| nv < bst.0 - 1e-12 * (1.0 + nv.abs())) {
            best = Some((nv, c.ev("simplify", &[v])?, p));
        }
    }
    // the limits at open ends and infinities
    for (pt, sign) in limits {
        let rule = c.rule(x, pt);
        let dir = h.call(h.sym("options"), &[c.rule(h.sym_term(h.sym("direction")), h.sym_term(h.sym(if sign < 0 { "from_above" } else { "from_below" })))]);
        let l = if sign == 0 {
            // an open end: both sides (the side inside the domain is the one that matters; both agree for a
            // continuous function)
            c.ev("limit", &[g, rule])?
        } else {
            c.ev("limit", &[g, rule, dir])?
        };
        if c.is_call(l, "limit") {
            return Ok(None);
        }
        if l == b.neg(c.inf()) {
            return Ok(Some((b.neg(c.inf()), vec![pt])));
        }
        if l == c.inf() {
            continue;
        }
        let Some(nl) = c.num(l)? else { return Ok(None) };
        match &best {
            Some(bst) if nl > bst.0 + 1e-12 * (1.0 + nl.abs()) => {}
            // the infimum is a limit that is not attained
            _ => return Ok(None),
        }
    }
    Ok(best.map(|(_, v, p)| (v, vec![p])))
}

/// The minimum of a polynomial in several variables with a positive definite leading form (decided by the
/// critical point being unique and the polynomial a sum with an even-degree part of positive coefficients only).
#[allow(clippy::type_complexity)]
fn minimize_n<'h>(c: &mut C<'_, '_, '_, '_, 'h>, g: T<'h>, cond: Option<T<'h>>, vars: &[T<'h>]) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let b = c.b;
    let h = b.heap();
    if cond.is_some() {
        return Ok(None);
    }
    // a convex quadratic: the Hessian constant and positive definite (leading minors)
    let hess = c.ev("d", &[g, c.list(vars), c.list(vars)])?;
    let Some(rows) = c.cx.as_list(hess).map(|v| v.to_vec()) else { return Ok(None) };
    let n = vars.len();
    for k in 1..=n {
        let minor: Vec<T<'h>> = rows[..k]
            .iter()
            .map(|&r| {
                let items = c.cx.as_list(r).unwrap_or(&[]).to_vec();
                c.list(&items[..k.min(items.len())])
            })
            .collect();
        let det = c.ev("det", &[c.list(&minor)])?;
        let Some(d) = c.num(det)? else { return Ok(None) };
        if vars.iter().any(|&v| !is_free(b, det, v)) || d <= 0.0 {
            return Ok(None);
        }
    }
    let grad = c.ev("d", &[g, c.list(vars)])?;
    let Some(gs) = c.cx.as_list(grad).map(|v| v.to_vec()) else { return Ok(None) };
    let eqs: Vec<T<'h>> = gs.iter().map(|&e| b.call("equal", &[e, b.zero()])).collect();
    let s = c.ev("solve", &[c.list(&eqs), c.list(vars)])?;
    let Some(&[sol]) = c.cx.as_list(s) else { return Ok(None) };
    let Some(rules) = c.cx.as_list(sol).map(|v| v.to_vec()) else { return Ok(None) };
    let v = c.ev("rewrite", &[g, c.list(&rules)])?;
    let v = c.ev("simplify", &[v])?;
    let mut point = vec![];
    for &x in vars {
        let p = c.ev("rewrite", &[x, c.list(&rules)])?;
        if vars.iter().any(|&y| !is_free(b, p, y)) {
            return Ok(None);
        }
        point.push(p);
    }
    let _ = h;
    Ok(Some((v, point)))
}

/// One point satisfying `cond`.
fn find_instance<'h>(c: &mut C<'_, '_, '_, '_, 'h>, cond: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    if let [x] = vars {
        let Some((ends, points)) = pieces(c, cond, *x)? else {
            let r = c.ev("reduce", &[cond, *x, h.sym_term(h.sym("reals"))])?;
            if c.cx.is_false(r) {
                return Ok(Some(c.list(&[])));
            }
            return Ok(None);
        };
        let check = |c: &mut C<'_, '_, '_, '_, 'h>, v: T<'h>| -> Res<bool> {
            let r = c.at(cond, *x, v)?;
            Ok(c.cx.is_true(r))
        };
        let mut cands: Vec<T<'h>> = points.clone();
        for &(e, _) in &ends {
            for d in [0, 1, -1] {
                let f = c.ev("floor", &[e])?;
                cands.push(c.cx.eval(b.add(&[f, b.int(d)]))?);
            }
            cands.push(c.cx.eval(b.add(&[e, b.rat(1, 2)]))?);
            cands.push(c.cx.eval(b.add(&[e, b.rat(-1, 2)]))?);
        }
        if ends.len() >= 2 {
            let mid = b.mul(&[b.rat(1, 2), b.add(&[ends[0].0, ends[1].0])]);
            cands.push(c.cx.eval(mid)?);
        }
        cands.push(b.zero());
        for v in cands {
            if check(c, v)? {
                return Ok(Some(c.list(&[c.list(&[c.rule(*x, v)])])));
            }
        }
        return Ok(None);
    }
    let s = c.ev("solve", &[cond, c.list(vars)])?;
    let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
    let Some(&first) = items.first() else { return Ok(Some(c.list(&[]))) };
    let Some(rules) = c.cx.as_list(first).map(|v| v.to_vec()) else { return Ok(None) };
    // the unknowns not solved for are 0
    let mut zero = vec![];
    for &x in vars {
        if !rules.iter().any(|&r| matches!(h.view(r), View::App { args: &[a, _], .. } if a == x)) {
            zero.push(c.rule(x, b.zero()));
        }
    }
    let mut out = vec![];
    for &x in vars {
        let v = c.ev("rewrite", &[x, c.list(&rules)])?;
        let v = c.ev("rewrite", &[v, c.list(&zero)])?;
        out.push(c.rule(x, v));
    }
    Ok(Some(c.list(&[c.list(&out)])))
}

/// `function_range(f, x, y)`, `function_range([f, cond], x, y)`: the values of `f` as a condition on `y`, from the
/// global minimum and maximum (one variable, continuous on a connected domain assumed): `a <= y <= b`, `y >= a`,
/// `y <= b` or `true`.
fn range<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [fc, x, y] = args else { return Ok(Reply::Decline) };
    if !matches!(h.view(*x), View::Sym(_)) || !matches!(h.view(*y), View::Sym(_)) {
        return Ok(Reply::Decline);
    }
    let _scope = StopScope::enter(Some(stop));
    let b = Build::new(h, stop);
    let mut c = C { cx, b: &b };
    let (f, cond) = match c.cx.as_list(*fc) {
        Some([f, k]) => (*f, Some(*k)),
        Some(_) => return Ok(Reply::Decline),
        None => (*fc, None),
    };
    let Some((lo, _)) = minimize1(&mut c, f, cond, *x)? else { return Ok(Reply::Decline) };
    let Some((hi, _)) = minimize1(&mut c, b.neg(f), cond, *x)? else { return Ok(Reply::Decline) };
    let hi = c.cx.eval(b.neg(hi))?;
    let inf = c.inf();
    let mut parts = vec![];
    if lo != b.neg(inf) {
        parts.push(b.call("greater_equal", &[*y, lo]));
    }
    if hi != inf {
        parts.push(b.call("less_equal", &[*y, hi]));
    }
    let r = match parts.as_slice() {
        [] => c.cx.bool(true),
        [p] => *p,
        [_, u] => b.call("and", &[b.call("less_equal", &[lo, *y]), *u]),
        _ => return Ok(Reply::Decline),
    };
    Ok(Reply::Done(c.cx.eval(r)?))
}
