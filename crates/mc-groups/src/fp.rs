//! Finitely presented groups: coset enumeration, rewriting systems and abelian invariants.
//!
//! Letters: generator i is 2i, its inverse 2i + 1 (so `x ^ 1` is the inverse letter).
//!
//! - Coset enumeration by the HLT strategy of Haselgrove, Leech and Trotter with the coincidence procedure of Holt,
//!   Eick and O'Brien, *Handbook of Computational Group Theory*, 5.1-5.2 (scan and fill each relator at each live
//!   coset in order, then fill the row); the table is compressed and standardised (cosets numbered in the order a
//!   breadth-first walk over the columns meets them; Sims, *Computation with Finitely Presented Groups*, 4.7).
//! - Knuth-Bendix completion for the shortlex order on the letters a < a^-1 < b < b^-1 < ... (Sims, ch. 2-3; Holt
//!   et al. 12.2): critical pairs from the overlaps of left-hand sides, with interreduction after each pass.
//! - Abelian invariants from the Smith normal form of the matrix of exponent sums of the relators.
use crate::Ctl;
use mc_eval::Res;

/// A word: letters as described in the module documentation.
pub type Word = Vec<u32>;

/// The inverse word.
pub fn inverse(w: &[u32]) -> Word {
    w.iter().rev().map(|&x| x ^ 1).collect()
}

/// Freely reduced.
pub fn free_reduce(w: &[u32]) -> Word {
    let mut out: Word = Vec::with_capacity(w.len());
    for &x in w {
        if out.last() == Some(&(x ^ 1)) {
            out.pop();
        } else {
            out.push(x);
        }
    }
    out
}

/// Why an enumeration stopped.
pub enum Stop {
    /// more cosets than allowed were defined
    Limit,
}

/// A complete coset table: `rows[c][x]` the coset c x (0-based), columns for the 2 m letters.
pub struct Table {
    pub rows: Vec<Vec<u32>>,
}

const NONE: u32 = u32::MAX;

struct Enum<'a> {
    cols: usize,
    t: Vec<u32>,
    /// union-find parents; p[c] == c for live cosets
    p: Vec<u32>,
    count: usize,
    limit: usize,
    ctl: &'a Ctl,
}

impl Enum<'_> {
    fn get(&self, c: u32, x: u32) -> u32 {
        self.t[c as usize * self.cols + x as usize]
    }

    fn set(&mut self, c: u32, x: u32, d: u32) {
        self.t[c as usize * self.cols + x as usize] = d;
    }

    fn define(&mut self, c: u32, x: u32) -> Result<(), Stop> {
        if self.count >= self.limit {
            return Err(Stop::Limit);
        }
        let d = self.p.len() as u32;
        self.p.push(d);
        self.t.extend(std::iter::repeat_n(NONE, self.cols));
        self.count += 1;
        self.set(c, x, d);
        self.set(d, x ^ 1, c);
        Ok(())
    }

    fn rep(&mut self, k: u32) -> u32 {
        let mut l = k;
        while self.p[l as usize] != l {
            l = self.p[l as usize];
        }
        let mut m = k;
        while self.p[m as usize] != l {
            let next = self.p[m as usize];
            self.p[m as usize] = l;
            m = next;
        }
        l
    }

    fn merge(&mut self, k: u32, l: u32, q: &mut Vec<u32>) {
        let (k1, l1) = (self.rep(k), self.rep(l));
        if k1 == l1 {
            return;
        }
        let (m, d) = (k1.min(l1), k1.max(l1));
        self.p[d as usize] = m;
        q.push(d);
        self.count -= 1;
    }

    fn coincidence(&mut self, a: u32, b: u32) {
        let mut q = Vec::new();
        self.merge(a, b, &mut q);
        let mut i = 0;
        while i < q.len() {
            let e = q[i];
            i += 1;
            for x in 0..self.cols as u32 {
                let f = self.get(e, x);
                if f == NONE {
                    continue;
                }
                if self.get(f, x ^ 1) == e {
                    self.set(f, x ^ 1, NONE);
                }
                let (e1, f1) = (self.rep(e), self.rep(f));
                if self.get(e1, x) != NONE {
                    let g = self.get(e1, x);
                    self.merge(f1, g, &mut q);
                } else if self.get(f1, x ^ 1) != NONE {
                    let g = self.get(f1, x ^ 1);
                    self.merge(e1, g, &mut q);
                } else {
                    self.set(e1, x, f1);
                    self.set(f1, x ^ 1, e1);
                }
            }
        }
    }

    fn scan_and_fill(&mut self, c: u32, w: &[u32]) -> Result<(), Stop> {
        if w.is_empty() {
            return Ok(());
        }
        let mut f = c;
        let mut b = c;
        let mut i = 0usize;
        let mut j = w.len() as isize - 1;
        loop {
            while (i as isize) <= j && self.get(f, w[i]) != NONE {
                f = self.get(f, w[i]);
                i += 1;
            }
            if (i as isize) > j {
                if f != b {
                    self.coincidence(f, b);
                }
                return Ok(());
            }
            while j >= i as isize && self.get(b, w[j as usize] ^ 1) != NONE {
                b = self.get(b, w[j as usize] ^ 1);
                j -= 1;
            }
            if j < i as isize {
                self.coincidence(f, b);
                return Ok(());
            } else if j == i as isize {
                self.set(f, w[i], b);
                self.set(b, w[i] ^ 1, f);
                return Ok(());
            } else {
                self.define(f, w[i])?;
            }
        }
    }
}

/// Enumerate the cosets of the subgroup generated by `sub` in the group with `m` generators and the relators
/// `rels`; `None` when more than `limit` cosets are needed at once.
pub fn enumerate(m: usize, rels: &[Word], sub: &[Word], limit: usize, ctl: &Ctl) -> Res<Option<Table>> {
    let cols = 2 * m;
    let mut e = Enum { cols, t: vec![NONE; cols], p: vec![0], count: 1, limit, ctl };
    // relators and their cyclic conjugates are all scanned at every coset, so the plain relators suffice
    let rels: Vec<Word> = rels.iter().map(|r| free_reduce(r)).filter(|r| !r.is_empty()).collect();
    for w in sub {
        if e.scan_and_fill(0, &free_reduce(w)).is_err() {
            return Ok(None);
        }
    }
    let mut c: u32 = 0;
    while (c as usize) < e.p.len() {
        if e.p[c as usize] == c {
            for r in &rels {
                if e.p[c as usize] != c {
                    break;
                }
                if e.scan_and_fill(c, r).is_err() {
                    return Ok(None);
                }
            }
            if e.p[c as usize] == c {
                for x in 0..cols as u32 {
                    if e.get(c, x) == NONE && e.define(c, x).is_err() {
                        return Ok(None);
                    }
                }
            }
        }
        if c.is_multiple_of(1024) {
            e.ctl.step(1024)?;
        }
        c += 1;
    }
    // standardise: breadth first from coset 0 over the live cosets
    let mut num = vec![NONE; e.p.len()];
    let mut order = vec![0u32];
    num[0] = 0;
    let mut k = 0;
    while k < order.len() {
        let c = order[k];
        for x in 0..cols as u32 {
            let d = e.get(c, x);
            if d == NONE {
                return Ok(None);
            }
            let d = e.rep(d);
            if num[d as usize] == NONE {
                num[d as usize] = order.len() as u32;
                order.push(d);
            }
        }
        k += 1;
    }
    let mut rows = Vec::with_capacity(order.len());
    for &c in &order {
        let mut row = Vec::with_capacity(cols);
        for x in 0..cols as u32 {
            let d = e.get(c, x);
            if d == NONE {
                return Ok(None);
            }
            let d = e.rep(d);
            row.push(num[d as usize]);
        }
        rows.push(row);
    }
    Ok(Some(Table { rows }))
}

/// Shortlex comparison: shorter first, then letter by letter.
fn shortlex(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// A confluent rewriting system: rules l -> r with l > r in shortlex.
pub struct Rewriting {
    pub rules: Vec<(Word, Word)>,
}

impl Rewriting {
    /// The normal form of w.
    pub fn reduce(&self, w: &[u32]) -> Word {
        let mut w = w.to_vec();
        'outer: loop {
            for (l, r) in &self.rules {
                if l.len() <= w.len() {
                    if let Some(i) = (0..=w.len() - l.len()).find(|&i| w[i..i + l.len()] == l[..]) {
                        let mut nw = w[..i].to_vec();
                        nw.extend_from_slice(r);
                        nw.extend_from_slice(&w[i + l.len()..]);
                        w = nw;
                        continue 'outer;
                    }
                }
            }
            return w;
        }
    }
}

/// Knuth-Bendix completion of the group's rewriting system; `None` past `max_rules` rules.
pub fn knuth_bendix(m: usize, rels: &[Word], max_rules: usize, ctl: &Ctl) -> Res<Option<Rewriting>> {
    let mut sys = Rewriting { rules: Vec::new() };
    let mut pending: Vec<(Word, Word)> = Vec::new();
    for x in 0..2 * m as u32 {
        pending.push((vec![x, x ^ 1], Vec::new()));
    }
    for r in rels {
        pending.push((free_reduce(r), Vec::new()));
    }
    loop {
        // add the pending equations, interreducing
        while let Some((a, b)) = pending.pop() {
            ctl.step(1)?;
            let (a, b) = (sys.reduce(&a), sys.reduce(&b));
            if a == b {
                continue;
            }
            let (l, r) = if shortlex(&a, &b) == std::cmp::Ordering::Greater { (a, b) } else { (b, a) };
            // rules whose left side contains l become equations again
            let mut keep = Vec::new();
            for (l2, r2) in sys.rules.drain(..) {
                if l2.len() >= l.len() && (0..=l2.len() - l.len()).any(|i| l2[i..i + l.len()] == l[..]) {
                    pending.push((l2, r2));
                } else {
                    keep.push((l2, r2));
                }
            }
            sys.rules = keep;
            sys.rules.push((l, r));
            // right sides in normal form
            for k in 0..sys.rules.len() {
                let r2 = sys.reduce(&sys.rules[k].1.clone());
                sys.rules[k].1 = r2;
            }
            if sys.rules.len() > max_rules {
                return Ok(None);
            }
        }
        // critical pairs
        let n = sys.rules.len();
        for i in 0..n {
            for j in 0..n {
                let (l1, r1) = &sys.rules[i];
                let (l2, r2) = &sys.rules[j];
                for k in 1..l1.len().min(l2.len()) {
                    if l1[l1.len() - k..] == l2[..k] {
                        // l1 = u v, l2 = v w: u r2 and r1 w
                        let mut a = l1[..l1.len() - k].to_vec();
                        a.extend_from_slice(r2);
                        let mut b = r1.clone();
                        b.extend_from_slice(&l2[k..]);
                        let (a, b) = (sys.reduce(&a), sys.reduce(&b));
                        if a != b {
                            pending.push((a, b));
                        }
                    }
                }
            }
        }
        if pending.is_empty() {
            sys.rules.sort_by(|a, b| shortlex(&a.0, &b.0));
            return Ok(Some(sys));
        }
        if pending.len() > 50 * max_rules {
            return Ok(None);
        }
    }
}

/// The elementary divisors of Z^m / (row space of the exponent-sum matrix): prime powers in increasing order, then
/// a 0 for each infinite cyclic factor; `None` on overflow.
pub fn abelian_invariants(m: usize, rels: &[Word]) -> Option<Vec<u128>> {
    let mut a: Vec<Vec<i128>> = rels
        .iter()
        .map(|r| {
            let mut row = vec![0i128; m];
            for &x in r {
                row[(x / 2) as usize] += if x % 2 == 0 { 1 } else { -1 };
            }
            row
        })
        .collect();
    let diag = smith(&mut a, m)?;
    let mut out = Vec::new();
    let mut zeros = m - diag.len();
    for d in diag {
        let d = d.unsigned_abs();
        if d == 0 {
            zeros += 1;
            continue;
        }
        let mut r = d;
        let mut p = 2u128;
        while r > 1 {
            let mut q = 1u128;
            while r % p == 0 {
                r /= p;
                q *= p;
            }
            if q > 1 {
                out.push(q);
            }
            p += 1;
        }
    }
    out.sort_unstable();
    out.extend(std::iter::repeat_n(0, zeros));
    Some(out)
}

/// The nonzero diagonal of the Smith normal form of the rows a (m columns), by unimodular row and column
/// operations; the zero diagonal entries are the columns not reached.
fn smith(a: &mut [Vec<i128>], m: usize) -> Option<Vec<i128>> {
    let mut diag = Vec::new();
    let mut t = 0;
    loop {
        // the entry of least absolute value among rows >= t, columns >= t
        let mut best: Option<(usize, usize)> = None;
        for (i, row) in a.iter().enumerate().skip(t) {
            for (j, &v) in row.iter().enumerate().skip(t) {
                if v != 0 && best.is_none_or(|(bi, bj)| v.abs() < a[bi][bj].abs()) {
                    best = Some((i, j));
                }
            }
        }
        let Some((bi, bj)) = best else { return Some(diag) };
        a.swap(t, bi);
        for row in a.iter_mut() {
            row.swap(t, bj);
        }
        let p = a[t][t];
        let mut clean = true;
        for i in t + 1..a.len() {
            let q = a[i][t] / p;
            if q != 0 {
                for j in t..m {
                    a[i][j] = a[i][j].checked_sub(q.checked_mul(a[t][j])?)?;
                }
            }
            if a[i][t] != 0 {
                clean = false;
            }
        }
        for j in t + 1..m {
            let q = a[t][j] / p;
            if q != 0 {
                for row in a.iter_mut().skip(t) {
                    row[j] = row[j].checked_sub(q.checked_mul(row[t])?)?;
                }
            }
            if a[t][j] != 0 {
                clean = false;
            }
        }
        if !clean {
            continue;
        }
        // p must divide every remaining entry; else add the offending row and repeat
        let mut bad = None;
        'f: for i in t + 1..a.len() {
            for j in t + 1..m {
                if a[i][j] % p != 0 {
                    bad = Some(i);
                    break 'f;
                }
            }
        }
        if let Some(i) = bad {
            for j in t..m {
                let v = a[i][j];
                a[t][j] = a[t][j].checked_add(v)?;
            }
            continue;
        }
        diag.push(p.abs());
        t += 1;
        if t >= m || t >= a.len() {
            return Some(diag);
        }
    }
}
