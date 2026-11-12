//! The equational facts of a context as a Groebner basis, and the normal form of a candidate against it (tier 2 of
//! the decision service: the equations of the assumptions are compiled once into a degree-reverse-lexicographic
//! basis that every candidate is reduced against).
//!
//! **Compiling** (`decide$basis(list(d1, ..., dk))`, an internal native): each difference `l - r` of a fact
//! `l == r` is read as a polynomial with rational coefficients in its kernels (`mc-poly`'s conversion: symbols,
//! calls such as `sin(x)`, non-integer powers and `b^-1` are variables), and the reduced Groebner basis in
//! degree-reverse-lexicographic order is computed by `mc-poly`'s Buchberger algorithm (public API only). The result
//! is a term, `list(list(kernels...), list(g1, ...))`, so the evaluator's memo keeps it per set of facts: a context
//! (a hash-consed term) is compiled once, however many questions are asked under it.
//!
//! **Reducing** a candidate `e`: `e` and the basis are read in one ring (the basis kernels plus those of `e`, in
//! canonical order; a Groebner basis stays one when variables are added, since degrevlex on the larger ring
//! restricts to the old order), and `e` is fully reduced over Q (Cox, Little and O'Shea, *Ideals, Varieties, and
//! Algorithms*, 2.3). The normal form `r` satisfies `e - r ∈ (g1, ..., gk)`, so `e = r` wherever the facts hold:
//! deciding the sign of `r` decides that of `e`. A basis `[1]` (inconsistent facts) is not used.
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{Native, NativeCx, Reply, Res};
use mc_poly::convert::{Conv, Heads};
use mc_poly::groebner::{groebner, GPoly, MonomialOrder};
use mc_poly::qpoly::QPoly;
use mc_poly::work::{Halt, Meter, Work, WorkResult};
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The internal head of the compiled basis (not writable as an identifier).
pub const BASIS: &str = "decide$basis";
/// Most equations compiled, and most kernels.
const MAX_EQS: usize = 12;
const MAX_KERNELS: usize = 12;
/// Most reduction steps of one candidate.
const MAX_STEPS: u64 = 20_000;

const ORDER: MonomialOrder = MonomialOrder::DegRevLex;

/// The evaluator's fuel as a [`Work`] budget.
pub(crate) struct Tick<'c, 'a, 'k, 'h> {
    pub cx: &'c mut NativeCx<'a, 'k, 'h>,
    pub abort: Option<mc_eval::Abort>,
}

impl Work for Tick<'_, '_, '_, '_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        match self.cx.tick(units) {
            Ok(()) => Ok(()),
            Err(a) => {
                self.abort = Some(a);
                Err(Halt::Stopped)
            }
        }
    }
}

/// The native `decide$basis(list(d1, ...))`.
pub struct BasisNative {
    pub(crate) stop: Arc<AtomicBool>,
}

impl Native for BasisNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [ds] = args else { return Ok(Reply::Decline) };
        let Some(ds) = cx.as_list(*ds) else { return Ok(Reply::Decline) };
        let ds = ds.to_vec();
        let h: &'h Heap = cx.heap();
        let ar = Arith::new(h).with_stop(self.stop.clone());
        let hd = Heads::new(h);
        let mut tk = Tick { cx, abort: None };
        let r = {
            let mut m = Meter::new(&mut tk);
            compile(&ar, &hd, &ds, &mut m)
        };
        let cx = tk.cx;
        if let Some(a) = tk.abort {
            return Err(a);
        }
        Ok(Reply::Done(match r {
            Ok(Some((ks, gs))) => {
                let kl = cx.list(&ks);
                let gl = cx.list(&gs);
                cx.list(&[kl, gl])
            }
            _ => cx.list(&[]),
        }))
    }

    fn reads_term(&self) -> bool {
        false
    }
}

/// The kernels and the basis polynomials (as terms) of the equations `ds = 0`; `None` when there are too many or
/// they are not polynomial.
fn compile<'h>(ar: &Arith<'h, Heap>, hd: &Heads<'h>, ds: &[T<'h>], m: &mut Meter) -> WorkResult<Option<(Vec<T<'h>>, Vec<T<'h>>)>> {
    if ds.is_empty() || ds.len() > MAX_EQS {
        return Ok(None);
    }
    let conv = Conv::new(ar, hd);
    let (ring, shared) = conv.ring(ds, &[], m)?;
    if ring.nvars() == 0 || ring.nvars() > MAX_KERNELS || (0..ring.nvars()).any(|i| ring.is_numeric(ar, i)) {
        return Ok(None);
    }
    let main: Vec<usize> = (0..ring.nvars()).collect();
    let mut input = vec![];
    for &d in ds {
        let q = conv.to_poly(d, &ring, &shared, m)?;
        input.push(GPoly::from_poly(&q.num, &main, &[], ORDER));
    }
    let basis = groebner(&input, ORDER, m)?;
    if basis.iter().any(|g| g.is_constant()) {
        // inconsistent facts: nothing is decided from them
        return Ok(None);
    }
    let mut out = vec![];
    for g in &basis {
        let p = g.to_poly(ring.nvars(), &main, &[]);
        out.push(conv.int_poly_term(&p, &ring, m)?);
    }
    Ok(Some((ring.kernels.clone(), out)))
}

/// A polynomial over Q as terms `(exponents, coefficient)`, decreasing in degrevlex.
type Terms = Vec<(Vec<u32>, Rat)>;

fn terms_of(q: &QPoly) -> Terms {
    let mut t: Terms = (0..q.num.len()).map(|i| (q.num.exps(i).to_vec(), q.coef(i))).collect();
    t.sort_by(|a, b| ORDER.cmp(&b.0, &a.0));
    t
}

/// `p - c x^s g` (terms merged in degrevlex).
fn sub_shifted(p: &Terms, c: &Rat, s: &[u32], g: &Terms) -> Terms {
    let mut acc: FxHashMap<Vec<u32>, Rat> = FxHashMap::default();
    for (e, x) in p {
        acc.insert(e.clone(), x.clone());
    }
    for (e, x) in g {
        let k: Vec<u32> = e.iter().zip(s).map(|(a, b)| a + b).collect();
        let v = acc.remove(&k).unwrap_or_else(|| Rat::from_i64(0)).sub(&c.mul(x));
        if !v.is_zero() {
            acc.insert(k, v);
        }
    }
    let mut out: Terms = acc.into_iter().collect();
    out.sort_by(|a, b| ORDER.cmp(&b.0, &a.0));
    out
}

/// The normal form of `p` modulo the basis `gs` (all in one ring); `None` past the step bound.
fn normal_form(p: Terms, gs: &[Terms], m: &mut Meter) -> WorkResult<Option<Terms>> {
    let mut p = p;
    let mut rem: Terms = vec![];
    let mut steps = 0u64;
    while let Some((e, c)) = p.first().cloned() {
        steps += 1;
        m.step(1 + p.len() as u64)?;
        if steps > MAX_STEPS {
            return Ok(None);
        }
        let hit = gs.iter().find(|g| g.first().is_some_and(|(lm, _)| lm.iter().zip(&e).all(|(a, b)| a <= b)));
        match hit {
            Some(g) => {
                let (lm, lc) = &g[0];
                let s: Vec<u32> = e.iter().zip(lm).map(|(a, b)| a - b).collect();
                let q = c.mul(&lc.recip().expect("a nonzero leading coefficient"));
                p = sub_shifted(&p, &q, &s, g);
            }
            None => {
                rem.push((e, c));
                p.remove(0);
            }
        }
    }
    Ok(Some(rem))
}

/// A compiled basis: its kernels and polynomials as terms.
#[derive(Clone)]
pub struct Basis<'h> {
    /// the kernels of the basis
    pub kernels: Vec<T<'h>>,
    /// the basis polynomials
    pub polys: Vec<T<'h>>,
}

impl<'h> Basis<'h> {
    /// The basis of the equations `ds = 0`, compiled through the memoised native; `None` when there is none.
    pub fn of(cx: &mut NativeCx<'_, '_, 'h>, ds: &[T<'h>]) -> Res<Option<Basis<'h>>> {
        if ds.is_empty() {
            return Ok(None);
        }
        let h = cx.heap();
        let l = cx.list(ds);
        let r = cx.eval(h.call(h.sym(BASIS), &[l]))?;
        let Some([ks, gs]) = cx.as_list(r) else { return Ok(None) };
        let (Some(ks), Some(gs)) = (cx.as_list(*ks), cx.as_list(*gs)) else { return Ok(None) };
        if gs.is_empty() {
            return Ok(None);
        }
        Ok(Some(Basis { kernels: ks.to_vec(), polys: gs.to_vec() }))
    }

    /// The normal form of `e` modulo the basis as a term, or `None` when `e` is not reduced (not polynomial in the
    /// kernels, unchanged, or past the bounds).
    pub fn reduce(&self, cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, e: T<'h>) -> Res<Option<T<'h>>> {
        let h = cx.heap();
        let hd = Heads::new(h);
        let conv = Conv::new(ar, &hd);
        let mut tk = Tick { cx, abort: None };
        let r = {
            let mut m = Meter::new(&mut tk);
            (|| -> WorkResult<Option<T<'h>>> {
                let mut all = self.polys.clone();
                all.push(e);
                let (ring, shared) = conv.ring(&all, &self.kernels, &mut m)?;
                if ring.nvars() > 2 * MAX_KERNELS || (0..ring.nvars()).any(|i| ring.is_numeric(ar, i)) {
                    return Ok(None);
                }
                // e must share a kernel with the basis, or nothing reduces
                let qe = conv.to_poly(e, &ring, &shared, &mut m)?;
                let mut gs = vec![];
                for &g in &self.polys {
                    gs.push(terms_of(&conv.to_poly(g, &ring, &shared, &mut m)?));
                }
                let pe = terms_of(&qe);
                let Some(nf) = normal_form(pe.clone(), &gs, &mut m)? else { return Ok(None) };
                if nf == pe {
                    return Ok(None);
                }
                let q = to_qpoly(ring.nvars(), &nf);
                Ok(Some(conv.to_term(&q, &ring, &mut m)?))
            })()
        };
        if let Some(a) = tk.abort {
            return Err(a);
        }
        Ok(r.ok().flatten())
    }
}

/// A [`QPoly`] of `nv` variables from rational terms.
fn to_qpoly(nv: usize, t: &Terms) -> QPoly {
    let mut acc = QPoly::zero(nv);
    for (e, c) in t {
        let mono = QPoly::from_poly(mc_poly::poly::Poly::monomial(e, mc_poly::z::Z::ONE));
        acc = acc.add(&mono.scale(c));
    }
    acc
}

/// Is `t` the empty list (an uninstalled or failed basis)?
pub fn is_empty_list<'h>(h: &'h Heap, t: T<'h>) -> bool {
    matches!(h.view(t), View::App { args: &[], .. })
}

/// The order of two exponent vectors in the basis order (for tests).
pub fn order(a: &[u32], b: &[u32]) -> Ordering {
    ORDER.cmp(a, b)
}
