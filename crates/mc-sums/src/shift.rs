//! Shifted function values written in terms of the unshifted ones: `gamma(u + 3) = u (u + 1) (u + 2) gamma(u)`,
//! `digamma(u + 1) = digamma(u) + 1/u`, `fibonacci(u + 5) = 3 fibonacci(u) + 5 fibonacci(u + 1)`, ... (the
//! recurrences of the functions), used by `discrete_shift`, `difference_delta` and `discrete_ratio`.
use crate::cx::Sx;
use mc_api::{names, Store, View, T};
use mc_eval::Res;

/// The most steps one shift is expanded by.
const MAX_SHIFT: i64 = 32;

/// `u = v + m` with `m` the integer constant term of `u` (0 when there is none).
pub fn offset<'h>(sx: &mut Sx<'_, '_, '_, 'h>, u: T<'h>) -> Res<(T<'h>, i64)> {
    let h = sx.h();
    let u = sx.ev("expand", &[u])?;
    if let View::App { head, args } = h.view(u) {
        if head == sx.sym(names::ADD) {
            if let Some(m) = sx.as_int(args[0]) {
                let v = sx.plus(u, -m)?;
                return Ok((v, m));
            }
        }
    }
    Ok((u, 0))
}

/// `t` with the shifted values of known functions of arguments containing `k` reduced.
pub fn reduce<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, k: T<'h>) -> Res<T<'h>> {
    let h = sx.h();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(reduce(sx, a, k)?);
    }
    let name = match h.view(head) {
        View::Sym(s) => h.sym_name(s),
        _ => String::new(),
    };
    if let Some(v) = one(sx, &name, &new, k)? {
        return Ok(v);
    }
    if new.as_slice() == args {
        return Ok(t);
    }
    sx.eval(h.app(head, &new))
}

/// One call `name(args)` reduced, or `None`.
fn one<'h>(sx: &mut Sx<'_, '_, '_, 'h>, name: &str, args: &[T<'h>], k: T<'h>) -> Res<Option<T<'h>>> {
    // the argument that is shifted
    let pos = match name {
        "gamma" | "factorial" | "digamma" | "fibonacci" | "floor" | "ceiling" | "round" | "fractional_part" | "catalan_number" | "harmonic_number" => 0,
        "polygamma" | "zeta" if args.len() == 2 => 1,
        _ => return Ok(None),
    };
    let Some(&u) = args.get(pos) else { return Ok(None) };
    if sx.free(u, k) {
        return Ok(None);
    }
    let (v, m) = offset(sx, u)?;
    if m == 0 || m.abs() > MAX_SHIFT {
        return Ok(None);
    }
    let at = |sx: &mut Sx<'_, '_, '_, 'h>, j: i64| sx.plus(v, j);
    let mut a = args.to_vec();
    a[pos] = v;
    let base = sx.ev(name, &a)?;
    Ok(Some(match name {
        "floor" | "ceiling" | "round" => sx.plus(base, m)?,
        "fractional_part" => base,
        "gamma" | "factorial" => {
            // gamma(v + m) = gamma(v) prod_{j=0}^{m-1} (v + j); factorial: j from 1
            let (lo, hi) = if name == "gamma" { (0, m - 1) } else { (1, m) };
            let mut fs = vec![];
            if m > 0 {
                for j in lo..=hi {
                    fs.push(at(sx, j)?);
                }
                let p = sx.mul(&fs)?;
                let p = sx.ev("expand", &[p])?;
                sx.mul(&[p, base])?
            } else {
                // gamma(v - m') = gamma(v) / prod_{j=1}^{m'} (v - j); factorial: j from 0
                let (lo, hi) = if name == "gamma" { (1, -m) } else { (0, -m - 1) };
                for j in lo..=hi {
                    fs.push(at(sx, -j)?);
                }
                let p = sx.mul(&fs)?;
                sx.div(base, p)?
            }
        }
        "digamma" | "polygamma" | "zeta" | "harmonic_number" => {
            // the terms 1/(v + j)^s added (m > 0) or removed (m < 0)
            let (s, sign, lo): (i64, i64, i64) = match name {
                "digamma" => (1, 1, 0),
                "polygamma" => {
                    let Some(s) = sx.as_int(args[0]) else { return Ok(None) };
                    let f = (1..=s).product::<i64>();
                    (s + 1, if s % 2 == 0 { f } else { -f }, 0)
                }
                "zeta" => {
                    let Some(s) = sx.as_int(args[0]) else { return Ok(None) };
                    (s, -1, 0)
                }
                _ => {
                    let r = match args.get(1) {
                        Some(&r) => match sx.as_int(r) {
                            Some(r) => r,
                            None => return Ok(None),
                        },
                        None => 1,
                    };
                    (r, 1, 1)
                }
            };
            let mut terms = vec![base];
            let (from, to, sg) = if m > 0 { (lo, lo + m - 1, sign) } else { (lo + m, lo - 1, -sign) };
            for j in from..=to {
                let x = at(sx, j)?;
                let p = sx.pow(x, sx.int(-s))?;
                terms.push(sx.mul(&[sx.int(sg), p])?);
            }
            sx.add(&terms)?
        }
        "fibonacci" if m >= 2 => {
            // F(v + m) = F(m - 1) F(v) + F(m) F(v + 1)
            let fm1 = sx.ev("fibonacci", &[sx.int(m - 1)])?;
            let fm = sx.ev("fibonacci", &[sx.int(m)])?;
            let v1 = at(sx, 1)?;
            let f1 = sx.ev("fibonacci", &[v1])?;
            let a = sx.mul(&[fm1, base])?;
            let b = sx.mul(&[fm, f1])?;
            sx.add(&[a, b])?
        }
        "catalan_number" if m > 0 => {
            // C(v + 1) = 2 (2 v + 1) / (v + 2) C(v)
            let mut fs = vec![base];
            for j in 0..m {
                let x = at(sx, j)?;
                let x2 = sx.mul(&[sx.int(2), x])?;
                let x21 = sx.plus(x2, 1)?;
                let num = sx.mul(&[sx.int(2), x21])?;
                let den = sx.plus(x, 2)?;
                fs.push(sx.div(num, den)?);
            }
            sx.mul(&fs)?
        }
        _ => return Ok(None),
    }))
}
