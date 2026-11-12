//! Moves of the search added in round 19 (phase 12A). Each is an identity for every value of the variables (or,
//! for [`sqrt_parity`], proved by the substitution itself), and each is kept by the search only when the
//! complexity drops.
//!
//! | move | rewriting |
//! |---|---|
//! | [`rationalize`] | `1/(a + b sqrt(c))^k` as `(a - b sqrt(c))^k/(a^2 - b^2 c)^k`, then expanded (`3 + 4/(3 + sqrt 13)` is `sqrt 13`) |
//! | [`power_collect`] | the terms of a sum with powers `B^e` of one base whose exponents differ by integers, over the lowest power: `sum c_i B^(e_i) = B^(e_min) expand(sum c_i B^(e_i - e_min))` (the Laplacian of `1/r` is 0) |
//! | [`pythagoras_one`] | a constant `k` of a sum with `sin(u) cos(u)` written `k (sin(u)^2 + cos(u)^2)`, then factored (`1 + 2 sin cos` is `(sin + cos)^2`) |
//! | [`sqrt_parity`] | `sqrt(u^2)` replaced by `u` when the replacements by `u` and by `-u` give one result (the value is one of the two at every point) |
//! | [`common_factor`] | the factors common to all terms of a sum taken out (`-h + x^4 h` is `h (x^4 - 1)`) |
//! | [`im_split`], [`im_combine`] | `im_part` and `re` over sums with real numeric coefficients, `im_part(log z) = arg(z)`, `re(log z) = log(abs(z))`; and back |
//! | [`log_primes`] | the logarithms of positive rationals in a sum over their prime factors (`log(64)/18 - log(2)/3` is 0) |
//! | [`linear_equation`] | a linear equation with rational coefficients with its terms moved to one side, divided by their content, and the negative ones moved back (`2x - 4y + 6z - 10 == -8` is `x + 3z == 1 + 2y`) |
use crate::term::{apply, rewrite, B};
use mc_api::{Store, Sym, View, T};
use mc_eval::{NativeCx, Res};

fn rational<'h>(b: &B<'h>, t: T<'h>) -> bool {
    matches!(b.heap().view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. })
}

/// `(a, b, c)` when `t` is `a + b sqrt(c)` (rationals, `c > 0` not a square).
fn quadratic_surd<'h>(b: &B<'h>, t: T<'h>) -> Option<(T<'h>, T<'h>, T<'h>)> {
    let ts = b.terms(t);
    let (a, rest) = match ts.as_slice() {
        [a, r] if rational(b, *a) => (*a, *r),
        [r] => (b.int(0), *r),
        _ => return None,
    };
    let (k, rad) = match b.call_of(rest) {
        Some((m, &[k, r])) if m == b.hd.mul && rational(b, k) => (k, r),
        _ => (b.int(1), rest),
    };
    match b.call_of(rad) {
        Some((p, &[c, e])) if p == b.hd.pow && e == b.rat(1, 2) && rational(b, c) => Some((a, k, c)),
        _ => None,
    }
}

/// Denominators `a + b sqrt(c)` rationalised (see the module documentation).
pub fn rationalize<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let r = rewrite(cx, b, t, &mut |_, b, node| {
        let (base, k) = b.int_power(node);
        if !(-8..0).contains(&k) {
            return Ok(None);
        }
        let Some((a, c, d)) = quadratic_surd(b, base) else { return Ok(None) };
        let conj = b.sub(a, b.mul(&[c, b.pow(d, b.rat(1, 2))]));
        let norm = b.sub(b.mul(&[a, a]), b.mul(&[c, c, d]));
        if !rational(b, norm) || norm == b.int(0) {
            return Ok(None);
        }
        Ok(Some(b.mul(&[b.powi(conj, -k), b.powi(norm, k)])))
    })?;
    if r == t {
        return Ok(t);
    }
    let r = cx.eval(r)?;
    apply(cx, b, "expand", r)
}

/// The base and exponent of a power with a non-integer exponent (`(B, e)`), else `None`.
fn frac_power<'h>(b: &B<'h>, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    match b.call_of(t) {
        Some((p, &[base, e])) if p == b.hd.pow && !matches!(b.heap().view(e), View::Int(_)) && b.call_of(base).is_some() => Some((base, e)),
        _ => None,
    }
}

/// Sums of terms with powers of one base whose exponents differ by integers, over the lowest power.
pub fn power_collect<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let r = rewrite(cx, b, t, &mut |cx, b, node| {
        let Some((s, terms)) = b.call_of(node) else { return Ok(None) };
        if s != b.hd.add || terms.len() < 2 {
            return Ok(None);
        }
        // the bases of fractional powers among the factors of the terms
        let mut bases: Vec<T<'h>> = vec![];
        for &u in terms {
            for f in b.factors(u) {
                if let Some((base, _)) = frac_power(b, f) {
                    if !bases.contains(&base) {
                        bases.push(base);
                    }
                }
            }
        }
        for base in bases {
            // each term: (exponent of base, the rest)
            let mut parts: Vec<(T<'h>, T<'h>)> = vec![];
            for &u in terms {
                let fs = b.factors(u);
                let mut e = b.int(0);
                let mut rest = vec![];
                for f in fs {
                    match b.call_of(f) {
                        Some((p, &[x, k])) if p == b.hd.pow && x == base => e = k,
                        _ => rest.push(f),
                    }
                }
                parts.push((e, b.mul(&rest)));
            }
            let fracs: Vec<T<'h>> = parts.iter().map(|p| p.0).filter(|&e| e != b.int(0)).collect();
            if fracs.len() < 2 {
                continue;
            }
            let low = fracs[0];
            // all exponents (0 included) differ from the first fractional one by integers
            let mut shifts = vec![];
            let mut ok = true;
            for &(e, _) in &parts {
                match b.heap().view(b.sub(e, low)) {
                    View::Int(k) if k.abs() <= 8 => shifts.push(k),
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let m = *shifts.iter().min().unwrap_or(&0);
            let inner: Vec<T<'h>> = parts.iter().zip(&shifts).map(|(&(_, r), &k)| b.mul(&[r, b.powi(base, k - m)])).collect();
            let inner = apply(cx, b, "expand", b.add(&inner))?;
            return Ok(Some(b.mul(&[b.pow(base, b.add(&[low, b.int(m)])), inner])));
        }
        Ok(None)
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

/// The angles `u` with both `sin(u)` and `cos(u)` in `t`.
fn sin_cos_angles<'h>(b: &B<'h>, t: T<'h>, out: &mut Vec<T<'h>>, seen_sin: &mut Vec<T<'h>>, seen_cos: &mut Vec<T<'h>>) {
    if let Some((s, args)) = b.call_of(t) {
        if let [u] = args {
            if s == b.hd.sin && !seen_sin.contains(u) {
                seen_sin.push(*u);
                if seen_cos.contains(u) && !out.contains(u) {
                    out.push(*u);
                }
            }
            if s == b.hd.cos && !seen_cos.contains(u) {
                seen_cos.push(*u);
                if seen_sin.contains(u) && !out.contains(u) {
                    out.push(*u);
                }
            }
        }
        for &a in args {
            sin_cos_angles(b, a, out, seen_sin, seen_cos);
        }
    }
}

/// A constant `k` of a sum that has `sin(u)` and `cos(u)` written `k (sin^2 + cos^2)`, then factored, in every sum
/// of `t` where that is smaller.
pub fn pythagoras_one<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, cost: &mut crate::cost::Cost<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if h.mask(t) & h.mask(h.sym_term(b.hd.sin)) == 0 || h.mask(t) & h.mask(h.sym_term(b.hd.cos)) == 0 {
        return Ok(t);
    }
    let r = rewrite(cx, b, t, &mut |cx, b, node| {
        let Some((s, terms)) = b.call_of(node) else { return Ok(None) };
        if s != b.hd.add {
            return Ok(None);
        }
        let Some(k) = terms.iter().copied().find(|&u| rational(b, u)) else { return Ok(None) };
        let mut angles = vec![];
        sin_cos_angles(b, node, &mut angles, &mut vec![], &mut vec![]);
        let [u] = angles.as_slice() else { return Ok(None) };
        let one = b.add(&[b.powi(b.call(b.hd.sin, &[*u]), 2), b.powi(b.call(b.hd.cos, &[*u]), 2)]);
        let rest: Vec<T<'h>> = terms.iter().copied().filter(|&x| x != k).chain([b.mul(&[k, one])]).collect();
        let e = apply(cx, b, "expand", b.add(&rest))?;
        let f = apply(cx, b, "factor", e)?;
        Ok((cost.size(f) < cost.size(node)).then_some(f))
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

/// The powers `(u^2)^(k/2)` (odd `k`) of `t` replaced by `u^k` when the replacements by `u^k` and by `(-u)^k` give
/// one result: `sqrt(u^2)` is `u` or `-u` at every point, so the result is the value everywhere.
pub fn sqrt_parity<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let mut kernels: Vec<(T<'h>, T<'h>, T<'h>)> = vec![];
    collect_root_squares(b, t, &mut kernels);
    let mut cur = t;
    let mut bases: Vec<T<'h>> = vec![];
    for k in &kernels {
        if !bases.contains(&k.1) {
            bases.push(k.1);
        }
    }
    let rule_head = b.heap().sym(mc_api::names::RULE);
    for u in bases {
        let with = |v: T<'h>| -> T<'h> {
            let rules: Vec<T<'h>> =
                kernels.iter().filter(|k| k.1 == u).map(|k| b.call(rule_head, &[k.0, b.pow(v, b.mul(&[b.int(2), k.2]))])).collect();
            b.call(b.hd.list, &rules)
        };
        let plus = cx.eval(b.call_named("rewrite", &[cur, with(u)]))?;
        let minus = cx.eval(b.call_named("rewrite", &[cur, with(b.neg(u))]))?;
        if plus == minus {
            cur = plus;
        }
    }
    Ok(cur)
}

/// The kernels `(u^2)^e` with `e` a half of an odd integer: (kernel, u, e).
fn collect_root_squares<'h>(b: &B<'h>, t: T<'h>, out: &mut Vec<(T<'h>, T<'h>, T<'h>)>) {
    if let Some((s, args)) = b.call_of(t) {
        if let (true, [base, e]) = (s == b.hd.pow, args) {
            let odd_half = matches!(b.heap().view(*e), View::Rational { den, .. } if den == b.int(2));
            if odd_half {
                let (u, two) = b.int_power(*base);
                if two == 2 && !out.iter().any(|x| x.0 == t) {
                    out.push((t, u, *e));
                }
            }
        }
        for &a in args {
            collect_root_squares(b, a, out);
        }
    }
}

/// The factors common to every term of a sum taken out (`-h + x^4 h` is `h (-1 + x^4)`): the factors of the first
/// term that every other term has, with the least integer power.
pub fn common_factor<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let Some((s, terms)) = b.call_of(t) else { return Ok(t) };
    if s != b.hd.add || terms.len() < 2 {
        return Ok(t);
    }
    let powers = |u: T<'h>| -> Vec<(T<'h>, i64)> { b.factors(u).into_iter().filter(|&f| !b.is_number(f)).map(|f| b.int_power(f)).collect() };
    let mut common: Vec<(T<'h>, i64)> = powers(terms[0]).into_iter().filter(|p| p.1 > 0).collect();
    for &u in &terms[1..] {
        let ps = powers(u);
        common = common
            .into_iter()
            .filter_map(|(base, k)| ps.iter().find(|p| p.0 == base && p.1 > 0).map(|p| (base, k.min(p.1))))
            .collect();
    }
    if common.is_empty() {
        return Ok(t);
    }
    let c = b.mul(&common.iter().map(|&(x, k)| b.powi(x, k)).collect::<Vec<_>>());
    let rest: Vec<T<'h>> = terms.iter().map(|&u| b.div(u, c)).collect();
    cx.eval(b.mul(&[c, b.add(&rest)]))
}

fn real_number<'h>(b: &B<'h>, t: T<'h>) -> bool {
    b.is_number(t) && b.complex_parts(t).is_none()
}

/// `im_part` and `re` over sums with real numeric coefficients, and of logarithms.
pub fn im_split<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let (re, im) = (h.sym("re"), h.sym("im_part"));
    if h.mask(t) & (h.mask(h.sym_term(re)) | h.mask(h.sym_term(im))) == 0 {
        return Ok(t);
    }
    let r = rewrite(cx, b, t, &mut |_, b, node| {
        let Some((s, &[u])) = b.call_of(node) else { return Ok(None) };
        if s != re && s != im {
            return Ok(None);
        }
        if let Some(z) = b.unary(u, b.hd.log) {
            return Ok(Some(if s == im { b.call_named("arg", &[z]) } else { b.call(b.hd.log, &[b.call_named("abs", &[z])]) }));
        }
        let terms = b.terms(u);
        if terms.len() < 2 && !matches!(b.call_of(u), Some((m, _)) if m == b.hd.mul) {
            return Ok(None);
        }
        let mut out = vec![];
        let mut changed = terms.len() > 1;
        for x in terms {
            let fs = b.factors(x);
            let (nums, rest): (Vec<T<'h>>, Vec<T<'h>>) = fs.into_iter().partition(|&f| real_number(b, f));
            if !nums.is_empty() && !rest.is_empty() {
                changed = true;
            }
            let r = b.mul(&rest);
            let part = match b.unary(r, b.hd.log) {
                Some(z) if s == im => b.call_named("arg", &[z]),
                Some(z) => b.call(b.hd.log, &[b.call_named("abs", &[z])]),
                None => b.call(s, &[r]),
            };
            out.push(b.mul(&[b.mul(&nums), part]));
        }
        Ok(changed.then(|| b.add(&out)))
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

/// The terms `c im_part(u)` (and `c re(u)`) of a sum, `c` real numbers, as one `im_part` (one `re`).
pub fn im_combine<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let (re, im) = (h.sym("re"), h.sym("im_part"));
    if h.mask(t) & (h.mask(h.sym_term(re)) | h.mask(h.sym_term(im))) == 0 {
        return Ok(t);
    }
    let r = rewrite(cx, b, t, &mut |_, b, node| {
        let Some((s, terms)) = b.call_of(node) else { return Ok(None) };
        if s != b.hd.add {
            return Ok(None);
        }
        let mut out = vec![];
        let mut changed = false;
        for which in [re, im] {
            let mut inside = vec![];
            for &x in terms {
                let (c, f) = match b.call_of(x) {
                    Some((m, &[c, f])) if m == b.hd.mul && real_number(b, c) => (c, f),
                    _ => (b.int(1), x),
                };
                if let Some(u) = b.unary(f, which) {
                    inside.push(b.mul(&[c, u]));
                }
            }
            if inside.len() >= 2 {
                changed = true;
                out.push(b.call(which, &[b.add(&inside)]));
            }
        }
        if !changed {
            return Ok(None);
        }
        for &x in terms {
            let f = match b.call_of(x) {
                Some((m, &[c, f])) if m == b.hd.mul && real_number(b, c) => f,
                _ => x,
            };
            let grouped = |w: Sym| b.unary(f, w).is_some() && terms.iter().filter(|&&y| {
                let g = match b.call_of(y) {
                    Some((m, &[c, g])) if m == b.hd.mul && real_number(b, c) => g,
                    _ => y,
                };
                b.unary(g, w).is_some()
            }).count() >= 2;
            if !grouped(re) && !grouped(im) {
                out.push(x);
            }
        }
        Ok(Some(b.add(&out)))
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

/// The prime factorisation of a positive machine integer by trial division (`None` past `10^6` trials).
fn factorize(mut n: u64) -> Option<Vec<(u64, i64)>> {
    let mut out = vec![];
    let mut p = 2u64;
    let mut trials = 0;
    while p * p <= n {
        trials += 1;
        if trials > 1_000_000 {
            return None;
        }
        let mut k = 0;
        while n.is_multiple_of(p) {
            n /= p;
            k += 1;
        }
        if k > 0 {
            out.push((p, k));
        }
        p += if p == 2 { 1 } else { 2 };
    }
    if n > 1 {
        out.push((n, 1));
    }
    Some(out)
}

/// The terms `c log(q)` of a sum (`c` rational, `q` a positive rational with machine parts) over the logarithms of
/// the primes of the `q`, when there are two or more.
pub fn log_primes<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if h.mask(t) & h.mask(h.sym_term(b.hd.log)) == 0 {
        return Ok(t);
    }
    let r = rewrite(cx, b, t, &mut |_, b, node| {
        let Some((s, terms)) = b.call_of(node) else { return Ok(None) };
        if s != b.hd.add {
            return Ok(None);
        }
        let mut rest = vec![];
        let mut primes: Vec<(u64, T<'h>)> = vec![];
        let mut found = 0;
        for &x in terms {
            let (c, l) = match b.call_of(x) {
                Some((m, &[c, l])) if m == b.hd.mul && rational(b, c) => (c, l),
                _ => (b.int(1), x),
            };
            let q = b.unary(l, b.hd.log);
            let parts = q.and_then(|q| match b.heap().view(q) {
                View::Int(n) if n > 1 => Some((n as u64, 1u64)),
                View::Rational { num, den } => match (b.heap().view(num), b.heap().view(den)) {
                    (View::Int(n), View::Int(d)) if n > 0 => Some((n as u64, d as u64)),
                    _ => None,
                },
                _ => None,
            });
            let fs = parts.and_then(|(n, d)| Some((factorize(n)?, factorize(d)?)));
            match fs {
                Some((num, den)) => {
                    found += 1;
                    for (p, k) in num.into_iter().chain(den.into_iter().map(|(p, k)| (p, -k))) {
                        let add = b.mul(&[c, b.int(k)]);
                        match primes.iter_mut().find(|e| e.0 == p) {
                            Some(e) => e.1 = b.add(&[e.1, add]),
                            None => primes.push((p, add)),
                        }
                    }
                }
                None => rest.push(x),
            }
        }
        if found < 2 {
            return Ok(None);
        }
        for (p, c) in primes {
            rest.push(b.mul(&[c, b.call(b.hd.log, &[b.int(p as i64)])]));
        }
        Ok(Some(b.add(&rest)))
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

/// Is `t` linear in its symbols with rational coefficients (a sum of rationals and rational multiples of
/// symbols)?
fn linear_terms<'h>(b: &B<'h>, t: T<'h>) -> Option<Vec<(T<'h>, T<'h>)>> {
    let mut out = vec![];
    for x in b.terms(t) {
        if rational(b, x) {
            out.push((x, b.int(1)));
            continue;
        }
        let (c, v) = match b.call_of(x) {
            Some((m, &[c, v])) if m == b.hd.mul && rational(b, c) => (c, v),
            _ => (b.int(1), x),
        };
        b.sym_of(v)?;
        out.push((c, v));
    }
    Some(out)
}

/// `l == r` for a linear equation with rational coefficients: the terms on one side divided by their content, the
/// negative ones and the constant on the right.
pub fn linear_equation<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let equal = h.sym("equal");
    let Some((s, &[l, r])) = b.call_of(t) else { return Ok(t) };
    if s != equal {
        return Ok(t);
    }
    let d = apply(cx, b, "expand", b.sub(l, r))?;
    let Some(terms) = linear_terms(b, d) else { return Ok(t) };
    if terms.iter().all(|x| x.1 == b.int(1)) {
        // no variable: the evaluator decides the equation
        return Ok(t);
    }
    // the content: the gcd of the numerators over the lcm of the denominators, by the evaluator
    let coeffs: Vec<T<'h>> = terms.iter().map(|x| x.0).collect();
    let g = cx.eval(b.call_named("gcd", &coeffs))?;
    if !rational(b, g) || g == b.int(0) {
        return Ok(t);
    }
    let mut left = vec![];
    let mut right = vec![];
    let lead_negative = terms.iter().filter(|x| x.1 != b.int(1)).all(|x| b.ar.compare(x.0, b.int(0)).is_some_and(|o| o == mc_arith::number::Cmp3::Less));
    let g = if lead_negative { b.neg(g) } else { g };
    for (c, v) in terms {
        let c = b.div(c, g);
        let positive = b.ar.compare(c, b.int(0)).is_some_and(|o| o == mc_arith::number::Cmp3::Greater);
        if v == b.int(1) {
            right.push(b.neg(c));
        } else if positive {
            left.push(b.mul(&[c, v]));
        } else {
            right.push(b.mul(&[b.neg(c), v]));
        }
    }
    let out = b.call(equal, &[b.add(&left), b.add(&right)]);
    cx.eval(out)
}
