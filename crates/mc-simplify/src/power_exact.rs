//! `power_expand(e, assumptions: facts)`: the power and logarithm laws with the branch corrections they need, decided
//! under the facts where the facts allow.
//!
//! With the principal branches (`-pi < arg z <= pi`) the identities are exact only with an integer of winding:
//!
//! - `(a b)^c = a^c b^c exp(2 pi i c k)` and `log(a b) = log a + log b + 2 pi i k` and
//!   `arg(a b) = arg a + arg b + 2 pi k`, with `k = floor(1/2 - (arg a + arg b)/(2 pi))`;
//! - `(z^p)^q = z^(p q) exp(2 pi i q k)` and `log(z^p) = p log z + 2 pi i k`, with `k = floor(1/2 - Im(p log z)/(2 pi))`;
//! - `log(exp(u)) = u + 2 pi i floor(1/2 - Im(u)/(2 pi))`.
//!
//! (The principal value of `log w` is `ln |w| + i Arg w`, and the sum of the arguments of the factors differs from
//! the argument of the product by a multiple of `2 pi`; `k` is the integer that brings the sum into `(-pi, pi]`.)
//! The integer `k` is found from interval bounds of the arguments (bounds on `arg(x)` given as facts, `x` known
//! positive or negative real, numbers); when the bounds leave more than one integer, the `floor` stays in the
//! result. A correction with an integer `q` (`exp(2 pi i q k) = 1`) is dropped.
//!
//! This is the behaviour of the explicit option only; the default stays the formal rewriting of [`crate::power`].
use crate::facts::Facts;
use crate::term::B;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// A real interval (infinite ends have no flag).
#[derive(Clone, Copy, Debug)]
struct Iv {
    lo: f64,
    hi: f64,
    lo_closed: bool,
    hi_closed: bool,
}

impl Iv {
    fn point(x: f64) -> Iv {
        Iv { lo: x, hi: x, lo_closed: true, hi_closed: true }
    }
    fn is_point(&self) -> bool {
        self.lo == self.hi && self.lo_closed && self.hi_closed
    }
    fn add(self, o: Iv) -> Iv {
        Iv { lo: self.lo + o.lo, hi: self.hi + o.hi, lo_closed: self.lo_closed && o.lo_closed, hi_closed: self.hi_closed && o.hi_closed }
    }
    /// Product with a point `c`.
    fn scale(self, c: f64) -> Iv {
        if c >= 0.0 {
            Iv { lo: self.lo * c, hi: self.hi * c, ..self }
        } else {
            Iv { lo: self.hi * c, hi: self.lo * c, lo_closed: self.hi_closed, hi_closed: self.lo_closed }
        }
    }
    /// Product: by the corners; an end is closed only when every corner attaining it is.
    fn mul(self, o: Iv) -> Option<Iv> {
        if o.is_point() {
            return Some(self.scale(o.lo));
        }
        if self.is_point() {
            return Some(o.scale(self.lo));
        }
        let ends = [(self.lo, self.lo_closed), (self.hi, self.hi_closed)];
        let others = [(o.lo, o.lo_closed), (o.hi, o.hi_closed)];
        let mut corners = vec![];
        for &(a, fa) in &ends {
            for &(b, fb) in &others {
                let v = a * b;
                if v.is_nan() {
                    return None;
                }
                corners.push((v, fa && fb));
            }
        }
        let lo = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        let hi = corners.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max);
        let closed = |x: f64| corners.iter().filter(|c| c.0 == x).all(|c| c.1);
        Some(Iv { lo, hi, lo_closed: closed(lo), hi_closed: closed(hi) })
    }
}

/// One summand of the argument of `floor`: its term and, when known, its range.
struct Piece<'h> {
    term: T<'h>,
    iv: Option<Iv>,
}

struct Ctx<'a, 'b, 'h> {
    b: &'a B<'h>,
    facts: Option<&'a mut Facts<'b, 'h>>,
}

/// `power_expand(t)` with the corrections.
pub fn power_expand_exact<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, facts: Option<&mut Facts<'_, 'h>>, t: T<'h>) -> Res<T<'h>> {
    let mut c = Ctx { b, facts };
    let r = go(cx, &mut c, t, 0)?;
    cx.eval(r)
}

fn pi<'h>(b: &B<'h>) -> T<'h> {
    let h = b.heap();
    h.sym_term(h.sym("pi"))
}

/// `2 pi i`
fn two_pi_i<'h>(b: &B<'h>) -> T<'h> {
    b.mul(&[b.int(2), pi(b), b.i()])
}

fn is_integer<'h>(b: &B<'h>, t: T<'h>) -> bool {
    matches!(b.heap().view(t), View::Int(_))
}

/// The machine value of a closed number term.
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<Option<f64>> {
    let v = cx.eval(b.call_named("n", &[t]))?;
    Ok(match b.heap().view(v) {
        View::Real(x) if x.is_finite() => Some(x),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// The range of the real expression `t` from the facts (both ends needed).
fn range_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &mut Ctx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Iv>> {
    let b = c.b;
    if let Some(f) = c.facts.as_deref_mut() {
        let info = f.info(cx, t)?;
        if let (Some(lo), Some(hi)) = (info.range.lo, info.range.hi) {
            if let (Some(l), Some(h)) = (value(cx, b, lo.c)?, value(cx, b, hi.c)?) {
                return Ok(Some(Iv { lo: l, hi: h, lo_closed: !lo.strict, hi_closed: !hi.strict }));
            }
        }
    }
    // bounds given on an atom like `im_part(x)` that the decision service does not record: read the facts
    let facts: Vec<T<'h>> = cx.assumptions().to_vec();
    let mut iv = Iv { lo: f64::NEG_INFINITY, hi: f64::INFINITY, lo_closed: false, hi_closed: false };
    let mut stack = facts;
    while let Some(fa) = stack.pop() {
        let Some((s, a)) = b.call_of(fa) else { continue };
        let name = b.heap().sym_name(s);
        match name.as_str() {
            "and" => stack.extend(a.iter().copied()),
            "less" | "less_equal" | "greater" | "greater_equal" => {
                let up = matches!(name.as_str(), "less" | "less_equal");
                let closed = matches!(name.as_str(), "less_equal" | "greater_equal");
                for (i, w) in a.windows(2).enumerate() {
                    let _ = i;
                    // w[0] op w[1]: for `less`, w[0] below w[1]
                    let (small, big) = if up { (w[0], w[1]) } else { (w[1], w[0]) };
                    if big == t {
                        if let Some(v) = real_number(cx, b, small)? {
                            if v > iv.lo || (v == iv.lo && !closed) {
                                iv.lo = v;
                                iv.lo_closed = closed;
                            }
                        }
                    }
                    if small == t {
                        if let Some(v) = real_number(cx, b, big)? {
                            if v < iv.hi || (v == iv.hi && !closed) {
                                iv.hi = v;
                                iv.hi_closed = closed;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(if iv.lo.is_finite() && iv.hi.is_finite() { Some(iv) } else { None })
}

/// The value of an exact or inexact real number term.
fn real_number<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<Option<f64>> {
    if b.is_number(t) && b.complex_parts(t).is_none() {
        return value(cx, b, t);
    }
    Ok(None)
}

/// Is `t` known to be a real number (by the facts or because it is one)?
fn is_real<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &mut Ctx<'_, '_, 'h>, t: T<'h>) -> Res<bool> {
    if real_number(cx, c.b, t)?.is_some() {
        return Ok(true);
    }
    let Some(f) = c.facts.as_deref_mut() else { return Ok(false) };
    Ok(f.info(cx, t)?.is_real())
}

/// The range of `arg(t)`: a point for a number or a real of known sign, the facts' bounds on `arg(t)` otherwise.
fn arg_iv<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &mut Ctx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Iv>> {
    let b = c.b;
    if b.is_number(t) {
        let a = cx.eval(b.call_named("arg", &[t]))?;
        return Ok(value(cx, b, a)?.map(Iv::point));
    }
    if let Some(iv) = range_of(cx, c, b.call_named("arg", &[t]))? {
        return Ok(Some(iv));
    }
    if let Some((s, fs)) = b.call_of(t) {
        // a product of factors with point arguments: the sum, brought into (-pi, pi]
        if s == b.hd.mul {
            let mut sum = 0.0;
            for &f in fs {
                match arg_iv(cx, c, f)? {
                    Some(iv) if iv.is_point() => sum += iv.lo,
                    _ => return Ok(None),
                }
            }
            return Ok(Some(Iv::point(reduce(sum))));
        }
        if s == b.hd.pow {
            if let [base, e] = fs {
                if let View::Int(k) = b.heap().view(*e) {
                    if let Some(iv) = arg_iv(cx, c, *base)? {
                        if iv.is_point() {
                            return Ok(Some(Iv::point(reduce(iv.lo * k as f64))));
                        }
                    }
                }
            }
        }
    }
    // a real of known sign
    if let Some(f) = c.facts.as_deref_mut() {
        let info = f.info(cx, t)?;
        if info.is_real() {
            if f.positive(&info) || f.nonneg(&info) {
                return Ok(Some(Iv::point(0.0)));
            }
            if f.negative(&info) {
                return Ok(Some(Iv::point(std::f64::consts::PI)));
            }
        }
    }
    // the principal argument lies in (-pi, pi]
    Ok(Some(Iv { lo: -std::f64::consts::PI, hi: std::f64::consts::PI, lo_closed: false, hi_closed: true }))
}

/// An angle brought into `(-pi, pi]` (multiples of pi stay put up to rounding).
fn reduce(a: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut r = a - tau * ((a / tau) + 0.5).floor();
    if r <= -std::f64::consts::PI + 1e-12 {
        r += tau;
    }
    if (r - std::f64::consts::PI).abs() < 1e-12 {
        r = std::f64::consts::PI;
    }
    r
}

/// The piece `Im(p log z)` of `log(z^p)`: its term and range.
fn im_p_log<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &mut Ctx<'_, '_, 'h>, p: T<'h>, z: T<'h>) -> Res<Piece<'h>> {
    let b = c.b;
    if b.sym_of(z) == Some(b.hd.e) {
        return im_of(cx, c, p);
    }
    if is_real(cx, c, p)? {
        // a real exponent: Im(p log z) = p arg(z)
        let pr = match real_number(cx, b, p)? {
            Some(v) => Some(Iv::point(v)),
            None => range_of(cx, c, p)?,
        };
        let az = arg_iv(cx, c, z)?;
        let iv = match (pr, az) {
            (Some(p), Some(a)) => p.mul(a),
            _ => None,
        };
        return Ok(Piece { term: b.mul(&[p, b.call_named("arg", &[z])]), iv });
    }
    Ok(Piece { term: b.call_named("im_part", &[b.mul(&[p, b.call_named("log", &[z])])]), iv: None })
}

/// The piece `Im(u)` for the exponent `u` of an exponential.
fn im_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &mut Ctx<'_, '_, 'h>, u: T<'h>) -> Res<Piece<'h>> {
    let b = c.b;
    if is_real(cx, c, u)? {
        return Ok(Piece { term: b.int(0), iv: Some(Iv::point(0.0)) });
    }
    let term = b.call_named("im_part", &[u]);
    let iv = range_of(cx, c, term)?;
    Ok(Piece { term, iv })
}

/// `k = floor(1/2 - sum/(2 pi))`: an integer when the ranges decide it, else the `floor` term.
fn winding<'h>(b: &B<'h>, ps: &[Piece<'h>]) -> (Option<i64>, T<'h>) {
    let terms: Vec<T<'h>> = ps.iter().map(|p| p.term).collect();
    let sum = b.add(&terms);
    let arg = b.sub(b.rat(1, 2), b.div(sum, b.mul(&[b.int(2), pi(b)])));
    let sym = b.call_named("floor", &[arg]);
    let mut iv = Some(Iv::point(0.0));
    for p in ps {
        iv = match (iv, p.iv) {
            (Some(a), Some(q)) => Some(a.add(q)),
            _ => None,
        };
    }
    let Some(iv) = iv else { return (None, sym) };
    let tau = std::f64::consts::TAU;
    // v = 1/2 - s/(2 pi) decreases in s
    let (vl, vh, vh_closed) = (0.5 - iv.hi / tau, 0.5 - iv.lo / tau, iv.lo_closed);
    if !(vl.is_finite() && vh.is_finite()) {
        return (None, sym);
    }
    let near = |x: f64| (x - x.round()).abs() < 1e-9;
    let fl = if near(vl) { vl.round() } else { vl.floor() };
    let fh = if near(vh) {
        if vh_closed {
            vh.round()
        } else {
            vh.round() - 1.0
        }
    } else {
        vh.floor()
    };
    if fl == fh {
        (Some(fl as i64), sym)
    } else {
        (None, sym)
    }
}

/// The winding integer as a term; `None` when it is 0.
fn corr<'h>(b: &B<'h>, ps: &[Piece<'h>]) -> Option<T<'h>> {
    let (k, sym) = winding(b, ps);
    match k {
        Some(0) => None,
        Some(n) => Some(b.int(n)),
        None => Some(sym),
    }
}

fn arg_pieces<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &mut Ctx<'_, '_, 'h>, fs: &[T<'h>]) -> Res<Vec<Piece<'h>>> {
    let mut ps = vec![];
    for &f in fs {
        let iv = arg_iv(cx, c, f)?;
        ps.push(Piece { term: c.b.call_named("arg", &[f]), iv });
    }
    Ok(ps)
}

fn go<'h>(cx: &mut NativeCx<'_, '_, 'h>, c: &mut Ctx<'_, '_, 'h>, t: T<'h>, depth: usize) -> Res<T<'h>> {
    let b = c.b;
    let h = b.heap();
    cx.tick(1)?;
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if b.complex_parts(t).is_some() || depth > 200 {
        return Ok(t);
    }
    let hd = &b.hd;
    if let Some((s, a)) = b.call_of(t) {
        if s == hd.pow {
            if let [base, q] = a {
                let q = *q;
                if let Some((g, fs)) = b.call_of(*base) {
                    if g == hd.mul {
                        // (a b)^c = a^c b^c exp(2 pi i c k)
                        let mut parts = vec![];
                        for &f in fs {
                            let p = b.pow(f, q);
                            parts.push(go(cx, c, p, depth + 1)?);
                        }
                        if !is_integer(b, q) {
                            let ps = arg_pieces(cx, c, fs)?;
                            if let Some(k) = corr(b, &ps) {
                                parts.push(b.exp(b.mul(&[two_pi_i(b), q, k])));
                            }
                        }
                        return Ok(b.mul(&parts));
                    }
                    // (z^p)^q and (e^u)^q
                    let inner = match (g == hd.pow, g == hd.exp, fs) {
                        (true, _, [z, p]) => Some((*z, *p)),
                        (_, true, [u]) => Some((h.sym_term(hd.e), *u)),
                        _ => None,
                    };
                    if let Some((z, p)) = inner {
                        let z2 = go(cx, c, z, depth + 1)?;
                        let mut res = b.pow(z2, b.mul(&[p, q]));
                        if !is_integer(b, q) {
                            let piece = if g == hd.exp { im_of(cx, c, p)? } else { im_p_log(cx, c, p, z)? };
                            if let Some(k) = corr(b, &[piece]) {
                                res = b.mul(&[res, b.exp(b.mul(&[two_pi_i(b), q, k]))]);
                            }
                        }
                        return Ok(res);
                    }
                }
            }
        }
        if s == hd.log {
            if let [u] = a {
                if let Some((g, fs)) = b.call_of(*u) {
                    if g == hd.mul {
                        let mut sum = vec![];
                        for &f in fs {
                            sum.push(go(cx, c, b.call(hd.log, &[f]), depth + 1)?);
                        }
                        let ps = arg_pieces(cx, c, fs)?;
                        if let Some(k) = corr(b, &ps) {
                            sum.push(b.mul(&[two_pi_i(b), k]));
                        }
                        return Ok(b.add(&sum));
                    }
                    if let (true, [z, p]) = (g == hd.pow, fs) {
                        let l = go(cx, c, b.call(hd.log, &[*z]), depth + 1)?;
                        let mut sum = vec![b.mul(&[*p, l])];
                        let piece = im_p_log(cx, c, *p, *z)?;
                        if let Some(k) = corr(b, &[piece]) {
                            sum.push(b.mul(&[two_pi_i(b), k]));
                        }
                        return Ok(b.add(&sum));
                    }
                    if let (true, [v]) = (g == hd.exp, fs) {
                        let mut sum = vec![*v];
                        let piece = im_of(cx, c, *v)?;
                        if let Some(k) = corr(b, &[piece]) {
                            sum.push(b.mul(&[two_pi_i(b), k]));
                        }
                        return Ok(b.add(&sum));
                    }
                }
            }
        }
        if let ([u], "arg") = (a, h.sym_name(s).as_str()) {
            if let Some((g, fs)) = b.call_of(*u) {
                if g == hd.mul {
                    let mut sum = vec![];
                    for &f in fs {
                        sum.push(go(cx, c, b.call_named("arg", &[f]), depth + 1)?);
                    }
                    let ps = arg_pieces(cx, c, fs)?;
                    if let Some(k) = corr(b, &ps) {
                        sum.push(b.mul(&[b.int(2), pi(b), k]));
                    }
                    return Ok(b.add(&sum));
                }
            }
        }
    }
    let mut out = Vec::with_capacity(args.len());
    for &x in args {
        out.push(go(cx, c, x, depth + 1)?);
    }
    Ok(b.app(head, &out))
}
