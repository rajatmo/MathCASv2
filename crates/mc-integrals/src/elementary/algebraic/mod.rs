//! Integrands with roots (roadmap item 14B): a portfolio of substitutions tried in turn, every answer verified
//! by differentiation and the smallest kept (by `mc-simplify`'s cost).
//!
//! [`integrate`] reads the radicals of the integrand (powers with a non-integer rational exponent of a base that
//! depends on `x`) and tries, in order:
//!
//! 1. [`binomial`]: Chebyshev's theorem for `x^m (a + b x^n)^p`: its substitutions in the integrable cases, a proof
//!    of non-elementarity otherwise (decided by `mc_integrate::algebraic::binomial`);
//! 2. [`rationalise`]: the radicals of a monomial, linear or linear-fractional base `L` together, `t = L^(1/N)`;
//!    nested radicals lose their innermost layer and the rest goes round again;
//! 3. [`radical`]: Hermite reduction in `Q(x)[y]`, `y^q = g` (Ostrogradsky's undetermined coefficients,
//!    `mc_integrate::algebraic::hermite`): the algebraic part; for square roots with poles only at infinity the
//!    logarithmic part `mu log(A + B y)` from a unit found by Abel's continued fraction
//!    (`mc_integrate::algebraic::abel`); the remainder to the other methods; a remainder of the second kind proves
//!    non-elementarity;
//! 4. [`trigsub`] and [`euler`]: the trigonometric substitutions (`sin`, `tan`, `sec` forms) and Euler's three
//!    substitutions for `sqrt(a x^2 + b x + c)`, each one tried and scored;
//! 5. [`lower`]: lowering the degree of the radicand, `u = x^k` when the differential is a function of `x^k`,
//!    derivative-divides `u = h(x)` with the inner component of a functional decomposition of the radicand (F2's
//!    `decompose`), and Goursat's involutions `x -> c/x` (`u = x + c/x`) for quartic radicals;
//! 6. [`elliptic`]: Legendre's forms `elliptic_f`, `elliptic_e`, `elliptic_pi` for even quartic radicals and
//!    products of two even quadratic ones, and the reduction of cubic and quartic radicals with a rational root to
//!    them (`x = e + t^2`, `x = e + 1/u`);
//! 7. a sum term by term.
//!
//! Sub-integrals (the integrand after a substitution) go to the classic dispatcher (rational functions, the
//! trigonometric rules) or back into this portfolio, one level deeper (at most [`MAX_DEPTH`]); the whole stage runs
//! under one deadline. Every candidate is checked with the portfolio's check (`d(F)/dx - f` numerically at three
//! points, then `simplify`), so a substitution that is valid only on part of the line, or a branch choice, can never
//! produce a wrong answer: it is dropped.
pub mod binomial;
pub mod elliptic;
pub mod euler;
pub mod lower;
pub mod radical;
pub mod rationalise;
pub mod trigsub;

use super::{rewrite, spend};
use crate::{apply, args_of, classic, is_free, portfolio};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_arith::Num;
use mc_eval::{NativeCx, Res};
use mc_integrate::algebraic::QPoly;
use rustc_hash::FxHashSet;
use std::cell::Cell;
use std::time::{Duration, Instant};

/// The time of the whole stage (all levels).
pub const TIME: Duration = Duration::from_millis(2000);
/// The deepest nesting of substitutions.
pub const MAX_DEPTH: u32 = 5;

thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Has the stage's time run out?
pub(crate) fn late() -> bool {
    DEADLINE.with(|c| c.get()).is_some_and(|d| Instant::now() >= d)
}

/// The outcome of the stage.
#[derive(Clone, Debug, Default)]
pub struct Answer<'h> {
    /// the smallest verified antiderivative
    pub found: Option<T<'h>>,
    /// a proof that no elementary antiderivative exists
    pub proof: Option<String>,
}

/// A radical of the integrand: `term = base^exp`, `exp = num/den` not an integer.
#[derive(Clone, Copy, Debug)]
pub struct Radical<'h> {
    /// the power
    pub term: T<'h>,
    /// its base (depends on x)
    pub base: T<'h>,
    /// the exponent's numerator
    pub num: i64,
    /// the exponent's denominator (at least 2)
    pub den: i64,
}

/// What the methods may use: the depth of the problem and which methods are off.
#[derive(Clone, Copy, Debug)]
pub struct Level {
    /// the nesting of substitutions
    pub depth: u32,
    /// Hermite reduction off (for its own remainder)
    pub no_hermite: bool,
}

/// The algebraic stage on `f` in `x` from the portfolio: one deadline, a fresh budget of the elementary rules.
pub fn top<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<Answer<'h>> {
    if radicals(b, f, x).is_empty() {
        return Ok(Answer::default());
    }
    let end = Instant::now() + TIME;
    let saved = DEADLINE.with(|c| c.replace(Some(end)));
    super::set_deadline(Some(end));
    let budget = super::fresh_budget();
    let r = integrate(cx, b, f, x, Level { depth: 1, no_hermite: false });
    super::restore_budget(budget);
    super::set_deadline(None);
    DEADLINE.with(|c| c.set(saved));
    let mut r = r?;
    // the radicals of the answer written back from substitutions, tidied (an identity)
    if let Some(t) = r.found {
        r.found = Some(tidy(cx, b, t, x)?);
    }
    Ok(r)
}

/// The portfolio on `f` (see the module): the smallest verified answer, and a proof of non-elementarity when a
/// method found one.
pub fn integrate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>, lv: Level) -> Res<Answer<'h>> {
    let mut ans = Answer::default();
    if lv.depth > MAX_DEPTH || late() || !spend(1) {
        return Ok(ans);
    }
    cx.tick(1)?;
    let f = tidy(cx, b, f, x)?;
    let rads = radicals(b, f, x);
    if rads.is_empty() {
        return Ok(ans);
    }
    let mut c = Cands { f, x, found: vec![], weights: vec![] };
    // the table's established forms (powers of a quadratic, one function of a linear argument)
    let (free, dep): (Vec<T<'h>>, Vec<T<'h>>) = super::factors(b, f).into_iter().partition(|&g| is_free(b, g, x));
    if let Some(r) = super::table::single(cx, b, b.mul(&dep), x)? {
        c.offer(cx, b, b.mul(&[b.mul(&free), r]))?;
    }
    binomial::run(cx, b, &mut c, &rads, lv, &mut ans.proof)?;
    if ans.proof.is_none() {
        rationalise::run(cx, b, &mut c, &rads, lv)?;
    }
    if !lv.no_hermite {
        radical::run(cx, b, &mut c, &rads, lv, &mut ans.proof)?;
    }
    if ans.proof.is_none() {
        trigsub::run(cx, b, &mut c, &rads, lv)?;
    }
    if c.found.is_empty() && ans.proof.is_none() {
        euler::run(cx, b, &mut c, &rads, lv)?;
    }
    if c.found.is_empty() && ans.proof.is_none() {
        lower::powers(cx, b, &mut c, &rads, lv)?;
        lower::decomposed(cx, b, &mut c, &rads, lv)?;
        lower::goursat(cx, b, &mut c, &rads, lv)?;
    }
    // Legendre's forms also when only the hypergeometric form of a binomial was found (the smaller one is kept)
    if c.found.iter().all(|&t| !crate::portfolio::is_free_head(b, t, "hypergeometric_2f1")) {
        elliptic::run(cx, b, &mut c, &rads, lv)?;
    }
    if c.found.is_empty() {
        split(cx, b, &mut c, &rads, lv, &mut ans.proof)?;
    }
    if c.found.is_empty() && ans.proof.is_none() {
        if let Some(terms) = args_of(b, f, b.hd.add) {
            let mut out = vec![];
            for &t in terms {
                match sub(cx, b, t, x, lv)? {
                    Some(r) => out.push(r),
                    None => break,
                }
            }
            if out.len() == terms.len() {
                c.offer(cx, b, b.add(&out))?;
            }
        }
    }
    ans.found = smallest(b, c.found, &c.weights);
    Ok(ans)
}

/// The verified candidates of one problem.
pub struct Cands<'h> {
    /// the integrand
    pub f: T<'h>,
    /// the variable
    pub x: T<'h>,
    /// the verified answers
    pub found: Vec<T<'h>>,
    /// their weights in percent (see [`Cands::offer_weighted`])
    pub weights: Vec<u64>,
}

/// The weight of answers whose back-substitution divides by a power of `x` (Euler's second substitution,
/// Chebyshev's third case): they are singular at 0 where the integrand may not be, so a form of about the same size
/// without that is preferred.
pub const SINGULAR: u64 = 130;

impl<'h> Cands<'h> {
    /// Check `r` and keep it when it is an antiderivative of `f`.
    pub fn offer(&mut self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, r: T<'h>) -> Res<bool> {
        self.offer_weighted(cx, b, r, 100)
    }

    /// [`Cands::offer`] with a weight in percent of the answer's cost in the choice of the smallest.
    pub fn offer_weighted(&mut self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, r: T<'h>, weight: u64) -> Res<bool> {
        if self.found.contains(&r) {
            return Ok(true);
        }
        if portfolio::verified(cx, b, self.f, r, self.x)? {
            self.found.push(r);
            self.weights.push(weight);
            return Ok(true);
        }
        Ok(false)
    }
}

/// The smallest answer by `mc-simplify`'s cost times its weight (the first on a tie).
fn smallest<'h>(b: &Build<'h>, found: Vec<T<'h>>, weights: &[u64]) -> Option<T<'h>> {
    let mut cost = mc_simplify::cost::Cost::new(b.heap());
    let mut best: Option<(u64, T<'h>)> = None;
    for (t, w) in found.into_iter().zip(weights) {
        let c = cost.size(t) * w;
        if best.is_none_or(|(bc, _)| c < bc) {
            best = Some((c, t));
        }
    }
    best.map(|(_, t)| t)
}

/// A sub-integral one level deeper: the classic dispatcher when `g` has no radicals, else this portfolio first.
pub fn sub<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>, lv: Level) -> Res<Option<T<'h>>> {
    Ok(sub_answer(cx, b, g, t, Level { depth: lv.depth + 1, no_hermite: false })?.found)
}

/// A sub-integral at the level `lv` (see [`sub`]), with the proof of non-elementarity when there is one.
pub fn sub_answer<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, g: T<'h>, t: T<'h>, lv: Level) -> Res<Answer<'h>> {
    if late() || lv.depth > MAX_DEPTH {
        return Ok(Answer::default());
    }
    if radicals(b, g, t).is_empty() {
        return Ok(Answer { found: classic(cx, b, g, t, lv.depth)?, proof: None });
    }
    let a = integrate(cx, b, g, t, lv)?;
    if a.found.is_some() || a.proof.is_some() {
        return Ok(a);
    }
    Ok(Answer { found: classic(cx, b, g, t, lv.depth)?, proof: None })
}

/// Radicals of a rational function that is not linear-fractional split: `(N/D)^e = N^e D^(-e)` (where `N` and
/// `D` are positive, so at the points of the check), and the portfolio on the result; a proof for it holds for `f`
/// (the two agree on an interval).
fn split<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    c: &mut Cands<'h>,
    rads: &[Radical<'h>],
    lv: Level,
    proof: &mut Option<String>,
) -> Res<()> {
    let x = c.x;
    let mut reps = vec![];
    for r in rads {
        let (n, d) = num_den(cx, b, r.base)?;
        if is_free(b, d, x) || !radicals(b, r.base, x).is_empty() {
            continue;
        }
        let e = b.rat(r.num, r.den);
        reps.push((r.term, b.mul(&[b.pow(n, e), b.pow(d, b.neg(e))])));
    }
    if reps.is_empty() {
        return Ok(());
    }
    let g = change(b, c.f, x, &reps, x);
    let a = sub_answer(cx, b, g, x, Level { depth: lv.depth + 1, no_hermite: false })?;
    if let Some(r) = a.found {
        c.offer(cx, b, r)?;
    }
    if proof.is_none() {
        *proof = a.proof;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// helpers

/// `f` with the bases of its radicals expanded and over one denominator (after a substitution they are
/// `3 - (-1 + t^2)` and the like), innermost first.
fn tidy<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>, x: T<'h>) -> Res<T<'h>> {
    let mut reps = vec![];
    for r in radicals(b, f, x).iter().rev() {
        if !radicals(b, r.base, x).is_empty() || args_of(b, r.base, b.hd.add).is_none() {
            continue;
        }
        let e = apply(cx, b, "expand", &[r.base])?;
        let nb = apply(cx, b, "together", &[e])?;
        if nb != r.base {
            reps.push((r.term, b.pow(nb, b.rat(r.num, r.den))));
        }
    }
    if reps.is_empty() {
        return Ok(f);
    }
    Ok(rewrite(b, f, &mut |s| reps.iter().find(|(from, _)| *from == s).map(|(_, to)| *to)))
}

/// The radicals of `f` in `x` (distinct), innermost last.
pub fn radicals<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>) -> Vec<Radical<'h>> {
    let h = b.heap();
    let mut seen: FxHashSet<T<'h>> = FxHashSet::default();
    let mut out = vec![];
    let mut stack = vec![f];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) || is_free(b, u, x) {
            continue;
        }
        let View::App { args, .. } = h.view(u) else { continue };
        if let Some([base, e]) = args_of(b, u, b.hd.pow) {
            if let Some((num, den)) = rat_exp(b, *e) {
                if den >= 2 {
                    out.push(Radical { term: u, base: *base, num, den });
                }
            }
        }
        stack.extend_from_slice(args);
    }
    out
}

/// A rational exponent `p/q` (q > 0) with machine-sized parts.
pub fn rat_exp<'h>(b: &Build<'h>, e: T<'h>) -> Option<(i64, i64)> {
    let r = mc_integrate::terms::rat_of(&b.ar, e)?;
    mc_integrate::algebraic::parts(&r)
}

/// The coefficients of a polynomial in `x` with rational coefficients.
pub fn qpoly<'h>(b: &Build<'h>, t: T<'h>, x: T<'h>) -> Option<QPoly> {
    let cs = mc_integrate::terms::coeffs_in(&b.ar, t, x)?;
    let v: Option<Vec<Rat>> = cs.iter().map(|&c| mc_integrate::terms::rat_of(&b.ar, c)).collect();
    Some(mc_integrate::algebraic::poly(&v?))
}

/// Numerator and denominator of a rational function of `x` with rational coefficients.
pub fn qratfun<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>) -> Res<Option<(QPoly, QPoly)>> {
    let (n, d) = num_den(cx, b, t)?;
    Ok(match (qpoly(b, n, x), qpoly(b, d, x)) {
        (Some(n), Some(d)) if !d.is_empty() => Some((n, d)),
        _ => None,
    })
}

/// Numerator and denominator of `together(t)`.
pub fn num_den<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<(T<'h>, T<'h>)> {
    let t = apply(cx, b, "together", &[t])?;
    let n = apply(cx, b, "numerator", &[t])?;
    let d = apply(cx, b, "denominator", &[t])?;
    Ok((n, d))
}

/// A rational number as a term.
pub fn rat<'h>(b: &Build<'h>, q: &Rat) -> T<'h> {
    b.ar.number(&Num::rat(q.clone()))
}

/// A polynomial with rational coefficients as a term in `x`.
pub fn qterm<'h>(b: &Build<'h>, p: &[Rat], x: T<'h>) -> T<'h> {
    let terms: Vec<T<'h>> = p.iter().enumerate().filter(|(_, c)| !c.is_zero()).map(|(k, c)| b.mul(&[rat(b, c), b.powi(x, k as i64)])).collect();
    b.add(&terms)
}

/// `f` with the subterms of `reps` replaced (outermost first) and `x` replaced by `xt`.
pub fn change<'h>(b: &Build<'h>, f: T<'h>, x: T<'h>, reps: &[(T<'h>, T<'h>)], xt: T<'h>) -> T<'h> {
    rewrite(b, f, &mut |s| {
        if s == x {
            return Some(xt);
        }
        reps.iter().find(|(from, _)| *from == s).map(|(_, to)| *to)
    })
}

/// Does `t` contain a power of `v` with a non-integer exponent?
pub fn has_root_of<'h>(b: &Build<'h>, t: T<'h>, v: T<'h>) -> bool {
    radicals(b, t, v).iter().any(|r| r.base == v)
}
