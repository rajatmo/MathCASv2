//! A zero test for expressions in gamma functions of one variable (`simplify`, `full_simplify`).
//!
//! Every `gamma(m n + c)` (a positive integer `m` up to [`MAX_M`], a rational `c`, one symbol `n` shared by all the
//! gamma functions) is written by Gauss's multiplication formula (DLMF 5.5.6)
//! `gamma(m z) = (2 pi)^((1 - m)/2) m^(m z - 1/2) prod_(k = 0)^(m - 1) gamma(z + k/m)` and the recurrence
//! `gamma(z + 1) = z gamma(z)` (DLMF 5.5.1) as a rational function of `n` times `gamma(n + f)` with `f` in `[0, 1)`;
//! each `gamma(n + f)` is then a new symbol. When the result brought to one fraction (`together`) is 0, the input is
//! 0: the rewriting is an identity of meromorphic functions, so a zero after it is a proof (the converse need not
//! hold, and then the test says nothing). It replaces a search over the gamma shift forms of the terms that ran to
//! the time limit (`gamma(2 n + 1)` against products of `gamma(n)` and `gamma(n + 1/2)`, the Catalan numbers as
//! central binomial coefficients).
use crate::term::B;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The largest multiplier `m` of the variable in an argument that is written by the multiplication formula.
pub const MAX_M: i64 = 12;

/// The gamma arguments in `t` (distinct, in the order met).
fn gamma_args<'h>(b: &B<'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    // shared subterms are visited once (a term can be a tree exponentially larger than itself)
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if let Some((s, args)) = b.call_of(u) {
            if h.sym_name(s) == "gamma" && args.len() == 1 && !out.contains(&args[0]) {
                out.push(args[0]);
            }
            stack.extend_from_slice(args);
        }
    }
}

/// The symbols of `t` that are not constants.
fn symbols<'h>(b: &B<'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    // shared subterms are visited once (a term can be a tree exponentially larger than itself)
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Sym(s) => {
                if !matches!(h.sym_name(s).as_str(), "pi" | "e" | "im" | "euler_gamma" | "catalan") && !out.contains(&u) {
                    out.push(u);
                }
            }
            View::App { args, .. } => stack.extend_from_slice(args),
            _ => {}
        }
    }
}

fn is_rational<'h>(b: &B<'h>, t: T<'h>) -> bool {
    matches!(b.heap().view(t), View::Int(_) | View::Rational { .. })
}

/// `gamma(n + s)` for a rational `s`: `gamma(n + f)` (as the symbol of `f`) times the shift factors.
fn shifted<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, n: T<'h>, s: T<'h>, names: &mut Vec<(T<'h>, T<'h>)>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let j = cx.eval(b.call_named("floor", &[s]))?;
    let View::Int(j) = h.view(j) else { return Ok(None) };
    if j.abs() > 64 {
        return Ok(None);
    }
    let f = cx.eval(b.sub(s, b.int(j)))?;
    let g = match names.iter().find(|(g, _)| *g == f) {
        Some(&(_, sym)) => sym,
        None => {
            let sym = h.sym_term(h.sym(&format!("gamma$shift{}", names.len())));
            names.push((f, sym));
            sym
        }
    };
    let base = b.add(&[n, f]);
    let mut fs = vec![g];
    if j >= 0 {
        fs.extend((0..j).map(|i| b.add(&[base, b.int(i)])));
    } else {
        fs.extend((1..=-j).map(|i| b.powi(b.add(&[base, b.int(-i)]), -1)));
    }
    Ok(Some(b.mul(&fs)))
}

/// `t` with every power `c^(alpha n + beta)` of a positive rational `c` (an integer `alpha`, `beta` free of `n`)
/// written as `c^beta` times powers of new symbols standing for `p^n`, one per prime `p` of `c`
/// (`2^(2 n - 1)` and `4^n/2` become one term); `None` for a power of `n` it cannot write so.
fn prime_powers<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, n: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut pows = vec![];
    // shared subterms are visited once (a term can be a tree exponentially larger than itself)
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        if let Some((s, args)) = b.call_of(u) {
            if s == b.hd.pow && args.len() == 2 && is_rational(b, args[0]) {
                let mut syms = vec![];
                symbols(b, args[1], &mut syms);
                if syms.contains(&n) && !pows.contains(&u) {
                    pows.push(u);
                }
            }
            stack.extend_from_slice(args);
        }
    }
    let mut out = t;
    for pw in pows {
        cx.tick(1)?;
        let Some((_, &[c, e])) = b.call_of(pw) else { return Ok(None) };
        let alpha = cx.eval(b.call_named("d", &[e, n]))?;
        let View::Int(alpha) = h.view(alpha) else { return Ok(None) };
        let beta = cx.eval(b.sub(e, b.mul(&[b.int(alpha), n])))?;
        let mut syms = vec![];
        symbols(b, beta, &mut syms);
        if syms.contains(&n) {
            return Ok(None);
        }
        // the primes of the numerator and the denominator of c
        let (num, den) = match h.view(c) {
            View::Int(k) if k > 0 => (k, 1),
            View::Rational { .. } => {
                let nu = cx.eval(b.call_named("numerator", &[c]))?;
                let de = cx.eval(b.call_named("denominator", &[c]))?;
                match (h.view(nu), h.view(de)) {
                    (View::Int(a), View::Int(d)) if a > 0 => (a, d),
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        let mut fs = vec![b.pow(c, beta)];
        for (k, sign) in [(num, 1), (den, -1)] {
            let mut k = k;
            let mut p = 2i64;
            while k > 1 {
                if p * p > k {
                    p = k;
                }
                let mut e = 0;
                while k % p == 0 {
                    k /= p;
                    e += 1;
                }
                if e > 0 {
                    let s = h.sym_term(h.sym(&format!("gamma$prime{p}")));
                    fs.push(b.powi(s, sign * e * alpha));
                }
                p += 1;
            }
        }
        let rule = h.call(h.sym(mc_api::names::RULE), &[pw, b.mul(&fs)]);
        out = cx.eval(b.call_named("rewrite", &[out, rule]))?;
    }
    Ok(Some(out))
}

/// Is `t` proved 0 by the normal form of its gamma functions? `false` says nothing.
pub fn gamma_zero<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<bool> {
    let h = b.heap();
    let mut args = vec![];
    gamma_args(b, t, &mut args);
    if args.is_empty() || args.len() > 16 {
        return Ok(false);
    }
    let mut vars = vec![];
    for &a in &args {
        symbols(b, a, &mut vars);
    }
    let [n] = vars.as_slice() else { return Ok(false) };
    let n = *n;
    let mut names: Vec<(T<'h>, T<'h>)> = vec![];
    let mut rules = vec![];
    for &a in &args {
        cx.tick(1)?;
        // a = m n + c
        let m = cx.eval(b.call_named("d", &[a, n]))?;
        let View::Int(m) = h.view(m) else { return Ok(false) };
        if !(1..=MAX_M).contains(&m) {
            return Ok(false);
        }
        let c = cx.eval(b.sub(a, b.mul(&[b.int(m), n])))?;
        if !is_rational(b, c) {
            return Ok(false);
        }
        // the multiplication formula at z = n + c/m
        // the constant factors as powers of primes and of pi, which the arithmetic combines exactly (they were
        // combined by `power_expand` of the whole term, which also rewrote sqrt(x^2), log(exp(x)) and asin(sin(x))
        // in it to x: simplify(gamma(x + 1)/gamma(x) - sqrt(x^2)) was 0)
        let mut fs = vec![];
        if m > 1 {
            let e = b.rat(1 - m, 2);
            fs.push(b.pow(b.int(2), e));
            fs.push(b.pow(h.sym_term(h.sym("pi")), e));
            fs.push(b.pow(b.int(m), b.mul(&[b.int(m), n])));
            let (mut k, mut p) = (m, 2);
            while k > 1 {
                let mut j = 0;
                while k % p == 0 {
                    k /= p;
                    j += 1;
                }
                if j > 0 {
                    fs.push(b.pow(b.int(p), cx.eval(b.mul(&[b.int(j), b.sub(c, b.rat(1, 2))]))?));
                }
                p += 1;
            }
        }
        for k in 0..m {
            let s = cx.eval(b.add(&[b.div(c, b.int(m)), b.rat(k, m)]))?;
            let Some(g) = shifted(cx, b, n, s, &mut names)? else { return Ok(false) };
            fs.push(g);
        }
        let rhs = b.mul(&fs);
        rules.push(h.call(h.sym(mc_api::names::RULE), &[b.call_named("gamma", &[a]), rhs]));
    }
    let mut u = t;
    for r in rules {
        u = cx.eval(b.call_named("rewrite", &[u, r]))?;
    }
    let mut gs = vec![];
    gamma_args(b, u, &mut gs);
    if !gs.is_empty() {
        return Ok(false);
    }
    let Some(u) = prime_powers(cx, b, u, n)? else { return Ok(false) };
    let z = cx.eval(b.call_named("together", &[u]))?;
    Ok(z == b.int(0))
}
