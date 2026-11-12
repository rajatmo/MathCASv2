//! Solution formulas: the decomposition written back as a formula in the free variables.
//!
//! - **Cylindrical form** (`reduce`, `cylindrical_decomposition`): for each free variable in turn, a disjunction
//!   of pieces `x == r`, `a < x < b`, ... with the description of the next variables inside. Adjacent cells of a
//!   stack whose descriptions agree are combined (decision CAD.5): a section joins its neighbouring sectors when
//!   the cylinders above them are described by the same bounds (the same roots of the same factors), so
//!   `x^2 + y^2 < 1 and y > x` gives two pieces, as a hand-written answer would. Bounds are roots of factors over the
//!   cell: rationals, `(-b ± sqrt(b^2 - 4ac))/(2a)` with the branch chosen by the sign of `a` on the cell, or
//!   `root(&1 => f, k)` objects; for `reduce` a single point's coordinate is substituted into the description above
//!   it, and points with identical descriptions are written together (`(x == a or x == b) and y == x - 1`).
//! - **Sign conditions** (`resolve` with free variables): when the true and false cells are distinguished by the
//!   signs of the projection factors, a disjunction of conjunctions of sign conditions, each conjunction widened
//!   greedily (a condition dropped or relaxed while it still excludes every false cell) and covered ones removed
//!   (decision CAD.6). Otherwise the cylindrical form.
use crate::cad::{Cell, FacInfo, Label, Outcome, Truth};
use crate::formula::UNKNOWN;
use mc_api::{names, Store, T};
use mc_arith::num::{Int, Rat};
use mc_arith::{Arith, Num, Real};
use mc_core::Heap;
use mc_poly::convert::{Conv, Heads, Ring};
use mc_poly::ratfun::{self, RatFun};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};

/// The writer of formulas for one decomposition.
pub struct Writer<'h> {
    /// the canonicaliser
    pub ar: Arith<'h, Heap>,
    hd: Heads<'h>,
    /// the variables, in the decomposition order
    pub vars: Vec<T<'h>>,
    ring: Ring<'h>,
    /// decomposition variable i is ring variable `to_ring[i]`
    to_ring: Vec<usize>,
    /// substitute a single point's coordinate into the description above it
    pub substitute: bool,

}

/// A bound of a piece: the labels of a root and the factor data of a cell where it was read.
#[derive(Clone, Debug)]
struct Bound {
    labels: Vec<Label>,
    info: Vec<FacInfo>,
    sector_base: bool,
}

#[derive(Clone, Debug)]
enum Desc {
    True,
    Pieces(Vec<Piece>),
}

#[derive(Clone, Debug)]
enum Piece {
    Point { at: Bound, inner: Desc },
    Interval { lo: Option<(Bound, bool)>, hi: Option<(Bound, bool)>, inner: Desc },
}

fn meet(a: &[Label], b: &[Label]) -> Vec<Label> {
    a.iter().filter(|l| b.contains(l)).copied().collect()
}

/// Merge two bounds (the same root): common labels, the info of a sector base cell preferred.
fn merge_bound(a: &Bound, b: &Bound) -> Option<Bound> {
    let l = meet(&a.labels, &b.labels);
    if l.is_empty() {
        return None;
    }
    let (info, sb) = if a.sector_base || !b.sector_base { (a.info.clone(), a.sector_base) } else { (b.info.clone(), b.sector_base) };
    Some(Bound { labels: l, info, sector_base: sb })
}

fn merge_opt(a: &Option<(Bound, bool)>, b: &Option<(Bound, bool)>) -> Option<Option<(Bound, bool)>> {
    match (a, b) {
        (None, None) => Some(None),
        (Some((x, cx)), Some((y, cy))) if cx == cy => merge_bound(x, y).map(|z| Some((z, *cx))),
        _ => None,
    }
}

/// The merge of two descriptions when they describe the cylinders by the same bounds.
fn merge_desc(a: &Desc, b: &Desc) -> Option<Desc> {
    match (a, b) {
        (Desc::True, Desc::True) => Some(Desc::True),
        (Desc::Pieces(x), Desc::Pieces(y)) if x.len() == y.len() => {
            let mut out = vec![];
            for (p, q) in x.iter().zip(y) {
                out.push(merge_piece(p, q)?);
            }
            Some(Desc::Pieces(out))
        }
        _ => None,
    }
}

fn merge_piece(a: &Piece, b: &Piece) -> Option<Piece> {
    match (a, b) {
        (Piece::Point { at: x, inner: i }, Piece::Point { at: y, inner: j }) => {
            Some(Piece::Point { at: merge_bound(x, y)?, inner: merge_desc(i, j)? })
        }
        (Piece::Interval { lo: l1, hi: h1, inner: i }, Piece::Interval { lo: l2, hi: h2, inner: j }) => {
            Some(Piece::Interval { lo: merge_opt(l1, l2)?, hi: merge_opt(h1, h2)?, inner: merge_desc(i, j)? })
        }
        _ => None,
    }
}

/// The description of a stack (children of a cell whose factor data is `info`).
fn describe(cells: &[Cell], info: &[FacInfo], sector_base: bool) -> Desc {
    // the local pieces: (piece, is section)
    let mut out: Vec<Piece> = vec![];
    // the last piece and whether it ended at a section (so a following sector may extend it)
    for c in cells {
        let inner = match c.truth {
            Truth::False => {
                out.push(Piece::Interval { lo: None, hi: None, inner: Desc::Pieces(vec![]) });
                continue;
            }
            Truth::True => Desc::True,
            Truth::Mixed => describe(&c.children, &c.info, !c.section && sector_base),
        };
        let b = |labels: &Vec<Label>| Bound { labels: labels.clone(), info: info.to_vec(), sector_base };
        let piece = if c.section {
            Piece::Point { at: b(&c.labels), inner }
        } else {
            Piece::Interval { lo: c.lo.as_ref().map(|l| (b(l), false)), hi: c.hi.as_ref().map(|l| (b(l), false)), inner }
        };
        out.push(piece);
    }
    // combine adjacent true pieces (false pieces are markers with an empty description)
    let is_false = |p: &Piece| matches!(p, Piece::Interval { inner: Desc::Pieces(v), lo: None, hi: None } if v.is_empty());
    let mut res: Vec<Piece> = vec![];
    let mut i = 0;
    while i < out.len() {
        if is_false(&out[i]) {
            i += 1;
            continue;
        }
        let mut cur = out[i].clone();
        i += 1;
        while i < out.len() && !is_false(&out[i]) {
            match join(&cur, &out[i]) {
                Some(j) => {
                    cur = j;
                    i += 1;
                }
                None => break,
            }
        }
        res.push(cur);
    }
    Desc::Pieces(res)
}

/// Join a piece with the next one of the stack (a section after a sector, or a sector after a section).
fn join(a: &Piece, b: &Piece) -> Option<Piece> {
    match (a, b) {
        // (lo, r) then {r}: (lo, r]
        (Piece::Interval { lo, hi: Some((h, false)), inner: i }, Piece::Point { at, inner: j }) => {
            let at = merge_bound(h, at)?;
            Some(Piece::Interval { lo: lo.clone(), hi: Some((at, true)), inner: merge_desc(i, j)? })
        }
        // [.., r] or {r} then (r, hi): [.., hi)
        (Piece::Interval { lo, hi: Some((h, true)), inner: i }, Piece::Interval { lo: Some((l, false)), hi, inner: j }) => {
            merge_bound(h, l)?;
            Some(Piece::Interval { lo: lo.clone(), hi: hi.clone(), inner: merge_desc(i, j)? })
        }
        // {r} then (r, hi): [r, hi), only when nothing is described above them (a section with a description joins
        // the sector before it, as written by hand)
        (Piece::Point { at, inner: i }, Piece::Interval { lo: Some((l, false)), hi, inner: j })
            if matches!((i, j), (Desc::True, Desc::True)) =>
        {
            let at = merge_bound(at, l)?;
            Some(Piece::Interval { lo: Some((at, true)), hi: hi.clone(), inner: merge_desc(i, j)? })
        }
        _ => None,
    }
}

impl<'h> Writer<'h> {
    /// A writer for the variables `vars`.
    pub fn new(h: &'h Heap, vars: &[T<'h>], m: &mut Meter) -> WorkResult<Writer<'h>> {
        let ar = Arith::new(h);
        let hd = Heads::new(h);
        let ring = Conv::rational(&ar, &hd).ring(&[], vars, m)?.0;
        let to_ring = vars.iter().map(|&v| ring.var(v).expect("a variable of the ring")).collect();
        Ok(Writer { ar, hd, vars: vars.to_vec(), ring, to_ring, substitute: true })
    }

    /// A writer without a ring (for numbers and root objects in one slot variable).
    pub fn new_unmetered(h: &'h Heap, vars: &[T<'h>]) -> Writer<'h> {
        let mut u = mc_poly::work::Unlimited;
        let mut m = Meter::new(&mut u);
        // invariant: building a ring of symbols does no work that can be interrupted
        Writer::new(h, vars, &mut m).unwrap_or_else(|_| unreachable!("an unlimited meter does not stop"))
    }

    /// The slot symbol `&1`.
    pub fn slot(&self) -> T<'h> {
        let h = self.h();
        h.sym_term(h.sym("&1"))
    }

    fn h(&self) -> &'h Heap {
        self.ar.store()
    }

    /// `name(args)` (not canonicalised).
    pub fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        let h = self.h();
        h.call(h.sym(name), args)
    }

    /// The symbol `true` or `false`.
    pub fn bool(&self, b: bool) -> T<'h> {
        let h = self.h();
        h.sym_term(h.sym(if b { names::TRUE } else { names::FALSE }))
    }

    /// A rational as a term.
    pub fn rat(&self, q: &Rat) -> T<'h> {
        self.ar.number(&Num::Real(Real::Exact(q.clone())))
    }

    /// An integer polynomial of the decomposition's ring as a term.
    pub fn poly(&self, p: &Poly, m: &mut Meter) -> WorkResult<T<'h>> {
        Conv::rational(&self.ar, &self.hd).int_poly_term(&p.permute(&self.to_ring), &self.ring, m)
    }

    /// `num/den` in lowest terms as a term.
    fn ratfun(&self, num: Poly, den: Poly, m: &mut Meter) -> WorkResult<T<'h>> {
        let c = Conv::rational(&self.ar, &self.hd);
        let g = mc_poly::gcd::gcd(&num, &den, m)?;
        let (n, d) = if g.is_constant() {
            (num, den)
        } else {
            (mc_poly::work::exact(num.div_exact(&g, m)?, "gcd")?, mc_poly::work::exact(den.div_exact(&g, m)?, "gcd")?)
        };
        // a negative leading denominator moves its sign up
        let (n, d) = if d.lc().is_neg() { (n.neg(), d.neg()) } else { (n, d) };
        ratfun::to_term(&c, &RatFun { num: n.permute(&self.to_ring), den: d.permute(&self.to_ring) }, &self.ring, m)
    }

    /// The `k`-th real root (from 1) of the factor `f` in variable `v`, on a cell where its data is `fi`.
    pub fn root(&self, f: &Poly, v: usize, k: usize, fi: Option<&FacInfo>, m: &mut Meter) -> WorkResult<T<'h>> {
        let cs = crate::poly::coeffs(f, v);
        let deg = cs.len() - 1;
        let lc_sign = fi.map_or(1, |i| i.lc_sign);
        let nroots = fi.map_or(deg, |i| i.nroots);
        if deg == 1 || (deg == 2 && lc_sign == 0 && !cs[1].is_zero()) {
            // b y + c: -c/b
            let (b, c) = (cs[1].clone(), cs[0].clone());
            return self.ratfun(c.neg(), b, m);
        }
        if deg == 2 && lc_sign != 0 {
            let (a, b, c) = (&cs[2], &cs[1], &cs[0]);
            let four = Poly::constant(f.nvars(), Z::from_i128(4));
            let disc = b.mul(b, m)?.sub(&a.mul(c, m)?.mul(&four, m)?);
            // sqrt(D) = s sqrt(c' P) with s^2 c' the integer content of D; then the common integer factor g of b,
            // s and 2a cancels: ((-b/g) -+ (s/g) sqrt(c' P)) / (2a/g)
            let cont = disc.content().abs();
            let prim = if cont.is_zero() { disc.clone() } else { disc.div_exact_scalar(&cont) };
            let (s, rest) = square_part(&cont);
            let two_a = a.mul(&Poly::constant(f.nvars(), Z::from_i128(2)), m)?;
            let g = b.content().abs().gcd(&s).gcd(&two_a.content().abs());
            let g = if g.is_zero() { Z::ONE } else { g };
            let sq = if disc.is_zero() {
                self.ar.int(0)
            } else {
                let rad = self.ar.mul(&[self.ar.integer(&rest.to_int()), self.poly(&prim, m)?]);
                let h = self.h();
                let root2 = h.call(h.sym(names::POW), &[rad, self.rat(&Rat::new(Int::from_i64(1), Int::from_i64(2)))]);
                self.call(names::MUL, &[self.ar.integer(&s.div_exact(&g).to_int()), root2])
            };
            // the smaller root: (-b - sqrt D)/(2a) when a > 0
            let minus_first = lc_sign > 0;
            let use_minus = if nroots >= 2 { (k == 1) == minus_first } else { minus_first };
            let nb = self.poly(&b.neg().div_exact_scalar(&g), m)?;
            let sgn = if use_minus { self.call(names::MUL, &[self.ar.int(-1), sq]) } else { sq };
            let num = self.call(names::ADD, &[nb, sgn]);
            let den = two_a.div_exact_scalar(&g);
            let h = self.h();
            return Ok(match den.as_constant() {
                Some(c) => self.call(names::MUL, &[self.rat(&Rat::new(Int::from_i64(1), c.to_int())), num]),
                None => self.call(names::MUL, &[num, h.call(h.sym(names::POW), &[self.poly(&den, m)?, self.ar.int(-1)])]),
            });
        }
        // a root object over the slot variable
        let h = self.h();
        let slot = h.sym_term(h.sym("&1"));
        let mut ks = self.vars.clone();
        ks[v] = slot;
        let ring = Conv::rational(&self.ar, &self.hd).ring(&[], &ks, m)?.0;
        let perm: Vec<usize> = ks.iter().map(|&k| ring.var(k).expect("a variable of the ring")).collect();
        let body = Conv::rational(&self.ar, &self.hd).int_poly_term(&f.permute(&perm), &ring, m)?;
        let lam = self.call("fn", &[self.call(names::LIST, &[slot]), body]);
        Ok(self.call("root", &[lam, self.ar.int(k as i64)]))
    }

    fn bound_term(&self, o: &Outcome, b: &Bound, v: usize, m: &mut Meter) -> WorkResult<T<'h>> {
        let (f, k) = *b.labels.iter().min_by_key(|(f, _)| o.basis.facs[*f].degree(v).unwrap_or(0)).expect("a label");
        let fi = b.info.iter().find(|i| i.fac == f);
        self.root(&o.basis.facs[f], v, k, fi, m)
    }

    /// The pieces of a description, each a list of conjuncts: the condition on this variable (an interval is one
    /// conjunct) followed by the conjuncts of the description above it when that is a single piece.
    fn desc_parts(&self, o: &Outcome, d: &Desc, v: usize, m: &mut Meter) -> WorkResult<Option<Vec<Vec<T<'h>>>>> {
        let Desc::Pieces(ps) = d else { return Ok(None) };
        let x = self.vars[v];
        let mut out: Vec<Vec<T<'h>>> = vec![];
        for p in ps {
            let (cond, point, inner) = match p {
                Piece::Point { at, inner } => {
                    let r = self.bound_term(o, at, v, m)?;
                    (Some(self.call("equal", &[x, r])), Some(r), inner)
                }
                Piece::Interval { lo, hi, inner } => {
                    let lt = |c: bool| if c { "less_equal" } else { "less" };
                    let gt = |c: bool| if c { "greater_equal" } else { "greater" };
                    let cond = match (lo, hi) {
                        (None, None) => None,
                        (Some((l, c)), None) => Some(self.call(gt(*c), &[x, self.bound_term(o, l, v, m)?])),
                        (None, Some((u, c))) => Some(self.call(lt(*c), &[x, self.bound_term(o, u, v, m)?])),
                        (Some((l, c1)), Some((u, c2))) => {
                            let a = self.call(lt(*c1), &[self.bound_term(o, l, v, m)?, x]);
                            let b = self.call(lt(*c2), &[x, self.bound_term(o, u, v, m)?]);
                            Some(self.call("and", &[a, b]))
                        }
                    };
                    (cond, None, inner)
                }
            };
            let mut conj: Vec<T<'h>> = cond.into_iter().collect();
            let above: Vec<T<'h>> = match self.desc_parts(o, inner, v + 1, m)? {
                None => vec![],
                Some(parts) if parts.len() == 1 => parts.into_iter().next().expect("one piece"),
                Some(parts) => vec![self.or_term(parts)],
            };
            // a single point's coordinate is substituted into the description above it
            for t in above {
                conj.push(match (point, self.substitute) {
                    (Some(r), true) => {
                        let rule = self.call(names::RULE, &[x, r]);
                        self.call("rewrite", &[t, rule])
                    }
                    _ => t,
                });
            }
            out.push(conj);
        }
        Ok(Some(out))
    }

    fn and_term(&self, conj: Vec<T<'h>>) -> T<'h> {
        match conj.len() {
            0 => self.bool(true),
            1 => conj[0],
            _ => self.call("and", &conj),
        }
    }

    fn or_term(&self, parts: Vec<Vec<T<'h>>>) -> T<'h> {
        let ts: Vec<T<'h>> = parts.into_iter().map(|c| self.and_term(c)).collect();
        match ts.len() {
            0 => self.bool(false),
            1 => ts[0],
            _ => self.call("or", &ts),
        }
    }

    fn desc_term(&self, o: &Outcome, d: &Desc, v: usize, m: &mut Meter) -> WorkResult<T<'h>> {
        Ok(match self.desc_parts(o, d, v, m)? {
            None => self.bool(true),
            Some(parts) => self.or_term(parts),
        })
    }

    /// The cylindrical form of the free cells of a decomposition.
    pub fn cylindrical(&self, o: &Outcome, m: &mut Meter) -> WorkResult<T<'h>> {
        if let Some(t) = o.truth {
            return Ok(self.bool(t));
        }
        let d = describe(&o.cells, &o.info, true);
        self.desc_term(o, &d, 0, m)
    }

    /// The sign-condition form of the free cells, when the signs of the projection factors separate the true
    /// cells from the false ones; `None` otherwise.
    pub fn sign_formula(&self, o: &Outcome, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
        if let Some(t) = o.truth {
            return Ok(Some(self.bool(t)));
        }
        let nfree = o.problem.nfree;
        let facs: Vec<usize> = (0..o.basis.facs.len()).filter(|&f| o.basis.lv[f] <= nfree).collect();
        let mut sigs: Vec<(Vec<i8>, bool)> = vec![];
        collect(&o.cells, &facs, &mut sigs);
        let (ts, fs): (Vec<_>, Vec<_>) = sigs.into_iter().partition(|s| s.1);
        let ts: Vec<Vec<i8>> = dedup(ts.into_iter().map(|s| s.0).collect());
        let fs: Vec<Vec<i8>> = dedup(fs.into_iter().map(|s| s.0).collect());
        if ts.is_empty() {
            return Ok(Some(self.bool(false)));
        }
        if fs.is_empty() {
            return Ok(Some(self.bool(true)));
        }
        let n = facs.len();
        let covers = |c: &[u8], s: &[i8]| (0..n).all(|i| s[i] == UNKNOWN || c[i] & bit(s[i]) != 0);
        // separable?
        for t in &ts {
            let c: Vec<u8> = t.iter().map(|&x| if x == UNKNOWN { 7 } else { bit(x) }).collect();
            if fs.iter().any(|f| covers(&c, f)) {
                return Ok(None);
            }
        }
        let mut imps: Vec<Vec<u8>> = vec![];
        for t in &ts {
            let mut c: Vec<u8> = t.iter().map(|&x| if x == UNKNOWN { 7 } else { bit(x) }).collect();
            // widen: drop a condition, else relax it, highest level first
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by_key(|&i| std::cmp::Reverse(o.basis.lv[facs[i]]));
            for &i in &order {
                if c[i] == 7 {
                    continue;
                }
                let old = c[i];
                let mut best = old;
                for w in [7u8, old | 2, old | 5 & !old, old | 1, old | 4] {
                    if w & old != old || w == old {
                        continue;
                    }
                    c[i] = w;
                    if !fs.iter().any(|f| covers(&c, f)) {
                        best = w;
                        break;
                    }
                }
                c[i] = best;
            }
            if !imps.contains(&c) {
                imps.push(c);
            }
        }
        // drop implicants whose true cells the others cover (fewest conditions kept first)
        // the order of the disjuncts: by the condition on each factor, lowest level first, `<` before `<=`,
        // `==`, `>=`, `>`, `!=` and no condition
        let mut by_level: Vec<usize> = (0..n).collect();
        by_level.sort_by_key(|&i| (o.basis.lv[facs[i]], facs[i]));
        let code = |s: u8| match s {
            1 => 0u8,
            3 => 1,
            2 => 2,
            6 => 3,
            4 => 4,
            5 => 5,
            _ => 6,
        };
        let key = |c: &Vec<u8>| -> Vec<u8> { by_level.iter().map(|&i| code(c[i])).collect() };
        imps.sort_by_key(|c| c.iter().filter(|&&x| x != 7).count());
        let mut keep: Vec<Vec<u8>> = imps.clone();
        let mut i = keep.len();
        while i > 0 {
            i -= 1;
            let others: Vec<&Vec<u8>> = keep.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, c)| c).collect();
            let strict = |c: &[u8], s: &[i8]| (0..n).all(|k| if s[k] == UNKNOWN { c[k] == 7 } else { c[k] & bit(s[k]) != 0 });
            let redundant = ts.iter().filter(|t| covers(&keep[i], t)).all(|t| others.iter().any(|c| strict(c, t)));
            if redundant {
                keep.remove(i);
            }
        }
        keep.sort_by_key(key);
        let mut ors = vec![];
        for c in &keep {
            let mut ands = vec![];
            let mut order: Vec<usize> = (0..n).filter(|&i| c[i] != 7).collect();
            order.sort_by_key(|&i| o.basis.lv[facs[i]]);
            for i in order {
                ands.push(self.sign_atom(&o.basis.facs[facs[i]], c[i], m)?);
            }
            ors.push(match ands.len() {
                0 => self.bool(true),
                1 => ands[0],
                _ => self.call("and", &ands),
            });
        }
        Ok(Some(match ors.len() {
            1 => ors[0],
            _ => self.call("or", &ors),
        }))
    }

    /// `p rel 0` for a set of signs (bits: 1 negative, 2 zero, 4 positive); a linear polynomial in one variable is
    /// written `x rel c`.
    fn sign_atom(&self, p: &Poly, set: u8, m: &mut Meter) -> WorkResult<T<'h>> {
        let rel = match set {
            1 => "less",
            2 => "equal",
            4 => "greater",
            3 => "less_equal",
            6 => "greater_equal",
            _ => "unequal",
        };
        let used = p.used_vars();
        if let [v] = used.as_slice() {
            if p.degree(*v) == Some(1) {
                let cs = crate::poly::coeffs(p, *v);
                let c1 = cs[1].as_constant().expect("univariate");
                let val = self.ratfun(cs[0].neg(), cs[1].clone(), m)?;
                let rel = if c1.is_neg() { flip(rel) } else { rel };
                return Ok(self.call(rel, &[self.vars[*v], val]));
            }
        }
        Ok(self.call(rel, &[self.poly(p, m)?, self.ar.int(0)]))
    }
}

/// `n = s^2 r` with `r` free of small square factors (primes below 1000 tried).
fn square_part(n: &Z) -> (Z, Z) {
    if n.is_zero() {
        return (Z::ONE, Z::ZERO);
    }
    let mut s = Z::ONE;
    let mut r = n.abs();
    let mut p = 2i64;
    while p < 1000 {
        let pp = Z::from_i128((p * p) as i128);
        while r.divisible(&pp) {
            r = r.div_exact(&pp);
            s = s.mul_i64(p);
        }
        p += if p == 2 { 1 } else { 2 };
    }
    (s, r)
}

fn flip(rel: &str) -> &'static str {
    match rel {
        "less" => "greater",
        "greater" => "less",
        "less_equal" => "greater_equal",
        "greater_equal" => "less_equal",
        "equal" => "equal",
        _ => "unequal",
    }
}

fn bit(s: i8) -> u8 {
    match s {
        -1 => 1,
        0 => 2,
        _ => 4,
    }
}

fn dedup(mut v: Vec<Vec<i8>>) -> Vec<Vec<i8>> {
    v.sort();
    v.dedup();
    v
}

/// The signatures (over `facs`) and truths of the free cells with a decided truth.
fn collect(cells: &[Cell], facs: &[usize], out: &mut Vec<(Vec<i8>, bool)>) {
    for c in cells {
        match c.truth {
            Truth::Mixed => collect(&c.children, facs, out),
            t => {
                let s: Vec<i8> = facs.iter().map(|&f| c.signs.get(f).copied().unwrap_or(UNKNOWN)).collect();
                out.push((s, t == Truth::True));
            }
        }
    }
}

/// Every free leaf cell's sample point and truth (for checks).
pub fn leaves(o: &Outcome) -> Vec<(Vec<crate::cad::Coord>, bool)> {
    fn walk(cells: &[Cell], pre: &mut Vec<crate::cad::Coord>, out: &mut Vec<(Vec<crate::cad::Coord>, bool)>) {
        for c in cells {
            pre.push(c.coord.clone());
            match c.truth {
                Truth::Mixed => walk(&c.children, pre, out),
                t => out.push((pre.clone(), t == Truth::True)),
            }
            pre.pop();
        }
    }
    let mut out = vec![];
    walk(&o.cells, &mut vec![], &mut out);
    out
}

