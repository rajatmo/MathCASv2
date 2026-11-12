//! Projection: the square-free basis of irreducible factors, level by level.
//!
//! For a set `A` of irreducible polynomials of level `k` (main variable `x_(k-1)`), the projection is
//!
//! - **Lazard** (the fallback, decision CAD.2): the leading and trailing coefficients of every `f` in `A`, the
//!   discriminant of every `f`, and the resultant of every pair. Proved complete by McCallum, Parusiński and
//!   Paunescu (2019) with Lazard's valuation-based lifting ([`crate::poly::lazard_residue`]): it never fails.
//! - **Brown** (the default: McCallum's projection with Brown's improvement, Brown 2001): the leading coefficients only, the
//!   discriminants and the resultants. Smaller, but valid only for well-oriented input: lifting reports
//!   [`crate::cad::CadError::NotWellOriented`] when a polynomial vanishes identically over a cell of positive
//!   dimension, and the decomposition is redone with Lazard's operator.
//!
//! An **equational constraint** `e = 0` at the top level (McCallum 1999) restricts the top projection to the
//! projection of `e`'s factors and the resultants of those factors with the other top-level factors. Every result
//! is factored into irreducible, primitive factors with positive leading coefficient (`mc_poly::factor`), and each
//! factor is kept once, at its level.
use crate::poly::{coeffs, factors, level};
use mc_poly::resultant::{discriminant, resultant};
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;

/// The projection operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// leading and trailing coefficients, discriminants, resultants (complete)
    Lazard,
    /// leading coefficients, discriminants, resultants (well-oriented input only)
    Brown,
}

/// The projection factors: every irreducible factor once, by level.
#[derive(Clone, Debug)]
pub struct Basis {
    /// the number of variables
    pub nv: usize,
    /// the factors
    pub facs: Vec<Poly>,
    /// the level of each factor
    pub lv: Vec<usize>,
    /// the factors of each level (index `level - 1`)
    pub by_level: Vec<Vec<usize>>,
    /// the factors of the top-level equational constraint, when one is used
    pub ec: Option<Vec<usize>>,
    /// the operator used
    pub method: Method,
}

/// Too many projection factors (the decomposition would not finish in reasonable time).
#[derive(Debug)]
pub struct TooLarge;

impl Basis {
    /// An empty basis in `nv` variables.
    pub fn new(nv: usize, method: Method) -> Basis {
        Basis { nv, facs: vec![], lv: vec![], by_level: vec![vec![]; nv], ec: None, method }
    }

    /// The index of the irreducible factor `p` (primitive, positive leading coefficient, not constant), added
    /// when new.
    pub fn add(&mut self, p: Poly) -> usize {
        let l = level(&p);
        debug_assert!(l >= 1);
        if let Some(&i) = self.by_level[l - 1].iter().find(|&&i| self.facs[i] == p) {
            return i;
        }
        self.facs.push(p);
        self.lv.push(l);
        self.by_level[l - 1].push(self.facs.len() - 1);
        self.facs.len() - 1
    }

    /// Add the irreducible factors of `p` (constants are dropped).
    fn add_factors(&mut self, p: &Poly, m: &mut Meter) -> WorkResult<()> {
        if p.is_zero() || p.is_constant() {
            return Ok(());
        }
        let (_, fs) = factors(p, m)?;
        for (f, _) in fs {
            self.add(f);
        }
        Ok(())
    }

    /// Close the basis under projection, from the top level down. `max` bounds the number of factors.
    pub fn close(&mut self, max: usize, m: &mut Meter) -> WorkResult<Result<(), TooLarge>> {
        for l in (2..=self.nv).rev() {
            let v = l - 1;
            let a: Vec<usize> = self.by_level[l - 1].clone();
            let (project, pairs_with): (Vec<usize>, Vec<usize>) = match (&self.ec, l == self.nv) {
                (Some(e), true) => (e.clone(), a.iter().copied().filter(|i| !e.contains(i)).collect()),
                _ => (a.clone(), vec![]),
            };
            let mut out: Vec<Poly> = vec![];
            for (n, &i) in project.iter().enumerate() {
                let f = self.facs[i].clone();
                let cs = coeffs(&f, v);
                // leading coefficient (and trailing for Lazard)
                out.push(cs[cs.len() - 1].clone());
                if self.method == Method::Lazard {
                    if let Some(t) = cs.iter().find(|c| !c.is_zero()) {
                        out.push(t.clone());
                    }
                }
                if cs.len() > 2 {
                    out.push(discriminant(&f, v, m)?);
                }
                for &j in &project[n + 1..] {
                    out.push(resultant(&f, &self.facs[j], v, m)?);
                }
                for &j in &pairs_with {
                    out.push(resultant(&f, &self.facs[j], v, m)?);
                }
            }
            for p in out {
                m.step(1)?;
                self.add_factors(&p, m)?;
                if self.facs.len() > max {
                    return Ok(Err(TooLarge));
                }
            }
        }
        Ok(Ok(()))
    }
}
