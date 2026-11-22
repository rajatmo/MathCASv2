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
//! - **Several variables**: the critical points of every face of the constraints (the Lagrange systems of every set
//!   of active constraints, `minimize_n`). Every face must be accounted for. A system `solve` does not solve is
//!   tried again with a factor of a product at a time and with an unknown that one equation determines uniquely
//!   substituted (`solve_eliminating`), then without multipliers (the minors of the gradients). A family with an
//!   integer parameter `c(1)` is enumerated between the bounds of a coordinate (`family`). A solution with unknowns
//!   left free (a curve of critical points, where `solve` may also leave out isolated ones) gives a candidate, and
//!   the face its critical values (`eliminate`, then `find_instance` for each value below the best,
//!   `critical_values`, for polynomial systems with algebraic coefficients); a small exact algebraic problem goes
//!   to the cylindrical decomposition after this native instead. Otherwise there is no answer, and a problem that
//!   is not algebraic fails with the reason instead of returning the smallest value found.
//! - **Angles** (`circle_problem`): a variable `t` that enters only through `cos(t)` and `sin(t)`, with constant
//!   ends, becomes the point `(u, v) = (cos t, sin t)` of the unit circle, cut to the arc its range sweeps by the
//!   chord through the arc's ends; the problem becomes algebraic in `u, v` (polar coordinates, trigonometric
//!   polynomials on boxes) and its proved minimum maps back to `t = arg(u + i v) + 2 pi k`.
//!
//! Values are compared numerically (`n`), results are exact. `maximize` is `minimize` of `-f`.
//!
//! `find_instance(cond, x)`, `find_instance(cond, [x, y])`: one point satisfying `cond`, `[[x -> v]]`: for one
//! variable from `reduce` (an equation's root, else an integer or simple rational inside an interval); for several,
//! from `solve` with free unknowns set to 0; `[]` when `cond` is false.
use crate::common::is_free;
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
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Minimize => "minimize",
            Kind::Maximize => "maximize",
            Kind::MinValue => "min_value",
            Kind::MaxValue => "max_value",
            Kind::ArgMin => "arg_min",
            Kind::ArgMax => "arg_max",
            Kind::FindInstance => "find_instance",
        }
    }
}

/// Is every function of the variables in `t` a sum, product or power with a constant exponent, or a case function
/// (`abs`, `max`, `piecewise`, ...) or norm of such (a problem the cylindrical decomposition or its recasts can take)?
fn algebraic<'h>(b: &Build<'h>, t: T<'h>, vars: &[T<'h>]) -> bool {
    let h = b.heap();
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        let View::App { head, args } = h.view(u) else { continue };
        // a composite head (`inactive(norm)`): its arguments decide
        let name = match h.view(head) {
            View::Sym(s) => h.sym_name(s),
            _ => {
                stack.extend(args.iter().copied());
                continue;
            }
        };
        let holds_var = vars.iter().any(|&x| !is_free(b, u, x));
        if holds_var {
            match name.as_str() {
                names::ADD | names::MUL | "list" | "and" | "or" | "not" | "less" | "less_equal" | "greater" | "greater_equal" | "equal"
                | "unequal" | "element" | "abs" | "real_abs" | "norm" | "max" | "min" | "sign" | "real_sign" | "boole" | "piecewise"
                | "unit_step" | "clip" | "floor" | "ceiling" | "round" | "sqrt" | "inactive" | "dot" => {}
                names::POW if args.len() == 2 && vars.iter().all(|&x| is_free(b, args[1], x)) => {}
                _ => return false,
            }
        }
        stack.extend(args.iter().copied());
    }
    true
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
    /// Is `t` a number (`is_numeric`)?
    fn is_number(&mut self, t: T<'h>) -> Res<bool> {
        if matches!(self.b.heap().view(t), View::Int(_) | View::Rational { .. } | View::Real(_)) {
            return Ok(true);
        }
        let r = self.ev("is_numeric", &[t])?;
        Ok(self.cx.is_true(r))
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
        // `find_instance(cond, vars, n)`: up to n points
        let (args, count) = match args {
            [fc, xs, k] if self.kind == Kind::FindInstance => match h.view(*k) {
                View::Int(k) if (1..=1000).contains(&k) => ([*fc, *xs], k as usize),
                _ => return Ok(Reply::Decline),
            },
            _ => (match args {
                [fc, xs] => [*fc, *xs],
                _ => return Ok(Reply::Decline),
            }, 1),
        };
        let [fc, xs] = &args;
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let mut c = C { cx, b: &b };
        // `minimize(f, element([x, y], region))`: the variables and a condition from the region
        let mut region = None;
        let mut xs = *xs;
        if let View::App { head, args: &[pts, _] } = h.view(xs) {
            if b.sym_of(head) == Some(h.sym("element")) && c.cx.as_list(pts).is_some() {
                region = Some(xs);
                xs = pts;
            }
        }
        let vars: Vec<T<'h>> = match c.cx.as_list(xs) {
            Some(v) => v.to_vec(),
            None => vec![xs],
        };
        if vars.iter().any(|v| !matches!(h.view(*v), View::Sym(_))) {
            return Ok(Reply::Decline);
        }
        if self.kind == Kind::FindInstance {
            return Ok(find_instance(&mut c, *fc, &vars, count)?.map_or(Reply::Decline, Reply::Done));
        }
        let (f, cond) = match c.cx.as_list(*fc) {
            Some([f, k]) => (*f, Some(*k)),
            Some(_) => return Ok(Reply::Decline),
            None => (*fc, None),
        };
        let cond = match (cond, region) {
            (Some(k), Some(r)) => Some(b.call("and", &[k, r])),
            (None, r) => r,
            (k, None) => k,
        };
        let max = matches!(self.kind, Kind::Maximize | Kind::MaxValue | Kind::ArgMax);
        let g = if max { b.neg(f) } else { f };
        let mut unproved = None;
        let r = match vars.as_slice() {
            [x] => match minimize1(&mut c, g, cond, *x, None)? {
                // a periodic problem (objective and constraints): one period holds every value
                None => match period(&mut c, &[Some(g), cond], *x)? {
                    Some(p) => {
                        let lo = b.call("less_equal", &[b.zero(), *x]);
                        let hi = b.call("less_equal", &[*x, p]);
                        let k = match cond {
                            Some(k) => b.call("and", &[k, lo, hi]),
                            None => b.call("and", &[lo, hi]),
                        };
                        minimize1(&mut c, g, Some(k), *x, Some(b.call("and", &[lo, hi])))?
                    }
                    None => None,
                }
                // a trigonometric objective: the point (cos x, sin x) of the circle instead of x
                .map_or_else(|| circle_problem(&mut c, g, cond, &vars), |r| Ok(Some(r)))?,
                r => r,
            },
            _ => match circle_problem(&mut c, g, cond, &vars)? {
                Some(r) => Some(r),
                None => minimize_n(&mut c, g, cond, &vars, &mut unproved)?,
            },
        };
        let Some((v, point)) = r else {
            // candidates were found but the minimum is not proved: a problem that is not algebraic (which the
            // cylindrical decomposition after this native cannot take) fails with the reason
            if let Some(why) = unproved {
                if !algebraic(&b, *fc, &vars) {
                    let name = h.sym_term(h.sym(self.kind.name()));
                    let tag = h.sym_term(h.sym("unsolved"));
                    return Ok(Reply::Done(b.call(names::FAILURE, &[tag, name, h.string(why)])));
                }
            }
            return Ok(Reply::Decline);
        };
        let v = if max { c.cx.eval(b.neg(v))? } else { v };
        if point.len() != vars.len() {
            // unbounded: only the value is known
            return Ok(match self.kind {
                Kind::MinValue | Kind::MaxValue => Reply::Done(v),
                _ => Reply::Decline,
            });
        }
        let rules: Vec<T<'h>> = vars.iter().zip(&point).map(|(&x, &p)| c.rule(x, p)).collect();
        // machine numbers in the problem (`x >= 2.`, which reduce reads as 2): a machine answer
        let machine = has_machine(h, *fc);
        let done = |t: T<'h>| if machine { Reply::Again(h.call(h.sym("n"), &[t])) } else { Reply::Done(t) };
        Ok(done(match self.kind {
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

/// The pieces of `reduce(cond, x, reals)`: closed and open ends (with the side of the domain: true above the end)
/// and single points. `None` when not reduced.
#[allow(clippy::type_complexity)]
fn pieces<'h>(c: &mut C<'_, '_, '_, '_, 'h>, cond: T<'h>, x: T<'h>) -> Res<Option<(Vec<(T<'h>, bool, bool)>, Vec<T<'h>>)>> {
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
                    // the domain lies above the end for x > a (a < x)
                    let above = (l == x) == op.starts_with("greater");
                    ends.push((other, op.ends_with("equal"), above));
                }
            }
            _ => return Ok(None),
        }
    }
    Ok(Some((ends, points)))
}

/// The minimum of `g` in one variable.
#[allow(clippy::type_complexity)]
/// `crit_box`: bounds given to solve with the critical point equation (one period of a periodic g, whose critical
/// points are then a finite exact set; machine roots there decline).
fn minimize1<'h>(
    c: &mut C<'_, '_, '_, '_, 'h>,
    g: T<'h>,
    cond: Option<T<'h>>,
    x: T<'h>,
    crit_box: Option<T<'h>>,
) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let b = c.b;
    let h = b.heap();
    // comparisons of floor and ceiling with integers written without them (ceiling(u) < 5 is u <= 4)
    let cond = match cond {
        Some(k) => Some(steps::plain_condition(c, k)?),
        None => None,
    };
    // a step function of x on a bounded domain: piece by piece
    if !steps::atoms(c, g, x).is_empty() {
        return steps::minimize_steps(c, g, cond, x);
    }
    // no other symbols
    let mut stack = mc_api::walk::Walk::new(g);
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
    // set when the critical points were sampled from a family of solutions
    let mut sampled = false;
    // critical points
    let dg = c.ev("d", &[g, x])?;
    let reals = h.sym_term(h.sym("reals"));
    let eq = b.call("equal", &[dg, b.zero()]);
    // a derivative that is a nonzero constant (a linear g): no critical points (solve of 1 == 0 is a failure)
    let constant = is_free(b, dg, x) && c.num(dg)?.is_some_and(|v| v != 0.0);
    let eq = match crit_box {
        Some(bx) => b.call("and", &[eq, bx]),
        None => eq,
    };
    let s = if constant { c.list(&[]) } else { c.ev("solve", &[eq, x, reals])? };
    if crit_box.is_some() && has_machine(h, s) && !has_machine(h, g) {
        return Ok(None);
    }
    if let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) {
        // a family of critical points (periodic functions: `x -> pi*c(1)` under `element(c(1), integers)`) is not a
        // finite set of candidates: decline rather than read one member as the only point
        let generated = b.heap().sym("c");
        for it in items {
            if let Some(&[r]) = c.cx.as_list(it) {
                if let View::App { args: &[_, v], .. } = h.view(r) {
                    if let Some(k) = find_call_of(h, v, generated) {
                        // members of a one-parameter family c(1) = -4..4; the range found is confirmed by a numeric
                        // scan below, so a family whose extremum lies elsewhere declines instead of answering
                        if !matches!(h.view(k), View::App { args: [a], .. } if *a == h.int(1)) {
                            return Ok(None);
                        }
                        for j in -4..=4i64 {
                            let m = c.at(v, k, b.int(j))?;
                            if is_free(b, m, x) && find_call_of(h, m, generated).is_none() {
                                cands.push(m);
                            }
                        }
                        sampled = true;
                        continue;
                    }
                    if is_free(b, v, x) {
                        cands.push(v);
                    }
                }
            } else {
                // a conditional solution, or not one rule: not a plain candidate
                return Ok(None);
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
            for (e, closed, above) in ends {
                if closed {
                    cands.push(e);
                } else {
                    // an open end: 2 approached from above (the domain lies above it), -2 from below
                    limits.push((e, if above { 2 } else { -2 }));
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
    // the real poles of g inside the domain (zeros of the denominator of together(g)): g is not defined there, and its
    // one-sided limits bound the values without being attained (t/(1 + t) is unbounded at -1; its minimum is not 1)
    let tg = c.ev("together", &[g])?;
    let den = c.ev("denominator", &[tg])?;
    let mut pole_limit: Option<(f64, T<'h>)> = None;
    if !is_free(b, den, x) {
        let s = c.ev("solve", &[b.call("equal", &[den, b.zero()]), x, reals])?;
        let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
        let generated = h.sym("c");
        let mut poles = vec![];
        for it in items {
            let Some(&[r]) = c.cx.as_list(it) else { return Ok(None) };
            let View::App { args: &[_, p], .. } = h.view(r) else { return Ok(None) };
            // a family of poles (tan x): its members c(1) = -4..4, as for the critical points
            let p = match find_call_of(h, p, generated) {
                Some(k) => {
                    for j in -4..=4i64 {
                        poles.push(c.at(p, k, b.int(j))?);
                    }
                    continue;
                }
                None => p,
            };
            poles.push(p);
        }
        for p in poles {
            if !is_free(b, p, x) || c.num(p)?.is_none() {
                return Ok(None);
            }
            if !inside(c, p)? {
                continue;
            }
            for side in ["from_above", "from_below"] {
                c.cx.tick(1)?;
                let rule = c.rule(x, p);
                let dir = h.call(h.sym("options"), &[c.rule(h.sym_term(h.sym("direction")), h.sym_term(h.sym(side)))]);
                let l = c.ev("limit", &[g, rule, dir])?;
                if l == b.neg(c.inf()) {
                    return Ok(Some((b.neg(c.inf()), vec![p])));
                }
                if l == c.inf() {
                    continue;
                }
                let Some(nl) = c.num(l)? else { return Ok(None) };
                // a finite limit at a point left out: when it is the infimum, the infimum is not attained
                if pole_limit.as_ref().is_none_or(|pl: &(f64, T<'h>)| nl < pl.0) {
                    pole_limit = Some((nl, l));
                }
            }
        }
    }
    // the limits at open ends and infinities
    for (pt, sign) in limits {
        let open = sign.abs() == 2;
        let rule = c.rule(x, pt);
        let from_above = sign == -1 || sign == 2;
        let dir = h.call(h.sym("options"), &[c.rule(h.sym_term(h.sym("direction")), h.sym_term(h.sym(if from_above { "from_above" } else { "from_below" })))]);
        let l = if open {
            // an open end: both sides (both agree for a continuous function); when there is no two-sided limit
            // (x^(1/x) at 0 is not real below 0), the side inside the domain, which is the one that matters
            let l = c.ev("limit", &[g, rule])?;
            if c.is_call(l, "limit") || c.is_call(l, "failure") {
                c.ev("limit", &[g, rule, dir])?
            } else {
                l
            }
        } else {
            c.ev("limit", &[g, rule, dir])?
        };
        let sign = if open { 0 } else { sign };
        if c.is_call(l, "limit") || c.is_call(l, "failure") {
            // no limit at an infinity: an oscillating g; with sampled family members the grid scan below decides
            if sampled && sign != 0 {
                continue;
            }
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
            // the infimum is a finite limit at an infinity: the value with the point at that infinity
            _ if sign != 0 => {
                if best.as_ref().is_none_or(|bst| nl < bst.0) {
                    best = Some((nl, l, pt));
                }
            }
            // the infimum is a limit at an open end that is not attained
            _ => return Ok(None),
        }
    }
    // the infimum is a finite limit at a pole, not attained: the value alone (min_value), no point (minimize declines)
    if let Some((nl, l)) = pole_limit {
        if best.as_ref().is_none_or(|bst| nl < bst.0 + 1e-12 * (1.0 + nl.abs())) {
            return Ok(Some((c.ev("simplify", &[l])?, vec![])));
        }
    }
    if sampled {
        // sampled family members: no value of g on a wide grid may lie below the infimum found
        if let Some((m, _, _)) = &best {
            for i in 0..=800 {
                c.cx.tick(1)?;
                let t = b.rat(i as i64 - 400, 10);
                if !inside(c, t)? {
                    continue;
                }
                let v = c.at(g, x, t)?;
                if let Some(nv) = c.num(v)? {
                    if nv < m - 1e-9 * (1.0 + m.abs()) {
                        return Ok(None);
                    }
                }
            }
        }
    }
    Ok(best.map(|(_, v, p)| (v, vec![p])))
}

/// A period of the terms `ts` in `x` when `x` occurs only in the linear arguments `a x + b` of sin, cos, tan, cot,
/// sec and csc with real slopes a in rational ratio: 2 pi L / |a1| for slopes a_i = (n_i/d_i) a1, L = lcm(n_i) /
/// gcd(d_i) (each function has the period 2 pi / |a_i|, of which this is a multiple). `None` otherwise.
fn period<'h>(c: &mut C<'_, '_, '_, '_, 'h>, ts: &[Option<T<'h>>], x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    let trig = ["sin", "cos", "tan", "cot", "sec", "csc"];
    let mut slopes: Vec<T<'h>> = vec![];
    let mut stack: Vec<T<'h>> = ts.iter().flatten().copied().collect();
    while let Some(t) = stack.pop() {
        c.cx.tick(1)?;
        if t == x {
            // x outside a trigonometric argument
            return Ok(None);
        }
        let View::App { head, args } = h.view(t) else { continue };
        let name = b.sym_of(head).map(|s| h.sym_name(s));
        if let (Some(n), [u]) = (name.as_deref(), args) {
            if trig.contains(&n) && !is_free(b, *u, x) {
                let a = c.ev("d", &[*u, x])?;
                let rest = c.ev("expand", &[b.sub(*u, b.mul(&[a, x]))])?;
                if !is_free(b, a, x) || !is_free(b, rest, x) || c.num(a)?.is_none_or(|v| v == 0.0) {
                    return Ok(None);
                }
                slopes.push(a);
                continue;
            }
        }
        stack.extend(args.iter().copied());
    }
    let Some(&a1) = slopes.first() else { return Ok(None) };
    let (mut l, mut g) = (1i64, 0i64);
    for &a in &slopes {
        let q = c.ev("simplify", &[b.div(a, a1)])?;
        let n = c.ev("numerator", &[q])?;
        let d = c.ev("denominator", &[q])?;
        let (View::Int(n), View::Int(d)) = (h.view(n), h.view(d)) else { return Ok(None) };
        let n = n.unsigned_abs() as i64;
        if n == 0 || n > 1 << 20 || d > 1 << 20 {
            return Ok(None);
        }
        l = l / gcd(l, n) * n;
        g = gcd(g, d);
        if l > 1 << 30 {
            return Ok(None);
        }
    }
    let pi = h.sym_term(h.sym("pi"));
    let p = b.mul(&[b.rat(2 * l, g), pi, b.powi(c.ev("abs", &[a1])?, -1)]);
    Ok(Some(c.ev("simplify", &[p])?))
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

/// Does `t` contain a machine real?
fn has_machine(h: &mc_core::Heap, t: T<'_>) -> bool {
    let mut stack = mc_api::walk::Walk::new(t);
    let mut budget = 100_000usize;
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Real(_) => return true,
            View::App { head, args } if budget > args.len() => {
                budget -= args.len() + 1;
                stack.push(head);
                stack.extend(args.iter().copied());
            }
            _ => {}
        }
    }
    false
}

/// The first call in `t` whose head is the symbol `s` (a generated parameter `c(k)`).
fn find_call_of<'h>(h: &'h mc_core::Heap, t: T<'h>, s: mc_core::Sym) -> Option<T<'h>> {
    match h.view(t) {
        View::App { head, args } => {
            if h.sym_term(s) == head {
                return Some(t);
            }
            find_call_of(h, head, s).or_else(|| args.iter().find_map(|&a| find_call_of(h, a, s)))
        }
        _ => None,
    }
}

/// A constraint `g <= 0`, `g < 0` or `g == 0`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rel {
    Le,
    Lt,
    Eq,
}

/// A conjunction of constraints.
type Conj<'h> = Vec<(T<'h>, Rel)>;

/// The constraints of `cond` as a disjunction of conjunctions; `None` when a part is not read.
fn constraints<'h>(c: &mut C<'_, '_, '_, '_, 'h>, cond: T<'h>) -> Res<Option<Vec<Conj<'h>>>> {
    let b = c.b;
    let h = b.heap();
    if c.cx.is_true(cond) {
        return Ok(Some(vec![vec![]]));
    }
    let View::App { head, args } = h.view(cond) else { return Ok(None) };
    let Some(s) = b.sym_of(head) else { return Ok(None) };
    let name = h.sym_name(s);
    let out = match (name.as_str(), args) {
        ("and", _) => {
            let mut acc: Vec<Conj<'h>> = vec![vec![]];
            for &a in args {
                let Some(alts) = constraints(c, a)? else { return Ok(None) };
                let mut next = vec![];
                for x in &acc {
                    for y in &alts {
                        next.push(x.iter().chain(y).copied().collect());
                    }
                }
                acc = next;
                if acc.len() > 16 {
                    return Ok(None);
                }
            }
            acc
        }
        ("or", _) => {
            let mut acc = vec![];
            for &a in args {
                let Some(alts) = constraints(c, a)? else { return Ok(None) };
                acc.extend(alts);
            }
            acc
        }
        (op @ ("less" | "less_equal" | "greater" | "greater_equal" | "equal"), _) if args.len() >= 2 => {
            // chains a op b op c
            let mut conj = vec![];
            for w in args.windows(2) {
                let (g, rel) = match op {
                    "less" => (b.sub(w[0], w[1]), Rel::Lt),
                    "less_equal" => (b.sub(w[0], w[1]), Rel::Le),
                    "greater" => (b.sub(w[1], w[0]), Rel::Lt),
                    "greater_equal" => (b.sub(w[1], w[0]), Rel::Le),
                    _ => (b.sub(w[0], w[1]), Rel::Eq),
                };
                conj.push((c.cx.eval(g)?, rel));
            }
            vec![conj]
        }
        ("element", &[pt, region]) => {
            let Some(coords) = c.cx.as_list(pt).map(|v| v.to_vec()) else { return Ok(None) };
            let Some(g) = region_constraint(c, &coords, region)? else { return Ok(None) };
            vec![vec![(g, Rel::Le)]]
        }
        _ => return Ok(None),
    };
    Ok(Some(out))
}

/// `|p - center|^2 - r^2` for `disk(center, r)` and `ball(center, r)` (unit and origin by default).
fn region_constraint<'h>(c: &mut C<'_, '_, '_, '_, 'h>, coords: &[T<'h>], region: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    let View::App { head, args } = h.view(region) else { return Ok(None) };
    let Some(s) = b.sym_of(head) else { return Ok(None) };
    if !matches!(h.sym_name(s).as_str(), "disk" | "ball") {
        return Ok(None);
    }
    let center: Vec<T<'h>> = match args.first() {
        Some(&ct) => match c.cx.as_list(ct) {
            Some(v) if v.len() == coords.len() => v.to_vec(),
            _ => return Ok(None),
        },
        None => vec![b.zero(); coords.len()],
    };
    let r = args.get(1).copied().unwrap_or_else(|| b.one());
    let mut terms: Vec<T<'h>> = coords.iter().zip(&center).map(|(&x, &a)| b.powi(b.sub(x, a), 2)).collect();
    terms.push(b.neg(b.powi(r, 2)));
    Ok(Some(c.cx.eval(b.add(&terms))?))
}

/// The real machine value of a closed term (a complex value with a negligible imaginary part counts as real).
fn real_value<'h>(c: &mut C<'_, '_, '_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let v = c.ev("n", &[t])?;
    let h = c.b.heap();
    Ok(match h.view(v) {
        View::Real(x) if x.is_finite() => Some(x),
        View::Int(k) => Some(k as f64),
        _ => {
            let re = c.ev("re", &[v])?;
            let im = c.ev("im_part", &[v])?;
            match (h.view(re), h.view(im)) {
                (View::Real(a), View::Real(bb)) if a.is_finite() && bb.abs() <= 1e-9 * (1.0 + a.abs()) => Some(a),
                (View::Real(a), View::Int(0)) if a.is_finite() => Some(a),
                _ => None,
            }
        }
    })
}

/// Does the point satisfy the constraints (with a small tolerance on the non-strict ones)?
fn feasible<'h>(c: &mut C<'_, '_, '_, '_, 'h>, cons: &[(T<'h>, Rel)], rules: T<'h>) -> Res<bool> {
    for &(g, rel) in cons {
        let v = c.ev("rewrite", &[g, rules])?;
        let Some(x) = real_value(c, v)? else { return Ok(false) };
        let tol = 1e-9 * (1.0 + x.abs());
        let ok = match rel {
            Rel::Le => x <= tol,
            Rel::Lt => x < -tol,
            Rel::Eq => x.abs() <= tol,
        };
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A minimum found: (machine value, exact value, point, machine point).
type Best<'h> = Option<(f64, T<'h>, Vec<T<'h>>, Vec<f64>)>;

/// The minimum of `g` in several variables subject to `cond`: the candidates are the solutions of the Lagrange
/// (KKT) systems of every set of active constraints (equations always active; with as many active constraints as
/// variables the constraints alone); a system not solved, or solved with unknowns left free, gives its critical
/// values (`critical_values`). The smallest value at a feasible candidate is the minimum when no far feasible point
/// (sampled along fixed directions) is smaller; `(-inf, [])` when the values keep falling far out. A face that is not
/// accounted for gives `None` with the reason in `unproved`.
#[allow(clippy::type_complexity)]
fn minimize_n<'h>(
    c: &mut C<'_, '_, '_, '_, 'h>,
    g: T<'h>,
    cond: Option<T<'h>>,
    vars: &[T<'h>],
    unproved: &mut Option<&'static str>,
) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let b = c.b;
    let h = b.heap();
    let n = vars.len();
    // a kink (`abs`, `max`, `min`, `sign`, ...) can hold the minimum where the gradient does not vanish: the
    // critical points alone would give a wrong value, so the case is left to the decomposition
    for name in ["abs", "max", "min", "sign", "floor", "ceiling", "round", "clip", "piecewise", "unit_step"] {
        if find_call_of(h, g, h.sym(name)).is_some() {
            return Ok(None);
        }
    }
    // so can a root u^(p/q), 0 < p/q < 1, where u vanishes: its gradient is not defined there (the norm
    // (x^2 + y^2)^(1/2) has its minimum at the origin, where no gradient vanishes), unless u == 0 is proved
    // infeasible (`find_instance` gives [])
    for u in radical_kink(b, g, vars) {
        let at = b.call("equal", &[u, b.zero()]);
        let at = match cond {
            Some(k) => b.call("and", &[at, k]),
            None => at,
        };
        let fi = c.ev("find_instance", &[at, c.list(vars)])?;
        if !c.cx.as_list(fi).is_some_and(|v| v.is_empty()) {
            return Ok(None);
        }
    }
    let branches = match cond {
        None => vec![vec![]],
        Some(k) => match constraints(c, k)? {
            Some(bs) => bs,
            None => return Ok(None),
        },
    };
    let grad_g = c.ev("d", &[g, c.list(vars)])?;
    let Some(grad_g) = c.cx.as_list(grad_g).map(|v| v.to_vec()).filter(|v| v.len() == n) else { return Ok(None) };
    let mut best: Best<'h> = None;
    // a face left open: no answer, after the other faces (`Some(reason)` for a failure, `None` for the
    // cylindrical decomposition); the remaining faces and the far points are still gone through, which keeps the
    // work of a parametric problem (`(x - a)^2 + (y - b)^2` before a plot samples it) as it was
    let mut open: Option<Option<&'static str>> = None;
    for cons in &branches {
        let eqs: Vec<usize> = (0..cons.len()).filter(|&i| cons[i].1 == Rel::Eq).collect();
        let ineqs: Vec<usize> = (0..cons.len()).filter(|&i| cons[i].1 != Rel::Eq).collect();
        if ineqs.len() > 10 {
            return Ok(None);
        }
        for mask in 0u32..(1 << ineqs.len()) {
            c.cx.tick(1)?;
            let chosen = ineqs.iter().enumerate().filter(|(j, _)| mask >> j & 1 == 1).map(|(_, &i)| i);
            let active: Vec<usize> = eqs.iter().copied().chain(chosen).collect();
            // a strict inequality is never active at an attained minimum
            if active.iter().any(|&i| cons[i].1 == Rel::Lt) || active.len() > n {
                continue;
            }
            let mut system: Vec<T<'h>> = vec![];
            let mut unknowns: Vec<T<'h>> = vars.to_vec();
            if active.len() < n {
                let lams: Vec<T<'h>> = (0..active.len()).map(|j| h.sym_term(h.sym(&format!("optimize`l{j}")))).collect();
                let mut grads = vec![];
                for &i in &active {
                    let gr = c.ev("d", &[cons[i].0, c.list(vars)])?;
                    let Some(gr) = c.cx.as_list(gr).map(|v| v.to_vec()) else { return Ok(None) };
                    grads.push(gr);
                }
                for k in 0..n {
                    let mut terms = vec![grad_g[k]];
                    for (j, gr) in grads.iter().enumerate() {
                        terms.push(b.neg(b.mul(&[lams[j], gr[k]])));
                    }
                    let e = c.cx.eval(b.add(&terms))?;
                    system.push(b.call("equal", &[e, b.zero()]));
                }
                unknowns.extend(lams);
            }
            for &i in &active {
                system.push(b.call("equal", &[cons[i].0, b.zero()]));
            }
            // a face whose critical points are not all known can hold the minimum: no answer (a smaller value
            // found elsewhere would not be proved the minimum)
            let mut judged = judge(c, g, vars, cons, &system, &unknowns, &mut best)?;
            // without multipliers: the gradients of g and of the active constraints dependent, every minor of
            // order k + 1 zero (a superset of the critical points: the singular points of the face too)
            if !judged && !active.is_empty() && active.len() < n {
                let mut rows = vec![grad_g.clone()];
                for &i in &active {
                    let gr = c.ev("d", &[cons[i].0, c.list(vars)])?;
                    let Some(gr) = c.cx.as_list(gr).map(|v| v.to_vec()) else { return Ok(None) };
                    rows.push(gr);
                }
                let mut minors = vec![];
                for cols in subsets(n, active.len() + 1) {
                    let m: Vec<T<'h>> = rows.iter().map(|r| c.list(&cols.iter().map(|&j| r[j]).collect::<Vec<_>>())).collect();
                    let d = c.ev("det", &[c.list(&m)])?;
                    let d = c.ev("together", &[d])?;
                    let d = c.ev("numerator", &[d])?;
                    minors.push(b.call("equal", &[d, b.zero()]));
                }
                for &i in &active {
                    minors.push(b.call("equal", &[cons[i].0, b.zero()]));
                }
                judged = judge(c, g, vars, cons, &minors, vars, &mut best)?;
            }
            // a small exact algebraic problem without a candidate yet: the cylindrical decomposition after this
            // native proves it (and gives its own sample point)
            let exact = |e: T<'h>| algebraic(b, e, vars) && !has_machine(h, e);
            let region = || cond.is_some_and(|k| find_call_of(h, k, h.sym("element")).is_some());
            if !judged && best.is_none() && n <= 3 && !region() && exact(g) && cons.iter().all(|&(e, _)| exact(e)) {
                open.get_or_insert(None);
                continue;
            }
            // the critical values instead of the points: finitely many, each attained at a feasible critical
            // point or proved not to be
            if !judged && !critical_values(c, g, vars, cons, &system, &unknowns, &mut best)? {
                open.get_or_insert(Some("the critical points on a face of the constraints are not found"));
            }
        }
    }
    // far feasible points: below the best candidate means unbounded (or a minimum not attained)
    let mut far: Option<(f64, f64)> = None;
    let dirs = directions(n);
    for cons in &branches {
        for d in &dirs {
            let mut vals = [f64::NAN; 2];
            for (k, r) in [1.0e3f64, 1.0e6].iter().enumerate() {
                c.cx.tick(1)?;
                let pr: Vec<T<'h>> = vars.iter().zip(d).map(|(&x, &di)| c.rule(x, h.real(di * r))).collect();
                let pr = c.list(&pr);
                if !feasible(c, cons, pr)? {
                    continue;
                }
                let v = c.ev("rewrite", &[g, pr])?;
                if let Some(x) = real_value(c, v)? {
                    vals[k] = x;
                }
            }
            if vals[0].is_finite() && vals[1].is_finite() && far.is_none_or(|f| vals[1] < f.1) {
                far = Some((vals[0], vals[1]));
            }
        }
    }
    if let Some(why) = open {
        *unproved = why;
        return Ok(None);
    }
    if let Some((f3, f6)) = far {
        let below = best.as_ref().is_none_or(|bst| f6 < bst.0 - 1e-9 * (1.0 + bst.0.abs()));
        if below {
            if f6 < f3 - 1.0 && f6 < -1.0e3 {
                return Ok(Some((b.neg(c.inf()), vec![])));
            }
            return Ok(None);
        }
    }
    Ok(best.map(|(_, v, p, _)| (v, p)))
}

/// An end of a variable's range: (value, machine value, strict).
type End<'h> = Option<(T<'h>, f64, bool)>;

/// The constant ends of the range of `t` when every constraint of `cons` holding `t` is `a t + b <= 0` (or `< 0`)
/// with numbers `a != 0` and `b`, at most one each way; with the indices of those constraints. `None` otherwise.
#[allow(clippy::type_complexity)]
fn constant_range<'h>(c: &mut C<'_, '_, '_, '_, 'h>, cons: &[(T<'h>, Rel)], vars: &[T<'h>], t: T<'h>) -> Res<Option<(End<'h>, End<'h>, Vec<usize>)>> {
    let b = c.b;
    let (mut lo, mut hi, mut used) = (None, None, vec![]);
    for (j, &(e, rel)) in cons.iter().enumerate() {
        if is_free(b, e, t) {
            continue;
        }
        if rel == Rel::Eq || vars.iter().any(|&y| y != t && !is_free(b, e, y)) {
            return Ok(None);
        }
        let a = c.ev("d", &[e, t])?;
        let rest = c.cx.eval(b.sub(e, b.mul(&[a, t])))?;
        let rest = c.ev("expand", &[rest])?;
        if !is_free(b, a, t) || !is_free(b, rest, t) {
            return Ok(None);
        }
        let Some(an) = real_value(c, a)? else { return Ok(None) };
        let end = c.cx.eval(b.neg(b.div(rest, a)))?;
        let Some(en) = real_value(c, end)? else { return Ok(None) };
        let slot = if an > 0.0 {
            &mut hi
        } else if an < 0.0 {
            &mut lo
        } else {
            return Ok(None);
        };
        if slot.is_some() {
            return Ok(None);
        }
        *slot = Some((end, en, rel == Rel::Lt));
        used.push(j);
    }
    Ok(Some((lo, hi, used)))
}

/// The part of the unit circle `(u, v) = (cos t, sin t)` swept by `t` between the ends: `Some(None)` for the whole
/// circle (a range of at least a period, attained at one end at least), `Some(Some(arc))` for a shorter closed range
/// `[lo, hi]`: the arc is the circle cut by the chord through its ends, `u cos m + v sin m >= cos(w/2)` with `m`
/// the middle and `w` the width. `None` when not decided (a short range with an open end, an empty range).
fn circle_part<'h>(c: &mut C<'_, '_, '_, '_, 'h>, lo: End<'h>, hi: End<'h>, u: T<'h>, v: T<'h>) -> Res<Option<Option<T<'h>>>> {
    let b = c.b;
    let h = b.heap();
    let (Some((l, ln, ls)), Some((r, rn, rs))) = (lo, hi) else { return Ok(Some(None)) };
    let period = 2.0 * std::f64::consts::PI;
    let pi = h.sym_term(h.sym("pi"));
    let w = c.cx.eval(b.sub(r, l))?;
    let wn = rn - ln;
    if wn > period + 1e-9 {
        return Ok(Some(None));
    }
    if (wn - period).abs() <= 1e-9 {
        let gap = c.ev("simplify", &[b.sub(w, b.mul(&[b.int(2), pi]))])?;
        return Ok((b.is_zero(gap) && !(ls && rs)).then_some(None));
    }
    if wn <= 1e-9 || ls || rs {
        return Ok(None);
    }
    let m = c.cx.eval(b.div(b.add(&[l, r]), b.int(2)))?;
    let side = b.add(&[b.mul(&[u, b.call("cos", &[m])]), b.mul(&[v, b.call("sin", &[m])])]);
    let half = b.call("cos", &[b.div(w, b.int(2))]);
    Ok(Some(Some(c.cx.eval(b.call("greater_equal", &[side, half]))?)))
}

/// Trigonometric problems on boxes (polar coordinates, angles): a variable `t` that enters the objective and the
/// other constraints only through `cos(t)` and `sin(t)` (after `trig_expand`), and whose own constraints are
/// constant ends, is replaced by the point `(u, v) = (cos t, sin t)` of the unit circle (`u^2 + v^2 == 1`, cut to
/// the arc the range sweeps: `circle_part`). The feasible sets correspond point for point, so the two problems have
/// the same values; the new one is algebraic in `u, v`, and its minimum comes from `minimize` (critical points of
/// every face, or the cylindrical decomposition), proved there. The point maps back to `t = arg(u + i v) + 2 pi k`
/// inside the range, and the objective is checked there. `None` when no variable qualifies or the new problem has
/// no answer.
#[allow(clippy::type_complexity)]
fn circle_problem<'h>(c: &mut C<'_, '_, '_, '_, 'h>, g: T<'h>, cond: Option<T<'h>>, vars: &[T<'h>]) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let b = c.b;
    let h = b.heap();
    let (sin_s, cos_s) = (h.sym("sin"), h.sym("cos"));
    let trig = |t: T<'h>| find_call_of(h, t, sin_s).is_some() || find_call_of(h, t, cos_s).is_some();
    if !trig(g) && !cond.is_some_and(trig) {
        return Ok(None);
    }
    let cons: Conj<'h> = match cond {
        None => vec![],
        Some(k) => match constraints(c, k)? {
            Some(mut bs) if bs.len() == 1 => bs.pop().unwrap_or_default(),
            _ => return Ok(None),
        },
    };
    // the candidates: variables with constant ends only, in a sine or cosine
    let mut ranges: Vec<Option<(End<'h>, End<'h>, Vec<usize>)>> = vec![];
    for &t in vars {
        let inside = [g].iter().chain(cons.iter().map(|(e, _)| e)).any(|&e| {
            [sin_s, cos_s].iter().any(|&s| {
                let mut stack = mc_api::walk::Walk::new(e);
                let mut hit = false;
                while let Some(u) = stack.pop() {
                    if let View::App { head, args } = h.view(u) {
                        if b.sym_of(head) == Some(s) && args.iter().any(|&a| !is_free(b, a, t)) {
                            hit = true;
                            break;
                        }
                        stack.extend(args.iter().copied());
                    }
                }
                hit
            })
        });
        ranges.push(if inside { constant_range(c, &cons, vars, t)? } else { None });
    }
    let circles: Vec<(T<'h>, T<'h>)> = (0..vars.len())
        .map(|i| (h.sym_term(h.sym(&format!("optimize`u{i}"))), h.sym_term(h.sym(&format!("optimize`v{i}")))))
        .collect();
    let mut parts_of: Vec<Option<Option<T<'h>>>> = vec![];
    for (i, r) in ranges.iter().enumerate() {
        parts_of.push(match r {
            Some((lo, hi, _)) => circle_part(c, *lo, *hi, circles[i].0, circles[i].1)?,
            None => None,
        });
    }
    let mut eligible: Vec<bool> = parts_of.iter().map(|p| p.is_some()).collect();
    // the objective and the other constraints in u, v; a variable left over after the substitution stays
    let (g2, rest) = loop {
        if !eligible.iter().any(|&e| e) {
            return Ok(None);
        }
        let mut rules = vec![];
        for (i, &t) in vars.iter().enumerate() {
            if eligible[i] {
                rules.push(c.rule(b.call("cos", &[t]), circles[i].0));
                rules.push(c.rule(b.call("sin", &[t]), circles[i].1));
            }
        }
        let rl = c.list(&rules);
        let used: Vec<usize> = (0..vars.len()).filter(|&i| eligible[i]).flat_map(|i| ranges[i].as_ref().map(|r| r.2.clone()).unwrap_or_default()).collect();
        let mut pieces = vec![];
        for e in [g].into_iter().chain((0..cons.len()).filter(|j| !used.contains(j)).map(|j| cons[j].0)) {
            let x = c.ev("trig_expand", &[e])?;
            pieces.push(c.ev("rewrite", &[x, rl])?);
        }
        let mut changed = false;
        for (i, &t) in vars.iter().enumerate() {
            if eligible[i] && pieces.iter().any(|&p| !is_free(b, p, t)) {
                eligible[i] = false;
                changed = true;
            }
        }
        if !changed {
            let rels: Vec<Rel> = (0..cons.len()).filter(|j| !used.contains(j)).map(|j| cons[j].1).collect();
            break (pieces[0], pieces[1..].iter().copied().zip(rels).collect::<Vec<_>>());
        }
    };
    let mut vars2 = vec![];
    let mut cond2 = vec![];
    for (e, rel) in rest {
        let op = match rel {
            Rel::Le => "less_equal",
            Rel::Lt => "less",
            Rel::Eq => "equal",
        };
        cond2.push(b.call(op, &[e, b.zero()]));
    }
    for (i, &x) in vars.iter().enumerate() {
        if !eligible[i] {
            vars2.push(x);
            continue;
        }
        let (u, v) = circles[i];
        vars2.extend([u, v]);
        cond2.push(b.call("equal", &[b.add(&[b.powi(u, 2), b.powi(v, 2)]), b.one()]));
        if let Some(Some(arc)) = parts_of[i] {
            cond2.push(arc);
        }
    }
    let r = c.ev("minimize", &[c.list(&[g2, b.call("and", &cond2)]), c.list(&vars2)])?;
    let Some(&[val, rules]) = c.cx.as_list(r) else { return Ok(None) };
    let Some(rules) = c.cx.as_list(rules).map(|v| v.to_vec()).filter(|v| v.len() == vars2.len()) else { return Ok(None) };
    let Some(valn) = real_value(c, val)? else { return Ok(None) };
    let rl = c.list(&rules);
    let pi = h.sym_term(h.sym("pi"));
    let im = h.sym_term(h.sym("im"));
    let period = 2.0 * std::f64::consts::PI;
    let mut point = vec![];
    for (i, &x) in vars.iter().enumerate() {
        if !eligible[i] {
            let p = c.ev("rewrite", &[x, rl])?;
            point.push(p);
            continue;
        }
        let (u, v) = circles[i];
        let (u, v) = (c.ev("rewrite", &[u, rl])?, c.ev("rewrite", &[v, rl])?);
        let theta = c.ev("arg", &[b.add(&[u, b.mul(&[im, v])])])?;
        let Some(tn) = real_value(c, theta)? else { return Ok(None) };
        let (lo, hi, _) = ranges[i].clone().unwrap_or((None, None, vec![]));
        let near = |a: f64, z: f64| (a - z).abs() <= 1e-9 * (1.0 + z.abs());
        let k = match (lo, hi) {
            (Some((_, ln, ls)), _) => {
                let k = ((ln - tn) / period - 1e-12).ceil();
                if ls && near(tn + k * period, ln) {
                    k + 1.0
                } else {
                    k
                }
            }
            (None, Some((_, rn, rs))) => {
                let k = ((rn - tn) / period + 1e-12).floor();
                if rs && near(tn + k * period, rn) {
                    k - 1.0
                } else {
                    k
                }
            }
            (None, None) => 0.0,
        };
        let t = tn + k * period;
        if lo.is_some_and(|(_, ln, _)| t < ln - 1e-9 * (1.0 + ln.abs())) || hi.is_some_and(|(_, rn, _)| t > rn + 1e-9 * (1.0 + rn.abs())) {
            return Ok(None);
        }
        let p = b.add(&[theta, b.mul(&[b.int(2 * k as i64), pi])]);
        point.push(c.ev("simplify", &[p])?);
    }
    // the objective at the point mapped back, and the point in the original constraints
    let pr: Vec<T<'h>> = vars.iter().zip(&point).map(|(&x, &p)| c.rule(x, p)).collect();
    let pr = c.list(&pr);
    if !feasible(c, &cons, pr)? {
        return Ok(None);
    }
    let at = c.ev("rewrite", &[g, pr])?;
    match real_value(c, at)? {
        Some(an) if (an - valn).abs() <= 1e-8 * (1.0 + valn.abs()) => Ok(Some((val, point))),
        _ => Ok(None),
    }
}

/// The bases `u` of the powers `u^(p/q)` in `g` with 0 < p/q < 1 and `u` depending on one of `vars` (a point where
/// u vanishes has no gradient).
fn radical_kink<'h>(b: &Build<'h>, g: T<'h>, vars: &[T<'h>]) -> Vec<T<'h>> {
    let mut out = vec![];
    let h = b.heap();
    let mut stack = mc_api::walk::Walk::new(g);
    while let Some(u) = stack.pop() {
        let View::App { head, args } = h.view(u) else { continue };
        if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::POW) {
            if let [base, e] = args {
                let fractional = match h.view(*e) {
                    View::Rational { num, den } => match (h.view(num), h.view(den)) {
                        (View::Int(p), View::Int(q)) => p > 0 && q > p,
                        _ => false,
                    },
                    View::Real(v) => v > 0.0 && v < 1.0,
                    _ => false,
                };
                if fractional && vars.iter().any(|&x| !is_free(b, *base, x)) && !out.contains(base) {
                    out.push(*base);
                }
            }
        }
        stack.extend(args.iter().copied());
    }
    out
}

/// The solutions of `system` in `unknowns` as candidates (`consider`); `false` when not solved or a solution cannot
/// be judged.
fn judge<'h>(
    c: &mut C<'_, '_, '_, '_, 'h>,
    g: T<'h>,
    vars: &[T<'h>],
    cons: &[(T<'h>, Rel)],
    system: &[T<'h>],
    unknowns: &[T<'h>],
    best: &mut Best<'h>,
) -> Res<bool> {
    let Some(sols) = solve_eliminating(c, system, unknowns, 2)? else { return Ok(false) };
    let mut all = true;
    for rules in sols {
        all &= consider(c, g, vars, cons, &rules, best)?;
    }
    Ok(all)
}

/// `solve_staged`, and when that gives no answer, an unknown with exactly one solution of one of the equations (no
/// family, no condition: `x*log(y) == 2` gives `y -> exp(2/x)`) substituted into the others, up to `depth` times.
#[allow(clippy::type_complexity)]
fn solve_eliminating<'h>(c: &mut C<'_, '_, '_, '_, 'h>, system: &[T<'h>], unknowns: &[T<'h>], depth: usize) -> Res<Option<Vec<Vec<T<'h>>>>> {
    let b = c.b;
    let h = b.heap();
    let reals = h.sym_term(h.sym("reals"));
    // equations that are false (a constant gradient) leave no solution, true ones no condition
    let mut eqs = vec![];
    for &e in system {
        let v = c.cx.eval(e)?;
        if c.cx.is_false(v) {
            return Ok(Some(vec![]));
        }
        if !c.cx.is_true(v) {
            eqs.push(v);
        }
    }
    let system = &eqs[..];
    if let Some(s) = solve_staged(c, system, unknowns, reals)? {
        return Ok(Some(s));
    }
    if depth == 0 {
        return Ok(None);
    }
    // a product is zero where a factor is: one system per factor
    for (i, &e) in system.iter().enumerate() {
        let View::App { args: &[l, r], .. } = h.view(e) else { continue };
        let d = c.cx.eval(b.sub(l, r))?;
        let d = c.ev("factor", &[d])?;
        let View::App { head, args: fs } = h.view(d) else { continue };
        if b.sym_of(head) != Some(h.sym(names::MUL)) {
            continue;
        }
        let fs: Vec<T<'h>> = fs.iter().copied().filter(|&f| unknowns.iter().any(|&u| !is_free(b, f, u))).collect();
        if fs.len() < 2 || fs.len() > 4 {
            continue;
        }
        let mut out = vec![];
        for f in fs {
            let mut sub = system.to_vec();
            sub[i] = b.call("equal", &[f, b.zero()]);
            let Some(sols) = solve_eliminating(c, &sub, unknowns, depth - 1)? else { return Ok(None) };
            out.extend(sols);
        }
        return Ok(Some(out));
    }
    if system.len() < 2 {
        return Ok(None);
    }
    let ce = h.sym("conditional_expression");
    let cs = h.sym("c");
    for (i, &e) in system.iter().enumerate() {
        for (j, &u) in unknowns.iter().enumerate() {
            if is_free(b, e, u) {
                continue;
            }
            c.cx.tick(1)?;
            let s = c.ev("solve", &[e, u, reals])?;
            let Some(&[sol]) = c.cx.as_list(s) else { continue };
            let Some(&[rule]) = c.cx.as_list(sol) else { continue };
            let View::App { args: &[a, v], .. } = h.view(rule) else { continue };
            // a condition that holds for every real value of the other unknowns (`im_part(1/x) == 0`) is dropped
            let v = match h.view(v) {
                View::App { head, args: &[w, cond] } if b.sym_of(head) == Some(ce) => {
                    let real: Vec<T<'h>> = unknowns.iter().map(|&x| b.call("element", &[x, reals])).collect();
                    let t = c.ev("simplify", &[cond, b.call("and", &real)])?;
                    if !c.cx.is_true(t) {
                        continue;
                    }
                    w
                }
                _ => v,
            };
            if a != u || !is_free(b, v, u) || find_call_of(h, v, ce).is_some() || find_call_of(h, v, cs).is_some() {
                continue;
            }
            let r = c.list(&[c.rule(u, v)]);
            let others: Vec<T<'h>> = unknowns.iter().enumerate().filter(|&(k, _)| k != j).map(|(_, &x)| x).collect();
            let real: Vec<T<'h>> = others.iter().map(|&x| b.call("element", &[x, reals])).collect();
            let real = b.call("and", &real);
            let mut rest = vec![];
            for (k, &f) in system.iter().enumerate() {
                if k != i {
                    // `log(exp(w)) == w` for the real unknowns
                    let f = c.ev("rewrite", &[f, r])?;
                    rest.push(c.ev("simplify", &[f, real])?);
                }
            }
            let Some(sols) = solve_eliminating(c, &rest, &others, depth - 1)? else { continue };
            let mut out = vec![];
            for mut rs in sols {
                let back = c.ev("rewrite", &[v, c.list(&rs)])?;
                rs.push(c.rule(u, back));
                out.push(rs);
            }
            return Ok(Some(out));
        }
    }
    Ok(None)
}

/// The members of a family of solutions with one integer parameter `c(1)` that lie in the feasible set: a
/// coordinate `a + s c(1)` (a number `s != 0`) ranges over [min, max] of its variable on the (algebraic) constraints,
/// from `min_value` and `max_value`, which bounds `c(1)`; each member is a candidate. `false` when no coordinate is
/// linear in `c(1)`, the bounds are not found, or there are more than 200 members.
fn family<'h>(
    c: &mut C<'_, '_, '_, '_, 'h>,
    g: T<'h>,
    vars: &[T<'h>],
    cons: &[(T<'h>, Rel)],
    rules: &[T<'h>],
    general: &[T<'h>],
    best: &mut Best<'h>,
) -> Res<bool> {
    let b = c.b;
    let h = b.heap();
    let c1 = b.call("c", &[b.one()]);
    let c2 = b.call("c", &[b.int(2)]);
    if general.iter().any(|&p| !is_free(b, p, c2)) || !cons.iter().all(|&(e, _)| algebraic(b, e, vars)) {
        return Ok(false);
    }
    let ce = h.sym("conditional_expression");
    let cond: Vec<T<'h>> = cons
        .iter()
        .map(|&(e, rel)| {
            let op = match rel {
                Rel::Le => "less_equal",
                Rel::Lt => "less",
                Rel::Eq => "equal",
            };
            b.call(op, &[e, b.zero()])
        })
        .collect();
    let cond = b.call("and", &cond);
    for (i, &p) in general.iter().enumerate() {
        let p = match h.view(p) {
            View::App { head, args: &[w, _] } if b.sym_of(head) == Some(ce) => w,
            _ => p,
        };
        let slope = c.ev("d", &[p, c1])?;
        if !is_free(b, slope, c1) || vars.iter().any(|&x| !is_free(b, slope, x)) {
            continue;
        }
        let Some(sn) = real_value(c, slope)? else { continue };
        if sn.abs() < 1e-12 {
            continue;
        }
        let a = c.ev("rewrite", &[p, c.rule(c1, b.zero())])?;
        if vars.iter().any(|&x| !is_free(b, a, x)) {
            continue;
        }
        let Some(an) = real_value(c, a)? else { return Ok(false) };
        let pair = c.list(&[vars[i], cond]);
        let lo = c.ev("min_value", &[pair, c.list(vars)])?;
        let hi = c.ev("max_value", &[pair, c.list(vars)])?;
        let (Some(lo), Some(hi)) = (real_value(c, lo)?, real_value(c, hi)?) else { return Ok(false) };
        let (k0, k1) = ((lo - an) / sn, (hi - an) / sn);
        let (k0, k1) = (k0.min(k1).floor() as i64 - 1, k0.max(k1).ceil() as i64 + 1);
        if k1 - k0 > 200 {
            return Ok(false);
        }
        for k in k0..=k1 {
            c.cx.tick(1)?;
            let kr = c.list(&[c.rule(c1, b.int(k))]);
            let mut member = vec![];
            for &r in rules {
                member.push(c.ev("rewrite", &[r, kr])?);
            }
            if !consider(c, g, vars, cons, &member, best)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    Ok(false)
}

/// The k-element subsets of 0..n, in order.
fn subsets(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = vec![];
    let mut cur = vec![];
    fn go(i: usize, n: usize, k: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if cur.len() == k {
            out.push(cur.clone());
            return;
        }
        for j in i..n {
            cur.push(j);
            go(j + 1, n, k, cur, out);
            cur.pop();
        }
    }
    go(0, n, k, &mut cur, &mut out);
    out
}

/// The critical points of a face when `solve` does not give them all (a curve of critical points, on which `g` is
/// constant): the critical values are the real roots of the polynomial in `z` that `eliminate` leaves from the
/// system with `z == g` (finitely many for a polynomial problem); from the smallest up, each value below the best
/// so far is attained at a feasible critical point from `find_instance` (a candidate, and the face is done) or proved
/// not to be (`find_instance` gives `[]`). `false` when a step gives no answer (no polynomial in `z`, roots or
/// instances not found).
fn critical_values<'h>(
    c: &mut C<'_, '_, '_, '_, 'h>,
    g: T<'h>,
    vars: &[T<'h>],
    cons: &[(T<'h>, Rel)],
    system: &[T<'h>],
    unknowns: &[T<'h>],
    best: &mut Best<'h>,
) -> Res<bool> {
    let b = c.b;
    let h = b.heap();
    let z = h.sym_term(h.sym("optimize`z"));
    // `eliminate` and `find_instance` decide polynomial systems with algebraic numbers as coefficients only
    let rational = |t: T<'h>| {
        let mut stack = mc_api::walk::Walk::new(t);
        while let Some(u) = stack.pop() {
            match h.view(u) {
                View::Sym(_) if !unknowns.contains(&u) => return false,
                View::App { head, args } => {
                    let ok = match b.sym_of(head).map(|s| h.sym_name(s)) {
                        Some(n) if n == names::POW => args.len() == 2 && matches!(h.view(args[1]), View::Int(_) | View::Rational { .. }),
                        Some(n) => matches!(n.as_str(), names::ADD | names::MUL | "equal" | "less" | "less_equal"),
                        None => false,
                    };
                    if !ok {
                        return false;
                    }
                    stack.extend(args.iter().copied());
                }
                _ => {}
            }
        }
        true
    };
    if !rational(g) || !system.iter().all(|&e| rational(e)) || !cons.iter().all(|&(e, _)| rational(e)) {
        return Ok(false);
    }
    // machine numbers as the rationals they stand for; a denominator `q` cleared with a new unknown `w`,
    // `w q == 1`, so that the points where it vanishes do not enter
    let mut eqs = vec![];
    let mut elim = unknowns.to_vec();
    for &e in system.iter().chain([b.call("equal", &[z, g])].iter()) {
        let d = match h.view(e) {
            View::App { args: &[l, r], .. } => c.cx.eval(b.sub(l, r))?,
            _ => e,
        };
        let d = c.ev("rationalize", &[d, b.zero()])?;
        let d = c.ev("together", &[d])?;
        let num = c.ev("numerator", &[d])?;
        let den = c.ev("denominator", &[d])?;
        eqs.push(b.call("equal", &[num, b.zero()]));
        if !c.is_number(den)? {
            let w = h.sym_term(h.sym(&format!("optimize`w{}", elim.len())));
            eqs.push(b.call("equal", &[c.cx.eval(b.mul(&[w, den]))?, b.one()]));
            elim.push(w);
        }
    }
    let r = c.ev("eliminate", &[c.list(&eqs), c.list(&elim)])?;
    if c.cx.is_false(r) {
        return Ok(true);
    }
    let View::App { head, args: &[l, rr] } = h.view(r) else { return Ok(false) };
    if b.sym_of(head) != Some(h.sym("equal")) {
        return Ok(false);
    }
    let p = c.cx.eval(b.sub(l, rr))?;
    if is_free(b, p, z) || unknowns.iter().any(|&x| !is_free(b, p, x)) {
        return Ok(false);
    }
    let roots = c.ev("solve", &[b.call("equal", &[p, b.zero()]), z, h.sym_term(h.sym("reals"))])?;
    let Some(items) = c.cx.as_list(roots).map(|v| v.to_vec()) else { return Ok(false) };
    let mut vals = vec![];
    for it in items {
        let v = c.ev("rewrite", &[z, it])?;
        let Some(vn) = real_value(c, v)? else { return Ok(false) };
        vals.push((vn, v));
    }
    vals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let gr = c.ev("rationalize", &[g, b.zero()])?;
    let mut parts = vec![];
    for &e in system {
        parts.push(c.ev("rationalize", &[e, b.zero()])?);
    }
    for &(e, rel) in cons {
        let e = c.ev("rationalize", &[e, b.zero()])?;
        let op = match rel {
            Rel::Le => "less_equal",
            Rel::Lt => "less",
            Rel::Eq => "equal",
        };
        parts.push(b.call(op, &[e, b.zero()]));
    }
    for (vn, v) in vals {
        if best.as_ref().is_some_and(|bst| vn >= bst.0 - 1e-12 * (1.0 + vn.abs())) {
            break;
        }
        c.cx.tick(1)?;
        let mut at = parts.clone();
        at.push(b.call("equal", &[gr, v]));
        let fi = c.ev("find_instance", &[b.call("and", &at), c.list(unknowns)])?;
        let Some(items) = c.cx.as_list(fi).map(|v| v.to_vec()) else { return Ok(false) };
        let Some(&first) = items.first() else { continue };
        let Some(rules) = c.cx.as_list(first).map(|v| v.to_vec()) else { return Ok(false) };
        if !consider(c, g, vars, cons, &rules, best)? || !best.as_ref().is_some_and(|bst| bst.0 <= vn + 1e-9 * (1.0 + vn.abs())) {
            return Ok(false);
        }
        break;
    }
    Ok(true)
}

/// Take the solution `rules` as a candidate when it is a real feasible point smaller than the best so far. `false`
/// when the solution cannot be judged: a coordinate that is not a number (an unknown left free, an integer
/// parameter `c(k)` of a periodic family). A solution set with free unknowns is not trusted to be complete either
/// (`solve` of a system with a curve of solutions can leave out isolated ones: `solve([x*(x - 1) == 0, x*y == 0],
/// [x, y])` gives `x == 0` only), so the minimum is then not proved.
fn consider<'h>(
    c: &mut C<'_, '_, '_, '_, 'h>,
    g: T<'h>,
    vars: &[T<'h>],
    cons: &[(T<'h>, Rel)],
    rules: &[T<'h>],
    best: &mut Best<'h>,
) -> Res<bool> {
    let b = c.b;
    let rl = c.list(rules);
    let mut general = vec![];
    for &x in vars {
        general.push(c.ev("rewrite", &[x, rl])?);
    }
    let c1 = b.call("c", &[b.one()]);
    if general.iter().any(|&p| !is_free(b, p, c1)) {
        return family(c, g, vars, cons, rules, &general, best);
    }
    let free: Vec<T<'h>> = vars.iter().copied().filter(|&y| general.iter().any(|&p| !is_free(b, p, y))).collect();
    if !free.is_empty() {
        // a point of the set (the free variables at 0) is still a candidate; the set is not trusted complete
        let zero: Vec<T<'h>> = free.iter().map(|&x| c.rule(x, b.zero())).collect();
        let zl = c.list(&zero);
        let mut fixed = vec![];
        for (&x, &p) in vars.iter().zip(&general) {
            let v = c.ev("rewrite", &[p, zl])?;
            fixed.push(c.rule(x, v));
        }
        consider(c, g, vars, cons, &fixed, best)?;
        return Ok(false);
    }
    let mut point = vec![];
    for v in general {
        if !c.is_number(v)? {
            return Ok(false);
        }
        point.push(v);
    }
    let pr: Vec<T<'h>> = vars.iter().zip(&point).map(|(&x, &p)| c.rule(x, p)).collect();
    let pr = c.list(&pr);
    if !feasible(c, cons, pr)? {
        return Ok(true);
    }
    let v = c.ev("rewrite", &[g, pr])?;
    let mut key = vec![];
    for &p in &point {
        // a point with a non-real coordinate is not a point of the real problem
        let Some(coord) = real_value(c, p)? else { return Ok(true) };
        key.push(coord);
    }
    // outside the objective's real domain
    let Some(nv) = real_value(c, v)? else { return Ok(true) };
    // ties go to the smaller point (coordinate by coordinate)
    let better = match best.as_ref() {
        None => true,
        Some(bst) => {
            let tol = 1e-12 * (1.0 + nv.abs());
            nv < bst.0 - tol || (nv <= bst.0 + tol && key.iter().zip(&bst.3).map(|(a, b)| a.total_cmp(b)).find(|o| o.is_ne()) == Some(std::cmp::Ordering::Less))
        }
    };
    if better {
        let mut pts = vec![];
        for p in point {
            pts.push(c.ev("simplify", &[p])?);
        }
        *best = Some((nv, c.ev("simplify", &[v])?, pts, key));
    }
    Ok(true)
}

/// Fixed sample directions: the axes both ways, the diagonals and a few others.
fn directions(n: usize) -> Vec<Vec<f64>> {
    let mut out = vec![];
    for i in 0..n {
        for s in [1.0, -1.0] {
            let mut d = vec![0.0; n];
            d[i] = s;
            out.push(d);
        }
    }
    for mask in 0..(1u32 << n.min(6)) {
        out.push((0..n).map(|i| if i < 6 && mask >> i & 1 == 1 { -1.0 } else { 1.0 }).collect());
    }
    let mut s = 0x2545_F491_4F6C_DD1Du64;
    for _ in 0..16 {
        out.push(
            (0..n)
                .map(|_| {
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    (s % 2001) as f64 / 1000.0 - 1.0
                })
                .collect(),
        );
    }
    out
}

/// `solve(system, unknowns, reals)` as lists of rules, after substituting the equations linear in an unknown with
/// coefficient and rest free of the unknowns (so that the rest may become solvable); `None` when solve gives no
/// list.
#[allow(clippy::type_complexity)]
fn solve_staged<'h>(c: &mut C<'_, '_, '_, '_, 'h>, system: &[T<'h>], unknowns: &[T<'h>], reals: T<'h>) -> Res<Option<Vec<Vec<T<'h>>>>> {
    let b = c.b;
    let h = b.heap();
    let mut fixed: Vec<T<'h>> = vec![];
    let mut rest: Vec<T<'h>> = vec![];
    let mut left: Vec<T<'h>> = unknowns.to_vec();
    for &e in system {
        let mut done = false;
        if let View::App { args: &[l, r], .. } = h.view(e) {
            let d = c.cx.eval(b.sub(l, r))?;
            for (i, &x) in left.iter().enumerate() {
                let dd = c.ev("d", &[d, x])?;
                if b.is_zero(dd) || unknowns.iter().any(|&u| !is_free(b, dd, u)) {
                    continue;
                }
                let r0 = c.ev("rewrite", &[d, c.rule(x, b.zero())])?;
                if unknowns.iter().all(|&u| is_free(b, r0, u)) {
                    let v = c.cx.eval(b.neg(b.div(r0, dd)))?;
                    fixed.push(c.rule(x, v));
                    left.remove(i);
                    done = true;
                    break;
                }
            }
        }
        if !done {
            rest.push(e);
        }
    }
    let rest: Vec<T<'h>> = if fixed.is_empty() {
        rest
    } else {
        let fl = c.list(&fixed);
        let mut out = vec![];
        for e in rest {
            out.push(c.ev("rewrite", &[e, fl])?);
        }
        out
    };
    let rest: Vec<T<'h>> = rest.into_iter().filter(|&e| !c.cx.is_true(e)).collect();
    if rest.is_empty() {
        return Ok(Some(vec![fixed]));
    }
    if left.is_empty() || rest.iter().any(|&e| c.cx.is_false(e)) {
        return Ok(Some(vec![]));
    }
    // the last variables first: a solution with free variables then expresses the last ones in the first ones,
    // which are set to 0
    let order: Vec<T<'h>> = left.iter().rev().copied().collect();
    let s = c.ev("solve", &[c.list(&rest), c.list(&order), reals])?;
    let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
    let mut out = vec![];
    for it in items {
        let Some(rs) = c.cx.as_list(it).map(|v| v.to_vec()) else { return Ok(None) };
        let mut all = rs.clone();
        let rl = c.list(&rs);
        for &f in &fixed {
            all.push(c.ev("rewrite", &[f, rl])?);
        }
        out.push(all);
    }
    Ok(Some(out))
}

/// One point satisfying `cond`.
/// Does `cond` hold `element(v, booleans)` (anywhere)?
fn boolean_declared<'h>(h: &'h impl Store, cond: T<'h>, v: T<'h>) -> bool {
    let el = h.sym_term(h.sym("element"));
    let bools = h.sym_term(h.sym("booleans"));
    let mut stack = mc_api::walk::Walk::new(cond);
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if head == el && args.len() == 2 && args[0] == v && args[1] == bools {
                return true;
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Points of the families of `solve(cond, x)` in one variable: each solution with its integer parameters `c(k)` set
/// to 0, 1, -1, 2, -2 in turn, kept when the condition evaluates to `true` there (exact substitution); up to `n` of
/// them. `[]` when `solve` finds no solution of an equation; `None` when fewer than `n` points are found.
fn family_points<'h>(c: &mut C<'_, '_, '_, '_, 'h>, cond: T<'h>, x: T<'h>, n: usize) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    let eq_head = h.sym_term(h.sym("equal"));
    if !matches!(h.view(cond), View::App { head, .. } if head == eq_head) {
        return Ok(None);
    }
    let s = c.ev("solve", &[cond, x])?;
    let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
    if items.is_empty() {
        return Ok(Some(c.list(&[])));
    }
    let ce = h.sym_term(h.sym("conditional_expression"));
    let mut vals = vec![];
    for &sol in &items {
        let Some(&[r]) = c.cx.as_list(sol) else { return Ok(None) };
        let View::App { args: &[a, v], .. } = h.view(r) else { return Ok(None) };
        if a != x {
            return Ok(None);
        }
        let v = match h.view(v) {
            View::App { head, args: &[e, _] } if head == ce => e,
            _ => v,
        };
        vals.push(v);
    }
    let cs: Vec<T<'h>> = (1..=3).map(|k| b.call("c", &[b.int(k)])).collect();
    let mut out: Vec<T<'h>> = vec![];
    for k in [0i64, 1, -1, 2, -2, 3, -3] {
        for &v in &vals {
            c.cx.tick(1)?;
            let fixed: Vec<T<'h>> = cs.iter().map(|&ck| c.rule(ck, b.int(k))).collect();
            let p = c.ev("rewrite", &[v, c.list(&fixed)])?;
            if !is_free(b, p, cs[0]) || !is_free(b, p, cs[1]) || !is_free(b, p, cs[2]) {
                return Ok(None);
            }
            let pt = c.list(&[c.rule(x, p)]);
            if out.contains(&pt) {
                continue;
            }
            let t = c.ev("rewrite", &[cond, pt])?;
            // a shift by whole periods is removed by `simplify` (cos(acos(1/5) + 2 pi) == 1/5)
            let t = if c.cx.is_true(t) || c.cx.is_false(t) { t } else { c.ev("simplify", &[t])? };
            if c.cx.is_true(t) {
                out.push(pt);
                if out.len() == n {
                    return Ok(Some(c.list(&out)));
                }
            }
        }
    }
    Ok(None)
}

fn find_instance<'h>(c: &mut C<'_, '_, '_, '_, 'h>, cond: T<'h>, vars: &[T<'h>], n: usize) -> Res<Option<T<'h>>> {
    let b = c.b;
    let h = b.heap();
    if let [x] = vars {
        if n > 1 {
            return family_points(c, cond, *x, n);
        }
        let Some((ends, points)) = pieces(c, cond, *x)? else {
            let r = c.ev("reduce", &[cond, *x, h.sym_term(h.sym("reals"))])?;
            if c.cx.is_false(r) {
                return Ok(Some(c.list(&[])));
            }
            return family_points(c, cond, *x, 1);
        };
        let check = |c: &mut C<'_, '_, '_, '_, 'h>, v: T<'h>| -> Res<bool> {
            let r = c.at(cond, *x, v)?;
            Ok(c.cx.is_true(r))
        };
        let mut cands: Vec<T<'h>> = points.clone();
        for &(e, _, _) in &ends {
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
    // a variable declared Boolean (element(a, booleans)): each truth value in turn, the rest searched with it
    if let Some(i) = vars.iter().position(|&v| boolean_declared(h, cond, v)) {
        if n > 1 {
            return Ok(None);
        }
        let a = vars[i];
        let rest: Vec<T<'h>> = vars.iter().copied().filter(|&v| v != a).collect();
        for val in [names::TRUE, names::FALSE] {
            let tv = h.sym_term(h.sym(val));
            let ci = c.ev("rewrite", &[cond, c.rule(a, tv)])?;
            if c.cx.is_false(ci) {
                continue;
            }
            let sub = if rest.is_empty() {
                if c.cx.is_true(ci) { c.list(&[c.list(&[])]) } else { continue }
            } else {
                c.ev("find_instance", &[ci, c.list(&rest)])?
            };
            if let Some(&[first]) = c.cx.as_list(sub) {
                if let Some(rs) = c.cx.as_list(first) {
                    let mut out = vec![];
                    for &x in vars {
                        if x == a {
                            out.push(c.rule(a, tv));
                        } else if let Some(&r) = rs.iter().find(|&&r| matches!(h.view(r), View::App { args: &[k, _], .. } if k == x)) {
                            out.push(r);
                        }
                    }
                    if out.len() == vars.len() {
                        return Ok(Some(c.list(&[c.list(&out)])));
                    }
                }
            }
        }
        return Ok(None);
    }
    let s = c.ev("solve", &[cond, c.list(vars)])?;
    let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
    if items.is_empty() {
        return Ok(Some(c.list(&[])));
    }
    // each solution with the unknowns not solved for set to 0, 1, 2, -1, 1/2, ...; the instance is the first whose
    // substitution makes the whole condition true (a solution of the equations can still violate a `!=`, as z = 0
    // does for x y z != 0); undecided ones are kept for the end, false ones dropped
    // the equations hold by construction: only the other conjuncts are checked, exactly and then numerically
    let eq_head = h.sym_term(h.sym("equal"));
    let is_eq = |t: T<'h>| matches!(h.view(t), View::App { head, .. } if head == eq_head);
    let conjuncts: Vec<T<'h>> = match h.view(cond) {
        View::App { head, args } if head == h.sym_term(h.sym("and")) => args.to_vec(),
        _ => vec![cond],
    };
    let eqs: Vec<T<'h>> = conjuncts.iter().copied().filter(|&t| is_eq(t)).collect();
    let others: Vec<T<'h>> = conjuncts.into_iter().filter(|&t| !is_eq(t)).collect();
    let rest = match others.as_slice() {
        [] => h.sym_term(h.sym(names::TRUE)),
        [one] => *one,
        _ => h.call(h.sym("and"), &others),
    };
    let mut fallback = None;
    let mut found: Vec<T<'h>> = vec![];
    let mut complete = items.len() <= 8;
    for &sol in items.iter().take(8) {
        let Some(rules) = c.cx.as_list(sol).map(|v| v.to_vec()) else { return Ok(None) };
        let free: Vec<T<'h>> = vars
            .iter()
            .copied()
            .filter(|&x| !rules.iter().any(|&r| matches!(h.view(r), View::App { args: &[a, _], .. } if a == x)))
            .collect();
        if !free.is_empty() {
            complete = false;
        }
        let tries: Vec<T<'h>> = if free.is_empty() {
            vec![b.zero()]
        } else {
            vec![b.zero(), b.int(1), b.int(2), b.int(-1), b.rat(1, 2), b.int(3), b.int(-2)]
        };
        for &t in &tries {
            let fixed: Vec<T<'h>> = free.iter().map(|&x| c.rule(x, t)).collect();
            let mut out = vec![];
            let mut excluded = false;
            for &x in vars {
                let v = c.ev("rewrite", &[x, c.list(&rules)])?;
                let mut v = c.ev("rewrite", &[v, c.list(&fixed)])?;
                // a value solve keeps under a condition on the unknowns left free (x y z != 0 with z free): at the
                // chosen values the condition is on numbers, decided at 30 digits (ball arithmetic); a value whose
                // condition fails is no instance
                if let View::App { head, args: &[w, k] } = h.view(v) {
                    if head == h.sym_term(h.sym("conditional_expression")) {
                        let truth = c.ev("n", &[k, b.int(30)])?;
                        if c.cx.is_true(truth) {
                            v = w;
                        } else if c.cx.is_false(truth) {
                            excluded = true;
                        }
                    }
                }
                out.push(c.rule(x, v));
            }
            if excluded {
                continue;
            }
            let point = c.list(&out);
            // at the point's 30-digit values (products of nested radicals are slow to expand exactly); a value that
            // is exactly 0 stays exact
            let approx = c.ev("n", &[point, b.int(30)])?;
            let r = c.ev("rewrite", &[rest, approx])?;
            // the equations again, numerically: a parametric radical solution can change branch at a value of
            // the free unknown, so that the point does not solve them
            let mut eqs_hold = true;
            for &e in eqs.iter() {
                let View::App { args: &[l, rr], .. } = h.view(e) else { continue };
                let d = c.ev("rewrite", &[b.add(&[l, b.mul(&[b.int(-1), rr])]), approx])?;
                let z = c.ev("chop", &[d])?;
                if z != b.zero() {
                    eqs_hold = false;
                    break;
                }
            }
            if !eqs_hold {
                complete = false;
                continue;
            }
            if c.cx.is_true(r) {
                if !found.contains(&point) {
                    found.push(point);
                }
                if found.len() == n {
                    return Ok(Some(c.list(&found)));
                }
                continue;
            }
            if !c.cx.is_false(r) {
                complete = false;
                if fallback.is_none() {
                    fallback = Some(point);
                }
            }
        }
    }
    if n == 1 {
        return Ok(fallback.map(|p| c.list(&[p])));
    }
    // fewer than n: all of them when the solution set is finite and every solution was examined
    Ok(complete.then(|| c.list(&found)))
}

/// Optimization of step functions of one variable on a bounded domain.
///
/// The objective holds floor, ceiling, round, integer_part, unit_step, heaviside_theta or sign of continuous
/// arguments (possibly nested). The domain [lo, hi] is cut at the points where an innermost step atom jumps (its
/// argument crosses an integer, a half-integer for round, 0 for the sign functions: found exactly by `solve` for every
/// level between the exact minimum and maximum of the argument on the piece); on each open piece the atom is a
/// constant (its value at the midpoint), and the cutting repeats for the atoms of the remaining expression. Each
/// piece then carries a continuous function whose minimum on the closed piece is found by the continuous method;
/// at an end of the piece that minimum is only a limit of the objective, which takes its own value at the cut point
/// (a candidate of its own). The smallest attained value wins (among equal values the rightmost point); when a limit
/// lies below every attained value the infimum is not attained and only the value is known (Hansen and Walster,
/// Global Optimization Using Interval Analysis, 2004, ch. 15, for the splitting of a domain at the discontinuities).
mod steps {
    use super::{is_free, minimize1, pieces, View, C, T};
    use mc_api::Store;
    use mc_eval::Res;

    /// The step functions cut at.
    const STEP: &[&str] = &["floor", "ceiling", "round", "integer_part", "unit_step", "heaviside_theta", "sign"];
    /// The most pieces of one domain.
    const MAX_PIECES: usize = 400;

    /// The step atoms of `g` that hold `x` (each once).
    pub(super) fn atoms<'h>(c: &C<'_, '_, '_, '_, 'h>, g: T<'h>, x: T<'h>) -> Vec<T<'h>> {
        let b = c.b;
        let h = b.heap();
        let mut out = vec![];
        let mut stack = mc_api::walk::Walk::new(g);
        while let Some(u) = stack.pop() {
            if let View::App { head, args } = h.view(u) {
                if args.len() == 1 && b.sym_of(head).is_some_and(|s| STEP.contains(&h.sym_name(s).as_str())) && !is_free(b, u, x) && !out.contains(&u) {
                    out.push(u);
                }
                stack.extend(args.iter().copied());
            }
        }
        out
    }

    /// `cond` with every comparison of floor(u) or ceiling(u) with an integer n written as one of u (floor(u) < n is
    /// u < n, ceiling(u) < n is u <= n - 1, and so on); other parts unchanged.
    pub(super) fn plain_condition<'h>(c: &mut C<'_, '_, '_, '_, 'h>, cond: T<'h>) -> Res<T<'h>> {
        let b = c.b;
        let h = b.heap();
        let View::App { head, args } = h.view(cond) else { return Ok(cond) };
        let Some(s) = b.sym_of(head) else { return Ok(cond) };
        let name = h.sym_name(s);
        match (name.as_str(), args) {
            ("and" | "or", _) => {
                let mut out = vec![];
                for &a in args {
                    out.push(plain_condition(c, a)?);
                }
                c.cx.eval(h.call(s, &out))
            }
            (op @ ("less" | "less_equal" | "greater" | "greater_equal"), &[l, r]) => {
                // as step(u) op n, the step on the left
                let (step, n, op) = if matches!(h.view(r), View::Int(_)) {
                    (l, r, op)
                } else if matches!(h.view(l), View::Int(_)) {
                    let flipped = match op {
                        "less" => "greater",
                        "less_equal" => "greater_equal",
                        "greater" => "less",
                        _ => "less_equal",
                    };
                    (r, l, flipped)
                } else {
                    return Ok(cond);
                };
                let View::Int(nv) = h.view(n) else { return Ok(cond) };
                let View::App { head: sh, args: &[u] } = h.view(step) else { return Ok(cond) };
                let Some(ss) = b.sym_of(sh) else { return Ok(cond) };
                let floor = match h.sym_name(ss).as_str() {
                    "floor" => true,
                    "ceiling" => false,
                    _ => return Ok(cond),
                };
                // floor(u) < n: u < n; <= n: u < n + 1; > n: u >= n + 1; >= n: u >= n
                // ceiling(u) < n: u <= n - 1; <= n: u <= n; > n: u > n; >= n: u > n - 1
                let (rel, m) = match (floor, op) {
                    (true, "less") => ("less", nv),
                    (true, "less_equal") => ("less", nv + 1),
                    (true, "greater") => ("greater_equal", nv + 1),
                    (true, _) => ("greater_equal", nv),
                    (false, "less") => ("less_equal", nv - 1),
                    (false, "less_equal") => ("less_equal", nv),
                    (false, "greater") => ("greater", nv),
                    (false, _) => ("greater", nv - 1),
                };
                c.cx.eval(b.call(rel, &[u, b.int(m)]))
            }
            _ => Ok(cond),
        }
    }

    /// An innermost atom: one whose argument holds no other step atom.
    fn innermost<'h>(c: &C<'_, '_, '_, '_, 'h>, g: T<'h>, x: T<'h>) -> Option<T<'h>> {
        let h = c.b.heap();
        atoms(c, g, x).into_iter().find(|&a| match h.view(a) {
            View::App { args: &[u], .. } => atoms(c, u, x).is_empty(),
            _ => false,
        })
    }

    /// The jumps of the atom `a` inside the open interval (l, r), exact and sorted; `None` when they are not found.
    fn jumps<'h>(c: &mut C<'_, '_, '_, '_, 'h>, a: T<'h>, x: T<'h>, l: T<'h>, r: T<'h>) -> Res<Option<Vec<T<'h>>>> {
        let b = c.b;
        let h = b.heap();
        let View::App { head, args: &[u] } = h.view(a) else { return Ok(None) };
        let name = b.sym_of(head).map(|s| h.sym_name(s)).unwrap_or_default();
        let (Some(lv), Some(rv)) = (c.num(l)?, c.num(r)?) else { return Ok(None) };
        let closed = b.call("and", &[b.call("less_equal", &[l, x]), b.call("less_equal", &[x, r])]);
        // the levels where the atom jumps
        let levels: Vec<T<'h>> = if matches!(name.as_str(), "unit_step" | "heaviside_theta" | "sign") {
            vec![b.zero()]
        } else {
            let Some((lo_u, _)) = minimize1(c, u, Some(closed), x, None)? else { return Ok(None) };
            let Some((hi_u, _)) = minimize1(c, b.neg(u), Some(closed), x, None)? else { return Ok(None) };
            let (Some(a0), Some(a1)) = (c.num(lo_u)?, c.num(b.neg(hi_u))?) else { return Ok(None) };
            let half = if name == "round" { 0.5 } else { 0.0 };
            let k0 = (a0 - half).floor() as i64;
            let k1 = (a1 - half).ceil() as i64;
            if k1 - k0 > MAX_PIECES as i64 {
                return Ok(None);
            }
            (k0..=k1).map(|k| if name == "round" { b.rat(2 * k + 1, 2) } else { b.int(k) }).collect()
        };
        let reals = h.sym_term(h.sym("reals"));
        let mut out: Vec<(f64, T<'h>)> = vec![];
        for lev in levels {
            c.cx.tick(1)?;
            let eq = b.call("and", &[b.call("equal", &[u, lev]), closed]);
            let s = c.ev("solve", &[eq, x, reals])?;
            let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
            for it in items {
                let Some(&[rule]) = c.cx.as_list(it) else { return Ok(None) };
                let View::App { args: &[_, p], .. } = h.view(rule) else { return Ok(None) };
                if !is_free(b, p, x) {
                    return Ok(None);
                }
                let Some(pv) = c.num(p)? else { return Ok(None) };
                if pv > lv && pv < rv && !out.iter().any(|q| (q.0 - pv).abs() <= 1e-12 * (1.0 + pv.abs())) {
                    out.push((pv, p));
                }
            }
            if out.len() > MAX_PIECES {
                return Ok(None);
            }
        }
        out.sort_by(|p, q| p.0.total_cmp(&q.0));
        Ok(Some(out.into_iter().map(|p| p.1).collect()))
    }

    /// One piece: the open interval and the continuous function the objective is on it.
    struct Piece<'h> {
        l: T<'h>,
        r: T<'h>,
        g: T<'h>,
    }

    /// Cut `g` on (l, r) until no step atom is left; the cut points into `cuts`. False when a cut is not found.
    fn split<'h>(c: &mut C<'_, '_, '_, '_, 'h>, g: T<'h>, x: T<'h>, l: T<'h>, r: T<'h>, out: &mut Vec<Piece<'h>>, cuts: &mut Vec<T<'h>>) -> Res<bool> {
        let b = c.b;
        c.cx.tick(1)?;
        let Some(a) = innermost(c, g, x) else {
            out.push(Piece { l, r, g });
            return Ok(true);
        };
        let Some(js) = jumps(c, a, x, l, r)? else { return Ok(false) };
        cuts.extend(js.iter().copied());
        let mut ends = vec![l];
        ends.extend(js);
        ends.push(r);
        for w in ends.windows(2) {
            let mid = c.cx.eval(b.div(b.add(&[w[0], w[1]]), b.int(2)))?;
            let av = c.at(a, x, mid)?;
            if c.num(av)?.is_none() {
                return Ok(false);
            }
            let rule = c.rule(a, av);
            let gi = c.ev("rewrite", &[g, rule])?;
            if !split(c, gi, x, w[0], w[1], out, cuts)? || out.len() > MAX_PIECES {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The best attained value so far: (value, exact value, point, machine point).
    type Best<'h> = Option<(f64, T<'h>, T<'h>, f64)>;

    /// `v` at `p` as the best when it is smaller, or equal at a point further right.
    fn offer<'h>(best: &mut Best<'h>, nv: f64, v: T<'h>, p: T<'h>, pv: f64) {
        let tol = 1e-12 * (1.0 + nv.abs());
        let better = match best {
            None => true,
            Some(bst) => nv < bst.0 - tol || (nv <= bst.0 + tol && pv > bst.3),
        };
        if better {
            *best = Some((nv, v, p, pv));
        }
    }

    /// The minimum of the step function `g` (module comment).
    #[allow(clippy::type_complexity)]
    pub(super) fn minimize_steps<'h>(c: &mut C<'_, '_, '_, '_, 'h>, g: T<'h>, cond: Option<T<'h>>, x: T<'h>) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
        let b = c.b;
        let h = b.heap();
        // no other symbols
        let mut stack = mc_api::walk::Walk::new(g);
        while let Some(u) = stack.pop() {
            match h.view(u) {
                View::Sym(s) if u != x && !matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma") => return Ok(None),
                View::App { args, .. } => stack.extend(args.iter().copied()),
                _ => {}
            }
        }
        // a bounded domain: its ends and single points
        let Some(k) = cond else { return Ok(None) };
        let Some((ends, points)) = pieces(c, k, x)? else { return Ok(None) };
        let mut marks: Vec<(f64, T<'h>)> = vec![];
        for p in ends.iter().map(|e| e.0).chain(points.iter().copied()) {
            let Some(v) = c.num(p)? else { return Ok(None) };
            if !marks.iter().any(|q| q.0 == v) {
                marks.push((v, p));
            }
        }
        marks.sort_by(|p, q| p.0.total_cmp(&q.0));
        let (Some(first), Some(last)) = (marks.first().map(|m| m.1), marks.last().map(|m| m.1)) else { return Ok(None) };
        let inside = |c: &mut C<'_, '_, '_, '_, 'h>, v: T<'h>| -> Res<bool> {
            let r = c.at(k, x, v)?;
            Ok(c.cx.is_true(r))
        };
        for v in [b.sub(first, b.one()), b.add(&[last, b.one()])] {
            let v = c.cx.eval(v)?;
            if inside(c, v)? {
                return Ok(None);
            }
        }
        let mut cuts: Vec<T<'h>> = marks.iter().map(|m| m.1).collect();
        let mut ps: Vec<Piece<'h>> = vec![];
        for w in marks.windows(2) {
            let mid = c.cx.eval(b.div(b.add(&[w[0].1, w[1].1]), b.int(2)))?;
            if inside(c, mid)? && !split(c, g, x, w[0].1, w[1].1, &mut ps, &mut cuts)? {
                return Ok(None);
            }
        }
        let mut best: Best<'h> = None;
        let mut limit: Option<(f64, T<'h>)> = None;
        // the values at the cut points and the domain's ends
        for p in cuts {
            c.cx.tick(1)?;
            if !inside(c, p)? {
                continue;
            }
            let v = c.at(g, x, p)?;
            let (Some(nv), Some(pv)) = (c.num(v)?, c.num(p)?) else { continue };
            let v = c.ev("simplify", &[v])?;
            offer(&mut best, nv, v, p, pv);
        }
        // the pieces
        for pc in ps {
            c.cx.tick(1)?;
            let (Some(lv), Some(rv)) = (c.num(pc.l)?, c.num(pc.r)?) else { return Ok(None) };
            if is_free(b, pc.g, x) {
                let Some(nv) = c.num(pc.g)? else { return Ok(None) };
                let mid = c.cx.eval(b.div(b.add(&[pc.l, pc.r]), b.int(2)))?;
                let v = c.ev("simplify", &[pc.g])?;
                offer(&mut best, nv, v, mid, (lv + rv) / 2.0);
                continue;
            }
            let closed = b.call("and", &[b.call("less_equal", &[pc.l, x]), b.call("less_equal", &[x, pc.r])]);
            let Some((v, pt)) = minimize1(c, pc.g, Some(closed), x, None)? else { return Ok(None) };
            let [p] = pt.as_slice() else { return Ok(None) };
            let (Some(nv), Some(pv)) = (c.num(v)?, c.num(*p)?) else { return Ok(None) };
            if pv > lv && pv < rv {
                offer(&mut best, nv, v, *p, pv);
            } else if limit.as_ref().is_none_or(|l| nv < l.0) {
                limit = Some((nv, v));
            }
        }
        if let Some((nl, l)) = limit {
            if best.as_ref().is_none_or(|bst| nl < bst.0 - 1e-12 * (1.0 + nl.abs())) {
                // the infimum is a limit at a cut point, not attained
                return Ok(Some((c.ev("simplify", &[l])?, vec![])));
            }
        }
        Ok(best.map(|(_, v, p, _)| (v, vec![p])))
    }
}



