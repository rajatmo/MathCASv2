//! `simplify(e)`: the cheapest equivalent form found by a bounded search.
//!
//! The plan (`00-plan.md`, layer 2) replaces piles of heuristics by *a rule set plus a cost measure*: explore
//! equivalent forms, keep the cheapest. This is the first, search-based form of that idea (decision 2B.1); an
//! e-graph (equality saturation) can replace the search behind the same function later.
//!
//! - **Cost**: the measure of [`crate::cost`] (decision s-cost.1): a leaf count in which integers weigh by their
//!   digits, with tie-breaks (fewer negative powers, then fewer distinct functions); ties beyond those keep the
//!   order found (the input first, so `simplify` never trades a form for an equally simple one). One cache of
//!   subterm sizes serves every candidate of the call. `complexity_function: f` replaces it by a user measure.
//! - **Moves**: equivalences valid for all values of the variables (decision 2B.2): `together`, `cancel`, `factor`,
//!   `expand` (item K), `trig_reduce`, the Pythagorean substitutions followed by `expand` or `together`,
//!   `tan`/`sec`/... as sines and cosines followed by `together`, `exp_to_trig`, products of exponentials as one
//!   exponential, and the sign convention for sines and cosines of sums (decision 2B.4).
//!   Inside `assuming` (decision A.6), `refine` under the context's facts is one more move, and the input and the
//!   result are refined.
//! - **Search**: breadth first to [`DEPTH`] moves, keeping the [`BEAM`] cheapest new forms per level; forms are
//!   hash-consed, so duplicates are found by handle.
//! - **Budgets** (gap G73): a polynomial move (`together`, `cancel`, `factor`, `expand`) runs with the internal
//!   `time_budget:` option of item K ([`MOVE_SECONDS`]) and gives its input back past it; the whole search stops
//!   after [`SEARCH_SECONDS`] with the cheapest form found so far; a form more than [`GROWTH`] times the input's
//!   size (and above [`SMALL`] leaves) is not explored further.
//! - **Inside out**: the arguments of other functions are simplified first (`sin((x^2 - 1)/(x - 1))` becomes
//!   `sin(1 + x)`), and lists element by element.
use crate::facts::Facts;
use crate::refine::refine;
use crate::cost::{Cost, Measure, Score};
use crate::term::{apply, rewrite, B};
use crate::trig::{exp_combine, normal_signs, pythagoras, to_sin_cos, trig_reduce, Pythagoras};
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashSet;

/// Most moves from the input.
pub const DEPTH: usize = 3;
/// Forms kept per level of the search.
pub const BEAM: usize = 6;
/// Seconds one polynomial move may take (multivariate `factor` can be exponential in the degrees, gap G73).
pub const MOVE_SECONDS: f64 = 0.25;
/// Seconds the whole search of one term may take; past them the cheapest form found so far is the answer.
pub const SEARCH_SECONDS: f64 = 2.0;
/// A form larger than this many times the input (and than [`SMALL`] leaves) is not explored further.
pub const GROWTH: u64 = 8;
/// Below this many leaves a form is always explored.
pub const SMALL: u64 = 200;

/// A move of the search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Move {
    Together,
    Cancel,
    Factor,
    Expand,
    TrigReduce,
    SinExpand,
    CosExpand,
    SinTogether,
    CosTogether,
    SinCosTogether,
    ExpToTrig,
    ExpCombine,
    Signs,
    LogCombine,
    /// constant parts of exponents as coefficients (decision r17fix.1)
    PowerSplit,
    /// `gamma(z + k)/gamma(z)` by the recurrence (decision r17fix.2)
    GammaRatio,
    /// circular and hyperbolic functions as exponentials (`cosh(x) - sinh(x)` is `exp(-x)`; r17fix.2)
    TrigToExp,
    /// the special-function identities of [`crate::identities`] (round 19, phase 12C)
    Identities,
    /// round 19 moves of [`crate::moves`]
    Rationalize,
    PowerCollect,
    PythagorasOne,
    SqrtParity,
    ImSplit,
    ImCombine,
    LogPrimes,
    LinearEquation,
    CommonFactor,
    /// `function_expand` (in `full_simplify`, and with a user complexity measure)
    FunctionExpand,
    /// the n-th function of `transformation_functions:`
    User(usize),
    /// `refine` under the facts of the context (only inside `assuming`)
    Refine,
}

const MOVES: [Move; 28] = [
    Move::Together,
    Move::Cancel,
    Move::Factor,
    Move::Expand,
    Move::TrigReduce,
    Move::SinExpand,
    Move::CosExpand,
    Move::SinTogether,
    Move::CosTogether,
    Move::SinCosTogether,
    Move::ExpToTrig,
    Move::ExpCombine,
    Move::Signs,
    Move::LogCombine,
    Move::PowerSplit,
    Move::GammaRatio,
    Move::TrigToExp,
    Move::Identities,
    Move::Rationalize,
    Move::PowerCollect,
    Move::PythagorasOne,
    Move::SqrtParity,
    Move::ImSplit,
    Move::ImCombine,
    Move::LogPrimes,
    Move::LinearEquation,
    Move::CommonFactor,
    Move::FunctionExpand,
];

impl Move {
    /// Is the move left out with `trig: false`? (Only the trigonometric rewriting of round 19: the earlier moves
    /// stay, so `simplify(sin(x)^2 + cos(x)^2, trig: false)` is still 1 as before the option was read.)
    fn trig(self) -> bool {
        matches!(self, Move::PythagorasOne)
    }

    /// Is a form found with this move spot-checked before it is returned (the identities of round 19)?
    fn checked(self) -> bool {
        matches!(
            self,
            Move::Identities
                | Move::Rationalize
                | Move::PowerCollect
                | Move::PythagorasOne
                | Move::SqrtParity
                | Move::ImSplit
                | Move::ImCombine
                | Move::LogPrimes
                | Move::LinearEquation
                | Move::CommonFactor
                | Move::FunctionExpand
        )
    }

    /// Is a form of this move taken over the input at equal cost (the gamma form of `x gamma(x)`, as the
    /// reference's measure prefers it; in `full_simplify` also an identity's named function)?
    fn preferred_on_tie(self, full: bool) -> bool {
        matches!(self, Move::GammaRatio) || (full && matches!(self, Move::Identities))
    }
}

/// The settings of one call of `simplify` or `full_simplify`: the mode and the options (phase 12A).
pub struct Config<'h> {
    /// `full_simplify`: `function_expand` as a move, every subexpression searched bottom up, a deeper search
    pub full: bool,
    /// `trig: false` leaves out the trigonometric moves
    pub trig: bool,
    /// the default moves (`transformation_functions:` without `automatic` turns them off)
    pub automatic: bool,
    /// the functions of `transformation_functions:`, each one more move
    pub user: Vec<T<'h>>,
    /// a user complexity measure is in use
    pub user_measure: bool,
    /// the end of the call (`time_constraint:`)
    pub deadline: std::time::Instant,
    /// the facts of the context, for the spot check
    pub facts: Vec<T<'h>>,
    /// the symbols of the decision service
    pub syms: std::sync::Arc<crate::facts::Syms>,
    /// the arithmetic's stop flag (the builders of the search's parallel moves poll it)
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl<'h> Config<'h> {
    /// The defaults of `simplify` (`full` false) or `full_simplify`.
    pub fn new(full: bool, syms: std::sync::Arc<crate::facts::Syms>) -> Config<'h> {
        let secs = if full { FULL_SECONDS } else { CALL_SECONDS };
        Config {
            full,
            trig: true,
            automatic: true,
            user: vec![],
            user_measure: false,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs_f64(secs),
            facts: vec![],
            syms,
            stop: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    fn moves(&self) -> Vec<Move> {
        let mut v: Vec<Move> = if self.automatic {
            MOVES
                .into_iter()
                .filter(|m| self.trig || !m.trig())
                .filter(|m| *m != Move::FunctionExpand || self.full)
                .collect()
        } else {
            vec![]
        };
        v.extend((0..self.user.len()).map(Move::User));
        v
    }

    fn depth(&self) -> usize {
        if self.full {
            FULL_DEPTH
        } else {
            DEPTH
        }
    }

    fn beam(&self) -> usize {
        if self.full {
            FULL_BEAM
        } else {
            BEAM
        }
    }
}

/// Seconds a whole `simplify` call may take by default (`time_constraint:` changes it).
pub const CALL_SECONDS: f64 = 4.0;
/// Seconds a whole `full_simplify` call may take by default.
pub const FULL_SECONDS: f64 = 5.0;
/// Most moves from the input in `full_simplify`.
pub const FULL_DEPTH: usize = 4;
/// Forms kept per level in `full_simplify`.
pub const FULL_BEAM: usize = 8;

/// What a move works with: the facts (an empty set outside `assuming`), the settings, the cost.
struct Ctx<'a, 'c, 'f, 'h> {
    fx: &'a mut Facts<'f, 'h>,
    has_facts: bool,
    cfg: &'a Config<'h>,
    cost: &'c mut Cost<'h>,
}

fn step<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, k: &mut Ctx<'_, '_, '_, 'h>, m: Move, t: T<'h>) -> Res<T<'h>> {
    match m {
        Move::Refine => {
            if k.has_facts {
                refine(cx, b, k.fx, t)
            } else {
                Ok(t)
            }
        }
        Move::Identities => identities(cx, b, k.fx, t, false),
        Move::PythagorasOne => crate::moves::pythagoras_one(cx, b, k.cost, t),
        Move::FunctionExpand => crate::fexpand::function_expand(cx, b, k.fx, t),
        m => plain_step(cx, b, k.cfg, m, t),
    }
}

/// Does a move consult the facts or the measure (the others need only the builder)?
fn needs_context(m: Move) -> bool {
    matches!(m, Move::Refine | Move::Identities | Move::PythagorasOne | Move::FunctionExpand)
}

/// A move of the search as a parallel task (roadmap item 16A): its own builder, and its own facts and measure when it
/// consults them, made from the call's settings (a move's result depends only on the move, the form and the facts).
fn task_step<'h>(cx: &mut NativeCx<'_, '_, 'h>, cfg: &Config<'h>, has_facts: bool, measure: Measure<'h>, m: Move, t: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    let _scope = mc_arith::num::StopScope::enter(Some(&cfg.stop));
    let b = B::new(h, &cfg.stop);
    if !needs_context(m) {
        return plain_step(cx, &b, cfg, m, t);
    }
    if m == Move::Refine && !has_facts {
        return Ok(t);
    }
    let mut fx = Facts::new(cx, &b, &cfg.syms, &cfg.facts)?;
    let mut cost = Cost::with(h, measure);
    let mut k = Ctx { fx: &mut fx, has_facts, cfg, cost: &mut cost };
    step(cx, &b, &mut k, m, t)
}

/// The moves that need only the builder.
fn plain_step<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, cfg: &Config<'h>, m: Move, t: T<'h>) -> Res<T<'h>> {
    match m {
        Move::Refine | Move::Identities | Move::PythagorasOne | Move::FunctionExpand => Ok(t),
        Move::Together => bounded(cx, b, "together", t),
        Move::Cancel => bounded(cx, b, "cancel", t),
        Move::Factor => bounded(cx, b, "factor", t),
        Move::Expand => bounded(cx, b, "expand", t),
        Move::TrigReduce => trig_reduce(cx, b, t),
        Move::SinExpand => {
            let r = pythagoras(cx, b, t, Pythagoras::ToSin)?;
            apply(cx, b, "expand", r)
        }
        Move::CosExpand => {
            let r = pythagoras(cx, b, t, Pythagoras::ToCos)?;
            apply(cx, b, "expand", r)
        }
        Move::SinTogether => {
            let r = pythagoras(cx, b, t, Pythagoras::ToSin)?;
            apply(cx, b, "together", r)
        }
        Move::CosTogether => {
            let r = pythagoras(cx, b, t, Pythagoras::ToCos)?;
            apply(cx, b, "together", r)
        }
        Move::SinCosTogether => {
            let r = to_sin_cos(cx, b, t)?;
            apply(cx, b, "together", r)
        }
        Move::ExpToTrig => crate::trig::exp_to_trig_with(cx, b, t, false),
        Move::ExpCombine => exp_combine(cx, b, t),
        Move::Signs => normal_signs(cx, b, t),
        Move::LogCombine => log_combine(cx, b, t),
        Move::PowerSplit => crate::powers::power_split(cx, b, t),
        Move::GammaRatio => crate::powers::gamma_ratio(cx, b, t),
        Move::TrigToExp => crate::trig::trig_to_exp(cx, b, t),
        Move::Rationalize => crate::moves::rationalize(cx, b, t),
        Move::PowerCollect => crate::moves::power_collect(cx, b, t),
        Move::SqrtParity => crate::moves::sqrt_parity(cx, b, t),
        Move::ImSplit => crate::moves::im_split(cx, b, t),
        Move::ImCombine => crate::moves::im_combine(cx, b, t),
        Move::LogPrimes => crate::moves::log_primes(cx, b, t),
        Move::LinearEquation => crate::moves::linear_equation(cx, b, t),
        Move::CommonFactor => crate::moves::common_factor(cx, b, t),
        Move::User(i) => {
            let f = cfg.user[i];
            cx.eval(b.app(f, &[t]))
        }
    }
}

/// The identities of the table marked for `simplify` (only those marked `ALWAYS` when `always` is set), and the
/// families of sums, bottom up, evaluated.
fn identities<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, fx: &mut Facts<'_, 'h>, t: T<'h>, always: bool) -> Res<T<'h>> {
    use crate::identities::{family, node, SIMPLIFY};
    let r = rewrite(cx, b, t, &mut |cx, b, n| {
        if let Some((v, a)) = node(cx, b, fx, SIMPLIFY, n)? {
            if a || !always {
                return Ok(Some(v));
            }
        }
        if always {
            return Ok(None);
        }
        family(cx, b, SIMPLIFY, n)
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

/// Bits a combined logarithm's argument may have (`k log(q)` as `log(q^k)` is not tried past them).
const LOG_BITS: u64 = 4096;

/// The terms `k log(q)` of a sum (positive integers k, positive rationals q) as one logarithm of the product of
/// the `q^k` (decision s-cost.6): `log(2) + log(3)` is `log(6)`; `100 log(2)` is `log(2^100)`, which only a measure
/// that does not weigh digits prefers. Valid for every value: the arguments are positive numbers.
fn log_combine<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let positive_rational = |u: T<'h>| match h.view(u) {
        mc_api::View::Int(n) => n > 0,
        mc_api::View::BigInt { neg, .. } => !neg,
        mc_api::View::Rational { num, .. } => !matches!(h.view(num), mc_api::View::Int(n) if n < 0)
            && !matches!(h.view(num), mc_api::View::BigInt { neg: true, .. }),
        _ => false,
    };
    let bits = |u: T<'h>| -> u64 {
        let one = |v: T<'h>| match h.view(v) {
            mc_api::View::Int(n) => 64 - n.unsigned_abs().leading_zeros() as u64,
            mc_api::View::BigInt { mag, .. } => 64 * mag.len() as u64,
            _ => 64,
        };
        match h.view(u) {
            mc_api::View::Rational { num, den } => one(num) + one(den),
            _ => one(u),
        }
    };
    let mut prod = b.int(1);
    let mut rest = Vec::new();
    let mut found = 0;
    let mut single_multiple = false;
    for u in b.terms(t) {
        let (k, l) = match b.call_of(u) {
            Some((m, [k, l])) if m == b.hd.mul => match h.view(*k) {
                mc_api::View::Int(k) if k > 0 => (k, *l),
                _ => (0, u),
            },
            _ => (1, u),
        };
        match b.unary(l, b.hd.log) {
            Some(q) if k > 0 && positive_rational(q) && (k as u64).saturating_mul(bits(q)) <= LOG_BITS => {
                prod = b.mul(&[prod, b.powi(q, k)]);
                found += 1;
                single_multiple = k > 1;
            }
            _ => rest.push(u),
        }
    }
    if found == 0 || (found == 1 && !single_multiple) || bits(prod) > LOG_BITS {
        return Ok(t);
    }
    rest.push(b.call(b.hd.log, &[prod]));
    cx.eval(b.add(&rest))
}

/// `name(t, options(time_budget: MOVE_SECONDS))`: the polynomial move, `t` itself past the budget.
fn bounded<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, name: &str, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let rule = h.call(h.sym(mc_api::names::RULE), &[h.sym_term(h.sym("time_budget")), h.real(MOVE_SECONDS)]);
    let opts = h.call(h.sym("options"), &[rule]);
    cx.eval(b.call_named(name, &[t, opts]))
}

/// Is `t` worth searching: a compound term that is not a number?
fn searchable<'h>(b: &B<'h>, t: T<'h>) -> bool {
    b.call_of(t).is_some() && !b.is_number(t)
}

/// The cheapest form of `t` the search finds (its arguments already simplified), `depth` moves deep at most.
///
/// A form reached by a move of round 19 ([`Move::checked`]) is spot-checked against `t` before it is returned
/// ([`crate::verify`]); a refuted one gives way to the cheapest form reached by the other moves alone.
fn search<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, k: &mut Ctx<'_, '_, '_, 'h>, depth: usize, t: T<'h>) -> Res<T<'h>> {
    if !searchable(b, t) {
        return Ok(t);
    }
    let s0 = k.cost.score(cx, t)?;
    // the best form, whether a checked move led to it, and the best form without one
    let mut best = (s0, t, false);
    let mut plain = (s0, t);
    let cap = (k.cost.size(t) * GROWTH).max(SMALL);
    let deadline = (std::time::Instant::now() + std::time::Duration::from_secs_f64(SEARCH_SECONDS)).min(k.cfg.deadline);
    let mut seen: FxHashSet<T<'h>> = FxHashSet::default();
    seen.insert(t);
    let mut level = vec![(t, false)];
    let mut moves = k.cfg.moves();
    if k.has_facts {
        moves.push(Move::Refine);
    }
    let full = k.cfg.full;
    let (cfg, has_facts, measure) = (k.cfg, k.has_facts, k.cost.measure());
    // (a small term: its first level of moves runs whatever the deadline)
    let small = k.cost.size(t) <= 32;
    'search: for round in 0..depth {
        let mut next: Vec<(Score, T<'h>, bool)> = Vec::new();
        // every move from every form of the level, in parallel (roadmap item 16A: the results in this order, so the
        // search is the same for any number of threads); a move not started before the deadline ends the search
        let pairs: Vec<(T<'h>, bool, Move)> = level.iter().flat_map(|&(u, f)| moves.iter().map(move |&m| (u, f, m))).collect();
        let found = cx.par_until(pairs.len(), &|r: &Option<T<'h>>| r.is_none(), &|cx, i| {
            // the time budget of the search: the cheapest form so far (gap G73); the moves from a small term
            // itself always run, so a very short limit still gives the identities one move away, whatever part of
            // the limit the preparation took (a race with the clock before)
            if (round > 0 || !small) && std::time::Instant::now() >= deadline {
                return Ok(None);
            }
            let (u, _, m) = pairs[i];
            task_step(cx, cfg, has_facts, measure, m, u).map(Some)
        })?;
        for (i, r) in found.into_iter().enumerate() {
            let (_, flagged, m) = pairs[i];
            let Some(r) = r else { break 'search };
            // a move that fails (a limit inside a kernel, a failure term) is not a form
            if !seen.insert(r) || h_is_failure(b, r) {
                continue;
            }
            let c = k.cost.score(cx, r)?;
            let f = flagged || m.checked();
            let tie = best.1 == t && m.preferred_on_tie(full) && c.order(&best.0) == std::cmp::Ordering::Equal;
            if c.simpler_than(&best.0) || tie {
                best = (c, r, f);
            }
            if !f && c.simpler_than(&plain.0) {
                plain = (c, r);
            }
            // the size budget: a blown-up form is not explored further
            if k.cost.size(r) <= cap {
                next.push((c, r, f));
            }
        }
        if next.is_empty() {
            break;
        }
        // stable: equal costs keep the order found
        next.sort_by(|a, b| a.0.order(&b.0));
        next.truncate(k.cfg.beam());
        level = next.into_iter().map(|(_, r, f)| (r, f)).collect();
    }
    if best.2 && crate::verify::refuted(cx, b, &k.cfg.facts, t, best.1)? {
        return Ok(plain.1);
    }
    Ok(best.1)
}

/// `t` with `abs(u)` as `u` or `-u` and `sign(u)` as `1` or `-1` where the facts decide the sign of `u`, the
/// arguments themselves left as they are (decision s-cost.5).
fn decide_signs<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let (abs, sign) = (h.sym("abs"), h.sym("sign"));
    if h.mask(t) & (h.mask(h.sym_term(abs)) | h.mask(h.sym_term(sign))) == 0 {
        return Ok(t);
    }
    let r = rewrite(cx, b, t, &mut |cx, b, node| {
        let Some((s, [u])) = b.call_of(node) else { return Ok(None) };
        if s != abs && s != sign {
            return Ok(None);
        }
        let i = f.info(cx, *u)?;
        Ok(if f.positive(&i) {
            Some(if s == abs { *u } else { b.int(1) })
        } else if f.negative(&i) {
            Some(if s == abs { b.neg(*u) } else { b.int(-1) })
        } else if s == abs && f.nonneg(&i) {
            Some(*u)
        } else {
            None
        })
    })?;
    if r == t {
        Ok(t)
    } else {
        cx.eval(r)
    }
}

/// Is `t` a failure term?
fn h_is_failure<'h>(b: &B<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    matches!(b.call_of(t), Some((s, _)) if s == h.sym(mc_api::names::FAILURE))
}

/// `simplify(e)` (and `full_simplify(e)`, by `cfg`). With `facts` (inside `assuming`, decision A.6) the search also
/// refines, and the input and the result are refined; without, the moves are only the equivalences valid for
/// every value.
pub fn simplify<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &B<'h>,
    facts: Option<&mut Facts<'_, 'h>>,
    cost: &mut Cost<'h>,
    cfg: &Config<'h>,
    t: T<'h>,
) -> Res<T<'h>> {
    let r = match facts {
        Some(f) => run(cx, b, f, true, cost, cfg, t)?,
        None => {
            let mut own = Facts::new(cx, b, &cfg.syms, &[])?;
            run(cx, b, &mut own, false, cost, cfg, t)?
        }
    };
    let r = golden_ratio_number(cx, b, cost, cfg, r)?;
    crate::boolean::improve(cx, b, cost, r)
}

/// Does `t` hold the symbol `golden_ratio`?
fn has_golden_ratio<'h>(b: &B<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) => {
                if h.sym_name(s) == "golden_ratio" {
                    return true;
                }
            }
            View::App { args, .. } => stack.extend_from_slice(args),
            _ => {}
        }
    }
    false
}

/// A result that still holds `golden_ratio` is a number when written with its radical `(1 + sqrt(5))/2` and
/// simplified, if that gives an integer or a rational (`1 + 1/golden_ratio - golden_ratio` is 0: the constant is the
/// root of `x^2 - x - 1`, so its powers and its reciprocal reduce to `a golden_ratio + b`). Otherwise `r` stays.
fn golden_ratio_number<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, cost: &mut Cost<'h>, cfg: &Config<'h>, r: T<'h>) -> Res<T<'h>> {
    if !has_golden_ratio(b, r) || !is_free_of_symbols_but_constants(b, r) {
        return Ok(r);
    }
    let h = b.heap();
    let radical = b.div(b.add(&[b.int(1), b.call_named("sqrt", &[b.int(5)])]), b.int(2));
    let rule = h.call(h.sym(mc_api::names::RULE), &[h.sym_term(h.sym("golden_ratio")), radical]);
    let s = cx.eval(b.call_named("rewrite", &[r, rule]))?;
    let mut own = Facts::new(cx, b, &cfg.syms, &[])?;
    let s = run(cx, b, &mut own, false, cost, cfg, s)?;
    Ok(if matches!(h.view(s), View::Int(_) | View::BigInt { .. } | View::Rational { .. }) { s } else { r })
}

/// Does `t` hold `sin`, `cos` or `tan` of a sum with a term that is a multiple of `pi`?
fn has_pi_shift<'h>(b: &B<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let pi = h.sym_term(h.sym("pi"));
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        let Some((s, args)) = b.call_of(u) else { continue };
        if (s == b.hd.sin || s == b.hd.cos || s == b.hd.tan) && args.len() == 1 {
            let ts = b.terms(args[0]);
            if ts.len() > 1 && ts.iter().any(|&q| q == pi || b.call_of(q).is_some_and(|(m, fs)| m == b.hd.mul && fs.contains(&pi))) {
                return true;
            }
        }
        stack.extend_from_slice(args);
    }
    false
}

fn run<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &B<'h>,
    fx: &mut Facts<'_, 'h>,
    has_facts: bool,
    cost: &mut Cost<'h>,
    cfg: &Config<'h>,
    t: T<'h>,
) -> Res<T<'h>> {
    // full_simplify: a term that function_expand brings to 0 (or an equation it brings to true) is that, before any
    // search (beta(a, b) - beta(b, a): the search over the gamma forms of the two terms is long)
    if cfg.full && !has_facts {
        let fe = cx.eval(b.call_named("function_expand", &[t]))?;
        if fe == b.int(0) || fe == b.heap().sym_term(b.heap().sym(mc_api::names::TRUE)) {
            return Ok(fe);
        }
    }
    let mut k = Ctx { fx, has_facts, cfg, cost };
    let t0 = prepare(cx, b, &mut k, t)?;
    if !has_facts {
        return inside_out(cx, b, &mut k, t0);
    }
    // inside `assuming` (decisions A.6, s-cost.5): the search starts from the refined input and the result is
    // refined again; the refined result is the answer unless the result before refining, the input, or the input
    // with only its signs decided (`abs`, `sign` of arguments of known sign) is strictly simpler. So
    // `simplify(abs(x), x < 0)` keeps `abs(x)` (2 against 4 for `-x`), `abs((q p^2)^(1/4))` for q > 0 is
    // `(q p^2)^(1/4)`, and a refinement of equal size is taken (`log(x^p)` as `p log(x)` for x > 0, p real)
    let signs = decide_signs(cx, b, k.fx, t0)?;
    let t1 = refine(cx, b, k.fx, t0)?;
    let r = inside_out(cx, b, &mut k, t1)?;
    let r2 = refine(cx, b, k.fx, r)?;
    let mut best = (k.cost.score(cx, r2)?, r2);
    // a trigonometric function of a sum with a multiple of pi is a period shift that the facts removed
    // (`sin(x + n pi)` for an integer `n`): the shifted form is not a candidate against its reduction
    let shifted = has_pi_shift(b, t0) && !has_pi_shift(b, r2);
    for c in [r, t0, signs] {
        if shifted && has_pi_shift(b, c) {
            continue;
        }
        let s = k.cost.score(cx, c)?;
        if s.simpler_than(&best.0) {
            best = (s, c);
        }
    }
    Ok(best.1)
}

/// Is `l == r` proved by the search bringing `l - r` (term by term) to 0? Sides that are lists, strings, booleans
/// or numbers are left to the evaluator.
fn equation_holds<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, k: &mut Ctx<'_, '_, '_, 'h>, depth: usize, l: T<'h>, r: T<'h>) -> Res<bool> {
    let h = b.heap();
    let plain = |t: T<'h>| match b.call_of(t) {
        Some((s, _)) => s != b.hd.list && !matches!(h.sym_name(s).as_str(), "and" | "or" | "not" | "rule" | "equal"),
        None => !matches!(h.view(t), mc_api::View::Str(_)) && b.sym_of(t).is_none_or(|s| !matches!(h.sym_name(s).as_str(), "true" | "false")),
    };
    if l == r || !plain(l) || !plain(r) || (b.is_number(l) && b.is_number(r)) {
        return Ok(false);
    }
    let mut ts = b.terms(l);
    ts.extend(b.terms(r).into_iter().map(|u| b.neg(u)));
    let d = cx.eval(b.add(&ts))?;
    if d == b.int(0) {
        return Ok(true);
    }
    let d = search(cx, b, k, depth, d)?;
    Ok(d == b.int(0))
}

/// Largest subexpression (leaves) `full_simplify` searches on its own before the whole term.
pub const FULL_PART: u64 = 400;

/// The search on the arguments of non-arithmetic functions first, then on the whole term. In `full_simplify` every
/// sum, product and power below the top is searched too, bottom up (two moves deep), before the term it is in.
fn inside_out<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, k: &mut Ctx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    // inside out: every argument of a function other than the arithmetic (and every element of a list) is
    // simplified before the call it is in; the arithmetic is searched as a whole
    let arith = [b.hd.add, b.hd.mul, b.hd.pow, b.hd.complex];
    let depth = k.cfg.depth();
    let full = k.cfg.full;
    let equal = b.heap().sym("equal");
    let inner = rewrite(cx, b, t, &mut |cx, b, node| {
        let Some((s, args)) = b.call_of(node) else { return Ok(None) };
        if s == b.hd.list {
            return Ok(None);
        }
        if arith.contains(&s) {
            if !full || node == t || k.cost.size(node) > FULL_PART || std::time::Instant::now() >= k.cfg.deadline {
                return Ok(None);
            }
            let r = search(cx, b, k, 2, node)?;
            return Ok((r != node).then_some(r));
        }
        let mut changed = false;
        let mut out = Vec::with_capacity(args.len());
        for &a in args {
            let r = if !searchable(b, a) { a } else { search(cx, b, k, depth, a)? };
            changed |= r != a;
            out.push(r);
        }
        // an equation whose difference of sides simplifies to 0 is true (round 19: `simplify(eqs /. sol)`)
        if let (true, [l, r]) = (s == equal, out.as_slice()) {
            if equation_holds(cx, b, k, depth, *l, *r)? {
                return Ok(Some(cx.bool(true)));
            }
        }
        Ok(changed.then(|| b.call(s, &out)))
    })?;
    let inner = cx.eval(inner)?;
    if let Some((s, items)) = b.call_of(inner) {
        if s == b.hd.list {
            let mut out = Vec::with_capacity(items.len());
            for &a in items {
                out.push(search(cx, b, k, depth, a)?);
            }
            return Ok(b.call(s, &out));
        }
    }
    search(cx, b, k, depth, inner)
}

/// The functions with mirror symmetry, `f(conjugate(z)) = conjugate(f(z))` (decision 2B.5: the identity is used
/// also for the functions with a branch cut on the negative reals).
const MIRROR: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "sinh", "cosh", "tanh", "coth", "sech", "csch", "exp", "gamma",
    "factorial", "erf", "erfc", "erfi", "exp_integral_e", "exp_integral_ei", "binomial", "beta",
];

/// Rewritings applied once before the search (decision 2B.6): `conjugate(f(z))` as `f(conjugate(z))` for the
/// functions of [`MIRROR`]; `degree` as `pi/180`; `and`/`or` with repeated arguments removed and the rest sorted;
/// `floor(n + x)`, `ceiling(n + x)` as `n + floor(x)`, `n + ceiling(x)` for integers n; `sqrt(a + b sqrt(c))`
/// denested (rationals a > 0, b, c when a^2 - b^2 c is the square of a rational).
fn prepare<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, k: &mut Ctx<'_, '_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let cost = &mut *k.cost;
    let h = b.heap();
    let degree = h.sym("degree");
    let mentions_degree = h.mask(t) & h.mask(h.sym_term(degree)) != 0;
    let t = if mentions_degree {
        let to = b.div(h.sym_term(h.sym("pi")), b.int(180));
        let rule = b.call(h.sym(mc_api::names::RULE), &[h.sym_term(degree), to]);
        cx.eval(b.call_named("rewrite", &[t, rule]))?
    } else {
        t
    };
    let r = rewrite(cx, b, t, &mut |cx, b, node| {
        let h = b.heap();
        let Some((s, args)) = b.call_of(node) else { return Ok(None) };
        let name = h.sym_name(s);
        match (name.as_str(), args) {
            ("conjugate", [u]) => {
                if let Some((g, inner)) = b.call_of(*u) {
                    if MIRROR.contains(&h.sym_name(g).as_str()) {
                        let conj: Vec<T<'h>> = inner.iter().map(|&a| b.call(s, &[a])).collect();
                        return Ok(Some(b.call(g, &conj)));
                    }
                }
                Ok(None)
            }
            ("and" | "or", _) if args.len() > 1 => {
                let mut v = args.to_vec();
                cx.sort(&mut v);
                v.dedup();
                // bounds on one expression: `x > 1 and x > 2` is `x > 2`, `x > 1 or x > 2` is `x > 1`
                let v = merge_bounds(b, &v, name == "and");
                if v.len() == 1 {
                    return Ok(Some(v[0]));
                }
                Ok((v.as_slice() != args).then(|| b.call(s, &v)))
            }
            ("add", _) if args.len() > 1 => {
                // exp(u) terms whose exponents agree after expansion (`exp((p^2 + s^2)/4)` and `exp(p^2/4 +
                // s^2/4)`) are written with one exponent so that they combine
                let mut found: Vec<(T<'h>, T<'h>, T<'h>)> = vec![]; // (exp term, original arg, expanded arg)
                for &t in args {
                    let factors = b.call_of(t).filter(|&(m, _)| m == b.hd.mul).map_or_else(|| vec![t], |(_, fs)| fs.to_vec());
                    for f in factors {
                        if let Some((g, [base, u])) = b.call_of(f) {
                            if g == b.hd.pow && b.sym_of(*base) == Some(h.sym("e")) && b.call_of(*u).is_some() {
                                let e = cx.eval(b.call_named("expand", &[*u]))?;
                                found.push((f, *u, e));
                            }
                        }
                    }
                }
                let mut rules = vec![];
                for (i, &(f, u, e)) in found.iter().enumerate() {
                    if e != u && found.iter().enumerate().any(|(j, &(_, u2, e2))| j != i && e2 == e && u2 != u || j != i && u2 == e && e2 == e) {
                        rules.push((f, b.pow(h.sym_term(h.sym("e")), e)));
                    }
                }
                if rules.is_empty() {
                    return Ok(None);
                }
                let mut out = args.to_vec();
                for (f, new) in rules {
                    out = out
                        .into_iter()
                        .map(|t| {
                            if t == f {
                                new
                            } else if let Some((m, fs)) = b.call_of(t).filter(|&(m, _)| m == b.hd.mul) {
                                b.call(m, &fs.iter().map(|&x| if x == f { new } else { x }).collect::<Vec<_>>())
                            } else {
                                t
                            }
                        })
                        .collect();
                }
                Ok(Some(b.add(&out)))
            }
            ("less" | "greater" | "less_equal" | "greater_equal" | "equal" | "unequal", [l, r]) => {
                // a difference that expands to a number decides the relation (`x^2 > x^2 + 1` is false)
                if b.is_number(*l) && b.is_number(*r) {
                    return Ok(None);
                }
                let d = cx.eval(b.call_named("expand", &[b.sub(*l, *r)]))?;
                let Some(o) = b.ar.compare(d, b.int(0)) else { return Ok(None) };
                use mc_arith::number::Cmp3;
                let v = match (name.as_str(), o) {
                    ("less", c) => c == Cmp3::Less,
                    ("greater", c) => c == Cmp3::Greater,
                    ("less_equal", c) => c != Cmp3::Greater,
                    ("greater_equal", c) => c != Cmp3::Less,
                    ("equal", c) => c == Cmp3::Equal,
                    (_, c) => c != Cmp3::Equal,
                };
                Ok(Some(cx.bool(v)))
            }
            ("floor" | "ceiling", [u]) => {
                let (ints, rest): (Vec<T<'h>>, Vec<T<'h>>) =
                    b.terms(*u).into_iter().partition(|&x| matches!(h.view(x), mc_api::View::Int(_) | mc_api::View::BigInt { .. }));
                if ints.is_empty() || rest.is_empty() {
                    return Ok(None);
                }
                Ok(Some(b.add(&[b.add(&ints), b.call(s, &[b.add(&rest)])])))
            }
            _ if s == b.hd.pow => {
                // (x^a)^c = x^(a c) for a positive real constant x (pi, e, 2 sqrt(3)) and a real number a
                if let [inner, c] = args {
                    if let Some((g, [x, a])) = b.call_of(*inner) {
                        if g == b.hd.pow && b.is_number(*a) && !b.is_number(*x) && is_free_of_symbols_but_constants(b, *x) {
                            let pos = cx.eval(b.call_named("is_positive", &[*x]))?;
                            if cx.is_true(pos) {
                                return Ok(Some(b.pow(*x, b.mul(&[*a, *c]))));
                            }
                        }
                    }
                }
                if let Some(r) = denest(cx, b, args)? {
                    return Ok(Some(r));
                }
                if let Some(r) = denest_symbolic(cx, b, k.fx, args)? {
                    return Ok(Some(r));
                }
                // a compound exponent expanded when that is smaller (G51: mp^(-2 (4 - k) + 6 - k) is mp^(-2 + k))
                let [base, ex] = args else { return Ok(None) };
                if b.call_of(*ex).is_some() && !b.is_number(*ex) {
                    let e2 = apply(cx, b, "expand", *ex)?;
                    if cost.size(e2) < cost.size(*ex) {
                        return Ok(Some(b.pow(*base, e2)));
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    })?;
    let r = if r == t { t } else { cx.eval(r)? };
    // the identities that remove a special function (marked ALWAYS), whatever the cost, when the spot check
    // does not refute them (round 19: `product_log(x e^x)` is `x` for `x >= -1`)
    let a = identities(cx, b, k.fx, r, true)?;
    if a != r && !crate::verify::refuted(cx, b, &k.cfg.facts, r, a)? {
        return Ok(a);
    }
    Ok(r)
}

/// Is `t` built from numbers and the constants `pi`, `e`, `euler_gamma`, ... only (no free symbols)?
fn is_free_of_symbols_but_constants<'h>(b: &B<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            mc_api::View::Sym(sy) => {
                if !matches!(h.sym_name(sy).as_str(), "pi" | "e" | "euler_gamma" | "catalan" | "golden_ratio") {
                    return false;
                }
            }
            mc_api::View::App { head, args } => {
                if b.sym_of(head).is_none() {
                    return false;
                }
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    true
}

/// The relations `e > a`, `e >= a`, `e < a`, `e <= a` (`a` a real number) of `parts` on one expression `e`: of
/// several lower (upper) bounds on `e` joined by `and` the strongest stays, joined by `or` the weakest; the other
/// parts keep their places.
fn merge_bounds<'h>(b: &B<'h>, parts: &[T<'h>], and: bool) -> Vec<T<'h>> {
    use mc_arith::number::Cmp3;
    let h = b.heap();
    // (index, expression, number, lower bound?, strict?)
    let mut bounds: Vec<(usize, T<'h>, T<'h>, bool, bool)> = vec![];
    for (i, &p) in parts.iter().enumerate() {
        let Some((s, [l, r])) = b.call_of(p) else { continue };
        let (lower, strict) = match h.sym_name(s).as_str() {
            "greater" => (true, true),
            "greater_equal" => (true, false),
            "less" => (false, true),
            "less_equal" => (false, false),
            _ => continue,
        };
        if b.is_number(*r) && !b.is_number(*l) {
            bounds.push((i, *l, *r, lower, strict));
        }
    }
    let mut drop = vec![false; parts.len()];
    for x in 0..bounds.len() {
        for y in 0..bounds.len() {
            let (i, e, a, lo, st) = bounds[x];
            let (j, e2, a2, lo2, st2) = bounds[y];
            if x == y || e != e2 || lo != lo2 || drop[i] || drop[j] {
                continue;
            }
            // is bound x implied by (and: implies) bound y? keep the stronger one for `and`, weaker for `or`
            let Some(c) = b.ar.compare(a, a2) else { continue };
            // stronger(x over y): lower bounds: larger number (or equal and strict); upper: smaller
            let x_stronger = match (lo, c) {
                (true, Cmp3::Greater) | (false, Cmp3::Less) => true,
                (_, Cmp3::Equal) => st && !st2,
                _ => false,
            };
            let same = c == Cmp3::Equal && st == st2;
            if (x_stronger && !and) || (!x_stronger && and && !same) {
                drop[i] = true;
            } else if same && x < y {
                drop[j] = true;
            }
        }
    }
    parts.iter().enumerate().filter(|(i, _)| !drop[*i]).map(|(_, &p)| p).collect()
}

/// `sqrt(a + b sqrt(c))` as `sqrt((a + d)/2) + sign(b) sqrt((a - d)/2)`, d = sqrt(a^2 - b^2 c) rational.
fn denest<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [base, ex] = args else { return Ok(None) };
    let half = b.rat(1, 2);
    if *ex != half {
        return Ok(None);
    }
    let rational = |u: T<'h>| matches!(h.view(u), mc_api::View::Int(_) | mc_api::View::BigInt { .. } | mc_api::View::Rational { .. });
    let terms = b.terms(*base);
    let [a, rest] = terms.as_slice() else { return Ok(None) };
    if !rational(*a) {
        return Ok(None);
    }
    // rest = coef * c^(1/2)
    let (coef, rad) = match b.call_of(*rest) {
        Some((m, [k, r])) if m == b.hd.mul && rational(*k) => (*k, *r),
        _ => (b.int(1), *rest),
    };
    let c = match b.call_of(rad) {
        Some((p, [c, e])) if p == b.hd.pow && *e == half && rational(*c) => *c,
        _ => return Ok(None),
    };
    let pos = |u: T<'h>| b.ar.compare(u, b.int(0)).is_some_and(|o| o == mc_arith::number::Cmp3::Greater);
    if !pos(*a) {
        return Ok(None);
    }
    let disc = b.sub(b.mul(&[*a, *a]), b.mul(&[coef, coef, c]));
    if !rational(disc) || b.ar.compare(disc, b.int(0)).is_some_and(|o| o == mc_arith::number::Cmp3::Less) {
        return Ok(None);
    }
    let d = cx.eval(b.pow(disc, half))?;
    if !rational(d) {
        return Ok(None);
    }
    let p = b.pow(b.mul(&[half, b.add(&[*a, d])]), half);
    let q = b.pow(b.mul(&[half, b.sub(*a, d)]), half);
    let q = if pos(coef) { q } else { b.neg(q) };
    Ok(Some(b.add(&[p, q])))
}

/// `sqrt(a + b sqrt(c))` for symbolic `a` and `c` and a positive rational `b`, as `sqrt((a + d)/2) + sqrt((a - d)/2)`
/// with `d^2 = a^2 - b^2 c` a perfect square (found by factoring; Borodin, Fagin, Hopcroft and Tompa 1985 for the
/// radical denesting identity). `(a + d)/2` and `(a - d)/2` have the sum `a` and the product `b^2 c / 4`, so both are
/// nonnegative when `a > 0` and `c >= 0` (the facts decide these), and the sum is symmetric in the sign of `d`.
fn denest_symbolic<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &B<'h>,
    fx: &mut Facts<'_, 'h>,
    args: &[T<'h>],
) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [base, ex] = args else { return Ok(None) };
    let half = b.rat(1, 2);
    if *ex != half || fx.is_empty() {
        return Ok(None);
    }
    let terms = b.terms(*base);
    let [t0, t1] = terms.as_slice() else { return Ok(None) };
    let rational = |u: T<'h>| matches!(h.view(u), mc_api::View::Int(_) | mc_api::View::BigInt { .. } | mc_api::View::Rational { .. });
    for (a, rest) in [(*t0, *t1), (*t1, *t0)] {
        let (coef, rad) = match b.call_of(rest) {
            Some((m, [k, r])) if m == b.hd.mul && rational(*k) => (*k, *r),
            _ => (b.int(1), rest),
        };
        let c = match b.call_of(rad) {
            Some((p, [c, e])) if p == b.hd.pow && *e == half && !b.is_number(*c) => *c,
            _ => continue,
        };
        if b.is_number(a) || b.ar.compare(coef, b.int(0)).is_none_or(|o| o != mc_arith::number::Cmp3::Greater) {
            continue;
        }
        let (ia, ic) = (fx.info(cx, a)?, fx.info(cx, c)?);
        if !fx.positive(&ia) || !fx.nonneg(&ic) {
            continue;
        }
        let disc = cx.eval(b.call_named("expand", &[b.sub(b.mul(&[a, a]), b.mul(&[coef, coef, c]))]))?;
        let fac = cx.eval(b.call_named("factor", &[disc]))?;
        // d = the square root of the factored form, when every factor is a square
        let mut d: Vec<T<'h>> = vec![];
        let mut ok = true;
        for f in b.factors(fac) {
            if rational(f) {
                let r = cx.eval(b.pow(f, half))?;
                if !rational(r) {
                    ok = false;
                    break;
                }
                d.push(r);
            } else {
                let (u, k) = b.int_power(f);
                if k % 2 != 0 || k <= 0 {
                    ok = false;
                    break;
                }
                d.push(b.powi(u, k / 2));
            }
        }
        if !ok {
            continue;
        }
        let d = cx.eval(b.mul(&d))?;
        let p = b.pow(b.mul(&[half, b.add(&[a, d])]), half);
        let q = b.pow(b.mul(&[half, b.sub(a, d)]), half);
        return Ok(Some(cx.eval(b.add(&[p, q]))?));
    }
    Ok(None)
}
