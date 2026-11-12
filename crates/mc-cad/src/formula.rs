//! Formulas over polynomial atoms: the quantifier-free matrix of a problem, its atoms as signed products of
//! basis factors, and three-valued evaluation from the signs known at a (partial) sample point.
use mc_poly::Poly;

/// A relation `p rel 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rel {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
}

impl Rel {
    /// Does the sign `s` satisfy the relation?
    pub fn holds(self, s: i32) -> bool {
        match self {
            Rel::Lt => s < 0,
            Rel::Le => s <= 0,
            Rel::Gt => s > 0,
            Rel::Ge => s >= 0,
            Rel::Eq => s == 0,
            Rel::Ne => s != 0,
        }
    }
    /// The negated relation.
    pub fn negate(self) -> Rel {
        match self {
            Rel::Lt => Rel::Ge,
            Rel::Le => Rel::Gt,
            Rel::Gt => Rel::Le,
            Rel::Ge => Rel::Lt,
            Rel::Eq => Rel::Ne,
            Rel::Ne => Rel::Eq,
        }
    }
    /// The relation of `-p rel' 0` equivalent to `p rel 0`.
    pub fn flip(self) -> Rel {
        match self {
            Rel::Lt => Rel::Gt,
            Rel::Le => Rel::Ge,
            Rel::Gt => Rel::Lt,
            Rel::Ge => Rel::Le,
            r => r,
        }
    }
    /// The name of the comparison head.
    pub fn name(self) -> &'static str {
        match self {
            Rel::Lt => "less",
            Rel::Le => "less_equal",
            Rel::Gt => "greater",
            Rel::Ge => "greater_equal",
            Rel::Eq => "equal",
            Rel::Ne => "unequal",
        }
    }
}

/// A quantifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quant {
    /// there is
    Exists,
    /// for every
    ForAll,
}

/// A quantifier-free formula over atoms (indices).
#[derive(Clone, Debug, PartialEq)]
pub enum Fm {
    /// true or false
    Const(bool),
    /// an atom
    Atom(usize),
    /// conjunction
    And(Vec<Fm>),
    /// disjunction
    Or(Vec<Fm>),
    /// negation
    Not(Box<Fm>),
}

impl Fm {
    /// The atoms that occur.
    pub fn atoms(&self, out: &mut Vec<usize>) {
        match self {
            Fm::Const(_) => {}
            Fm::Atom(i) => {
                if !out.contains(i) {
                    out.push(*i)
                }
            }
            Fm::And(v) | Fm::Or(v) => v.iter().for_each(|f| f.atoms(out)),
            Fm::Not(f) => f.atoms(out),
        }
    }
    /// Negation pushed to the atoms is not done here; `not` stays a node.
    pub fn negation(f: Fm) -> Fm {
        match f {
            Fm::Const(b) => Fm::Const(!b),
            Fm::Not(g) => *g,
            g => Fm::Not(Box::new(g)),
        }
    }
    /// The negation normal form: negations pushed into the relations (negated atoms are added to `atoms`).
    pub fn nnf(&self, atoms: &mut Vec<PAtom>, neg: bool) -> Fm {
        match self {
            Fm::Const(b) => Fm::Const(*b != neg),
            Fm::Atom(i) => {
                if !neg {
                    return Fm::Atom(*i);
                }
                let a = atoms[*i].clone();
                let b = PAtom { rel: a.rel.negate(), ..a };
                match atoms.iter().position(|x| *x == b) {
                    Some(j) => Fm::Atom(j),
                    None => {
                        atoms.push(b);
                        Fm::Atom(atoms.len() - 1)
                    }
                }
            }
            Fm::Not(g) => g.nnf(atoms, !neg),
            Fm::And(v) => {
                let w = v.iter().map(|g| g.nnf(atoms, neg)).collect();
                if neg {
                    Fm::Or(w)
                } else {
                    Fm::And(w)
                }
            }
            Fm::Or(v) => {
                let w = v.iter().map(|g| g.nnf(atoms, neg)).collect();
                if neg {
                    Fm::And(w)
                } else {
                    Fm::Or(w)
                }
            }
        }
    }
    /// The conjuncts of a conjunction (the formula itself otherwise).
    pub fn conjuncts(&self) -> Vec<&Fm> {
        match self {
            Fm::And(v) => v.iter().flat_map(|f| f.conjuncts()).collect(),
            f => vec![f],
        }
    }
}

/// A polynomial atom `num / den rel 0` (integer polynomials; `den` is 1 for a polynomial).
#[derive(Clone, Debug, PartialEq)]
pub struct PAtom {
    /// numerator
    pub num: Poly,
    /// denominator
    pub den: Poly,
    /// relation
    pub rel: Rel,
}

/// An atom as a signed product of basis factors.
#[derive(Clone, Debug, Default)]
pub struct Atom {
    /// the sign of the constant factor (0 when the numerator is zero)
    pub unit: i32,
    /// (factor, multiplicity) of the numerator
    pub num: Vec<(usize, u32)>,
    /// (factor, multiplicity) of the denominator
    pub den: Vec<(usize, u32)>,
    /// the relation
    pub rel: Option<Rel>,
}

/// A denominator vanishes: the atom's value is undefined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Undefined;

/// A sign not yet known.
pub const UNKNOWN: i8 = 2;

/// Three-valued truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tri {
    /// true
    T,
    /// false
    F,
    /// not determined by the known signs
    U,
}

impl Atom {
    /// The sign of the atom's rational function from the factor signs: `Err(Undefined)` when a denominator vanishes,
    /// `Ok(None)` when a needed sign is unknown.
    pub fn sign(&self, signs: &[i8]) -> Result<Option<i32>, Undefined> {
        let mut unknown = false;
        let mut s = 1i32;
        for &(f, k) in &self.den {
            match signs[f] {
                0 => return Err(Undefined),
                UNKNOWN => unknown = true,
                x => {
                    if k % 2 == 1 {
                        s *= x as i32
                    }
                }
            }
        }
        if self.unit == 0 {
            return Ok(if unknown { None } else { Some(0) });
        }
        s *= self.unit;
        for &(f, k) in &self.num {
            match signs[f] {
                0 => {
                    s = 0;
                }
                UNKNOWN => unknown = true,
                x => {
                    if k % 2 == 1 {
                        s *= x as i32
                    }
                }
            }
        }
        // a zero factor decides the sign even when others are unknown, unless a denominator is unknown
        if s == 0 {
            return Ok(if self.den.iter().any(|&(f, _)| signs[f] == UNKNOWN) { None } else { Some(0) });
        }
        Ok(if unknown { None } else { Some(s) })
    }
}

/// The truth of `f` (in negation normal form) from the factor signs known so far; an atom whose denominator
/// vanishes is false there (a comparison of an undefined value, decision CAD.10).
pub fn eval(f: &Fm, atoms: &[Atom], signs: &[i8]) -> Tri {
    eval3(f, atoms, signs)
}

fn eval3(f: &Fm, atoms: &[Atom], signs: &[i8]) -> Tri {
    match f {
        Fm::Const(b) => {
            if *b {
                Tri::T
            } else {
                Tri::F
            }
        }
        Fm::Atom(i) => {
            let a = &atoms[*i];
            match a.sign(signs) {
                Err(Undefined) => Tri::F,
                Ok(Some(s)) => {
                    if a.rel.expect("a relation").holds(s) {
                        Tri::T
                    } else {
                        Tri::F
                    }
                }
                _ => Tri::U,
            }
        }
        Fm::And(v) => {
            let mut r = Tri::T;
            for g in v {
                match eval3(g, atoms, signs) {
                    Tri::F => return Tri::F,
                    Tri::U => r = Tri::U,
                    Tri::T => {}
                }
            }
            r
        }
        Fm::Or(v) => {
            let mut r = Tri::F;
            for g in v {
                match eval3(g, atoms, signs) {
                    Tri::T => return Tri::T,
                    Tri::U => r = Tri::U,
                    Tri::F => {}
                }
            }
            r
        }
        Fm::Not(g) => match eval3(g, atoms, signs) {
            Tri::T => Tri::F,
            Tri::F => Tri::T,
            Tri::U => Tri::U,
        },
    }
}
