//! `r_solve(eqs, a, n)` and `r_solve_value(eqs, a(n), n)` for one sequence `a`.
//!
//! The recurrence `sum_s c_s(n) a(n + s) = f(n)` (shifts any integers) is read with its initial values `a(k) == v`:
//!
//! - **first order** `a(n + 1) = r(n) a(n) + g(n)`: `a(n) = c(1) h(n) + h(n) sum_{k=0}^{n-1} g(k) / h(k + 1)` with
//!   `h(n) = prod_{k=1}^{n-1} r(k)` in closed form (`product`), the sum by `sum` (Gosper, geometric, ...);
//! - **constant coefficients**: the roots of the characteristic polynomial (the calculus crate's solver), and
//!   for the right side `f = sum b^n P(n)` a particular solution `b^n u(n)` with `u` a rational solution of
//!   `sum c_s b^s u(n + s) = P(n)` (Abramov), which covers the resonant cases without a special rule;
//! - **polynomial coefficients**: the hypergeometric solutions by Petkovšek's Hyper (a full basis is required) and
//!   the particular solution as above.
//!
//! Initial values fix the constants (`solve`). Every solution is substituted back into the recurrence at several
//! integers, with the constants and parameters given rational values, before it is returned. With a target
//! `a(5)` (or a list of such), `r_solve_value` iterates the recurrence from the initial values instead, which
//! also covers nonlinear recurrences.
use crate::cx::Sx;
use crate::{recur, some, sumfn, verify, w};
use mc_api::{names, Store, View, T};
use mc_eval::Res;
use mc_poly::work::Meter;

/// The highest order solved.
const MAX_ORDER: i64 = 12;
/// The most steps of an iteration.
const MAX_STEPS: i64 = 10_000;

/// The equations, flattened from lists and `and`.
fn flatten<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
    let h = sx.h();
    match h.view(t) {
        View::App { head, args } if head == sx.sym(names::LIST) || head == sx.sym("and") => {
            for &a in args {
                flatten(sx, a, out);
            }
        }
        _ => out.push(t),
    }
}

/// Every occurrence `a(arg)` in `t`: `(arg, term)`.
fn occurrences<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>, a: T<'h>) -> Vec<(T<'h>, T<'h>)> {
    let h = sx.h();
    let mut out = vec![];
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if head == a {
                if let &[x] = args {
                    if !out.iter().any(|o: &(T<'h>, T<'h>)| o.1 == u) {
                        out.push((x, u));
                    }
                    continue;
                }
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    out
}

/// `t` with the subterms equal to a `from` replaced.
fn replace<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>, pairs: &[(T<'h>, T<'h>)]) -> T<'h> {
    if let Some(&(_, to)) = pairs.iter().find(|p| p.0 == t) {
        return to;
    }
    let h = sx.h();
    match h.view(t) {
        View::App { head, args } => {
            let new: Vec<T<'h>> = args.iter().map(|&x| replace(sx, x, pairs)).collect();
            if new.as_slice() == args {
                t
            } else {
                h.app(head, &new)
            }
        }
        _ => t,
    }
}

/// The parsed problem.
struct Problem<'h> {
    /// `l - r` of the recurrence
    diff: T<'h>,
    /// the shifts and their occurrences `(s, a(n + s))`
    shifts: Vec<(i64, T<'h>)>,
    /// initial values `(k, v)`
    inits: Vec<(T<'h>, T<'h>)>,
}

fn problem<'h>(sx: &mut Sx<'_, '_, '_, 'h>, eqs: T<'h>, a: T<'h>, n: T<'h>) -> Res<Option<Problem<'h>>> {
    let h = sx.h();
    let mut all = vec![];
    flatten(sx, eqs, &mut all);
    let mut rec = None;
    let mut inits = vec![];
    for e in all {
        let View::App { head, args: &[l, r] } = h.view(e) else { return Ok(None) };
        if head != sx.sym("equal") {
            return Ok(None);
        }
        let occ = occurrences(sx, e, a);
        if occ.is_empty() {
            return Ok(None);
        }
        if occ.iter().any(|o| !sx.free(o.0, n)) {
            if rec.is_some() {
                return Ok(None);
            }
            rec = Some((l, r, occ));
        } else {
            let (k, v) = match (h.view(l), h.view(r)) {
                (View::App { head: hl, args: &[k] }, _) if hl == a && sx.free(r, a) => (k, r),
                (_, View::App { head: hr, args: &[k] }) if hr == a && sx.free(l, a) => (k, l),
                _ => return Ok(None),
            };
            inits.push((k, v));
        }
    }
    let (l, r, occ) = some!(rec);
    let mut shifts = vec![];
    for (arg, term) in occ {
        let s = sx.sub(arg, n)?;
        let s = sx.ev("expand", &[s])?;
        let s = some!(sx.as_int(s));
        shifts.push((s, term));
    }
    let diff = sx.sub(l, r)?;
    Ok(Some(Problem { diff, shifts, inits }))
}

/// What is asked for.
#[derive(Clone, Copy)]
enum Target<'h> {
    /// `a(n)`
    Applied,
    /// `a`
    Function,
    /// `a(3)` or `[a(3), a(4)]`
    Points(T<'h>),
}

/// `r_solve` (`value` false) or `r_solve_value`.
pub fn r_solve<'h>(
    sx: &mut Sx<'_, '_, '_, 'h>,
    eqs: T<'h>,
    target: T<'h>,
    n: T<'h>,
    value: bool,
    gen: Option<T<'h>>,
    m: &mut Meter,
) -> Res<Option<T<'h>>> {
    let h = sx.h();
    if !matches!(h.view(n), View::Sym(_)) {
        return Ok(None);
    }
    // the sequence and the target
    let (a, tgt) = match h.view(target) {
        View::Sym(_) => (target, Target::Function),
        View::App { head, args: &[x] } if matches!(h.view(head), View::Sym(_)) => {
            if x == n {
                (head, Target::Applied)
            } else if value && sx.free(x, n) {
                (head, Target::Points(target))
            } else {
                return Ok(None);
            }
        }
        View::App { head, args } if value && head == sx.sym(names::LIST) && !args.is_empty() => {
            let View::App { head: a0, .. } = h.view(args[0]) else { return Ok(None) };
            (a0, Target::Points(target))
        }
        _ => return Ok(None),
    };
    let p = some!(problem(sx, eqs, a, n)?);
    if let Target::Points(pts) = tgt {
        return iterate(sx, &p, a, n, pts);
    }
    let sols = some!(general(sx, &p, a, n, gen, m)?);
    let mut out = vec![];
    for s in sols {
        let v = match tgt {
            Target::Function => sx.call("fn", &[sx.list(&[n]), s]),
            _ => s,
        };
        out.push(v);
    }
    if value {
        return Ok(Some(match out.as_slice() {
            [s] => *s,
            _ => sx.list(&out),
        }));
    }
    let lhs = match tgt {
        Target::Function => a,
        _ => sx.call_term(a, &[n]),
    };
    let rules: Vec<T<'h>> = out.iter().map(|&s| sx.list(&[sx.rule(lhs, s)])).collect();
    Ok(Some(sx.list(&rules)))
}

impl<'h> Sx<'_, '_, '_, 'h> {
    /// `f(args)` for a head term.
    fn call_term(&self, f: T<'h>, args: &[T<'h>]) -> T<'h> {
        self.h().app(f, args)
    }
}

/// `recurrence_table(eqs, a, [n, nmin, nmax(, step)])`: the values by iteration.
pub fn table<'h>(sx: &mut Sx<'_, '_, '_, 'h>, eqs: T<'h>, a: T<'h>, spec: T<'h>) -> Res<Option<T<'h>>> {
    let h = sx.h();
    if !matches!(h.view(a), View::Sym(_)) {
        return Ok(None);
    }
    let s = some!(sx.as_list(spec)).to_vec();
    // `[n, nmax]` starts at the first initial value (the recurrence's own start, 1 when there is none)
    let first_init = match s.first() {
        Some(&n0) if matches!(h.view(n0), View::Sym(_)) => match problem(sx, eqs, a, n0)? {
            Some(p) => p.inits.iter().filter_map(|(k, _)| sx.as_int(*k)).min().unwrap_or(1),
            None => 1,
        },
        _ => 1,
    };
    let (n, lo, hi, st) = match s.as_slice() {
        [n, b] => (*n, Some(first_init), sx.as_int(*b), 1),
        [n, a0, b] => (*n, sx.as_int(*a0), sx.as_int(*b), 1),
        [n, a0, b, c] => (*n, sx.as_int(*a0), sx.as_int(*b), some!(sx.as_int(*c))),
        _ => return Ok(None),
    };
    let (lo, hi) = (some!(lo), some!(hi));
    if st < 1 || hi < lo || hi - lo > MAX_STEPS || !matches!(h.view(n), View::Sym(_)) {
        return Ok(None);
    }
    let p = some!(problem(sx, eqs, a, n)?);
    let pts: Vec<T<'h>> = (lo..=hi).step_by(st as usize).map(|j| h.app(a, &[sx.int(j)])).collect();
    let pl = sx.list(&pts);
    iterate(sx, &p, a, n, pl)
}

/// `a(k)` at integer points by running the recurrence from the initial values.
fn iterate<'h>(sx: &mut Sx<'_, '_, '_, 'h>, p: &Problem<'h>, a: T<'h>, n: T<'h>, pts: T<'h>) -> Res<Option<T<'h>>> {
    let lo = p.shifts.iter().map(|s| s.0).min().unwrap_or(0);
    let hi = p.shifts.iter().map(|s| s.0).max().unwrap_or(0);
    let order = hi - lo;
    if order <= 0 || order > MAX_ORDER {
        return Ok(None);
    }
    // the recurrence solved for the highest shift: y_hi in terms of y_lo..y_{hi-1}
    let ys: Vec<T<'h>> = (0..=order).map(|i| sx.sym(&format!("rsolve$y{i}"))).collect();
    let pairs: Vec<(T<'h>, T<'h>)> = p.shifts.iter().map(|&(s, t)| (t, ys[(s - lo) as usize])).collect();
    let d = replace(sx, p.diff, &pairs);
    let d = sx.eval(d)?;
    let eq = sx.call("equal", &[d, sx.int(0)]);
    let top = ys[order as usize];
    let sol = sx.ev("solve", &[eq, top])?;
    let sols = some!(sx.as_list(sol)).to_vec();
    let [one] = sols.as_slice() else { return Ok(None) };
    let rule = some!(sx.as_list(*one).and_then(|r| r.first().copied()));
    let View::App { args: &[_, step], .. } = sx.h().view(rule) else { return Ok(None) };
    // the initial values: consecutive integers
    let mut known: Vec<(i64, T<'h>)> = vec![];
    for &(k, v) in &p.inits {
        known.push((some!(sx.as_int(k)), v));
    }
    known.sort_by_key(|x| x.0);
    let first = some!(known.first()).0;
    let mut vals: Vec<T<'h>> = vec![];
    for (i, &(k, v)) in known.iter().enumerate() {
        if k != first + i as i64 {
            return Ok(None);
        }
        vals.push(v);
    }
    if (vals.len() as i64) < order {
        return Ok(None);
    }
    let want: Vec<T<'h>> = match sx.as_list(pts) {
        Some(xs) => xs.to_vec(),
        None => vec![pts],
    };
    let mut idx = vec![];
    for &w in &want {
        let View::App { head, args: &[x] } = sx.h().view(w) else { return Ok(None) };
        if head != a {
            return Ok(None);
        }
        let x = some!(sx.as_int(x));
        if x < first || x - first > MAX_STEPS {
            return Ok(None);
        }
        idx.push(x);
    }
    let last = *idx.iter().max().unwrap_or(&first);
    let mut j = first + vals.len() as i64;
    while j <= last {
        sx.cx.tick(1)?;
        // n with n + hi = j
        let nv = j - hi;
        let mut rules = vec![sx.rule(n, sx.int(nv))];
        for i in 0..order {
            let at = (j - order + i - first) as usize;
            rules.push(sx.rule(ys[i as usize], vals[at]));
        }
        let v = sx.subst_all(step, &rules)?;
        if sx.bad(v) || sx.cost(v) > 100_000 {
            return Ok(None);
        }
        vals.push(v);
        j += 1;
    }
    let out: Vec<T<'h>> = idx.iter().map(|&x| vals[(x - first) as usize]).collect();
    Ok(Some(match sx.as_list(pts) {
        Some(_) => sx.list(&out),
        None => out[0],
    }))
}

/// The general solution with the initial values applied.
fn general<'h>(sx: &mut Sx<'_, '_, '_, 'h>, p: &Problem<'h>, a: T<'h>, n: T<'h>, gen: Option<T<'h>>, m: &mut Meter) -> Res<Option<Vec<T<'h>>>> {
    let lo = p.shifts.iter().map(|s| s.0).min().unwrap_or(0);
    let hi = p.shifts.iter().map(|s| s.0).max().unwrap_or(0);
    let order = hi - lo;
    if order <= 0 || order > MAX_ORDER {
        return Ok(None);
    }
    // linear: diff = sum c_s y_s + rest
    let ys: Vec<T<'h>> = (0..=order).map(|i| sx.sym(&format!("rsolve$y{i}"))).collect();
    let pairs: Vec<(T<'h>, T<'h>)> = p.shifts.iter().map(|&(s, t)| (t, ys[(s - lo) as usize])).collect();
    let d = replace(sx, p.diff, &pairs);
    let d = sx.eval(d)?;
    let mut cs = vec![];
    for &y in &ys {
        let c = sx.ev("d", &[d, y])?;
        if ys.iter().any(|&w| !sx.free(c, w)) {
            return Ok(None);
        }
        cs.push(c);
    }
    let zeros: Vec<T<'h>> = ys.iter().map(|&y| sx.rule(y, sx.int(0))).collect();
    let rest = sx.subst_all(d, &zeros)?;
    // linearity check
    let mut lin = vec![rest];
    for (c, y) in cs.iter().zip(&ys) {
        lin.push(sx.mul(&[*c, *y])?);
    }
    let lsum = sx.add(&lin)?;
    let chk = sx.sub(d, lsum)?;
    let chk = sx.ev("expand", &[chk])?;
    if chk != sx.int(0) {
        return Ok(None);
    }
    // re-index so that the lowest shift is 0: p_i(n) = c_{lo+i}(n - lo), f(n) = -rest(n - lo)
    let nl = sx.plus(n, -lo)?;
    let mut ps = vec![];
    for c in cs {
        ps.push(sx.subst(c, n, nl)?);
    }
    let rest = sx.subst(rest, n, nl)?;
    let f = sx.mul(&[sx.int(-1), rest])?;
    let f = sx.ev("expand", &[f])?;
    if ps[0] == sx.int(0) || ps[order as usize] == sx.int(0) {
        return Ok(None);
    }
    let constant = |i: usize, sx: &Sx<'_, '_, '_, 'h>| -> T<'h> {
        match gen {
            Some(g) => sx.h().app(g, &[sx.int(i as i64)]),
            None => sx.call("c", &[sx.int(i as i64)]),
        }
    };
    let homogeneous = sx.int(0) == f;
    // the homogeneous solutions
    let basis: Vec<T<'h>> = if order == 1 {
        // h(n) = prod_{k=1}^{n-1} r(k)
        let r = sx.div(ps[0], ps[1])?;
        let r = sx.mul(&[sx.int(-1), r])?;
        let kk = sx.sym("rsolve$k");
        let rk = sx.subst(r, n, kk)?;
        let nm1 = sx.plus(n, -1)?;
        let spec = sx.list(&[kk, sx.int(1), nm1]);
        let hn = sx.ev("product", &[rk, spec])?;
        if sx.contains_any(hn, &["product"]) || sx.bad(hn) {
            return Ok(None);
        }
        vec![hn]
    } else if ps.iter().all(|&c| sx.free(c, n)) {
        // the calculus crate's characteristic roots
        let mut terms = vec![];
        for (i, &c) in ps.iter().enumerate() {
            let ni = sx.plus(n, i as i64)?;
            terms.push(sx.mul(&[c, sx.call_term(a, &[ni])])?);
        }
        let lhs = sx.add(&terms)?;
        let eq = sx.call("equal", &[lhs, sx.int(0)]);
        let an = sx.call_term(a, &[n]);
        let marker = sx.rule(sx.sym("rsolve$inner"), sx.sym(names::TRUE));
        let opts = sx.call("options", &[marker]);
        let r = sx.ev("r_solve", &[eq, an, n, opts])?;
        let s = some!(sx.as_list(r).and_then(|l| l.first().copied()));
        let s = some!(sx.as_list(s).and_then(|l| l.first().copied()));
        let View::App { args: &[_, g], .. } = sx.h().view(s) else { return Ok(None) };
        // split the combination into its basis: the coefficient of each c(i)
        let mut b = vec![];
        for i in 1..=order as usize {
            let ci = sx.call("c", &[sx.int(i as i64)]);
            let bi = sx.ev("d", &[g, ci])?;
            b.push(bi);
        }
        b
    } else {
        some!(hyper_basis(sx, &ps, n, order as usize, m)?)
    };
    if basis.len() != order as usize {
        return Ok(None);
    }
    // a particular solution
    let part = if homogeneous {
        sx.int(0)
    } else if order == 1 {
        // h(n) sum_{k=0}^{n-1} g(k) / h(k + 1), g = f / p_1
        let g = sx.div(f, ps[1])?;
        let kk = sx.sym("rsolve$j");
        let gk = sx.subst(g, n, kk)?;
        let k1 = sx.plus(kk, 1)?;
        let hk1 = sx.subst(basis[0], n, k1)?;
        let term = sx.div(gk, hk1)?;
        let nm1 = sx.plus(n, -1)?;
        let spec = sx.list(&[kk, sx.int(0), nm1]);
        let term = sx.ev("simplify", &[term])?;
        let s = sx.ev("sum", &[term, spec])?;
        sx.trace("r_solve particular sum", s);
        if sx.contains_any(s, &["sum"]) {
            return Ok(None);
        }
        sx.mul(&[basis[0], s])?
    } else {
        some!(particular(sx, &ps, f, n, m)?)
    };
    let part = sumfn::tidy(sx, part)?;
    let mut terms = vec![part];
    for (i, &b) in basis.iter().enumerate() {
        terms.push(sx.mul(&[constant(i + 1, sx), b])?);
    }
    let g = sx.add(&terms)?;
    sx.trace("r_solve general", g);
    // the constants from the initial values
    let consts: Vec<T<'h>> = (1..=order as usize).map(|i| constant(i, sx)).collect();
    let sols = if p.inits.is_empty() {
        vec![g]
    } else {
        let mut eqs = vec![];
        for &(k, v) in &p.inits {
            let at = sx.subst(g, n, k)?;
            eqs.push(sx.call("equal", &[at, v]));
        }
        let el = sx.list(&eqs);
        let cl = sx.list(&consts);
        let s = sx.ev("solve", &[el, cl])?;
        let items = some!(sx.as_list(s)).to_vec();
        let mut out = vec![];
        for rs in items {
            let rules = some!(sx.as_list(rs)).to_vec();
            let v = sx.subst_all(g, &rules)?;
            out.push(sumfn::tidy(sx, v)?);
        }
        out
    };
    // every solution satisfies the recurrence at a few integers
    for &s in &sols {
        sx.trace("r_solve check", s);
        if !check(sx, &ps, f, s, n, &consts)? {
            sx.trace("r_solve check failed", s);
            return Ok(None);
        }
    }
    Ok(Some(sols))
}

/// A basis of hypergeometric solutions by Hyper (all `order` of them, else `None`).
fn hyper_basis<'h>(sx: &mut Sx<'_, '_, '_, 'h>, ps: &[T<'h>], n: T<'h>, order: usize, m: &mut Meter) -> Res<Option<Vec<T<'h>>>> {
    let zv = sx.sym("hyper$z");
    let ring = w!(sx, sx.ring(ps, &[n, zv], m));
    if !sx.clean(&ring, &[n]) {
        return Ok(None);
    }
    let nvar = some!(ring.var(n));
    // polynomial coefficients: clear the denominators
    let mut rfs = vec![];
    for &c in ps {
        rfs.push(some!(w!(sx, sx.read(c, &ring, m))));
    }
    let mut den = crate::rf::zc(ring.nvars(), 1);
    for r in &rfs {
        den = w!(sx, crate::rf::lcm(&den, &r.den, m));
    }
    let mut coefs = vec![];
    for r in &rfs {
        let q = w!(sx, crate::rf::div(&den, &r.den, m));
        coefs.push(w!(sx, r.num.mul(&q, m)));
    }
    let sols = some!(sumfn::solve_recurrence(sx, &coefs, nvar, &ring, n, m)?);
    if sols.len() < order {
        return Ok(None);
    }
    Ok(Some(sols[..order].to_vec()))
}

/// A particular solution of `sum_i p_i(n) y(n + i) = f(n)`: `f` split into terms `b^n P(n)` with `P` rational, each
/// solved by `b^n u(n)` with `u` a rational solution of `sum_i p_i(n) b^i u(n + i) = P(n)`.
fn particular<'h>(sx: &mut Sx<'_, '_, '_, 'h>, ps: &[T<'h>], f: T<'h>, n: T<'h>, m: &mut Meter) -> Res<Option<T<'h>>> {
    let h = sx.h();
    let parts: Vec<T<'h>> = match h.view(f) {
        View::App { head, args } if head == sx.sym(names::ADD) => args.to_vec(),
        _ => vec![f],
    };
    // group by the base b
    let mut groups: Vec<(T<'h>, Vec<T<'h>>)> = vec![];
    for t in parts {
        let r = some!(crate::hyper::ratio(sx, t, n)?);
        let r = sx.ev("simplify", &[r])?;
        let ring = w!(sx, sx.ring(&[r], &[n], m));
        if !sx.clean(&ring, &[n]) {
            return Ok(None);
        }
        let rf = some!(w!(sx, sx.read(r, &ring, m)));
        let v = some!(ring.var(n));
        let (dn, dd) = (crate::rf::deg(&rf.num, v), crate::rf::deg(&rf.den, v));
        if dn != dd {
            return Ok(None);
        }
        let ln = w!(sx, sx.pterm(&crate::rf::lcoeff(&rf.num, v), &ring, m));
        let ld = w!(sx, sx.pterm(&crate::rf::lcoeff(&rf.den, v), &ring, m));
        let b = sx.div(ln, ld)?;
        let bn = sx.pow(b, n)?;
        let pp = sx.div(t, bn)?;
        let pp = sx.ev("simplify", &[pp])?;
        if !crate::hyper::rational_in(sx, pp, n)? {
            return Ok(None);
        }
        match groups.iter_mut().find(|g| g.0 == b) {
            Some(g) => g.1.push(pp),
            None => groups.push((b, vec![pp])),
        }
    }
    let mut out = vec![];
    for (b, pts) in groups {
        let pp = sx.add(&pts)?;
        let mut qs = vec![];
        for (i, &c) in ps.iter().enumerate() {
            let bi = sx.pow(b, sx.int(i as i64))?;
            qs.push(sx.mul(&[c, bi])?);
        }
        let mut terms = qs.clone();
        terms.push(pp);
        let zv = sx.sym("hyper$z");
        let ring = w!(sx, sx.ring(&terms, &[n, zv], m));
        if !sx.clean(&ring, &[n]) {
            return Ok(None);
        }
        let nvar = some!(ring.var(n));
        let mut rfs = vec![];
        for &c in &qs {
            rfs.push(some!(w!(sx, sx.read(c, &ring, m))));
        }
        let mut den = crate::rf::zc(ring.nvars(), 1);
        for r in &rfs {
            den = w!(sx, crate::rf::lcm(&den, &r.den, m));
        }
        let mut coefs = vec![];
        for r in &rfs {
            let q = w!(sx, crate::rf::div(&den, &r.den, m));
            coefs.push(w!(sx, r.num.mul(&q, m)));
        }
        let prf = some!(w!(sx, sx.read(pp, &ring, m)));
        let prf = w!(sx, prf.mul_poly(&den, m));
        let (part, _) = some!(w!(sx, recur::rational_solutions(&coefs, Some(&prf), nvar, m)));
        let u = some!(part);
        let ut = w!(sx, sx.term(&u, &ring, m));
        let bn = sx.pow(b, n)?;
        out.push(sx.mul(&[bn, ut])?);
    }
    Ok(Some(sx.add(&out)?))
}

/// Does `s(n)` satisfy `sum_i p_i(n) s(n + i) = f(n)` at a few integers (constants and parameters rational)?
fn check<'h>(sx: &mut Sx<'_, '_, '_, 'h>, ps: &[T<'h>], f: T<'h>, s: T<'h>, n: T<'h>, consts: &[T<'h>]) -> Res<bool> {
    let mut rules = vec![];
    for (i, &c) in consts.iter().enumerate() {
        rules.push(sx.rule(c, sx.int(2 + i as i64)));
    }
    let s = sx.subst_all(s, &rules)?;
    let mut params = verify::symbols(sx, s, &[n]);
    for &c in ps.iter().chain(std::iter::once(&f)) {
        for x in verify::symbols(sx, c, &[n]) {
            if !params.contains(&x) {
                params.push(x);
            }
        }
    }
    let mut agreed = 0;
    for nv in 2..12i64 {
        sx.cx.tick(1)?;
        let mut terms = vec![];
        let mut ok = true;
        for (i, &c) in ps.iter().enumerate() {
            let ci = sx.subst(c, n, sx.int(nv))?;
            let si = sx.subst(s, n, sx.int(nv + i as i64))?;
            if sx.bad(ci) || sx.bad(si) {
                ok = false;
                break;
            }
            terms.push(sx.mul(&[ci, si])?);
        }
        if !ok {
            continue;
        }
        let lhs = sx.add(&terms)?;
        let fv = sx.subst(f, n, sx.int(nv))?;
        let lhs = verify::at_params(sx, lhs, &params, nv as usize)?;
        let fv = verify::at_params(sx, fv, &params, nv as usize)?;
        match verify::equal(sx, lhs, fv)? {
            Some(true) => agreed += 1,
            Some(false) => return Ok(false),
            None => {}
        }
        if agreed >= 4 {
            break;
        }
    }
    Ok(agreed >= 2)
}
