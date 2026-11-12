//! `reduce` of polynomial equations and `!=` conditions over the complex numbers in several variables (or one
//! variable with parameters): a triangular decomposition, the complete answer as a disjunction of triangular
//! systems in which each variable is solved in terms of the earlier ones (item 11B).
//!
//! The listed variables `x_1, ..., x_n` come after the parameters (the other symbols, lowest); later variables are
//! solved in terms of earlier ones, as the cylindrical form over the reals. The lexicographic Groebner basis `G`
//! of the equations (`x_n` highest) is split on the leading coefficient `l` of its element `g` of least degree in
//! the highest variable `x_k` that leads an element:
//!
//! - `l != 0`: over the points of the lower basis where `l` does not vanish, `g` generates the fibre (Gianni 1989,
//!   Kalkbrener 1991: the specialisation of a lex basis), so `x_k` is a root of `g` and the lower elements are
//!   decomposed in turn, with `l != 0` added;
//! - `l == 0`: `l` is added to the equations and the whole is decomposed again (the ideal grows strictly, since
//!   `l` is not in it for a reduced basis, so this ends).
//!
//! This is the comprehensive (generic and special cases) splitting of Kapur, Sun and Wang (2010) restricted to
//! the leading coefficients of a lex basis. A `!=` condition is reduced to its normal form modulo the basis: zero
//! empties the case, a nonzero constant drops; the rest are kept at the end of each case. Roots in a variable
//! come from the formulas (degree up to 4, pure powers) or root objects. Every case is checked at a rational point
//! of its free variables: the solved values must satisfy the equations numerically (decision CAD.22).
use crate::cx::{Env, Tick};
use crate::system::lex_basis;
use crate::univariate::{poly_roots, root_objects, Alg};
use mc_poly::factor::factor;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{NativeCx, Res};
use mc_poly::convert::Ring;
use mc_poly::ratfun;
use mc_poly::work::{Halt, Meter, WorkResult};
use mc_poly::{Poly, Z};
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The most cases of one answer.
const MAX_CASES: usize = 64;
/// The deepest splitting.
const MAX_DEPTH: usize = 16;

/// A conjunct of one case.
#[derive(Clone)]
enum Item<'h> {
    /// `x == r_1 or x == r_2 ...`: the variable's level, the variable and its roots
    Eq(usize, T<'h>, Vec<T<'h>>),
    /// a leading coefficient that does not vanish: its level and its term
    Lc(usize, T<'h>),
    /// a condition of the input, reduced
    Ne(T<'h>),
}

impl Item<'_> {
    fn level(&self) -> usize {
        match self {
            Item::Eq(l, ..) | Item::Lc(l, _) => *l,
            Item::Ne(_) => usize::MAX,
        }
    }
}

struct Tri<'e, 'h> {
    env: &'e Env<'h>,
    ring: &'e Ring<'h>,
    /// ring variables, highest first (the lex order)
    ord: Vec<usize>,
}

/// `reduce(cond, vars)` over the complex numbers for conditions that are `and`, `or` and lists of polynomial
/// equations and `!=` with rational coefficients; `None` for anything else, or when a case fails its check.
pub(crate) fn complex<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, cond: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let env = Env::new(h, stop.clone(), "reduce");
    let Some(dnf) = disjuncts(&env, cond) else { return Ok(None) };
    if dnf.len() > MAX_CASES {
        return Ok(None);
    }
    let _scope = StopScope::enter(Some(stop));
    let mut tick = Tick { cx, stop, abort: None };
    let r = {
        let mut m = Meter::new(&mut tick);
        decompose(&env, &dnf, vars, &mut m)
    };
    let (cases, checks) = match r {
        Ok(Some(x)) => x,
        Ok(None) | Err(Halt::Memory) => return Ok(None),
        Err(Halt::Stopped) => match tick.abort {
            Some(a) => return Err(a),
            // raised through the number kernels' flag before the fuel refill saw the interrupt: stop, do not decline
            None if tick.cx.stopped() => return Err(mc_eval::Abort::Limit),
            None => return Ok(None),
        },
    };
    // every case checked at its point, in parallel (roadmap item 16A); the first failing case declines
    let (cfg, gen, name) = (env.cfg, env.gen, env.name);
    let ok = cx.par_until(checks.len(), &|ok: &bool| !*ok, &|cx, i| {
        let _scope = StopScope::enter(Some(stop));
        let mut env = Env::new(cx.heap(), stop.clone(), name);
        (env.cfg, env.gen) = (cfg, gen);
        let (items, eqs) = &checks[i];
        check(cx, &env, items, eqs, vars)
    })?;
    if ok.iter().any(|&k| !k) {
        return Ok(None);
    }
    let t = match cases.len() {
        0 => env.sym(names::FALSE),
        1 => cases[0],
        _ => env.call("or", &cases),
    };
    Ok(Some(cx.eval(t)?))
}

/// The disjunctive normal form: each disjunct a list of equation differences and a list of `!=` differences.
#[allow(clippy::type_complexity)]
fn disjuncts<'h>(env: &Env<'h>, t: T<'h>) -> Option<Vec<(Vec<T<'h>>, Vec<T<'h>>)>> {
    if env.is_sym(t, names::TRUE) {
        return Some(vec![(vec![], vec![])]);
    }
    if env.is_sym(t, names::FALSE) {
        return Some(vec![]);
    }
    let (name, args): (String, &[T<'h>]) = match env.as_list(t) {
        Some(items) => ("and".to_string(), items),
        None => env.head_name(t)?,
    };
    match name.as_str() {
        "and" => {
            let mut acc = vec![(vec![], vec![])];
            for &a in args {
                let d = disjuncts(env, a)?;
                let mut next = vec![];
                for (e1, n1) in &acc {
                    for (e2, n2) in &d {
                        let mut e = e1.clone();
                        e.extend_from_slice(e2);
                        let mut n = n1.clone();
                        n.extend_from_slice(n2);
                        next.push((e, n));
                    }
                }
                if next.len() > MAX_CASES {
                    return None;
                }
                acc = next;
            }
            Some(acc)
        }
        "or" => {
            let mut out = vec![];
            for &a in args {
                out.extend(disjuncts(env, a)?);
            }
            Some(out)
        }
        "equal" | "unequal" if args.len() == 2 => {
            let d = env.ar.sub(args[0], args[1]);
            Some(if name == "equal" { vec![(vec![d], vec![])] } else { vec![(vec![], vec![d])] })
        }
        _ => None,
    }
}

/// The cases (one term each) and, for the checks, each case's items and the equations of its disjunct.
#[allow(clippy::type_complexity)]
fn decompose<'h>(
    env: &Env<'h>,
    dnf: &[(Vec<T<'h>>, Vec<T<'h>>)],
    vars: &[T<'h>],
    m: &mut Meter,
) -> WorkResult<Option<(Vec<T<'h>>, Vec<(Vec<Item<'h>>, Vec<T<'h>>)>)>> {
    let all: Vec<T<'h>> = dnf.iter().flat_map(|(e, n)| e.iter().chain(n.iter()).copied()).collect();
    let ring = env.ring(&all, vars, m)?;
    // only symbols: the listed variables and parameters
    let mut params = vec![];
    for &k in &ring.kernels {
        if vars.contains(&k) {
            continue;
        }
        match env.h().view(k) {
            View::Sym(s) if !matches!(env.h().sym_name(s).as_str(), "pi" | "e" | "im" | "inf" | "euler_gamma" | "catalan") => params.push(k),
            _ => return Ok(None),
        }
    }
    if vars.len() < 2 && params.is_empty() {
        return Ok(None);
    }
    // highest first: the listed variables from the last, then the parameters from the last
    let mut ord: Vec<usize> = vars.iter().rev().map(|&v| ring.var(v).expect("a listed variable")).collect();
    ord.extend(params.iter().rev().map(|&p| ring.var(p).expect("a parameter")));
    let tri = Tri { env, ring: &ring, ord };
    let rc = env.rconv();
    let mut cases: Vec<Vec<Item<'h>>> = vec![];
    let mut checks = vec![];
    for (eqs, nes) in dnf {
        let mut e = vec![];
        let mut n = vec![];
        for &d in eqs {
            let Ok(f) = ratfun::read(&rc, d, &ring, m)? else { return Ok(None) };
            if f.num.nvars() != ring.nvars() {
                return Ok(None);
            }
            e.push(f.num);
            if !f.den.is_constant() {
                n.push(f.den);
            }
        }
        for &d in nes {
            let Ok(f) = ratfun::read(&rc, d, &ring, m)? else { return Ok(None) };
            n.push(f.num);
            if !f.den.is_constant() {
                n.push(f.den);
            }
        }
        let n: Vec<(Poly, bool)> = n.into_iter().map(|p| (p, true)).collect();
        let Some(cs) = tri.split(e, n, 0, m)? else { return Ok(None) };
        for c in cs {
            checks.push((c.clone(), eqs.clone()));
            cases.push(c);
            if cases.len() > MAX_CASES {
                return Ok(None);
            }
        }
    }
    let mut terms: Vec<(usize, T<'h>)> = vec![];
    let nparams = params.len();
    for mut c in cases {
        c.sort_by_key(Item::level);
        // a case that solves no listed variable (the equations vanish identically in them): the conditions on the
        // parameters from the highest down, as the coefficients from the lowest power up
        if c.iter().all(|it| !matches!(it, Item::Eq(l, ..) if *l >= nparams)) {
            c.sort_by_key(|it| match it {
                Item::Eq(l, ..) => nparams - l,
                _ => usize::MAX,
            });
        }
        let first = c.first().map_or(usize::MAX, Item::level);
        let conj: Vec<T<'h>> = c.iter().map(|it| item_term(env, it)).collect();
        let t = match conj.len() {
            0 => env.sym(names::TRUE),
            1 => conj[0],
            _ => env.call("and", &conj),
        };
        terms.push((first, t));
    }
    // cases with conditions on lower variables first (stable)
    terms.sort_by_key(|x| x.0);
    let mut out: Vec<T<'h>> = vec![];
    for (_, t) in terms {
        if !out.contains(&t) {
            out.push(t);
        }
    }
    if out.iter().any(|&t| env.is_sym(t, names::TRUE)) {
        out = vec![env.sym(names::TRUE)];
    }
    Ok(Some((out, checks)))
}

fn item_term<'h>(env: &Env<'h>, it: &Item<'h>) -> T<'h> {
    match it {
        Item::Eq(_, x, rs) => {
            let eqs: Vec<T<'h>> = rs.iter().map(|&r| env.call("equal", &[*x, r])).collect();
            if eqs.len() == 1 {
                eqs[0]
            } else {
                env.call("or", &eqs)
            }
        }
        Item::Lc(_, p) | Item::Ne(p) => env.call("unequal", &[*p, env.int(0)]),
    }
}

impl<'h> Tri<'_, 'h> {
    /// The lex comparison of exponent vectors.
    fn cmp(&self, a: &[u32], b: &[u32]) -> Ordering {
        for &v in &self.ord {
            match a[v].cmp(&b[v]) {
                Ordering::Equal => {}
                o => return o,
            }
        }
        Ordering::Equal
    }

    /// The leading term.
    fn lead(&self, p: &Poly) -> (Vec<u32>, Z) {
        let mut best: Option<(&[u32], &Z)> = None;
        for (e, c) in p.terms() {
            if best.is_none_or(|b| self.cmp(e, b.0) == Ordering::Greater) {
                best = Some((e, c));
            }
        }
        let (e, c) = best.expect("a nonzero polynomial");
        (e.to_vec(), c.clone())
    }

    /// The position in `ord` of the highest variable of p (`None` for a constant).
    fn main_var(&self, p: &Poly) -> Option<usize> {
        let d = p.degrees();
        self.ord.iter().position(|&v| d[v] > 0)
    }

    /// The level of p: 0 for the lowest variable.
    fn level(&self, p: &Poly) -> usize {
        self.main_var(p).map_or(0, |i| self.ord.len() - 1 - i)
    }

    /// The normal form of p modulo the basis (fraction free), primitive with a positive leading coefficient.
    fn normal_form(&self, p: &Poly, g: &[Poly], m: &mut Meter) -> WorkResult<Poly> {
        let nv = p.nvars();
        let leads: Vec<(Vec<u32>, Z)> = g.iter().map(|q| self.lead(q)).collect();
        let mut rest = p.clone();
        let mut out = Poly::zero(nv);
        let mut steps = 0;
        while !rest.is_zero() {
            m.step(rest.len() as u64 + 1)?;
            steps += 1;
            if steps > 10_000 {
                return Err(Halt::Memory);
            }
            let (e, c) = self.lead(&rest);
            match leads.iter().position(|(le, _)| le.iter().zip(&e).all(|(a, b)| a <= b)) {
                Some(i) => {
                    let (le, lc) = &leads[i];
                    let mono: Vec<u32> = e.iter().zip(le).map(|(a, b)| a - b).collect();
                    let gg = lc.gcd(&c);
                    let (s, t) = (lc.div_exact(&gg), c.div_exact(&gg));
                    rest = rest.scale(&s).sub(&g[i].mul_term(&mono, &t));
                    out = out.scale(&s);
                }
                None => {
                    let t = Poly::monomial(&e, c);
                    out = out.add(&t);
                    rest = rest.sub(&t);
                }
            }
        }
        Ok(self.positive(&out))
    }

    /// Primitive with a positive leading coefficient.
    fn positive(&self, p: &Poly) -> Poly {
        if p.is_zero() {
            return p.clone();
        }
        let (_, q) = p.primitive();
        if self.lead(&q).1.is_neg() {
            q.neg()
        } else {
            q
        }
    }

    /// The product of the distinct irreducible factors (primitive, positive leading coefficient).
    fn square_free(&self, p: &Poly, m: &mut Meter) -> WorkResult<Poly> {
        let f = factor(p, true, m)?;
        let mut q = Poly::constant(p.nvars(), Z::ONE);
        for (g, _) in &f.factors {
            q = q.mul(g, m)?;
        }
        Ok(self.positive(&q))
    }

    fn term(&self, p: &Poly, m: &mut Meter) -> WorkResult<T<'h>> {
        self.env.conv().int_poly_term(p, self.ring, m)
    }

    /// `num/den` as a term: a rational multiple of a polynomial when `den` is a constant.
    fn quotient(&self, num: &Poly, den: &Poly, m: &mut Meter) -> WorkResult<T<'h>> {
        let env = self.env;
        let (num, den) = if den.lc().is_neg() { (num.neg(), den.neg()) } else { (num.clone(), den.clone()) };
        if let Some(d) = den.as_constant() {
            let c = num.content();
            let g = c.gcd(&d);
            let (n2, d2) = (num.div_exact_scalar(&g), d.div_exact(&g));
            let t = self.term(&n2, m)?;
            return Ok(if d2.is_one() { t } else { env.ar.mul(&[env.ar.div(env.int(1), env.ar.integer(&d2.to_int())), t]) });
        }
        let rc = env.rconv();
        ratfun::to_term(&rc, &ratfun::RatFun { num, den }, self.ring, m)
    }

    /// The distinct roots in `x_v` of an irreducible factor: rational functions (degree 1), the quadratic formula
    /// (with the square part of a numeric discriminant taken out), pure powers with roots of unity, the solver's
    /// formulas for numeric coefficients, root objects for higher degrees with parameters.
    fn roots(&self, f: &Poly, v: usize, m: &mut Meter) -> WorkResult<Vec<T<'h>>> {
        let env = self.env;
        let a = Alg { env };
        let d = f.degree(v).unwrap_or(0) as usize;
        let cs = f.coeffs_in(v);
        let coef = |k: u32| cs.iter().find(|(e, _)| *e == k).map_or_else(|| Poly::zero(f.nvars()), |x| x.1.clone());
        if d == 1 {
            return Ok(vec![self.quotient(&coef(0).neg(), &coef(1), m)?]);
        }
        let numeric = cs.iter().all(|(_, c)| c.is_constant());
        // a pure power c_d x^d + c_0
        if cs.len() == 2 && (d == 2 || d == 3) {
            let base = self.quotient(&coef(0).neg(), &coef(d as u32), m)?;
            if numeric && d == 2 {
                return Ok(poly_roots(env, f, self.ring, v, m)?.unwrap_or_default());
            }
            let r = a.pow(base, 1, d as i64);
            return Ok(if d == 2 {
                vec![a.neg(r), r]
            } else {
                vec![r, a.mul(&[a.pow(a.int(-1), 4, 3), r]), a.mul(&[a.pow(a.int(-1), 2, 3), r])]
            });
        }
        if d == 2 && numeric {
            // (-b -+ s sqrt(r)) / (2a), D = s^2 r, divided by the common factor of b, s and 2a
            let (qa, qb, qc) = (coef(2).lc(), coef(1).as_constant().unwrap_or(Z::ZERO), coef(0).as_constant().unwrap_or(Z::ZERO));
            let disc = qb.mul(&qb).sub(&Z::from_i128(4).mul(&qa).mul(&qc));
            let (sq, rest) = square_part(&disc);
            let two_a = Z::from_i128(2).mul(&qa);
            let g = qb.abs().gcd(&sq).gcd(&two_a);
            let g = if g.is_zero() { Z::ONE } else { g };
            let int = |z: &Z| env.ar.integer(&z.to_int());
            let rad = a.mul(&[int(&sq.div_exact(&g)), a.sqrt(int(&rest))]);
            let nb = int(&qb.neg().div_exact(&g));
            let den = env.ar.div(env.int(1), int(&two_a.div_exact(&g)));
            return Ok(vec![a.mul(&[den, a.sub(nb, rad)]), a.mul(&[den, a.add(&[nb, rad])])]);
        }
        if d >= 3 && !numeric {
            return root_objects(env, f, self.ring, v, m);
        }
        Ok(poly_roots(env, f, self.ring, v, m)?.unwrap_or_default())
    }

    /// The cases of `eqs == 0 and n != 0 for n in nes` (the flag: an input condition, else a leading coefficient);
    /// `None` beyond the bounds.
    #[allow(clippy::type_complexity)]
    fn split(&self, eqs: Vec<Poly>, nes: Vec<(Poly, bool)>, depth: usize, m: &mut Meter) -> WorkResult<Option<Vec<Vec<Item<'h>>>>> {
        if depth > MAX_DEPTH {
            return Ok(None);
        }
        m.step(1)?;
        let mut sq = vec![];
        for p in eqs.into_iter().filter(|p| !p.is_zero()) {
            sq.push(self.square_free(&p, m)?);
        }
        let eqs = sq;
        let g = if eqs.is_empty() { vec![] } else { lex_basis(&eqs, self.ring, &self.ord, m)? };
        let g: Vec<Poly> = g.into_iter().filter(|p| !p.is_zero()).map(|p| self.positive(&p)).collect();
        if g.iter().any(Poly::is_constant) {
            return Ok(Some(vec![]));
        }
        let mut ns = vec![];
        for (n, orig) in &nes {
            let r = self.normal_form(n, &g, m)?;
            let r = if r.is_zero() || r.is_constant() { r } else { self.square_free(&r, m)? };
            if r.is_zero() {
                return Ok(Some(vec![]));
            }
            if !r.is_constant() && !ns.iter().any(|(q, _): &(Poly, bool)| *q == r) {
                ns.push((r, *orig));
            }
        }
        if g.is_empty() {
            let mut items = vec![];
            for (n, orig) in &ns {
                let t = self.term(n, m)?;
                items.push(if *orig { Item::Ne(t) } else { Item::Lc(self.level(n), t) });
            }
            return Ok(Some(vec![items]));
        }
        // the highest leading variable and its element of least degree
        let k = g.iter().filter_map(|p| self.main_var(p)).min().expect("a nonconstant element");
        let v = self.ord[k];
        let gk = g
            .iter()
            .filter(|p| self.main_var(p) == Some(k))
            .min_by_key(|p| (p.degree(v).unwrap_or(0), p.len()))
            .expect("an element in x_k");
        let lc = {
            let d = gk.degree(v).unwrap_or(0);
            let cs = gk.coeffs_in(v);
            let c = cs.iter().find(|(e, _)| *e == d).map(|x| x.1.clone()).expect("the leading coefficient");
            let c = self.positive(&c);
            if c.is_constant() {
                c
            } else {
                self.square_free(&c, m)?
            }
        };
        let low: Vec<Poly> = g.iter().filter(|p| self.main_var(p).is_some_and(|j| j > k)).cloned().collect();
        let (hi_ns, mut low_ns): (Vec<_>, Vec<_>) = ns.iter().cloned().partition(|(n, _)| self.main_var(n).is_some_and(|j| j <= k));
        let x = self.ring.kernels[v];
        let mut roots: Vec<T<'h>> = vec![];
        for (f, _) in factor(gk, false, m)?.factors {
            if f.degree(v).unwrap_or(0) == 0 {
                continue;
            }
            for r in self.roots(&f, v, m)? {
                if !roots.contains(&r) {
                    roots.push(r);
                }
            }
        }
        if roots.is_empty() {
            return Ok(None);
        }
        // numeric roots in increasing order (real part, then imaginary)
        let vals: Option<Vec<(f64, f64)>> = roots.iter().map(|&r| self.env.machine(r)).collect();
        if let Some(vals) = vals {
            let mut idx: Vec<usize> = (0..roots.len()).collect();
            idx.sort_by(|&a, &b| vals[a].partial_cmp(&vals[b]).unwrap_or(Ordering::Equal));
            roots = idx.into_iter().map(|i| roots[i]).collect();
        }
        let level = self.ord.len() - 1 - k;
        let mut out = vec![];
        let general = !lc.is_constant();
        if general {
            low_ns.push((lc.clone(), false));
        }
        let Some(sub) = self.split(low, low_ns, depth + 1, m)? else { return Ok(None) };
        for mut c in sub {
            // a lower variable with one numeric value: substituted (x == 1 and y == 1, not y == 2 - x)
            let rules: Vec<(T<'h>, T<'h>)> = c
                .iter()
                .filter_map(|it| match it {
                    Item::Eq(_, y, rs) if rs.len() == 1 && self.env.ar.num(rs[0]).is_some() => Some((*y, rs[0])),
                    _ => None,
                })
                .collect();
            let mut rs = roots.clone();
            if !rules.is_empty() {
                for r in rs.iter_mut() {
                    *r = self.env.subst(*r, &rules, m)?;
                }
            }
            c.push(Item::Eq(level, x, rs));
            for (n, orig) in &hi_ns {
                let t = self.term(n, m)?;
                c.push(if *orig { Item::Ne(t) } else { Item::Lc(self.level(n), t) });
            }
            out.push(c);
        }
        if general {
            let mut e2 = g.clone();
            e2.push(lc);
            let Some(sub) = self.split(e2, nes, depth + 1, m)? else { return Ok(None) };
            out.extend(sub);
        }
        if out.len() > MAX_CASES {
            return Ok(None);
        }
        Ok(Some(out))
    }
}

/// A case holds at a point: its free variables at rationals, each solved variable at its first root; the
/// equations of its disjunct must vanish there (numerically, 30 digits). A point where a condition of the case
/// fails (a leading coefficient or an input `!=` vanishes) is not a test.
fn check<'h>(cx: &mut NativeCx<'_, '_, 'h>, env: &Env<'h>, items: &[Item<'h>], eqs: &[T<'h>], vars: &[T<'h>]) -> Res<bool> {
    let h = cx.heap();
    // the symbols of the case and the equations
    let mut syms: Vec<T<'h>> = vec![];
    let mut stack: Vec<T<'h>> = eqs.to_vec();
    for it in items {
        match it {
            Item::Eq(_, x, rs) => {
                stack.push(*x);
                stack.extend(rs.iter().copied());
            }
            Item::Lc(_, p) | Item::Ne(p) => stack.push(*p),
        }
    }
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) => {
                let n = h.sym_name(s);
                if !matches!(n.as_str(), "pi" | "e" | "im" | "true" | "false" | "fn" | "root") && !n.starts_with('&') && !syms.contains(&u) {
                    syms.push(u);
                }
            }
            View::App { head, args } => {
                // the bound variable of a root object is not free
                if let View::Sym(s) = h.view(head) {
                    if h.sym_name(s).as_str() == "fn" {
                        continue;
                    }
                }
                stack.extend(args.iter().copied());
            }
            _ => {}
        }
    }
    let solved: Vec<T<'h>> = items.iter().filter_map(|it| if let Item::Eq(_, x, _) = it { Some(*x) } else { None }).collect();
    let samples = [(3, 7), (-5, 11), (13, 17), (-7, 19), (11, 23), (17, 29), (-19, 31), (23, 37)];
    let mut rules: Vec<T<'h>> = vec![];
    let mut j = 0;
    for &s in &syms {
        if solved.contains(&s) {
            continue;
        }
        let (p, q) = samples[j % samples.len()];
        j += 1;
        rules.push(h.call(h.sym(names::RULE), &[s, env.ar.div(env.int(p), env.int(q))]));
    }
    // solved variables in level order (lowest first)
    let mut eq_items: Vec<&Item<'h>> = items.iter().filter(|it| matches!(it, Item::Eq(..))).collect();
    eq_items.sort_by_key(|it| it.level());
    let _ = vars;
    for it in eq_items {
        let Item::Eq(_, x, rs) = it else { continue };
        let rl = cx.list(&rules);
        let v = cx.eval(h.call(h.sym("rewrite"), &[rs[0], rl]))?;
        let nv = cx.eval(h.call(h.sym("n"), &[v, h.int(40)]))?;
        rules.push(h.call(h.sym(names::RULE), &[*x, nv]));
    }
    let rl = cx.list(&rules);
    for it in items {
        if let Item::Lc(_, p) | Item::Ne(p) = it {
            let v = cx.eval(h.call(h.sym("rewrite"), &[*p, rl]))?;
            if small(cx, v)? {
                return Ok(true);
            }
        }
    }
    for &e in eqs {
        let v = cx.eval(h.call(h.sym("rewrite"), &[e, rl]))?;
        if !small(cx, v)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Is a numeric value below 10^-15 in absolute value (undecided values count as small)?
fn small<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>) -> Res<bool> {
    let h = cx.heap();
    let a = cx.eval(h.call(h.sym("abs"), &[v]))?;
    let a = cx.eval(h.call(h.sym("n"), &[a, h.int(30)]))?;
    Ok(match h.view(a) {
        View::Real(x) => x < 1e-15,
        View::Int(k) => k == 0,
        _ => {
            let lt = cx.eval(h.call(h.sym("less"), &[a, h.call(h.sym(names::POW), &[h.int(10), h.int(-15)])]))?;
            !cx.is_false(lt)
        }
    })
}

/// `n = s^2 r` with the square factors of primes below 1000 in `s` (`r` keeps the sign).
fn square_part(n: &Z) -> (Z, Z) {
    if n.is_zero() {
        return (Z::ONE, Z::ZERO);
    }
    let mut s = Z::ONE;
    let mut r = n.abs();
    let mut p = 2i64;
    while p < 1000 {
        let pp = Z::from_i128((p * p) as i128);
        while r.divisible(&pp) {
            r = r.div_exact(&pp);
            s = s.mul(&Z::from_i128(p as i128));
        }
        p += if p == 2 { 1 } else { 2 };
    }
    (s, if n.is_neg() { r.neg() } else { r })
}
