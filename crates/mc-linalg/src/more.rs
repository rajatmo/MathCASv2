//! Matrix functions and constructors defined by formulas (`mc_numeric::formula`):
//! `matrix_function(f, m)`, `matrix_exp`, `matrix_log`, `matrix_power`, `conjugate_transpose` (gap queue G3),
//! `hilbert_matrix`, `rotation_matrix`.
//!
//! `matrix_function(f, m)` (decision P6N.8): a diagonal matrix maps its diagonal; a triangular matrix with distinct
//! diagonal entries uses Parlett's recurrence (exact for exact entries); a matrix of machine numbers with a
//! complete set of eigenvectors is V f(D) V^-1; otherwise f(m) is p(m) for the polynomial p interpolating f and its
//! derivatives at the eigenvalues with their multiplicities (Newton's divided differences, confluent for repeated
//! eigenvalues; derivatives by `d`). Exact results are simplified entry by entry.
use mc_api::{Store, View, T};
use mc_arith::Kind;
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;
type M<'h> = Vec<Vec<T<'h>>>;

/// The rows of a square matrix.
fn square<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<M<'h>> {
    let rows = fx.as_list(t)?;
    let n = rows.len();
    if n == 0 {
        return None;
    }
    rows.iter().map(|&r| fx.as_list(r).filter(|r| r.len() == n).map(|r| r.to_vec())).collect()
}

fn to_term<'h>(fx: &Fx<'_, '_, '_, 'h>, m: &M<'h>) -> T<'h> {
    let rows: Vec<T<'h>> = m.iter().map(|r| fx.list(r)).collect();
    fx.list(&rows)
}

fn is_zero<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    fx.num(t).is_some_and(|n| n.is_zero())
}

fn identity<'h>(fx: &Fx<'_, '_, '_, 'h>, n: usize) -> M<'h> {
    (0..n).map(|i| (0..n).map(|j| fx.int(i64::from(i == j))).collect()).collect()
}

fn matmul<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &M<'h>, b: &M<'h>) -> M<'h> {
    let n = a.len();
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    let terms: Vec<T<'h>> = (0..n).map(|k| fx.mul(&[a[i][k], b[k][j]])).collect();
                    fx.add(&terms)
                })
                .collect()
        })
        .collect()
}

/// f applied to x (f a symbol, a lambda or any head), evaluated.
fn apply<'h>(fx: &mut Fx<'_, '_, '_, 'h>, f: T<'h>, x: T<'h>) -> Res<T<'h>> {
    fx.eval(fx.h.app(f, &[x]))
}

/// The k-th derivative of f at x, divided by k!.
fn taylor<'h>(fx: &mut Fx<'_, '_, '_, 'h>, f: T<'h>, x: T<'h>, k: usize) -> Res<T<'h>> {
    if k == 0 {
        return apply(fx, f, x);
    }
    let v = fx.sym("matrix_function·x");
    let fv = fx.h.app(f, &[v]);
    let d = fx.eval(fx.call("d", &[fv, fx.list(&[v, fx.int(k as i64)])]))?;
    let rule = fx.call("rule", &[v, x]);
    let at = fx.eval(fx.call("rewrite", &[d, rule]))?;
    let fact: i64 = (1..=k as i64).product();
    Ok(fx.div(at, fx.int(fact)))
}

fn machine_matrix<'h>(fx: &Fx<'_, '_, '_, 'h>, m: &M<'h>) -> bool {
    let mut any = false;
    for r in m {
        for &x in r {
            match fx.num(x) {
                Some(n) if n.kind() == Kind::Machine => any = true,
                Some(_) => {}
                None => return false,
            }
        }
    }
    any
}

/// Parlett's recurrence for an upper triangular t with distinct diagonal entries.
fn parlett<'h>(fx: &mut Fx<'_, '_, '_, 'h>, f: T<'h>, t: &M<'h>) -> Res<Option<M<'h>>> {
    let n = t.len();
    let mut fm: M<'h> = vec![vec![fx.int(0); n]; n];
    for i in 0..n {
        fm[i][i] = apply(fx, f, t[i][i])?;
    }
    for d in 1..n {
        for i in 0..n - d {
            let j = i + d;
            let den = fx.eval(fx.sub(t[j][j], t[i][i]))?;
            if is_zero(fx, den) {
                return Ok(None);
            }
            let mut num = vec![fx.mul(&[t[i][j], fx.sub(fm[j][j], fm[i][i])])];
            for k in i + 1..j {
                num.push(fx.sub(fx.mul(&[fm[i][k], t[k][j]]), fx.mul(&[t[i][k], fm[k][j]])));
            }
            fm[i][j] = fx.div(fx.add(&num), den);
            fx.tick(1)?;
        }
    }
    Ok(Some(fm))
}

fn transpose<'h>(m: &M<'h>) -> M<'h> {
    let n = m.len();
    (0..n).map(|i| (0..n).map(|j| m[j][i]).collect()).collect()
}

/// The interpolation form p(m) at the eigenvalues (with multiplicities).
fn interpolate<'h>(fx: &mut Fx<'_, '_, '_, 'h>, f: T<'h>, m: &M<'h>) -> Res<Option<M<'h>>> {
    let n = m.len();
    let ev = fx.eval(fx.call("eigenvalues", &[to_term(fx, m)]))?;
    let Some(zs) = fx.as_list(ev) else { return Ok(None) };
    if zs.len() != n {
        return Ok(None);
    }
    // equal eigenvalues next to each other
    let mut z: Vec<T<'h>> = vec![];
    for &x in zs {
        match z.iter().rposition(|&y| y == x) {
            Some(p) => z.insert(p + 1, x),
            None => z.push(x),
        }
    }
    // divided differences, confluent for equal nodes
    let mut dd: Vec<Vec<T<'h>>> = vec![vec![fx.int(0); n]; n];
    for i in 0..n {
        dd[i][i] = apply(fx, f, z[i])?;
    }
    for d in 1..n {
        for i in 0..n - d {
            let j = i + d;
            dd[i][j] = if z[i] == z[j] {
                taylor(fx, f, z[i], d)?
            } else {
                let num = fx.sub(dd[i + 1][j], dd[i][j - 1]);
                fx.div(num, fx.sub(z[j], z[i]))
            };
            fx.tick(1)?;
        }
    }
    // p(m) = sum_k dd[0][k] prod_{j<k} (m - z_j)
    let id = identity(fx, n);
    let mut prod = id.clone();
    let mut acc: M<'h> = vec![vec![fx.int(0); n]; n];
    for k in 0..n {
        for r in 0..n {
            for c in 0..n {
                acc[r][c] = fx.add(&[acc[r][c], fx.mul(&[dd[0][k], prod[r][c]])]);
            }
        }
        if k + 1 < n {
            let shifted: M<'h> =
                (0..n).map(|r| (0..n).map(|c| fx.sub(m[r][c], fx.mul(&[z[k], id[r][c]]))).collect()).collect();
            prod = matmul(fx, &prod, &shifted);
        }
        fx.tick(n as u64)?;
    }
    Ok(Some(acc))
}

/// V f(D) V^-1 for a matrix of machine numbers with a complete set of eigenvectors.
fn by_eigenvectors<'h>(fx: &mut Fx<'_, '_, '_, 'h>, f: T<'h>, m: &M<'h>) -> Res<Option<M<'h>>> {
    let n = m.len();
    let es = fx.eval(fx.call("eigensystem", &[to_term(fx, m)]))?;
    let Some(&[vals, vecs]) = fx.as_list(es) else { return Ok(None) };
    let (Some(vals), Some(vecs)) = (fx.as_list(vals), fx.as_list(vecs)) else { return Ok(None) };
    if vals.len() != n || vecs.len() != n {
        return Ok(None);
    }
    let v = fx.eval(fx.call("transpose", &[fx.list(vecs)]))?;
    let vi = fx.eval(fx.call("inverse", &[v]))?;
    if fx.as_list(vi).is_none() {
        return Ok(None);
    }
    let mut fd = vec![];
    for &l in vals {
        fd.push(apply(fx, f, l)?);
    }
    let d = fx.eval(fx.call("diagonal_matrix", &[fx.list(&fd)]))?;
    let r = fx.eval(fx.call("dot", &[v, d, vi]))?;
    // a real matrix and a real-valued function on its spectrum: drop rounding-level imaginary parts
    let r = fx.eval(fx.call("chop", &[r]))?;
    Ok(square(fx, r))
}

fn matrix_function<'h>(fx: &mut Fx<'_, '_, '_, 'h>, f: T<'h>, mt: T<'h>) -> R<'h> {
    let Some(m) = square(fx, mt) else { return Ok(None) };
    let m = numeric_entries(fx, m)?;
    let n = m.len();
    let diagonal = (0..n).all(|i| (0..n).all(|j| i == j || is_zero(fx, m[i][j])));
    if diagonal {
        let mut out = m.clone();
        for i in 0..n {
            out[i][i] = apply(fx, f, m[i][i])?;
        }
        return Ok(Some(to_term(fx, &out)));
    }
    let machine = machine_matrix(fx, &m);
    let upper = (0..n).all(|i| (0..i).all(|j| is_zero(fx, m[i][j])));
    let lower = (0..n).all(|i| (i + 1..n).all(|j| is_zero(fx, m[i][j])));
    let mut r = None;
    if upper {
        r = parlett(fx, f, &m)?;
    } else if lower {
        r = parlett(fx, f, &transpose(&m))?.map(|t| transpose(&t));
    }
    if r.is_none() && machine {
        r = by_eigenvectors(fx, f, &m)?;
    }
    if r.is_none() {
        r = interpolate(fx, f, &m)?;
    }
    let Some(mut r) = r else { return Ok(None) };
    if !machine {
        for row in r.iter_mut() {
            for e in row.iter_mut() {
                *e = fx.eval(fx.call("simplify", &[*e]))?;
            }
        }
    }
    Ok(Some(to_term(fx, &r)))
}

fn matrix_function_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    // method: ... selects an algorithm; the result is the same function of the matrix, so it is accepted and the
    // form is chosen as without it
    if let Some(&o) = a.get(2) {
        let rule = fx.sym(mc_api::names::RULE);
        let is_method = fx.call_of(o, "options").is_some_and(|rs| {
            rs.iter().all(|&r| matches!(fx.h.view(r), View::App { head, args: &[k, _] } if head == rule && k == fx.sym("method")))
        });
        if !is_method {
            return Ok(None);
        }
    }
    matrix_function(fx, a[0], a[1])
}

/// exp(m) v for a numeric strictly triangular (so nilpotent) m: the series sum_k m^k v / k! stops at the first zero
/// vector (after at most n terms), so it is exact for exact entries and costs one sparse product per term (Taylor
/// series of the exponential; Higham, Functions of Matrices, 2008, ch. 10). `None` if m is not of that kind.
fn exp_nilpotent_action<'h>(fx: &mut Fx<'_, '_, '_, 'h>, m: &M<'h>, v: &[T<'h>]) -> Res<Option<Vec<T<'h>>>> {
    let n = m.len();
    if v.len() != n || m.iter().flatten().any(|&x| fx.num(x).is_none()) {
        return Ok(None);
    }
    let upper = (0..n).all(|i| (0..=i).all(|j| is_zero(fx, m[i][j])));
    let lower = (0..n).all(|i| (i..n).all(|j| is_zero(fx, m[i][j])));
    if !upper && !lower {
        return Ok(None);
    }
    let rows: Vec<Vec<(usize, T<'h>)>> =
        m.iter().map(|r| r.iter().enumerate().filter(|(_, &x)| !is_zero(fx, x)).map(|(j, &x)| (j, x)).collect()).collect();
    let zero = fx.int(0);
    let mut w: Vec<T<'h>> = v.to_vec();
    let mut sum: Vec<T<'h>> = v.to_vec();
    for k in 1..=n as i64 {
        let mut next = Vec::with_capacity(n);
        for r in &rows {
            let terms: Vec<T<'h>> =
                r.iter().filter(|&&(j, _)| !is_zero(fx, w[j])).map(|&(j, x)| fx.mul(&[x, w[j]])).collect();
            fx.tick(r.len() as u64 + 1)?;
            next.push(if terms.is_empty() { zero } else { fx.eval(fx.div(fx.add(&terms), fx.int(k)))? });
        }
        if next.iter().all(|&x| is_zero(fx, x)) {
            break;
        }
        for (s, &x) in sum.iter_mut().zip(&next) {
            if !is_zero(fx, x) {
                *s = fx.eval(fx.add(&[*s, x]))?;
            }
        }
        w = next;
    }
    Ok(Some(sum))
}

/// Machine numbers with exact numeric constants (4 pi, e): the whole matrix at machine precision, as arithmetic
/// with a machine number is (`m` itself when an entry has no numeric value).
fn numeric_entries<'h>(fx: &mut Fx<'_, '_, '_, 'h>, m: M<'h>) -> Res<M<'h>> {
    if machine_matrix(fx, &m) || !m.iter().flatten().any(|&x| fx.num(x).is_some_and(|v| v.kind() == Kind::Machine)) {
        return Ok(m);
    }
    let mut nm = m.clone();
    for e in nm.iter_mut().flatten() {
        if fx.num(*e).is_none() {
            *e = fx.eval(fx.call("n", &[*e]))?;
            if fx.num(*e).is_none() {
                return Ok(m);
            }
        }
    }
    Ok(nm)
}

/// The entries of `m` evaluated.
fn evaluated<'h>(fx: &mut Fx<'_, '_, '_, 'h>, m: M<'h>) -> Res<M<'h>> {
    let mut out = m;
    for e in out.iter_mut().flatten() {
        *e = fx.eval(*e)?;
    }
    Ok(out)
}

/// exp of a matrix of numbers, some inexact (machine numbers, or balls of at least `p` digits), by scaling and
/// squaring with the Taylor series (Moler and Van Loan, "Nineteen dubious ways to compute the exponential of a
/// matrix", 2003, method 3), carried out with 25 guard digits: the result is accurate to the input's precision
/// (machine numbers back as machine numbers). `None` for exact matrices and other entries.
fn numeric_exp<'h>(fx: &mut Fx<'_, '_, '_, 'h>, m: &M<'h>) -> Res<Option<M<'h>>> {
    let n = m.len();
    if n > 16 {
        return Ok(None);
    }
    let mut machine = false;
    let mut inexact = false;
    for &x in m.iter().flatten() {
        match fx.num(x) {
            Some(v) => {
                machine |= v.kind() == Kind::Machine;
                inexact |= v.kind() != Kind::Exact;
            }
            None => return Ok(None),
        }
    }
    if !inexact {
        return Ok(None);
    }
    let target = if machine {
        16.0
    } else {
        // the least precision of the balls (a zero ball has none)
        let mut least = f64::INFINITY;
        for &x in m.iter().flatten() {
            if fx.num(x).is_some_and(|v| v.kind() == Kind::Ball) {
                let p = fx.eval(fx.call("precision", &[x]))?;
                if let Some(p) = fx.f64_of(p).filter(|p| p.is_finite() && *p > 0.0) {
                    least = least.min(p);
                }
            }
        }
        if !least.is_finite() {
            return Ok(None);
        }
        least
    };
    let wp = (target + 25.0).ceil();
    let wpt = fx.int(wp as i64);
    let mut a = m.clone();
    let mut norm = 0.0f64;
    for row in a.iter_mut() {
        let mut s = 0.0;
        for e in row.iter_mut() {
            *e = fx.eval(fx.call("set_precision", &[*e, wpt]))?;
            let ab = fx.eval(fx.call("abs", &[*e]))?;
            s += fx.f64_of(ab).unwrap_or(f64::INFINITY);
        }
        norm = norm.max(s);
    }
    if !norm.is_finite() || norm > 1e6 {
        return Ok(None);
    }
    // ||a / 2^s|| <= 1/2
    let s = if norm > 0.5 { (norm.log2().ceil() as i64 + 1).max(0) } else { 0 };
    let scale = fx.pow(fx.int(2), fx.int(-s));
    for e in a.iter_mut().flatten() {
        *e = fx.eval(fx.mul(&[*e, scale]))?;
    }
    let tol = 10f64.powf(-wp);
    let mut sum = identity(fx, n);
    let mut term = identity(fx, n);
    for k in 1..400i64 {
        fx.tick((n * n * n) as u64)?;
        term = matmul(fx, &term, &a);
        let kk = fx.int(k);
        for e in term.iter_mut().flatten() {
            *e = fx.eval(fx.div(*e, kk))?;
        }
        let mut big = 0.0f64;
        for (srow, trow) in sum.iter_mut().zip(&term) {
            for (se, &te) in srow.iter_mut().zip(trow) {
                *se = fx.eval(fx.add(&[*se, te]))?;
                let ab = fx.eval(fx.call("abs", &[te]))?;
                big = big.max(fx.f64_of(ab).unwrap_or(f64::INFINITY));
            }
        }
        if big < tol {
            break;
        }
    }
    for _ in 0..s {
        fx.tick((n * n * n) as u64)?;
        let sq = matmul(fx, &sum, &sum);
        sum = evaluated(fx, sq)?;
    }
    let tp = fx.int(target.round().max(1.0) as i64);
    for e in sum.iter_mut().flatten() {
        *e = if machine { fx.eval(fx.call("n", &[*e]))? } else { fx.eval(fx.call("set_precision", &[*e, tp]))? };
    }
    Ok(Some(sum))
}

fn matrix_exp_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let f = fx.sym("exp");
    if a.len() == 1 {
        if let Some(m) = square(fx, a[0]) {
            let m = numeric_entries(fx, m)?;
            if let Some(r) = numeric_exp(fx, &m)? {
                return Ok(Some(to_term(fx, &r)));
            }
        }
    }
    if a.len() == 2 {
        if let (Some(m), Some(v)) = (square(fx, a[0]), fx.as_list(a[1]).map(|v| v.to_vec())) {
            if let Some(r) = exp_nilpotent_action(fx, &m, &v)? {
                return Ok(Some(fx.list(&r)));
            }
        }
        // matrix_exp(m, v) = matrix_exp(m) . v
        let Some(e) = matrix_function(fx, f, a[0])? else { return Ok(None) };
        return Ok(Some(fx.call("dot", &[e, a[1]])));
    }
    matrix_function(fx, f, a[0])
}

fn matrix_log_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    matrix_function(fx, fx.sym("log"), a[0])
}

fn matrix_power_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (mt, k) = (a[0], a[1]);
    let Some(m) = square(fx, mt) else { return Ok(None) };
    if let Some(k) = fx.int_of(k) {
        let base = if k < 0 {
            let inv = fx.eval(fx.call("inverse", &[mt]))?;
            match square(fx, inv) {
                Some(i) => i,
                None => return Ok(Some(inv)),
            }
        } else {
            m.clone()
        };
        // repeated squaring
        let mut e = k.unsigned_abs();
        let mut acc = identity(fx, m.len());
        let mut b = base;
        while e > 0 {
            fx.tick(m.len() as u64)?;
            if e & 1 == 1 {
                acc = matmul(fx, &acc, &b);
            }
            e >>= 1;
            if e > 0 {
                b = matmul(fx, &b, &b);
            }
        }
        return Ok(Some(to_term(fx, &acc)));
    }
    // a non-integer power: the matrix function x^k
    let x = fx.sym("matrix_power·x");
    let lam = fx.call("fn", &[fx.list(&[x]), fx.pow(x, k)]);
    matrix_function(fx, lam, mt)
}

fn conjugate_transpose_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if fx.as_list(a[0]).is_none() {
        return Ok(None);
    }
    Ok(Some(fx.call("conjugate", &[fx.call("transpose", a)])))
}

/// The arguments of a matrix constructor without its trailing `options(...)`, and the digits asked by
/// `working_precision` (`machine_precision` is a number near 15.95: machine numbers). `target_structure` may be
/// `"Dense"`, `"Sparse"` or `automatic` (all give the dense form); any other option or value gives `None`.
fn ctor_options<'h>(fx: &Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> Option<(Vec<T<'h>>, Option<T<'h>>)> {
    let Some((&last, rest)) = a.split_last() else { return Some((a.to_vec(), None)) };
    let Some(rules) = fx.call_of(last, "options") else { return Some((a.to_vec(), None)) };
    let mut digits = None;
    for &r in rules {
        let &[k, v] = fx.call_of(r, "rule")? else { return None };
        let View::Sym(name) = fx.h.view(k) else { return None };
        match fx.h.sym_name(name).as_str() {
            "target_structure" => match fx.h.view(v) {
                View::Str("Dense" | "Sparse") => {}
                View::Sym(s) if matches!(fx.h.sym_name(s).as_str(), "automatic" | "auto") => {}
                _ => return None,
            },
            "working_precision" => digits = Some(v),
            _ => return None,
        }
    }
    Some((rest.to_vec(), digits))
}

/// The matrix `m` at the working precision `digits` (an integer, or a machine number: machine precision).
fn at_working_precision<'h>(fx: &mut Fx<'_, '_, '_, 'h>, m: T<'h>, digits: Option<T<'h>>) -> R<'h> {
    Ok(Some(match digits {
        None => m,
        Some(d) if fx.int_of(d).is_some() => fx.eval(fx.call("n", &[m, d]))?,
        Some(_) => fx.eval(fx.call("n", &[m]))?,
    }))
}

fn hilbert_matrix_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some((a, digits)) = ctor_options(fx, a) else { return Ok(None) };
    let a = a.as_slice();
    let dims: Vec<i64> = match a {
        [n] => match fx.int_of(*n) {
            Some(n) => vec![n, n],
            None => match fx.as_list(*n) {
                Some(&[r, c]) => match (fx.int_of(r), fx.int_of(c)) {
                    (Some(r), Some(c)) => vec![r, c],
                    _ => return Ok(None),
                },
                _ => return Ok(None),
            },
        },
        [r, c] => match (fx.int_of(*r), fx.int_of(*c)) {
            (Some(r), Some(c)) => vec![r, c],
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    if dims.iter().any(|&d| !(1..=2000).contains(&d)) {
        return Ok(None);
    }
    fx.tick((dims[0] * dims[1]) as u64)?;
    let rows: Vec<T<'h>> = (1..=dims[0])
        .map(|i| {
            let r: Vec<T<'h>> = (1..=dims[1]).map(|j| fx.rat(1, i + j - 1)).collect();
            fx.list(&r)
        })
        .collect();
    let m = fx.list(&rows);
    at_working_precision(fx, m, digits)
}

fn rotation_matrix_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some((a, _)) = ctor_options(fx, a) else { return Ok(None) };
    let a = a.as_slice();
    let th = a[0];
    // rotation_matrix([u, v]): the rotation taking the direction of u to that of v (in the plane of u and v)
    if let (Some(&[u, v]), 1) = (fx.as_list(th), a.len()) {
        if let (Some(u), Some(v)) = (fx.as_list(u).map(<[T<'h>]>::to_vec), fx.as_list(v).map(<[T<'h>]>::to_vec)) {
            return rotation_between(fx, &u, &v);
        }
    }
    let c = fx.eval(fx.call("cos", &[th]))?;
    let s = fx.eval(fx.call("sin", &[th]))?;
    if a.len() == 1 {
        let rows = [fx.list(&[c, fx.neg(s)]), fx.list(&[s, c])];
        return Ok(Some(fx.list(&rows)));
    }
    // about the axis w (Rodrigues): c I + s [u]x + (1 - c) u u^T with u = w/|w|
    let Some(w) = fx.as_list(a[1]).filter(|w| w.len() == 3).map(|w| w.to_vec()) else { return Ok(None) };
    let nn = fx.add(&[fx.pow(w[0], fx.int(2)), fx.pow(w[1], fx.int(2)), fx.pow(w[2], fx.int(2))]);
    let inv = fx.pow(nn, fx.rat(-1, 2));
    let u: Vec<T<'h>> = w.iter().map(|&x| fx.mul(&[x, inv])).collect();
    let k = [[fx.int(0), fx.neg(u[2]), u[1]], [u[2], fx.int(0), fx.neg(u[0])], [fx.neg(u[1]), u[0], fx.int(0)]];
    let one_c = fx.sub(fx.int(1), c);
    let mut rows = vec![];
    for i in 0..3 {
        let mut r = vec![];
        for j in 0..3 {
            let d = if i == j { c } else { fx.int(0) };
            let e = fx.add(&[d, fx.mul(&[s, k[i][j]]), fx.mul(&[one_c, u[i], u[j]])]);
            r.push(fx.eval(fx.call("simplify", &[e]))?);
        }
        rows.push(fx.list(&r));
    }
    Ok(Some(fx.list(&rows)))
}

/// The rotation taking the direction of `u` to that of `v`, vectors of 2 or 3 entries: in the plane,
/// `[[c, -s], [s, c]]` with `c = u.v / (|u| |v|)`, `s = (u_1 v_2 - u_2 v_1) / (|u| |v|)`; in space, the rotation
/// about `w = u x v` by that angle (Rodrigues: `c I + [w]x / (|u| |v|) + (1 - c) w w^T / |w|^2`), the identity for
/// parallel vectors of one direction and `None` for opposite ones (the plane is not determined).
fn rotation_between<'h>(fx: &mut Fx<'_, '_, '_, 'h>, u: &[T<'h>], v: &[T<'h>]) -> R<'h> {
    if u.len() != v.len() || !(2..=3).contains(&u.len()) {
        return Ok(None);
    }
    // complex entries: the unitary rotation of the library rule (lib/lists.mc), not this real formula (whose u.u
    // can vanish)
    for &x in u.iter().chain(v) {
        let im = fx.eval(fx.call("im_part", &[x]))?;
        let nim = fx.eval(fx.call("n", &[im]))?;
        if !fx.is_int(im, 0) && fx.f64_of(nim) != Some(0.0) {
            return Ok(None);
        }
    }
    let dot = |a: &[T<'h>], b: &[T<'h>]| fx.add(&a.iter().zip(b).map(|(&x, &y)| fx.mul(&[x, y])).collect::<Vec<_>>());
    let nn = fx.mul(&[fx.pow(dot(u, u), fx.rat(1, 2)), fx.pow(dot(v, v), fx.rat(1, 2))]);
    let inv = fx.pow(nn, fx.int(-1));
    let c = fx.mul(&[dot(u, v), inv]);
    let simp = |fx: &mut Fx<'_, '_, '_, 'h>, e: T<'h>| fx.eval(fx.call("simplify", &[e]));
    if u.len() == 2 {
        let s = fx.mul(&[fx.sub(fx.mul(&[u[0], v[1]]), fx.mul(&[u[1], v[0]])), inv]);
        let (c, s) = (simp(fx, c)?, simp(fx, s)?);
        let rows = [fx.list(&[c, fx.neg(s)]), fx.list(&[s, c])];
        return Ok(Some(fx.list(&rows)));
    }
    let w = [
        fx.sub(fx.mul(&[u[1], v[2]]), fx.mul(&[u[2], v[1]])),
        fx.sub(fx.mul(&[u[2], v[0]]), fx.mul(&[u[0], v[2]])),
        fx.sub(fx.mul(&[u[0], v[1]]), fx.mul(&[u[1], v[0]])),
    ];
    let ww = fx.eval(dot(&w, &w))?;
    if is_zero(fx, ww) {
        let c = simp(fx, c)?;
        return Ok(fx.num(c).is_some_and(|n| !n.is_zero() && n.as_real().is_some_and(|r| r.to_f64() > 0.0)).then(|| to_term(fx, &identity(fx, 3))));
    }
    let k = [[fx.int(0), fx.neg(w[2]), w[1]], [w[2], fx.int(0), fx.neg(w[0])], [fx.neg(w[1]), w[0], fx.int(0)]];
    let one_c = fx.mul(&[fx.sub(fx.int(1), c), fx.pow(ww, fx.int(-1))]);
    let mut rows = vec![];
    for i in 0..3 {
        let mut r = vec![];
        for j in 0..3 {
            let d = if i == j { c } else { fx.int(0) };
            let e = fx.add(&[d, fx.mul(&[k[i][j], inv]), fx.mul(&[one_c, w[i], w[j]])]);
            r.push(simp(fx, e)?);
        }
        rows.push(fx.list(&r));
    }
    Ok(Some(fx.list(&rows)))
}

/// sparse_array(rules, dims, default) (decision P6N.14): the array is built dense (MathCAS has no sparse storage);
/// rules are `position -> value` with literal positions or patterns (`[i_, i_] -> 2`), the first matching rule
/// wins, dimensions default to the largest positions, the default value to 0; at most 10^6 elements.
/// `sparse_array(automatic, dims, default, [1, [offsets, columns], values])`, the compressed row form: row i (or
/// the one row of a vector) holds the entries offsets[i] + 1 .. offsets[i + 1], each at the remaining indices
/// `columns[k]` with the value `values[k]`; built dense like the rule form.
fn sparse_compressed<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let [_, dims_t, default, data] = *a else { return Ok(None) };
    let Some(dims) = fx.as_list(dims_t).and_then(|d| d.iter().map(|&x| fx.int_of(x).filter(|v| *v >= 1)).collect::<Option<Vec<i64>>>())
    else {
        return Ok(None);
    };
    let Some(&[_, structure, values]) = fx.as_list(data) else { return Ok(None) };
    let Some(&[offsets, columns]) = fx.as_list(structure) else { return Ok(None) };
    let (Some(offsets), Some(columns), Some(values)) = (fx.as_list(offsets), fx.as_list(columns), fx.as_list(values)) else {
        return Ok(None);
    };
    let Some(offsets) = offsets.iter().map(|&x| fx.int_of(x).filter(|v| *v >= 0)).collect::<Option<Vec<i64>>>() else {
        return Ok(None);
    };
    let total: i64 = dims.iter().product();
    let rows = if dims.len() == 1 { 1 } else { dims[0] as usize };
    if dims.is_empty() || total > 1_000_000 || offsets.len() != rows + 1 || columns.len() != values.len() {
        return Ok(None);
    }
    let mut rules = Vec::with_capacity(values.len());
    for r in 0..rows {
        let (lo, hi) = (offsets[r] as usize, offsets[r + 1] as usize);
        if lo > hi || hi > values.len() {
            return Ok(None);
        }
        for k in lo..hi {
            let Some(rest) = fx.as_list(columns[k]) else { return Ok(None) };
            let mut pos = if dims.len() == 1 { vec![] } else { vec![fx.int(r as i64 + 1)] };
            pos.extend_from_slice(rest);
            if pos.len() != dims.len() {
                return Ok(None);
            }
            rules.push(fx.call("rule", &[fx.list(&pos), values[k]]));
        }
    }
    if rules.is_empty() {
        // no explicit entries: every element is the default
        let mut level: Vec<T<'h>> = vec![default; total as usize];
        for &d in dims.iter().skip(1).rev() {
            level = level.chunks(d as usize).map(|c| fx.list(c)).collect();
        }
        return Ok(Some(fx.list(&level)));
    }
    let r = fx.list(&rules);
    sparse_array_e(fx, &[r, dims_t, default])
}

/// The nesting depth of lists of `t` along the first elements (0 for a non-list).
fn list_depth<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> usize {
    match fx.as_list(t) {
        Some(xs) if !xs.is_empty() => 1 + list_depth(fx, xs[0]),
        Some(_) => 1,
        None => 0,
    }
}

/// The entries of the nested list `t` of `left` more levels with their offsets from its corner (zero based).
fn collect_block<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, left: usize, idx: &mut Vec<i64>, out: &mut Vec<(Vec<i64>, T<'h>)>) {
    if left == 0 {
        out.push((idx.clone(), t));
        return;
    }
    if let Some(xs) = fx.as_list(t) {
        for (k, &x) in xs.iter().enumerate() {
            idx.push(k as i64);
            collect_block(fx, x, left - 1, idx, out);
            idx.pop();
        }
    }
}

/// The integer vector of a list of machine integers (a bare integer is a one-element vector).
fn int_vec<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<Vec<i64>> {
    match fx.as_list(t) {
        Some(xs) => xs.iter().map(|&x| fx.int_of(x)).collect(),
        None => fx.int_of(t).map(|v| vec![v]),
    }
}

/// Rules with a `band(start, end, step)` position (the positions start + k step, k = 0, 1, ..., within `end` and
/// the array's dimensions) written as rules with literal positions. The value is a scalar (every position of the
/// band), a vector (one value per position, from the start on) or one block / a list of blocks of the array's rank
/// (each block's corner at the band's next position, which advances by the block's size times the step).
/// `None`: a band that cannot be expanded (no end and no dimensions, no step, a bad value).
fn expand_bands<'h>(fx: &mut Fx<'_, '_, '_, 'h>, rules: &[T<'h>], dims: Option<&[i64]>) -> Res<Option<Vec<T<'h>>>> {
    let mut out = Vec::new();
    for &r in rules {
        let Some(&[l, v]) = fx.call_of(r, "rule").or_else(|| fx.call_of(r, "rule_delayed")) else {
            out.push(r);
            continue;
        };
        let Some(bargs) = fx.call_of(l, "band") else {
            out.push(r);
            continue;
        };
        let Some(mut start) = bargs.first().and_then(|&t| int_vec(fx, t)) else { return Ok(None) };
        let rank = start.len();
        // a negative position counts from the end of its dimension
        let from_end = |v: &mut Vec<i64>| {
            if let Some(d) = dims.filter(|d| d.len() == v.len()) {
                for (x, &dd) in v.iter_mut().zip(d) {
                    if *x < 0 && -*x <= dd {
                        *x += dd + 1;
                    }
                }
            }
        };
        from_end(&mut start);
        let auto = fx.sym("automatic");
        let end = match bargs.get(1) {
            Some(&t) if t != auto => match int_vec(fx, t) {
                Some(mut e) if e.len() == rank => {
                    from_end(&mut e);
                    Some(e)
                }
                _ => return Ok(None),
            },
            _ => None,
        };
        let step = match bargs.get(2) {
            None => vec![1; rank],
            Some(&t) => match fx.as_list(t) {
                Some(_) => match int_vec(fx, t) {
                    Some(s) if s.len() == rank => s,
                    _ => return Ok(None),
                },
                None => match fx.int_of(t) {
                    Some(s) => vec![s; rank],
                    None => return Ok(None),
                },
            },
        };
        if dims.is_some_and(|d| d.len() != rank) || step.iter().all(|&s| s == 0) {
            return Ok(None);
        }
        let inside = |pos: &[i64]| -> bool {
            pos.iter().enumerate().all(|(i, &p)| {
                p >= 1
                    && dims.is_none_or(|d| p <= d[i])
                    && end.as_ref().is_none_or(|e| match step[i].signum() {
                        1 => p <= e[i],
                        -1 => p >= e[i],
                        _ => true,
                    })
            })
        };
        let at = |t: i64| -> Vec<i64> { (0..rank).map(|i| start[i] + t * step[i]).collect() };
        let pos_term = |fx: &Fx<'_, '_, '_, 'h>, pos: &[i64]| -> T<'h> { fx.list(&pos.iter().map(|&p| fx.int(p)).collect::<Vec<_>>()) };
        let value = fx.eval(v)?;
        let depth = list_depth(fx, value);
        if depth == 0 {
            if end.is_none() && dims.is_none() {
                return Ok(None);
            }
            let mut t = 0;
            while inside(&at(t)) && t < 1_000_000 {
                fx.tick(1)?;
                out.push(fx.call("rule", &[pos_term(fx, &at(t)), value]));
                t += 1;
            }
        } else if depth == 1 {
            let Some(xs) = fx.as_list(value).map(<[_]>::to_vec) else { return Ok(None) };
            // the values repeat cyclically along a band with an explicit end that is longer than the list
            let bounded = end.is_some();
            let mut t = 0usize;
            while !xs.is_empty() && (bounded || t < xs.len()) && t < 1_000_000 {
                let pos = at(t as i64);
                if !inside(&pos) {
                    break;
                }
                fx.tick(1)?;
                out.push(fx.call("rule", &[pos_term(fx, &pos), xs[t % xs.len()]]));
                t += 1;
            }
        } else if depth == rank || depth == rank + 1 {
            let blocks: Vec<T<'h>> = if depth == rank { vec![value] } else { fx.as_list(value).map(<[_]>::to_vec).unwrap_or_default() };
            let mut corner = start.clone();
            // blocks repeat cyclically while their corner is inside the band (with an explicit end)
            let bounded = end.is_some();
            let rounds = if bounded { 1_000_000 } else { 1 };
            let blocks: Vec<T<'h>> = (0..rounds).flat_map(|_| blocks.iter().copied()).take(if bounded { 100_000 } else { blocks.len() }).collect();
            for b in blocks {
                if !inside(&corner) {
                    break;
                }
                let mut size = Vec::with_capacity(rank);
                let mut probe = b;
                for _ in 0..rank {
                    let Some(xs) = fx.as_list(probe) else { return Ok(None) };
                    size.push(xs.len() as i64);
                    match xs.first() {
                        Some(&f) => probe = f,
                        None => break,
                    }
                }
                let mut entries = Vec::new();
                collect_block(fx, b, rank, &mut Vec::new(), &mut entries);
                for (off, x) in entries {
                    let pos: Vec<i64> = (0..rank).map(|i| corner[i] + off[i]).collect();
                    if pos.iter().enumerate().all(|(i, &p)| p >= 1 && dims.is_none_or(|d| p <= d[i])) {
                        out.push(fx.call("rule", &[pos_term(fx, &pos), x]));
                    }
                }
                for i in 0..rank {
                    corner[i] += size.get(i).copied().unwrap_or(1) * step[i];
                }
            }
        } else {
            return Ok(None);
        }
    }
    Ok(Some(out))
}

/// A value of the small condition evaluator.
#[derive(Clone, Copy, PartialEq)]
enum Cv {
    I(i64),
    B(bool),
}

/// Evaluate a condition built of integers, the pattern variables `env` and the arithmetic and comparisons of
/// machine integers (`add`, `mul`, `abs`, `mod`, `power` with a small exponent, `less` ..., `and`, `or`, `not`)
/// without the evaluator; `None` for anything else (the evaluator then decides).
fn quick_condition<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, env: &[(T<'h>, i64)]) -> Option<Cv> {
    match fx.h.view(t) {
        View::Int(v) => Some(Cv::I(v)),
        View::Sym(_) => env.iter().find(|(s, _)| *s == t).map(|&(_, v)| Cv::I(v)),
        View::App { head, args } => {
            let View::Sym(hs) = fx.h.view(head) else { return None };
            let name = fx.h.sym_name(hs);
            let int = |u: T<'h>| match quick_condition(fx, u, env)? {
                Cv::I(v) => Some(v),
                Cv::B(_) => None,
            };
            let boolean = |u: T<'h>| match quick_condition(fx, u, env)? {
                Cv::B(v) => Some(v),
                Cv::I(_) => None,
            };
            match (name.as_str(), args) {
                ("add", _) => args.iter().try_fold(0i64, |a, &u| a.checked_add(int(u)?)).map(Cv::I),
                ("mul", _) => args.iter().try_fold(1i64, |a, &u| a.checked_mul(int(u)?)).map(Cv::I),
                ("abs", &[u]) => int(u)?.checked_abs().map(Cv::I),
                ("mod", &[a, m]) => {
                    let (a, m) = (int(a)?, int(m)?);
                    (m != 0).then(|| Cv::I(a.rem_euclid(m.abs()) + if m < 0 && a.rem_euclid(m.abs()) != 0 { m } else { 0 }))
                }
                ("power", &[a, e]) => {
                    let (a, e) = (int(a)?, int(e)?);
                    u32::try_from(e).ok().and_then(|e| a.checked_pow(e)).map(Cv::I)
                }
                ("equal", &[a, b]) => Some(Cv::B(int(a)? == int(b)?)),
                ("unequal", &[a, b]) => Some(Cv::B(int(a)? != int(b)?)),
                ("less", &[a, b]) => Some(Cv::B(int(a)? < int(b)?)),
                ("less_equal", &[a, b]) => Some(Cv::B(int(a)? <= int(b)?)),
                ("greater", &[a, b]) => Some(Cv::B(int(a)? > int(b)?)),
                ("greater_equal", &[a, b]) => Some(Cv::B(int(a)? >= int(b)?)),
                ("and", _) => args.iter().try_fold(true, |a, &u| Some(a && boolean(u)?)).map(Cv::B),
                ("or", _) => args.iter().try_fold(false, |a, &u| Some(a || boolean(u)?)).map(Cv::B),
                ("not", &[u]) => Some(Cv::B(!boolean(u)?)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The variables and the condition of a position pattern `[i_, j_]` or `[i_, j_] when cond` (`i_` a named blank).
fn simple_position_pattern<'h>(fx: &Fx<'_, '_, '_, 'h>, p: T<'h>, rank: usize) -> Option<(Vec<T<'h>>, Option<T<'h>>)> {
    let (list, cond) = match fx.call_of(p, "condition") {
        Some(&[l, c]) => (l, Some(c)),
        _ => (p, None),
    };
    let blank = |q: T<'h>| match fx.call_of(q, "any") {
        Some(&[v]) if matches!(fx.h.view(v), View::Sym(_)) => Some(v),
        _ => None,
    };
    let vars: Vec<T<'h>> = if rank == 1 && fx.as_list(list).is_none() { vec![blank(list)?] } else { fx.as_list(list)?.iter().map(|&q| blank(q)).collect::<Option<_>>()? };
    (vars.len() == rank).then_some((vars, cond))
}

fn sparse_array_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a.len() == 4 && a[0] == fx.sym("automatic") {
        return sparse_compressed(fx, a);
    }
    if a.len() == 4 {
        return Ok(None);
    }
    let rules_t = a[0];
    let Some(rules) = fx.as_list(rules_t).map(|r| r.to_vec()) else {
        // a single rule
        if fx.call_of(rules_t, "rule").is_some() || fx.call_of(rules_t, "rule_delayed").is_some() {
            let mut b = a.to_vec();
            b[0] = fx.list(&[rules_t]);
            return sparse_array_e(fx, &b);
        }
        return Ok(None);
    };
    let is_rule = |fx: &Fx<'_, '_, '_, 'h>, r: T<'h>| fx.call_of(r, "rule").or_else(|| fx.call_of(r, "rule_delayed"));
    if rules.is_empty() || !rules.iter().all(|&r| is_rule(fx, r).is_some()) {
        // a dense array: itself
        return Ok((a.len() == 1).then_some(rules_t));
    }
    // band positions become rules with literal positions
    let has_band = rules.iter().any(|&r| is_rule(fx, r).is_some_and(|p| fx.call_of(p[0], "band").is_some()));
    let rules = if has_band {
        let first_rank = rules
            .iter()
            .find_map(|&r| is_rule(fx, r).and_then(|p| fx.call_of(p[0], "band")).and_then(|b| b.first().and_then(|&t| int_vec(fx, t))))
            .map_or(1, |s| s.len());
        let dims_hint: Option<Vec<i64>> = a.get(1).and_then(|&d| match fx.as_list(d) {
            Some(_) => int_vec(fx, d),
            None => fx.int_of(d).map(|v| vec![v; first_rank]),
        });
        match expand_bands(fx, &rules, dims_hint.as_deref())? {
            Some(r) if !r.is_empty() => r,
            _ => return Ok(None),
        }
    } else {
        rules
    };
    // a rule `[_, ..., _] -> v` stands for all the other positions: the default value
    let all_any = |fx: &Fx<'_, '_, '_, 'h>, p: T<'h>| fx.as_list(p).is_some_and(|ps| !ps.is_empty() && ps.iter().all(|&q| fx.call_of(q, "any").is_some_and(|a| a.is_empty())));
    let mut rules = rules;
    let mut default_rule = None;
    if rules.len() > 1 {
        if let Some(k) = rules.iter().position(|&r| is_rule(fx, r).is_some_and(|p| all_any(fx, p[0]))) {
            default_rule = is_rule(fx, rules[k]).map(|p| p[1]);
            rules.remove(k);
        }
    }
    let lhs: Vec<T<'h>> = rules.iter().map(|&r| is_rule(fx, r).expect("a rule")[0]).collect();
    // literal positions
    let literal = |fx: &Fx<'_, '_, '_, 'h>, p: T<'h>| -> Option<Vec<i64>> {
        match fx.as_list(p) {
            Some(ix) => ix.iter().map(|&i| fx.int_of(i).filter(|v| *v >= 1)).collect(),
            None => fx.int_of(p).filter(|v| *v >= 1).map(|v| vec![v]),
        }
    };
    let dims: Vec<i64> = match a.get(1) {
        Some(&d) => match fx.as_list(d) {
            Some(ds) => match ds.iter().map(|&x| fx.int_of(x)).collect::<Option<Vec<i64>>>() {
                Some(v) => v,
                None => return Ok(None),
            },
            // one size n: an n x ... x n array of the rank of the positions
            None => match fx.int_of(d) {
                Some(v) => {
                    // a list position, or one under a condition (`[i_, j_] when ...`)
                    let pos_list = |p: T<'h>| match fx.h.view(p) {
                        View::App { args, .. } if fx.as_list(p).is_none() => args.first().and_then(|&q| fx.as_list(q)),
                        _ => fx.as_list(p),
                    };
                    let rank = lhs.first().and_then(|&p| pos_list(p)).map_or(1, |l| l.len().max(1));
                    vec![v; rank]
                }
                None => return Ok(None),
            },
        },
        None => {
            let ps: Option<Vec<Vec<i64>>> = lhs.iter().map(|&p| literal(fx, p)).collect();
            let Some(ps) = ps else { return Ok(None) };
            let k = ps[0].len();
            if ps.iter().any(|p| p.len() != k) {
                return Ok(None);
            }
            (0..k).map(|j| ps.iter().map(|p| p[j]).max().unwrap_or(1)).collect()
        }
    };
    let total: i64 = dims.iter().product();
    if dims.is_empty() || dims.iter().any(|&d| d < 1) || total > 1_000_000 {
        return Ok(None);
    }
    let default = match (a.get(2).copied(), default_rule) {
        (Some(d), _) => d,
        (None, Some(d)) => fx.eval(d)?,
        (None, None) => fx.int(0),
    };
    // a negative position counts from the end of its dimension (-1 is the last element)
    let signed = |fx: &Fx<'_, '_, '_, 'h>, p: T<'h>| -> Option<Vec<i64>> {
        let ix: Vec<i64> = match fx.as_list(p) {
            Some(ix) => ix.iter().map(|&i| fx.int_of(i)).collect::<Option<_>>()?,
            None => vec![fx.int_of(p)?],
        };
        if ix.len() != dims.len() {
            return None;
        }
        ix.iter().zip(&dims).map(|(&v, &d)| if v < 0 && -v <= d { Some(d + v + 1) } else { Some(v).filter(|v| *v >= 1) }).collect()
    };
    let lits: Vec<Option<Vec<i64>>> = lhs.iter().map(|&p| literal(fx, p).or_else(|| signed(fx, p))).collect();
    let all_literal = lits.iter().all(Option::is_some);
    let mut flat = vec![default; total as usize];
    let index = |pos: &[i64]| -> Option<usize> {
        if pos.len() != dims.len() || pos.iter().zip(&dims).any(|(p, d)| *p > *d) {
            return None;
        }
        Some(pos.iter().zip(&dims).fold(0usize, |acc, (p, d)| acc * *d as usize + (*p - 1) as usize))
    };
    if all_literal {
        // the first rule for a position wins: fill in reverse
        for (k, l) in lits.iter().enumerate().rev() {
            if let Some(i) = index(l.as_deref().expect("literal")) {
                let v = is_rule(fx, rules[k]).expect("a rule")[1];
                flat[i] = fx.eval(v)?;
            }
        }
    } else {
        let quick: Vec<Option<(Vec<T<'h>>, Option<T<'h>>)>> = lhs.iter().map(|&p| simple_position_pattern(fx, p, dims.len())).collect();
        for (i, slot) in flat.iter_mut().enumerate() {
            fx.tick(rules.len() as u64)?;
            // the position of flat index i
            let mut rem = i;
            let mut pos = vec![0i64; dims.len()];
            for j in (0..dims.len()).rev() {
                pos[j] = (rem % dims[j] as usize) as i64 + 1;
                rem /= dims[j] as usize;
            }
            let pt = if dims.len() == 1 { fx.int(pos[0]) } else { fx.list(&pos.iter().map(|&v| fx.int(v)).collect::<Vec<_>>()) };
            for (k, &r) in rules.iter().enumerate() {
                let hit = match &lits[k] {
                    Some(l) => *l == pos,
                    None => match &quick[k] {
                        Some(q) => {
                            // a repeated variable must take the same value at each of its places
                            let mut env: Vec<(T<'h>, i64)> = Vec::with_capacity(q.0.len());
                            let mut ok = true;
                            for (v, &x) in q.0.iter().zip(&pos) {
                                match env.iter().find(|(s, _)| s == v) {
                                    Some(&(_, y)) if y != x => ok = false,
                                    Some(_) => {}
                                    None => env.push((*v, x)),
                                }
                            }
                            match (ok, q.1) {
                                (false, _) => false,
                                (true, None) => true,
                                (true, Some(c)) => match quick_condition(fx, c, &env) {
                                    Some(Cv::B(b)) => b,
                                    _ => {
                                        let m = fx.eval(fx.call("match_q", &[pt, lhs[k]]))?;
                                        m == fx.bool(true)
                                    }
                                },
                            }
                        }
                        None => {
                            let m = fx.eval(fx.call("match_q", &[pt, lhs[k]]))?;
                            m == fx.bool(true)
                        }
                    },
                };
                if hit {
                    *slot = fx.eval(fx.call("rewrite", &[pt, r]))?;
                    break;
                }
            }
        }
    }
    // nest
    let mut level: Vec<T<'h>> = flat;
    for &d in dims.iter().skip(1).rev() {
        level = level.chunks(d as usize).map(|c| fx.list(c)).collect();
    }
    Ok(Some(fx.list(&level)))
}

macro_rules! def {
    ($name:literal, $ar:expr, $e:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: false, exact: Some($e), numeric: None, summary: $s }
    };
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("matrix_function", &[2, 3], matrix_function_e, "matrix_function(f, m): the matrix function f(m) (diagonal, triangular, eigenvector or interpolation forms)"),
    def!("matrix_exp", &[1, 2], matrix_exp_e, "matrix_exp(m): the matrix exponential; matrix_exp(m, v) its product with v"),
    def!("matrix_log", &[1], matrix_log_e, "matrix_log(m): the principal matrix logarithm"),
    def!("matrix_power", &[2], matrix_power_e, "matrix_power(m, k): the k-th matrix power (integer k by repeated squaring, negative k through the inverse)"),
    def!("conjugate_transpose", &[1], conjugate_transpose_e, "conjugate_transpose(m): the conjugate of the transpose of m"),
    def!("hilbert_matrix", &[1, 2, 3], hilbert_matrix_e, "hilbert_matrix(n): the n x n Hilbert matrix 1/(i + j - 1); hilbert_matrix([m, n]) rectangular"),
    def!("sparse_array", &[1, 2, 3, 4], sparse_array_e, "sparse_array(rules, dims, default): the array with the values of position -> value rules (literal or pattern positions), built dense; sparse_array(automatic, dims, default, data): the compressed row form"),
    def!("rotation_matrix", &[1, 2, 3], rotation_matrix_e, "rotation_matrix(theta): the 2D rotation by theta; rotation_matrix(theta, w): the 3D rotation about the axis w"),
];
