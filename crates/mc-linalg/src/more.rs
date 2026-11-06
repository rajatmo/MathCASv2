//! Matrix functions and constructors defined by formulas (`mc_numeric::formula`, package p6-numeric):
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

fn matrix_exp_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let f = fx.sym("exp");
    if a.len() == 2 {
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

fn hilbert_matrix_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
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
    Ok(Some(fx.list(&rows)))
}

fn rotation_matrix_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let th = a[0];
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
    let default = a.get(2).copied().unwrap_or_else(|| fx.int(0));
    let lits: Vec<Option<Vec<i64>>> = lhs.iter().map(|&p| literal(fx, p)).collect();
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
                    None => {
                        let m = fx.eval(fx.call("match_q", &[pt, lhs[k]]))?;
                        m == fx.bool(true)
                    }
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
    def!("hilbert_matrix", &[1, 2], hilbert_matrix_e, "hilbert_matrix(n): the n x n Hilbert matrix 1/(i + j - 1); hilbert_matrix([m, n]) rectangular"),
    def!("sparse_array", &[1, 2, 3, 4], sparse_array_e, "sparse_array(rules, dims, default): the array with the values of position -> value rules (literal or pattern positions), built dense; sparse_array(automatic, dims, default, data): the compressed row form"),
    def!("rotation_matrix", &[1, 2], rotation_matrix_e, "rotation_matrix(theta): the 2D rotation by theta; rotation_matrix(theta, w): the 3D rotation about the axis w"),
];
