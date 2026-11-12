//! Combinatorics: finite posets, matroids, Young tableaux.
//!
//! - Posets (Stanley, *Enumerative Combinatorics* I, ch. 3): the order is the reflexive transitive closure of the
//!   given relations; the Moebius function by mu(x, x) = 1, mu(x, y) = -sum_(x <= z < y) mu(x, z); linear
//!   extensions by choosing a minimal element at each step, counted over the down-sets.
//! - Matroids (Oxley, *Matroid Theory*): a rank function on subsets of the ground set {0..m-1} (linear over Q,
//!   graphic, uniform, or from the bases); bases, circuits and the Tutte polynomial
//!   T(x, y) = sum_A (x - 1)^(r(E) - r(A)) (y - 1)^(|A| - r(A)) over all subsets.
//! - Young tableaux (Sagan, *The Symmetric Group*, ch. 3; Stanley, *Enumerative Combinatorics* II, 7.11-7.13):
//!   Robinson-Schensted(-Knuth) row insertion and its inverse, hook lengths, and the standard tableaux of a shape.

/// A partial order on {0..k-1}: `le[i][j]` when i <= j.
pub struct Poset {
    pub k: usize,
    pub le: Vec<Vec<bool>>,
}

impl Poset {
    /// The reflexive transitive closure of the pairs (a <= b); `None` when it is not antisymmetric.
    pub fn new(k: usize, pairs: &[(usize, usize)]) -> Option<Poset> {
        let mut le = vec![vec![false; k]; k];
        for (i, row) in le.iter_mut().enumerate() {
            row[i] = true;
        }
        for &(a, b) in pairs {
            le[a][b] = true;
        }
        for m in 0..k {
            for i in 0..k {
                if le[i][m] {
                    for j in 0..k {
                        if le[m][j] {
                            le[i][j] = true;
                        }
                    }
                }
            }
        }
        for i in 0..k {
            for j in 0..k {
                if i != j && le[i][j] && le[j][i] {
                    return None;
                }
            }
        }
        Some(Poset { k, le })
    }

    /// A linear order compatible with the poset (a topological order).
    fn topo(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.k).collect();
        // the number of elements below is strictly increasing along chains
        idx.sort_by_key(|&i| (0..self.k).filter(|&j| self.le[j][i]).count());
        idx
    }

    /// The Moebius function as a matrix.
    pub fn moebius(&self) -> Vec<Vec<i64>> {
        let k = self.k;
        let t = self.topo();
        let mut mu = vec![vec![0i64; k]; k];
        for x in 0..k {
            mu[x][x] = 1;
            for &y in &t {
                if y == x || !self.le[x][y] {
                    continue;
                }
                let mut s = 0i64;
                for z in 0..k {
                    if z != y && self.le[x][z] && self.le[z][y] {
                        s += mu[x][z];
                    }
                }
                mu[x][y] = -s;
            }
        }
        mu
    }

    /// The covering pairs x < y with nothing strictly between.
    pub fn covers(&self) -> Vec<(usize, usize)> {
        let k = self.k;
        let mut out = Vec::new();
        for x in 0..k {
            for y in 0..k {
                if x != y && self.le[x][y] && !(0..k).any(|z| z != x && z != y && self.le[x][z] && self.le[z][y]) {
                    out.push((x, y));
                }
            }
        }
        out
    }

    /// Every linear extension (as orders of the elements), up to `limit` of them; `None` past it.
    pub fn linear_extensions(&self, limit: usize) -> Option<Vec<Vec<usize>>> {
        let mut out = Vec::new();
        let mut cur = Vec::new();
        let mut used = vec![false; self.k];
        if self.ext(&mut cur, &mut used, &mut out, limit) {
            Some(out)
        } else {
            None
        }
    }

    fn ext(&self, cur: &mut Vec<usize>, used: &mut [bool], out: &mut Vec<Vec<usize>>, limit: usize) -> bool {
        if cur.len() == self.k {
            out.push(cur.clone());
            return out.len() <= limit;
        }
        for x in 0..self.k {
            if !used[x] && (0..self.k).all(|y| y == x || used[y] || !self.le[y][x]) {
                used[x] = true;
                cur.push(x);
                let ok = self.ext(cur, used, out, limit);
                cur.pop();
                used[x] = false;
                if !ok {
                    return false;
                }
            }
        }
        true
    }

    /// The number of linear extensions, by counting over the down-sets (k <= 26).
    pub fn linear_extension_count(&self) -> Option<u128> {
        let k = self.k;
        if k > 26 {
            return None;
        }
        let below: Vec<u32> = (0..k).map(|x| (0..k).filter(|&y| y != x && self.le[y][x]).fold(0u32, |m, y| m | 1 << y)).collect();
        let mut memo: std::collections::HashMap<u32, u128> = std::collections::HashMap::new();
        fn count(mask: u32, k: usize, below: &[u32], memo: &mut std::collections::HashMap<u32, u128>) -> u128 {
            if mask.count_ones() as usize == k {
                return 1;
            }
            if let Some(&v) = memo.get(&mask) {
                return v;
            }
            let mut s = 0u128;
            for x in 0..k {
                if mask & (1 << x) == 0 && below[x] & !mask == 0 {
                    s += count(mask | 1 << x, k, below, memo);
                }
            }
            memo.insert(mask, s);
            s
        }
        Some(count(0, k, &below, &mut memo))
    }

    /// Whether every pair has a least upper bound and a greatest lower bound.
    pub fn is_lattice(&self) -> bool {
        let k = self.k;
        let bound = |x: usize, y: usize, up: bool| -> bool {
            let rel = |a: usize, b: usize| if up { self.le[a][b] } else { self.le[b][a] };
            let ub: Vec<usize> = (0..k).filter(|&z| rel(x, z) && rel(y, z)).collect();
            ub.iter().any(|&z| ub.iter().all(|&w| rel(z, w)))
        };
        (0..k).all(|x| (0..k).all(|y| bound(x, y, true) && bound(x, y, false)))
    }
}

/// The rank function of a matroid on {0..m-1}.
pub enum Matroid {
    /// the columns of an integer matrix (rows)
    Linear(Vec<Vec<i128>>),
    /// the edges of a multigraph on vertices 0..v-1
    Graphic(usize, Vec<(usize, usize)>),
    Uniform(usize, usize),
    /// the bases, as bit masks
    Bases(usize, Vec<u64>),
}

fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 {
        a.abs()
    } else {
        gcd(b, a % b)
    }
}

/// The rank of the integer vectors (as rows), by fraction-free elimination with the rows reduced by their
/// contents; `None` on overflow.
pub fn int_rank(rows: &mut [Vec<i128>]) -> Option<usize> {
    let mut rank = 0;
    let cols = rows.first().map_or(0, |r| r.len());
    for c in 0..cols {
        let Some(p) = (rank..rows.len()).find(|&r| rows[r][c] != 0) else { continue };
        rows.swap(rank, p);
        for r in rank + 1..rows.len() {
            if rows[r][c] != 0 {
                let (a, b) = (rows[rank][c], rows[r][c]);
                for j in c..cols {
                    rows[r][j] = rows[r][j].checked_mul(a)?.checked_sub(rows[rank][j].checked_mul(b)?)?;
                }
                let g = rows[r].iter().fold(0, |g, &x| gcd(g, x));
                if g > 1 {
                    for x in rows[r].iter_mut() {
                        *x /= g;
                    }
                }
            }
        }
        rank += 1;
    }
    Some(rank)
}

impl Matroid {
    pub fn size(&self) -> usize {
        match self {
            Matroid::Linear(a) => a.first().map_or(0, |r| r.len()),
            Matroid::Graphic(_, e) => e.len(),
            Matroid::Uniform(_, m) => *m,
            Matroid::Bases(m, _) => *m,
        }
    }

    /// The rank of the subset `s` (a bit mask); `None` on overflow.
    pub fn rank(&self, s: u64) -> Option<usize> {
        match self {
            Matroid::Linear(a) => {
                let m = self.size();
                let mut rows: Vec<Vec<i128>> = (0..m).filter(|&j| s >> j & 1 == 1).map(|j| a.iter().map(|r| r[j]).collect()).collect();
                int_rank(&mut rows)
            }
            Matroid::Graphic(v, e) => {
                let mut p: Vec<usize> = (0..*v).collect();
                fn find(p: &mut [usize], mut x: usize) -> usize {
                    while p[x] != x {
                        p[x] = p[p[x]];
                        x = p[x];
                    }
                    x
                }
                let mut r = 0;
                for (j, &(a, b)) in e.iter().enumerate() {
                    if s >> j & 1 == 1 {
                        let (ra, rb) = (find(&mut p, a), find(&mut p, b));
                        if ra != rb {
                            p[ra] = rb;
                            r += 1;
                        }
                    }
                }
                Some(r)
            }
            Matroid::Uniform(r, _) => Some((s.count_ones() as usize).min(*r)),
            Matroid::Bases(_, bs) => Some(bs.iter().map(|b| (b & s).count_ones() as usize).max().unwrap_or(0)),
        }
    }

    /// The bases, as bit masks in increasing order of their sorted element lists.
    pub fn bases(&self) -> Option<Vec<u64>> {
        let m = self.size();
        let full = if m == 64 { u64::MAX } else { (1u64 << m) - 1 };
        let r = self.rank(full)?;
        let mut out = Vec::new();
        for s in combinations(m, r) {
            if self.rank(s)? == r {
                out.push(s);
            }
        }
        Some(out)
    }

    /// The circuits (minimal dependent sets), by size and then lexicographically.
    pub fn circuits(&self) -> Option<Vec<u64>> {
        let m = self.size();
        let mut out = Vec::new();
        for k in 1..=m {
            for s in combinations(m, k) {
                if self.rank(s)? == k - 1 && (0..m).filter(|&j| s >> j & 1 == 1).all(|j| self.rank(s & !(1 << j)) == Some(k - 1)) {
                    out.push(s);
                }
            }
        }
        Some(out)
    }

    /// The Tutte polynomial as coefficients t[i][j] of x^i y^j.
    pub fn tutte(&self) -> Option<Vec<Vec<i128>>> {
        let m = self.size();
        let full = (1u64 << m) - 1;
        let re = self.rank(full)?;
        // n[a][b]: subsets with corank a and nullity b
        let mut n = vec![vec![0i128; m + 1]; re + 1];
        for s in 0..=full {
            let r = self.rank(s)?;
            n[re - r][s.count_ones() as usize - r] += 1;
        }
        let binom = |a: usize, b: usize| -> i128 {
            let mut c: i128 = 1;
            for i in 0..b {
                c = c * (a - i) as i128 / (i + 1) as i128;
            }
            c
        };
        let mut t = vec![vec![0i128; m + 1]; re + 1];
        for a in 0..=re {
            for b in 0..=m {
                if n[a][b] == 0 {
                    continue;
                }
                for i in 0..=a {
                    for j in 0..=b {
                        let sg = if (a - i + b - j) % 2 == 0 { 1 } else { -1 };
                        t[i][j] += sg * n[a][b] * binom(a, i) * binom(b, j);
                    }
                }
            }
        }
        Some(t)
    }
}

/// The k-subsets of {0..m-1} as bit masks, in lexicographic order of their sorted element lists.
pub fn combinations(m: usize, k: usize) -> Vec<u64> {
    let mut out = Vec::new();
    let mut c: Vec<usize> = (0..k).collect();
    if k > m {
        return out;
    }
    loop {
        out.push(c.iter().fold(0u64, |s, &j| s | 1 << j));
        let mut i = k;
        loop {
            if i == 0 {
                return out;
            }
            i -= 1;
            if c[i] < m - k + i {
                c[i] += 1;
                for j in i + 1..k {
                    c[j] = c[j - 1] + 1;
                }
                break;
            }
            if i == 0 {
                return out;
            }
        }
    }
}

/// A tableau: rows of entries.
pub type Tableau<X> = Vec<Vec<X>>;

/// Robinson-Schensted-Knuth row insertion of the word: the insertion tableau P and the recording tableau Q.
pub fn rsk<X: PartialOrd + Clone>(w: &[X]) -> (Tableau<X>, Tableau<usize>) {
    let mut p: Tableau<X> = Vec::new();
    let mut q: Tableau<usize> = Vec::new();
    for (step, x) in w.iter().enumerate() {
        let mut x = x.clone();
        let mut r = 0;
        loop {
            if r == p.len() {
                p.push(vec![x]);
                q.push(vec![step + 1]);
                break;
            }
            match p[r].iter().position(|y| *y > x) {
                Some(j) => {
                    x = std::mem::replace(&mut p[r][j], x);
                    r += 1;
                }
                None => {
                    p[r].push(x);
                    q[r].push(step + 1);
                    break;
                }
            }
        }
    }
    (p, q)
}

/// The inverse of [`rsk`]: the word whose insertion and recording tableaux are p and q (q standard).
pub fn rsk_inverse<X: PartialOrd + Clone>(p: &Tableau<X>, q: &Tableau<usize>) -> Option<Vec<X>> {
    let mut p = p.clone();
    let mut q = q.clone();
    let n: usize = q.iter().map(|r| r.len()).sum();
    if p.iter().map(|r| r.len()).collect::<Vec<_>>() != q.iter().map(|r| r.len()).collect::<Vec<_>>() {
        return None;
    }
    let mut w = Vec::with_capacity(n);
    for step in (1..=n).rev() {
        let r = q.iter().position(|row| row.last() == Some(&step))?;
        q[r].pop();
        let mut x = p[r].pop()?;
        for rr in (0..r).rev() {
            // the largest entry strictly less than x
            let j = p[rr].iter().rposition(|y| *y < x)?;
            x = std::mem::replace(&mut p[rr][j], x);
        }
        if q[r].is_empty() {
            q.pop();
            p.pop();
        }
        w.push(x);
    }
    w.reverse();
    Some(w)
}

/// The hook lengths of the shape.
pub fn hooks(shape: &[usize]) -> Vec<Vec<usize>> {
    let conj = |j: usize| shape.iter().filter(|&&l| l > j).count();
    shape.iter().enumerate().map(|(i, &l)| (0..l).map(|j| l - j + conj(j) - i - 1).collect()).collect()
}

/// The standard Young tableaux of the shape, up to `limit`; `None` past it. The entries are placed largest first
/// at removable corners, so the list comes out in a fixed order (sorted afterwards by the rows read in order).
pub fn standard_tableaux(shape: &[usize], limit: usize) -> Option<Vec<Tableau<usize>>> {
    let n: usize = shape.iter().sum();
    let mut out = Vec::new();
    let mut t: Vec<Vec<usize>> = shape.iter().map(|&l| vec![0; l]).collect();
    let mut sh = shape.to_vec();
    fn rec(sh: &mut Vec<usize>, k: usize, t: &mut Vec<Vec<usize>>, out: &mut Vec<Tableau<usize>>, limit: usize) -> bool {
        if k == 0 {
            out.push(t.clone());
            return out.len() <= limit;
        }
        for r in 0..sh.len() {
            let l = sh[r];
            if l > 0 && (r + 1 == sh.len() || sh[r + 1] < l) {
                t[r][l - 1] = k;
                sh[r] -= 1;
                let ok = rec(sh, k - 1, t, out, limit);
                sh[r] += 1;
                if !ok {
                    return false;
                }
            }
        }
        true
    }
    if !rec(&mut sh, n, &mut t, &mut out, limit) {
        return None;
    }
    out.sort();
    Some(out)
}

/// The partitions of n in decreasing lexicographic order.
pub fn partitions(n: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    fn rec(n: usize, max: usize, cur: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if n == 0 {
            out.push(cur.clone());
            return;
        }
        for k in (1..=n.min(max)).rev() {
            cur.push(k);
            rec(n - k, k, cur, out);
            cur.pop();
        }
    }
    rec(n, n, &mut Vec::new(), &mut out);
    out
}
