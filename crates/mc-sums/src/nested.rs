//! Nested harmonic sums: `sum(c s^k/k^r H(k - d), [k, lo, n])` where H is a multiple harmonic number (or a
//! harmonic number, or a product of them) of `k` or `k - 1`, in multiple harmonic numbers of `n`.
//!
//! With `S(n; (r1, s1), ..., (rm, sm)) = multiple_harmonic_number(n, [r1, ...], [s1, ...])`, the sum over
//! n >= n1 > ... > nm >= 1 of s1^n1/n1^r1 ... (the S-sums of Moch, Uwer and Weinzierl, J. Math. Phys. 43 (2002), and
//! Ablinger, Blümlein and Schneider, J. Math. Phys. 54 (2013)):
//!
//! - the definition: `sum_(k=1)^n s^k/k^r S(k - 1; w) = S(n; (r, s), w)`;
//! - a factor of `k` rather than `k - 1` by the first index: `S(k; (r1, s1), w) = S(k - 1; (r1, s1), w) + s1^k/k^r1
//!   S(k - 1; w)`;
//! - products at one argument by the quasi-shuffle (stuffle) product (Hoffman, J. Algebraic Combin. 11 (2000)):
//!   `S(n; a u) S(n; b v) = S(n; a (u * b v)) + S(n; b (a u * v)) + S(n; (a + b)(u * v))`, letters adding as
//!   `(r1 + r2, s1 s2)`;
//! - a summand in `k + j` is shifted to `k` first; Stirling numbers `s(k, 2) = (-1)^k (k - 1)! H(k - 1)` and
//!   `S(k, 2) = 2^(k - 1) - 1` (DLMF 26.8.2 and 26.8.42 with the first cases) are written out.
//!
//! The lower bound `lo >= 1` subtracts the value at `lo - 1` (a number). Every closed form is checked against the
//! terms added one by one at several points (`verify::check_sum`) before it is returned.
use crate::cx::Sx;
use crate::verify;
use mc_api::{names, Store, View, T};
use mc_eval::Res;

/// The most terms the expansion of a summand may have.
const MAX_TERMS: usize = 64;

/// One letter (r, s): the factor s^k/k^r.
type Letter<'h> = (T<'h>, T<'h>);

/// c times the product of the inner sums S(k - 1; w) (empty words are 1), times s^k/k^r.
struct Term<'h> {
    c: T<'h>,
    outer: Letter<'h>,
    inner: Vec<Vec<Letter<'h>>>,
}

/// `sum(f, [k, lo, hi])` as multiple harmonic numbers of `hi`, or `None`.
pub fn harmonic_sum<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>) -> Res<Option<T<'h>>> {
    let Some(l0) = sx.as_int(lo) else { return Ok(None) };
    if !sx.free(hi, k) || !sx.contains_any(f, &["harmonic_number", "multiple_harmonic_number", "alternating_harmonic_number", "stirling_s1", "stirling_s2"]) && !geometric_power(sx, f, k) {
        return Ok(None);
    }
    // Stirling numbers of order 2 written out; then the summand as a sum of terms, summed by `sum`
    if sx.contains_any(f, &["stirling_s1", "stirling_s2"]) {
        let Some(g) = stirling(sx, f, k)? else { return Ok(None) };
        let g = sx.ev("expand", &[g])?;
        if sx.contains_any(g, &["stirling_s1", "stirling_s2"]) {
            return Ok(None);
        }
        let spec = sx.list(&[k, lo, hi]);
        let s = sx.ev("sum", &[g, spec])?;
        return Ok((!sx.contains_any(s, &["sum"]) && verify::check_sum(sx, f, k, lo, hi, s)?).then_some(s));
    }
    if l0 < 1 {
        return Ok(None);
    }
    // a summand in k + j: shifted to k
    let (f, l0, top) = match shift_of(sx, f, k)? {
        Some(j) if j != 0 => {
            let kj = sx.plus(k, -j)?;
            (sx.subst(f, k, kj)?, l0 + j, sx.plus(hi, j)?)
        }
        _ => (f, l0, hi),
    };
    if l0 < 1 {
        return Ok(None);
    }
    let ex = sx.ev("expand", &[f])?;
    let h = sx.h();
    let parts: Vec<T<'h>> = match h.view(ex) {
        View::App { head, args } if head == sx.sym(names::ADD) => args.to_vec(),
        _ => vec![ex],
    };
    let mut terms = vec![];
    for p in parts {
        let Some(t) = read_term(sx, p, k)? else { return Ok(None) };
        terms.extend(t);
        if terms.len() > MAX_TERMS {
            return Ok(None);
        }
    }
    let mut out = vec![];
    for t in terms {
        // the product of the inner sums by the stuffle product
        let mut words: Vec<(T<'h>, Vec<Letter<'h>>)> = vec![(sx.int(1), vec![])];
        for w in &t.inner {
            let mut next = vec![];
            for (c, u) in &words {
                for (d, v) in stuffle(sx, u, w)? {
                    next.push((sx.mul(&[*c, d])?, v));
                }
            }
            words = next;
            if words.len() > MAX_TERMS {
                return Ok(None);
            }
        }
        for (c, w) in words {
            let mut full = vec![t.outer];
            full.extend(w);
            let upper = mhn(sx, top, &full)?;
            let lower = mhn(sx, sx.int(l0 - 1), &full)?;
            let d = sx.sub(upper, lower)?;
            out.push(sx.mul(&[t.c, c, d])?);
        }
    }
    // the parts' own sums spread into one (a part that is a sum stayed nested in the result)
    let mut flat = vec![];
    let mut stack = out;
    while let Some(t) = stack.pop() {
        match sx.h().view(t) {
            View::App { head, args } if head == sx.sym(names::ADD) => stack.extend_from_slice(args),
            // a number times a sum is spread over its terms (else equal multiples of one sum combine into a sum
            // nested in the result)
            View::App { head, args: &[c, u] } if head == sx.sym(names::MUL) && sx.args_of(u, names::ADD).is_some() && sx.free(c, k) && sx.ar.num(c).is_some() => {
                for &a in sx.args_of(u, names::ADD).unwrap_or(&[]) {
                    stack.push(sx.mul(&[c, a])?);
                }
            }
            _ => flat.push(t),
        }
    }
    let s = sx.add(&flat)?;
    if sx.contains_any(s, &["sum"]) || sx.bad(s) {
        return Ok(None);
    }
    Ok(verify::check_sum(sx, f, k, sx.int(l0), top, s)?.then_some(s))
}

/// Does the summand have a factor s^k with s free of k (a depth-one sum s^k/k^r)?
fn geometric_power<'h>(sx: &Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>) -> bool {
    let h = sx.h();
    let factors: Vec<T<'h>> = match h.view(f) {
        View::App { head, args } if head == sx.sym(names::MUL) => args.to_vec(),
        _ => vec![f],
    };
    factors.iter().any(|&u| matches!(h.view(u), View::App { head, args: &[b, e] } if head == sx.sym(names::POW) && sx.free(b, k) && !sx.free(e, k)))
}

/// The Stirling numbers s(k, 2) and S(k, 2) of the summation index written out.
fn stirling<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>) -> Res<Option<T<'h>>> {
    let two = sx.int(2);
    let s1 = sx.call("stirling_s1", &[k, two]);
    let s2 = sx.call("stirling_s2", &[k, two]);
    // s(k, 2) = (-1)^k (k - 1)! H(k - 1), S(k, 2) = 2^(k - 1) - 1 (k >= 1)
    let km1 = sx.plus(k, -1)?;
    let sign = sx.pow(sx.int(-1), k)?;
    let fact = sx.ev("factorial", &[km1])?;
    let hk = sx.ev("harmonic_number", &[km1])?;
    let v1 = sx.mul(&[sign, fact, hk])?;
    let p2 = sx.pow(two, km1)?;
    let v2 = sx.plus(p2, -1)?;
    let r1 = sx.rule(s1, v1);
    let r2 = sx.rule(s2, v2);
    let g = sx.subst_all(f, &[r1, r2])?;
    Ok((g != f).then_some(g))
}

/// The shift j when the powers of the summation index in the summand have the one base k + j; when no power of the
/// index gives one, the shift of the harmonic numbers of k + j (all of one j: harmonic_number(k + 1, 2)^2).
fn shift_of<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>) -> Res<Option<i64>> {
    let h = sx.h();
    let factors: Vec<T<'h>> = match h.view(f) {
        View::App { head, args } if head == sx.sym(names::MUL) => args.to_vec(),
        _ => vec![f],
    };
    let harmonic = ["harmonic_number", "multiple_harmonic_number", "alternating_harmonic_number"];
    let mut shift = None;
    let mut hshift = None;
    let mut hmixed = false;
    for u in factors {
        let base = match h.view(u) {
            View::App { head, args: &[b, e] } if head == sx.sym(names::POW) && sx.as_int(e).is_some() && !sx.free(b, k) => b,
            _ if u != k && sx.free(u, k) => continue,
            _ if u == k => k,
            _ => u,
        };
        if let View::App { head, args } = h.view(base) {
            if !args.is_empty() && harmonic.iter().any(|n| head == sx.sym(n)) {
                let d = sx.sub(args[0], k)?;
                match sx.as_int(d) {
                    Some(j) if hshift.is_none_or(|s| s == j) => hshift = Some(j),
                    _ => hmixed = true,
                }
                continue;
            }
        }
        let d = sx.sub(base, k)?;
        let Some(j) = sx.as_int(d) else { continue };
        if shift.is_some_and(|s| s != j) {
            return Ok(None);
        }
        shift = Some(j);
    }
    Ok(if shift.is_some() || hmixed { shift } else { hshift })
}

/// The word of an inner sum of `k - d` (d 0 or 1): the multiple harmonic number (or harmonic number) as letters
/// with a coefficient; `None` when `u` is not one.
fn inner_word<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>, k: T<'h>) -> Res<Option<(T<'h>, i64, Vec<Letter<'h>>)>> {
    let one = sx.int(1);
    let (arg, c, word) = if let Some(a) = sx.args_of(u, "harmonic_number") {
        let w = match a {
            [_] => vec![(one, one)],
            [_, r] => vec![(*r, one)],
            [_, r, s] => vec![(*r, *s)],
            _ => return Ok(None),
        };
        (a[0], one, w)
    } else if let Some(a) = sx.args_of(u, "alternating_harmonic_number") {
        // the sum of (-1)^(j + 1) s^j/j^r is -S(n; (r, -s))
        let m1 = sx.int(-1);
        let w = match a {
            [_] => vec![(one, m1)],
            [_, r] => vec![(*r, m1)],
            [_, r, s] => vec![(*r, sx.mul(&[m1, *s])?)],
            _ => return Ok(None),
        };
        (a[0], m1, w)
    } else if let Some(a) = sx.args_of(u, "multiple_harmonic_number") {
        let (rs, ss) = match a {
            [_, rs] => {
                let rs = sx.as_list(*rs).map(|x| x.to_vec());
                let ss = rs.as_ref().map(|r| vec![one; r.len()]);
                (rs, ss)
            }
            [_, rs, ss] => (sx.as_list(*rs).map(|x| x.to_vec()), sx.as_list(*ss).map(|x| x.to_vec())),
            _ => return Ok(None),
        };
        let (Some(rs), Some(ss)) = (rs, ss) else { return Ok(None) };
        if rs.len() != ss.len() || rs.is_empty() {
            return Ok(None);
        }
        (a[0], one, rs.into_iter().zip(ss).collect())
    } else {
        return Ok(None);
    };
    if word.iter().any(|&(r, s)| !sx.free(r, k) || !sx.free(s, k) || sx.as_int(r).is_none()) {
        return Ok(None);
    }
    let d = sx.sub(k, arg)?;
    match sx.as_int(d) {
        Some(d @ (0 | 1)) => Ok(Some((c, d, word))),
        _ => Ok(None),
    }
}

/// A product term of the summand as terms c s^k/k^r prod S(k - 1; w): `None` when a factor is of another kind.
fn read_term<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, k: T<'h>) -> Res<Option<Vec<Term<'h>>>> {
    let h = sx.h();
    let factors: Vec<T<'h>> = match h.view(t) {
        View::App { head, args } if head == sx.sym(names::MUL) => args.to_vec(),
        _ => vec![t],
    };
    let mut c = vec![];
    let mut r = sx.int(0);
    let mut s = sx.int(1);
    let mut inners: Vec<(i64, Vec<Letter<'h>>)> = vec![];
    for u in factors {
        if sx.free(u, k) {
            c.push(u);
            continue;
        }
        if u == k {
            r = sx.plus(r, -1)?;
            continue;
        }
        if let View::App { head, args: &[b, e] } = h.view(u) {
            if head == sx.sym(names::POW) {
                if b == k && sx.as_int(e).is_some() {
                    let ne = sx.mul(&[sx.int(-1), e])?;
                    r = sx.add(&[r, ne])?;
                    continue;
                }
                if sx.free(b, k) {
                    // b^(a k + e0) with an integer a: (b^a)^k b^e0
                    let e0 = sx.subst(e, k, sx.int(0))?;
                    let ek = sx.sub(e, e0)?;
                    let a = sx.div(ek, k)?;
                    let a = sx.ev("expand", &[a])?;
                    if sx.as_int(a).is_none() {
                        return Ok(None);
                    }
                    let ba = sx.pow(b, a)?;
                    s = sx.mul(&[s, ba])?;
                    let be = sx.pow(b, e0)?;
                    c.push(be);
                    continue;
                }
                // a power of an inner sum
                if let (Some(n), Some((ci, d, w))) = (sx.as_int(e), inner_word(sx, b, k)?) {
                    if (1..=4).contains(&n) {
                        for _ in 0..n {
                            c.push(ci);
                            inners.push((d, w.clone()));
                        }
                        continue;
                    }
                }
                return Ok(None);
            }
        }
        let Some((ci, d, w)) = inner_word(sx, u, k)? else { return Ok(None) };
        c.push(ci);
        inners.push((d, w));
    }
    let c = sx.mul(&c)?;
    // every inner sum of k as one of k - 1 plus its first letter at k
    let mut terms = vec![Term { c, outer: (r, s), inner: vec![] }];
    for (d, w) in inners {
        let mut next = vec![];
        for t in terms {
            if d == 0 {
                let (r1, s1) = w[0];
                let r2 = sx.add(&[t.outer.0, r1])?;
                let s2 = sx.mul(&[t.outer.1, s1])?;
                let mut inner = t.inner.clone();
                let tail = w[1..].to_vec();
                if !tail.is_empty() {
                    inner.push(tail);
                }
                next.push(Term { c: t.c, outer: (r2, s2), inner });
            }
            let mut inner = t.inner;
            inner.push(w.clone());
            next.push(Term { c: t.c, outer: t.outer, inner });
        }
        terms = next;
        if terms.len() > MAX_TERMS {
            return Ok(None);
        }
    }
    // depth one with s = 1 is a power sum, which other methods give in closed form
    if terms.iter().all(|t| t.inner.is_empty()) && terms.iter().all(|t| t.outer.1 == sx.int(1) || sx.as_int(t.outer.0).is_none_or(|r| r < 1)) {
        return Ok(None);
    }
    Ok(Some(terms))
}

/// The stuffle (quasi-shuffle) product of two words: (coefficient, word) pairs.
fn stuffle<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: &[Letter<'h>], v: &[Letter<'h>]) -> Res<Vec<(T<'h>, Vec<Letter<'h>>)>> {
    if u.is_empty() {
        return Ok(vec![(sx.int(1), v.to_vec())]);
    }
    if v.is_empty() {
        return Ok(vec![(sx.int(1), u.to_vec())]);
    }
    let mut out = vec![];
    for (c, w) in stuffle(sx, &u[1..], v)? {
        let mut x = vec![u[0]];
        x.extend(w);
        out.push((c, x));
    }
    for (c, w) in stuffle(sx, u, &v[1..])? {
        let mut x = vec![v[0]];
        x.extend(w);
        out.push((c, x));
    }
    let a = sx.add(&[u[0].0, v[0].0])?;
    let b = sx.mul(&[u[0].1, v[0].1])?;
    for (c, w) in stuffle(sx, &u[1..], &v[1..])? {
        let mut x = vec![(a, b)];
        x.extend(w);
        out.push((c, x));
    }
    Ok(out)
}

/// S(n; w): a harmonic number for one letter, a multiple harmonic number otherwise (the empty word is 1). A first
/// letter (0, 1) is summed out: S(n; (0, 1), (r, s), w) = n S(n; (r, s), w) - S(n; (r - 1, s), w), the inner index
/// counted n - j times.
fn mhn<'h>(sx: &mut Sx<'_, '_, '_, 'h>, n: T<'h>, w: &[Letter<'h>]) -> Res<T<'h>> {
    if w.len() >= 2 && w[0] == (sx.int(0), sx.int(1)) {
        let a = mhn(sx, n, &w[1..])?;
        let mut down = w[1..].to_vec();
        down[0].0 = sx.plus(down[0].0, -1)?;
        let b = mhn(sx, n, &down)?;
        let na = sx.mul(&[n, a])?;
        return sx.sub(na, b);
    }
    match w {
        [] => Ok(sx.int(1)),
        [(r, s)] if *s == sx.int(1) => {
            if *r == sx.int(1) {
                sx.ev("harmonic_number", &[n])
            } else {
                sx.ev("harmonic_number", &[n, *r])
            }
        }
        [(r, s)] => sx.ev("harmonic_number", &[n, *r, *s]),
        _ => {
            let rs: Vec<T<'h>> = w.iter().map(|l| l.0).collect();
            let ss: Vec<T<'h>> = w.iter().map(|l| l.1).collect();
            let (rl, sl) = (sx.list(&rs), sx.list(&ss));
            sx.ev("multiple_harmonic_number", &[n, rl, sl])
        }
    }
}
