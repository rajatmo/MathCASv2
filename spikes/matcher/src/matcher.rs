//! Matching (semantics 6.1, 6.2) by backtracking in continuation-passing style: `m(p, t, b, k)` tries every way `p`
//! matches `t` in the enumeration order, extending the bindings `b`, and calls the continuation `k` for each; it
//! stops at the first `k` that returns true. Bindings are undone on the way back, so a caller that wants the first
//! match copies them inside its continuation. This one shape handles nested AC patterns, sequence variables,
//! alternatives and conditions uniformly: a later failure resumes the enumeration of an earlier, nested match.
//!
//! Two strategies for arguments of AC heads, selected by [`Matcher::smart`]; everything else is shared:
//!
//! - **naive**: each pattern argument in turn tries every unused subject argument (single patterns) or every subset
//!   of unused arguments (sequence variables); only the final "all arguments used" test and the arity bounds prune.
//! - **smart** (constraint-driven, after Eker 1995): before the search, a candidate set per single pattern argument
//!   (literals by identity, fixed-head patterns by head and an independent match test, typed variables by type, bound
//!   variables by identity), then a bipartite-matching (Hall) test that every single pattern can get a distinct
//!   candidate; during the search, candidates only, a per-step test that every remaining single pattern still has an
//!   unused candidate, the last sequence variable takes the rest without enumeration, and of identical subject
//!   arguments only the first unused copy is tried.
//!
//! Every pruning step only removes branches without solutions, and both strategies walk the same order, so their
//! first matches are identical (checked by the tests and by every workload). Duplicate copies do produce duplicate
//! matches in the naive strategy; the bindings, which are what the order is defined on, are the same.
use crate::pat::{has_type, AppInfo, Pat};
use crate::store::{Store, Term};
use crate::Limit;

/// What conditions can see besides the bindings: the store, and own values of symbols (each read is recorded by
/// the evaluator for its read set, decision 13.1).
pub trait Env {
    fn st(&self) -> &Store;
    fn value(&self, sym: Term) -> Option<Term>;
}

/// An environment without own values, for matching outside an evaluator (`cases`, tests).
pub struct PlainEnv<'a>(pub &'a Store);

impl Env for PlainEnv<'_> {
    fn st(&self) -> &Store {
        self.0
    }
    fn value(&self, _: Term) -> Option<Term> {
        None
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Val {
    Unbound,
    One(Term),
    /// A sequence: offset and length in [`Binds::seq`].
    Seq(u32, u32),
}

/// Pattern-variable bindings. Sequences live in one shared buffer used as a stack, so binding a sequence allocates
/// nothing once the buffer has grown.
#[derive(Clone, Default, Debug)]
pub struct Binds {
    vals: Vec<Val>,
    seq: Vec<Term>,
}

impl Binds {
    pub fn reset(&mut self, n: usize) {
        self.vals.clear();
        self.vals.resize(n, Val::Unbound);
        self.seq.clear();
    }
    pub fn get(&self, v: u16) -> Val {
        self.vals[v as usize]
    }
    /// The term bound to a single variable.
    pub fn one(&self, v: u16) -> Term {
        match self.vals[v as usize] {
            Val::One(t) => t,
            other => panic!("variable {v} is not bound to one term: {other:?}"),
        }
    }
    /// The terms bound to a sequence variable.
    pub fn seq(&self, v: u16) -> &[Term] {
        match self.vals[v as usize] {
            Val::Seq(o, n) => &self.seq[o as usize..(o + n) as usize],
            other => panic!("variable {v} is not bound to a sequence: {other:?}"),
        }
    }
    pub fn n_vars(&self) -> usize {
        self.vals.len()
    }
    /// Copy without giving up `self`'s capacity.
    pub fn copy_from(&mut self, o: &Binds) {
        self.vals.clone_from(&o.vals);
        self.seq.clone_from(&o.seq);
    }
    /// A store-independent fingerprint of the bindings (for comparing matchers).
    pub fn fingerprint(&self, st: &Store) -> u64 {
        let mut h = 0x1234_5678;
        for v in &self.vals {
            h = crate::combine(
                h,
                match *v {
                    Val::Unbound => 7,
                    Val::One(t) => crate::fingerprint(st, t),
                    Val::Seq(o, n) => self.seq[o as usize..(o + n) as usize]
                        .iter()
                        .fold(11, |h, &t| crate::combine(h, crate::fingerprint(st, t))),
                },
            );
        }
        h
    }
}

/// The continuation: called with the bindings of each match found; returning true stops the enumeration.
pub type K<'k> = &'k mut dyn FnMut(&mut Binds) -> bool;

pub struct Matcher<'a> {
    pub env: &'a dyn Env,
    pub smart: bool,
    /// Naive strategy only: the last sequence variable takes the remaining arguments instead of enumerating
    /// subsets (isolates the effect of that one pruning step; the smart strategy always does it).
    pub rest: bool,
    pub limit: &'a Limit,
}

/// Bitmask of the first `n` positions.
#[inline]
fn all(n: usize) -> u64 {
    if n == 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    }
}

/// Spike limit: AC argument sets are bitmasks. A real implementation uses a bit set.
const MAX_AC: usize = 64;
/// Spike limit on the number of arguments of an AC pattern (candidate sets live on the stack).
const MAX_AC_PAT: usize = 32;

impl<'a> Matcher<'a> {
    pub fn new(env: &'a dyn Env, smart: bool, limit: &'a Limit) -> Matcher<'a> {
        Matcher { env, smart, rest: false, limit }
    }

    pub fn with_rest(mut self, rest: bool) -> Matcher<'a> {
        self.rest = rest;
        self
    }

    #[inline]
    fn st(&self) -> &'a Store {
        self.env.st()
    }

    /// The first match of `p` against `t`, copied into `out`. `scratch` is a reusable work area. Returns false when
    /// there is no match or a limit stopped the search (check the limit).
    pub fn first(&self, p: &Pat, nvars: usize, t: Term, scratch: &mut Binds, out: &mut Binds) -> bool {
        scratch.reset(nvars);
        let found = self.m(p, t, scratch, &mut |b| {
            out.copy_from(b);
            true
        });
        found && !self.limit.hit()
    }

    /// Does `p` match `t` at all?
    pub fn matches(&self, p: &Pat, nvars: usize, t: Term, scratch: &mut Binds) -> bool {
        scratch.reset(nvars);
        self.m(p, t, scratch, &mut |_| true) && !self.limit.hit()
    }

    /// Enumerate all matches (for tests): `f` gets each one's bindings.
    pub fn all_matches(&self, p: &Pat, nvars: usize, t: Term, mut f: impl FnMut(&Binds)) {
        let mut b = Binds::default();
        b.reset(nvars);
        self.m(p, t, &mut b, &mut |b| {
            f(b);
            false
        });
    }

    fn bind_one(&self, var: Option<u16>, t: Term, b: &mut Binds, k: K) -> bool {
        let Some(v) = var else { return k(b) };
        match b.vals[v as usize] {
            Val::Unbound => {
                b.vals[v as usize] = Val::One(t);
                let r = k(b);
                b.vals[v as usize] = Val::Unbound;
                r
            }
            Val::One(u) => u == t && k(b),
            Val::Seq(..) => false,
        }
    }

    /// Bind a sequence variable to `ts` (an argument run, or the chosen arguments under an AC head).
    fn bind_seq(&self, var: Option<u16>, ts: &mut dyn Iterator<Item = Term>, b: &mut Binds, k: K) -> bool {
        let Some(v) = var else { return k(b) };
        match b.vals[v as usize] {
            Val::Unbound => {
                let off = b.seq.len();
                b.seq.extend(ts);
                b.vals[v as usize] = Val::Seq(off as u32, (b.seq.len() - off) as u32);
                let r = k(b);
                b.vals[v as usize] = Val::Unbound;
                b.seq.truncate(off);
                r
            }
            Val::Seq(o, n) => {
                let same = {
                    let old = &b.seq[o as usize..(o + n) as usize];
                    let mut i = 0;
                    let mut ok = true;
                    for t in ts {
                        if i >= old.len() || old[i] != t {
                            ok = false;
                            break;
                        }
                        i += 1;
                    }
                    ok && i == old.len()
                };
                same && k(b)
            }
            Val::One(_) => false,
        }
    }

    /// Match `p` against `t`, calling `k` for each match.
    pub fn m(&self, p: &Pat, t: Term, b: &mut Binds, k: K) -> bool {
        if self.limit.tick() {
            return true; // stop everything; callers check the limit
        }
        match p {
            Pat::Blank { var, ty } => {
                if let Some(ty) = ty {
                    if !has_type(self.st(), t, *ty) {
                        return false;
                    }
                }
                self.bind_one(*var, t, b, k)
            }
            Pat::Lit(l) => *l == t && k(b),
            Pat::App { head, args, info } => {
                let st = self.st();
                if st.head(t) != Some(*head) {
                    return false;
                }
                let ts = st.args(t);
                if info.ac {
                    self.m_ac(args, info, ts, b, k)
                } else if !info.has_seq {
                    ts.len() == args.len() && self.m_fixed(args, ts, b, k)
                } else {
                    ts.len() >= info.min as usize && self.m_seq(args, ts, b, k)
                }
            }
            Pat::Alt(ps) => ps.iter().any(|q| self.m(q, t, b, k)),
            Pat::Cond(q, c) => {
                let env = self.env;
                self.m(q, t, b, &mut |b| c(b, env) && k(b))
            }
            Pat::Seq { .. } => false,
        }
    }

    /// Arguments one to one (no sequence variables).
    fn m_fixed(&self, ps: &[Pat], ts: &[Term], b: &mut Binds, k: K) -> bool {
        match ps.split_first() {
            None => k(b),
            Some((p, rest)) => self.m(p, ts[0], b, &mut |b| self.m_fixed(rest, &ts[1..], b, k)),
        }
    }

    /// Arguments with sequence variables, left to right; a sequence variable tries the shortest run first.
    fn m_seq(&self, ps: &[Pat], ts: &[Term], b: &mut Binds, k: K) -> bool {
        let Some((p, rest)) = ps.split_first() else { return ts.is_empty() && k(b) };
        if let Pat::Seq { var, min } = p {
            let rest_min: usize = rest.iter().map(Pat::min_len).sum();
            if ts.len() < rest_min + *min as usize {
                return false;
            }
            let max = ts.len() - rest_min;
            // with no sequence variable after this one, only one length can work
            let lo = if rest.iter().any(Pat::is_seq) { *min as usize } else { max };
            for len in lo..=max {
                if self.limit.tick() {
                    return true;
                }
                let run = &ts[..len];
                if self.bind_seq(*var, &mut run.iter().copied(), b, &mut |b| self.m_seq(rest, &ts[len..], b, k)) {
                    return true;
                }
            }
            false
        } else {
            !ts.is_empty() && self.m(p, ts[0], b, &mut |b| self.m_seq(rest, &ts[1..], b, k))
        }
    }

    /// Arguments of an AC head: `ts` is in canonical order, `ps` in class order.
    fn m_ac(&self, ps: &[Pat], info: &AppInfo, ts: &[Term], b: &mut Binds, k: K) -> bool {
        let n = ts.len();
        if n < info.min as usize || (!info.has_seq && n != info.n_single as usize) {
            return false;
        }
        assert!(n <= MAX_AC && ps.len() <= MAX_AC_PAT, "spike limit on AC arity");
        if self.smart {
            self.ac_smart_entry(ps, ts, b, k)
        } else {
            self.ac_naive(ps, ts, 0, b, k)
        }
    }

    // --- naive ---------------------------------------------------------------------------------------------------

    fn ac_naive(&self, ps: &[Pat], ts: &[Term], used: u64, b: &mut Binds, k: K) -> bool {
        let Some((p, rest)) = ps.split_first() else { return used == all(ts.len()) && k(b) };
        if let Pat::Seq { var, min } = p {
            if self.rest && rest.is_empty() {
                let free = all(ts.len()) & !used;
                if (free.count_ones() as usize) < *min as usize {
                    return false;
                }
                let mut it = (0..ts.len()).filter(|j| free & (1 << j) != 0).map(|j| ts[j]);
                return self.bind_seq(*var, &mut it, b, k);
            }
            return self.subsets(*var, *min as usize, rest, ts, used, 0, 0, None, b, k);
        }
        for j in 0..ts.len() {
            if used & (1 << j) == 0 && self.m(p, ts[j], b, &mut |b| self.ac_naive(rest, ts, used | (1 << j), b, k)) {
                return true;
            }
        }
        false
    }

    /// Enumerate subsets of the unused arguments for a sequence variable, in lexicographic order of their sorted
    /// positions with a prefix before its extensions ({} < {0} < {0,1} < {0,2} < {1} ...). `cands` is `Some` for
    /// the smart strategy (it then skips duplicate copies and checks the remaining patterns' candidates).
    #[allow(clippy::too_many_arguments)]
    fn subsets(
        &self,
        var: Option<u16>,
        min: usize,
        rest: &[Pat],
        ts: &[Term],
        used: u64,
        chosen: u64,
        from: usize,
        cands: Option<&[u64]>,
        b: &mut Binds,
        k: K,
    ) -> bool {
        if self.limit.tick() {
            return true;
        }
        if chosen.count_ones() as usize >= min {
            let mut it = (0..ts.len()).filter(|j| chosen & (1 << j) != 0).map(|j| ts[j]);
            let ok = match cands {
                None => self.bind_seq(var, &mut it, b, &mut |b| self.ac_naive(rest, ts, used | chosen, b, k)),
                Some(c) => {
                    // `c` holds the candidates of this sequence variable followed by those of `rest`
                    let u = used | chosen;
                    let rc = &c[c.len() - rest.len()..];
                    self.feasible(rest, ts.len(), u, rc)
                        && self.bind_seq(var, &mut it, b, &mut |b| self.ac_smart(rest, ts, rc, u, b, k))
                }
            };
            if ok {
                return true;
            }
        }
        for j in from..ts.len() {
            let taken = used | chosen;
            if taken & (1 << j) != 0 {
                continue;
            }
            // smart: of identical arguments, take the first unused copy only (see ac_smart)
            if cands.is_some() && j > 0 && ts[j] == ts[j - 1] && taken & (1 << (j - 1)) == 0 {
                continue;
            }
            if self.subsets(var, min, rest, ts, used, chosen | (1 << j), j + 1, cands, b, k) {
                return true;
            }
        }
        false
    }

    // --- smart ---------------------------------------------------------------------------------------------------

    fn ac_smart_entry(&self, ps: &[Pat], ts: &[Term], b: &mut Binds, k: K) -> bool {
        let st = self.st();
        let n = ts.len();
        let mut cands = [0u64; MAX_AC_PAT];
        for (i, p) in ps.iter().enumerate() {
            cands[i] = all(n);
            if p.is_seq() {
                continue;
            }
            let mut c = 0u64;
            match p.strip() {
                Pat::Lit(l) => {
                    for (j, t) in ts.iter().enumerate() {
                        if t == l {
                            c |= 1 << j;
                        }
                    }
                }
                Pat::Blank { var, ty } => {
                    let bound = var.map(|v| b.vals[v as usize]);
                    for (j, &t) in ts.iter().enumerate() {
                        let ok = match bound {
                            Some(Val::One(u)) => u == t,
                            Some(Val::Seq(..)) => false,
                            _ => ty.is_none_or(|ty| has_type(st, t, ty)),
                        };
                        if ok {
                            c |= 1 << j;
                        }
                    }
                }
                Pat::App { head, .. } => {
                    // canonical order groups compound arguments by head: one contiguous range
                    for (j, &t) in ts.iter().enumerate() {
                        let ok = if j > 0 && ts[j - 1] == t {
                            c & (1 << (j - 1)) != 0 // same term as the previous one: same answer
                        } else {
                            st.head(t) == Some(*head) && self.m(p, t, b, &mut |_| true)
                        };
                        if ok {
                            c |= 1 << j;
                        }
                    }
                    if self.limit.hit() {
                        return true;
                    }
                }
                _ => c = all(n), // alternatives: no cheap filter
            }
            cands[i] = c;
            if c == 0 {
                return false;
            }
        }
        let cands = &cands[..ps.len()];
        let singles: Vec<u64> = ps.iter().zip(cands).filter(|(p, _)| !p.is_seq()).map(|(_, c)| *c).collect();
        if !hall(&singles, all(n)) {
            return false;
        }
        self.ac_smart(ps, ts, cands, 0, b, k)
    }

    /// Can the remaining patterns still be satisfied with the unused arguments? Counts, and a candidate left for
    /// every single pattern.
    fn feasible(&self, ps: &[Pat], n: usize, used: u64, cands: &[u64]) -> bool {
        let free = all(n) & !used;
        let left = free.count_ones() as usize;
        let need: usize = ps.iter().map(Pat::min_len).sum();
        if left < need || (!ps.iter().any(Pat::is_seq) && left != ps.len()) {
            return false;
        }
        ps.iter().zip(cands).all(|(p, c)| p.is_seq() || c & free != 0)
    }

    fn ac_smart(&self, ps: &[Pat], ts: &[Term], cands: &[u64], used: u64, b: &mut Binds, k: K) -> bool {
        let Some((p, rest)) = ps.split_first() else { return used == all(ts.len()) && k(b) };
        let n = ts.len();
        if let Pat::Seq { var, min } = p {
            if rest.is_empty() {
                // the last pattern argument takes everything left: one subset, no enumeration
                let free = all(n) & !used;
                if (free.count_ones() as usize) < *min as usize {
                    return false;
                }
                let mut it = (0..n).filter(|j| free & (1 << j) != 0).map(|j| ts[j]);
                return self.bind_seq(*var, &mut it, b, k);
            }
            return self.subsets(*var, *min as usize, rest, ts, used, 0, 0, Some(cands), b, k);
        }
        let mut c = cands[0] & !used;
        while c != 0 {
            let j = c.trailing_zeros() as usize;
            c &= c - 1;
            // Of identical arguments take the first unused copy: if a solution gave this pattern copy j while the
            // identical copy j-1 is unused here, swapping the two gives a solution that is earlier in the order.
            if j > 0 && ts[j] == ts[j - 1] && used & (1 << (j - 1)) == 0 {
                continue;
            }
            let u = used | (1 << j);
            if !self.feasible(rest, n, u, &cands[1..]) {
                continue;
            }
            if self.m(p, ts[j], b, &mut |b| self.ac_smart(rest, ts, &cands[1..], u, b, k)) {
                return true;
            }
        }
        false
    }
}

/// Is there a matching that gives each single pattern (candidate set `c[i]`) a distinct argument within `avail`?
/// Kuhn's augmenting paths; the sets are tiny.
fn hall(c: &[u64], avail: u64) -> bool {
    fn augment(i: usize, c: &[u64], avail: u64, owner: &mut [u8; 64], seen: &mut u64) -> bool {
        let mut m = c[i] & avail & !*seen;
        while m != 0 {
            let j = m.trailing_zeros() as usize;
            m &= m - 1;
            *seen |= 1 << j;
            if owner[j] == u8::MAX || augment(owner[j] as usize, c, avail, owner, seen) {
                owner[j] = i as u8;
                return true;
            }
        }
        false
    }
    let mut owner = [u8::MAX; 64];
    (0..c.len()).all(|i| {
        let mut seen = 0;
        augment(i, c, avail, &mut owner, &mut seen)
    })
}
