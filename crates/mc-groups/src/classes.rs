//! Conjugacy classes and character tables.
//!
//! Classes: for groups of at most [`ENUM_LIMIT`] elements every element is numbered by its coordinates in the
//! stabiliser chain and the classes are the orbits of conjugation by the generators (breadth first). Larger groups
//! get class representatives from random elements and their powers, tested for conjugacy by backtrack search and
//! counted by their centraliser orders until the class sizes add up to |G| (a Las Vegas method: the result is
//! exact when it is returned).
//!
//! Character tables by the Dixon-Schneider method (Dixon, "High speed computation of group characters", Numer.
//! Math. 10, 1967; Schneider, J. Symbolic Comput. 9, 1990): the class multiplication coefficients c_jk^l = #{x in
//! C_j : x^-1 z_l in C_k} give matrices M_j whose common eigenvectors are the vectors of central character values
//! w_k = |C_k| chi(g_k)/chi(1). The eigenspaces are split modulo a prime p = 1 (mod e) above 2 sqrt|G| (e the
//! exponent); chi(1)^2 = |G| / sum_k w_k w_k' / |C_k| (k' the class of the inverses), and each value is lifted
//! from its reductions on the powers of g_k as sum_s m_s zeta_o^s, with m_s = (1/o) sum_l chi(g_k^l) z^(-s l) the
//! multiplicity of the eigenvalue zeta^s (0 <= m_s <= chi(1) < p).
use crate::backtrack;
use crate::groups::Group;
use crate::perm::{self, Perm};
use crate::Ctl;
use mc_eval::Res;

/// Groups up to this order get their classes by enumerating the elements.
pub const ENUM_LIMIT: u128 = 2_000_000;

/// The classes of a group enumerated element by element.
pub struct Classes {
    /// representatives (the first element of each class found; the identity first)
    pub reps: Vec<Perm>,
    pub sizes: Vec<u64>,
    /// the class of each element, by its chain index
    pub class_of: Vec<u32>,
    pub pos: Vec<Vec<u32>>,
    pub order: u64,
}

fn element_at(g: &Group, mut idx: u64) -> Perm {
    let c = &g.chain;
    let mut digits = Vec::with_capacity(c.levels.len());
    for l in &c.levels {
        let r = l.orbit.len() as u64;
        digits.push((idx % r) as usize);
        idx /= r;
    }
    let mut p = perm::identity(g.n);
    for (i, l) in c.levels.iter().enumerate().rev() {
        let x = l.orbit[digits[i]];
        p = perm::mul(&p, l.trans[x as usize].as_ref().expect("orbit points have transversal elements"));
    }
    p
}

impl Classes {
    pub fn index(&self, g: &Group, p: &[u32]) -> u64 {
        g.chain.index_of(&self.pos, p.to_vec()).expect("the element is in the group")
    }

    pub fn class(&self, g: &Group, p: &[u32]) -> usize {
        self.class_of[self.index(g, p) as usize] as usize
    }

    /// Classes sorted by element order and then size, the identity first.
    pub fn compute(g: &Group, ctl: &Ctl) -> Res<Option<Classes>> {
        let Some(order) = g.order().filter(|&o| o <= ENUM_LIMIT) else { return Ok(None) };
        let order = order as u64;
        let pos = g.chain.positions();
        let mut class_of = vec![u32::MAX; order as usize];
        let mut reps: Vec<Perm> = Vec::new();
        let mut sizes: Vec<u64> = Vec::new();
        for start in 0..order {
            if class_of[start as usize] != u32::MAX {
                continue;
            }
            let x = element_at(g, start);
            let id = reps.len() as u32;
            class_of[start as usize] = id;
            let mut queue = vec![x.clone()];
            let mut k = 0;
            while k < queue.len() {
                for s in &g.gens {
                    let y = perm::conj(&queue[k], s);
                    let iy = g.chain.index_of(&pos, y.clone()).expect("conjugates stay in the group") as usize;
                    if class_of[iy] == u32::MAX {
                        class_of[iy] = id;
                        queue.push(y);
                    }
                }
                k += 1;
                if k % 4096 == 0 {
                    ctl.step(4096)?;
                }
            }
            reps.push(x);
            sizes.push(queue.len() as u64);
        }
        let mut c = Classes { reps, sizes, class_of, pos, order };
        c.sort();
        Ok(Some(c))
    }

    fn sort(&mut self) {
        let r = self.reps.len();
        let mut idx: Vec<usize> = (0..r).collect();
        let key = |i: usize| (perm::order(&self.reps[i]).unwrap_or(u128::MAX), self.sizes[i], perm::cycle_type(&self.reps[i]));
        idx.sort_by_key(|&i| key(i));
        let mut newid = vec![0u32; r];
        for (k, &i) in idx.iter().enumerate() {
            newid[i] = k as u32;
        }
        for c in self.class_of.iter_mut() {
            *c = newid[*c as usize];
        }
        self.reps = idx.iter().map(|&i| self.reps[i].clone()).collect();
        self.sizes = idx.iter().map(|&i| self.sizes[i]).collect();
    }

    /// The elements of each class (by chain index).
    pub fn members(&self) -> Vec<Vec<u64>> {
        let mut out = vec![Vec::new(); self.reps.len()];
        for (i, &c) in self.class_of.iter().enumerate() {
            out[c as usize].push(i as u64);
        }
        out
    }
}

/// Class representatives and sizes of any group: by enumeration when it is small enough, else by random elements,
/// their powers, conjugacy tests and centraliser orders (`None` when the sizes do not add up within the budget).
pub fn reps_and_sizes(g: &Group, ctl: &Ctl) -> Res<Option<(Vec<Perm>, Vec<u128>)>> {
    if let Some(c) = Classes::compute(g, ctl)? {
        return Ok(Some((c.reps, c.sizes.iter().map(|&s| s as u128).collect())));
    }
    let Some(order) = g.order() else { return Ok(None) };
    let mut rng = crate::chain::Rng::new(0x9e37_79b9_7f4a_7c15);
    let mut reps: Vec<Perm> = vec![perm::identity(g.n)];
    let mut sizes: Vec<u128> = vec![1];
    let mut total: u128 = 1;
    let mut tries = 0;
    let mut pending: Vec<Perm> = Vec::new();
    while total < order {
        tries += 1;
        if tries > 4000 {
            return Ok(None);
        }
        let x = match pending.pop() {
            Some(x) => x,
            None => g.chain.random(&mut rng),
        };
        let ct = perm::cycle_type(&x);
        let mut known = false;
        for r in &reps {
            if perm::cycle_type(r) == ct && backtrack::conjugator(g, r, &x, ctl)?.is_some() {
                known = true;
                break;
            }
        }
        if known {
            continue;
        }
        let c = backtrack::centralizer(g, std::slice::from_ref(&x), ctl)?;
        let size = order / c.order().unwrap_or(order);
        // the powers of a new representative are candidates too
        let o = perm::order(&x).unwrap_or(1) as i64;
        for k in 2..o {
            pending.push(perm::pow(&x, k));
        }
        reps.push(x);
        sizes.push(size);
        total += size;
    }
    let mut idx: Vec<usize> = (0..reps.len()).collect();
    idx.sort_by_key(|&i| (perm::order(&reps[i]).unwrap_or(u128::MAX), sizes[i], perm::cycle_type(&reps[i])));
    Ok(Some((idx.iter().map(|&i| reps[i].clone()).collect(), idx.iter().map(|&i| sizes[i]).collect())))
}

/// Class representatives of any group (see [`reps_and_sizes`]).
pub fn class_reps_any(g: &Group, ctl: &Ctl) -> Res<Option<Vec<Perm>>> {
    Ok(reps_and_sizes(g, ctl)?.map(|r| r.0))
}

fn pow_mod(mut b: u64, mut e: u64, p: u64) -> u64 {
    let mut r = 1u64;
    b %= p;
    while e > 0 {
        if e & 1 == 1 {
            r = (r as u128 * b as u128 % p as u128) as u64;
        }
        b = (b as u128 * b as u128 % p as u128) as u64;
        e >>= 1;
    }
    r
}

fn inv_mod(a: u64, p: u64) -> u64 {
    pow_mod(a, p - 2, p)
}

fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    let mut d = 2;
    while d * d <= n {
        if n.is_multiple_of(d) {
            return false;
        }
        d += 1;
    }
    true
}

/// Row-reduced basis of the null space of the d x d matrix a over F_p.
fn nullspace(a: &[Vec<u64>], p: u64) -> Vec<Vec<u64>> {
    let d = a.len();
    let mut m: Vec<Vec<u64>> = a.to_vec();
    let mut pivots = Vec::new();
    let mut row = 0;
    for col in 0..d {
        let Some(r) = (row..d).find(|&r| m[r][col] != 0) else { continue };
        m.swap(row, r);
        let iv = inv_mod(m[row][col], p);
        for x in m[row].iter_mut() {
            *x = *x * iv % p;
        }
        for r in 0..d {
            if r != row && m[r][col] != 0 {
                let f = m[r][col];
                for c in 0..d {
                    m[r][c] = (m[r][c] + p - f * m[row][c] % p) % p;
                }
            }
        }
        pivots.push(col);
        row += 1;
    }
    let mut out = Vec::new();
    for free in 0..d {
        if pivots.contains(&free) {
            continue;
        }
        let mut v = vec![0u64; d];
        v[free] = 1;
        for (r, &pc) in pivots.iter().enumerate() {
            v[pc] = (p - m[r][free]) % p;
        }
        out.push(v);
    }
    out
}

/// Coordinates of v in the basis `basis` (rows), which spans a space containing v.
fn coords(basis: &[Vec<u64>], v: &[u64], p: u64) -> Vec<u64> {
    // solve sum a_i basis_i = v by elimination on the transposed system
    let d = basis.len();
    let r = v.len();
    let mut m: Vec<Vec<u64>> = (0..r).map(|k| {
        let mut row: Vec<u64> = (0..d).map(|i| basis[i][k]).collect();
        row.push(v[k]);
        row
    }).collect();
    let mut prow = 0;
    let mut pivcols = Vec::new();
    for col in 0..d {
        let Some(rr) = (prow..r).find(|&rr| m[rr][col] != 0) else { continue };
        m.swap(prow, rr);
        let iv = inv_mod(m[prow][col], p);
        for x in m[prow].iter_mut() {
            *x = *x * iv % p;
        }
        for rr in 0..r {
            if rr != prow && m[rr][col] != 0 {
                let f = m[rr][col];
                for c in 0..=d {
                    m[rr][c] = (m[rr][c] + p - f * m[prow][c] % p) % p;
                }
            }
        }
        pivcols.push(col);
        prow += 1;
    }
    let mut out = vec![0u64; d];
    for (k, &c) in pivcols.iter().enumerate() {
        out[c] = m[k][d];
    }
    out
}

/// One character value: the multiplicities m_s of zeta_o^s (s = 0..o-1).
pub type Value = (u64, Vec<u64>);

/// The character table: classes (sorted), and for each irreducible character (by degree) the values on the
/// classes, each as (order o of the class elements, multiplicities of the o-th roots of unity).
pub fn character_table(g: &Group, ctl: &Ctl) -> Res<Option<(Classes, Vec<Vec<Value>>)>> {
    let Some(cl) = Classes::compute(g, ctl)? else { return Ok(None) };
    let r = cl.reps.len();
    let order = cl.order;
    let ords: Vec<u64> = cl.reps.iter().map(|x| perm::order(x).unwrap_or(1) as u64).collect();
    let mut e = 1u64;
    for &o in &ords {
        e = e / gcd(e, o) * o;
    }
    // p = 1 (mod e), p > 2 sqrt|G|
    let mut s = 1u64;
    while s * s <= order {
        s += 1;
    }
    let mut p = e + 1;
    while !(p > 2 * s && is_prime(p)) {
        p += e;
    }
    // a primitive e-th root of unity mod p
    let z = (2..p)
        .map(|a| pow_mod(a, (p - 1) / e, p))
        .find(|&w| (1..e).filter(|d| e.is_multiple_of(*d)).all(|d| pow_mod(w, d, p) != 1))
        .expect("F_p has primitive e-th roots for p = 1 mod e");
    let members = cl.members();
    let inv_class: Vec<usize> = cl.reps.iter().map(|x| cl.class(g, &perm::inv(x))).collect();
    // power maps: class of rep_k^l
    let powmap: Vec<Vec<usize>> = (0..r)
        .map(|k| (0..ords[k]).map(|l| cl.class(g, &perm::pow(&cl.reps[k], l as i64))).collect())
        .collect();
    let class_matrix = |j: usize| -> Res<Vec<Vec<u64>>> {
        let mut m = vec![vec![0u64; r]; r];
        for &xi in &members[j] {
            let x = element_at(g, xi);
            let xinv = perm::inv(&x);
            for l in 0..r {
                let y = perm::mul(&xinv, &cl.reps[l]);
                let k = cl.class(g, &y);
                m[k][l] += 1;
            }
            ctl.step(r as u64)?;
        }
        Ok(m)
    };
    let id_class = 0usize;
    let mut spaces: Vec<Vec<Vec<u64>>> = vec![(0..r).map(|i| (0..r).map(|k| u64::from(k == i)).collect()).collect()];
    let mut j = 1;
    while spaces.iter().any(|w| w.len() > 1) {
        if j >= r {
            return Ok(None);
        }
        let m = class_matrix(j)?;
        let mut next = Vec::new();
        for w in spaces {
            if w.len() == 1 {
                next.push(w);
                continue;
            }
            let d = w.len();
            // the matrix of M_j on W: columns are the coordinates of M_j w_t
            let mut a = vec![vec![0u64; d]; d];
            for (t, wt) in w.iter().enumerate() {
                let mw: Vec<u64> = (0..r).map(|k| (0..r).fold(0u64, |acc, l| (acc + m[k][l] % p * wt[l]) % p)).collect();
                let c = coords(&w, &mw, p);
                for i in 0..d {
                    a[i][t] = c[i];
                }
            }
            let mut found = 0;
            for lam in 0..p {
                let mut b = a.clone();
                for (i, row) in b.iter_mut().enumerate() {
                    row[i] = (row[i] + p - lam) % p;
                }
                let ns = nullspace(&b, p);
                if ns.is_empty() {
                    continue;
                }
                found += ns.len();
                let vecs: Vec<Vec<u64>> = ns
                    .iter()
                    .map(|c| (0..r).map(|k| (0..d).fold(0u64, |acc, t| (acc + c[t] * w[t][k]) % p)).collect())
                    .collect();
                next.push(vecs);
                if found == d {
                    break;
                }
                ctl.step(1)?;
            }
            if found != d {
                return Ok(None);
            }
        }
        spaces = next;
        j += 1;
    }
    let sizes_p: Vec<u64> = cl.sizes.iter().map(|&s| s % p).collect();
    let mut table = Vec::new();
    for w in spaces {
        let v = &w[0];
        let s0 = inv_mod(v[id_class], p);
        let w: Vec<u64> = v.iter().map(|&x| x * s0 % p).collect();
        let mut sum = 0u64;
        for k in 0..r {
            sum = (sum + w[k] * w[inv_class[k]] % p * inv_mod(sizes_p[k], p)) % p;
        }
        let deg2 = (order % p) * inv_mod(sum, p) % p;
        let Some(deg) = (1..=s).find(|&d| d * d % p == deg2) else { return Ok(None) };
        let chi: Vec<u64> = (0..r).map(|k| deg % p * w[k] % p * inv_mod(sizes_p[k], p) % p).collect();
        let mut row = Vec::with_capacity(r);
        for k in 0..r {
            let o = ords[k];
            let zo = pow_mod(z, e / o, p);
            let io = inv_mod(o % p, p);
            let mut ms = Vec::with_capacity(o as usize);
            for s_ in 0..o {
                let mut acc = 0u64;
                for l in 0..o {
                    let zz = pow_mod(zo, (o - (s_ * l) % o) % o, p);
                    acc = (acc + chi[powmap[k][l as usize]] * zz) % p;
                }
                let m = acc * io % p;
                if m > deg {
                    return Ok(None);
                }
                ms.push(m);
            }
            row.push((o, ms));
        }
        table.push((deg, row));
    }
    // by degree; within a degree the larger multiplicities of 1 first (so the trivial character leads)
    table.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| key(&b.1).cmp(&key(&a.1))));
    Ok(Some((cl, table.into_iter().map(|t| t.1).collect())))
}

fn key(row: &[Value]) -> Vec<Vec<u64>> {
    row.iter().map(|v| v.1.clone()).collect()
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}
