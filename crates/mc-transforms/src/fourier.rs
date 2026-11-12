//! Fourier transforms: the exponential transform and its inverse in the unitary convention (decision 2C.10),
//! `F(w) = 1/sqrt(2 pi) integrate(f(t) exp(i w t), [t, -inf, inf])`, `f(t) = 1/sqrt(2 pi) integrate(F(w) exp(-i w
//! t), [w, -inf, inf])`, and the sine and cosine transforms `sqrt(2/pi) integrate(f(t) sin(w t), [t, 0, inf])` (each
//! its own inverse). `fourier_parameters: [a, b]` gives the general normalisation `sqrt(|b|/(2 pi)^(1 - a))` and
//! kernel `exp(i b w t)` (`(2 pi)^(1 + a)` for the inverses; twice the root for sine and cosine).
//!
//! The exponential transform, in order: linearity (undefined functions stay as transforms); polynomials as
//! distributions (`t^k` gives `sqrt(2 pi) (-i)^k dirac_delta^(k)(w)`, Gel'fand and Shilov, *Generalized Functions*
//! I, 1964, chapter II.2); the table ([`TABLE`]: steps, `sign`, `1/t`, powers and logarithms of `abs(t)`, `sin(a
//! t)/t`); the modulation rule `F[exp(i c t) g](w) = G(w + c)`; trigonometric functions of linear arguments as
//! exponentials; the Gaussian closed form and the steps of `f` as bounds; even and odd integrands through the
//! cosine and sine transforms (so that the value holds for every real `w`: `e^-|w|`, not two half-plane forms);
//! then the definite integral (item 14D).
//!
//! The sine and cosine transforms: the table ([`SIN_TABLE`], [`COS_TABLE`]); the definite integral with `w > 0`;
//! and the Laplace transform at `s = -i w` and `i w` (`integrate(f cos(w t)) = (L(-i w) + L(i w))/2`, valid where
//! the Laplace integral converges for `re(s) > 0`; Sneddon, *Fourier Transforms*, 1951, section 1.4), simplified
//! for `w > 0`.
use super::pattern::Table;
use super::verify;
use super::{factors, held, linear, split_constant, terms, unknown_function, Cx, Kind, MAX_DEPTH};
use super::call_of;
use mc_calculus::common::is_failure;
use mc_limit::bounds::{sign, subst};
use mc_calculus::common::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The exponential transform's table, kernel `exp(i SGN w t)` (`SGN` is 1 for the transform, -1 for the inverse).
/// Sources: Gel'fand and Shilov I, chapter II.2 (steps, `1/t`, `abs(t)^a`, `log(abs(t))`); Erdélyi et al., *Tables
/// of Integral Transforms* I (1954), 1.2 and 2.2.
pub(crate) const TABLE: &str = r#"
[unit_step(t), (pi/2)^(1/2)*dirac_delta(s) + SGN*im/((2*pi)^(1/2)*s)]
[sign(t), SGN*im*(2/pi)^(1/2)/s]
[t^(-1), SGN*im*(pi/2)^(1/2)*sign(s)]
[abs(t)^a, -(2/pi)^(1/2)*gamma(1 + a)*sin(pi*a/2)*abs(s)^(-1 - a)]
[log(abs(t)), -(pi/2)^(1/2)/abs(s) - euler_gamma*(2*pi)^(1/2)*dirac_delta(s)]
[sin(a*t)/t, (1/2)*(pi/2)^(1/2)*(sign(a - s) + sign(a + s))]
[sinc(a*t), (1/2)*(pi/2)^(1/2)*(sign(a - s) + sign(a + s))/a]
[dirac_delta(t), (2*pi)^(-1/2)]
[sech(a*t), (pi/2)^(1/2)/a*sech(pi*s/(2*a))]
[bessel_j(1, t)/t, (2 - 2*s^2)^(1/2)/pi^(1/2)*heaviside_theta(1 - s^2)]
[(1 - t^2)^(-1/2)*unit_box(t/2), (pi/2)^(1/2)*bessel_j(0, abs(s))]
[bessel_j(0, t), (2/pi)^(1/2)*(1 - s^2)^(-1/2)*heaviside_theta(1 - s^2)]
"#;

/// The sine transform's table (`t` the variable, `s` the transform's, for `s > 0`). Sources: Erdélyi et al. I,
/// 2.2-2.4; Oberhettinger, *Tables of Fourier Transforms and Fourier Transforms of Distributions* (1990), part I.
pub(crate) const SIN_TABLE: &str = r#"
[t^(-1), (pi/2)^(1/2)*sign(s)]
[unit_step(t), (2/pi)^(1/2)/s]
[unit_box(t - 1/2), (2/pi)^(1/2)*(1 - cos(s))/s]
[t^a, (2/pi)^(1/2)*gamma(a + 1)*sin(pi*(a + 1)/2)*s^(-a - 1), refine(a < 0)]
[exp(-a*t^2), (2/pi)^(1/2)*a^(-1/2)*dawson_f(s/(2*a^(1/2)))]
[t*exp(-a*t^2), (1/2)*2^(-1/2)*a^(-3/2)*s*exp(-s^2/(4*a))]
[log(t)/t, -(pi/2)^(1/2)*(euler_gamma + log(s))]
[log(t), -(2/pi)^(1/2)*(euler_gamma + log(s))/s]
[log(1 + a*t^2)/t, -(2*pi)^(1/2)*exp_integral_ei(-s/a^(1/2))]
"#;

/// The cosine transform's table (for `s > 0`).
pub(crate) const COS_TABLE: &str = r#"
[unit_step(t), (2*pi)^(1/2)*dirac_delta(s)]
[unit_box(t - 1/2), (2/pi)^(1/2)*sinc(s)]
[t^2, -(2*pi)^(1/2)*derivative(2)(dirac_delta)(s)]
[t^a, (2/pi)^(1/2)*gamma(a + 1)*cos(pi*(a + 1)/2)*s^(-a - 1), refine(a < 0)]
[exp(-a*t^2), (2*a)^(-1/2)*exp(-s^2/(4*a)), possibly(re(a) > 0)]
[cos(a*t^2), (1/2)*a^(-1/2)*(cos(s^2/(4*a)) + sin(s^2/(4*a)))]
[sin(a*t^2), (1/2)*a^(-1/2)*(cos(s^2/(4*a)) - sin(s^2/(4*a)))]
[log(t)*t^(-1/2), -(1/2)*s^(-1/2)*(2*euler_gamma + pi + log(16) + 2*log(s))]
[t^2, -(2*pi)^(1/2)*derivative(2)(dirac_delta)(s)]
[log(1 + a/t^2), (2*pi)^(1/2)*(1 - exp(-a^(1/2)*s))/s]
"#;

/// The transform `kind` of `f` in `t` at `w`, or `None`.
pub fn fourier<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, kind: Kind, f: T<'h>, t: T<'h>, w: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    let pi = b.sym(h.sym("pi"));
    let inverse = matches!(kind, Kind::InverseFourier | Kind::InverseFourierSin | Kind::InverseFourierCos);
    // the general parameters [a, b]: a normalisation and the frequency scale
    let (norm, scale) = match tc.opts.fourier {
        None => (b.one(), b.one()),
        Some((pa, pb)) => {
            let ab = b.call("abs", &[pb]);
            let ex = if inverse { b.add(&[b.one(), pa]) } else { b.sub(b.one(), pa) };
            let root = b.pow(b.div(ab, b.pow(b.mul(&[b.int(2), pi]), ex)), b.rat(1, 2));
            let base = if matches!(kind, Kind::Fourier | Kind::InverseFourier) {
                b.pow(b.mul(&[b.int(2), pi]), b.rat(1, 2))
            } else {
                b.mul(&[b.int(2), b.pow(b.div(pi, b.int(2)), b.rat(1, 2))])
            };
            (cx.eval(b.mul(&[root, base]))?, pb)
        }
    };
    let wv = if scale == b.one() { w } else { h.sym_term(h.sym("transform·w")) };
    let r = match kind {
        Kind::Fourier | Kind::InverseFourier => {
            let sgn = if kind == Kind::Fourier { 1 } else { -1 };
            let table_src = TABLE.replace("SGN", if sgn == 1 { "1" } else { "(-1)" });
            let table = Table::read(cx, b, &table_src, t, wv)?;
            let mut st = State { table, checked: true };
            let Some(v) = expo(cx, tc, &mut st, kind, sgn, f, t, wv, 0)? else { return Ok(None) };
            let v = cx.eval(v)?;
            if is_failure(b, v) || !super::free_outside(b, v, t) {
                return Ok(None);
            }
            let v = super::inverse::trig_form(cx, b, v)?;
            let _ = st.checked;
            v
        }
        _ => {
            let sin = matches!(kind, Kind::FourierSin | Kind::InverseFourierSin);
            let Some(v) = sin_cos(cx, tc, kind, sin, f, t, wv, 0)? else { return Ok(None) };
            v
        }
    };
    if scale == b.one() && norm == b.one() {
        return Ok(Some(r));
    }
    if matches!(kind, Kind::Fourier | Kind::InverseFourier) {
        let r = subst(cx, b, r, wv, b.mul(&[scale, w]))?;
        return Ok(Some(cx.eval(b.mul(&[norm, r]))?));
    }
    // the sine and cosine transforms are for positive arguments: abs(b) w, the sign of b outside the sine
    let ab = cx.eval(b.call("abs", &[scale]))?;
    let r = subst(cx, b, r, wv, b.mul(&[ab, w]))?;
    let r = apply(cx, b, "power_expand", &[r])?;
    let sg = if matches!(kind, Kind::FourierSin | Kind::InverseFourierSin) { b.call("sign", &[scale]) } else { b.one() };
    Ok(Some(cx.eval(b.mul(&[norm, sg, r]))?))
}

struct State<'h> {
    table: Table<'h>,
    checked: bool,
}

#[allow(clippy::too_many_arguments)]
fn expo<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    tc: &Cx<'_, 'h>,
    st: &mut State<'h>,
    kind: Kind,
    sgn: i64,
    f: T<'h>,
    t: T<'h>,
    w: T<'h>,
    depth: u32,
) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    let i = b.sym(h.sym("im"));
    let root2pi = b.pow(b.mul(&[b.int(2), b.sym(h.sym("pi"))]), b.rat(1, 2));
    // polynomials: derivatives of delta functions
    if is_free(b, f, t) {
        return Ok(Some(b.mul(&[f, root2pi, b.call("dirac_delta", &[w])])));
    }
    if let Some(cs) = poly_coefficients(cx, b, f, t)? {
        let mut out = vec![];
        for (k, &c) in cs.iter().enumerate() {
            if b.is_zero(c) {
                continue;
            }
            let delta = if k == 0 { b.call("dirac_delta", &[w]) } else { b.app(b.derivative(&[k as i64], b.sym(h.sym("dirac_delta"))), &[w]) };
            out.push(b.mul(&[c, root2pi, b.pow(b.mul(&[b.int(-sgn), i]), b.int(k as i64)), delta]));
        }
        return Ok(Some(b.add(&out)));
    }
    let f = sinc_as_sin(cx, b, f)?;
    // linearity
    let ts = terms(b, f);
    if ts.len() > 1 {
        let mut out = vec![];
        for g in ts {
            match expo(cx, tc, st, kind, sgn, g, t, w, depth + 1)? {
                Some(r) => out.push(r),
                None => {
                    let (c, rest) = split_constant(b, g, t);
                    if unknown_function(cx, b, rest, t) {
                        out.push(b.mul(&[c, held(b, kind, rest, t, w)]));
                    } else {
                        return Ok(None);
                    }
                }
            }
        }
        return Ok(Some(b.add(&out)));
    }
    let (c, rest) = split_constant(b, f, t);
    if c != b.one() {
        return Ok(expo(cx, tc, st, kind, sgn, rest, t, w, depth + 1)?.map(|r| b.mul(&[c, r])));
    }
    // a derivative of an undefined function, g^(k)(t): (-i SGN w)^k G(w) (the kernel exp(i SGN w t), integration
    // by parts; D21-09: the call was held and evaluated again until the depth limit)
    if let View::App { head, args: [u] } = h.view(f) {
        if *u == t {
            if let Some((orders, g)) = b.as_derivative(head) {
                if let [k] = orders.as_slice() {
                    if b.sym_of(g).is_some_and(|sy| !cx.is_defined(sy)) {
                        let gt = b.app(g, &[t]);
                        let fac = b.mul(&[b.int(-sgn), b.sym(h.sym("im")), w]);
                        return Ok(Some(b.mul(&[b.pow(fac, b.int(*k)), held(b, kind, gt, t, w)])));
                    }
                }
            }
        }
    }
    if unknown_function(cx, b, f, t) {
        return Ok(Some(held(b, kind, f, t, w)));
    }
    if let Some(r) = st.table.lookup(cx, b, f)? {
        st.checked = false;
        return Ok(Some(r));
    }
    let fs = factors(b, f);
    // modulation: exp(i c t + d) g(t) is exp(d) G(w + SGN c)
    for (k, &g) in fs.iter().enumerate() {
        let Some((n, [u])) = call_of(b, g) else { continue };
        if n != "exp" {
            continue;
        }
        let Some((a, d)) = linear(cx, b, *u, t)? else { continue };
        let c = cx.eval(b.mul(&[b.neg(i), a]))?;
        if !is_free(b, c, h.sym_term(h.sym("complex"))) {
            continue;
        }
        let mut rest = fs.clone();
        rest.remove(k);
        let rest = cx.eval(b.mul(&rest))?;
        let Some(r) = expo(cx, tc, st, kind, sgn, rest, t, w, depth + 1)? else { return Ok(None) };
        let r = cx.eval(r)?;
        let r = subst(cx, b, r, w, b.add(&[w, b.mul(&[b.int(sgn), c])]))?;
        return Ok(Some(b.mul(&[b.call("exp", &[d]), r])));
    }
    // trigonometric functions of linear arguments as exponentials
    if has_trig_linear(cx, b, f, t)? {
        let e = apply(cx, b, "trig_to_exp", &[f])?;
        let e = apply(cx, b, "expand", &[e])?;
        if e != f {
            return expo(cx, tc, st, kind, sgn, e, t, w, depth + 1);
        }
    }
    // the steps as bounds, the Gaussian closed form
    if let Some(r) = direct(cx, b, f, t, w, sgn, true)? {
        return Ok(Some(r));
    }
    // even and odd integrands: the cosine and sine transforms at abs(w)
    if depth < 3 {
        let g = subst(cx, b, f, t, b.neg(t))?;
        let even = apply(cx, b, "simplify", &[b.sub(g, f)])?;
        let odd = apply(cx, b, "simplify", &[b.add(&[g, f])])?;
        let parity = if b.is_zero(even) { Some(true) } else if b.is_zero(odd) { Some(false) } else { None };
        if let Some(even) = parity {
            let wp = h.sym_term(h.sym("transform·wp"));
            let sk = if even { Kind::FourierCos } else { Kind::FourierSin };
            let inner = Cx { b, stop: tc.stop, opts: super::Opts { conditions: false, strips: false, fourier: None }, depth: tc.depth + 1 };
            if let Some(r) = sin_cos(cx, &inner, sk, !even, f, t, wp, depth + 1)? {
                if !super::has_held(b, r, sk) {
                    let aw = b.call("abs", &[w]);
                    let r = subst(cx, b, r, wp, aw)?;
                    let r = if even { r } else { cx.eval(b.mul(&[b.int(sgn), i, b.call("sign", &[w]), r]))? };
                    // abs(w) sign(w) is w
                    let real = h.call(h.sym("element"), &[w, h.sym_term(h.sym("reals"))]);
                    let simp = cx.eval_assuming(&[real], b.call("simplify", &[r]))?;
                    let size = super::laplace::size;
                    return Ok(Some(if !is_failure(b, simp) && size(b, simp) < size(b, r) { simp } else { r }));
                }
            }
        }
    }
    if let Some(r) = rational_residues(cx, b, f, t, w, sgn)? {
        return Ok(Some(r));
    }
    direct(cx, b, f, t, w, sgn, false)
}

/// A rational function without real poles, numerator of lower degree: Jordan's lemma with the half plane chosen by
/// the sign of `w`: `sqrt(2 pi) i (heaviside_theta(SGN w) sum(res, upper) - heaviside_theta(-SGN w) sum(res,
/// lower))` (residues of `R(t) exp(i SGN w t)`).
fn rational_residues<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, t: T<'h>, w: T<'h>, sgn: i64) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if !super::inverse::rational_in(b, f, t) {
        return Ok(None);
    }
    let r = apply(cx, b, "together", &[f])?;
    let n = apply(cx, b, "numerator", &[r])?;
    let d = apply(cx, b, "denominator", &[r])?;
    let (Some(dn), Some(dd)) = (super::inverse::degree(cx, b, n, t)?, super::inverse::degree(cx, b, d, t)?) else { return Ok(None) };
    if dn >= dd {
        return Ok(None);
    }
    let Some(ps) = super::inverse::poles(cx, b, d, t)? else { return Ok(None) };
    let i = b.sym(h.sym("im"));
    let kern = cx.eval(b.mul(&[b.int(sgn), i, w]))?;
    let (mut up, mut down) = (vec![], vec![]);
    for (p, m, q) in ps {
        let ip = apply(cx, b, "im_part", &[p])?;
        let side = sign(cx, b, ip)?;
        let res = super::inverse::residue_at(cx, b, n, p, m, q, t, kern)?;
        match side {
            Some(1) => up.push(res),
            Some(-1) => down.push(res),
            _ => return Ok(None),
        }
    }
    let root = b.pow(b.mul(&[b.int(2), b.sym(h.sym("pi"))]), b.rat(1, 2));
    let sw = b.mul(&[b.int(sgn), w]);
    let upper = cx.eval(b.add(&up))?;
    let upper = super::inverse::trig_form(cx, b, upper)?;
    let lower = cx.eval(b.add(&down))?;
    let lower = super::inverse::trig_form(cx, b, lower)?;
    let v = b.mul(&[
        root,
        i,
        b.sub(
            b.mul(&[b.call("heaviside_theta", &[sw]), upper]),
            b.mul(&[b.call("heaviside_theta", &[b.neg(sw)]), lower]),
        ),
    ]);
    Ok(Some(cx.eval(v)?))
}

/// `sinc(u)` as `sin(u)/u` (their values at 0 do not matter for an integral).
fn sinc_as_sin<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if is_free(b, f, h.sym_term(h.sym("sinc"))) {
        return Ok(f);
    }
    let u = h.sym_term(h.sym("transform·u"));
    let pat = h.call(h.sym("pattern"), &[u, h.call(h.sym("blank"), &[])]);
    let _ = pat;
    fn walk<'h>(b: &Build<'h>, t: T<'h>) -> T<'h> {
        let h = b.heap();
        let View::App { head, args } = h.view(t) else { return t };
        let new: Vec<T<'h>> = args.iter().map(|&a| walk(b, a)).collect();
        if b.sym_of(head) == Some(h.sym("sinc")) && new.len() == 1 {
            return b.div(b.call("sin", &[new[0]]), new[0]);
        }
        b.app(head, &new)
    }
    cx.eval(walk(b, f))
}

/// The coefficients of a polynomial in `t` (lowest first), `None` for other terms.
fn poly_coefficients<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, t: T<'h>) -> Res<Option<Vec<T<'h>>>> {
    if !super::inverse::rational_in(b, f, t) {
        return Ok(None);
    }
    let tg = apply(cx, b, "together", &[f])?;
    let den = apply(cx, b, "denominator", &[tg])?;
    if !is_free(b, den, t) {
        return Ok(None);
    }
    let ex = apply(cx, b, "expand", &[f])?;
    let cl = apply(cx, b, "coefficient_list", &[ex, t])?;
    match args_of(b, cl, b.hd.list) {
        Some(cs) if cs.iter().all(|&c| is_free(b, c, t)) => Ok(Some(cs.to_vec())),
        _ => Ok(None),
    }
}

fn has_trig_linear<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, t: T<'h>) -> Res<bool> {
    let h = b.heap();
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if is_free(b, u, t) {
            continue;
        }
        if let Some((n, args)) = call_of(b, u) {
            if matches!(n.as_str(), "sin" | "cos") && args.len() == 1 {
                if let Some((a, _)) = linear(cx, b, args[0], t)? {
                    // a real frequency
                    if is_free(b, a, h.sym_term(h.sym("complex"))) {
                        return Ok(true);
                    }
                }
            }
            stack.extend_from_slice(args);
        }
    }
    Ok(false)
}

/// The transform as one integral: steps of `t` turned into bounds, the Gaussian closed form; with `quick` only
/// the closed form, else also `integrate`.
fn direct<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, t: T<'h>, w: T<'h>, sgn: i64, quick: bool) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let i = h.call(h.sym(names::COMPLEX), &[b.int(0), b.int(sgn)]);
    let kernel = b.call("exp", &[b.mul(&[i, w, t])]);
    let norm = b.mul(&[b.pow(b.int(2), b.rat(-1, 2)), b.pow(b.sym(h.sym("pi")), b.rat(-1, 2))]);
    let inf = b.sym(h.sym("inf"));
    let (mut lo, mut hi) = (b.neg(inf), inf);
    let mut rest = vec![];
    for g in factors(b, f) {
        match step_bound(cx, b, g, t)? {
            Some((true, a)) => lo = if lo == b.neg(inf) { a } else { cx.eval(b.call("max", &[lo, a]))? },
            Some((false, a)) => hi = if hi == inf { a } else { cx.eval(b.call("min", &[hi, a]))? },
            None => rest.push(g),
        }
    }
    if lo != b.neg(inf) && hi != inf {
        let d = cx.eval(b.sub(hi, lo))?;
        if sign(cx, b, d)? == Some(-1) {
            return Ok(Some(b.zero()));
        }
    }
    let g = cx.eval(b.mul(&[b.mul(&rest), kernel]))?;
    if lo == b.neg(inf) && hi == inf {
        if let Some(r) = gaussian(cx, b, g, t)? {
            return Ok(Some(b.mul(&[norm, r])));
        }
        if quick {
            return Ok(None);
        }
    } else if quick && (lo == b.neg(inf) || hi == inf) {
        return Ok(None);
    }
    let Some(r) = super::definite(cx, b, g, t, lo, hi, false)? else { return Ok(None) };
    Ok(Some(b.mul(&[norm, r])))
}

/// `unit_step(t - a)` as the lower bound `a` (`(true, a)`), `unit_step(b - t)` as the upper bound `b`.
fn step_bound<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>) -> Res<Option<(bool, T<'h>)>> {
    let h = b.heap();
    let View::App { head, args: &[u] } = h.view(g) else { return Ok(None) };
    if b.sym_of(head) != Some(h.sym("unit_step")) || is_free(b, u, t) {
        return Ok(None);
    }
    let Some((c, d)) = linear(cx, b, u, t)? else { return Ok(None) };
    Ok(match h.view(c) {
        View::Int(1) => Some((true, cx.eval(b.neg(d))?)),
        View::Int(-1) => Some((false, d)),
        _ => None,
    })
}

/// `integrate(c exp(q(t)), [t, -inf, inf])` for a quadratic `q = -a t^2 + b t + d` with `re(a) > 0` decided:
/// `c sqrt(pi/a) exp(b^2/(4 a) + d)`.
fn gaussian<'h>(cx: &mut NativeCx<'_, '_, 'h>, bd: &Build<'h>, g: T<'h>, t: T<'h>) -> Res<Option<T<'h>>> {
    let h = bd.heap();
    let mut expo = vec![];
    let mut coef = vec![];
    for f in factors(bd, g) {
        if is_free(bd, f, t) {
            coef.push(f);
            continue;
        }
        let e = match h.view(f) {
            View::App { head, args: &[u] } if bd.sym_of(head) == Some(h.sym("exp")) => u,
            View::App { head, args: &[base, u] } if bd.sym_of(head) == Some(bd.hd.pow) && bd.sym_of(base) == Some(h.sym("e")) => u,
            _ => return Ok(None),
        };
        expo.push(e);
    }
    if expo.is_empty() {
        return Ok(None);
    }
    let q = cx.eval(bd.call("expand", &[bd.add(&expo)]))?;
    let cl = cx.eval(bd.call("coefficient_list", &[q, t]))?;
    let View::App { args: &[d, b1, a2], .. } = h.view(cl) else { return Ok(None) };
    if [d, b1, a2].iter().any(|&k| !is_free(bd, k, t)) {
        return Ok(None);
    }
    let a = bd.neg(a2);
    let re = cx.eval(bd.call("re", &[a]))?;
    let pos = cx.eval(bd.call("is_positive", &[re]))?;
    if !cx.is_true(pos) && sign(cx, bd, re)? != Some(1) {
        return Ok(None);
    }
    // sqrt(pi) a^(-1/2), not sqrt(pi/a): the root of a stays apart from the numbers (the tables' form)
    let root = bd.mul(&[bd.pow(bd.sym(h.sym("pi")), bd.rat(1, 2)), bd.pow(a, bd.rat(-1, 2))]);
    let ex = bd.add(&[bd.div(bd.mul(&[b1, b1]), bd.mul(&[bd.int(4), a])), d]);
    Ok(Some(cx.eval(bd.mul(&[bd.mul(&coef), root, bd.call("exp", &[ex])]))?))
}

/// The sine (`sin`) or cosine transform of `f` (module comment), for `w > 0`.
#[allow(clippy::too_many_arguments)]
fn sin_cos<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, kind: Kind, sin: bool, f: T<'h>, t: T<'h>, w: T<'h>, depth: u32) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    cx.tick(1)?;
    let f = sinc_as_sin(cx, b, f)?;
    let f = super::laplace::split_powers(cx, b, f, t)?;
    // linearity
    let ts = terms(b, f);
    if ts.len() > 1 {
        let mut out = vec![];
        let mut all = true;
        for &g in &ts {
            match sin_cos(cx, tc, kind, sin, g, t, w, depth + 1)? {
                Some(r) => out.push(r),
                None => {
                    let (c, rest) = split_constant(b, g, t);
                    if unknown_function(cx, b, rest, t) {
                        out.push(b.mul(&[c, held(b, kind, rest, t, w)]));
                    } else {
                        all = false;
                        break;
                    }
                }
            }
        }
        if all {
            return Ok(Some(cx.eval(b.add(&out))?));
        }
    }
    let (c, rest) = split_constant(b, f, t);
    if c != b.one() {
        return Ok(match sin_cos(cx, tc, kind, sin, rest, t, w, depth + 1)? {
            Some(r) => Some(cx.eval(b.mul(&[c, r]))?),
            None => None,
        });
    }
    if unknown_function(cx, b, f, t) {
        return Ok(Some(held(b, kind, f, t, w)));
    }
    let wpos = h.call(h.sym("greater"), &[w, b.zero()]);
    let root = b.pow(b.div(b.int(2), b.sym(h.sym("pi"))), b.rat(1, 2));
    let table = Table::read(cx, b, if sin { SIN_TABLE } else { COS_TABLE }, t, w)?;
    if let Some(r) = table.lookup(cx, b, f)? {
        if verify::sin_cos(cx, b, sin, f, t, w, r)? != verify::Verdict::Refuted {
            return Ok(Some(r));
        }
    }
    // the definite integral, for w > 0
    let k = b.call(if sin { "sin" } else { "cos" }, &[b.mul(&[w, t])]);
    let g = cx.eval(b.mul(&[f, k]))?;
    let inf = h.sym_term(h.sym("inf"));
    let range = h.call(b.hd.list, &[t, b.zero(), inf]);
    let opt = h.call(h.sym("options"), &[h.call(h.sym(names::RULE), &[h.sym_term(h.sym("generate_conditions")), cx.bool(false)])]);
    let r = cx.eval_assuming(&[wpos], b.call("integrate", &[g, range, opt]))?;
    if is_free(b, r, h.sym_term(h.sym("integrate"))) && !is_failure(b, r) && is_free(b, r, t) {
        // the normalisation last, as the tables write it (sqrt(2/pi) times the integral)
        let r = simplify_positive(cx, b, r, wpos)?;
        return Ok(Some(cx.eval(b.mul(&[root, r]))?));
    }
    // g(t)/t for the sine transform: the integral of the cosine transform of g from 0 to w
    if sin {
        let fs = factors(b, f);
        if let Some(i) = fs.iter().position(|&g| args_of(b, g, b.hd.pow) == Some(&[t, b.int(-1)][..])) {
            let mut rest = fs.clone();
            rest.remove(i);
            let rest = cx.eval(b.mul(&rest))?;
            let u = h.sym_term(h.sym("transform·u"));
            let inner = Cx { b, stop: tc.stop, opts: super::Opts { conditions: false, strips: false, fourier: None }, depth: tc.depth + 1 };
            if inner.depth <= MAX_DEPTH && is_free(b, rest, h.sym_term(h.sym("sin"))) {
                if let Some(c) = sin_cos(cx, &inner, Kind::FourierCos, false, rest, t, u, depth + 1)? {
                    if !super::has_held(b, c, Kind::FourierCos) && is_free(b, c, h.sym_term(h.sym("dirac_delta"))) {
                        let range = h.call(b.hd.list, &[u, b.zero(), w]);
                        let opt = h.call(h.sym("options"), &[h.call(h.sym(names::RULE), &[h.sym_term(h.sym("generate_conditions")), cx.bool(false)])]);
                        let v = cx.eval_assuming(&[wpos], b.call("integrate", &[c, range, opt]))?;
                        if is_free(b, v, h.sym_term(h.sym("integrate"))) && !is_failure(b, v) {
                            let v = simplify_positive(cx, b, v, wpos)?;
                            if verify::sin_cos(cx, b, true, f, t, w, v)? != verify::Verdict::Refuted {
                                return Ok(Some(v));
                            }
                        }
                    }
                }
            }
        }
    }
    if !sin {
        let fs = factors(b, f);
        if let Some(i) = fs.iter().position(|&g| args_of(b, g, b.hd.pow) == Some(&[t, b.int(-1)][..])) {
            let mut rest = fs.clone();
            rest.remove(i);
            let rest = cx.eval(b.mul(&rest))?;
            let at0 = apply(cx, b, "limit", &[rest, h.call(h.sym(names::RULE), &[t, b.zero()])])?;
            let u = h.sym_term(h.sym("transform·u"));
            let inner = Cx { b, stop: tc.stop, opts: super::Opts { conditions: false, strips: false, fourier: None }, depth: tc.depth + 1 };
            if b.is_zero(at0) && inner.depth <= MAX_DEPTH {
                if let Some(c) = sin_cos(cx, &inner, Kind::FourierSin, true, rest, t, u, depth + 1)? {
                    if !super::has_held(b, c, Kind::FourierSin) && is_free(b, c, h.sym_term(h.sym("dirac_delta"))) {
                        let upos = h.call(h.sym("greater"), &[u, b.zero()]);
                        if let Some(big) = mc_integrals::antiderivative(cx, b, c, u, 0)? {
                            let big = cx.eval(big)?;
                            let lim = cx.eval_assuming(&[upos], b.call("limit", &[big, h.call(h.sym(names::RULE), &[u, inf])]))?;
                            if is_free(b, lim, h.sym_term(h.sym("limit"))) && is_free(b, lim, inf) && !is_failure(b, lim) {
                                let at_w = subst(cx, b, big, u, w)?;
                                let v = cx.eval(b.sub(lim, at_w))?;
                                let v = simplify_positive(cx, b, v, wpos)?;
                                if verify::sin_cos(cx, b, false, f, t, w, v)? != verify::Verdict::Refuted {
                                    return Ok(Some(v));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // the Laplace transform on the imaginary axis
    let s = h.sym_term(h.sym("transform·s"));
    let inner = Cx { b, stop: tc.stop, opts: super::Opts { conditions: false, strips: false, fourier: None }, depth: tc.depth + 1 };
    if inner.depth <= MAX_DEPTH {
        if let Some(l) = super::laplace::laplace(cx, &inner, f, t, s)? {
            if !super::has_held(b, l, Kind::Laplace) {
                let i = b.sym(h.sym("im"));
                let lm = subst(cx, b, l, s, b.mul(&[b.neg(i), w]))?;
                let lp = subst(cx, b, l, s, b.mul(&[i, w]))?;
                let v = if sin { b.div(b.sub(lm, lp), b.mul(&[b.int(2), i])) } else { b.div(b.add(&[lm, lp]), b.int(2)) };
                let v = cx.eval(v)?;
                if !is_failure(b, v) {
                    let v = simplify_positive(cx, b, v, wpos)?;
                    let v = cx.eval(b.mul(&[root, v]))?;
                    // the boundary values of L are the transform only when f decays exponentially (no singularity
                    // of L on the imaginary axis), or when the numeric check confirms them
                    let decays = {
                        let g = cx.eval(b.mul(&[f, b.call("exp", &[b.div(t, b.int(10))])]))?;
                        let lim = apply(cx, b, "limit", &[g, h.call(h.sym(names::RULE), &[t, inf])])?;
                        b.is_zero(lim)
                    };
                    let verdict = verify::sin_cos(cx, b, sin, f, t, w, v)?;
                    if verdict == verify::Verdict::Confirmed || decays && verdict != verify::Verdict::Refuted {
                        return Ok(Some(v));
                    }
                }
            }
        }
    }
    Ok(None)
}

/// `v` simplified with `w > 0` (and the facts), when that is smaller.
fn simplify_positive<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>, wpos: T<'h>) -> Res<T<'h>> {
    let size = super::laplace::size;
    if size(b, v) > 400 {
        return Ok(v);
    }
    let mut best = v;
    for f in ["exp_to_trig", "simplify"] {
        let r = cx.eval_assuming(&[wpos], b.call(f, &[best]))?;
        if !is_failure(b, r) && (size(b, r) < size(b, best) || f == "simplify" && size(b, r) <= size(b, best)) {
            best = r;
        }
    }
    let r = cx.eval_assuming(&[wpos], b.call("simplify", &[best]))?;
    if !is_failure(b, r) && size(b, r) < size(b, best) {
        best = r;
    }
    Ok(best)
}
