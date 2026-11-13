//! Integral transforms (roadmap items 2C and 14E): Laplace (one- and two-sided), Mellin, Fourier (exponential,
//! sine and cosine) and their inverses, and the discrete pair `z_transform` / `inverse_z_transform`.
//!
//! One front end ([`Transform`]) for every transform: a list (or an equation, a rule) maps; several variables
//! (`laplace_transform(f, [x, y], [p, q])`) iterate, one variable after the other; a target that is not a symbol
//! (`laplace_transform(f, t, 0.3)`) is computed with a symbol and substituted, and a numeric one falls back to the
//! numerical integral; options `generate_conditions:` and (Fourier kinds) `fourier_parameters:`; `assumptions:` is
//! handled by the wrapper of every native (facts in the context). Each transform is a module:
//!
//! - [`laplace`]: the Laplace transform and the two-sided one, by linearity, the shift, scaling, derivative,
//!   integral and multiplication rules (Doetsch, *Introduction to the Theory and Application of the Laplace
//!   Transformation*, 1974, sections 8-12), a table of special functions (Abramowitz and Stegun 29.3; Oberhettinger
//!   and Badii, *Tables of Laplace Transforms*, 1973) and, for what they do not cover, the definite integral of
//!   item 14D;
//! - [`inverse`]: the inverse Laplace transform: rational functions by partial fractions and residues (the
//!   Heaviside expansion, Doetsch section 22), the shift and scaling rules, the derivative rule `L^-1[F] = -L^-1[F']/t`
//!   for logarithms and inverse tangents, and a table of algebraic and special forms; the two-sided inverse on a
//!   strip by the residues on either side of it;
//! - [`mellin`]: the Mellin transform (the definite integral with the scaling and power rules, a table; Titchmarsh,
//!   *Introduction to the Theory of Fourier Integrals*, 1948, chapter 1; Oberhettinger, *Tables of Mellin
//!   Transforms*, 1974) and its inverse by Mellin-Barnes residue series summed by `sum` (Paris and Kaminski,
//!   *Asymptotics and Mellin-Barnes Integrals*, 2001, chapter 3);
//! - [`fourier`]: the exponential Fourier transform (Gaussians, distributions of polynomials and exponentials,
//!   even and odd parts through the sine and cosine transforms, the definite integral) and the sine and cosine
//!   transforms (the definite integral, and the Laplace transform at `s = ∓ i w`), with `fourier_parameters:`;
//! - [`ztrans`]: the Z transform by its rules (shift, multiplication by `a^n` and by `n`, deltas and steps) and
//!   `sum`, and its inverse for rational functions by partial fractions in `z`;
//! - [`pattern`]: the matcher of the transform tables (written in the language itself and read once per call);
//! - [`verify`]: the numeric check of a transform found by rules or tables (a transform at a sample point against
//!   `n_integrate`, an inverse against the forward integral of its value).
//!
//! When no method gives a value the call stays unevaluated; a value the numeric check refutes is not returned.
//!
//! A crate of its own since round 34 (split out of `mc-calculus`); [`Kind`], the transforms by name, is
//! `mc-calculus`'s, since the derivative operator needs the names.
#![warn(missing_docs)]
pub mod fourier;
pub mod inverse;
pub mod laplace;
pub mod mellin;
mod meijer_inv;
mod pattern;
mod verify;
pub mod ztrans;

use mc_calculus::common::{apply, args_of, is_free};
use mc_calculus::term::Build;
use mc_calculus::transform_kind::Kind;
use mc_api::{names, Store, View, T};
use mc_arith::num::StopScope;
use mc_eval::{Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The options of one transform call.
#[derive(Clone, Copy, Debug)]
pub struct Opts<'h> {
    /// `generate_conditions: true`: the value as `conditional_expression(value, conditions)` when it has conditions
    pub conditions: bool,
    /// the regions of convergence found by the tables and rules are kept as conditions even without `conditions` (the
    /// check of a value at a point, D23-14); the definite integral is not asked for them
    pub strips: bool,
    /// `fourier_parameters: [a, b]` (default `[0, 1]`)
    pub fourier: Option<(T<'h>, T<'h>)>,
}

/// The native of every transform.
pub struct Transform {
    /// which transform
    pub kind: Kind,
    /// the flag `mc-arith` polls in long numeric loops
    pub stop: Arc<AtomicBool>,
}

/// The state of one call, passed to the modules.
pub struct Cx<'a, 'h> {
    /// the term builder
    pub b: &'a Build<'h>,
    /// the stop flag
    pub stop: &'a Arc<AtomicBool>,
    /// the options
    pub opts: Opts<'h>,
    /// the nesting of transforms inside rules (a bound against loops)
    pub depth: u32,
}

/// The deepest nesting of transforms called by the rules of a transform.
pub const MAX_DEPTH: u32 = 8;

impl Native for Transform {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (args, opt) = match args.split_last() {
            Some((&last, rest)) if matches!(h.view(last), View::App { head, .. } if head == h.sym_term(h.sym("options"))) => (rest, Some(last)),
            _ => (args, None),
        };
        let [f, x, y] = args else { return Ok(Reply::Decline) };
        let _scope = StopScope::enter(Some(&self.stop));
        let b = Build::new(h, &self.stop);
        let Some(opts) = self.options(cx, &b, opt) else { return Ok(Reply::Decline) };
        let tc = Cx { b: &b, stop: &self.stop, opts, depth: 0 };
        Ok(match transform_any(cx, &tc, self.kind, *f, *x, *y)? {
            Some(r) => Reply::Done(r),
            None => Reply::Decline,
        })
    }
}

impl Transform {
    fn options<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, opt: Option<T<'h>>) -> Option<Opts<'h>> {
        let h = b.heap();
        // the two-sided Laplace transform gives its region of convergence by default
        let mut o = Opts { conditions: self.kind == Kind::BilateralLaplace, strips: false, fourier: None };
        let Some(opt) = opt else { return Some(o) };
        let View::App { args, .. } = h.view(opt) else { return None };
        for &r in args {
            let View::App { args: &[k, v], .. } = h.view(r) else { return None };
            let View::Sym(s) = h.view(k) else { return None };
            match h.sym_name(s).as_str() {
                "generate_conditions" => o.conditions = cx.is_true(v),
                "fourier_parameters" if self.kind.fourier_like() => match args_of(b, v, b.hd.list) {
                    Some(&[p, q]) => o.fourier = Some((p, q)),
                    _ => return None,
                },
                _ => return None,
            }
        }
        Some(o)
    }
}

/// The transform of `f` (a list, an equation or a rule maps; lists of variables iterate), or `None`.
pub fn transform_any<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, kind: Kind, f: T<'h>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    // lists, equations and rules map
    if let View::App { head, args } = h.view(f) {
        let s = b.sym_of(head);
        if s == Some(b.hd.list) || s == Some(h.sym("equal")) || s == Some(h.sym(names::RULE)) {
            let mut out = Vec::with_capacity(args.len());
            for &g in args {
                match transform_any(cx, tc, kind, g, x, y)? {
                    Some(r) => out.push(r),
                    // a side that is an undefined function of the variable alone keeps its transform held (`a^n ==
                    // f(n)`: the equation of the transforms)
                    None if is_call_of_variable(cx, b, g, x) => out.push(held(b, kind, g, x, y)),
                    None => return Ok(None),
                }
            }
            return Ok(Some(cx.eval(b.app(head, &out))?));
        }
    }
    // several variables: one after the other
    if let (Some(xs), Some(ys)) = (args_of(b, x, b.hd.list), args_of(b, y, b.hd.list)) {
        if xs.len() != ys.len() || xs.is_empty() {
            return Ok(None);
        }
        let mut g = f;
        for (&xi, &yi) in xs.iter().zip(ys) {
            match transform_one(cx, tc, kind, g, xi, yi)? {
                Some(r) => g = r,
                None => return Ok(None),
            }
            // a step that left part of the transform unevaluated stops the iteration
            if !is_free(b, g, h.sym_term(h.sym(kind.name()))) {
                return Ok(None);
            }
        }
        return Ok(Some(g));
    }
    transform_one(cx, tc, kind, f, x, y)
}

/// Is `f` a call `g(x)` of an undefined function `g` at the symbol `x` alone?
fn is_call_of_variable<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    matches!(h.view(x), View::Sym(_))
        && matches!(h.view(f), View::App { head, args: &[a] } if a == x && matches!(h.view(head), View::Sym(s) if !cx.is_defined(s)))
}

/// The transform of `f` in the symbol `x` at `y` (a symbol, or any term: computed at a symbol, then substituted).
pub fn transform_one<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, kind: Kind, f: T<'h>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    if !matches!(h.view(x), View::Sym(_)) {
        return Ok(None);
    }
    cx.tick(1)?;
    // an undefined function of the variable alone: the call stays (its transform is itself; decided first, since the
    // tidying of a value holding the call would evaluate it again)
    if let View::App { head, .. } = h.view(f) {
        if matches!(h.view(head), View::Sym(s) if !cx.is_defined(s)) && args_of(b, f, b.sym_of(head).unwrap()) == Some(&[x][..]) {
            return Ok(None);
        }
    }
    let f = normal(cx, b, f)?;
    // a power x^(2^31): the rules and the integral behind them would expand it or loop over it (X23-10)
    if mc_integrals::has_huge_power(b, f) {
        return Ok(None);
    }
    // a product of undefined functions of the variable (f(n) g(n)): no rule gives its transform, and the rules
    // that hold it back would evaluate it again without end (D23-20)
    {
        let fs = factors(b, f);
        if fs.len() > 1
            && fs.iter().all(|&g| match h.view(g) {
                View::App { head, args } => matches!(h.view(head), View::Sym(s) if !cx.is_defined(s)) && args.iter().all(|&a| a == x || is_free(b, a, x)),
                _ => false,
            })
        {
            return Ok(None);
        }
    }
    let f = split_exponentials(cx, b, f, x)?;
    if matches!(h.view(y), View::Sym(_)) && y != x {
        let Some(r) = dispatch(cx, tc, kind, f, x, y)? else { return Ok(None) };
        // the call itself (an undefined function): unevaluated
        if r == held(b, kind, f, x, y) {
            return Ok(None);
        }
        return Ok(Some(finish(cx, b, r, y, kind == Kind::Z)?));
    }
    // the unilateral inverse Z transform of a rational function bounded at infinity is 0 for n < 0 (D23-13)
    if kind == Kind::InverseZ {
        if let View::Int(k) = h.view(y) {
            if k < 0 && ztrans::causal_rational(cx, b, f, x)? {
                return Ok(Some(b.zero()));
            }
        }
    }
    // the sine and cosine transforms at a negative number: the closed forms are for w > 0; the cosine transform is
    // even in w and the sine transform odd (B26-7)
    if matches!(kind, Kind::FourierSin | Kind::FourierCos) && tc.opts.fourier.is_none() {
        let lt = cx.eval(h.call(h.sym("less"), &[y, b.zero()]))?;
        if cx.is_true(lt) && is_free(b, y, x) && only_symbol(b, y, x) {
            let yp = cx.eval(b.neg(y))?;
            if let Some(r) = transform_one(cx, tc, kind, f, x, yp)? {
                return Ok(Some(if kind == Kind::FourierSin { cx.eval(b.neg(r))? } else { r }));
            }
            return Ok(None);
        }
    }
    // a value at a point: the transform at a symbol, then the point
    let ys =h.sym_term(h.sym("transform·point"));
    // the value must hold at the point: found with its conditions, which are checked there (outside the region of
    // convergence the closed form is only the analytic continuation, D23-14)
    let cond_cx = Cx { b, stop: tc.stop, opts: Opts { strips: true, ..tc.opts }, depth: tc.depth };
    if let Some(r) = dispatch(cx, &cond_cx, kind, f, x, ys)? {
        let (r, cond) = conditional(b, r);
        if let Some(c) = cond {
            let cv = mc_limit::bounds::subst(cx, b, c, ys, y)?;
            if cx.is_false(cv) {
                return Ok(None);
            }
        }
        if is_free(b, r, h.sym_term(h.sym(kind.name()))) {
            let v = mc_limit::bounds::subst(cx, b, r, ys, y)?;
            if !mc_calculus::common::is_failure(b, v) {
                return Ok(Some(finish(cx, b, v, y, kind == Kind::Z)?));
            }
        }
    }
    numeric_point(cx, tc, kind, f, x, y)
}

fn dispatch<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, kind: Kind, f: T<'h>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    match kind {
        Kind::Laplace => laplace::laplace(cx, tc, f, x, y),
        Kind::BilateralLaplace => laplace::bilateral(cx, tc, f, x, y),
        Kind::InverseLaplace => inverse::inverse_laplace(cx, tc, f, x, y),
        Kind::InverseBilateralLaplace => inverse::inverse_bilateral(cx, tc, f, x, y),
        Kind::Mellin => mellin::mellin(cx, tc, f, x, y),
        Kind::InverseMellin => mellin::inverse_mellin(cx, tc, f, x, y),
        Kind::Fourier | Kind::InverseFourier | Kind::FourierSin | Kind::InverseFourierSin | Kind::FourierCos | Kind::InverseFourierCos => {
            fourier::fourier(cx, tc, kind, f, x, y)
        }
        Kind::Z => ztrans::z_transform(cx, tc, f, x, y),
        Kind::InverseZ => ztrans::inverse_z(cx, tc, f, x, y),
    }
}

/// A transform at a numeric point by its integral (`n_integrate`), when no closed form was found.
fn numeric_point<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, kind: Kind, f: T<'h>, x: T<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
    let b = tc.b;
    let h = b.heap();
    let ny = apply(cx, b, "n", &[y])?;
    if !matches!(h.view(ny), View::Real(_)) && call_of(b, ny).map(|(n, _)| n) != Some("complex".into()) {
        return Ok(None);
    }
    // the integrand must be numeric apart from x
    let inf = h.sym_term(h.sym("inf"));
    let (g, lo, factor) = match kind {
        Kind::Laplace => (b.mul(&[f, b.call("exp", &[b.neg(b.mul(&[ny, x]))])]), b.zero(), b.one()),
        Kind::BilateralLaplace => (b.mul(&[f, b.call("exp", &[b.neg(b.mul(&[ny, x]))])]), b.neg(inf), b.one()),
        Kind::Mellin => (b.mul(&[f, b.pow(x, b.sub(ny, b.one()))]), b.zero(), b.one()),
        Kind::FourierSin | Kind::InverseFourierSin if tc.opts.fourier.is_none() => {
            (b.mul(&[f, b.call("sin", &[b.mul(&[ny, x])])]), b.zero(), b.pow(b.div(b.int(2), b.sym(h.sym("pi"))), b.rat(1, 2)))
        }
        Kind::FourierCos | Kind::InverseFourierCos if tc.opts.fourier.is_none() => {
            (b.mul(&[f, b.call("cos", &[b.mul(&[ny, x])])]), b.zero(), b.pow(b.div(b.int(2), b.sym(h.sym("pi"))), b.rat(1, 2)))
        }
        _ => return Ok(None),
    };
    let g = cx.eval(g)?;
    if !only_symbol(b, g, x) {
        return Ok(None);
    }
    let spec = h.call(b.hd.list, &[x, lo, inf]);
    // at most 3 s: an integral over an infinite range that diverges or oscillates must not run on (B23-13)
    let r = apply(cx, b, "time_constrained", &[b.call("n_integrate", &[g, spec]), b.int(3)])?;
    if matches!(h.view(r), View::Real(_)) || call_of(b, r).map(|(n, _)| n) == Some("complex".into()) {
        let nf = apply(cx, b, "n", &[factor])?;
        return Ok(Some(cx.eval(b.mul(&[nf, r]))?));
    }
    Ok(None)
}

/// Does `t` mention no symbol but `x` and the constants?
fn only_symbol<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) if u != x => {
                if !matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma" | "catalan" | "golden_ratio" | "im" | "inf") {
                    return false;
                }
            }
            View::App { args, .. } => stack.extend_from_slice(args),
            _ => {}
        }
    }
    true
}

/// The integrand normalised: `e^u` as `exp(u)`, `heaviside_theta` as `unit_step` (their values at the jump do
/// not matter for an integral), evaluated.
fn normal<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>) -> Res<T<'h>> {
    let r = rename(b, f);
    if r == f {
        return Ok(f);
    }
    cx.eval(r)
}

fn rename<'h>(b: &Build<'h>, t: T<'h>) -> T<'h> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return t };
    let new: Vec<T<'h>> = args.iter().map(|&a| rename(b, a)).collect();
    if b.sym_of(head) == Some(b.hd.pow) && new.len() == 2 && b.sym_of(new[0]) == Some(b.hd.e) {
        return b.call("exp", &[new[1]]);
    }
    if b.sym_of(head) == Some(h.sym("heaviside_theta")) && new.len() == 1 {
        return b.call("unit_step", &new);
    }
    if b.sym_of(head) == Some(h.sym("unit_box")) && new.len() > 1 {
        let parts: Vec<T<'h>> = new.iter().map(|&a| b.call("unit_box", &[a])).collect();
        return h.call(b.hd.mul, &parts);
    }
    if new.as_slice() == args {
        t
    } else {
        b.app(head, &new)
    }
}

/// Exponentials of sums split into the factors free of `x` and the others (`exp(-x - 3 y^2)` is `exp(-3 y^2)
/// exp(-x)` for a transform in `x`), so that the constant part is a constant factor.
fn split_exponentials<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<T<'h>> {
    fn walk<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> T<'h> {
        let h = b.heap();
        let View::App { head, args } = h.view(t) else { return t };
        let new: Vec<T<'h>> = args.iter().map(|&a| walk(b, a, x)).collect();
        if b.sym_of(head) == Some(b.hd.pow) && new.len() == 2 && b.sym_of(new[0]) == Some(b.hd.e) {
            if let Some(ts) = args_of(b, new[1], b.hd.add) {
                let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = ts.iter().partition(|&&u| is_free(b, u, x));
                if !free.is_empty() && !dep.is_empty() {
                    return h.call(b.hd.mul, &[b.call("exp", &[b.add(&free)]), b.call("exp", &[b.add(&dep)])]);
                }
            }
        }
        if new.as_slice() == args {
            t
        } else {
            h.app(head, &new)
        }
    }
    let r = walk(b, f, x);
    if r == f {
        return Ok(f);
    }
    cx.eval(r)
}

/// The value of a transform written as the tables write it (decision n14e.4): `sqrt(pi) (1/pi)^(1/2)` combined,
/// a common rational factor of an exponent taken out (`exp((1/4) (p^2 + s^2))`), linear factors of the variable
/// `y` in denominators with odd powers given a positive leading coefficient (`z/(z - a)`, not `-z/(a - z)`), and
/// bounds on `re(y)` as one chain (`-1 < re(s) and re(s) < 1`).
pub(crate) fn finish<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>, y: T<'h>, flip: bool) -> Res<T<'h>> {
    let h = b.heap();
    let mut v = v;
    // a number with a zero imaginary part is real
    if let Some((n, [re, im])) = call_of(b, v) {
        if n == "complex" && matches!(h.view(*im), View::Real(z) if z == 0.0) {
            v = *re;
        }
    }
    // under assumptions: refined (sqrt(a^2) is a when a > 0)
    // (the factor (1/pi)^(1/2) of sqrt(2/pi) kept apart: refine would write it pi^(-1/2), which simplify does not
    // identify with the tables' form)
    if !cx.assumptions().is_empty() && laplace::size(b, v) < 300 {
        let pi = h.sym_term(h.sym("pi"));
        let inv_root = h.call(b.hd.pow, &[h.call(b.hd.pow, &[pi, b.int(-1)]), b.rat(1, 2)]);
        let fs = factors(b, v);
        let (kept, rest): (Vec<T<'h>>, Vec<T<'h>>) = fs.iter().partition(|&&g| g == inv_root);
        let rest = b.mul(&rest);
        let r = apply(cx, b, "refine", &[rest])?;
        if !mc_calculus::common::is_failure(b, r) && laplace::size(b, r) <= laplace::size(b, rest) && is_free(b, r, inv_root) == is_free(b, rest, inv_root) {
            let mut all = kept;
            all.push(r);
            v = cx.eval(b.mul(&all))?;
        }
    }
    let r = finish_walk(cx, b, v, y, flip)?;
    if r == v {
        return Ok(v);
    }
    cx.eval(r)
}

fn finish_walk<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, y: T<'h>, flip: bool) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let mut new = Vec::with_capacity(args.len());
    for &a in args {
        new.push(finish_walk(cx, b, a, y, flip)?);
    }
    let changed = new.as_slice() != args;
    let hs = b.sym_of(head);
    let pi = h.sym_term(h.sym("pi"));
    // conditions: a < re(y) and re(y) < c
    if hs == Some(h.sym("and")) {
        let re = cx.eval(b.call("re", &[y]))?;
        let (mut lo, mut hi, mut rest) = (None, None, vec![]);
        for &c in &new {
            match call_of(b, c) {
                Some((n, [l, r])) if n == "greater" && *l == re && lo.is_none() => lo = Some(*r),
                Some((n, [l, r])) if n == "less" && *r == re && lo.is_none() => lo = Some(*l),
                Some((n, [l, r])) if n == "less" && *l == re && hi.is_none() => hi = Some(*r),
                Some((n, [l, r])) if n == "greater" && *r == re && hi.is_none() => hi = Some(*l),
                _ => rest.push(c),
            }
        }
        if let (Some(l), Some(u)) = (lo, hi) {
            let mut out = vec![h.call(h.sym("less"), &[l, re]), h.call(h.sym("less"), &[re, u])];
            out.extend(rest);
            return Ok(h.call(h.sym("and"), &out));
        }
    }
    // (pi^k)^q with k q an integer (D21-15: (pi^-2)^(1/2) is 1/pi)
    if hs == Some(b.hd.pow) && new.len() == 2 {
        if let Some(&[c, k]) = args_of(b, new[0], b.hd.pow) {
            if c == pi {
                let e = cx.eval(b.mul(&[k, new[1]]))?;
                if matches!(h.view(e), View::Int(_)) && matches!(h.view(new[1]), View::Rational { .. }) {
                    return Ok(b.pow(pi, e));
                }
            }
        }
    }
    if hs == Some(b.hd.mul) {
        // abs(u) sign(u) is u (D21-15)
        for i in 0..new.len() {
            let Some((n1, [u])) = call_of(b, new[i]) else { continue };
            if n1 != "abs" {
                continue;
            }
            if let Some(j) = new.iter().position(|&g| matches!(call_of(b, g), Some((n2, [v])) if n2 == "sign" && v == u)) {
                let mut out: Vec<T<'h>> = new.iter().enumerate().filter(|&(k, _)| k != i && k != j).map(|(_, &g)| g).collect();
                out.push(*u);
                return Ok(b.mul(&out));
            }
        }
        // sqrt(pi)-type factors with (1/pi)^(1/2)
        let inv_root = h.call(b.hd.pow, &[h.call(b.hd.pow, &[pi, b.int(-1)]), b.rat(1, 2)]);
        if let Some(i) = new.iter().position(|&g| g == inv_root) {
            if let Some(j) = new.iter().position(|&g| g == pi || matches!(args_of(b, g, b.hd.pow), Some(&[p, _]) if p == pi)) {
                let e = if new[j] == pi { b.one() } else { args_of(b, new[j], b.hd.pow).unwrap()[1] };
                let mut out: Vec<T<'h>> = new.iter().enumerate().filter(|&(k, _)| k != i && k != j).map(|(_, &g)| g).collect();
                out.push(b.pow(pi, b.sub(e, b.rat(1, 2))));
                return Ok(b.mul(&out));
            }
        }
        // odd powers of linear factors a - y in the denominator: (y - a) with the sign outside
        let mut sign_flip = false;
        let mut out = Vec::with_capacity(new.len());
        for &g in &new {
            if let Some(&[base, e]) = args_of(b, g, b.hd.pow) {
                if let View::Int(k) = h.view(e) {
                    if flip && k < 0 && k % 2 != 0 {
                        if let Some((a, _)) = linear(cx, b, base, y)? {
                            if matches!(h.view(a), View::Int(n) if n < 0) {
                                let nb = cx.eval(b.neg(base))?;
                                out.push(b.powi(nb, k));
                                sign_flip = !sign_flip;
                                continue;
                            }
                        }
                    }
                }
            }
            out.push(g);
        }
        if sign_flip {
            out.push(b.int(-1));
            return Ok(b.mul(&out));
        }
    }
    // exp(c (u1 + u2 + ...)) with the common rational factor c of the terms
    if hs == Some(b.hd.pow) && new.len() == 2 && b.sym_of(new[0]) == Some(b.hd.e) {
        if let Some(ts) = args_of(b, new[1], b.hd.add) {
            let mut coefs = vec![];
            for &u in ts {
                let c = match args_of(b, u, b.hd.mul) {
                    Some(fs) => fs.iter().copied().find(|&g| matches!(h.view(g), View::Int(_) | View::Rational { .. })).unwrap_or(b.one()),
                    None if matches!(h.view(u), View::Int(_) | View::Rational { .. }) => u,
                    None => b.one(),
                };
                coefs.push(cx.eval(b.call("abs", &[c]))?);
            }
            let g = cx.eval(b.call("gcd", &coefs))?;
            if matches!(h.view(g), View::Int(_) | View::Rational { .. }) && g != b.one() && !b.is_zero(g) {
                let inner = cx.eval(b.div(new[1], g))?;
                return Ok(h.call(b.hd.pow, &[new[0], h.call(b.hd.mul, &[g, inner])]));
            }
        }
    }
    Ok(if changed { h.app(head, &new) } else { t })
}

// ---------------------------------------------------------------- helpers shared by the transforms

/// The name of the head of a call and its arguments, with `e^u` (the evaluated form of `exp(u)`) as `exp(u)`.
pub(crate) fn call_of<'h>(b: &Build<'h>, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    if let Some(args) = args_of(b, t, b.hd.pow) {
        if args.len() == 2 && b.sym_of(args[0]) == Some(b.hd.e) {
            return Some(("exp".to_string(), &args[1..]));
        }
    }
    mc_calculus::common::call_of(b, t)
}

/// `u = a x + c` with `a`, `c` free of `x`: `(a, c)`.
pub(crate) fn linear<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, x: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
    if is_free(b, u, x) {
        return Ok(None);
    }
    let a = apply(cx, b, "d", &[u, x])?;
    if !is_free(b, a, x) || b.is_zero(a) {
        return Ok(None);
    }
    let c = apply(cx, b, "expand", &[b.sub(u, b.mul(&[a, x]))])?;
    if !is_free(b, c, x) {
        return Ok(None);
    }
    Ok(Some((a, c)))
}

/// The factors of a product (the term itself otherwise).
pub(crate) fn factors<'h>(b: &Build<'h>, t: T<'h>) -> Vec<T<'h>> {
    match args_of(b, t, b.hd.mul) {
        Some(a) => a.to_vec(),
        None => vec![t],
    }
}

/// The terms of a sum (the term itself otherwise).
pub(crate) fn terms<'h>(b: &Build<'h>, t: T<'h>) -> Vec<T<'h>> {
    match args_of(b, t, b.hd.add) {
        Some(a) => a.to_vec(),
        None => vec![t],
    }
}

/// `f` as (the product of its factors free of `x`, the product of the others).
pub(crate) fn split_constant<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> (T<'h>, T<'h>) {
    let (mut free, mut dep) = (vec![], vec![]);
    for g in factors(b, f) {
        if is_free(b, g, x) {
            free.push(g);
            continue;
        }
        // exp(c + u(x)): exp(c) is a constant factor (the evaluator joins the two exponentials again)
        if let Some(&[e, u]) = args_of(b, g, b.hd.pow) {
            if b.sym_of(e) == Some(b.hd.e) {
                if let Some(ts) = args_of(b, u, b.hd.add) {
                    let (c, d): (Vec<T<'h>>, Vec<T<'h>>) = ts.iter().partition(|&&v| is_free(b, v, x));
                    if !c.is_empty() {
                        free.push(b.call("exp", &[b.add(&c)]));
                        dep.push(b.call("exp", &[b.add(&d)]));
                        continue;
                    }
                }
            }
        }
        dep.push(g);
    }
    (b.mul(&free), b.mul(&dep))
}

/// Does `f` hold a function of `x` with no definition (`g(x)`, `g'(x)`, `g(2 x)`), so that a transform of it can
/// only be written with the transform itself?
pub(crate) fn unknown_function<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if is_free(b, u, x) {
            continue;
        }
        let View::App { head, args } = h.view(u) else { continue };
        match h.view(head) {
            View::Sym(s) if !cx.is_defined(s) => return true,
            // derivative(n)(g)(x)
            View::App { .. } if b.as_derivative(head).is_some() => return true,
            _ => {}
        }
        stack.extend_from_slice(args);
    }
    false
}

/// Is `t` free of `x` outside the transforms it holds (`laplace_transform(g(t), t, s)` mentions `t` as its own
/// bound variable)?
pub(crate) fn free_outside<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    if t == x {
        return false;
    }
    let View::App { head, args } = h.view(t) else { return true };
    if let Some(s) = b.sym_of(head) {
        if Kind::ALL.iter().any(|k| h.sym(k.name()) == s) {
            return true;
        }
    }
    free_outside(b, head, x) && args.iter().all(|&a| free_outside(b, a, x))
}

/// `name(f, x, y)` unevaluated (a part of a transform no rule reaches).
pub(crate) fn held<'h>(b: &Build<'h>, kind: Kind, f: T<'h>, x: T<'h>, y: T<'h>) -> T<'h> {
    b.call(kind.name(), &[f, x, y])
}

/// Is `t` a call of `kind`'s function (an unevaluated transform)?
pub(crate) fn has_held<'h>(b: &Build<'h>, t: T<'h>, kind: Kind) -> bool {
    let h = b.heap();
    !is_free(b, t, h.sym_term(h.sym(kind.name())))
}

/// `integrate(g, [x, lo, hi])` with `generate_conditions` as in `opts` (and the facts of the context); the value
/// or `None` (unevaluated, divergent, a failure).
pub(crate) fn definite<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    g: T<'h>,
    x: T<'h>,
    lo: T<'h>,
    hi: T<'h>,
    conditions: bool,
) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let range = h.call(b.hd.list, &[x, lo, hi]);
    let opt = h.call(
        h.sym("options"),
        &[h.call(h.sym(names::RULE), &[h.sym_term(h.sym("generate_conditions")), cx.bool(conditions)])],
    );
    let r = apply(cx, b, "integrate", &[g, range, opt])?;
    if matches!(h.view(r), View::App { head, .. } if b.sym_of(head) == Some(h.sym("integrate"))) {
        return Ok(None);
    }
    if !is_free(b, r, h.sym_term(h.sym("integrate"))) || mc_calculus::common::is_failure(b, r) {
        return Ok(None);
    }
    Ok(Some(r))
}

/// The value of a `conditional_expression(v, c)` and its condition (`(t, None)` for other terms).
pub(crate) fn conditional<'h>(b: &Build<'h>, t: T<'h>) -> (T<'h>, Option<T<'h>>) {
    match call_of(b, t) {
        Some((n, [v, c])) if n == "conditional_expression" => (*v, Some(*c)),
        _ => (t, None),
    }
}

/// `v` with the condition `c` when conditions are asked for.
pub(crate) fn with_condition<'h>(cx: &mut NativeCx<'_, '_, 'h>, tc: &Cx<'_, 'h>, v: T<'h>, c: Option<T<'h>>) -> Res<T<'h>> {
    match c {
        Some(c) if (tc.opts.conditions || tc.opts.strips) && !cx.is_true(c) => cx.eval(tc.b.call("conditional_expression", &[v, c])),
        _ => Ok(v),
    }
}

/// `true` when both are conditions to join: `a and b` (each may be absent).
pub(crate) fn and<'h>(b: &Build<'h>, a: Option<T<'h>>, c: Option<T<'h>>) -> Option<T<'h>> {
    match (a, c) {
        (Some(a), Some(c)) => Some(b.call("and", &[a, c])),
        (a, None) => a,
        (None, c) => c,
    }
}

/// The delta functions of the discrete transforms: `discrete_delta(n1, ..., nk)` is 1 when every argument is 0 and
/// 0 when one is a nonzero number; `kronecker_delta(n1, ..., nk)` is 1 when the arguments are equal and 0 when two
/// are different numbers.
pub struct Delta {
    /// `true` for `kronecker_delta`
    pub kronecker: bool,
}

impl Native for Delta {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let num = |t: T<'h>| matches!(h.view(t), View::Int(_) | View::Rational { .. } | View::Real(_) | View::BigInt { .. });
        if args.is_empty() {
            return Ok(Reply::Done(h.int(1)));
        }
        if self.kronecker {
            if args.len() == 1 {
                return Ok(match h.view(args[0]) {
                    View::Int(0) => Reply::Done(h.int(1)),
                    _ if num(args[0]) => Reply::Done(h.int(0)),
                    _ => Reply::Decline,
                });
            }
            if args.iter().all(|&a| a == args[0]) {
                return Ok(Reply::Done(h.int(1)));
            }
            // each argument against the first: equal (a zero difference, 1.0 == 1), provably unequal (a non-zero
            // number, or a numeric constant whose value is not 0: pi and 3, x and x + 1), or undecided (ui B21-2,
            // B21-5)
            let mut all_equal = true;
            for &a in &args[1..] {
                let d = cx.eval(h.call(h.sym("add"), &[args[0], h.call(h.sym("mul"), &[h.int(-1), a])]))?;
                let zero = match h.view(d) {
                    View::Int(v) => v == 0,
                    View::Real(r) => r == 0.0,
                    _ if num(d) => false,
                    _ => {
                        let v = cx.eval(h.call(h.sym("n"), &[d, h.int(30)]))?;
                        let big = cx.eval(h.call(h.sym("greater"), &[h.call(h.sym("abs"), &[v]), h.call(h.sym("pow"), &[h.int(10), h.int(-25)])]))?;
                        if cx.is_true(big) {
                            false
                        } else {
                            all_equal = false;
                            continue;
                        }
                    }
                };
                if !zero {
                    return Ok(Reply::Done(h.int(0)));
                }
            }
            if all_equal {
                return Ok(Reply::Done(h.int(1)));
            }
            return Ok(Reply::Decline);
        }
        if args.iter().any(|&a| num(a) && !matches!(h.view(a), View::Int(0)) && !matches!(h.view(a), View::Real(r) if r == 0.0)) {
            return Ok(Reply::Done(h.int(0)));
        }
        if args.iter().all(|&a| matches!(h.view(a), View::Int(0)) || matches!(h.view(a), View::Real(r) if r == 0.0)) {
            return Ok(Reply::Done(h.int(1)));
        }
        Ok(Reply::Decline)
    }
}

/// Register the natives of this crate with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag).
pub fn install(k: &mut mc_eval::Kernel, h: &mc_core::Heap, stop: &Arc<AtomicBool>) {
    use mc_eval::Attributes;
    // the transforms read the assumption context (strips, signs of parameters)
    let reads = Attributes { uses_assumptions: true, ..Attributes::default() };
    for kind in Kind::ALL {
        k.register(h.sym(kind.name()), reads, mc_calculus::common::wrap(kind.name(), Transform { kind, stop: stop.clone() }));
    }
    k.register(h.sym("discrete_delta"), Attributes::default(), Box::new(Delta { kronecker: false }));
    k.register(h.sym("kronecker_delta"), Attributes::default(), Box::new(Delta { kronecker: true }));
}
