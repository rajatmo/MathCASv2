//! Permutations of {0, ..., n-1} as image vectors, with the right action: `x^(p q) = (x^p)^q`, so the product
//! `p q` applies `p` first (the convention of Seress, *Permutation Group Algorithms*, and of most computational
//! group theory). Points are 0-based here and 1-based in terms.

/// A permutation: `p[x]` is the image of `x`.
pub type Perm = Vec<u32>;

/// The identity of degree `n`.
pub fn identity(n: usize) -> Perm {
    (0..n as u32).collect()
}

/// Whether `p` is the identity.
pub fn is_identity(p: &[u32]) -> bool {
    p.iter().enumerate().all(|(i, &x)| x as usize == i)
}

/// `p` extended to degree `n` (fixing the new points); `n` must be at least `p.len()`.
pub fn widen(p: &[u32], n: usize) -> Perm {
    let mut q = p.to_vec();
    q.extend(p.len() as u32..n as u32);
    q
}

/// The product `a b` (first `a`, then `b`) of two permutations of the same degree.
pub fn mul(a: &[u32], b: &[u32]) -> Perm {
    a.iter().map(|&x| b[x as usize]).collect()
}

/// The inverse.
pub fn inv(p: &[u32]) -> Perm {
    let mut q = vec![0; p.len()];
    for (i, &x) in p.iter().enumerate() {
        q[x as usize] = i as u32;
    }
    q
}

/// `p^k` for any integer k.
pub fn pow(p: &[u32], k: i64) -> Perm {
    let base = if k < 0 { inv(p) } else { p.to_vec() };
    let mut e = k.unsigned_abs();
    let mut r = identity(p.len());
    let mut b = base;
    while e > 0 {
        if e & 1 == 1 {
            r = mul(&r, &b);
        }
        b = mul(&b, &b);
        e >>= 1;
    }
    r
}

/// The conjugate `g^-1 p g` (the image of `p` under relabelling the points by `g`).
pub fn conj(p: &[u32], g: &[u32]) -> Perm {
    let mut q = vec![0; p.len()];
    for (i, &x) in p.iter().enumerate() {
        q[g[i] as usize] = g[x as usize];
    }
    q
}

/// The commutator `a^-1 b^-1 a b`.
pub fn comm(a: &[u32], b: &[u32]) -> Perm {
    mul(&mul(&inv(a), &inv(b)), &mul(a, b))
}

/// The disjoint cycles of length at least 2, each starting at its least point, ordered by that point.
pub fn cycles(p: &[u32]) -> Vec<Vec<u32>> {
    let mut seen = vec![false; p.len()];
    let mut out = Vec::new();
    for s in 0..p.len() {
        if seen[s] || p[s] as usize == s {
            continue;
        }
        let mut c = vec![s as u32];
        seen[s] = true;
        let mut x = p[s] as usize;
        while x != s {
            seen[x] = true;
            c.push(x as u32);
            x = p[x] as usize;
        }
        out.push(c);
    }
    out
}

/// The lengths of all cycles, fixed points included, in decreasing order.
pub fn cycle_type(p: &[u32]) -> Vec<usize> {
    let mut seen = vec![false; p.len()];
    let mut out = Vec::new();
    for s in 0..p.len() {
        if seen[s] {
            continue;
        }
        let mut len = 0;
        let mut x = s;
        while !seen[x] {
            seen[x] = true;
            len += 1;
            x = p[x] as usize;
        }
        out.push(len);
    }
    out.sort_unstable_by(|a, b| b.cmp(a));
    out
}

fn gcd(a: u128, b: u128) -> u128 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// The order (the least common multiple of the cycle lengths); `None` past `u128`.
pub fn order(p: &[u32]) -> Option<u128> {
    let mut l: u128 = 1;
    for c in cycle_type(p) {
        let c = c as u128;
        l = (l / gcd(l, c)).checked_mul(c)?;
    }
    Some(l)
}

/// The sign: +1 for even, -1 for odd.
pub fn sign(p: &[u32]) -> i64 {
    let odd = cycle_type(p).iter().filter(|&&c| c % 2 == 0).count() % 2;
    if odd == 1 {
        -1
    } else {
        1
    }
}

/// The largest moved point plus one (0 for the identity).
pub fn support_end(p: &[u32]) -> usize {
    (0..p.len()).rev().find(|&i| p[i] as usize != i).map_or(0, |i| i + 1)
}
