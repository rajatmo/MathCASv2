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
            return Ok(find_instance(&mut c, *fc, &vars)?.map_or(Reply::Decline, Reply::Done));
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
        let r = match vars.as_slice() {
            [x] => minimize1(&mut c, g, cond, *x)?,
            _ => minimize_n(&mut c, g, cond, &vars)?,
        };
        let Some((v, point)) = r else { return Ok(Reply::Decline) };
        let v = if max { c.cx.eval(b.neg(v))? } else { v };
        if point.len() != vars.len() {
            // unbounded: only the value is known
            return Ok(match self.kind {
                Kind::MinValue | Kind::MaxValue => Reply::Done(v),
                _ => Reply::Decline,
            });
        }
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
    Ok(best.map(|(_, v, p)| (v, vec![p])))
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
/// variables the constraints alone); a solution leaving variables free has them set to 0. The smallest value at a
/// feasible candidate is the minimum when no far feasible point (sampled along fixed directions) is smaller;
/// `(-inf, [])` when the values keep falling far out.
#[allow(clippy::type_complexity)]
fn minimize_n<'h>(c: &mut C<'_, '_, '_, '_, 'h>, g: T<'h>, cond: Option<T<'h>>, vars: &[T<'h>]) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let b = c.b;
    let h = b.heap();
    let n = vars.len();
    let branches = match cond {
        None => vec![vec![]],
        Some(k) => match constraints(c, k)? {
            Some(bs) => bs,
            None => return Ok(None),
        },
    };
    let reals = h.sym_term(h.sym("reals"));
    let grad_g = c.ev("d", &[g, c.list(vars)])?;
    let Some(grad_g) = c.cx.as_list(grad_g).map(|v| v.to_vec()) else { return Ok(None) };
    let mut best: Best<'h> = None;
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
            let Some(sols) = solve_staged(c, &system, &unknowns, reals)? else { continue };
            for rules in sols {
                consider(c, g, vars, cons, &rules, &mut best)?;
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

/// Take the solution `rules` as a candidate when it is a real feasible point smaller than the best so far.
fn consider<'h>(
    c: &mut C<'_, '_, '_, '_, 'h>,
    g: T<'h>,
    vars: &[T<'h>],
    cons: &[(T<'h>, Rel)],
    rules: &[T<'h>],
    best: &mut Best<'h>,
) -> Res<()> {
    let b = c.b;
    // the variables' values, free ones set to 0
    let zero_rules: Vec<T<'h>> = vars.iter().map(|&x| c.rule(x, b.zero())).collect();
    let (rl, zl) = (c.list(rules), c.list(&zero_rules));
    let mut point = vec![];
    for &x in vars {
        let v = c.ev("rewrite", &[x, rl])?;
        let v = c.ev("rewrite", &[v, zl])?;
        if vars.iter().any(|&y| !is_free(b, v, y)) {
            return Ok(());
        }
        point.push(v);
    }
    let pr: Vec<T<'h>> = vars.iter().zip(&point).map(|(&x, &p)| c.rule(x, p)).collect();
    let pr = c.list(&pr);
    if !feasible(c, cons, pr)? {
        return Ok(());
    }
    let v = c.ev("rewrite", &[g, pr])?;
    let Some(nv) = real_value(c, v)? else { return Ok(()) };
    let mut key = vec![];
    for &p in &point {
        key.push(real_value(c, p)?.unwrap_or(f64::INFINITY));
    }
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
    Ok(())
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
