//! The natives: argument checks (pure), then the effect, then the draws under one lock of the session state.
use crate::{Generator, Rng};
use mc_api::{names, Store, View, T};
use mc_arith::num::{Int, Nat};
use mc_arith::{Arith, Num, Real};
use mc_core::Heap;
use mc_eval::{Native, NativeCx, Reply, Res};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Fun {
    Seed,
    RandomSeed,
    Real,
    Integer,
    Normal,
    Choice,
    Sample,
}

pub(crate) const TABLE: &[(&str, Fun)] = &[
    ("seed", Fun::Seed),
    ("random_seed", Fun::RandomSeed),
    ("random_real", Fun::Real),
    ("random_integer", Fun::Integer),
    ("random_normal", Fun::Normal),
    ("random_choice", Fun::Choice),
    ("random_sample", Fun::Sample),
];

pub(crate) struct RandomNative {
    pub(crate) f: Fun,
    pub(crate) name: &'static str,
    pub(crate) rng: Rng,
}

/// The most elements one call makes (2^32): beyond, `failure(argument, ...)` instead of an allocation failure.
const MAX_ELEMENTS: u64 = 1 << 32;

/// What to draw, decided from the arguments before the effect.
enum Job<'h> {
    Seed(Option<Int>),
    RandomSeed,
    /// uniform reals a + (b - a) u
    Real(f64, f64),
    /// integers in [a, b]
    Integer(Int, Int),
    /// mu + sigma z
    Normal(f64, f64),
    Choice(&'h [T<'h>]),
    Sample(&'h [T<'h>], usize),
}

type Checked<'h, X> = Result<X, &'static str>;

/// `failure(argument, f, message)`.
fn bad<'h>(h: &'h Heap, f: &str, message: &str) -> Reply<'h> {
    let tags = [h.sym_term(h.sym("argument")), h.sym_term(h.sym(f)), h.string(message)];
    Reply::Done(h.call(h.sym(names::FAILURE), &tags))
}

fn integer(h: &Heap, t: T<'_>) -> Option<Int> {
    match h.view(t) {
        View::Int(v) => Some(Int::from_i64(v)),
        View::BigInt { neg, mag } => Some(Int::new(neg, Nat::from_limbs(mag.to_vec()))),
        _ => None,
    }
}

/// A finite real: a number, or a closed expression made numeric by `n` (`pi`, `sqrt(2)`).
fn real<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let num = match ar.num(t) {
        Some(v) => Some(v),
        None => {
            let v = cx.eval(h.call(h.sym("n"), &[t]))?;
            ar.num(v)
        }
    };
    Ok(match num {
        Some(Num::Real(r)) => Some(match r {
            Real::Machine(x) => x,
            r => r.to_f64(),
        })
        .filter(|x| x.is_finite()),
        _ => None,
    })
}

/// The shape of a count argument: `n` (a list of n) or `[n1, ..., nk]` (nested lists), each a natural number.
fn shape<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Checked<'h, Vec<usize>> {
    let h = cx.heap();
    let one = |t: T<'h>| match h.view(t) {
        View::Int(v) if v >= 0 && (v as u64) <= MAX_ELEMENTS => Ok(v as usize),
        _ => Err("the count must be a natural number, at most 2^32, or a list of them"),
    };
    let dims: Vec<usize> = match cx.as_list(t) {
        Some(items) if !items.is_empty() => items.iter().map(|&x| one(x)).collect::<Result<_, _>>()?,
        Some(_) => return Err("the list of dimensions is empty"),
        None => vec![one(t)?],
    };
    let mut total = 1u64;
    for &d in &dims {
        total = total.saturating_mul(d as u64);
    }
    if total > MAX_ELEMENTS {
        return Err("more than 2^32 elements");
    }
    Ok(dims)
}

/// The range of `random_real`: `b` ([0, b]) or `[a, b]`, finite reals with a <= b.
fn real_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Checked<'h, (f64, f64)>> {
    const MSG: &str = "the range must be a real b >= 0 or a list [a, b] of reals with a <= b";
    let (a, b) = match cx.as_list(t) {
        Some(&[a, b]) => match (real(cx, a)?, real(cx, b)?) {
            (Some(a), Some(b)) => (a, b),
            _ => return Ok(Err(MSG)),
        },
        Some(_) => return Ok(Err(MSG)),
        None => match real(cx, t)? {
            Some(b) => (0.0, b),
            None => return Ok(Err(MSG)),
        },
    };
    Ok(if a <= b && (b - a).is_finite() { Ok((a, b)) } else { Err(MSG) })
}

/// The range of `random_integer`: `n` ([0, n], n >= 0) or `[a, b]`, integers with a <= b (decision RNG.8).
fn integer_range<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Checked<'h, (Int, Int)> {
    const MSG: &str = "the range must be an integer n >= 0 or a list [a, b] of integers with a <= b";
    let h = cx.heap();
    let (a, b) = match cx.as_list(t) {
        Some(&[a, b]) => match (integer(h, a), integer(h, b)) {
            (Some(a), Some(b)) => (a, b),
            _ => return Err(MSG),
        },
        Some(_) => return Err(MSG),
        None => (Int::from_i64(0), integer(h, t).ok_or(MSG)?),
    };
    if a.cmp_int(&b).is_le() {
        Ok((a, b))
    } else {
        Err(MSG)
    }
}

/// Nested lists of the shape `dims` from the flat elements, row-major.
fn nest<'h>(cx: &NativeCx<'_, '_, 'h>, flat: &[T<'h>], dims: &[usize]) -> T<'h> {
    match dims {
        [] => flat[0],
        [_] => cx.list(flat),
        [d, rest @ ..] => {
            let size: usize = rest.iter().product();
            let rows: Vec<T<'h>> = (0..*d).map(|i| nest(cx, &flat[i * size..(i + 1) * size], rest)).collect();
            cx.list(&rows)
        }
    }
}

impl RandomNative {
    /// The job and the shape of the result (`None`: one value), or a reply (decline or failure).
    #[allow(clippy::type_complexity)]
    fn job<'h>(
        &self,
        cx: &mut NativeCx<'_, '_, 'h>,
        args: &[T<'h>],
    ) -> Res<Result<(Job<'h>, Option<Vec<usize>>), Reply<'h>>> {
        let h = cx.heap();
        let name = self.name;
        let fail = |m: &str| Err(bad(h, name, m));
        let count = |cx: &NativeCx<'_, '_, 'h>, t: Option<&T<'h>>| -> Result<Option<Vec<usize>>, Reply<'h>> {
            match t {
                None => Ok(None),
                Some(&t) => shape(cx, t).map(Some).map_err(|m| bad(h, name, m)),
            }
        };
        Ok(match (self.f, args) {
            (Fun::Seed, []) => Ok((Job::Seed(None), None)),
            (Fun::Seed, [n]) => match integer(h, *n) {
                Some(n) => Ok((Job::Seed(Some(n)), None)),
                None => fail("the seed must be an integer"),
            },
            (Fun::RandomSeed, []) => Ok((Job::RandomSeed, None)),
            (Fun::Real, []) => Ok((Job::Real(0.0, 1.0), None)),
            (Fun::Real, [r, n @ ..]) if n.len() <= 1 => match real_range(cx, *r)? {
                Ok((a, b)) => count(cx, n.first()).map(|s| (Job::Real(a, b), s)),
                Err(m) => fail(m),
            },
            (Fun::Integer, [r, n @ ..]) if n.len() <= 1 => match integer_range(cx, *r) {
                Ok((a, b)) => count(cx, n.first()).map(|s| (Job::Integer(a, b), s)),
                Err(m) => fail(m),
            },
            (Fun::Normal, []) => Ok((Job::Normal(0.0, 1.0), None)),
            (Fun::Normal, [mu, sigma, n @ ..]) if n.len() <= 1 => match (real(cx, *mu)?, real(cx, *sigma)?) {
                (Some(mu), Some(sigma)) if sigma >= 0.0 => count(cx, n.first()).map(|s| (Job::Normal(mu, sigma), s)),
                _ => fail("the mean must be a real and the standard deviation a real >= 0"),
            },
            (Fun::Choice, [xs, n @ ..]) if n.len() <= 1 => match cx.as_list(*xs) {
                Some(items) if !items.is_empty() => count(cx, n.first()).map(|s| (Job::Choice(items), s)),
                _ => fail("the first argument must be a nonempty list"),
            },
            (Fun::Sample, [xs, n @ ..]) if n.len() <= 1 => match cx.as_list(*xs) {
                Some(items) => match n.first().map(|&n| h.view(n)) {
                    None => Ok((Job::Sample(items, items.len()), None)),
                    Some(View::Int(k)) if k >= 0 && k as u64 <= items.len() as u64 => {
                        Ok((Job::Sample(items, k as usize), None))
                    }
                    Some(_) => fail("the count must be a natural number at most the length of the list"),
                },
                None => fail("the first argument must be a list"),
            },
            _ => Err(Reply::Decline),
        })
    }
}

impl Native for RandomNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let (job, dims) = match self.job(cx, args)? {
            Ok(v) => v,
            Err(reply) => return Ok(reply),
        };
        // decision RNG.7: the draw is an effect, announced before the state is touched (refused on speculative
        // tasks and inside conditions, so draws happen on the frontier, in sequential order)
        cx.effect()?;
        let h = cx.heap();
        let ar = Arith::new(h);
        match job {
            Job::Seed(n) => {
                *self.rng.lock() = match n {
                    Some(n) => Generator::seeded(n),
                    None => Generator::from_os(),
                };
                return Ok(Reply::Done(cx.null()));
            }
            Job::RandomSeed => {
                let s = self.rng.lock().seed().clone();
                return Ok(Reply::Done(ar.integer(&s)));
            }
            Job::Sample(items, k) => {
                // partial Fisher–Yates over the positions: k draws
                let mut v: Vec<T<'h>> = items.to_vec();
                let mut g = self.rng.lock();
                let s = g.stream();
                for i in 0..k {
                    cx.tick(1)?;
                    let j = i + s.below((v.len() - i) as u64) as usize;
                    v.swap(i, j);
                }
                v.truncate(k);
                return Ok(Reply::Done(cx.list(&v)));
            }
            _ => {}
        }
        let n: usize = dims.as_ref().map_or(1, |d| d.iter().product());
        let mut flat: Vec<T<'h>> = Vec::with_capacity(n.min(1 << 20));
        {
            let mut g = self.rng.lock();
            let s = g.stream();
            match &job {
                Job::Real(a, b) => {
                    let w = b - a;
                    for _ in 0..n {
                        cx.tick(1)?;
                        flat.push(h.real(a + w * s.uniform()));
                    }
                }
                Job::Integer(a, b) => {
                    // machine ranges without the big-integer layer
                    if let (Some(a), Some(b)) = (a.to_i64(), b.to_i64()) {
                        let w = (b as i128 - a as i128) as u128;
                        for _ in 0..n {
                            cx.tick(1)?;
                            let r = if w == u64::MAX as u128 { s.next_u64() } else { s.below(w as u64 + 1) };
                            flat.push(ar.integer(&Int::from_i64(a).add(&Int::from_u64(r))));
                        }
                    } else {
                        for _ in 0..n {
                            cx.tick(1)?;
                            flat.push(ar.integer(&s.integer(a, b)));
                        }
                    }
                }
                Job::Normal(mu, sigma) => {
                    // pairs; an odd count drops the last pair's second value (as Stream::normals)
                    for i in 0..n.div_ceil(2) {
                        cx.tick(2)?;
                        let (x, y) = s.normal_pair();
                        flat.push(h.real(mu + sigma * x));
                        if 2 * i + 1 < n {
                            flat.push(h.real(mu + sigma * y));
                        }
                    }
                }
                Job::Choice(items) => {
                    for _ in 0..n {
                        cx.tick(1)?;
                        flat.push(items[s.below(items.len() as u64) as usize]);
                    }
                }
                Job::Seed(_) | Job::RandomSeed | Job::Sample(..) => unreachable!(),
            }
        }
        Ok(Reply::Done(match &dims {
            None => flat[0],
            // a shape with a zero dimension: nested empty lists down to it
            Some(d) if n == 0 => nest_empty(cx, d),
            Some(d) => nest(cx, &flat, d),
        }))
    }

    fn reads_term(&self) -> bool {
        false
    }
}

/// The nested lists of a shape with a zero dimension.
fn nest_empty<'h>(cx: &NativeCx<'_, '_, 'h>, dims: &[usize]) -> T<'h> {
    match dims {
        [] | [0, ..] | [_] => cx.list(&[]),
        [d, rest @ ..] => {
            let row = nest_empty(cx, rest);
            cx.list(&vec![row; *d])
        }
    }
}
