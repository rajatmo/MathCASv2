//! Sparse Gaussian elimination over a field: one solution of `A u = b` (free unknowns zero), or `None` when the
//! system is inconsistent.
use mc_poly::ring::CoeffRing;
use mc_poly::work::{exact, Meter, WorkResult};

/// A sparse row: (column, value) in increasing column order, and the right-hand side.
pub struct Row<E> {
    /// the entries
    pub cols: Vec<(usize, E)>,
    /// the right-hand side
    pub rhs: E,
}

/// `a - c b` for sparse rows.
fn axpy<R: CoeffRing>(r: &R, a: &Row<R::E>, c: &R::E, b: &Row<R::E>, m: &mut Meter) -> WorkResult<Row<R::E>> {
    let mut out = Vec::with_capacity(a.cols.len() + b.cols.len());
    let (mut i, mut j) = (0, 0);
    while i < a.cols.len() || j < b.cols.len() {
        m.step(1)?;
        let ca = a.cols.get(i).map(|e| e.0).unwrap_or(usize::MAX);
        let cb = b.cols.get(j).map(|e| e.0).unwrap_or(usize::MAX);
        if ca < cb {
            out.push(a.cols[i].clone());
            i += 1;
        } else if cb < ca {
            let v = r.neg(&r.mul(c, &b.cols[j].1, m)?);
            if !r.is_zero(&v) {
                out.push((cb, v));
            }
            j += 1;
        } else {
            let v = r.sub(&a.cols[i].1, &r.mul(c, &b.cols[j].1, m)?);
            if !r.is_zero(&v) {
                out.push((ca, v));
            }
            i += 1;
            j += 1;
        }
    }
    let rhs = r.sub(&a.rhs, &r.mul(c, &b.rhs, m)?);
    Ok(Row { cols: out, rhs })
}

/// One solution in `n` unknowns, or `None` (inconsistent).
pub fn solve<R: CoeffRing>(r: &R, rows: Vec<Row<R::E>>, n: usize, m: &mut Meter) -> WorkResult<Option<Vec<R::E>>> {
    // pivot rows, normalised (leading entry one), by pivot column
    let mut piv: Vec<Option<Row<R::E>>> = (0..n).map(|_| None).collect();
    let mut order: Vec<usize> = vec![];
    for mut row in rows {
        // reduce by the existing pivots in increasing column order
        loop {
            m.step(row.cols.len() as u64 + 1)?;
            let Some(&(c, _)) = row.cols.first() else { break };
            match &piv[c] {
                Some(p) => {
                    let coef = row.cols[0].1.clone();
                    row = axpy(r, &row, &coef, p, m)?;
                }
                None => break,
            }
            // the first entry is gone: go on with the new first entry (row echelon form is enough)
        }
        match row.cols.first() {
            None => {
                if !r.is_zero(&row.rhs) {
                    return Ok(None);
                }
            }
            Some(&(c, ref lead)) => {
                let inv = exact(r.inv(lead, m)?, "a nonzero entry of a field")?;
                let cols = row
                    .cols
                    .iter()
                    .map(|(j, v)| Ok((*j, r.mul(v, &inv, m)?)))
                    .collect::<WorkResult<Vec<_>>>()?;
                let rhs = r.mul(&row.rhs, &inv, m)?;
                piv[c] = Some(Row { cols, rhs });
                order.push(c);
            }
        }
    }
    // back substitution, last pivot first
    let mut u: Vec<R::E> = (0..n).map(|_| r.zero()).collect();
    let mut cols: Vec<usize> = order.clone();
    cols.sort_unstable();
    for &c in cols.iter().rev() {
        let p = piv[c].as_ref().expect("a pivot");
        let mut v = p.rhs.clone();
        for (j, a) in &p.cols[1..] {
            m.step(1)?;
            if !r.is_zero(&u[*j]) {
                v = r.sub(&v, &r.mul(a, &u[*j], m)?);
            }
        }
        u[c] = v;
    }
    Ok(Some(u))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_arith::num::Rat;
    use mc_poly::ring::Rationals;
    use mc_poly::work::Unlimited;

    fn q(n: i64) -> Rat {
        Rat::from_i64(n)
    }

    #[test]
    fn solves_and_detects_inconsistency() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        // x + y = 3, x - y = 1, 2x + 2y = 6 (dependent): x = 2, y = 1
        let rows = vec![
            Row { cols: vec![(0, q(1)), (1, q(1))], rhs: q(3) },
            Row { cols: vec![(0, q(1)), (1, q(-1))], rhs: q(1) },
            Row { cols: vec![(0, q(2)), (1, q(2))], rhs: q(6) },
        ];
        let u = solve(&Rationals, rows, 2, &mut m).unwrap().unwrap();
        assert_eq!(u, vec![q(2), q(1)]);
        // x + y = 1, x + y = 2: none
        let rows = vec![Row { cols: vec![(0, q(1)), (1, q(1))], rhs: q(1) }, Row { cols: vec![(0, q(1)), (1, q(1))], rhs: q(2) }];
        assert!(solve(&Rationals, rows, 2, &mut m).unwrap().is_none());
        // a free unknown is zero: x + z = 1 in three unknowns
        let rows = vec![Row { cols: vec![(0, q(1)), (2, q(1))], rhs: q(1) }];
        assert_eq!(solve(&Rationals, rows, 3, &mut m).unwrap().unwrap(), vec![q(1), q(0), q(0)]);
    }
}
