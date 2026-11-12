//! The decomposition: preprocessing, projection and lifting with certified sample points, and the truth of every
//! cell (Collins 1975; Collins and Hong 1991 for partial decompositions).
//!
//! **Preprocessing.** Equations of a conjunction are replaced by their lexicographic Groebner basis in the
//! decomposition's variable order (decision CAD.4), so triangular and univariate equations appear before
//! projection; an inconsistent basis makes the conjunction false at once. Every atom is factored into the
//! irreducible factors of the basis. An equation of the conjunction whose factors all have the top level is used
//! as an equational constraint (McCallum 1999): the top stacks are cut only by its roots, and the sectors between
//! them are false.
//!
//! **Lifting** is depth first. A cell's sample point has coordinates in one number field `K` (the rationals
//! while every coordinate is rational): sectors get the simplest rational between their bounds, sections the
//! real algebraic root itself. The stack over a cell comes from the real roots of each factor's Lazard residue at
//! the sample point, isolated over `K` by `mc_algebra::realroot::real_roots` (validated ball arithmetic first,
//! exact arithmetic over `K` as the fallback, decision F2.5), merged by exact comparison. The sign of a factor at a
//! sample point is exact: an element of `K` (sectors) or `sign_at_algebraic` (sections), so every truth value is
//! certified (decision CAD.3). A cell whose truth is already decided by the factors of its level is not lifted
//! further, and a quantified stack stops at the first child that decides it (`exists`: true, `for_all`: false).
//!
//! **Bounds.** Every loop charges the [`Meter`]; the numbers of factors and cells are capped ([`Limits`]), and a
//! progress callback sees the base stack advance.
use crate::formula::{eval, Atom, Fm, PAtom, Quant, Rel, Tri, UNKNOWN};
use crate::poly::{coeffs, eval_full, eval_uni, factors, lazard_residue, rsign};
use crate::project::{Basis, Method};
use mc_algebra::algnum::{adjoin, map_elem, rationals};
use mc_algebra::field::{Elem, NumberField};
use mc_algebra::realroot::{real_roots, sign_at_algebraic, simplest_between, RealAlg};
use mc_algebra::AlgNum;
use mc_arith::num::{Int, Rat};
use mc_poly::fglm::fglm;
use mc_poly::groebner::{groebner, GPoly, MonomialOrder};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;
use std::cmp::Ordering;

/// A problem: a prenex formula in `nv` real variables `x_0, ..., x_(nv-1)` (the decomposition order); the first
/// `nfree` are free, `quants[i]` binds `x_(nfree + i)`.
#[derive(Clone, Debug)]
pub struct Problem {
    /// the number of variables
    pub nv: usize,
    /// the number of free variables
    pub nfree: usize,
    /// the quantifiers of the bound variables, innermost last
    pub quants: Vec<Quant>,
    /// the atoms
    pub atoms: Vec<PAtom>,
    /// the quantifier-free matrix
    pub matrix: Fm,
}

/// Caps on the size of a decomposition.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// projection factors
    pub max_factors: usize,
    /// cells
    pub max_cells: usize,
    /// the projection operator to start with
    pub method: Method,
    /// Groebner preprocessing of the equations
    pub groebner: bool,
    /// equational constraints
    pub equational: bool,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_factors: 300, max_cells: 200_000, method: Method::Brown, groebner: true, equational: true }
    }
}

/// Why a decomposition was not finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CadError {
    /// Brown's projection met a polynomial vanishing identically over a cell of positive dimension
    NotWellOriented,
    /// a sign or a root isolation was not decided within the precision limits
    Undecided,
    /// more factors or cells than the limits allow
    TooLarge,
}

/// A coordinate of a sample point.
#[derive(Clone, Debug)]
pub enum Coord {
    /// a rational number
    Rat(Rat),
    /// a real algebraic number
    Alg(RealAlg),
}

impl Coord {
    /// The rational value, when rational.
    pub fn as_rat(&self) -> Option<Rat> {
        match self {
            Coord::Rat(q) => Some(q.clone()),
            Coord::Alg(a) => a.as_rat(),
        }
    }
}

/// A root of a stack: which factors vanish there, as (factor, index of the root among the factor's real roots
/// over the base cell, from 1).
pub type Label = (usize, usize);

/// The truth of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Truth {
    /// the formula holds on the whole cell (or the quantified formula holds there)
    True,
    /// it holds nowhere on the cell
    False,
    /// it holds on some cells of the cylinder above (free variables only)
    Mixed,
}

/// What the stack over a cell knows about one factor of the next level.
#[derive(Clone, Debug)]
pub struct FacInfo {
    /// the factor
    pub fac: usize,
    /// its number of distinct real roots over the cell
    pub nroots: usize,
    /// the sign of its leading coefficient at the sample point
    pub lc_sign: i32,
    /// its degree in the next variable
    pub degree: usize,
}

/// A cell of the decomposition.
#[derive(Clone, Debug)]
pub struct Cell {
    /// a section (else a sector)
    pub section: bool,
    /// the sample coordinate at this level
    pub coord: Coord,
    /// a section: the labels of its root
    pub labels: Vec<Label>,
    /// a sector: the labels of its lower bound (`None`: unbounded below)
    pub lo: Option<Vec<Label>>,
    /// a sector: the labels of its upper bound (`None`: unbounded above)
    pub hi: Option<Vec<Label>>,
    /// the truth
    pub truth: Truth,
    /// the stack above (cells of free variables that were lifted)
    pub children: Vec<Cell>,
    /// the factors of the next level over this cell
    pub info: Vec<FacInfo>,
    /// the signs of the factors of levels up to this one at the sample point (free cells whose cylinder is decided
    /// or whose level is the last free one); `UNKNOWN` for the rest
    pub signs: Vec<i8>,
}

/// The result of a decomposition.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// the problem after preprocessing
    pub problem: Problem,
    /// the projection factors
    pub basis: Basis,
    /// the atoms as products of factors
    pub atoms: Vec<Atom>,
    /// the stack over the origin of `R^0`
    pub cells: Vec<Cell>,
    /// the info of the base stack
    pub info: Vec<FacInfo>,
    /// the truth of the whole problem when it has no free variable
    pub truth: Option<bool>,
    /// the number of cells built
    pub ncells: usize,
    /// the projection operator that finished
    pub method: Method,
    /// the sample point of the first true cell over the first `nwit` variables (when asked for)
    pub witness: Option<Vec<Coord>>,
}

impl Outcome {
    /// The outcome of a sentence decided without a decomposition.
    pub fn closed(p: &Problem, truth: bool) -> Outcome {
        Outcome {
            problem: p.clone(),
            basis: Basis::new(p.nv, Method::Lazard),
            atoms: vec![],
            cells: vec![],
            info: vec![],
            truth: Some(truth),
            ncells: 0,
            method: Method::Lazard,
            witness: None,
        }
    }
}

/// The sample point of a cell in one number field.
#[derive(Clone)]
struct Point {
    k: NumberField,
    elems: Vec<Elem>,
    coords: Vec<Coord>,
    /// is the cell of positive dimension?
    positive: bool,
}

/// The preprocessed problem: Groebner basis of the equations of a top-level conjunction.
fn preprocess(p: &Problem, lim: &Limits, m: &mut Meter) -> WorkResult<Problem> {
    let mut p = p.clone();
    let mut atoms = p.atoms.clone();
    p.matrix = crate::vs::simplify(p.matrix.nnf(&mut atoms, false));
    p.atoms = atoms;
    if !lim.groebner {
        return Ok(p);
    }
    let Fm::And(parts) = &p.matrix else { return Ok(p) };
    let mut eqs = vec![];
    let mut rest = vec![];
    for f in parts {
        match f {
            Fm::Atom(i) if p.atoms[*i].rel == Rel::Eq && p.atoms[*i].den.is_constant() && !p.atoms[*i].num.is_constant() => {
                eqs.push(*i)
            }
            g => rest.push(g.clone()),
        }
    }
    if eqs.len() < 2 {
        return Ok(p);
    }
    let nv = p.nv;
    let main: Vec<usize> = (0..nv).rev().collect();
    let gs: Vec<GPoly> = eqs.iter().map(|&i| GPoly::from_poly(&p.atoms[i].num, &main, &[], MonomialOrder::DegRevLex)).collect();
    let grev = groebner(&gs, MonomialOrder::DegRevLex, m)?;
    let basis = match fglm(&grev, m)? {
        Some(b) => b,
        None => {
            let gs: Vec<GPoly> = eqs.iter().map(|&i| GPoly::from_poly(&p.atoms[i].num, &main, &[], MonomialOrder::Lex)).collect();
            groebner(&gs, MonomialOrder::Lex, m)?
        }
    };
    if basis.iter().any(|g| g.is_constant()) {
        p.matrix = Fm::Const(false);
        return Ok(p);
    }
    let mut parts: Vec<Fm> = vec![];
    for g in &basis {
        let q = g.to_poly(nv, &main, &[]);
        p.atoms.push(PAtom { num: q, den: Poly::constant(nv, mc_poly::Z::ONE), rel: Rel::Eq });
        parts.push(Fm::Atom(p.atoms.len() - 1));
    }
    parts.extend(rest);
    p.matrix = Fm::And(parts);
    Ok(p)
}

/// The atoms of `p` as products of factors of `basis`.
fn factor_atoms(p: &Problem, basis: &mut Basis, m: &mut Meter) -> WorkResult<Vec<Atom>> {
    let mut out = vec![];
    for a in &p.atoms {
        let mut atom = Atom { rel: Some(a.rel), ..Atom::default() };
        if a.num.is_zero() {
            atom.unit = 0;
        } else {
            let (u, fs) = factors(&a.num, m)?;
            let (ud, gs) = factors(&a.den, m)?;
            atom.unit = u * ud;
            atom.num = fs.into_iter().map(|(f, k)| (basis.add(f), k)).collect();
            atom.den = gs.into_iter().map(|(f, k)| (basis.add(f), k)).collect();
        }
        out.push(atom);
    }
    Ok(out)
}

/// An equational constraint: the factors of an equation of the top-level conjunction, all of the top level.
fn equational(p: &Problem, atoms: &[Atom], basis: &Basis) -> Option<Vec<usize>> {
    let mut best: Option<(u64, Vec<usize>)> = None;
    for f in p.matrix.conjuncts() {
        let Fm::Atom(i) = f else { continue };
        let a = &atoms[*i];
        if a.rel != Some(Rel::Eq) || a.unit == 0 || a.num.is_empty() || !a.den.is_empty() {
            continue;
        }
        if !a.num.iter().all(|&(g, _)| basis.lv[g] == p.nv) {
            continue;
        }
        let deg: u64 = a.num.iter().map(|&(g, _)| basis.facs[g].degree(p.nv - 1).unwrap_or(0) as u64).sum();
        if best.as_ref().is_none_or(|b| deg < b.0) {
            best = Some((deg, a.num.iter().map(|&(g, _)| g).collect()));
        }
    }
    best.map(|b| b.1)
}

/// Decompose: the cells and their truth. `progress(done, total)` sees the base stack advance.
pub fn decompose(
    p: &Problem,
    lim: &Limits,
    progress: &mut dyn FnMut(usize, usize),
    m: &mut Meter,
) -> WorkResult<Result<Outcome, CadError>> {
    decompose_with(p, lim, 0, progress, m)
}

/// A point of the first `nwit` variables of an existential problem at which it holds: `Ok(None)` when it holds
/// nowhere.
pub fn decompose_witness(p: &Problem, lim: &Limits, nwit: usize, m: &mut Meter) -> WorkResult<Result<Option<Vec<Coord>>, CadError>> {
    Ok(decompose_with(p, lim, nwit, &mut |_, _| {}, m)?.map(|o| if o.truth == Some(true) { Some(o.witness.unwrap_or_default()) } else { None }))
}

fn decompose_with(
    p: &Problem,
    lim: &Limits,
    nwit: usize,
    progress: &mut dyn FnMut(usize, usize),
    m: &mut Meter,
) -> WorkResult<Result<Outcome, CadError>> {
    let p = preprocess(p, lim, m)?;
    let mut lim = *lim;
    loop {
        match attempt(&p, &lim, lim.method, nwit, progress, m)? {
            // an equational constraint vanishing on a cell of positive dimension: without it; then the complete
            // operator in place of the smaller one (decision CAD.2)
            Err(CadError::NotWellOriented) if lim.equational => lim.equational = false,
            Err(CadError::NotWellOriented) if lim.method == Method::Brown => lim.method = Method::Lazard,
            r => return Ok(r),
        }
    }
}

fn attempt(
    p: &Problem,
    lim: &Limits,
    method: Method,
    nwit: usize,
    progress: &mut dyn FnMut(usize, usize),
    m: &mut Meter,
) -> WorkResult<Result<Outcome, CadError>> {
    let mut basis = Basis::new(p.nv, method);
    let atoms = factor_atoms(p, &mut basis, m)?;
    if lim.equational && p.nv >= 2 {
        basis.ec = equational(p, &atoms, &basis);
    }
    let lim = &Limits { equational: basis.ec.is_some(), ..*lim };
    if basis.close(lim.max_factors, m)?.is_err() {
        return Ok(Err(CadError::TooLarge));
    }
    let mut lf = Lifter { p, atoms: &atoms, basis: &basis, lim, ncells: 0, signs: vec![UNKNOWN; basis.facs.len()], nwit, witness: None, witness_rational: false, par: None };
    let pt = Point { k: rationals(), elems: vec![], coords: vec![], positive: false };
    // a matrix decided before any coordinate (a constant formula)
    let t0 = eval(&p.matrix, &atoms, &lf.signs);
    if t0 != Tri::U || p.nv == 0 {
        let b = t0 == Tri::T;
        let truth_cell = if b { Truth::True } else { Truth::False };
        let (truth, cells) = if p.nfree == 0 {
            (Some(b), vec![])
        } else {
            let whole = Cell {
                section: false,
                coord: Coord::Rat(Rat::from_i64(0)),
                labels: vec![],
                lo: None,
                hi: None,
                truth: truth_cell,
                children: vec![],
                info: vec![],
                signs: vec![UNKNOWN; basis.facs.len()],
            };
            (None, vec![whole])
        };
        return Ok(Ok(Outcome { problem: p.clone(), basis: basis.clone(), atoms, cells, info: vec![], truth, ncells: 0, method, witness: Some(vec![]) }));
    }
    // the stacks of each level in parallel when the decomposition runs under a budget (roadmap item 16A)
    let par = crate::par::current();
    let root = par.as_ref().map(|(b, _)| b.root());
    lf.par = match (&par, &root) {
        (Some((_, stop)), Some(r)) => Some(ParCx { sub: r, stop, top: true }),
        _ => None,
    };
    let r = lf.stack(0, &pt, Some(progress), m)?;
    let (cells, info, agg) = match r {
        Ok(x) => x,
        Err(e) => return Ok(Err(e)),
    };
    let truth = if p.nfree == 0 { Some(agg == Truth::True) } else { None };
    let ncells = lf.ncells;
    let witness = lf.witness.take();
    Ok(Ok(Outcome { problem: p.clone(), basis: basis.clone(), atoms: atoms.clone(), cells, info, truth, ncells, method, witness }))
}

struct Lifter<'a> {
    p: &'a Problem,
    atoms: &'a [Atom],
    basis: &'a Basis,
    lim: &'a Limits,
    ncells: usize,
    signs: Vec<i8>,
    nwit: usize,
    witness: Option<Vec<Coord>>,
    witness_rational: bool,
    /// the parallel lifting's budget element (`None`: sequential)
    par: Option<ParCx<'a>>,
}

/// What a lifter running under a budget needs: its element of the budget, the arithmetic's stop flag, and
/// whether it is the top (whose kept fuel goes to the evaluator through its meter) or a task (whose kept fuel
/// goes to its element).
#[derive(Clone, Copy)]
struct ParCx<'a> {
    sub: &'a mc_eval::Sub<'a>,
    stop: &'a std::sync::Arc<std::sync::atomic::AtomicBool>,
    top: bool,
}

/// A task's meter budget: its element of the parallel work and the arithmetic's stop flag.
struct SubWork<'a> {
    sub: &'a mc_eval::Sub<'a>,
    stop: &'a std::sync::atomic::AtomicBool,
}

impl mc_poly::work::Work for SubWork<'_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if self.stop.load(std::sync::atomic::Ordering::Relaxed) || !self.sub.charge(units) {
            return Err(mc_poly::work::Halt::Stopped);
        }
        Ok(())
    }
}

/// One cell of a stack computed by a task, with the cells its lifting built.
struct CellOut {
    r: WorkResult<Result<Cell, CadError>>,
    ncells: usize,
}

/// The cells a witness search may build looking for a rational sample once it has an irrational one.
const WITNESS_CELLS: usize = 300;

/// What the cells of one stack share: the level, the base point, the roots and samples, this level's factors at
/// the point, whether an equational constraint cuts it, and its quantifier.
struct Level<'l> {
    d: usize,
    pt: &'l Point,
    roots: &'l [Root],
    samples: &'l [Rat],
    uni: &'l [(usize, Vec<Elem>)],
    ec: bool,
    q: Option<Quant>,
}

/// A root of a stack with its labels.
struct Root {
    x: RealAlg,
    labels: Vec<Label>,
}

type StackResult = Result<(Vec<Cell>, Vec<FacInfo>, Truth), CadError>;

impl Lifter<'_> {
    /// The stack over the cell with sample point `pt` (of dimension `d = pt.elems.len()`): its cells (those
    /// built), the factor information, and the aggregated truth of the cell.
    fn stack(
        &mut self,
        d: usize,
        pt: &Point,
        mut progress: Option<&mut dyn FnMut(usize, usize)>,
        m: &mut Meter,
    ) -> WorkResult<StackResult> {
        let lev = d + 1;
        let nv = self.p.nv;
        let k = &pt.k;
        let all: Vec<usize> = self.basis.by_level[lev - 1].clone();
        let mut ec_top = (lev == nv).then(|| self.basis.ec.clone()).flatten();
        // an equational constraint vanishing identically over the cell does not cut the stack: over a point
        // every factor cuts it, over a cell of positive dimension the decomposition is redone without it
        if let Some(e) = &ec_top {
            for &f in e {
                if eval_uni(k, &self.basis.facs[f], &pt.elems, m)?.is_empty() {
                    if pt.positive {
                        return Ok(Err(CadError::NotWellOriented));
                    }
                    ec_top = None;
                    break;
                }
            }
        }
        let cutting: Vec<usize> = ec_top.clone().unwrap_or_else(|| all.clone());
        // plain values at the point (for signs) and the roots of the residues
        let mut uni: Vec<(usize, Vec<Elem>)> = vec![];
        let mut info = vec![];
        let mut roots: Vec<Root> = vec![];
        for &f in &all {
            let poly = &self.basis.facs[f];
            let cs = coeffs(poly, d);
            let lc = &cs[cs.len() - 1];
            let lcv = eval_full(k, lc, &pt.elems, m)?;
            let Some(lcs) = elem_sign(k, &lcv, m)? else { return Ok(Err(CadError::Undecided)) };
            if !cutting.contains(&f) {
                let u = eval_uni(k, poly, &pt.elems, m)?;
                uni.push((f, u));
                info.push(FacInfo { fac: f, nroots: 0, lc_sign: lcs, degree: cs.len() - 1 });
                continue;
            }
            let (res, nullified) = lazard_residue(k, poly, &pt.elems, m)?;
            if nullified && self.basis.method == Method::Brown && pt.positive {
                return Ok(Err(CadError::NotWellOriented));
            }
            uni.push((f, if nullified { vec![] } else { res.clone() }));
            let mut n = 0;
            if res.len() > 1 {
                let Some(rs) = real_roots(k, &res, m)? else { return Ok(Err(CadError::Undecided)) };
                n = rs.len();
                for (j, (x, _)) in rs.into_iter().enumerate() {
                    insert_root(&mut roots, x, (f, j + 1), m)?;
                }
            }
            info.push(FacInfo { fac: f, nroots: n, lc_sign: lcs, degree: cs.len() - 1 });
        }
        separate(&mut roots, m)?;
        let samples = gap_samples(&roots);
        let quantified = d >= self.p.nfree;
        let q = quantified.then(|| self.p.quants[d - self.p.nfree]);
        let ncells = 2 * roots.len() + 1;
        if let (Some(pc), 0) = (self.par, self.nwit) {
            let lv = Level { d, pt, roots: &roots, samples: &samples, uni: &uni, ec: ec_top.is_some(), q };
            return self.stack_par(&lv, pc, progress, info, m);
        }
        let mut cells: Vec<Cell> = vec![];
        let mut any_true = false;
        let mut any_false = false;
        for i in 0..ncells {
            if let Some(cb) = progress.as_deref_mut() {
                cb(i, ncells);
            }
            self.ncells += 1;
            if self.ncells > self.lim.max_cells {
                return Ok(Err(CadError::TooLarge));
            }
            m.step(1)?;
            let section = i % 2 == 1;
            let (coord, labels, lo, hi) = if section {
                let r = &roots[i / 2];
                (Coord::Alg(r.x.clone()), r.labels.clone(), None, None)
            } else {
                let j = i / 2;
                let lo = (j > 0).then(|| roots[j - 1].labels.clone());
                let hi = (j < roots.len()).then(|| roots[j].labels.clone());
                (Coord::Rat(samples[j].clone()), vec![], lo, hi)
            };
            // signs of this level's factors at the child's sample point
            let truth_now = if ec_top.is_some() && !section {
                Tri::F
            } else {
                for (f, u) in &uni {
                    let s = if labels.iter().any(|l| l.0 == *f) || u.is_empty() {
                        Some(0)
                    } else {
                        match &coord {
                            Coord::Rat(r) => {
                                let v = horner(k, u, &k.of(r));
                                elem_sign(k, &v, m)?
                            }
                            Coord::Alg(x) => match x.as_rat() {
                                Some(r) => {
                                    let v = horner(k, u, &k.of(&r));
                                    elem_sign(k, &v, m)?
                                }
                                None => sign_at_algebraic(k, u, x, m)?,
                            },
                        }
                    };
                    let Some(s) = s else { return Ok(Err(CadError::Undecided)) };
                    self.signs[*f] = s as i8;
                }
                eval(&self.p.matrix, self.atoms, &self.signs)
            };
            let mut cell = Cell {
                section,
                coord,
                labels,
                lo,
                hi,
                truth: Truth::Mixed,
                children: vec![],
                info: vec![],
                signs: vec![],
            };
            let free_cell = lev <= self.p.nfree;
            if free_cell {
                cell.signs = self.signs.clone();
            }
            match truth_now {
                Tri::T | Tri::F => {
                    cell.truth = if truth_now == Tri::T { Truth::True } else { Truth::False };
                }
                Tri::U => {
                    // lift: the child's point
                    let child = match self.child_point(pt, &cell, m)? {
                        Some(c) => c,
                        None => return Ok(Err(CadError::Undecided)),
                    };
                    let r = self.stack(d + 1, &child, None, m)?;
                    let (ch, inf, agg) = match r {
                        Ok(x) => x,
                        Err(e) => return Ok(Err(e)),
                    };
                    cell.truth = agg;
                    cell.info = inf;
                    if free_cell && lev < self.p.nfree {
                        cell.children = ch;
                    }
                }
            }
            // a witness: a cell of the last witness level, or one decided true at once (its whole cylinder is true)
            if cell.truth == Truth::True && (lev == self.nwit || (lev < self.nwit && truth_now == Tri::T)) && !self.witness_rational {
                let mut w = pt.coords.clone();
                w.push(cell.coord.clone());
                let rational = w.iter().all(|c| c.as_rat().is_some());
                if self.witness.is_none() || rational {
                    self.witness = Some(w);
                    self.witness_rational = rational;
                }
            }
            // forget this level's signs before the next child
            for (f, _) in &uni {
                self.signs[*f] = UNKNOWN;
            }
            match cell.truth {
                Truth::True => any_true = true,
                Truth::False => any_false = true,
                Truth::Mixed => {
                    any_true = true;
                    any_false = true;
                }
            }
            // a witness search goes on past a true cell with an irrational sample, for a rational one (decision
            // CAD.11)
            let stop = match q {
                Some(Quant::Exists) => {
                    cell.truth == Truth::True && (lev > self.nwit || self.witness_rational || self.ncells > WITNESS_CELLS)
                }
                Some(Quant::ForAll) => cell.truth == Truth::False,
                None => false,
            };
            if !quantified {
                cells.push(cell);
            }
            if stop {
                break;
            }
        }
        if let Some(cb) = progress {
            cb(ncells, ncells);
        }
        let agg = match q {
            Some(Quant::Exists) => {
                if any_true {
                    Truth::True
                } else {
                    Truth::False
                }
            }
            Some(Quant::ForAll) => {
                if any_false {
                    Truth::False
                } else {
                    Truth::True
                }
            }
            None => match (any_true, any_false) {
                (true, false) => Truth::True,
                (false, true) => Truth::False,
                _ => Truth::Mixed,
            },
        };
        Ok(Ok((cells, info, agg)))
    }

    /// [`Lifter::stack`]'s cells under a budget (decision n16a.3): every cell of the stack (its signs, its truth,
    /// its lifting) is an element of a parallel run on its own lifter and meter, ending at the first cell that
    /// decides a quantified stack or fails; this lifter then takes the cells in order, charging their fuel and
    /// counting their cells as the sequential loop does, and computes again itself a cell whose task was stopped.
    /// Only without a witness search (whose choices depend on the order of the whole traversal).
    fn stack_par(
        &mut self,
        lv: &Level<'_>,
        pc: ParCx<'_>,
        mut progress: Option<&mut dyn FnMut(usize, usize)>,
        info: Vec<FacInfo>,
        m: &mut Meter,
    ) -> WorkResult<StackResult> {
        let ncells = 2 * lv.roots.len() + 1;
        let decides = |t: Truth| match lv.q {
            Some(Quant::Exists) => t == Truth::True,
            Some(Quant::ForAll) => t == Truth::False,
            None => false,
        };
        let outs = {
            let base = &*self;
            pc.sub.until(ncells, &|o: &CellOut| !matches!(&o.r, Ok(Ok(c)) if !decides(c.truth)), &|i, sub| {
                let _scope = mc_arith::num::StopScope::enter(Some(pc.stop));
                let mut w = SubWork { sub, stop: pc.stop };
                let mut mm = Meter::new(&mut w);
                let mut lf = Lifter {
                    p: base.p,
                    atoms: base.atoms,
                    basis: base.basis,
                    lim: base.lim,
                    ncells: 0,
                    signs: base.signs.clone(),
                    nwit: 0,
                    witness: None,
                    witness_rational: false,
                    par: Some(ParCx { sub, stop: pc.stop, top: false }),
                };
                let r = lf.one_cell(lv, i, &mut mm);
                CellOut { r, ncells: lf.ncells }
            })
        };
        let quantified = lv.d >= self.p.nfree;
        let mut cells: Vec<Cell> = vec![];
        let (mut any_true, mut any_false) = (false, false);
        for (i, (out, fuel)) in outs.into_iter().enumerate() {
            if let Some(cb) = progress.as_deref_mut() {
                cb(i, ncells);
            }
            self.ncells += 1;
            if self.ncells > self.lim.max_cells {
                return Ok(Err(CadError::TooLarge));
            }
            // the task's fuel, kept: the evaluator's (the top) or the enclosing task's
            if pc.top {
                m.work().charge(fuel)?;
            } else {
                pc.sub.settle(fuel);
            }
            let cell = match out.r {
                // stopped (an interrupt, a limit, the fuel cap of the budget, a cancellation): again, here
                Err(_) => {
                    if pc.stop.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err(mc_poly::work::Halt::Stopped);
                    }
                    match self.one_cell(lv, i, m)? {
                        Ok(c) => c,
                        Err(e) => return Ok(Err(e)),
                    }
                }
                Ok(r) => {
                    // the cells the task built, counted as the sequential traversal counts them
                    if self.ncells + out.ncells > self.lim.max_cells {
                        return Ok(Err(CadError::TooLarge));
                    }
                    self.ncells += out.ncells;
                    match r {
                        Ok(c) => c,
                        Err(e) => return Ok(Err(e)),
                    }
                }
            };
            match cell.truth {
                Truth::True => any_true = true,
                Truth::False => any_false = true,
                Truth::Mixed => {
                    any_true = true;
                    any_false = true;
                }
            }
            let stop = decides(cell.truth);
            if !quantified {
                cells.push(cell);
            }
            if stop {
                break;
            }
        }
        if let Some(cb) = progress {
            cb(ncells, ncells);
        }
        let agg = match lv.q {
            Some(Quant::Exists) => {
                if any_true {
                    Truth::True
                } else {
                    Truth::False
                }
            }
            Some(Quant::ForAll) => {
                if any_false {
                    Truth::False
                } else {
                    Truth::True
                }
            }
            None => match (any_true, any_false) {
                (true, false) => Truth::True,
                (false, true) => Truth::False,
                _ => Truth::Mixed,
            },
        };
        Ok(Ok((cells, info, agg)))
    }

    /// Cell `i` of the stack of level `lv`: its signs and truth, and its lifting when the level does not decide it
    /// (the cells above counted in `self.ncells`); this level's signs are reset afterwards.
    fn one_cell(&mut self, lv: &Level<'_>, i: usize, m: &mut Meter) -> WorkResult<Result<Cell, CadError>> {
        let lev = lv.d + 1;
        let k = &lv.pt.k;
        m.step(1)?;
        let section = i % 2 == 1;
        let (coord, labels, lo, hi) = if section {
            let r = &lv.roots[i / 2];
            (Coord::Alg(r.x.clone()), r.labels.clone(), None, None)
        } else {
            let j = i / 2;
            let lo = (j > 0).then(|| lv.roots[j - 1].labels.clone());
            let hi = (j < lv.roots.len()).then(|| lv.roots[j].labels.clone());
            (Coord::Rat(lv.samples[j].clone()), vec![], lo, hi)
        };
        let r = self.one_cell_at(lv, k, lev, section, coord, labels, lo, hi, m);
        for (f, _) in lv.uni {
            self.signs[*f] = UNKNOWN;
        }
        r
    }

    #[allow(clippy::too_many_arguments)]
    fn one_cell_at(
        &mut self,
        lv: &Level<'_>,
        k: &NumberField,
        lev: usize,
        section: bool,
        coord: Coord,
        labels: Vec<Label>,
        lo: Option<Vec<Label>>,
        hi: Option<Vec<Label>>,
        m: &mut Meter,
    ) -> WorkResult<Result<Cell, CadError>> {
        let truth_now = if lv.ec && !section {
            Tri::F
        } else {
            for (f, u) in lv.uni {
                let s = if labels.iter().any(|l| l.0 == *f) || u.is_empty() {
                    Some(0)
                } else {
                    match &coord {
                        Coord::Rat(r) => elem_sign(k, &horner(k, u, &k.of(r)), m)?,
                        Coord::Alg(x) => match x.as_rat() {
                            Some(r) => elem_sign(k, &horner(k, u, &k.of(&r)), m)?,
                            None => sign_at_algebraic(k, u, x, m)?,
                        },
                    }
                };
                let Some(s) = s else { return Ok(Err(CadError::Undecided)) };
                self.signs[*f] = s as i8;
            }
            eval(&self.p.matrix, self.atoms, &self.signs)
        };
        let mut cell = Cell { section, coord, labels, lo, hi, truth: Truth::Mixed, children: vec![], info: vec![], signs: vec![] };
        let free_cell = lev <= self.p.nfree;
        if free_cell {
            cell.signs = self.signs.clone();
        }
        match truth_now {
            Tri::T | Tri::F => {
                cell.truth = if truth_now == Tri::T { Truth::True } else { Truth::False };
                Ok(Ok(cell))
            }
            Tri::U => {
                let Some(child) = self.child_point(lv.pt, &cell, m)? else { return Ok(Err(CadError::Undecided)) };
                Ok(match self.stack(lv.d + 1, &child, None, m)? {
                    Ok((ch, inf, agg)) => {
                        cell.truth = agg;
                        cell.info = inf;
                        if free_cell && lev < self.p.nfree {
                            cell.children = ch;
                        }
                        Ok(cell)
                    }
                    Err(e) => Err(e),
                })
            }
        }
    }

    /// The sample point of a child cell, in one number field.
    fn child_point(&self, pt: &Point, cell: &Cell, m: &mut Meter) -> WorkResult<Option<Point>> {
        let mut coords = pt.coords.clone();
        coords.push(cell.coord.clone());
        if let Some(r) = cell.coord.as_rat() {
            let mut elems = pt.elems.clone();
            elems.push(pt.k.of(&r));
            return Ok(Some(Point { k: pt.k.clone(), elems, coords, positive: pt.positive || !cell.section }));
        }
        let Coord::Alg(x) = &cell.coord else { return Ok(None) };
        let Some(kb) = x.field(m)? else { return Ok(None) };
        let Some(tw) = adjoin(&pt.k, &kb, m)? else { return Ok(None) };
        let mut elems: Vec<Elem> = pt.elems.iter().map(|e| map_elem(&tw.field, e, &tw.theta)).collect();
        elems.push(tw.beta.clone());
        Ok(Some(Point { k: tw.field, elems, coords, positive: pt.positive }))
    }
}

/// The value of a dense polynomial over `k` at `x`.
fn horner(k: &NumberField, u: &[Elem], x: &Elem) -> Elem {
    let mut v = k.zero();
    for c in u.iter().rev() {
        v = k.add(&k.mul(&v, x), c);
    }
    v
}

/// The exact sign of an element of a real field.
fn elem_sign(k: &NumberField, a: &Elem, m: &mut Meter) -> WorkResult<Option<i32>> {
    if a.is_empty() {
        return Ok(Some(0));
    }
    if let Some(q) = k.as_rat(a) {
        return Ok(Some(rsign(&q)));
    }
    AlgNum { k: k.clone(), a: a.clone() }.sign(m)
}

/// Insert a root into the increasing list, merging equal ones.
fn insert_root(roots: &mut Vec<Root>, x: RealAlg, label: Label, m: &mut Meter) -> WorkResult<()> {
    let mut x = x;
    let mut pos = roots.len();
    for (i, r) in roots.iter_mut().enumerate() {
        match x.compare(&mut r.x, m)? {
            Ordering::Less => {
                pos = i;
                break;
            }
            Ordering::Equal => {
                r.labels.push(label);
                return Ok(());
            }
            Ordering::Greater => {}
        }
    }
    roots.insert(pos, Root { x, labels: vec![label] });
    Ok(())
}

/// Refine neighbouring isolating intervals until they are disjoint.
fn separate(roots: &mut [Root], m: &mut Meter) -> WorkResult<()> {
    for i in 1..roots.len() {
        let mut n = 0;
        while roots[i - 1].x.iv.hi.cmp_value(&roots[i].x.iv.lo) != Ordering::Less {
            m.step(roots[i].x.poly.len() as u64)?;
            let (a, b) = roots.split_at_mut(i);
            a[i - 1].x.halve();
            b[0].x.halve();
            n += 1;
            if n > 100_000 {
                // invariant: distinct real algebraic numbers separate after finitely many halvings
                return Err(mc_poly::work::Halt::Stopped);
            }
        }
    }
    Ok(())
}

/// Rational samples of the gaps: below the first root, between neighbours, above the last (simplest numbers).
fn gap_samples(roots: &[Root]) -> Vec<Rat> {
    let zero = Rat::from_i64(0);
    let one = Int::from_i64(1);
    if roots.is_empty() {
        return vec![zero];
    }
    let mut out = vec![];
    // below
    let r0 = &roots[0].x.iv;
    let exact0 = r0.lo == r0.hi;
    let below = if r0.lo.cmp_value(&zero) == Ordering::Greater || (!exact0 && r0.lo.is_zero()) {
        zero.clone()
    } else {
        let (f, _) = r0.lo.floor_fract();
        Rat::from_int(f.sub(&one))
    };
    out.push(below);
    for i in 1..roots.len() {
        let (a, b) = (&roots[i - 1].x.iv, &roots[i].x.iv);
        let s = simplest_between(&a.hi, &b.lo);
        let bad = (a.lo == a.hi && s == a.hi) || (b.lo == b.hi && s == b.lo);
        out.push(if bad { a.hi.add(&b.lo).mul(&Rat::new(Int::from_i64(1), Int::from_i64(2))) } else { s });
    }
    let rn = &roots[roots.len() - 1].x.iv;
    let exactn = rn.lo == rn.hi;
    let above = if rn.hi.cmp_value(&zero) == Ordering::Less || (!exactn && rn.hi.is_zero()) {
        zero
    } else {
        let (f, _) = rn.hi.floor_fract();
        Rat::from_int(f.add(&one))
    };
    out.push(above);
    out
}

