//! Checks of closed forms before they are returned.
//!
//! A definite sum with symbolic bounds or parameters is compared with the sum added term by term at a few points:
//! the bound variables take small integer values and the other symbols fixed rationals that are not integers (so
//! that no `gamma` pole is hit by accident). Equal exact values pass; otherwise the difference is evaluated with
//! 30 digits of ball arithmetic and must be below `1e-20` in absolute value. Points where either side is singular
//! are skipped; at least two points must agree, and none may disagree.
use crate::cx::Sx;
use mc_api::{names, Store, View, T};
use mc_eval::Res;

/// Values given to the parameters, in turn.
const PARAMS: &[(i64, i64)] = &[(3, 7), (5, 11), (2, 13), (7, 9), (4, 15), (11, 17)];
/// Symbols that are constants, not parameters.
const CONSTANTS: &[&str] = &["pi", "e", "euler_gamma", "catalan", "golden_ratio", "inf", "complex_infinity", "indeterminate", "degree", "glaisher", "khinchin", "true", "false"];

/// The symbols of `t` in argument positions, other than the constants and `except` (in order of appearance).
pub fn symbols<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>, except: &[T<'h>]) -> Vec<T<'h>> {
    let h = sx.h();
    let mut out = vec![];
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) => {
                let name = h.sym_name(s);
                if !except.contains(&u) && !out.contains(&u) && !CONSTANTS.contains(&name.as_str()) && !name.contains('$') {
                    out.push(u);
                }
            }
            View::App { args, .. } => stack.extend(args.iter().rev()),
            _ => {}
        }
    }
    out
}

/// Is `a - b` zero (exactly, or within 1e-20 at 30 digits)? `None` when undecided (a side is singular).
pub fn equal<'h>(sx: &mut Sx<'_, '_, '_, 'h>, a: T<'h>, b: T<'h>) -> Res<Option<bool>> {
    if sx.bad(a) || sx.bad(b) {
        return Ok(None);
    }
    let d = sx.sub(a, b)?;
    if d == sx.int(0) {
        return Ok(Some(true));
    }
    let dn = sx.ev("n", &[d, sx.int(30)])?;
    // a value that is exactly zero does not reach the precision asked for: its ball shrinks around 0
    if let Some(&[tag, _]) = sx.args_of(dn, names::FAILURE) {
        return Ok(if tag == sx.sym("precision_not_reached") { Some(true) } else { None });
    }
    if sx.bad(dn) {
        return Ok(None);
    }
    // the tolerance scales with the size of the values compared
    let an = sx.ev("n", &[a, sx.int(30)])?;
    let scale = sx.ev("abs", &[an])?;
    let scale = sx.ev("max", &[scale, sx.int(1)])?;
    let tol = sx.mul(&[scale, sx.h().real(1e-20)])?;
    let ad = sx.ev("abs", &[dn])?;
    let small = sx.ev("less", &[ad, tol])?;
    if sx.is_true(small) {
        return Ok(Some(true));
    }
    let big = sx.ev("greater", &[ad, tol])?;
    if sx.is_true(big) {
        return Ok(Some(false));
    }
    Ok(None)
}

/// Does `closed` equal `sum(f, [k, lo, hi])` at the check points? `vars` are the symbols of the bounds (given
/// integers `base, base + 1, ...`), the other symbols are parameters.
pub fn check_sum<'h>(sx: &mut Sx<'_, '_, '_, 'h>, f: T<'h>, k: T<'h>, lo: T<'h>, hi: T<'h>, closed: T<'h>) -> Res<bool> {
    let bound_vars = {
        let mut v = symbols(sx, lo, &[k]);
        for s in symbols(sx, hi, &[k]) {
            if !v.contains(&s) {
                v.push(s);
            }
        }
        v
    };
    let mut params = symbols(sx, f, &[k]);
    for s in symbols(sx, closed, &[k]) {
        if !params.contains(&s) {
            params.push(s);
        }
    }
    params.retain(|p| !bound_vars.contains(p));
    let mut agreed = 0;
    let mut tried = 0;
    // the bound variables from 1: closed forms are generic (an identity may fail at 0, as sum((-1)^k binomial(n, k)) = 0)
    for trial in 1..9i64 {
        sx.cx.tick(1)?;
        let mut rules = vec![];
        for (i, &v) in bound_vars.iter().enumerate() {
            rules.push(sx.rule(v, sx.int(trial + i as i64)));
        }
        for (i, &p) in params.iter().enumerate() {
            let (a, b) = PARAMS[(i + trial as usize) % PARAMS.len()];
            let q = sx.div(sx.int(a), sx.int(b))?;
            rules.push(sx.rule(p, q));
        }
        let lo_v = sx.subst_all(lo, &rules)?;
        let hi_v = sx.subst_all(hi, &rules)?;
        let (Some(l), Some(u)) = (sx.as_int(lo_v), sx.as_int(hi_v)) else { continue };
        if u - l > 400 || u < l - 1 {
            continue;
        }
        tried += 1;
        let fv = sx.subst_all(f, &rules)?;
        let mut acc = sx.int(0);
        let mut ok = true;
        for j in l..=u {
            let tj = sx.subst(fv, k, sx.int(j))?;
            if sx.bad(tj) {
                ok = false;
                break;
            }
            acc = sx.add(&[acc, tj])?;
        }
        if !ok {
            continue;
        }
        let cv = sx.subst_all(closed, &rules)?;
        match equal(sx, acc, cv)? {
            Some(true) => agreed += 1,
            Some(false) => return Ok(false),
            None => {}
        }
        if agreed >= 4 {
            break;
        }
    }
    let _ = tried;
    Ok(agreed >= 2 || (agreed >= 1 && bound_vars.is_empty() && params.is_empty()))
}

/// The value of `t` as a machine real or complex at the parameter values of trial `trial` (the symbols of `t`
/// other than `except` get fixed rationals); `None` when it is not numeric.
pub fn at_params<'h>(sx: &mut Sx<'_, '_, '_, 'h>, t: T<'h>, params: &[T<'h>], trial: usize) -> Res<T<'h>> {
    let mut rules = vec![];
    for (i, &p) in params.iter().enumerate() {
        let (a, b) = PARAMS[(i + trial) % PARAMS.len()];
        let q = sx.div(sx.int(a), sx.int(b))?;
        rules.push(sx.rule(p, q));
    }
    sx.subst_all(t, &rules)
}

/// Is `t` the symbol `name`?
pub fn is_sym<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>, name: &str) -> bool {
    t == sx.sym(name)
}

/// `-inf`?
pub fn is_neg_inf<'h>(sx: &Sx<'_, '_, '_, 'h>, t: T<'h>) -> bool {
    let h = sx.h();
    matches!(h.view(t), View::App { head, args: &[a, b] } if head == sx.sym(names::MUL) && a == h.int(-1) && b == sx.sym("inf"))
}
