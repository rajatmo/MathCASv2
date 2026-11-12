//! The numeric answers of `n_solve` for what is solved exactly first (`n(solve(...))`): the values made numbers
//! (the branches `product_log(k, z)` of the Lambert function evaluated here, by Halley's iteration from the series at
//! the branch point and the asymptotic series, Corless et al. 1996, sections 4 and 5), the generated parameters kept
//! as integers, families of solutions listed as solutions (`max_roots: inf` keeps them as families), and the order:
//! one unknown by real part then imaginary part, conjugate pairs of a system with the negative
//! imaginary part first (decision 11C.13).
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// A complex number in machine precision.
#[derive(Clone, Copy, Debug)]
struct C(f64, f64);

impl C {
    fn add(self, o: C) -> C {
        C(self.0 + o.0, self.1 + o.1)
    }
    fn sub(self, o: C) -> C {
        C(self.0 - o.0, self.1 - o.1)
    }
    fn mul(self, o: C) -> C {
        C(self.0 * o.0 - self.1 * o.1, self.0 * o.1 + self.1 * o.0)
    }
    fn div(self, o: C) -> C {
        let d = o.0 * o.0 + o.1 * o.1;
        C((self.0 * o.0 + self.1 * o.1) / d, (self.1 * o.0 - self.0 * o.1) / d)
    }
    fn exp(self) -> C {
        let r = self.0.exp();
        C(r * self.1.cos(), r * self.1.sin())
    }
    fn ln(self) -> C {
        C(self.0.hypot(self.1).ln(), self.1.atan2(self.0))
    }
    fn sqrt(self) -> C {
        let r = self.0.hypot(self.1);
        let a = ((r + self.0) / 2.0).max(0.0).sqrt();
        let b = ((r - self.0) / 2.0).max(0.0).sqrt();
        C(a, if self.1 < 0.0 { -b } else { b })
    }
    fn abs(self) -> f64 {
        self.0.hypot(self.1)
    }
}

/// W_k(z) in machine precision, `None` when the iteration does not settle.
fn lambert_w(k: i64, z: C) -> Option<C> {
    let e = std::f64::consts::E;
    let two_pi_k = C(0.0, 2.0 * std::f64::consts::PI * k as f64);
    // near the branch point -1/e: w = -1 +- p - p^2/3 with p = sqrt(2 (e z + 1))
    let p = C(2.0, 0.0).mul(C(e, 0.0).mul(z).add(C(1.0, 0.0))).sqrt();
    let near = p.abs() < 0.5;
    let mut w = if k == 0 && near {
        C(-1.0, 0.0).add(p).sub(p.mul(p).div(C(3.0, 0.0)))
    } else if k == -1 && near && z.1 == 0.0 {
        C(-1.0, 0.0).sub(p).sub(p.mul(p).div(C(3.0, 0.0)))
    } else if k == 0 && z.abs() < 1.0 {
        z
    } else {
        let l1 = z.ln().add(two_pi_k);
        l1.sub(l1.ln())
    };
    for _ in 0..100 {
        let ew = w.exp();
        let f = w.mul(ew).sub(z);
        let w1 = w.add(C(1.0, 0.0));
        let denom = ew.mul(w1).sub(w.add(C(2.0, 0.0)).mul(f).div(w1.mul(C(2.0, 0.0))));
        let step = f.div(denom);
        w = w.sub(step);
        if !(w.0.is_finite() && w.1.is_finite()) {
            return None;
        }
        if step.abs() <= 1e-15 * (1.0 + w.abs()) {
            // the real branches stay real
            if z.1 == 0.0 && (k == 0 && z.0 >= -1.0 / e || k == -1 && (-1.0 / e..0.0).contains(&z.0)) {
                w.1 = 0.0;
            }
            return Some(w);
        }
    }
    None
}

/// A machine number of a term, if it is one.
fn machine<'h>(h: &'h impl Store, t: T<'h>, cx: &NativeCx<'_, '_, 'h>) -> Option<C> {
    let _ = cx;
    match h.view(t) {
        View::Real(x) => Some(C(x, 0.0)),
        View::Int(k) => Some(C(k as f64, 0.0)),
        View::App { head, args: &[a, b] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s).eq_ignore_ascii_case(names::COMPLEX)) => {
            let re = match h.view(a) {
                View::Real(x) => x,
                View::Int(k) => k as f64,
                _ => return None,
            };
            let im = match h.view(b) {
                View::Real(x) => x,
                View::Int(k) => k as f64,
                _ => return None,
            };
            Some(C(re, im))
        }
        _ => None,
    }
}

/// `t` with `product_log(k, z)` (numbers k, z) evaluated and `gen(r)` (r an inexact integer) made `gen(k)`.
fn fix<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, gen: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if head == gen {
        if let [a] = args {
            if let View::Real(x) = h.view(*a) {
                if x.fract() == 0.0 && x.abs() < 1e15 {
                    return Ok(h.app(gen, &[h.int(x as i64)]));
                }
            }
        }
        return Ok(t);
    }
    let mut new = Vec::with_capacity(args.len());
    let mut changed = false;
    for &a in args {
        let b = fix(cx, a, gen)?;
        changed |= b != a;
        new.push(b);
    }
    let t = if changed { h.app(head, &new) } else { t };
    if head == h.sym_term(h.sym("product_log")) {
        if let View::App { args: &[k, z], .. } = h.view(t) {
            if let (Some(kc), Some(zc)) = (machine(h, k, cx), machine(h, z, cx)) {
                if kc.1 == 0.0 && kc.0.fract() == 0.0 {
                    if let Some(w) = lambert_w(kc.0 as i64, zc) {
                        let v = if w.1 == 0.0 {
                            h.real(w.0)
                        } else {
                            cx.eval(h.call(h.sym(names::ADD), &[h.real(w.0), h.call(h.sym(names::MUL), &[h.real(w.1), h.sym_term(h.sym("im"))])]))?
                        };
                        return Ok(v);
                    }
                }
            }
        }
    }
    Ok(if changed { cx.eval(t)? } else { t })
}

/// The value of rule `x -> v`.
fn rule_value<'h>(h: &'h impl Store, r: T<'h>) -> Option<(T<'h>, T<'h>)> {
    match h.view(r) {
        View::App { args: &[a, b], .. } => Some((a, b)),
        _ => None,
    }
}

/// The numeric answer of n_solve from the evaluated `n(solve(...))` (a list of rule lists, or anything else).
pub(crate) fn finish<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, gen: T<'h>, all_roots: bool, max_roots: Option<usize>) -> Res<T<'h>> {
    let h = cx.heap();
    let t = fix(cx, t, gen)?;
    let Some(sols) = cx.as_list(t).map(|s| s.to_vec()) else { return Ok(t) };
    let cond = h.sym_term(h.sym("conditional_expression"));
    let mut rows: Vec<Vec<(T<'h>, T<'h>)>> = vec![];
    for &s in &sols {
        let Some(rules) = cx.as_list(s) else { return Ok(t) };
        let Some(row) = rules.iter().map(|&r| rule_value(h, r)).collect::<Option<Vec<_>>>() else { return Ok(t) };
        rows.push(row);
    }
    let is_family = |v: T<'h>| matches!(h.view(v), View::App { head, .. } if head == cond);
    // families of one unknown: by their value at c = 0, then listed (unless all roots are asked for)
    if !rows.is_empty() && rows.iter().all(|r| r.len() == 1 && is_family(r[0].1)) {
        let x = rows[0][0].0;
        let mut fams: Vec<(f64, T<'h>)> = vec![];
        let mut real_period = true;
        for r in &rows {
            let View::App { args: &[v, _], .. } = h.view(r[0].1) else { return Ok(t) };
            let at0 = substitute_gen(cx, v, gen, 0)?;
            let at1 = substitute_gen(cx, v, gen, 1)?;
            let (m0, m1) = (machine(h, at0, cx), machine(h, at1, cx));
            if let (Some(a), Some(b)) = (m0, m1) {
                real_period &= (b.1 - a.1).abs() <= 1e-12 * (1.0 + a.1.abs());
            }
            let key = m0.map_or(f64::INFINITY, |c| c.0);
            fams.push((key, r[0].1));
        }
        fams.sort_by(|a, b| a.0.total_cmp(&b.0));
        if all_roots {
            let items: Vec<T<'h>> = fams.iter().map(|&(_, v)| cx.list(&[h.call(h.sym(names::RULE), &[x, v])])).collect();
            return Ok(cx.list(&items));
        }
        // the enumeration: per family c = 0, 1, -1, 2, ... (the second family -1 first), the families
        // alternating which comes first in each round, 16 roots
        let seq = |sign: i64, i: usize| -> i64 {
            if i == 0 {
                0
            } else {
                let n = i.div_ceil(2) as i64;
                if i % 2 == 1 { sign * n } else { -sign * n }
            }
        };
        let mut order: Vec<(usize, i64)> = vec![];
        if !real_period {
            // a period off the real line (exp(x) == 2): the principal members
            order = (0..fams.len()).map(|f| (f, 0)).collect();
        } else if fams.len() == 2 {
            for i in 0..8 {
                let (a, b) = ((0, seq(1, i)), (1, seq(-1, i)));
                if i % 2 == 0 {
                    order.push(a);
                    order.push(b);
                } else {
                    order.push(b);
                    order.push(a);
                }
            }
        } else {
            for i in 0..16 {
                for f in 0..fams.len() {
                    order.push((f, seq(1, i)));
                }
            }
            order.truncate(16.max(fams.len()));
        }
        let mut items = vec![];
        for (f, c) in order {
            let View::App { args: &[v, _], .. } = h.view(fams[f].1) else { return Ok(t) };
            let val = substitute_gen(cx, v, gen, c)?;
            let mut val = cx.eval(h.call(h.sym("n"), &[val]))?;
            if let Some(C(re, 0.0)) = machine(h, val, cx) {
                val = h.real(re);
            }
            items.push(cx.list(&[h.call(h.sym(names::RULE), &[x, val])]));
        }
        if let Some(k) = max_roots {
            items.truncate(k);
        }
        return Ok(cx.list(&items));
    }
    let numeric: Option<Vec<Vec<C>>> = rows.iter().map(|r| r.iter().map(|&(_, v)| machine(h, v, cx)).collect()).collect();
    let Some(vals) = numeric else { return Ok(t) };
    let mut idx: Vec<usize> = (0..rows.len()).collect();
    if rows.iter().all(|r| r.len() == 1) {
        idx.sort_by(|&a, &b| vals[a][0].0.total_cmp(&vals[b][0].0).then(vals[a][0].1.total_cmp(&vals[b][0].1)));
    } else {
        // a conjugate pair: the one whose first non-real value has a negative imaginary part first
        let mut i = 0;
        while i + 1 < idx.len() {
            let (a, b) = (&vals[idx[i]], &vals[idx[i + 1]]);
            let conj = a.iter().zip(b).all(|(p, q)| (p.0 - q.0).abs() <= 1e-9 * (1.0 + p.0.abs()) && (p.1 + q.1).abs() <= 1e-9 * (1.0 + p.1.abs()));
            if conj {
                if let Some(p) = a.iter().find(|p| p.1 != 0.0) {
                    if p.1 > 0.0 {
                        idx.swap(i, i + 1);
                    }
                }
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    let mut items: Vec<T<'h>> = idx.into_iter().map(|i| sols[i]).collect();
    if let Some(k) = max_roots {
        items.truncate(k);
    }
    Ok(cx.list(&items))
}

/// `v` with `gen(k)` (every k) replaced by the integer c.
fn substitute_gen<'h>(cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>, gen: T<'h>, c: i64) -> Res<T<'h>> {
    let h = cx.heap();
    fn walk<'h>(h: &'h mc_core::Heap, t: T<'h>, gen: T<'h>, c: T<'h>) -> T<'h> {
        match h.view(t) {
            View::App { head, .. } if head == gen => c,
            View::App { head, args } => {
                let new: Vec<T<'h>> = args.iter().map(|&a| walk(h, a, gen, c)).collect();
                if new.as_slice() == args {
                    t
                } else {
                    h.app(head, &new)
                }
            }
            _ => t,
        }
    }
    let w = walk(h, v, gen, h.int(c));
    cx.eval(w)
}

/// The roots of `f(x) == 0` beyond the ones found (`n_solve(eq, x, max_roots: k)` when the exact path gives fewer
/// than k, round 19 Q19-32): Newton's iteration in machine complex arithmetic (with a halved step while the residual
/// grows) from starting points on rings of radius 1/2 to 8 around the origin; a root is accepted when its residual
/// is below 10^-10 of the scale of f there and it differs from every root known. At most `want` new roots, in
/// increasing modulus (for a conjugate pair the one of positive imaginary part first). The derivative is `d(f, x)`.
pub(crate) fn more_roots<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, known: &[T<'h>], want: usize) -> Res<Vec<T<'h>>> {
    let h = cx.heap();
    let df = cx.eval(h.call(h.sym("d"), &[f, x]))?;
    let imu = h.sym_term(h.sym("im"));
    let at = |cx: &mut NativeCx<'_, '_, 'h>, g: T<'h>, z: C| -> Res<Option<C>> {
        let zt = h.call(h.sym(names::ADD), &[h.real(z.0), h.call(h.sym(names::MUL), &[h.real(z.1), imu])]);
        let v = cx.eval(h.call(h.sym("rewrite"), &[g, h.call(h.sym(names::RULE), &[x, zt])]))?;
        let v = cx.eval(h.call(h.sym("n"), &[v]))?;
        Ok(machine(h, v, cx).filter(|c| c.0.is_finite() && c.1.is_finite()))
    };
    let roots: Vec<C> = known.iter().filter_map(|&t| machine(h, t, cx)).collect();
    let mut found: Vec<C> = vec![];
    let near = |a: C, b: C| a.sub(b).abs() <= 1e-7 * (1.0 + a.abs());
    'seeds: for &r in &[0.5f64, 1.0, 2.0, 3.0, 5.0, 8.0] {
        for k in 0..12 {
            if found.len() >= 4 * want.max(1) {
                break 'seeds;
            }
            cx.tick(1)?;
            let t = std::f64::consts::PI * (2.0 * k as f64 + 0.5) / 12.0;
            let mut z = C(r * t.cos(), r * t.sin());
            let Some(mut fz) = at(cx, f, z)? else { continue };
            let mut ok = false;
            for _ in 0..60 {
                let Some(d) = at(cx, df, z)? else { break };
                if d.abs() == 0.0 {
                    break;
                }
                let step = fz.div(d);
                let mut lam = 1.0;
                let mut moved = false;
                for _ in 0..8 {
                    let w = z.sub(C(step.0 * lam, step.1 * lam));
                    if let Some(fw) = at(cx, f, w)? {
                        if fw.abs() < fz.abs() || fw.abs() == 0.0 {
                            z = w;
                            fz = fw;
                            moved = true;
                            break;
                        }
                    }
                    lam *= 0.5;
                }
                if !moved || z.abs() > 1e6 {
                    break;
                }
                if C(step.0 * lam, step.1 * lam).abs() <= 1e-14 * (1.0 + z.abs()) {
                    ok = true;
                    break;
                }
            }
            if !ok {
                continue;
            }
            // the residual against the size of the terms of f at z (relative to 1 + |f'| |z|)
            let Some(d) = at(cx, df, z)? else { continue };
            if fz.abs() > 1e-10 * (1.0 + d.abs() * z.abs()) {
                continue;
            }
            if z.1.abs() <= 1e-14 * z.abs() {
                z.1 = 0.0;
            }
            if roots.iter().chain(&found).any(|&w| near(w, z)) {
                continue;
            }
            found.push(z);
        }
    }
    found.sort_by(|a, b| a.abs().total_cmp(&b.abs()).then(b.1.total_cmp(&a.1)));
    found.truncate(want);
    Ok(found
        .into_iter()
        .map(|z| if z.1 == 0.0 { h.real(z.0) } else { h.call(h.sym(names::COMPLEX), &[h.real(z.0), h.real(z.1)]) })
        .collect())
}
