//! Backtrack search through a stabiliser chain (Seress, *Permutation Group Algorithms*, 9.1; Butler,
//! *Fundamental Algorithms for Permutation Groups*, ch. 10): the elements of G are the leaves of the tree of base
//! images, and a property is searched for by walking the tree with pruning on partial base images.
//!
//! - Subgroup search (centralisers, set stabilisers, intersections): levels are treated from the bottom up; at
//!   level i only the images that are least in their orbit under the part K^(i) of the subgroup found so far are
//!   tried, since the subgroup's elements mapping b_i into one K^(i)-orbit form one union of cosets.
//! - Coset search (conjugating elements): the same tree without the subgroup pruning, stopping at the first leaf.
//!
//! Pruning for "a^h = b" (centralisers and conjugacy): h maps the cycles of a onto those of b, so a point and its
//! image have equal cycle lengths, and b_m = a(b_l) for two base points forces c_m = b(c_l); the base is laid out
//! along the cycles of a, which makes most images forced.
use crate::chain::Chain;
use crate::groups::Group;
use crate::perm::{self, Perm};
use crate::Ctl;
use mc_eval::Res;

/// A condition on the elements searched for.
pub enum Cons {
    /// a^h = b (h^-1 a h = b)
    Conj { a: Perm, b: Perm, ainv: Perm, lena: Vec<u32>, lenb: Vec<u32> },
    /// h maps the set to itself
    SetStab(Vec<bool>),
    /// h lies in the group with this chain (whose base begins with the search base)
    InGroup(Chain),
}

fn cycle_lengths(p: &[u32]) -> Vec<u32> {
    let mut out = vec![0; p.len()];
    for c in perm::cycles(p) {
        for &x in &c {
            out[x as usize] = c.len() as u32;
        }
    }
    for (i, l) in out.iter_mut().enumerate() {
        if *l == 0 {
            let _ = i;
            *l = 1;
        }
    }
    out
}

impl Cons {
    pub fn conj(a: &[u32], b: &[u32]) -> Cons {
        Cons::Conj { a: a.to_vec(), b: b.to_vec(), ainv: perm::inv(a), lena: cycle_lengths(a), lenb: cycle_lengths(b) }
    }

    fn holds(&self, h: &[u32]) -> bool {
        match self {
            Cons::Conj { a, b, .. } => perm::conj(a, h) == *b,
            Cons::SetStab(s) => (0..h.len()).all(|x| s[x] == s[h[x] as usize]),
            Cons::InGroup(c) => c.contains(h),
        }
    }

    /// Whether the images `c[0..=j]` of the base points `base[0..=j]` can still be completed. `pos[x]` is the
    /// level of the base point x (or `usize::MAX`).
    fn ok(&self, j: usize, base: &[u32], c: &[u32], pos: &[usize]) -> bool {
        match self {
            Cons::Conj { a, b, ainv, lena, lenb } => {
                let (bj, cj) = (base[j] as usize, c[j] as usize);
                if lena[bj] != lenb[cj] {
                    return false;
                }
                let m = pos[a[bj] as usize];
                if m <= j && c[m] != b[cj] {
                    return false;
                }
                let l = pos[ainv[bj] as usize];
                if l <= j && b[c[l] as usize] as usize != cj {
                    return false;
                }
                true
            }
            Cons::SetStab(s) => s[base[j] as usize] == s[c[j] as usize],
            Cons::InGroup(h) => {
                // an element of H with base[l] -> c[l] for l <= j: peel off transversal elements level by level
                let mut t = perm::identity(h.n);
                for l in 0..=j {
                    let Some(lv) = h.levels.get(l) else {
                        // past H's base the remaining images are fixed
                        let ti = perm::inv(&t);
                        return (l..=j).all(|m| ti[c[m] as usize] == base[m]);
                    };
                    debug_assert_eq!(lv.base, base[l]);
                    // x = c_l under t^-1 must be in the orbit; then t <- u_x t
                    let ti = perm::inv(&t);
                    let x = ti[c[l] as usize];
                    match &lv.trans[x as usize] {
                        Some(u) => t = perm::mul(u, &t),
                        None => return false,
                    }
                }
                true
            }
        }
    }
}

/// A base prefix suited to the conditions: along the cycles of the first conjugation condition, the set of a set
/// stabiliser, or the base of a group condition.
fn prefix(n: usize, cons: &[Cons]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut used = vec![false; n];
    for c in cons {
        match c {
            Cons::Conj { a, .. } => {
                let mut cs = perm::cycles(a);
                cs.sort_by_key(|c| std::cmp::Reverse(c.len()));
                for cy in cs {
                    for x in cy {
                        if !used[x as usize] {
                            used[x as usize] = true;
                            out.push(x);
                        }
                    }
                }
            }
            Cons::SetStab(s) => {
                for x in 0..n {
                    if s[x] && !used[x] {
                        used[x] = true;
                        out.push(x as u32);
                    }
                }
            }
            Cons::InGroup(_) => {}
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

struct Tree<'a> {
    g: Chain,
    pos: Vec<usize>,
    base: Vec<u32>,
    cons: &'a [Cons],
    ctl: &'a Ctl,
}

impl Tree<'_> {
    fn dfs(&self, j: usize, r: &Perm, c: &mut Vec<u32>) -> Res<Option<Perm>> {
        if j == self.base.len() {
            return Ok(self.cons.iter().all(|k| k.holds(r)).then(|| r.clone()));
        }
        self.ctl.step(1)?;
        let lv = &self.g.levels[j];
        for &x in &lv.orbit {
            c[j] = r[x as usize];
            if !self.cons.iter().all(|k| k.ok(j, &self.base, c, &self.pos)) {
                continue;
            }
            let u = lv.trans[x as usize].as_ref().expect("orbit points have transversal elements");
            let r2 = perm::mul(u, r);
            if let Some(h) = self.dfs(j + 1, &r2, c)? {
                return Ok(Some(h));
            }
        }
        Ok(None)
    }
}

/// Re-base the group conditions on the search base.
fn rebase(cons: Vec<Cons>, base: &[u32], ctl: &Ctl) -> Res<Vec<Cons>> {
    let mut out = Vec::new();
    for c in cons {
        out.push(match c {
            Cons::InGroup(h) => Cons::InGroup(Chain::new(h.n, &h.strong_gens(), base, ctl)?),
            other => other,
        });
    }
    Ok(out)
}

fn tree<'a>(g: &Group, cons: &'a [Cons], ctl: &'a Ctl) -> Res<Tree<'a>> {
    let pre = prefix(g.n, cons);
    let chain = g.with_base(&pre, ctl)?;
    let base = chain.base();
    let mut pos = vec![usize::MAX; g.n];
    for (i, &b) in base.iter().enumerate() {
        pos[b as usize] = i;
    }
    Ok(Tree { g: chain, pos, base, cons, ctl })
}

/// The subgroup of the elements of g satisfying every condition.
pub fn subgroup(g: &Group, cons: Vec<Cons>, ctl: &Ctl) -> Res<Group> {
    let pre = prefix(g.n, &cons);
    let chain = g.with_base(&pre, ctl)?;
    let base = chain.base();
    let cons = rebase(cons, &base, ctl)?;
    let t = tree(g, &cons, ctl)?;
    debug_assert_eq!(t.base, base);
    let k = base.len();
    let mut found = Chain::new(g.n, &[], &base, ctl)?;
    let mut gens: Vec<Perm> = Vec::new();
    let mut c: Vec<u32> = base.clone();
    for i in (0..k).rev() {
        let mut orbit = t.g.levels[i].orbit.clone();
        orbit.sort_unstable();
        for &x in &orbit {
            if x == base[i] {
                continue;
            }
            // x must be least in its orbit under K^(i)
            let kg = found.gens_from(i);
            let o = crate::groups::orbit(g.n, &kg, x);
            if o.iter().any(|&y| y < x) {
                continue;
            }
            if o.contains(&base[i]) {
                continue;
            }
            c[..i].copy_from_slice(&base[..i]);
            c[i] = x;
            if !cons.iter().all(|kk| kk.ok(i, &base, &c, &t.pos)) {
                continue;
            }
            let u = t.g.levels[i].trans[x as usize].as_ref().expect("orbit points have transversal elements").clone();
            if let Some(h) = t.dfs(i + 1, &u, &mut c)? {
                found.extend(0, h.clone(), ctl)?;
                gens.push(h);
            }
        }
    }
    let chain = Chain::new(g.n, &gens, &[], ctl)?;
    Ok(Group { n: g.n, gens, chain })
}

/// An element of g satisfying every condition, if there is one.
pub fn element(g: &Group, cons: Vec<Cons>, ctl: &Ctl) -> Res<Option<Perm>> {
    let pre = prefix(g.n, &cons);
    let chain = g.with_base(&pre, ctl)?;
    let base = chain.base();
    let cons = rebase(cons, &base, ctl)?;
    let t = tree(g, &cons, ctl)?;
    let mut c = base.clone();
    if base.is_empty() {
        let id = perm::identity(g.n);
        return Ok(cons.iter().all(|k| k.holds(&id)).then_some(id));
    }
    t.dfs(0, &perm::identity(g.n), &mut c)
}

/// The centraliser in g of the elements xs.
pub fn centralizer(g: &Group, xs: &[Perm], ctl: &Ctl) -> Res<Group> {
    let cons: Vec<Cons> = xs.iter().filter(|x| !perm::is_identity(x)).map(|x| Cons::conj(x, x)).collect();
    if cons.is_empty() {
        return Ok(g.clone());
    }
    subgroup(g, cons, ctl)
}

/// An element h of g with h^-1 a h = b.
pub fn conjugator(g: &Group, a: &[u32], b: &[u32], ctl: &Ctl) -> Res<Option<Perm>> {
    if perm::cycle_type(a) != perm::cycle_type(b) {
        return Ok(None);
    }
    element(g, vec![Cons::conj(a, b)], ctl)
}

/// The stabiliser in g of the set s.
pub fn set_stabilizer(g: &Group, s: &[u32], ctl: &Ctl) -> Res<Group> {
    let mut m = vec![false; g.n];
    for &x in s {
        m[x as usize] = true;
    }
    subgroup(g, vec![Cons::SetStab(m)], ctl)
}

/// The intersection of g and h.
pub fn intersection(g: &Group, h: &Group, ctl: &Ctl) -> Res<Group> {
    subgroup(g, vec![Cons::InGroup(h.chain.clone())], ctl)
}
