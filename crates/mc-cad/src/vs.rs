//! Linear virtual substitution (Loos and Weispfenning 1993; Weispfenning 1988): the fast path before the
//! decomposition for quantified variables that occur at most linearly.
//!
//! For `exists x: phi` with `phi` in negation normal form and every atom `c x + d rel 0` (`c`, `d` polynomials in the
//! other variables), `phi` holds for some real `x` iff it holds at one of the test points `-inf`, `-d/c` (atoms
//! with `=`, `<=`, `>=`) and `-d/c + eps` (atoms with `!=`, `<`, `>`), each guarded by `c != 0`. The test points are
//! substituted virtually, so the result is again a polynomial formula, in one variable fewer:
//! `c' x + d'` at `-d/c` has the sign of `(c d' - c' d) c`; at `t + eps` it is decided by its value at `t` and then
//! by the sign of `c'`; at `-inf` by the sign of `c'` and then of `d'`. `for_all x: phi` is `not exists x: not phi`.
//! Variables are eliminated innermost first while they are linear and the formula stays small ([`MAX_ATOMS`]);
//! the rest goes to the decomposition, which then has fewer variables (decision CAD.8).
use crate::cad::Problem;
use crate::formula::{Fm, PAtom, Quant, Rel};
use crate::poly::coeffs;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};

/// The most distinct atoms a substitution may produce.
pub const MAX_ATOMS: usize = 400;

/// The problem with its innermost linear quantified variables eliminated; `None` when none is.
pub fn eliminate(p: &Problem, m: &mut Meter) -> WorkResult<Option<Problem>> {
    let mut cur = p.clone();
    let mut changed = false;
    while cur.nv > cur.nfree {
        let x = cur.nv - 1;
        if !cur.atoms.iter().all(|a| a.den.is_constant() && a.num.degree(x).unwrap_or(0) <= 1) {
            break;
        }
        let q = cur.quants[cur.nv - cur.nfree - 1];
        let Some(next) = step(&cur, x, q, m)? else { break };
        cur = next;
        changed = true;
    }
    Ok(changed.then_some(cur))
}

struct Sub {
    nv: usize,
    atoms: Vec<PAtom>,
}

impl Sub {
    fn atom(&mut self, num: Poly, rel: Rel) -> Fm {
        if let Some(c) = num.as_constant() {
            return Fm::Const(rel.holds(c.signum()));
        }
        // a constant sign factor out (primitive part, positive leading coefficient)
        let (cont, prim) = num.primitive();
        let (num, rel) = if cont.is_neg() { (prim, rel.flip()) } else { (prim, rel) };
        let (num, rel) = if num.lc().is_neg() { (num.neg(), rel.flip()) } else { (num, rel) };
        let den = Poly::constant(self.nv, Z::ONE);
        if let Some(i) = self.atoms.iter().position(|a| a.rel == rel && a.num == num) {
            return Fm::Atom(i);
        }
        self.atoms.push(PAtom { num, den, rel });
        Fm::Atom(self.atoms.len() - 1)
    }
}

/// Negation normal form: negations pushed into the relations.
fn nnf(f: &Fm, atoms: &[PAtom], neg: bool, out: &mut Vec<PAtom>) -> Fm {
    match f {
        Fm::Const(b) => Fm::Const(*b != neg),
        Fm::Atom(i) => {
            let a = &atoms[*i];
            let rel = if neg { a.rel.negate() } else { a.rel };
            out.push(PAtom { num: a.num.clone(), den: a.den.clone(), rel });
            Fm::Atom(out.len() - 1)
        }
        Fm::Not(g) => nnf(g, atoms, !neg, out),
        Fm::And(v) => {
            let w = v.iter().map(|g| nnf(g, atoms, neg, out)).collect();
            if neg {
                Fm::Or(w)
            } else {
                Fm::And(w)
            }
        }
        Fm::Or(v) => {
            let w = v.iter().map(|g| nnf(g, atoms, neg, out)).collect();
            if neg {
                Fm::And(w)
            } else {
                Fm::Or(w)
            }
        }
    }
}

/// Drop the last variable from a polynomial that does not contain it.
fn drop_last(p: &Poly, nv: usize) -> Poly {
    Poly::from_terms(nv - 1, p.terms().map(|(e, c)| (e[..nv - 1].to_vec(), c.clone())).collect())
}

/// The test point of a substitution.
#[derive(Clone)]
enum Point {
    MinusInf,
    /// `-d/c`, optionally `+ eps`
    At { c: Poly, d: Poly, eps: bool },
}

/// One elimination step of the variable `x` (the last).
fn step(p: &Problem, x: usize, q: Quant, m: &mut Meter) -> WorkResult<Option<Problem>> {
    let nv = p.nv;
    // exists x phi, phi in negation normal form (for_all: of not phi)
    let mut atoms = vec![];
    let f = nnf(&p.matrix, &p.atoms, q == Quant::ForAll, &mut atoms);
    // the linear form of each atom: (c, d)
    let lin: Vec<(Poly, Poly)> = atoms
        .iter()
        .map(|a| {
            let cs = coeffs(&a.num, x);
            let c = if cs.len() > 1 { cs[1].clone() } else { Poly::zero(nv) };
            (c, cs[0].clone())
        })
        .collect();
    let mut points = vec![Point::MinusInf];
    for (i, a) in atoms.iter().enumerate() {
        let (c, d) = &lin[i];
        if c.is_zero() {
            continue;
        }
        let eps = matches!(a.rel, Rel::Ne | Rel::Lt | Rel::Gt);
        let dup = points.iter().any(|t| matches!(t, Point::At { c: c2, d: d2, eps: e2 } if e2 == &eps && c2 == c && d2 == d));
        if !dup {
            points.push(Point::At { c: c.clone(), d: d.clone(), eps });
        }
    }
    let mut s = Sub { nv: nv - 1, atoms: vec![] };
    let mut ors = vec![];
    for t in &points {
        m.step(atoms.len() as u64 + 1)?;
        let g = subst(&f, &atoms, &lin, t, &mut s, nv, m)?;
        let guarded = match t {
            Point::At { c, .. } if !c.is_constant() => {
                let gc = s.atom(drop_last(c, nv), Rel::Ne);
                Fm::And(vec![gc, g])
            }
            _ => g,
        };
        ors.push(guarded);
        if s.atoms.len() > MAX_ATOMS {
            return Ok(None);
        }
    }
    let mut matrix = simplify(Fm::Or(ors));
    if q == Quant::ForAll {
        // not of the result, in negation normal form again
        let mut na = vec![];
        matrix = nnf(&matrix, &s.atoms, true, &mut na);
        s.atoms = na;
    }
    let mut quants = p.quants.clone();
    quants.pop();
    Ok(Some(Problem { nv: nv - 1, nfree: p.nfree, quants, atoms: s.atoms, matrix }))
}

fn subst(f: &Fm, atoms: &[PAtom], lin: &[(Poly, Poly)], t: &Point, s: &mut Sub, nv: usize, m: &mut Meter) -> WorkResult<Fm> {
    Ok(match f {
        Fm::Const(b) => Fm::Const(*b),
        Fm::And(v) => Fm::And(v.iter().map(|g| subst(g, atoms, lin, t, s, nv, m)).collect::<WorkResult<_>>()?),
        Fm::Or(v) => Fm::Or(v.iter().map(|g| subst(g, atoms, lin, t, s, nv, m)).collect::<WorkResult<_>>()?),
        Fm::Not(g) => Fm::negation(subst(g, atoms, lin, t, s, nv, m)?),
        Fm::Atom(i) => {
            let rel = atoms[*i].rel;
            let (c2, d2) = &lin[*i];
            let c2r = drop_last(c2, nv);
            let d2r = drop_last(d2, nv);
            if c2.is_zero() {
                return Ok(s.atom(d2r, rel));
            }
            match t {
                Point::MinusInf => {
                    // the sign at -inf is -sign(c'), then sign(d')
                    let (strict_c, zero_d): (Option<Rel>, Rel) = match rel {
                        Rel::Lt => (Some(Rel::Gt), Rel::Lt),
                        Rel::Gt => (Some(Rel::Lt), Rel::Gt),
                        Rel::Le => (Some(Rel::Gt), Rel::Le),
                        Rel::Ge => (Some(Rel::Lt), Rel::Ge),
                        Rel::Eq => (None, Rel::Eq),
                        Rel::Ne => (None, Rel::Ne),
                    };
                    match (rel, strict_c) {
                        (Rel::Eq, _) => Fm::And(vec![s.atom(c2r, Rel::Eq), s.atom(d2r, Rel::Eq)]),
                        (Rel::Ne, _) => Fm::Or(vec![s.atom(c2r, Rel::Ne), s.atom(d2r, Rel::Ne)]),
                        (_, Some(sc)) => Fm::Or(vec![s.atom(c2r.clone(), sc), Fm::And(vec![s.atom(c2r, Rel::Eq), s.atom(d2r, zero_d)])]),
                        _ => unreachable!("every relation handled"),
                    }
                }
                Point::At { c, d, eps } => {
                    // q(t) * c has the sign of q(t) times c^2: (c d' - c' d) c
                    let cr = drop_last(c, nv);
                    let dr = drop_last(d, nv);
                    let val = cr.mul(&d2r, m)?.sub(&c2r.mul(&dr, m)?);
                    let (val, sc) = match cr.as_constant() {
                        Some(k) => (val, k.signum()),
                        None => (val.mul(&cr, m)?, 1),
                    };
                    let val = if sc < 0 { val.neg() } else { val };
                    if !eps {
                        return Ok(s.atom(val, rel));
                    }
                    match rel {
                        Rel::Lt | Rel::Gt | Rel::Le | Rel::Ge => {
                            let strict = if matches!(rel, Rel::Lt | Rel::Le) { Rel::Lt } else { Rel::Gt };
                            let cr2 = match rel {
                                Rel::Lt => Rel::Lt,
                                Rel::Gt => Rel::Gt,
                                Rel::Le => Rel::Le,
                                _ => Rel::Ge,
                            };
                            Fm::Or(vec![s.atom(val.clone(), strict), Fm::And(vec![s.atom(val, Rel::Eq), s.atom(c2r, cr2)])])
                        }
                        Rel::Eq => Fm::And(vec![s.atom(val, Rel::Eq), s.atom(c2r, Rel::Eq)]),
                        Rel::Ne => Fm::Or(vec![s.atom(val, Rel::Ne), s.atom(c2r, Rel::Ne)]),
                    }
                }
            }
        }
    })
}

/// Constants folded out of conjunctions and disjunctions, nested ones flattened.
pub fn simplify(f: Fm) -> Fm {
    match f {
        Fm::And(v) => {
            let mut out = vec![];
            for g in v {
                match simplify(g) {
                    Fm::Const(true) => {}
                    Fm::Const(false) => return Fm::Const(false),
                    Fm::And(w) => out.extend(w),
                    h => out.push(h),
                }
            }
            match out.len() {
                0 => Fm::Const(true),
                1 => out.pop().expect("one"),
                _ => Fm::And(out),
            }
        }
        Fm::Or(v) => {
            let mut out = vec![];
            for g in v {
                match simplify(g) {
                    Fm::Const(false) => {}
                    Fm::Const(true) => return Fm::Const(true),
                    Fm::Or(w) => out.extend(w),
                    h => out.push(h),
                }
            }
            match out.len() {
                0 => Fm::Const(false),
                1 => out.pop().expect("one"),
                _ => Fm::Or(out),
            }
        }
        Fm::Not(g) => Fm::negation(simplify(*g)),
        g => g,
    }
}
