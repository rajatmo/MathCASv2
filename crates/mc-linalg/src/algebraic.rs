//! The number field `Q[t]/(f)` of an irreducible polynomial `f`: the exact eigenvectors of an eigenvalue that is a
//! root of `f` are a null space over this field (decision LA.11). The field and its elements are `mc-algebra`'s
//! ([`mc_algebra::Alg`], item F1): this module makes them a [`Field`] for the elimination.
use crate::elim::{Field, Weight};
pub use mc_algebra::{Alg, NumberField};

impl Field for Alg {
    fn zero_like(&self) -> Alg {
        Alg { c: vec![], k: self.k.clone() }
    }
    fn one_like(&self) -> Alg {
        Alg { c: self.k.one(), k: self.k.clone() }
    }
    fn add(&self, o: &Alg) -> Alg {
        Alg::add(self, o)
    }
    fn sub(&self, o: &Alg) -> Alg {
        Alg::sub(self, o)
    }
    fn mul(&self, o: &Alg) -> Alg {
        Alg::mul(self, o)
    }
    fn div(&self, o: &Alg) -> Option<Alg> {
        Alg::div(self, o)
    }
    fn weight(&self, _tol: f64) -> Weight {
        if self.is_zero() {
            Weight::Zero
        } else {
            Weight::Pivot(1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_arith::num::Rat;

    #[test]
    fn field_of_sqrt2() {
        // t^2 - 2
        let k = NumberField::new(vec![Rat::from_i64(-2), Rat::from_i64(0), Rat::from_i64(1)], None);
        let t = Alg::generator(&k);
        let two = Field::mul(&t, &t);
        assert_eq!(two, Alg::rational(Rat::from_i64(2), &k));
        // (1 + t)^-1 = t - 1
        let one = t.one_like();
        let inv = Field::div(&one, &Field::add(&one, &t)).unwrap();
        assert_eq!(inv, Field::sub(&t, &one));
    }
}
