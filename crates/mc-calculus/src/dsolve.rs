//! Symbolic ordinary differential equations and recurrences (decision 2C.13): `d_solve`, `d_solve_value`,
//! `r_solve`, `r_solve_value`.
//!
//! `d_solve(eqs, y, x)` for one unknown function `y` of `x` (`y` or `y(x)` as the second argument), the equation and
//! optional conditions `y(x0) == v`, `y'(x0) == v`, ... in `eqs`:
//!
//! - **linear with constant coefficients**, any order: the roots of the characteristic polynomial (with their
//!   multiplicities; a complex pair `a ± b i` as `exp(a x) cos(b x)`, `exp(a x) sin(b x)`), and for a right-hand side
//!   a particular solution by variation of parameters (orders 1 and 2, through `integrate`);
//! - **Euler** equations `a x^2 y'' + b x y' + c y = 0` through the indicial polynomial;
//! - **first-order linear** with variable coefficients through the integrating factor;
//! - **separable** first-order equations `y' = g(x) h(y)`: `integrate(1/h, y) == integrate(g, x) + c(1)` solved
//!   for `y` (every branch `solve` gives).
//!
//! The constants are `c(1)`, `c(2)`, ...; the conditions fix them (`solve` of the linear system). The result is
//! `[[y -> fn(list(x), sol)]]` (`[[y(x) -> sol]]` for `y(x)` as the second argument), one rule list per solution;
//! `d_solve_value` gives `sol`. Anything else leaves the call unevaluated.
//!
//! `r_solve(eqs, a, n)` (or `a(n)`) for linear recurrences with constant coefficients `sum c_k a(n + k) == 0`
//! (shifts may be negative): the roots of the characteristic polynomial, `r^n` (`n^j r^n` for a multiple root), the
//! constants fixed by initial values `a(k) == v`.
use crate::integrate::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Which function.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// `d_solve`
    DSolve,
    /// `d_solve_value`
    DSolveValue,
    /// `r_solve`
    RSolve,
    /// `r_solve_value`
    RSolveValue,
}

/// The native.
pub struct DSolveNative {
    /// which function
    pub kind: Kind,
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// The highest order handled.
const MAX_ORDER: usize = 12;

struct Cx<'a, 'b, 'c, 'k, 'h> {
    cx: &'a mut NativeCx<'c, 'k, 'h>,
    b: &'b Build<'h>,
}

impl<'h> Cx<'_, '_, '_, '_, 'h> {
    fn ev(&mut self, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
        let t = self.b.call(name, args);
        self.cx.eval(t)
    }
    fn eval(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.cx.eval(t)
    }
    fn sym(&self, n: &str) -> T<'h> {
        let h = self.b.heap();
        h.sym_term(h.sym(n))
    }
    fn rule(&self, a: T<'h>, v: T<'h>) -> T<'h> {
        self.b.call(names::RULE, &[a, v])
    }
    fn list(&self, xs: &[T<'h>]) -> T<'h> {
        self.b.heap().call(self.b.hd.list, xs)
    }
    fn subst(&mut self, e: T<'h>, rules: &[T<'h>]) -> Res<T<'h>> {
        let l = self.list(rules);
        self.ev("rewrite", &[e, l])
    }
    /// Is `t` an unevaluated call of `name` (or does it contain a failure)?
    fn stuck(&self, t: T<'h>, name: &str) -> bool {
        let h = self.b.heap();
        !is_free(self.b, t, h.sym_term(h.sym(name))) || !is_free(self.b, t, h.sym_term(h.sym(names::FAILURE)))
    }
    fn constant(&self, i: usize) -> T<'h> {
        self.b.call("c", &[self.b.int(i as i64)])
    }
}

/// The equations, flattened from lists and `and`.
fn flatten<'h>(b: &Build<'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } if b.sym_of(head) == Some(b.hd.list) || b.sym_of(head) == Some(h.sym("and")) => {
            for &a in args {
                flatten(b, a, out);
            }
        }
        _ => out.push(t),
    }
}

/// `(k, arg)` when `t` is `y(arg)` (k = 0) or `derivative(k)(y)(arg)`.
fn occurrence<'h>(b: &Build<'h>, t: T<'h>, y: T<'h>) -> Option<(usize, T<'h>)> {
    let h = b.heap();
    let View::App { head, args: &[arg] } = h.view(t) else { return None };
    if head == y {
        return Some((0, arg));
    }
    let (orders, f) = b.as_derivative(head)?;
    match orders.as_slice() {
        &[k] if f == y && k >= 1 => Some((k as usize, arg)),
        _ => None,
    }
}

/// Every occurrence of `y` in `t`.
fn occurrences<'h>(b: &Build<'h>, t: T<'h>, y: T<'h>, out: &mut Vec<(usize, T<'h>, T<'h>)>) {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let Some((k, a)) = occurrence(b, u, y) {
            out.push((k, a, u));
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
}

impl Native for DSolveNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [eqs, yspec, x] = args else { return Ok(Reply::Decline) };
        if !matches!(h.view(*x), View::Sym(_)) {
            return Ok(Reply::Decline);
        }
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        // y or y(x)
        let (y, applied) = match h.view(*yspec) {
            View::Sym(_) => (*yspec, false),
            View::App { head, args: &[a] } if a == *x && matches!(h.view(head), View::Sym(_)) => (head, true),
            _ => return Ok(Reply::Decline),
        };
        let mut c = Cx { cx, b: &b };
        let sols = match self.kind {
            Kind::DSolve | Kind::DSolveValue => d_solve(&mut c, *eqs, y, *x)?,
            Kind::RSolve | Kind::RSolveValue => r_solve(&mut c, *eqs, y, *x)?,
        };
        let Some(sols) = sols else { return Ok(Reply::Decline) };
        let value = matches!(self.kind, Kind::DSolveValue | Kind::RSolveValue);
        if value {
            return Ok(Reply::Done(match sols.as_slice() {
                [s] => *s,
                _ => c.list(&sols),
            }));
        }
        let mut out = vec![];
        for s in sols {
            let r = if applied {
                c.rule(*yspec, s)
            } else {
                let f = b.call("fn", &[c.list(&[*x]), s]);
                c.rule(y, f)
            };
            out.push(c.list(&[r]));
        }
        Ok(Reply::Done(c.list(&out)))
    }
}

/// The differential equation and the conditions `(order, point, value)`.
struct Problem<'h> {
    eq: T<'h>,
    conds: Vec<(usize, T<'h>, T<'h>)>,
}

/// Split `eqs` into the equation in `x` and the conditions at points.
fn problem<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, eqs: T<'h>, y: T<'h>, x: T<'h>) -> Option<Problem<'h>> {
    let b = c.b;
    let h = b.heap();
    let mut all = vec![];
    flatten(b, eqs, &mut all);
    let mut eq = None;
    let mut conds = vec![];
    for e in all {
        let View::App { head, args: &[l, r] } = h.view(e) else { return None };
        if b.sym_of(head) != Some(h.sym("equal")) {
            return None;
        }
        let mut occ = vec![];
        occurrences(b, e, y, &mut occ);
        if occ.is_empty() {
            return None;
        }
        if occ.iter().all(|o| o.1 == x) {
            if eq.is_some() {
                return None;
            }
            eq = Some(e);
            continue;
        }
        // a condition: one side an occurrence at a point free of x
        let (o, v) = match (occurrence(b, l, y), occurrence(b, r, y)) {
            (Some(o), _) if is_free(b, r, y) => (o, r),
            (_, Some(o)) if is_free(b, l, y) => (o, l),
            _ => return None,
        };
        if !is_free(b, o.1, x) {
            return None;
        }
        conds.push((o.0, o.1, v));
    }
    Some(Problem { eq: eq?, conds })
}

/// The general solutions (with constants `c(i)`), then the conditions applied.
fn d_solve<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, eqs: T<'h>, y: T<'h>, x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let Some(p) = problem(c, eqs, y, x) else { return Ok(None) };
    // the equation in the symbols Y0, Y1, ... for y, y', ...
    let mut occ = vec![];
    occurrences(b, p.eq, y, &mut occ);
    let n = occ.iter().map(|o| o.0).max().unwrap_or(0);
    if n == 0 || n > MAX_ORDER {
        return Ok(None);
    }
    let ys: Vec<T<'h>> = (0..=n).map(|k| c.sym(&format!("d_solve·{k}"))).collect();
    let pairs: Vec<(T<'h>, T<'h>)> = occ.iter().map(|&(k, _, term)| (term, ys[k])).collect();
    let View::App { args: &[l, r], .. } = b.heap().view(p.eq) else { return Ok(None) };
    let diff = c.eval(replace_terms(b, b.sub(l, r), &pairs))?;
    let diff = c.ev("expand", &[diff])?;
    let general = match general(c, diff, &ys, x, n)? {
        Some(g) => g,
        None => return Ok(None),
    };
    let mut out = vec![];
    for g in general {
        match conditions(c, g, &p.conds, x, n)? {
            Some(mut v) => out.append(&mut v),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// The general solutions of `diff == 0` (in the symbols `ys` for y, y', ..., y^(n)).
fn general<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, diff: T<'h>, ys: &[T<'h>], x: T<'h>, n: usize) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    // linear: every coefficient d(diff, Yk) free of the Ys
    let mut coefs = Vec::with_capacity(n + 1);
    let mut linear = true;
    for &yk in ys {
        let ck = c.ev("d", &[diff, yk])?;
        let ck = c.ev("simplify", &[ck])?;
        linear &= ys.iter().all(|&v| is_free(b, ck, v));
        coefs.push(ck);
    }
    if linear {
        let zeros: Vec<T<'h>> = ys.iter().map(|&v| c.rule(v, b.zero())).collect();
        let rest = c.subst(diff, &zeros)?;
        let f = c.ev("simplify", &[b.neg(rest)])?;
        let constant = coefs.iter().all(|&ck| is_free(b, ck, x));
        if constant {
            return Ok(constant_coefficients(c, &coefs, f, x)?.map(|s| vec![s]));
        }
        if let Some(s) = euler(c, &coefs, f, x)? {
            return Ok(Some(vec![s]));
        }
        if n == 1 {
            return Ok(first_order_linear(c, coefs[0], coefs[1], f, x)?.map(|s| vec![s]));
        }
        return Ok(None);
    }
    if n == 1 {
        return separable(c, diff, ys, x);
    }
    Ok(None)
}

/// The roots of the polynomial with coefficients `coefs` (lowest first) in a fresh variable, with multiplicities.
fn roots<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>]) -> Res<Option<Vec<(T<'h>, usize)>>> {
    let b = c.b;
    let m = c.sym("d_solve·m");
    let terms: Vec<T<'h>> = coefs.iter().enumerate().map(|(k, &ck)| b.mul(&[ck, b.powi(m, k as i64)])).collect();
    let poly = b.add(&terms);
    let eq = b.call("equal", &[poly, b.zero()]);
    let r = c.ev("roots", &[eq, m])?;
    if c.stuck(r, "roots") {
        return Ok(None);
    }
    let h = b.heap();
    let mut ors = vec![];
    match h.view(r) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("or")) => ors.extend_from_slice(args),
        _ => ors.push(r),
    }
    let mut out: Vec<(T<'h>, usize)> = vec![];
    for e in ors {
        let View::App { args: &[_, v], .. } = h.view(e) else { return Ok(None) };
        if !is_free(b, v, h.sym_term(h.sym("root"))) {
            return Ok(None);
        }
        match out.iter_mut().find(|(r, _)| *r == v) {
            Some(e) => e.1 += 1,
            None => out.push((v, 1)),
        }
    }
    Ok(Some(out))
}

/// The basis of solutions from roots: x^j exp(r x), complex pairs as exp(a x) cos(b x), exp(a x) sin(b x).
fn basis<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, rts: &[(T<'h>, usize)], x: T<'h>, log: bool) -> Res<Vec<T<'h>>> {
    let b = c.b;
    // the variable of the exponentials: x, or log(x) for an Euler equation
    let v = if log { b.call("log", &[x]) } else { x };
    // the numerical values of the roots, to pair complex conjugates
    let mut vals = vec![];
    for &(r, _) in rts {
        let nv = c.ev("n", &[r])?;
        let (re, im) = match b.heap().view(nv) {
            View::Real(v) => (v, 0.0),
            View::Int(k) => (k as f64, 0.0),
            _ => match b.ar.num(nv) {
                Some(mc_arith::Num::Complex(a, bb)) => (a.to_f64(), bb.to_f64()),
                _ => (f64::NAN, 0.0),
            },
        };
        vals.push((re, im));
    }
    let mut out = vec![];
    for (idx, &(r, mult)) in rts.iter().enumerate() {
        let (_, imv) = vals[idx];
        let complex = imv != 0.0;
        if complex && imv < 0.0 {
            // one of each conjugate pair: the one with a positive imaginary part
            continue;
        }
        let (re, im) = if complex {
            // the conjugate root: re = (r + s)/2, im = (r - s)/(2 i)
            let Some(k) = (0..rts.len()).find(|&k| k != idx && (vals[k].0 - vals[idx].0).abs() <= 1e-9 * (1.0 + vals[idx].0.abs()) && (vals[k].1 + imv).abs() <= 1e-9 * (1.0 + imv.abs())) else {
                return Ok(vec![]);
            };
            let s2 = rts[k].0;
            let re = c.ev("expand", &[b.mul(&[b.rat(1, 2), b.add(&[r, s2])])])?;
            let i2 = b.call(names::COMPLEX, &[b.zero(), b.int(-2)]);
            let im = c.ev("expand", &[b.div(b.sub(r, s2), b.neg(i2))])?;
            (re, im)
        } else {
            (r, b.zero())
        };
        for j in 0..mult {
            let pw = b.powi(v, j as i64);
            if complex {
                let e = b.call("exp", &[b.mul(&[re, v])]);
                let a = b.mul(&[im, v]);
                out.push(c.eval(b.mul(&[pw, e, b.call("cos", &[a])]))?);
                out.push(c.eval(b.mul(&[pw, e, b.call("sin", &[a])]))?);
            } else {
                let e = b.call("exp", &[b.mul(&[r, v])]);
                out.push(c.eval(b.mul(&[pw, e]))?);
            }
        }
    }
    Ok(out)
}

/// `sum c(i) y_i` over a basis.
fn combination<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ys: &[T<'h>]) -> Res<T<'h>> {
    let b = c.b;
    let terms: Vec<T<'h>> = ys.iter().enumerate().map(|(i, &y)| b.mul(&[c.constant(i + 1), y])).collect();
    c.eval(b.add(&terms))
}

/// Linear with constant coefficients: `sum coefs[k] y^(k) = f`.
fn constant_coefficients<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let Some(rts) = roots(c, coefs)? else { return Ok(None) };
    let ys = basis(c, &rts, x, false)?;
    if ys.len() != coefs.len() - 1 {
        return Ok(None);
    }
    let hom = combination(c, &ys)?;
    if f == b.zero() {
        return Ok(Some(hom));
    }
    let lead = coefs[coefs.len() - 1];
    let Some(yp) = particular(c, &ys, b.div(f, lead), x)? else { return Ok(None) };
    Ok(Some(c.eval(b.add(&[hom, yp]))?))
}

/// A particular solution of `y^(n) + ... = g` by variation of parameters (n = 1, 2).
fn particular<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, ys: &[T<'h>], g: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let r = match ys {
        [y1] => {
            let q = c.ev("simplify", &[b.div(g, *y1)])?;
            let i = c.ev("integrate", &[q, x])?;
            if c.stuck(i, "integrate") {
                return Ok(None);
            }
            b.mul(&[*y1, i])
        }
        [y1, y2] => {
            let d1 = c.ev("d", &[*y1, x])?;
            let d2 = c.ev("d", &[*y2, x])?;
            let w = c.ev("simplify", &[b.sub(b.mul(&[*y1, d2]), b.mul(&[d1, *y2]))])?;
            let a = c.ev("simplify", &[b.div(b.mul(&[*y2, g]), w)])?;
            let a = c.ev("integrate", &[a, x])?;
            let e = c.ev("simplify", &[b.div(b.mul(&[*y1, g]), w)])?;
            let e = c.ev("integrate", &[e, x])?;
            if c.stuck(a, "integrate") || c.stuck(e, "integrate") {
                return Ok(None);
            }
            b.add(&[b.neg(b.mul(&[*y1, a])), b.mul(&[*y2, e])])
        }
        _ => return Ok(None),
    };
    Ok(Some(c.ev("simplify", &[r])?))
}

/// An Euler equation `a x^2 y'' + b x y' + c y = 0` (a, b, c constants): `x^m` for the roots m of `a m (m - 1) + b m
/// + c`.
fn euler<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, coefs: &[T<'h>], f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    if coefs.len() != 3 || f != b.zero() {
        return Ok(None);
    }
    // a = coefs[2]/x^2, bb = coefs[1]/x, cc = coefs[0] constant
    let a = c.ev("simplify", &[b.div(coefs[2], b.powi(x, 2))])?;
    let bb = c.ev("simplify", &[b.div(coefs[1], x)])?;
    let cc = coefs[0];
    if ![a, bb, cc].iter().all(|&t| is_free(b, t, x)) {
        return Ok(None);
    }
    let ind = [cc, b.sub(bb, a), a];
    let Some(rts) = roots(c, &ind)? else { return Ok(None) };
    let ys = basis(c, &rts, x, true)?;
    if ys.len() != 2 {
        return Ok(None);
    }
    Ok(Some(combination(c, &ys)?))
}

/// `c1 y' + c0 y = f` with variable coefficients: the integrating factor.
fn first_order_linear<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, c0: T<'h>, c1: T<'h>, f: T<'h>, x: T<'h>) -> Res<Option<T<'h>>> {
    let b = c.b;
    let a = c.ev("simplify", &[b.neg(b.div(c0, c1))])?;
    let q = c.ev("simplify", &[b.div(f, c1)])?;
    let ia = c.ev("integrate", &[a, x])?;
    if c.stuck(ia, "integrate") {
        return Ok(None);
    }
    let mu = c.ev("simplify", &[b.call("exp", &[ia])])?;
    let inner = if q == b.zero() {
        c.constant(1)
    } else {
        let w = c.ev("simplify", &[b.div(q, mu)])?;
        let i = c.ev("integrate", &[w, x])?;
        if c.stuck(i, "integrate") {
            return Ok(None);
        }
        b.add(&[i, c.constant(1)])
    };
    Ok(Some(c.ev("expand", &[b.mul(&[mu, inner])])?))
}

/// A separable first-order equation `y' = g(x) h(y)`.
fn separable<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, diff: T<'h>, ys: &[T<'h>], x: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let h = b.heap();
    let eq = b.call("equal", &[diff, b.zero()]);
    let s = c.ev("solve", &[eq, ys[1]])?;
    let first = c.cx.as_list(s).and_then(|v| v.first().copied());
    let Some(rule) = first.and_then(|f| c.cx.as_list(f).and_then(|v| v.first().copied())) else { return Ok(None) };
    let View::App { args: &[_, fxy], .. } = h.view(rule) else { return Ok(None) };
    let y = ys[0];
    // h(y) = F at a point x* where it is finite and nonzero; g = F/h must be free of y
    let mut hy = None;
    for xs in [0i64, 1, 2, 3] {
        let at = c.subst(fxy, &[c.rule(x, b.int(xs))])?;
        if !c.stuck(at, "indeterminate") && at != b.zero() && is_free(b, at, h.sym_term(h.sym("complex_infinity"))) {
            hy = Some(at);
            break;
        }
    }
    let Some(hy) = hy else { return Ok(None) };
    let g = c.ev("simplify", &[b.div(fxy, hy)])?;
    if !is_free(b, g, y) {
        return Ok(None);
    }
    let lhs = c.ev("integrate", &[b.powi(hy, -1), y])?;
    let rhs = c.ev("integrate", &[g, x])?;
    if c.stuck(lhs, "integrate") || c.stuck(rhs, "integrate") {
        return Ok(None);
    }
    let implicit = b.call("equal", &[lhs, b.add(&[rhs, c.constant(1)])]);
    let sols = c.ev("solve", &[implicit, y])?;
    let Some(items) = c.cx.as_list(sols).map(|v| v.to_vec()) else { return Ok(None) };
    let mut out = vec![];
    for s in items {
        let Some(rules) = c.cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
        let Some(&r) = rules.first() else { return Ok(None) };
        let View::App { args: &[_, v], .. } = h.view(r) else { return Ok(None) };
        out.push(v);
    }
    if out.is_empty() {
        return Ok(None);
    }
    Ok(Some(out))
}

/// The solution `g` (with constants) under the conditions: the constants solved for (one solution per solution of
/// the system); `g` itself without conditions.
fn conditions<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, g: T<'h>, conds: &[(usize, T<'h>, T<'h>)], x: T<'h>, n: usize) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    if conds.is_empty() {
        return Ok(Some(vec![g]));
    }
    let mut eqs = vec![];
    for &(k, x0, v) in conds {
        let dk = if k == 0 { g } else { c.ev("d", &[g, c.list(&[x, b.int(k as i64)])])? };
        let at = c.subst(dk, &[c.rule(x, x0)])?;
        eqs.push(b.call("equal", &[at, v]));
    }
    let used: Vec<T<'h>> = (1..=n).map(|i| c.constant(i)).filter(|&ci| !is_free(b, g, ci)).collect();
    let s = c.ev("solve", &[c.list(&eqs), c.list(&used)])?;
    let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
    if items.is_empty() {
        return Ok(Some(vec![]));
    }
    let mut out = vec![];
    for rs in items {
        let Some(rules) = c.cx.as_list(rs).map(|v| v.to_vec()) else { return Ok(None) };
        let v = c.subst(g, &rules)?;
        out.push(c.ev("simplify", &[v])?);
    }
    Ok(Some(out))
}

/// `r_solve` for linear recurrences with constant coefficients.
fn r_solve<'h>(c: &mut Cx<'_, '_, '_, '_, 'h>, eqs: T<'h>, a: T<'h>, n: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    let b = c.b;
    let h = b.heap();
    let mut all = vec![];
    flatten(b, eqs, &mut all);
    let mut rec = None;
    let mut inits = vec![];
    for e in all {
        let View::App { head, args: &[l, r] } = h.view(e) else { return Ok(None) };
        if b.sym_of(head) != Some(h.sym("equal")) {
            return Ok(None);
        }
        let mut occ = vec![];
        occurrences(b, e, a, &mut occ);
        if occ.iter().any(|o| o.0 != 0) || occ.is_empty() {
            return Ok(None);
        }
        if occ.iter().any(|o| !is_free(b, o.1, n)) {
            if rec.is_some() {
                return Ok(None);
            }
            rec = Some((e, occ));
        } else {
            let (k, v) = match (h.view(l), h.view(r)) {
                (View::App { head: hl, args: &[k] }, _) if hl == a => (k, r),
                (_, View::App { head: hr, args: &[k] }) if hr == a => (k, l),
                _ => return Ok(None),
            };
            inits.push((k, v));
        }
    }
    let Some((e, occ)) = rec else { return Ok(None) };
    // the shifts: a(n + s)
    let mut shifts = vec![];
    for &(_, arg, term) in &occ {
        let s = c.ev("simplify", &[b.sub(arg, n)])?;
        let View::Int(s) = h.view(s) else { return Ok(None) };
        shifts.push((s, term));
    }
    let lo = shifts.iter().map(|s| s.0).min().unwrap_or(0);
    let hi = shifts.iter().map(|s| s.0).max().unwrap_or(0);
    let order = (hi - lo) as usize;
    if order == 0 || order > MAX_ORDER {
        return Ok(None);
    }
    let vs: Vec<T<'h>> = (0..=order).map(|k| c.sym(&format!("r_solve·{k}"))).collect();
    let pairs: Vec<(T<'h>, T<'h>)> = shifts.iter().map(|&(s, term)| (term, vs[(s - lo) as usize])).collect();
    let View::App { args: &[l, r], .. } = h.view(e) else { return Ok(None) };
    let diff = c.eval(replace_terms(b, b.sub(l, r), &pairs))?;
    let mut coefs = vec![];
    for &v in &vs {
        let ck = c.ev("d", &[diff, v])?;
        if vs.iter().any(|&w| !is_free(b, ck, w)) || !is_free(b, ck, n) {
            return Ok(None);
        }
        coefs.push(ck);
    }
    let zeros: Vec<T<'h>> = vs.iter().map(|&v| c.rule(v, b.zero())).collect();
    let rest = c.subst(diff, &zeros)?;
    if c.ev("simplify", &[rest])? != b.zero() {
        return Ok(None);
    }
    let Some(rts) = roots(c, &coefs)? else { return Ok(None) };
    let mut basis = vec![];
    for &(r, mult) in &rts {
        if r == b.zero() {
            return Ok(None);
        }
        for j in 0..mult {
            basis.push(c.eval(b.mul(&[b.powi(n, j as i64), b.pow(r, n)]))?);
        }
    }
    if basis.len() != order {
        return Ok(None);
    }
    let g = combination(c, &basis)?;
    if inits.is_empty() {
        return Ok(Some(vec![g]));
    }
    let mut eqs = vec![];
    for &(k, v) in &inits {
        let at = c.subst(g, &[c.rule(n, k)])?;
        eqs.push(b.call("equal", &[at, v]));
    }
    let consts: Vec<T<'h>> = (1..=order).map(|i| c.constant(i)).collect();
    let s = c.ev("solve", &[c.list(&eqs), c.list(&consts)])?;
    let Some(items) = c.cx.as_list(s).map(|v| v.to_vec()) else { return Ok(None) };
    let mut out = vec![];
    for rs in items {
        let Some(rules) = c.cx.as_list(rs).map(|v| v.to_vec()) else { return Ok(None) };
        let v = c.subst(g, &rules)?;
        out.push(c.ev("simplify", &[v])?);
    }
    Ok(Some(out))
}

/// `t` with each subterm equal to a `from` of `pairs` replaced by its `to` (exact terms, not patterns: the
/// occurrences `y'(x)` are calls with compound heads, which rules cannot match).
fn replace_terms<'h>(b: &Build<'h>, t: T<'h>, pairs: &[(T<'h>, T<'h>)]) -> T<'h> {
    if let Some(&(_, to)) = pairs.iter().find(|p| p.0 == t) {
        return to;
    }
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } => {
            let new: Vec<T<'h>> = args.iter().map(|&a| replace_terms(b, a, pairs)).collect();
            if new.as_slice() == args {
                t
            } else {
                h.app(head, &new)
            }
        }
        _ => t,
    }
}
