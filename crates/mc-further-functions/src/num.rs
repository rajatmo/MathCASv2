//! Machine numbers out of terms and back: the shared helpers of the natives of this crate.
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Res};

/// A complex machine number.
pub type C = (f64, f64);

/// The machine value of a term that is a number (after `n`), real or complex.
pub fn complex_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<C>> {
    let h = cx.heap();
    if let Some(v) = plain(h, t) {
        return Ok(Some(v));
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(plain(h, v))
}

/// The value of a number term without evaluating: integers, machine reals and complex numbers of those.
fn plain(h: &Heap, t: T<'_>) -> Option<C> {
    match h.view(t) {
        View::Int(a) => Some((a as f64, 0.0)),
        View::Real(a) => Some((a, 0.0)),
        View::App { head, args: &[a, b] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "complex") => {
            Some((real(h, a)?, real(h, b)?))
        }
        _ => None,
    }
}

fn real(h: &Heap, t: T<'_>) -> Option<f64> {
    match h.view(t) {
        View::Int(a) => Some(a as f64),
        View::Real(a) => Some(a),
        _ => None,
    }
}

/// A list of numbers as machine complex numbers.
pub fn complexes_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<C>>> {
    let Some(items) = cx.as_list(t).map(|l| l.to_vec()) else { return Ok(None) };
    let mut out = Vec::with_capacity(items.len());
    for e in items {
        match complex_of(cx, e)? {
            Some(v) => out.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// The term of a complex machine number (a real one when the imaginary part is 0).
pub fn term<'h>(h: &'h Heap, v: C) -> T<'h> {
    if v.1 == 0.0 {
        h.real(v.0)
    } else {
        h.call(h.sym("complex"), &[h.real(v.0), h.real(v.1)])
    }
}

pub fn add(a: C, b: C) -> C {
    (a.0 + b.0, a.1 + b.1)
}
pub fn sub(a: C, b: C) -> C {
    (a.0 - b.0, a.1 - b.1)
}
pub fn mul(a: C, b: C) -> C {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}
pub fn div(a: C, b: C) -> C {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}
pub fn abs(a: C) -> f64 {
    a.0.hypot(a.1)
}

/// The value of the polynomial with coefficients `c` (constant first) at z, by Horner's rule.
pub fn horner(c: &[C], z: C) -> C {
    c.iter().rev().fold((0.0, 0.0), |acc, &k| add(mul(acc, z), k))
}

/// The coefficients of the derivative.
pub fn deriv(c: &[C]) -> Vec<C> {
    c.iter().enumerate().skip(1).map(|(k, &v)| (v.0 * k as f64, v.1 * k as f64)).collect()
}

/// The coefficients of a product of polynomials.
pub fn pmul(a: &[C], b: &[C]) -> Vec<C> {
    if a.is_empty() || b.is_empty() {
        return vec![];
    }
    let mut out = vec![(0.0, 0.0); a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            out[i + j] = add(out[i + j], mul(x, y));
        }
    }
    out
}

/// The coefficients of a - b.
pub fn psub(a: &[C], b: &[C]) -> Vec<C> {
    let n = a.len().max(b.len());
    (0..n).map(|k| sub(a.get(k).copied().unwrap_or((0.0, 0.0)), b.get(k).copied().unwrap_or((0.0, 0.0)))).collect()
}

/// The coefficients with trailing (top-degree) zeros removed.
pub fn trim(mut c: Vec<C>) -> Vec<C> {
    let scale = c.iter().map(|&v| abs(v)).fold(0.0, f64::max);
    while c.last().is_some_and(|&v| abs(v) <= 1e-14 * scale.max(1e-300)) {
        c.pop();
    }
    c
}

/// All complex roots of the polynomial (constant first) by the Aberth-Ehrlich iteration.
pub fn roots(c: &[C]) -> Vec<C> {
    let c = trim(c.to_vec());
    let d = c.len().saturating_sub(1);
    if d == 0 {
        return vec![];
    }
    let lead = c[d];
    let mon: Vec<C> = c.iter().map(|&v| div(v, lead)).collect();
    let dc = deriv(&mon);
    // the Cauchy bound for the starting circle
    let r = 1.0 + mon[..d].iter().map(|&v| abs(v)).fold(0.0, f64::max);
    let mut z: Vec<C> = (0..d)
        .map(|k| {
            let t = 2.0 * std::f64::consts::PI * k as f64 / d as f64 + 0.4;
            (0.5 * r * t.cos(), 0.5 * r * t.sin())
        })
        .collect();
    for _ in 0..500 {
        let mut moved = 0.0f64;
        for i in 0..d {
            let p = horner(&mon, z[i]);
            let dp = horner(&dc, z[i]);
            if abs(p) == 0.0 {
                continue;
            }
            let ratio = div(p, dp);
            let mut s = (0.0, 0.0);
            for j in 0..d {
                if j != i {
                    s = add(s, div((1.0, 0.0), sub(z[i], z[j])));
                }
            }
            let w = div(ratio, sub((1.0, 0.0), mul(ratio, s)));
            if w.0.is_finite() && w.1.is_finite() {
                z[i] = sub(z[i], w);
                moved = moved.max(abs(w) / abs(z[i]).max(1.0));
            }
        }
        if moved < 1e-15 {
            break;
        }
    }
    z
}
