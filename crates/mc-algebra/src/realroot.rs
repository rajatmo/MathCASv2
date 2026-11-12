//! Real roots of polynomials with algebraic-number coefficients, real algebraic numbers with exact comparison and
//! exact signs of polynomials at them, and sample points between roots (roadmap phase F, item F2; decisions
//! F2.5-F2.7). This is what cylindrical decomposition (phase 11A) lifts with: the sample points of a cell are
//! [`RealAlg`]s, the polynomials over a sample point have coefficients in `Q(sample)`, and their real roots come back
//! as [`RealAlg`]s again.
//!
//! - A [`RealAlg`] is a real algebraic number as its minimal polynomial over Q (primitive, positive leading
//!   coefficient) with an isolating interval, open with rational ends that are not roots, or a single rational. Two
//!   are compared exactly: intervals first, refined until they are disjoint when the numbers differ; equality only
//!   when the minimal polynomials are equal and the intervals hold the same root (decided by one sign evaluation).
//! - [`real_roots`]: the real roots of `p` in `K[x]`, `K` a number field with a real embedding, with multiplicity.
//!   Square-free factors over K are isolated first by Descartes' rule in **ball arithmetic** (the coefficients are
//!   enclosed in real balls through the embedding; a sign variation is counted only when every sign is certain, so a
//!   test with one variation and no uncertain coefficient proves one root, as in the bitstream Descartes method of
//!   Eigenwillig, Kettner, Krandick, Mehlhorn, Schmitt and Wolpert, "A Descartes algorithm for polynomials with
//!   bit-stream coefficients", CASC 2005), with the precision doubled until it decides. Each isolated root is then
//!   identified exactly as the root of one irreducible factor of the norm `N_{K/Q}(p)`. When balls cannot decide at
//!   the highest precision (a root at a bisection point, roots closer than the precision), the **exact** method
//!   takes over: the gcd over K of `p` with each irreducible factor `q` of the norm says how many roots of `q` are
//!   roots of `p`, and certified complex enclosures of `q`'s roots are refined until all other roots are excluded
//!   (decision F2.5).
//! - [`sign_at_algebraic`]: the sign of `q(ξ)` for `q` over K and a [`RealAlg`] `ξ`: balls first; when a ball
//!   evaluation cannot exclude zero, the exact value in the field `K(ξ)` (a primitive element from `mc-algebra`'s
//!   towers) decides (decision F2.6).
//! - [`sample_points`]: sorted distinct roots and one rational point in each gap and beyond them, the simplest
//!   rational in each open interval (decision F2.7).
//!
//! The integer case (`K = Q`) uses `mc_poly::descartes` (Vincent–Collins–Akritas bisection; Rouillier and
//! Zimmermann, "Efficient isolation of polynomial's real roots", J. Comput. Appl. Math. 162 (2004)) on the
//! irreducible factors over Q directly.
use crate::algnum::{self, AlgNum};
use crate::embed;
use crate::field::{Elem, NumberField};
use crate::kpoly;
use crate::qpoly::{self, q, QPoly};
use mc_arith::num::{Ball, Float, Int, Mag, Nat, Rat};
use mc_numeric::cball::CBall;
use mc_poly::descartes::{self, Interval};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Z;
use std::cmp::Ordering;

/// The highest precision of the ball isolation (bits).
const MAX_BALL_BITS: u64 = 4096;
/// The most bisection levels below the root bound in the ball isolation, per bit of precision.
const DEPTH_PER_BITS: u64 = 2;
/// The most refinements of an exact comparison or identification.
const MAX_REFINE: usize = 20_000;

// ------------------------------------------------------------------------------------------- RealAlg

/// A real algebraic number: its minimal polynomial over Q and an isolating interval of the root (decision F2.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealAlg {
    /// the minimal polynomial: primitive integer coefficients, positive leading coefficient, index = degree
    pub poly: Vec<Z>,
    /// an open isolating interval of the root with rational ends that are not roots, or the rational root itself
    pub iv: Interval,
}

impl RealAlg {
    /// A rational number.
    pub fn rational(c: &Rat) -> RealAlg {
        let poly = vec![Z::from_int(c.num.neg()), Z::from_nat(c.den.clone())];
        RealAlg { poly, iv: Interval { lo: c.clone(), hi: c.clone() } }
    }
    /// The value when rational.
    pub fn as_rat(&self) -> Option<Rat> {
        self.iv.is_exact().then(|| self.iv.lo.clone())
    }
    /// The degree of the minimal polynomial.
    pub fn degree(&self) -> usize {
        self.poly.len() - 1
    }
    /// Halve the interval once.
    pub fn halve(&mut self) {
        self.iv = descartes::halve(&self.poly, &self.iv);
        self.normalise();
    }
    /// Refine the interval to a width of at most `width`.
    pub fn refine(&mut self, width: &Rat, m: &mut Meter) -> WorkResult<()> {
        self.iv = descartes::refine(&self.poly, &self.iv, width, m)?;
        self.normalise();
        Ok(())
    }
    /// A degree-1 polynomial or an interval shrunk to a point: the rational form.
    fn normalise(&mut self) {
        if self.iv.is_exact() && self.poly.len() > 2 {
            *self = RealAlg::rational(&self.iv.lo.clone());
        }
        if self.poly.len() == 2 && !self.iv.is_exact() {
            let r = Rat::new(self.poly[0].neg().to_int(), self.poly[1].to_int());
            self.iv = Interval { lo: r.clone(), hi: r };
        }
    }
    /// A real ball enclosing the number with about `bits` bits after the binary point.
    pub fn ball(&mut self, bits: u64, m: &mut Meter) -> WorkResult<Ball> {
        let w = Rat::new(Int::from_i64(1), Int::from_nat(Nat::pow2(bits)));
        self.refine(&w, m)?;
        Ok(interval_ball(&self.iv, bits + 8))
    }
    /// The field `Q(self)` with the embedding that is this number; `None` when the root could not be located.
    pub fn field(&self, m: &mut Meter) -> WorkResult<Option<NumberField>> {
        if let Some(c) = self.as_rat() {
            let e = embed::Embedding { index: 1, real: true, re: c.clone(), im: q(0), radius: q(0) };
            return Ok(Some(NumberField::new(vec![c.neg(), q(1)], Some(e))));
        }
        let f = qpoly::monic(&qpoly::from_ints(&self.poly.iter().map(Z::to_int).collect::<Vec<_>>()));
        let Some(rs) = embed::roots(&f, m)? else { return Ok(None) };
        let mut me = self.clone();
        let mut z = |bits: u64, m: &mut Meter| -> WorkResult<Option<CBall>> { Ok(Some(CBall::real(me.ball(bits, m)?))) };
        let Some(e) = embed::locate(&f, &rs, &mut z, m)? else { return Ok(None) };
        Ok(Some(NumberField::new(f, Some(e))))
    }
    /// The number as an [`AlgNum`] (the generator of its own field).
    pub fn alg_num(&self, m: &mut Meter) -> WorkResult<Option<AlgNum>> {
        if let Some(c) = self.as_rat() {
            return Ok(Some(AlgNum::rational(&c)));
        }
        Ok(self.field(m)?.map(|k| AlgNum { a: k.gen(), k }))
    }
    /// The sign of the integer polynomial `p` at this number, exactly.
    pub fn sign_of(&mut self, p: &[Z], m: &mut Meter) -> WorkResult<i32> {
        if let Some(c) = self.as_rat() {
            return Ok(descartes::sign_at(p, &c));
        }
        let p = trim_z(p.to_vec());
        if p.is_empty() {
            return Ok(0);
        }
        // the minimal polynomial divides p exactly when p vanishes here
        if divides(&self.poly, &p, m)? {
            return Ok(0);
        }
        for _ in 0..MAX_REFINE {
            if let Some(s) = interval_sign(&p, &self.iv) {
                return Ok(s);
            }
            m.step(p.len() as u64 * 4)?;
            self.halve();
            if let Some(c) = self.as_rat() {
                return Ok(descartes::sign_at(&p, &c));
            }
        }
        Err(mc_poly::work::Halt::Stopped)
    }
    /// The exact order of two real algebraic numbers.
    pub fn compare(&mut self, o: &mut RealAlg, m: &mut Meter) -> WorkResult<Ordering> {
        if let (Some(a), Some(b)) = (self.as_rat(), o.as_rat()) {
            return Ok(a.cmp_value(&b));
        }
        if self.poly == o.poly {
            if same_root(self, o) {
                return Ok(Ordering::Equal);
            }
        } else if let Some(c) = self.as_rat() {
            // a rational and an irrational number differ: the sign of o's polynomial at c decides on which side
            return rational_vs(&c, o, m);
        } else if let Some(c) = o.as_rat() {
            return Ok(rational_vs(&c, self, m)?.reverse());
        }
        // distinct numbers: refine until the intervals are disjoint
        for _ in 0..MAX_REFINE {
            m.step(self.poly.len() as u64 + o.poly.len() as u64)?;
            if self.iv.hi.cmp_value(&o.iv.lo) != Ordering::Greater {
                return Ok(Ordering::Less);
            }
            if o.iv.hi.cmp_value(&self.iv.lo) != Ordering::Greater {
                return Ok(Ordering::Greater);
            }
            let w = |iv: &Interval| iv.hi.sub(&iv.lo);
            if w(&self.iv).cmp_value(&w(&o.iv)) == Ordering::Less {
                o.halve();
            } else {
                self.halve();
            }
        }
        Err(mc_poly::work::Halt::Stopped)
    }
}

fn trim_z(mut p: Vec<Z>) -> Vec<Z> {
    while p.last().is_some_and(Z::is_zero) {
        p.pop();
    }
    p
}

/// Does the integer polynomial `d` divide `p` over Q?
fn divides(d: &[Z], p: &[Z], m: &mut Meter) -> WorkResult<bool> {
    let dq: QPoly = d.iter().map(|c| Rat::from_int(c.to_int())).collect();
    let pq: QPoly = p.iter().map(|c| Rat::from_int(c.to_int())).collect();
    Ok(qpoly::divrem(&pq, &dq, m)?.1.is_empty())
}

/// The ball of an interval at precision `w`.
fn interval_ball(iv: &Interval, w: u64) -> Ball {
    let mid = iv.mid();
    let half = iv.hi.sub(&iv.lo).mul(&Rat::new(Int::from_i64(1), Int::from_i64(2)));
    let b = embed::rat_ball(&mid, w);
    let hb = embed::rat_ball(&half, 64);
    let rad = b.rad.add(&Mag::from_float_up(&hb.mid)).add(&hb.rad);
    Ball { mid: b.mid, rad }
}

/// The sign of `p` on the whole interval when the ball evaluation excludes zero.
fn interval_sign(p: &[Z], iv: &Interval) -> Option<i32> {
    let x = interval_ball(iv, 128);
    let mut v = Ball::from_i64(0);
    for c in p.iter().rev() {
        v = v.mul(&x, 128).add(&Ball::exact(Float::new(c.is_neg(), c.mag(), 0)), 128);
    }
    (!v.contains_zero()).then_some(if v.mid.neg { -1 } else { 1 })
}

/// Do two isolating intervals of roots of the same square-free polynomial hold the same root?
fn same_root(a: &RealAlg, b: &RealAlg) -> bool {
    let (x, y) = (&a.iv, &b.iv);
    if x.hi.cmp_value(&y.lo) != Ordering::Greater || y.hi.cmp_value(&x.lo) != Ordering::Greater {
        return false;
    }
    let inside = |s: &Interval, t: &Interval| s.lo.cmp_value(&t.lo) != Ordering::Less && s.hi.cmp_value(&t.hi) != Ordering::Greater;
    if inside(x, y) || inside(y, x) {
        return true;
    }
    // partial overlap: lo1 < lo2 < hi1 < hi2 (or mirrored); is a's root right of lo2?
    let (first, second) = if x.lo.cmp_value(&y.lo) == Ordering::Less { (a, b) } else { (b, a) };
    let cut = &second.iv.lo;
    let s_cut = descartes::sign_at(&first.poly, cut);
    if s_cut == 0 {
        // the cut is a root, not in the open interval of `second`; first's root is it only if... it is not open
        return false;
    }
    let s_lo = descartes::sign_at(&first.poly, &first.iv.lo);
    // the root of `first` lies in (cut, hi1) exactly when the sign does not change on (lo1, cut)
    s_lo == s_cut
}

/// The order of a rational `c` and an irrational `x`.
fn rational_vs(c: &Rat, x: &mut RealAlg, m: &mut Meter) -> WorkResult<Ordering> {
    m.step(x.poly.len() as u64)?;
    if c.cmp_value(&x.iv.lo) != Ordering::Greater {
        return Ok(Ordering::Less);
    }
    if c.cmp_value(&x.iv.hi) != Ordering::Less {
        return Ok(Ordering::Greater);
    }
    // c inside the interval: it is not the root (an irrational number), so one side holds the root
    let s = descartes::sign_at(&x.poly, c);
    let slo = descartes::sign_at(&x.poly, &x.iv.lo);
    if s == slo {
        x.iv.lo = c.clone();
        return Ok(Ordering::Less);
    }
    x.iv.hi = c.clone();
    Ok(Ordering::Greater)
}

// ------------------------------------------------------------------------------------------- sample points

/// A sample point of the real line cut by roots.
#[derive(Clone, Debug, PartialEq)]
pub enum Sample {
    /// a rational point inside a gap (or beyond every root)
    Gap(Rat),
    /// a root
    Root(RealAlg),
}

/// The distinct numbers of `roots` in increasing order with a rational sample in every gap and beyond them (one
/// rational point when there is no root): `gap, root, gap, root, ..., gap`.
pub fn sample_points(roots: &[RealAlg], m: &mut Meter) -> WorkResult<Vec<Sample>> {
    let mut rs: Vec<RealAlg> = vec![];
    for r in roots {
        let mut r = r.clone();
        // insertion with exact comparison (the lists are short: the roots of a few polynomials)
        let mut pos = rs.len();
        let mut dup = false;
        for (i, s) in rs.iter_mut().enumerate() {
            match r.compare(s, m)? {
                Ordering::Less => {
                    pos = i;
                    break;
                }
                Ordering::Equal => {
                    dup = true;
                    break;
                }
                Ordering::Greater => {}
            }
        }
        if !dup {
            rs.insert(pos, r);
        }
    }
    // neighbours must have disjoint intervals for the gap points
    for i in 1..rs.len() {
        for _ in 0..MAX_REFINE {
            if rs[i - 1].iv.hi.cmp_value(&rs[i].iv.lo) == Ordering::Less {
                break;
            }
            m.step(rs[i].poly.len() as u64)?;
            let (a, b) = rs.split_at_mut(i);
            a[i - 1].halve();
            b[0].halve();
        }
    }
    let mut out = vec![];
    if rs.is_empty() {
        out.push(Sample::Gap(q(0)));
        return Ok(out);
    }
    out.push(Sample::Gap(below(&rs[0].iv.lo)));
    for i in 0..rs.len() {
        out.push(Sample::Root(rs[i].clone()));
        if i + 1 < rs.len() {
            out.push(Sample::Gap(simplest_between(&rs[i].iv.hi, &rs[i + 1].iv.lo)));
        } else {
            out.push(Sample::Gap(above(&rs[i].iv.hi)));
        }
    }
    Ok(out)
}

/// An integer below `x` (strictly).
fn below(x: &Rat) -> Rat {
    let (f, _) = x.floor_fract();
    Rat::from_int(f.sub(&Int::from_i64(1)))
}

/// An integer above `x` (strictly).
fn above(x: &Rat) -> Rat {
    let (f, _) = x.floor_fract();
    Rat::from_int(f.add(&Int::from_i64(1)))
}

/// The rational with the smallest denominator in the closed interval `[a, b]`, `a <= b` (Stern–Brocot descent via
/// continued fractions).
pub fn simplest_between(a: &Rat, b: &Rat) -> Rat {
    if a.cmp_value(b) == Ordering::Equal {
        return a.clone();
    }
    // an integer in [a, b]
    let (fa, ra) = a.floor_fract();
    let ca = if ra.is_zero() { fa.clone() } else { fa.add(&Int::from_i64(1)) };
    if Rat::from_int(ca.clone()).cmp_value(b) != Ordering::Greater {
        // the integer of least absolute value in [a, b]
        let (fb, _) = b.floor_fract();
        let zero = Int::from_i64(0);
        let pick = if !ca.neg || ca == zero {
            ca
        } else if fb.neg {
            fb
        } else {
            zero
        };
        return Rat::from_int(pick);
    }
    // a and b share the integer part fa: x = fa + 1 / y with y in [1/(b - fa), 1/(a - fa)]
    let fr = Rat::from_int(fa.clone());
    let (da, db) = (a.sub(&fr), b.sub(&fr));
    // invariant: a < b in the same unit interval, so b - fa > 0
    // invariant: a is not an integer here (an integer a would have been returned above), so a - fa > 0
    let y = simplest_between(&db.recip().expect("positive"), &da.recip().expect("positive"));
    fr.add(&y.recip().expect("y >= 1"))
}

// ------------------------------------------------------------------------------------------- isolation over K

/// The real roots of `p` over `k` (a field with a real embedding), in increasing order with multiplicity; `None`
/// when they could not be decided within the precision limits.
pub fn real_roots(k: &NumberField, p: &[Elem], m: &mut Meter) -> WorkResult<Option<Vec<(RealAlg, u32)>>> {
    let p = kpoly::trim(p.to_vec());
    if p.len() <= 1 {
        return Ok(Some(vec![]));
    }
    if let Some(pq) = kpoly::as_rational(k, &p) {
        return Ok(Some(rational_real_roots(&pq, m)?));
    }
    if !k.is_real() {
        return Ok(None);
    }
    let mut out: Vec<(RealAlg, u32)> = vec![];
    for (a, mult) in kpoly::square_free(k, &p, m)? {
        let roots = match ball_roots(k, &a, m)? {
            Some(r) => r,
            None => match exact_roots(k, &a, m)? {
                Some(r) => r,
                None => return Ok(None),
            },
        };
        out.extend(roots.into_iter().map(|r| (r, mult)));
    }
    sort_roots(&mut out, m)?;
    Ok(Some(out))
}

/// The real roots of a rational polynomial with multiplicity, increasing.
pub fn rational_real_roots(p: &[Rat], m: &mut Meter) -> WorkResult<Vec<(RealAlg, u32)>> {
    let mut out = vec![];
    for (f, mult) in qpoly::factor(p, m)? {
        let fz: Vec<Z> = qpoly::primitive(&f).into_iter().map(Z::from_int).collect();
        for iv in descartes::isolate(&fz, m)? {
            let mut r = RealAlg { poly: fz.clone(), iv };
            r.normalise();
            out.push((r, mult));
        }
    }
    sort_roots(&mut out, m)?;
    Ok(out)
}

fn sort_roots(rs: &mut [(RealAlg, u32)], m: &mut Meter) -> WorkResult<()> {
    // insertion sort with exact comparison
    for i in 1..rs.len() {
        let mut j = i;
        while j > 0 {
            let (a, b) = rs.split_at_mut(j);
            if b[0].0.compare(&mut a[j - 1].0, m)? == Ordering::Less {
                rs.swap(j - 1, j);
                j -= 1;
            } else {
                break;
            }
        }
    }
    Ok(())
}

/// An irreducible factor of a norm: integer and rational forms, real root intervals.
type Candidate = (Vec<Z>, QPoly, Vec<Interval>);

/// The irreducible factors over Q of the norm of `a`, as primitive integer polynomials, with their real roots.
fn norm_candidates(k: &NumberField, a: &[Elem], m: &mut Meter) -> WorkResult<Vec<Candidate>> {
    let n = kpoly::norm(k, a, m)?;
    let mut out = vec![];
    for (f, _) in qpoly::factor(&n, m)? {
        let fz: Vec<Z> = qpoly::primitive(&f).into_iter().map(Z::from_int).collect();
        let ivs = descartes::isolate(&fz, m)?;
        out.push((fz, f, ivs));
    }
    Ok(out)
}

/// Exact sign of `a` (over K) at a rational point.
fn sign_k_at(k: &NumberField, a: &[Elem], x: &Rat, m: &mut Meter) -> WorkResult<Option<i32>> {
    let v = kpoly::eval(k, a, &k.of(x));
    AlgNum { k: k.clone(), a: v }.sign(m)
}

/// Ball isolation of the square-free `a` over a real field, each root identified with its minimal polynomial;
/// `None` when the balls could not decide.
fn ball_roots(k: &NumberField, a: &[Elem], m: &mut Meter) -> WorkResult<Option<Vec<RealAlg>>> {
    let Some(ivs) = isolate_balls(k, a, m)? else { return Ok(None) };
    if ivs.is_empty() {
        return Ok(Some(vec![]));
    }
    let cands = norm_candidates(k, a, m)?;
    let mut out = vec![];
    for iv in ivs {
        if iv.is_exact() {
            out.push(RealAlg::rational(&iv.lo));
            continue;
        }
        let Some(r) = identify(k, a, iv, &cands, m)? else { return Ok(None) };
        out.push(r);
    }
    Ok(Some(out))
}

/// The root of `a` in the isolating interval `iv` as the root of one candidate (an irreducible factor of the
/// norm): the interval and the candidates' intervals are refined until exactly one candidate root meets it.
fn identify(
    k: &NumberField,
    a: &[Elem],
    mut iv: Interval,
    cands: &[Candidate],
    m: &mut Meter,
) -> WorkResult<Option<RealAlg>> {
    let mut live: Vec<RealAlg> = cands
        .iter()
        .flat_map(|(fz, _, ivs)| ivs.iter().map(move |i| RealAlg { poly: fz.clone(), iv: i.clone() }))
        .map(|mut r| {
            r.normalise();
            r
        })
        .collect();
    let Some(mut slo) = sign_k_at(k, a, &iv.lo, m)? else { return Ok(None) };
    for _ in 0..MAX_REFINE {
        m.step(live.len() as u64 + 1)?;
        // the open intervals meet (a rational candidate: it lies inside)
        live.retain(|r| r.iv.hi.cmp_value(&iv.lo) == Ordering::Greater && r.iv.lo.cmp_value(&iv.hi) == Ordering::Less);
        if live.len() == 1 {
            return Ok(Some(live.pop().expect("one candidate")));
        }
        if live.is_empty() {
            return Ok(None);
        }
        // halve the root's interval (exact signs of a at rational points) and every candidate's
        let mid = iv.mid();
        let Some(s) = sign_k_at(k, a, &mid, m)? else { return Ok(None) };
        if s == 0 {
            return Ok(Some(RealAlg::rational(&mid)));
        }
        if s == slo {
            iv.lo = mid;
            slo = s;
        } else {
            iv.hi = mid;
        }
        for r in live.iter_mut() {
            r.halve();
        }
    }
    Ok(None)
}

/// The exact method: for each irreducible factor `f` of the norm, `g = gcd_K(a, f)` has `deg g` roots among the
/// roots of `f`; certified complex enclosures of `f`'s roots are refined until all the others are excluded by
/// ball evaluation of `g`.
fn exact_roots(k: &NumberField, a: &[Elem], m: &mut Meter) -> WorkResult<Option<Vec<RealAlg>>> {
    let Some(theta) = k.embedding().cloned() else { return Ok(None) };
    let mut out = vec![];
    for (fz, f, ivs) in norm_candidates(k, a, m)? {
        let g = kpoly::gcd(k, a, &kpoly::lift(&f), m)?;
        let dg = kpoly::deg(&g).unwrap_or(0);
        if dg == 0 {
            continue;
        }
        let rs: Vec<RealAlg> = ivs.into_iter().map(|iv| {
            let mut r = RealAlg { poly: fz.clone(), iv };
            r.normalise();
            r
        }).collect();
        if dg == f.len() - 1 {
            out.extend(rs);
            continue;
        }
        let Some(emb) = embed::roots(&f, m)? else { return Ok(None) };
        let mut excluded = vec![false; emb.len()];
        let mut bits = 64;
        while excluded.iter().filter(|&&e| !e).count() > dg {
            if bits > MAX_BALL_BITS {
                return Ok(None);
            }
            let Some(t) = embed::enclose(k.modulus(), &theta, bits, m)? else { return Ok(None) };
            for (i, e) in emb.iter().enumerate() {
                if excluded[i] {
                    continue;
                }
                let Some(z) = embed::enclose(&f, e, bits, m)? else { return Ok(None) };
                let mut v = CBall::from_i64(0);
                for c in g.iter().rev() {
                    v = v.mul(&z, bits + 16).add(&embed::eval_ball(c, &t, bits + 16), bits + 16);
                }
                if !v.contains_zero() {
                    excluded[i] = true;
                }
            }
            bits *= 2;
        }
        // the real embeddings come first, ascending, as the Descartes intervals do
        let mut ri = 0;
        for (i, e) in emb.iter().enumerate() {
            if !e.real {
                continue;
            }
            if !excluded[i] {
                out.push(rs[ri].clone());
            }
            ri += 1;
        }
    }
    Ok(Some(out))
}

// ------------------------------------------------------------------------------------------- ball Descartes

/// A ball times `2^k`, exactly.
fn shl(b: &Ball, k: i64) -> Ball {
    Ball { mid: Float::new(b.mid.neg, b.mid.man.clone(), b.mid.exp + k), rad: b.rad.mul_2exp(k) }
}

/// `(min, max)` of the sign variations over every choice of the uncertain signs (balls containing zero).
fn variation_range(c: &[Ball]) -> (usize, usize) {
    // dp over the last nonzero sign: state +1, -1 or none (0); min and max variations
    const INF: usize = usize::MAX / 2;
    let mut best: [(usize, usize); 3] = [(0, 0), (INF, 0), (INF, 0)]; // index 0: none, 1: +, 2: -
    let mut valid = [true, false, false];
    for b in c {
        let choices: &[i32] = if b.contains_zero() {
            if b.mid.is_zero() && b.rad.is_zero() {
                &[0]
            } else {
                &[0, 1, -1]
            }
        } else if b.mid.neg {
            &[-1]
        } else {
            &[1]
        };
        let mut nb: [(usize, usize); 3] = [(INF, 0); 3];
        let mut nv = [false; 3];
        for st in 0..3 {
            if !valid[st] {
                continue;
            }
            let (lo, hi) = best[st];
            for &s in choices {
                let (to, add) = match (st, s) {
                    (_, 0) => (st, 0),
                    (0, 1) => (1, 0),
                    (0, -1) => (2, 0),
                    (1, 1) => (1, 0),
                    (1, -1) => (2, 1),
                    (2, -1) => (2, 0),
                    (2, 1) => (1, 1),
                    _ => unreachable!(),
                };
                let cand = (lo + add, hi + add);
                if !nv[to] {
                    nb[to] = cand;
                    nv[to] = true;
                } else {
                    nb[to] = (nb[to].0.min(cand.0), nb[to].1.max(cand.1));
                }
            }
        }
        best = nb;
        valid = nv;
    }
    let mut lo = INF;
    let mut hi = 0;
    for st in 0..3 {
        if valid[st] {
            lo = lo.min(best[st].0);
            hi = hi.max(best[st].1);
        }
    }
    (lo, hi)
}

/// `p(x + 1)` in place, in ball arithmetic at precision `w`.
fn shift_one(p: &mut [Ball], w: u64, m: &mut Meter) -> WorkResult<()> {
    let n = p.len();
    for i in 0..n {
        m.step((n - i) as u64)?;
        for j in (i..n - 1).rev() {
            p[j] = p[j].add(&p[j + 1], w);
        }
    }
    Ok(())
}

/// The isolating intervals of the real roots of the square-free `a` over the real field `k` by Descartes' rule in
/// ball arithmetic; `None` when no precision up to [`MAX_BALL_BITS`] decides.
pub fn isolate_balls(k: &NumberField, a: &[Elem], m: &mut Meter) -> WorkResult<Option<Vec<Interval>>> {
    let mut a = kpoly::trim(a.to_vec());
    let mut zero = false;
    while a.len() > 1 && a[0].is_empty() {
        a.remove(0);
        zero = true;
    }
    if a.len() <= 1 {
        return Ok(Some(if zero { vec![Interval { lo: q(0), hi: q(0) }] } else { vec![] }));
    }
    let mut w = 64;
    while w <= MAX_BALL_BITS {
        let Some(t) = algnum::generator_enclosure(k, w + 32, m)? else { return Ok(None) };
        let cs: Vec<Ball> = a.iter().map(|c| embed::eval_ball(c, &t, w + 32).re).collect();
        if let Some(mut ivs) = isolate_ball_poly(&cs, w, m)? {
            if zero {
                let pos = ivs.iter().position(|iv| iv.lo.cmp_value(&q(0)) != Ordering::Less).unwrap_or(ivs.len());
                ivs.insert(pos, Interval { lo: q(0), hi: q(0) });
            }
            return Ok(Some(ivs));
        }
        w *= 2;
    }
    Ok(None)
}

/// Ball Descartes on real ball coefficients (nonzero constant and leading coefficients); `None` when undecided at
/// this precision.
fn isolate_ball_poly(cs: &[Ball], w: u64, m: &mut Meter) -> WorkResult<Option<Vec<Interval>>> {
    let n = cs.len() - 1;
    if cs[n].contains_zero() || cs[0].contains_zero() {
        return Ok(None);
    }
    // 2^kb above the Cauchy bound 1 + max |c_i / c_n|
    let lead_low = Mag::from_float_down(&cs[n].mid).sub_down(&cs[n].rad);
    if lead_low.is_zero() {
        return Ok(None);
    }
    let top_up = |b: &Ball| -> f64 { Mag::from_float_up(&b.mid).add(&b.rad).log2() };
    let maxc = cs[..n].iter().map(top_up).fold(f64::NEG_INFINITY, f64::max);
    let kb = ((maxc - lead_low.log2()).max(0.0).ceil() as i64 + 2).max(1);
    let neg: Vec<Ball> = cs.iter().enumerate().map(|(i, c)| if i % 2 == 1 { c.neg() } else { c.clone() }).collect();
    let Some(negs) = positive_ball_roots(&neg, kb, w, m)? else { return Ok(None) };
    let Some(pos) = positive_ball_roots(cs, kb, w, m)? else { return Ok(None) };
    let mut out: Vec<Interval> = negs.into_iter().rev().map(|iv| Interval { lo: iv.hi.neg(), hi: iv.lo.neg() }).collect();
    out.extend(pos);
    Ok(Some(out))
}

fn dyadic(c: &Z, e: i64) -> Rat {
    if e >= 0 {
        Rat::from_int(c.to_int().shl(e as u64))
    } else {
        Rat::new(c.to_int(), Int::from_nat(Nat::pow2((-e) as u64)))
    }
}

/// The roots in `(0, 2^kb)`, increasing; `None` when undecided.
fn positive_ball_roots(p: &[Ball], kb: i64, w: u64, m: &mut Meter) -> WorkResult<Option<Vec<Interval>>> {
    let n = p.len() - 1;
    let q0: Vec<Ball> = p.iter().enumerate().map(|(i, c)| shl(c, kb * i as i64)).collect();
    let mut stack: Vec<(Vec<Ball>, Z, u64)> = vec![(q0, Z::ZERO, 0)];
    let mut out = vec![];
    let max_depth = w * DEPTH_PER_BITS / 2 + 8;
    while let Some((qq, c, j)) = stack.pop() {
        m.step(n as u64 + 1)?;
        if j > max_depth {
            return Ok(None);
        }
        let mut r: Vec<Ball> = qq.iter().rev().cloned().collect();
        shift_one(&mut r, w, m)?;
        let (lo, hi) = variation_range(&r);
        if hi == 0 {
            continue;
        }
        if lo == 1 && hi == 1 {
            out.push(Interval { lo: dyadic(&c, kb - j as i64), hi: dyadic(&c.add(&Z::ONE), kb - j as i64) });
            continue;
        }
        // left half 2^n q(x/2), right half left(x + 1)
        let left: Vec<Ball> = qq.iter().enumerate().map(|(i, a)| shl(a, (n - i) as i64)).collect();
        let mut right = left.clone();
        shift_one(&mut right, w, m)?;
        let c2 = c.mul_i64(2);
        stack.push((right, c2.add(&Z::ONE), j + 1));
        stack.push((left, c2, j + 1));
    }
    out.sort_by(|a, b| a.lo.cmp_value(&b.lo));
    Ok(Some(out))
}

/// The sign of `q(ξ)` for `q` over `k` and a real algebraic `ξ`, decided exactly: balls first, the exact value in
/// `K(ξ)` when the balls cannot exclude zero (decision F2.6). `None` when the field could not be built.
pub fn sign_at_algebraic(k: &NumberField, qp: &[Elem], xi: &RealAlg, m: &mut Meter) -> WorkResult<Option<i32>> {
    let qp = kpoly::trim(qp.to_vec());
    if qp.is_empty() {
        return Ok(Some(0));
    }
    if let Some(c) = xi.as_rat() {
        return sign_k_at(k, &qp, &c, m);
    }
    if let Some(qq) = kpoly::as_rational(k, &qp) {
        let (_, ints) = qpoly::clear(&qq);
        let z: Vec<Z> = ints.into_iter().map(Z::from_int).collect();
        return Ok(Some(xi.clone().sign_of(&z, m)?));
    }
    let mut x = xi.clone();
    // balls: the generator's and xi's enclosures at growing precision
    for bits in [64u64, 128, 256] {
        let Some(t) = algnum::generator_enclosure(k, bits + 32, m)? else { return Ok(None) };
        let xb = CBall::real(x.ball(bits, m)?);
        let mut v = CBall::from_i64(0);
        for c in qp.iter().rev() {
            v = v.mul(&xb, bits + 32).add(&embed::eval_ball(c, &t, bits + 32), bits + 32);
        }
        if !v.re.contains_zero() {
            return Ok(Some(if v.re.mid.neg { -1 } else { 1 }));
        }
    }
    // exact: the value in a common field of k and Q(xi)
    let Some(l) = x.field(m)? else { return Ok(None) };
    let Some(tw) = algnum::adjoin(k, &l, m)? else { return Ok(None) };
    let f = &tw.field;
    let mut v = f.zero();
    for c in qp.iter().rev() {
        v = f.add(&f.mul(&v, &tw.beta), &algnum::map_elem(f, c, &tw.theta));
    }
    AlgNum { k: f.clone(), a: v }.sign(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qpoly::of_i64;
    use mc_poly::work::Unlimited;

    fn sqrt_field(n: i64, m: &mut Meter) -> NumberField {
        let f = of_i64(&[-n, 0, 1]);
        let rs = embed::roots(&f, m).unwrap().unwrap();
        NumberField::new(f, Some(rs[1].clone()))
    }

    fn approx(r: &mut RealAlg, m: &mut Meter) -> f64 {
        let b = r.ball(60, m).unwrap();
        embed::rat_f64(&embed::float_rat(&b.mid))
    }

    #[test]
    fn real_algebraic_numbers() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // x^2 - 2 over Q: two roots
        let rs = rational_real_roots(&of_i64(&[-2, 0, 1]), &mut m).unwrap();
        assert_eq!(rs.len(), 2);
        let (mut a, mut b) = (rs[0].0.clone(), rs[1].0.clone());
        assert!((approx(&mut b, &mut m) - 2f64.sqrt()).abs() < 1e-12);
        assert_eq!(a.compare(&mut b, &mut m).unwrap(), Ordering::Less);
        let mut b2 = b.clone();
        b2.halve();
        b2.halve();
        assert_eq!(b.compare(&mut b2, &mut m).unwrap(), Ordering::Equal);
        // the sign of x^2 - 2 at sqrt 2 is 0, of x - 1 is 1
        assert_eq!(b.sign_of(&[Z::S(-2), Z::ZERO, Z::S(1)], &mut m).unwrap(), 0);
        assert_eq!(b.sign_of(&[Z::S(-1), Z::S(1)], &mut m).unwrap(), 1);
        // sqrt 2 against 1.414 and 1.415
        let mut r1 = RealAlg::rational(&Rat::new(Int::from_i64(1414), Int::from_i64(1000)));
        let mut r2 = RealAlg::rational(&Rat::new(Int::from_i64(1415), Int::from_i64(1000)));
        assert_eq!(r1.compare(&mut b, &mut m).unwrap(), Ordering::Less);
        assert_eq!(r2.compare(&mut b, &mut m).unwrap(), Ordering::Greater);
        // simplest rationals
        assert_eq!(simplest_between(&Rat::new(Int::from_i64(1), Int::from_i64(3)), &Rat::new(Int::from_i64(1), Int::from_i64(2))), Rat::new(Int::from_i64(1), Int::from_i64(2)));
        assert_eq!(simplest_between(&Rat::new(Int::from_i64(13), Int::from_i64(10)), &Rat::new(Int::from_i64(14), Int::from_i64(10))), Rat::new(Int::from_i64(4), Int::from_i64(3)));
        assert_eq!(simplest_between(&Rat::from_i64(-3), &Rat::from_i64(5)), Rat::from_i64(0));
        let s = sample_points(&[b.clone(), a.clone(), b.clone()], &mut m).unwrap();
        assert_eq!(s.len(), 5);
        assert_eq!(s[2], Sample::Gap(Rat::from_i64(0)));
    }

    #[test]
    fn roots_over_number_fields() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let k = sqrt_field(2, &mut m);
        let s2 = k.gen();
        let c = |v: i64| k.of(&q(v));
        // x^2 - sqrt 2: roots +- 2^(1/4); (x - sqrt 2)^2 (x + 1): sqrt 2 twice, -1
        let p = vec![k.neg(&s2), c(0), c(1)];
        let rs = real_roots(&k, &p, &mut m).unwrap().unwrap();
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[0].0.poly, vec![Z::S(-2), Z::ZERO, Z::ZERO, Z::ZERO, Z::S(1)]);
        let mut r = rs[1].0.clone();
        assert!((approx(&mut r, &mut m) - 2f64.powf(0.25)).abs() < 1e-12);
        let lin = vec![k.neg(&s2), c(1)];
        let sq = kpoly::mul(&k, &lin, &lin, &mut m).unwrap();
        let p = kpoly::mul(&k, &sq, &[c(1), c(1)], &mut m).unwrap();
        let rs = real_roots(&k, &p, &mut m).unwrap().unwrap();
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[0].0.as_rat(), Some(q(-1)));
        assert_eq!(rs[1].1, 2);
        assert_eq!(rs[1].0.poly, vec![Z::S(-2), Z::ZERO, Z::S(1)]);
        // the exact method agrees with the ball method
        let p = vec![c(-3), k.scale(&s2, &q(5)), c(-1), c(2), c(0), c(1)];
        let a = ball_roots(&k, &p, &mut m).unwrap().unwrap();
        let b = exact_roots(&k, &p, &mut m).unwrap().unwrap();
        let mut a2: Vec<(RealAlg, u32)> = a.into_iter().map(|r| (r, 1)).collect();
        let mut b2: Vec<(RealAlg, u32)> = b.into_iter().map(|r| (r, 1)).collect();
        sort_roots(&mut a2, &mut m).unwrap();
        sort_roots(&mut b2, &mut m).unwrap();
        assert_eq!(a2.len(), b2.len());
        for (x, y) in a2.iter_mut().zip(b2.iter_mut()) {
            assert_eq!(x.0.compare(&mut y.0, &mut m).unwrap(), Ordering::Equal);
        }
        // signs at algebraic points: (x^2 - sqrt 2) at 2^(1/4) is 0; x - sqrt 2 there is negative
        let p0 = vec![k.neg(&s2), c(0), c(1)];
        let r4 = real_roots(&k, &p0, &mut m).unwrap().unwrap();
        let fourth = r4[1].0.clone();
        assert_eq!(sign_at_algebraic(&k, &p0, &fourth, &mut m).unwrap(), Some(0));
        assert_eq!(sign_at_algebraic(&k, &lin, &fourth, &mut m).unwrap(), Some(-1));
    }
}
