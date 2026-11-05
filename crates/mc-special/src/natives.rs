//! The natives: list threading, argument kinds, and the dispatch to the exact rules, the machine path and the
//! ball path.
//!
//! - Lists thread (as for the arithmetic functions): `gamma([1, 2, 3])` is `[1, 1, 2]`.
//! - A machine real among the arguments makes the call a machine computation (contagion); outside the real domain
//!   (K_nu at x < 0, a non-integer polygamma order) the call stays.
//! - Otherwise a ball among the arguments gives a ball at the largest ball precision (exact arguments stay exact
//!   for the ball path); no enclosure (a ball touching a pole) leaves the call unevaluated.
//! - All exact: the exact rules ([`crate::exact`]); a pole is `failure(pole, call)` (decision SF.1); no closed
//!   form leaves the call unevaluated, for `n` to evaluate.
//! - Complex numbers and other terms stay (except the symbolic rules: `polygamma(0, x) = digamma(x)` and the
//!   half-integer `bessel_k` and `bessel_i` closed forms).
use crate::exact::{Exact, Rules};
use crate::exact_more::More;
use crate::functions::Special;
use mc_api::{names, Store, View};
use mc_arith::num::{stopped, Rat, StopScope};
use mc_arith::{Arith, Num, Real};
use mc_core::{Heap, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_numeric::fun::Mach;
use mc_numeric::machine::C64;
use mc_numeric::numeval::{cball_term, mach_term};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Failure tags of this package.
pub mod failure_tags {
    /// `failure(pole, call)`: the function has a pole at the exact arguments.
    pub const POLE: &str = "pole";
}

struct Fun {
    /// the variants of one name (one per arity)
    fs: Vec<Special>,
    stop: Arc<AtomicBool>,
}

/// `hypergeometric_pfq(as, bs, z)`.
struct Pfq {
    stop: Arc<AtomicBool>,
}

/// `f([a, b], c)` is `[f(a, c), f(b, c)]`; unequal lengths give `failure(shape_mismatch)`.
fn thread<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Option<Res<Reply<'h>>> {
    let lists: Vec<Option<&[T<'h>]>> = args.iter().map(|&a| cx.as_list(a)).collect();
    let len = lists.iter().flatten().map(|l| l.len()).next()?;
    if lists.iter().flatten().any(|l| l.len() != len) {
        return Some(Ok(Reply::Done(cx.failure(&["shape_mismatch"]))));
    }
    if let Err(e) = cx.tick(len as u64) {
        return Some(Err(e));
    }
    let h = cx.heap();
    let View::App { head, .. } = h.view(t) else { return None };
    let items: Vec<T<'h>> = (0..len)
        .map(|i| {
            let xs: Vec<T<'h>> = args.iter().zip(&lists).map(|(&a, l)| l.map_or(a, |l| l[i])).collect();
            h.app(head, &xs)
        })
        .collect();
    Some(Ok(Reply::Again(cx.list(&items))))
}

fn pole<'h>(h: &'h Heap, t: T<'h>) -> T<'h> {
    let tag = h.sym_term(h.sym(failure_tags::POLE));
    h.call(h.sym(names::FAILURE), &[tag, t])
}

fn exact_reply<'h>(h: &'h Heap, t: T<'h>, e: Option<Exact<'h>>) -> Reply<'h> {
    match e {
        Some(Exact::Value(v)) => Reply::Again(v),
        Some(Exact::Pole) => Reply::Done(pole(h, t)),
        None => Reply::Decline,
    }
}

impl Native for Fun {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let Some(f) = self.fs.iter().copied().find(|f| f.arity() == args.len()) else {
            return Ok(Reply::Decline);
        };
        if let Some(r) = thread(cx, t, args) {
            return r;
        }
        let h = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let nums: Vec<Option<Num>> = args.iter().map(|&a| ar.num(a)).collect();
        let reals: Option<Vec<Real>> = nums
            .iter()
            .map(|n| match n {
                Some(Num::Real(r)) => Some(r.clone()),
                _ => None,
            })
            .collect();
        let rules = Rules::new(h, &ar);
        let more = More::new(h, &ar);
        let Some(reals) = reals else {
            // complex machine numbers: the complex machine algorithms
            if nums.iter().all(Option::is_some)
                && nums.iter().flatten().any(|n| matches!(n, Num::Complex(Real::Machine(_), _) | Num::Real(Real::Machine(_))))
            {
                let zs: Option<Vec<C64>> = nums.iter().flatten().map(machine_c64).collect();
                if let Some(z) = zs.and_then(|zs| f.machine_complex(&zs)) {
                    cx.tick(1)?;
                    return Ok(Reply::Done(mach_term(&ar, Mach::Complex(z))));
                }
                return Ok(Reply::Decline);
            }
            if !f.is_original() {
                return Ok(exact_reply(h, t, more.rule(f, args)));
            }
            // symbolic arguments: the rules that hold for any x
            return Ok(match f {
                Special::Polygamma if nums[0].as_ref().is_some_and(|n| n.is_exact_zero()) => {
                    Reply::Again(h.call(h.sym("digamma"), &[args[1]]))
                }
                Special::BesselK | Special::BesselI if nums[1].is_none() => {
                    match nums[0].as_ref().and_then(Num::as_rat) {
                        Some(nu) if f == Special::BesselK => exact_reply(h, t, rules.bessel_k(nu, args[1])),
                        Some(nu) => exact_reply(h, t, rules.bessel_i(nu, args[1])),
                        None => Reply::Decline,
                    }
                }
                _ => Reply::Decline,
            });
        };
        if reals.iter().any(|r| matches!(r, Real::Machine(_))) {
            let xs: Vec<f64> = reals.iter().map(Real::to_f64).collect();
            if !f.real_domain(&xs) {
                return Ok(Reply::Decline);
            }
            cx.tick(1)?;
            let v = f.machine_value(&xs);
            if !f.is_original() && matches!(v, Mach::Real(r) if r.is_nan()) && !xs.iter().any(|x| x.is_nan()) {
                // no real value (a complex-valued or undefined point): the call stays
                return Ok(Reply::Decline);
            }
            return Ok(Reply::Done(mach_term(&ar, v)));
        }
        if let Some(p) = reals.iter().filter_map(Real::prec).max() {
            cx.tick(1 + p / 256)?;
            let r = {
                let _scope = StopScope::enter(Some(&self.stop));
                f.ball_value(&reals, p)
            };
            if stopped() || cx.stopped() {
                cx.tick(0)?;
                let tag = h.sym_term(h.sym(names::failure::INTERRUPTED));
                return Ok(Reply::Done(h.call(h.sym(names::FAILURE), &[tag, t])));
            }
            return Ok(match r {
                Some(z) => Reply::Done(cball_term(&ar, z, p)),
                None => Reply::Decline,
            });
        }
        let q: Vec<&Rat> = reals
            .iter()
            .map(|r| match r {
                Real::Exact(q) => q,
                _ => unreachable!("only exact reals remain"),
            })
            .collect();
        let e = match f {
            Special::Gamma => rules.gamma(q[0]),
            Special::LogGamma => rules.log_gamma(q[0]),
            Special::Digamma => rules.digamma(q[0]),
            Special::Polygamma => match q[0].num.to_i64().filter(|_| q[0].is_int()) {
                Some(0) => return Ok(Reply::Again(h.call(h.sym("digamma"), &[args[1]]))),
                Some(n) => rules.polygamma(n, q[1]),
                None => None,
            },
            Special::Beta => rules.beta(q[0], q[1]),
            Special::Zeta => rules.zeta(q[0]),
            Special::BesselK => rules.bessel_k(q[0], args[1]),
            Special::BesselI => rules.bessel_i(q[0], args[1]),
            _ => more.rule(f, args),
        };
        Ok(exact_reply(h, t, e))
    }
}

/// A machine-precision complex value of a number (exact parts are rounded).
fn machine_c64(n: &Num) -> Option<C64> {
    Some(match n {
        Num::Real(r) => C64::new(r.to_f64(), 0.0),
        Num::Complex(a, b) => C64::new(a.to_f64(), b.to_f64()),
    })
}

impl Native for Pfq {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if args.len() != 3 {
            return Ok(Reply::Decline);
        }
        let (Some(a), Some(b)) = (cx.as_list(args[0]), cx.as_list(args[1])) else { return Ok(Reply::Decline) };
        if let Some(zs) = cx.as_list(args[2]) {
            // a list of z threads
            cx.tick(zs.len() as u64)?;
            let h = cx.heap();
            let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
            let items: Vec<T<'h>> = zs.iter().map(|&z| h.app(head, &[args[0], args[1], z])).collect();
            return Ok(Reply::Again(cx.list(&items)));
        }
        let h = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let z = ar.num(args[2]);
        if z.as_ref().is_some_and(Num::is_exact_zero) {
            return Ok(Reply::Again(ar.int(1)));
        }
        // a zero numerator parameter: 1
        if a.iter().any(|&x| ar.num(x).is_some_and(|v| v.is_exact_zero())) {
            return Ok(Reply::Again(ar.int(1)));
        }
        // dispatch to the named functions of fixed arity
        let named = match (a.len(), b.len()) {
            (0, 1) => Some(("hypergeometric_0f1", vec![b[0], args[2]])),
            (1, 1) => Some(("hypergeometric_1f1", vec![a[0], b[0], args[2]])),
            (2, 1) => Some(("hypergeometric_2f1", vec![a[0], a[1], b[0], args[2]])),
            _ => None,
        };
        if let Some((name, xs)) = named {
            return Ok(Reply::Again(h.call(h.sym(name), &xs)));
        }
        let all: Vec<T<'h>> = a.iter().chain(b.iter()).copied().chain(std::iter::once(args[2])).collect();
        let reals: Option<Vec<Real>> = all
            .iter()
            .map(|&x| match ar.num(x) {
                Some(Num::Real(r)) => Some(r),
                _ => None,
            })
            .collect();
        let Some(reals) = reals else { return Ok(Reply::Decline) };
        let (ra, rest) = reals.split_at(a.len());
        let (rb, rz) = rest.split_at(b.len());
        if reals.iter().any(|r| matches!(r, Real::Machine(_))) {
            let fa: Vec<f64> = ra.iter().map(Real::to_f64).collect();
            let fb: Vec<f64> = rb.iter().map(Real::to_f64).collect();
            let v = crate::machine::hyper::hyp_pfq(&fa, &fb, rz[0].to_f64());
            if v.is_nan() {
                return Ok(Reply::Decline);
            }
            cx.tick(1)?;
            return Ok(Reply::Done(mach_term(&ar, Mach::Real(v))));
        }
        let Some(p) = reals.iter().filter_map(Real::prec).max() else { return Ok(Reply::Decline) };
        cx.tick(1 + p / 256)?;
        let r = {
            let _scope = StopScope::enter(Some(&self.stop));
            crate::ball::hyper::hyp_pfq(ra, rb, &rz[0], p)
        };
        if stopped() || cx.stopped() {
            cx.tick(0)?;
            let tag = h.sym_term(h.sym(names::failure::INTERRUPTED));
            return Ok(Reply::Done(h.call(h.sym(names::FAILURE), &[tag, t])));
        }
        Ok(match r {
            Some(z) => Reply::Done(cball_term(&ar, z, p)),
            None => Reply::Decline,
        })
    }
}

/// Registers the natives (one per name) and the library functions of numerical evaluation (one per name and
/// arity).
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    let mut names: Vec<&str> = vec![];
    for f in &crate::functions::ALL {
        mc_numeric::library::register(f);
        if !names.contains(&f.name()) {
            names.push(f.name());
        }
    }
    for name in names {
        let fs: Vec<Special> = crate::functions::ALL.iter().copied().filter(|f| f.name() == name).collect();
        k.register(h.sym(name), Attributes::default(), Box::new(Fun { fs, stop: stop.clone() }));
    }
    k.register(h.sym(crate::functions::PFQ.0), Attributes::default(), Box::new(Pfq { stop: stop.clone() }));
}
