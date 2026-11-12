//! Quantifier elimination over the complex numbers for equations and `!=` ([`eliminate`], one variable at a
//! time when each disjunct has at most one equation in it), and closed sentences with one block of quantifiers:
//! `exists x: phi` holds iff some disjunct of `phi`'s disjunctive normal form, a conjunction of `p_i == 0` and
//! `q_j != 0`, has a common complex zero, which by the Nullstellensatz is `1 not in (p_1, ..., 1 - z q_1 ... q_k)`
//! (Rabinowitsch), decided by a Groebner basis; `for_all x: phi` is `not exists x: not phi`.
use crate::cad::Problem;
use crate::formula::{Fm, PAtom, Quant, Rel};
use crate::output::Writer;
use crate::poly::coeffs;
use crate::front::Front;
use crate::natives::metered;
use mc_api::{names, Store, T};
use mc_eval::{NativeCx, Res};
use mc_poly::groebner::{groebner, GPoly, MonomialOrder};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The most disjuncts of a normal form.
const MAX_DISJUNCTS: usize = 64;

/// `resolve(formula, complexes)`: the elimination below, then the closed sentences above; `None` when neither
/// applies.
pub fn resolve<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, f: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let r = metered(cx, stop, |m| -> WorkResult<Option<T<'h>>> {
        let fr = Front::new(h);
        let Ok(parsed) = fr.problem(f, &[], m)? else { return Ok(None) };
        let p = parsed.prob;
        let bool_term = |b: bool| h.sym_term(h.sym(if b { names::TRUE } else { names::FALSE }));
        if p.nv == 0 {
            // a Boolean sentence: its value
            return Ok(match crate::vs::simplify(p.matrix.clone()) {
                Fm::Const(b) => Some(bool_term(b)),
                _ => None,
            });
        }
        if !p.atoms.iter().all(|a| matches!(a.rel, Rel::Eq | Rel::Ne) && a.den.is_constant()) {
            // a sentence with order relations: over the real and imaginary parts (decision CAD.20)
            if p.nfree == 0 {
                if let Some(q) = realify(&p, m)? {
                    if let Ok(o) = crate::cad::decompose(&q, &crate::cad::Limits::default(), &mut |_, _| {}, m)? {
                        if let Some(t) = o.truth {
                            return Ok(Some(bool_term(t)));
                        }
                    }
                }
            }
            return Ok(None);
        }
        // innermost variables eliminated one by one while they occur in at most one equation of each disjunct
        if let Some(dis) = eliminate(&p, m)? {
            let w = Writer::new(h, &parsed.vars[..p.nfree], m)?;
            return Ok(Some(dnf_term(&w, dis, m)?));
        }
        if p.nfree > 0 || p.quants.is_empty() || !p.quants.iter().all(|&q| q == p.quants[0]) {
            return Ok(None);
        }
        let neg = p.quants[0] == Quant::ForAll;
        let Some(dnf) = dnf(&p.matrix, &p.atoms, neg) else { return Ok(None) };
        // the disjuncts in parallel (roadmap item 16A), until one is satisfiable
        let found = crate::par::until(dnf.len(), m, &|r: &WorkResult<bool>| !matches!(r, Ok(false)), &|i, m| satisfiable(p.nv, &dnf[i].0, &dnf[i].1, m))?;
        let any = found.last() == Some(&true);
        Ok(Some(bool_term(any != neg)))
    })?;
    Ok(r.flatten())
}

/// A sentence over the complex numbers as one over the reals: each variable `x_k = a_k + i b_k` (both bound by
/// x_k's quantifier), `p == 0` as `re p == 0 and im p == 0`, `p != 0` as its negation, and an order relation
/// `p < 0` as `im p == 0 and re p < 0` (a comparison holds only between real values: that is how an inequality
/// over the complex numbers is read). `None` for denominators.
fn realify(p: &Problem, m: &mut Meter) -> WorkResult<Option<Problem>> {
    if p.atoms.iter().any(|a| !a.den.is_constant()) {
        return Ok(None);
    }
    let n = p.nv;
    let nv = 2 * n;
    let one = Poly::constant(nv, Z::ONE);
    let zero = Poly::zero(nv);
    // (re, im) of x_k^j, by k, computed on demand
    let mut pows: Vec<Vec<(Poly, Poly)>> = (0..n).map(|k| vec![(one.clone(), zero.clone()), (Poly::var(nv, 2 * k), Poly::var(nv, 2 * k + 1))]).collect();
    let mut atoms: Vec<PAtom> = vec![];
    let mut map: Vec<Fm> = vec![];
    let unit = |nv: usize| Poly::constant(nv, Z::ONE);
    for a in &p.atoms {
        let (mut re, mut im) = (zero.clone(), zero.clone());
        for (e, c) in a.num.terms() {
            m.step(1)?;
            let (mut tr, mut ti) = (Poly::constant(nv, c.clone()), zero.clone());
            for (k, &ek) in e.iter().enumerate() {
                if ek == 0 {
                    continue;
                }
                while pows[k].len() <= ek as usize {
                    let (lr, li) = pows[k].last().expect("a power").clone();
                    let (xr, xi) = pows[k][1].clone();
                    let nr = lr.mul(&xr, m)?.sub(&li.mul(&xi, m)?);
                    let ni = lr.mul(&xi, m)?.add(&li.mul(&xr, m)?);
                    pows[k].push((nr, ni));
                }
                let (pr, pi) = &pows[k][ek as usize];
                let nr = tr.mul(pr, m)?.sub(&ti.mul(pi, m)?);
                let ni = tr.mul(pi, m)?.add(&ti.mul(pr, m)?);
                (tr, ti) = (nr, ni);
            }
            re = re.add(&tr);
            im = im.add(&ti);
        }
        let d = Poly::constant(nv, a.den.lc());
        let mut push = |num: Poly, rel: Rel| -> Fm {
            atoms.push(PAtom { num, den: unit(nv), rel });
            Fm::Atom(atoms.len() - 1)
        };
        // the sign of a constant denominator multiplies the real part
        let re = re.mul(&d, m)?;
        let f = match a.rel {
            Rel::Eq => Fm::And(vec![push(re, Rel::Eq), push(im, Rel::Eq)]),
            Rel::Ne => Fm::Or(vec![push(re, Rel::Ne), push(im, Rel::Ne)]),
            r => Fm::And(vec![push(im, Rel::Eq), push(re, r)]),
        };
        map.push(f);
    }
    fn subst(f: &Fm, map: &[Fm]) -> Fm {
        match f {
            Fm::Const(b) => Fm::Const(*b),
            Fm::Atom(i) => map[*i].clone(),
            Fm::And(v) => Fm::And(v.iter().map(|g| subst(g, map)).collect()),
            Fm::Or(v) => Fm::Or(v.iter().map(|g| subst(g, map)).collect()),
            Fm::Not(g) => Fm::Not(Box::new(subst(g, map))),
        }
    }
    let quants: Vec<Quant> = p.quants.iter().flat_map(|&q| [q, q]).collect();
    Ok(Some(Problem { nv, nfree: 2 * p.nfree, quants, atoms, matrix: subst(&p.matrix, &map) }))
}

/// `exists x` of one disjunct (equations, `!=`): a disjunction of conditions on the other variables; `None` when
/// the disjunct is outside the method.
fn eliminate_one(eqs: Vec<Poly>, nes: Vec<Poly>, x: usize, nv: usize, m: &mut Meter) -> WorkResult<Option<Dnf>> {
    let (ex, e0): (Vec<Poly>, Vec<Poly>) = eqs.into_iter().partition(|e| e.degree(x).unwrap_or(0) > 0);
    let (nx, n0): (Vec<Poly>, Vec<Poly>) = nes.into_iter().partition(|e| e.degree(x).unwrap_or(0) > 0);
    let mut part: Dnf = vec![(e0, n0)];
    let and_with = |part: Dnf, alts: Dnf| -> Dnf {
        let mut r = vec![];
        for (e1, n1) in &part {
            for (e2, n2) in &alts {
                let mut e = e1.clone();
                e.extend(e2.iter().cloned());
                let mut n = n1.clone();
                n.extend(n2.iter().cloned());
                r.push((e, n));
            }
        }
        r
    };
    match (ex.as_slice(), nx.is_empty()) {
        ([], _) => {
            for qn in &nx {
                let cs: Vec<Poly> = coeffs(qn, x);
                if !satisfiable(nv, &cs, &[], m)? {
                    continue;
                }
                let alts: Dnf = coeffs(qn, x).into_iter().filter(|c| !c.is_zero()).map(|c| (vec![], vec![c])).collect();
                part = and_with(part, alts);
            }
        }
        ([e], true) => {
            let cs = coeffs(e, x);
            // the coefficients of the positive powers without a common zero: always solvable
            let mut alts: Dnf = if !satisfiable(nv, &cs[1..], &[], m)? {
                vec![(vec![], vec![])]
            } else {
                cs[1..].iter().filter(|c| !c.is_zero()).map(|c| (vec![], vec![c.clone()])).collect()
            };
            if alts.iter().any(|(e, n)| !e.is_empty() || !n.is_empty()) {
                alts.push((vec![cs[0].clone()], vec![]));
            }
            part = and_with(part, alts);
        }
        _ => {
            // several equations, or equations and `!=` (decision CAD.18)
            let Some(alts) = exists_x(ex, nx, x, nv, 0, m)? else { return Ok(None) };
            part = and_with(part, alts);
        }
    }
    Ok(Some(part))
}

/// A disjunction of conjunctions of `p == 0` (first) and `q != 0` (second).
type Dnf = Vec<(Vec<Poly>, Vec<Poly>)>;

/// Elimination of the bound variables over the complex numbers, innermost first (decision CAD.13): `exists x`
/// of a disjunct whose conjuncts in `x` are `q_j != 0` only holds iff no `q_j` vanishes identically in `x` (some
/// coefficient of each is nonzero); with exactly one equation `p == 0` and no `!=` in `x`, iff `p` is not a
/// nonzero constant in `x` (a coefficient of a positive power is nonzero, or the constant term is zero).
/// `for_all x: phi` is `not exists x: not phi`. `None` when some disjunct is outside these two forms. The result
/// is a formula in the free variables (in `nfree` variables).
fn eliminate(p: &Problem, m: &mut Meter) -> WorkResult<Option<Dnf>> {
    let Some(mut cur) = dnf(&p.matrix, &p.atoms, false) else { return Ok(None) };
    let mut nv = p.nv;
    for i in (0..p.quants.len()).rev() {
        m.step(1)?;
        let x = nv - 1;
        let q = p.quants[i];
        let phi = if q == Quant::ForAll { negate(&cur, nv) } else { Some(cur.clone()) };
        let Some(phi) = phi else { return Ok(None) };
        // the disjuncts eliminated in parallel (roadmap item 16A), in order until one is outside the method
        let parts = crate::par::until(phi.len(), m, &|r: &WorkResult<Option<Dnf>>| !matches!(r, Ok(Some(_))), &|i, m| {
            eliminate_one(phi[i].0.clone(), phi[i].1.clone(), x, nv, m)
        })?;
        let mut out: Dnf = vec![];
        for part in parts {
            let Some(part) = part else { return Ok(None) };
            out.extend(part);
            if out.len() > MAX_DISJUNCTS {
                return Ok(None);
            }
        }
        let out = clean(out, nv);
        cur = if q == Quant::ForAll {
            match negate(&out, nv) {
                Some(n) => n,
                None => return Ok(None),
            }
        } else {
            out
        };
        // x is gone
        cur = cur
            .into_iter()
            .map(|(e, n)| (e.iter().map(|p| drop_last(p, nv)).collect(), n.iter().map(|p| drop_last(p, nv)).collect()))
            .collect();
        nv -= 1;
        cur = clean(cur, nv);
    }
    tidy(cur, nv, m).map(Some)
}

/// The pseudo-remainder of `a` by `b` in `x` (`b` of positive degree in x).
fn prem(a: &Poly, b: &Poly, x: usize, m: &mut Meter) -> WorkResult<Poly> {
    let nv = a.nvars();
    let db = b.degree(x).unwrap_or(0);
    let lb = coeffs(b, x).pop().expect("a coefficient");
    let xv = Poly::var(nv, x);
    let mut r = a.clone();
    loop {
        m.step(r.len() as u64 + 1)?;
        let dr = match r.degree(x) {
            Some(d) if !r.is_zero() && d >= db => d,
            _ => return Ok(r),
        };
        let lr = coeffs(&r, x).pop().expect("a coefficient");
        let shift = xv.pow((dr - db) as u64, m)?;
        r = r.mul(&lb, m)?.sub(&lr.mul(&shift, m)?.mul(b, m)?);
    }
}

/// `exists x: p_1 == 0 and ... and q_1 != 0 and ...` (every polynomial with x) over the complex numbers, as a
/// normal form in the other variables: on the leading coefficient `c` of the equation `p` of least degree,
/// `c != 0` (the other equations replaced by their pseudo-remainders by `p`; with `p` alone, a root of `p` that is
/// not a root of `q = q_1 q_2 ...` exists iff `p` does not divide `q^deg p`, iff the pseudo-remainder of
/// `q^deg p` by `p` is not identically zero in x) or `c == 0` (p without its leading term). This is the
/// comprehensive splitting of Weispfenning (1992) and Kapur, Sun and Wang (2010) for one variable. `None` beyond
/// the bounds.
fn exists_x(eqs: Vec<Poly>, nes: Vec<Poly>, x: usize, nv: usize, depth: usize, m: &mut Meter) -> WorkResult<Option<Dnf>> {
    if depth > 12 {
        return Ok(None);
    }
    m.step(1)?;
    let eqs: Vec<Poly> = eqs.into_iter().filter(|p| !p.is_zero()).collect();
    let (ex, e0): (Vec<Poly>, Vec<Poly>) = eqs.into_iter().partition(|e| e.degree(x).unwrap_or(0) > 0);
    let with = |d: Dnf, e: &[Poly], n: &[Poly]| -> Dnf {
        d.into_iter()
            .map(|(mut a, mut b)| {
                a.extend(e.iter().cloned());
                b.extend(n.iter().cloned());
                (a, b)
            })
            .collect()
    };
    if ex.is_empty() {
        // `!=` only: each q not identically zero in x
        let mut acc: Dnf = vec![(e0.clone(), vec![])];
        for q in &nes {
            let alts: Vec<Poly> = coeffs(q, x).into_iter().filter(|c| !c.is_zero()).collect();
            let mut next = vec![];
            for (a, b) in &acc {
                for c in &alts {
                    let mut b2 = b.clone();
                    b2.push(c.clone());
                    next.push((a.clone(), b2));
                }
            }
            if next.len() > MAX_DISJUNCTS {
                return Ok(None);
            }
            acc = next;
        }
        return Ok(Some(acc));
    }
    let i = (0..ex.len()).min_by_key(|&i| (ex[i].degree(x).unwrap_or(0), ex[i].len())).expect("an equation");
    let p = ex[i].clone();
    let others: Vec<Poly> = ex.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, q)| q.clone()).collect();
    let mut cs = coeffs(&p, x);
    let c = cs.pop().expect("a leading coefficient");
    let mut out: Dnf = vec![];
    // c != 0
    let sub: Dnf = if !others.is_empty() {
        let mut eq2 = vec![p.clone()];
        for o in &others {
            eq2.push(prem(o, &p, x, m)?);
        }
        let Some(d) = exists_x(eq2, nes.clone(), x, nv, depth + 1, m)? else { return Ok(None) };
        d
    } else if nes.is_empty() {
        vec![(vec![], vec![])]
    } else {
        let d = p.degree(x).unwrap_or(0) as u64;
        let mut q = Poly::constant(nv, Z::ONE);
        for n in &nes {
            q = q.mul(n, m)?;
        }
        if q.degree(x).unwrap_or(0) as u64 * d > 64 {
            return Ok(None);
        }
        let r = prem(&q.pow(d, m)?, &p, x, m)?;
        coeffs(&r, x).into_iter().filter(|c| !c.is_zero()).map(|c| (vec![], vec![c])).collect()
    };
    let cn: Vec<Poly> = if c.is_constant() { vec![] } else { vec![c.clone()] };
    out.extend(with(sub, &e0, &cn));
    // c == 0
    if !c.is_constant() {
        let d = p.degree(x).unwrap_or(0);
        let lead = c.mul(&Poly::var(nv, x).pow(d as u64, m)?, m)?;
        let mut eq2 = others.clone();
        eq2.push(p.sub(&lead));
        let Some(sub) = exists_x(eq2, nes, x, nv, depth + 1, m)? else { return Ok(None) };
        let mut e = e0.clone();
        e.push(c);
        out.extend(with(sub, &e, &[]));
    }
    if out.len() > MAX_DISJUNCTS {
        return Ok(None);
    }
    Ok(Some(clean(out, nv)))
}

/// Each disjunct simplified: `!=` polynomials square free, factors of equations that a `!=` excludes removed,
/// inconsistent disjuncts (by a Groebner basis) dropped.
fn tidy(d: Dnf, nv: usize, m: &mut Meter) -> WorkResult<Dnf> {
    let mut out: Dnf = vec![];
    'next: for (eqs, nes) in clean(d, nv) {
        let mut nf: Vec<Poly> = vec![];
        let mut n2 = vec![];
        for q in &nes {
            let (_, fs) = crate::poly::factors(q, m)?;
            let mut sq = Poly::constant(nv, Z::ONE);
            for (f, _) in fs {
                let f = normal(&f);
                sq = sq.mul(&f, m)?;
                nf.push(f);
            }
            n2.push(sq);
        }
        let mut e2 = vec![];
        for e in &eqs {
            let (_, fs) = crate::poly::factors(e, m)?;
            let mut sq = Poly::constant(nv, Z::ONE);
            let mut any = false;
            for (f, _) in fs {
                let f = normal(&f);
                if !nf.contains(&f) {
                    sq = sq.mul(&f, m)?;
                    any = true;
                }
            }
            if !any {
                continue 'next;
            }
            e2.push(sq);
        }
        if !satisfiable(nv, &e2, &n2, m)? {
            continue;
        }
        out.push((e2, n2));
    }
    Ok(clean(out, nv))
}

/// Constant conditions decided, repeated ones removed.
fn clean(d: Dnf, nv: usize) -> Dnf {
    let mut out: Dnf = vec![];
    'next: for (eqs, nes) in d {
        let mut e2: Vec<Poly> = vec![];
        let mut n2: Vec<Poly> = vec![];
        for e in eqs {
            match e.as_constant() {
                Some(c) if c.is_zero() => {}
                Some(_) => continue 'next,
                None => {
                    let e = normal(&e);
                    if !e2.contains(&e) {
                        e2.push(e)
                    }
                }
            }
        }
        for q in nes {
            match q.as_constant() {
                Some(c) if c.is_zero() => continue 'next,
                Some(_) => {}
                None => {
                    let q = normal(&q);
                    if !n2.contains(&q) {
                        n2.push(q)
                    }
                }
            }
        }
        if e2.iter().any(|e| n2.contains(e)) {
            continue;
        }
        if e2.is_empty() && n2.is_empty() {
            return vec![(vec![], vec![])];
        }
        if !out.contains(&(e2.clone(), n2.clone())) {
            out.push((e2, n2));
        }
    }
    let _ = nv;
    out
}

/// A polynomial up to its constant factor (primitive, positive leading coefficient).
fn normal(p: &Poly) -> Poly {
    let (_, q) = p.primitive();
    if q.lc().is_neg() {
        q.neg()
    } else {
        q
    }
}

/// The negation of a normal form, as a normal form (bounded).
fn negate(d: &Dnf, nv: usize) -> Option<Dnf> {
    // not (or of ands) = and of (or of negated literals)
    let mut acc: Dnf = vec![(vec![], vec![])];
    for (eqs, nes) in d {
        let lits: Dnf = eqs.iter().map(|e| (vec![], vec![e.clone()])).chain(nes.iter().map(|q| (vec![q.clone()], vec![]))).collect();
        if lits.is_empty() {
            return Some(vec![]);
        }
        let mut next = vec![];
        for (e1, n1) in &acc {
            for (e2, n2) in &lits {
                let mut e = e1.clone();
                e.extend(e2.iter().cloned());
                let mut n = n1.clone();
                n.extend(n2.iter().cloned());
                next.push((e, n));
            }
        }
        if next.len() > MAX_DISJUNCTS {
            return None;
        }
        acc = clean(next, nv);
    }
    Some(acc)
}

/// Drop the last variable from a polynomial that does not contain it.
fn drop_last(p: &Poly, nv: usize) -> Poly {
    Poly::from_terms(nv - 1, p.terms().map(|(e, c)| (e[..nv - 1].to_vec(), c.clone())).collect())
}

/// A normal form as a term: equations before `!=`, disjuncts by their lowest variable.
fn dnf_term<'h>(w: &Writer<'h>, d: Dnf, m: &mut Meter) -> WorkResult<T<'h>> {
    let mut ors = vec![];
    let mut d = d;
    d.sort_by_key(|(e, n)| e.iter().chain(n.iter()).map(crate::poly::level).min().unwrap_or(0));
    for (eqs, nes) in d {
        let mut ands = vec![];
        for e in &eqs {
            ands.push(w.call("equal", &[w.poly(e, m)?, w.ar.int(0)]));
        }
        for q in &nes {
            ands.push(w.call("unequal", &[w.poly(q, m)?, w.ar.int(0)]));
        }
        ors.push(match ands.len() {
            0 => w.bool(true),
            1 => ands[0],
            _ => w.call("and", &ands),
        });
    }
    Ok(match ors.len() {
        0 => w.bool(false),
        1 => ors[0],
        _ => w.call("or", &ors),
    })
}

/// The disjunctive normal form of `f` (negated when `neg`) as (equations, nonzero polynomials); `None` when too
/// large.
fn dnf(f: &Fm, atoms: &[PAtom], neg: bool) -> Option<Vec<(Vec<Poly>, Vec<Poly>)>> {
    Some(match f {
        Fm::Const(b) => {
            if *b != neg {
                vec![(vec![], vec![])]
            } else {
                vec![]
            }
        }
        Fm::Atom(i) => {
            let a = &atoms[*i];
            let eq = (a.rel == Rel::Eq) != neg;
            if eq {
                vec![(vec![a.num.clone()], vec![])]
            } else {
                vec![(vec![], vec![a.num.clone()])]
            }
        }
        Fm::Not(g) => dnf(g, atoms, !neg)?,
        Fm::And(v) | Fm::Or(v) => {
            let and = matches!(f, Fm::And(_)) != neg;
            let parts: Vec<_> = v.iter().map(|g| dnf(g, atoms, neg)).collect::<Option<_>>()?;
            if and {
                let mut acc: Vec<(Vec<Poly>, Vec<Poly>)> = vec![(vec![], vec![])];
                for p in parts {
                    let mut next = vec![];
                    for (e1, n1) in &acc {
                        for (e2, n2) in &p {
                            let mut e = e1.clone();
                            e.extend(e2.iter().cloned());
                            let mut n = n1.clone();
                            n.extend(n2.iter().cloned());
                            next.push((e, n));
                        }
                    }
                    if next.len() > MAX_DISJUNCTS {
                        return None;
                    }
                    acc = next;
                }
                acc
            } else {
                let out: Vec<_> = parts.into_iter().flatten().collect();
                if out.len() > MAX_DISJUNCTS {
                    return None;
                }
                out
            }
        }
    })
}

/// Do the equations and inequations have a common complex solution?
fn satisfiable(nv: usize, eqs: &[Poly], nes: &[Poly], m: &mut Meter) -> WorkResult<bool> {
    // one more variable z for 1 - z prod(nes)
    let n = nv + 1;
    let lift = |p: &Poly| Poly::from_terms(n, p.terms().map(|(e, c)| (e.iter().copied().chain([0]).collect(), c.clone())).collect());
    let mut ps: Vec<Poly> = eqs.iter().map(lift).collect();
    if nes.iter().any(|q| q.is_zero()) {
        return Ok(false);
    }
    if !nes.is_empty() {
        let mut prod = Poly::constant(n, Z::ONE);
        for q in nes {
            prod = prod.mul(&lift(q), m)?;
        }
        let z = Poly::var(n, nv);
        ps.push(Poly::constant(n, Z::ONE).sub(&z.mul(&prod, m)?));
    }
    if ps.iter().any(|p| p.as_constant().is_some_and(|c| !c.is_zero())) {
        return Ok(false);
    }
    ps.retain(|p| !p.is_zero());
    if ps.is_empty() {
        return Ok(true);
    }
    let main: Vec<usize> = (0..n).collect();
    let gs: Vec<GPoly> = ps.iter().map(|p| GPoly::from_poly(p, &main, &[], MonomialOrder::DegRevLex)).collect();
    let b = groebner(&gs, MonomialOrder::DegRevLex, m)?;
    Ok(!b.iter().any(|g| g.is_constant()))
}
