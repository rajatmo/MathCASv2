//! The topology of a semialgebraic set in the line or the plane from a full cylindrical decomposition: closure,
//! interior, exterior, boundary and connected components (`cylindrical_decomposition(cond, vars, mode)`).
//!
//! **Cell adjacency in the plane** (Arnon, Collins and McCallum 1984, 1988; Basu, Pollack and Roy, *Algorithms in
//! Real Algebraic Geometry*, 5.2 and 11.2). Every cell over every base cell is lifted, so the decomposition is
//! truth invariant cell by cell. A section `y = s(x)` over a base sector `(a, b)` is a root of a factor `f`; it is
//! continuous and semialgebraic, so its limit at `a` exists in `R ∪ {±∞}`, and a finite limit is a root of `f(a, y)`
//! (when `f(a, y)` is not identically zero). The limit is found exactly: the rational samples `s_0 < ... < s_k` of
//! the sectors of the stack over `a` separate the roots of every factor there; a rational `x0` in `(a, b)` is
//! taken closer to `a` than every root of `f(x, s_i)` (none is `a`, since `f(a, s_i) != 0`), so no branch of `f`
//! crosses a separator between `a` and `x0`; the branch's root of `f(x0, y)` lies between `s_(i-1)` and `s_i` exactly
//! when its limit is the section of the stack over `a` between them (below `s_0`: `-∞`, above `s_k`: `+∞`). A sector
//! of the plane between two sections meets the line `x = a` in the closed interval between their limits, which is a
//! union of cells of the stack over `a`. So the closure of every cell is a union of cells, found exactly; no
//! numerical approximation enters a decision.
//!
//! From the closures: the closure of the set is the union of the closures of its true cells; the interior the
//! complement of the closure of the complement; the exterior the complement of the closure; the boundary the
//! closure less the interior; two true cells lie in one component when one meets the closure of the other.
//! A factor vanishing identically over a base section (no limit argument), parameters, or more than two free
//! variables decline.
use crate::cad::{decompose, Cell, Coord, Limits, Outcome, Truth};
use crate::front::Front;
use crate::natives::{check, metered};
use crate::output::Writer;
use crate::poly::zrat;
use mc_algebra::realroot::{rational_real_roots, RealAlg};
use mc_arith::num::Rat;
use mc_eval::{NativeCx, Res};
use mc_api::T;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// What `cylindrical_decomposition` returns in place of the set.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// the closure
    Closure,
    /// the interior
    Interior,
    /// the exterior (the interior of the complement)
    Exterior,
    /// the boundary
    Boundary,
    /// the connected components, as a list
    Components,
}

impl Mode {
    /// The mode named by the string argument.
    pub fn named(s: &str) -> Option<Mode> {
        Some(match s {
            "Closure" => Mode::Closure,
            "Interior" => Mode::Interior,
            "Exterior" => Mode::Exterior,
            "Boundary" => Mode::Boundary,
            "Components" => Mode::Components,
            _ => return None,
        })
    }
}

/// The most cells of the decomposition.
const MAX_CELLS: usize = 20_000;
/// The most cells whose closures are computed.
const MAX_LEAVES: usize = 4000;

/// The limit of a section over a base sector at one end of the sector.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lim {
    NegInf,
    PosInf,
    /// the section of this index in the stack over the end
    At(usize),
}

/// A leaf cell: its base cell and its index in the stack over it (0 in the line).
type Leaf = (usize, usize);

/// The formulas written, the free variables, and the outcomes they were written from.
type Written<'h> = (Vec<T<'h>>, Vec<T<'h>>, Vec<Outcome>);

/// `cylindrical_decomposition(cond, vars, mode)`: the set in `mode`, or `None` when it is not decided.
pub fn modes<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, cond: T<'h>, vars: &[T<'h>], mode: Mode) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let nvars = vars.len();
    if !(1..=2).contains(&nvars) {
        return Ok(None);
    }
    let r = metered(cx, stop, |m| -> WorkResult<Option<Written<'h>>> {
        let fr = Front::new(h);
        let Ok(mut p) = fr.problem(cond, vars, m)? else { return Ok(None) };
        if p.prob.nfree != nvars {
            // parameters
            return Ok(None);
        }
        if let Some(q) = crate::vs::eliminate(&p.prob, m)? {
            p.vars.truncate(q.nv);
            p.prob = q;
        }
        let lim = Limits { full: true, equational: false, max_cells: MAX_CELLS, ..Limits::default() };
        let Ok(o) = decompose(&p.prob, &lim, &mut |_, _| {}, m)? else { return Ok(None) };
        let Some(sets) = topology(&o, nvars, mode, m)? else { return Ok(None) };
        let w = Writer::new(h, &p.vars, m)?;
        let mut terms = vec![];
        let mut outs = vec![];
        for s in sets {
            let oc = with_truth(&o, nvars, &s);
            terms.push(w.cylindrical(&oc, m)?);
            outs.push(oc);
        }
        Ok(Some((terms, p.vars[..nvars].to_vec(), outs)))
    })?;
    let Some(Some((terms, fvars, outs))) = r else { return Ok(None) };
    let mut done = vec![];
    for (t, o) in terms.into_iter().zip(&outs) {
        let t = cx.eval(t)?;
        if !check(cx, t, &fvars, o)? {
            return Ok(None);
        }
        done.push(t);
    }
    Ok(Some(if mode == Mode::Components { cx.list(&done) } else { done[0] }))
}

/// The truth of every leaf cell in the asked sets (one set, or one per component).
fn topology(o: &Outcome, nvars: usize, mode: Mode, m: &mut Meter) -> WorkResult<Option<Vec<Vec<Vec<bool>>>>> {
    if o.truth.is_some() {
        return Ok(None);
    }
    // the leaves and their truth
    let mut truth: Vec<Vec<bool>> = vec![];
    let trivial = o.cells.len() == 1 && (nvars == 1 || o.cells[0].children.is_empty());
    for c in &o.cells {
        if nvars == 1 || trivial {
            match c.truth {
                Truth::True => truth.push(vec![true]),
                Truth::False => truth.push(vec![false]),
                Truth::Mixed => return Ok(None),
            }
        } else {
            if c.children.is_empty() {
                return Ok(None);
            }
            let mut row = vec![];
            for d in &c.children {
                match d.truth {
                    Truth::True => row.push(true),
                    Truth::False => row.push(false),
                    Truth::Mixed => return Ok(None),
                }
            }
            truth.push(row);
        }
    }
    let nleaves: usize = truth.iter().map(Vec::len).sum();
    if nleaves > MAX_LEAVES {
        return Ok(None);
    }
    // the closure of every leaf
    let mut closure: Vec<Vec<Vec<Leaf>>> = truth.iter().map(|r| vec![vec![]; r.len()]).collect();
    if trivial {
        closure[0][0].push((0, 0));
    } else if nvars == 1 {
        for (i, c) in o.cells.iter().enumerate() {
            m.step(1)?;
            let cl = &mut closure[i][0];
            cl.push((i, 0));
            if !c.section {
                if i > 0 {
                    cl.push((i - 1, 0));
                }
                if i + 1 < o.cells.len() {
                    cl.push((i + 1, 0));
                }
            }
        }
    } else {
        for (i, c) in o.cells.iter().enumerate() {
            let n = c.children.len();
            // the limits of the sections over a base sector at its ends
            let mut lims: Vec<[Option<Lim>; 2]> = vec![[None, None]; n];
            if !c.section {
                for (side, nb) in [(0usize, i.checked_sub(1)), (1, (i + 1 < o.cells.len()).then_some(i + 1))] {
                    let Some(nb) = nb else { continue };
                    for (j, d) in c.children.iter().enumerate() {
                        if d.section {
                            let Some(l) = limit(o, c, d, &o.cells[nb], side == 0, m)? else { return Ok(None) };
                            lims[j][side] = Some(l);
                        }
                    }
                }
            }
            for (j, d) in c.children.iter().enumerate() {
                m.step(1)?;
                let cl = &mut closure[i][j];
                cl.push((i, j));
                if !d.section {
                    if j > 0 {
                        cl.push((i, j - 1));
                    }
                    if j + 1 < n {
                        cl.push((i, j + 1));
                    }
                }
                if c.section {
                    continue;
                }
                for (side, nb) in [(0usize, i.checked_sub(1)), (1, (i + 1 < o.cells.len()).then_some(i + 1))] {
                    let Some(nb) = nb else { continue };
                    let len = o.cells[nb].children.len();
                    let (lo, hi) = if d.section {
                        (lims[j][side], lims[j][side])
                    } else {
                        let lo = if j == 0 { Some(Lim::NegInf) } else { lims[j - 1][side] };
                        let hi = if j + 1 == n { Some(Lim::PosInf) } else { lims[j + 1][side] };
                        (lo, hi)
                    };
                    let (Some(lo), Some(hi)) = (lo, hi) else { return Ok(None) };
                    // the cells of the stack over the end between the limits, both included
                    let a = match lo {
                        Lim::NegInf => 0,
                        Lim::At(k) => k,
                        Lim::PosInf => continue,
                    };
                    let b = match hi {
                        Lim::PosInf => len - 1,
                        Lim::At(k) => k,
                        Lim::NegInf => continue,
                    };
                    for k in a..=b.min(len - 1) {
                        cl.push((nb, k));
                    }
                }
            }
        }
    }
    let close = |pick: &dyn Fn(bool) -> bool| -> Vec<Vec<bool>> {
        let mut out: Vec<Vec<bool>> = truth.iter().map(|r| vec![false; r.len()]).collect();
        for (i, r) in truth.iter().enumerate() {
            for (j, &t) in r.iter().enumerate() {
                if pick(t) {
                    for &(a, b) in &closure[i][j] {
                        out[a][b] = true;
                    }
                }
            }
        }
        out
    };
    let map2 = |a: &Vec<Vec<bool>>, b: &Vec<Vec<bool>>, f: &dyn Fn(bool, bool) -> bool| -> Vec<Vec<bool>> {
        a.iter().zip(b).map(|(x, y)| x.iter().zip(y).map(|(&p, &q)| f(p, q)).collect()).collect()
    };
    Ok(Some(match mode {
        Mode::Closure => vec![close(&|t| t)],
        Mode::Exterior => vec![close(&|t| t).iter().map(|r| r.iter().map(|&b| !b).collect()).collect()],
        Mode::Interior => vec![close(&|t| !t).iter().map(|r| r.iter().map(|&b| !b).collect()).collect()],
        Mode::Boundary => vec![map2(&close(&|t| t), &close(&|t| !t), &|p, q| p && q)],
        Mode::Components => {
            // union-find over the true leaves
            let idx: Vec<usize> = truth.iter().scan(0, |acc, r| {
                let s = *acc;
                *acc += r.len();
                Some(s)
            }).collect();
            let mut parent: Vec<usize> = (0..nleaves).collect();
            fn find(p: &mut [usize], x: usize) -> usize {
                let mut x = x;
                while p[x] != x {
                    p[x] = p[p[x]];
                    x = p[x];
                }
                x
            }
            for (i, r) in truth.iter().enumerate() {
                for (j, &t) in r.iter().enumerate() {
                    if !t {
                        continue;
                    }
                    for &(a, b) in &closure[i][j] {
                        if truth[a][b] {
                            let (x, y) = (find(&mut parent, idx[i] + j), find(&mut parent, idx[a] + b));
                            parent[x.max(y)] = x.min(y);
                        }
                    }
                }
            }
            // components in the order of their first cell
            let mut roots: Vec<usize> = vec![];
            let mut sets: Vec<Vec<Vec<bool>>> = vec![];
            for (i, r) in truth.iter().enumerate() {
                for (j, &t) in r.iter().enumerate() {
                    if !t {
                        continue;
                    }
                    let rt = find(&mut parent, idx[i] + j);
                    let k = match roots.iter().position(|&x| x == rt) {
                        Some(k) => k,
                        None => {
                            roots.push(rt);
                            sets.push(truth.iter().map(|r| vec![false; r.len()]).collect());
                            roots.len() - 1
                        }
                    };
                    sets[k][i][j] = true;
                }
            }
            sets
        }
    }))
}

/// The outcome with the leaves' truth replaced (the base cells' truth aggregated).
fn with_truth(o: &Outcome, nvars: usize, t: &[Vec<bool>]) -> Outcome {
    let mut o = o.clone();
    let tv = |b: bool| if b { Truth::True } else { Truth::False };
    let trivial = o.cells.len() == 1 && (nvars == 1 || o.cells[0].children.is_empty());
    for (i, c) in o.cells.iter_mut().enumerate() {
        if nvars == 1 || trivial {
            c.truth = tv(t[i][0]);
            c.children.clear();
            continue;
        }
        for (j, d) in c.children.iter_mut().enumerate() {
            d.truth = tv(t[i][j]);
        }
        c.truth = if t[i].iter().all(|&b| b) {
            Truth::True
        } else if t[i].iter().all(|&b| !b) {
            Truth::False
        } else {
            Truth::Mixed
        };
    }
    o
}

/// `p(x, s)` as a dense polynomial in `x` (the variables above the second do not occur).
fn at_y(p: &Poly, s: &Rat) -> Vec<Rat> {
    let mut out: Vec<Rat> = vec![];
    for (e, c) in p.terms() {
        let i = e.first().copied().unwrap_or(0) as usize;
        let k = e.get(1).copied().unwrap_or(0);
        let mut v = zrat(c);
        for _ in 0..k {
            v = v.mul(s);
        }
        if out.len() <= i {
            out.resize(i + 1, Rat::from_i64(0));
        }
        out[i] = out[i].add(&v);
    }
    trim(out)
}

/// `p(x0, y)` as a dense polynomial in `y`.
fn at_x(p: &Poly, x0: &Rat) -> Vec<Rat> {
    let mut out: Vec<Rat> = vec![];
    for (e, c) in p.terms() {
        let k = e.first().copied().unwrap_or(0);
        let i = e.get(1).copied().unwrap_or(0) as usize;
        let mut v = zrat(c);
        for _ in 0..k {
            v = v.mul(x0);
        }
        if out.len() <= i {
            out.resize(i + 1, Rat::from_i64(0));
        }
        out[i] = out[i].add(&v);
    }
    trim(out)
}

fn trim(mut a: Vec<Rat>) -> Vec<Rat> {
    while a.last().is_some_and(Rat::is_zero) {
        a.pop();
    }
    a
}

fn real_alg(c: &Coord) -> RealAlg {
    match c {
        Coord::Rat(q) => RealAlg::rational(q),
        Coord::Alg(a) => a.clone(),
    }
}

/// The limit of the section `d` over the base sector `c` at its end `nb` (the left end when `left`).
fn limit(o: &Outcome, c: &Cell, d: &Cell, nb: &Cell, left: bool, m: &mut Meter) -> WorkResult<Option<Lim>> {
    let Some(xs) = c.coord.as_rat() else { return Ok(None) };
    let mut a = real_alg(&nb.coord);
    // the separators: the sector samples of the stack over the end
    let mut seps: Vec<Rat> = vec![];
    for (k, e) in nb.children.iter().enumerate() {
        if e.section != (k % 2 == 1) {
            return Ok(None);
        }
        if !e.section {
            let Some(q) = e.coord.as_rat() else { return Ok(None) };
            seps.push(q);
        }
    }
    if seps.len() * 2 != nb.children.len() + 1 {
        return Ok(None);
    }
    // a factor whose root the section is
    let Some(&(f, idx)) = d.labels.first() else { return Ok(None) };
    let fac = &o.basis.facs[f];
    if fac.used_vars().into_iter().any(|v| v > 1) {
        return Ok(None);
    }
    let Some(fi) = c.info.iter().find(|i| i.fac == f) else { return Ok(None) };
    // x0 between the end and every root of f(x, s_i) on the sector's side, and not past the sector's sample
    let mut near: Option<RealAlg> = None;
    let mut xr = RealAlg::rational(&xs);
    for s in &seps {
        let g = at_y(fac, s);
        if g.is_empty() {
            return Ok(None);
        }
        if g.len() == 1 {
            continue;
        }
        for (mut r, _) in rational_real_roots(&g, m)? {
            let ca = r.compare(&mut a, m)?;
            if ca == Ordering::Equal {
                // f(a, s) = 0: f vanishes identically over the end
                return Ok(None);
            }
            let cs = r.compare(&mut xr, m)?;
            let inside = if left { ca == Ordering::Greater && cs != Ordering::Greater } else { ca == Ordering::Less && cs != Ordering::Less };
            if !inside {
                continue;
            }
            let better = match &mut near {
                None => true,
                Some(n) => {
                    let o = r.compare(n, m)?;
                    if left {
                        o == Ordering::Less
                    } else {
                        o == Ordering::Greater
                    }
                }
            };
            if better {
                near = Some(r);
            }
        }
    }
    let x0 = match near {
        None => xs,
        Some(mut n) => {
            // a rational strictly between the end and the nearest root
            let (lo, hi) = if left { (&mut a, &mut n) } else { (&mut n, &mut a) };
            let mut k = 0;
            while lo.iv.hi.cmp_value(&hi.iv.lo) != Ordering::Less {
                m.step(lo.poly.len() as u64 + hi.poly.len() as u64)?;
                lo.halve();
                hi.halve();
                k += 1;
                if k > 100_000 {
                    return Ok(None);
                }
            }
            lo.iv.hi.add(&hi.iv.lo).mul(&Rat::new(mc_arith::num::Int::from_i64(1), mc_arith::num::Int::from_i64(2)))
        }
    };
    let hy = at_x(fac, &x0);
    let roots = rational_real_roots(&hy, m)?;
    if roots.len() != fi.nroots || idx == 0 || idx > roots.len() {
        return Ok(None);
    }
    let mut y = roots[idx - 1].0.clone();
    let mut below = 0;
    for s in &seps {
        match y.compare(&mut RealAlg::rational(s), m)? {
            Ordering::Greater => below += 1,
            Ordering::Equal => return Ok(None),
            Ordering::Less => {}
        }
    }
    Ok(Some(if below == 0 {
        Lim::NegInf
    } else if below == seps.len() {
        Lim::PosInf
    } else {
        let k = 2 * below - 1;
        if !nb.children[k].labels.iter().any(|l| l.0 == f) {
            return Ok(None);
        }
        Lim::At(k)
    }))
}
