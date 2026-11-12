//! Patterns (semantics 6.1, the subset this spike needs), their compilation, and the instance-of relation that
//! orders rules (semantics 6.3).
//!
//! Pattern variables are small integers local to one rule. A variable that occurs twice must bind identical terms
//! (non-linear patterns). Sequence variables bind lists of arguments (semantics 1.3: there is no splicing).
//!
//! Two choices made here, recorded as decisions in the report:
//! - Under an associative head a single variable `x_` matches exactly one argument; absorbing several arguments is
//!   what sequence variables are for. This removes the "variable takes a sub-sum" case from AC matching.
//! - The arguments of an AC pattern are kept in a fixed **class order** (literals, patterns with a fixed head, typed
//!   single variables, other single patterns, sequence variables; definition order within a class). Matches are
//!   enumerated in this pattern order, each argument taking the subject arguments in canonical order (semantics 6.2).
//!   Putting the most constrained arguments first is what lets a matcher prune without changing the order.
use crate::eval::{CondCx, Res};
use crate::heap::{Heap, View};
use crate::matcher::Binds;
use crate::Term;
use std::cell::Cell;
use std::sync::Arc;

/// Types usable in `x_ : T` (semantics 16, the part the workloads need).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ty {
    Integer,
    Real,
    Number,
    Symbol,
    Head(Term),
}

/// A condition: a closure over the bindings. Unlike spike 2's, it may **evaluate terms**: through [`CondCx`] it can
/// allocate in the shared heap, read own values and call the evaluator, and every read it makes (own values, rule
/// sets consulted by the evaluations it starts, read sets of their memo hits) enters the read set of the evaluation
/// whose dispatch runs it (question 2). It may fail with an abort (a speculative task reached an effect, or a limit),
/// which stops the match. A condition inside a pattern may only use variables bound inside its own subpattern (6.1).
pub type CondFn = Arc<dyn Fn(&Binds, &mut CondCx<'_, '_>) -> Res<bool> + Send + Sync>;

/// Facts about a compound pattern node, filled in by [`compile`].
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct AppInfo {
    /// The head is associative and commutative: match modulo AC.
    pub ac: bool,
    /// Some argument is a sequence variable.
    pub has_seq: bool,
    /// Number of single (non-sequence) argument patterns.
    pub n_single: u16,
    /// Minimum number of subject arguments.
    pub min: u16,
}

#[derive(Clone)]
pub enum Pat {
    /// `_`, `x_`, `x_ : T`
    Blank { var: Option<u16>, ty: Option<Ty> },
    /// `xs__` (min 1) or `xs___` (min 0); only as an argument.
    Seq { var: Option<u16>, min: u8 },
    /// Exactly this term. [`compile`] folds variable-free subpatterns into literals.
    Lit(Term),
    App { head: Term, args: Vec<Pat>, info: AppInfo },
    /// `p | q`: tried in order. All alternatives must bind the same variables.
    Alt(Vec<Pat>),
    /// `p /; c`
    Cond(Box<Pat>, CondFn),
}

// Builders, for writing workloads compactly.
pub fn blank() -> Pat {
    Pat::Blank { var: None, ty: None }
}
pub fn x(v: u16) -> Pat {
    Pat::Blank { var: Some(v), ty: None }
}
pub fn xt(v: u16, ty: Ty) -> Pat {
    Pat::Blank { var: Some(v), ty: Some(ty) }
}
pub fn seq1(v: u16) -> Pat {
    Pat::Seq { var: Some(v), min: 1 }
}
pub fn seq0(v: u16) -> Pat {
    Pat::Seq { var: Some(v), min: 0 }
}
pub fn lit(t: Term) -> Pat {
    Pat::Lit(t)
}
pub fn papp(head: Term, args: Vec<Pat>) -> Pat {
    Pat::App { head, args, info: AppInfo::default() }
}
pub fn alt(ps: Vec<Pat>) -> Pat {
    Pat::Alt(ps)
}
pub fn cond(p: Pat, f: impl Fn(&Binds, &mut CondCx<'_, '_>) -> Res<bool> + Send + Sync + 'static) -> Pat {
    Pat::Cond(Box::new(p), Arc::new(f))
}

impl Pat {
    pub fn is_seq(&self) -> bool {
        matches!(self, Pat::Seq { .. })
    }
    /// Minimum number of arguments this argument pattern consumes.
    pub fn min_len(&self) -> usize {
        match self {
            Pat::Seq { min, .. } => *min as usize,
            _ => 1,
        }
    }
    /// The pattern without conditions at its top.
    pub fn strip(&self) -> &Pat {
        let mut p = self;
        while let Pat::Cond(q, _) = p {
            p = q;
        }
        p
    }
    /// One more than the largest variable number (the size of the binding vector).
    pub fn n_vars(&self) -> usize {
        match self {
            Pat::Blank { var, .. } | Pat::Seq { var, .. } => var.map_or(0, |v| v as usize + 1),
            Pat::Lit(_) => 0,
            Pat::App { args, .. } | Pat::Alt(args) => args.iter().map(Pat::n_vars).max().unwrap_or(0),
            Pat::Cond(p, _) => p.n_vars(),
        }
    }
    /// The class of an argument under an AC head (see the module comment).
    fn ac_class(&self) -> u8 {
        match self.strip() {
            Pat::Lit(_) => 0,
            Pat::App { .. } => 1,
            Pat::Blank { ty: Some(_), .. } => 2,
            Pat::Seq { .. } => 4,
            _ => 3,
        }
    }
    /// The head of a pattern that only matches compound terms with one fixed head.
    pub fn fixed_head(&self, st: &Heap) -> Option<Term> {
        match self.strip() {
            Pat::App { head, .. } => Some(*head),
            Pat::Lit(t) => st.head(*t),
            Pat::Blank { ty: Some(Ty::Head(h)), .. } => Some(*h),
            _ => None,
        }
    }
}

/// Prepare a pattern for matching: mark AC nodes (`is_ac` tells which heads are associative and commutative),
/// flatten nested AC patterns with the same head, put AC arguments in class order, fold variable-free subpatterns
/// into literal terms (canonicalised, so that identity is enough to match them), and record arity facts.
pub fn compile(st: &Heap, p: Pat, is_ac: &dyn Fn(Term) -> bool) -> Pat {
    match p {
        Pat::App { head, args, .. } => {
            let ac = is_ac(head);
            let mut out = Vec::with_capacity(args.len());
            for a in args {
                let a = compile(st, a, is_ac);
                if ac {
                    // associativity: f(a, f(b, c)) is f(a, b, c), for patterns as for terms
                    match a {
                        Pat::App { head: h, args: inner, .. } if h == head => {
                            out.extend(inner);
                            continue;
                        }
                        Pat::Lit(t) if st.head(t) == Some(head) => {
                            out.extend(st.args(t).iter().map(|&u| Pat::Lit(u)));
                            continue;
                        }
                        a => out.push(a),
                    }
                } else {
                    out.push(a);
                }
            }
            if out.iter().all(|a| matches!(a, Pat::Lit(_))) {
                let ts: Vec<Term> = out.iter().map(|a| if let Pat::Lit(t) = a { *t } else { unreachable!() }).collect();
                let t = if ac { st.app_ac(head, &ts) } else { st.app(head, &ts) };
                return Pat::Lit(t);
            }
            if ac {
                // stable: definition order within a class
                out.sort_by_key(Pat::ac_class);
            }
            let n_single = out.iter().filter(|a| !a.is_seq()).count() as u16;
            let min = out.iter().map(Pat::min_len).sum::<usize>() as u16;
            let has_seq = out.iter().any(Pat::is_seq);
            Pat::App { head, args: out, info: AppInfo { ac, has_seq, n_single, min } }
        }
        Pat::Alt(ps) => Pat::Alt(ps.into_iter().map(|q| compile(st, q, is_ac)).collect()),
        Pat::Cond(q, c) => Pat::Cond(Box::new(compile(st, *q, is_ac)), c),
        p => p,
    }
}

/// Does term `t` have type `ty`?
#[inline]
pub fn has_type(st: &Heap, t: Term, ty: Ty) -> bool {
    match ty {
        Ty::Integer => matches!(st.view(t), View::Int(_)),
        Ty::Real => matches!(st.view(t), View::Real(_)),
        Ty::Number => matches!(st.view(t), View::Int(_) | View::Real(_)),
        Ty::Symbol => matches!(st.view(t), View::Sym(_)),
        Ty::Head(h) => st.head(t) == Some(h),
    }
}

fn subtype(a: Ty, b: Ty) -> bool {
    a == b || (b == Ty::Number && matches!(a, Ty::Integer | Ty::Real))
}

/// Structural equality of patterns; conditions are equal only if they are the same closure.
pub fn pat_eq(a: &Pat, b: &Pat) -> bool {
    match (a, b) {
        (Pat::Blank { var: v1, ty: t1 }, Pat::Blank { var: v2, ty: t2 }) => v1 == v2 && t1 == t2,
        (Pat::Seq { var: v1, min: m1 }, Pat::Seq { var: v2, min: m2 }) => v1 == v2 && m1 == m2,
        (Pat::Lit(s), Pat::Lit(t)) => s == t,
        (Pat::App { head: h1, args: a1, .. }, Pat::App { head: h2, args: a2, .. }) => {
            h1 == h2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| pat_eq(x, y))
        }
        (Pat::Alt(a1), Pat::Alt(a2)) => a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| pat_eq(x, y)),
        (Pat::Cond(p, c), Pat::Cond(q, d)) => Arc::ptr_eq(c, d) && pat_eq(p, q),
        _ => false,
    }
}

/// Identical up to a renaming of pattern variables (semantics 6.3: such a definition replaces the old rule).
pub fn same_up_to_renaming(a: &Pat, b: &Pat) -> bool {
    fn go(a: &Pat, b: &Pat, fwd: &mut Vec<Option<u16>>, back: &mut Vec<Option<u16>>) -> bool {
        let var_ok = |x: Option<u16>, y: Option<u16>, fwd: &mut Vec<Option<u16>>, back: &mut Vec<Option<u16>>| {
            match (x, y) {
                (None, None) => true,
                (Some(x), Some(y)) => {
                    let (xi, yi) = (x as usize, y as usize);
                    if fwd.len() <= xi {
                        fwd.resize(xi + 1, None);
                    }
                    if back.len() <= yi {
                        back.resize(yi + 1, None);
                    }
                    match (fwd[xi], back[yi]) {
                        (None, None) => {
                            fwd[xi] = Some(y);
                            back[yi] = Some(x);
                            true
                        }
                        (f, g) => f == Some(y) && g == Some(x),
                    }
                }
                _ => false,
            }
        };
        match (a, b) {
            (Pat::Blank { var: v1, ty: t1 }, Pat::Blank { var: v2, ty: t2 }) => t1 == t2 && var_ok(*v1, *v2, fwd, back),
            (Pat::Seq { var: v1, min: m1 }, Pat::Seq { var: v2, min: m2 }) => m1 == m2 && var_ok(*v1, *v2, fwd, back),
            (Pat::Lit(s), Pat::Lit(t)) => s == t,
            (Pat::App { head: h1, args: a1, .. }, Pat::App { head: h2, args: a2, .. }) => {
                h1 == h2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| go(x, y, fwd, back))
            }
            (Pat::Alt(a1), Pat::Alt(a2)) => a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| go(x, y, fwd, back)),
            (Pat::Cond(p, c), Pat::Cond(q, d)) => Arc::ptr_eq(c, d) && go(p, q, fwd, back),
            _ => false,
        }
    }
    go(a, b, &mut vec![], &mut vec![])
}

// ---------------------------------------------------------------------------------------------------------------
// Instance-of (semantics 6.3)

/// What a variable of the general pattern is bound to: a piece of the specific pattern.
#[derive(Clone)]
enum Piece {
    One(Pat),
    Many(Vec<Pat>),
}

/// Do two pieces of the specific pattern always stand for the same terms? Only if they are built from literals and
/// the same named variables: two anonymous blanks, or two copies of `a | b`, can match different terms.
fn rigid_eq(a: &Pat, b: &Pat) -> bool {
    match (a, b) {
        (Pat::Lit(s), Pat::Lit(t)) => s == t,
        (Pat::Blank { var: Some(v), .. }, Pat::Blank { var: Some(w), .. }) => v == w,
        (Pat::Seq { var: Some(v), .. }, Pat::Seq { var: Some(w), .. }) => v == w,
        (Pat::App { head: h1, args: a1, .. }, Pat::App { head: h2, args: a2, .. }) => {
            h1 == h2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| rigid_eq(x, y))
        }
        _ => false,
    }
}

fn piece_eq(a: &Piece, b: &Piece) -> bool {
    match (a, b) {
        (Piece::One(x), Piece::One(y)) => rigid_eq(x, y),
        (Piece::Many(xs), Piece::Many(ys)) => xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| rigid_eq(x, y)),
        _ => false,
    }
}

type GEnv = Vec<Option<Piece>>;
type GK<'k> = &'k mut dyn FnMut(&mut GEnv) -> bool;

/// Is `a` an instance of `b`, that is, does `b` match every term `a` matches? Conditions are ignored (semantics
/// 6.3). The check matches `b` against `a` read as a term whose variables are unknowns: a variable of `b` may bind a
/// piece of `a`, a typed variable only a piece whose type is known to fit, a sequence variable of `b` a run of `a`'s
/// arguments (including `a`'s own sequence variables), and nothing else matches a variable of `a`.
///
/// Sound, not complete: `true` is a proof, `false` may also mean "not provable this way" (for example `f(y_, zs___)`
/// against `f(ws__)`), in which case the two rules stay in definition order.
///
/// The search is bounded by [`INSTANCE_BUDGET`] steps; past it the answer is `false` ("not proven"), which keeps the
/// check sound: `true` is only reached through completed steps.
pub fn instance_of(st: &Heap, a: &Pat, b: &Pat) -> bool {
    let g = Gen { st, steps: Cell::new(0) };
    let mut env: GEnv = vec![None; b.n_vars()];
    g.gen(b, a, &mut env, &mut |_| true)
}

pub const INSTANCE_BUDGET: u32 = 1 << 20;

struct Gen<'a> {
    st: &'a Heap,
    steps: Cell<u32>,
}

impl Gen<'_> {
    fn tick(&self) -> bool {
        let s = self.steps.get() + 1;
        self.steps.set(s);
        s > INSTANCE_BUDGET
    }

    fn bind(&self, var: Option<u16>, piece: Piece, env: &mut GEnv, k: GK) -> bool {
        let Some(v) = var else { return k(env) };
        let v = v as usize;
        match &env[v] {
            Some(old) => piece_eq(old, &piece) && k(env),
            None => {
                env[v] = Some(piece);
                let r = k(env);
                env[v] = None;
                r
            }
        }
    }

    /// Is the type of the specific piece `a` known to be within `ty`?
    fn a_has_type(&self, a: &Pat, ty: Ty) -> bool {
        match a {
            Pat::Lit(t) => has_type(self.st, *t, ty),
            Pat::Blank { ty: Some(ta), .. } => subtype(*ta, ty),
            Pat::App { head, .. } => ty == Ty::Head(*head),
            _ => false,
        }
    }

    /// `b` generalises `a`; on success call `k` with the bindings.
    fn gen(&self, b: &Pat, a: &Pat, env: &mut GEnv, k: GK) -> bool {
        if self.tick() {
            return false;
        }
        let a = a.strip();
        match b.strip() {
            Pat::Blank { var, ty } => {
                if a.is_seq() {
                    return false;
                }
                if let Some(ty) = ty {
                    if !self.a_has_type(a, *ty) {
                        return false;
                    }
                }
                self.bind(*var, Piece::One(a.clone()), env, k)
            }
            Pat::Lit(t) => matches!(a, Pat::Lit(u) if u == t) && k(env),
            Pat::Alt(bs) => bs.iter().any(|bi| self.gen(bi, a, env, k)),
            Pat::App { head, args: bargs, info } => {
                // a literal compound is read as the pattern of its arguments
                let expanded;
                let aargs: &[Pat] = match a {
                    Pat::App { head: ah, args, .. } if ah == head => args,
                    Pat::Lit(t) if self.st.head(*t) == Some(*head) => {
                        expanded = self.st.args(*t).iter().map(|&u| Pat::Lit(u)).collect::<Vec<_>>();
                        &expanded
                    }
                    _ => return false,
                };
                if info.ac {
                    self.gen_ac(bargs, aargs, 0, env, k)
                } else {
                    self.gen_seq(bargs, aargs, env, k)
                }
            }
            Pat::Seq { .. } | Pat::Cond(..) => false,
        }
    }

    fn gen_seq(&self, bs: &[Pat], aas: &[Pat], env: &mut GEnv, k: GK) -> bool {
        let Some((b0, brest)) = bs.split_first() else { return aas.is_empty() && k(env) };
        if let Pat::Seq { var, min } = b0 {
            for len in 0..=aas.len() {
                let run = &aas[..len];
                if run.iter().map(Pat::min_len).sum::<usize>() < *min as usize {
                    continue;
                }
                let piece = Piece::Many(run.to_vec());
                if self.bind(*var, piece, env, &mut |env| self.gen_seq(brest, &aas[len..], env, k)) {
                    return true;
                }
            }
            false
        } else {
            match aas.split_first() {
                Some((a0, arest)) if !a0.is_seq() => self.gen(b0, a0, env, &mut |env| self.gen_seq(brest, arest, env, k)),
                _ => false,
            }
        }
    }

    /// Multiset version: `used` marks the arguments of `a` already covered.
    fn gen_ac(&self, bs: &[Pat], aas: &[Pat], used: u64, env: &mut GEnv, k: GK) -> bool {
        assert!(aas.len() <= 64, "spike limit: AC patterns with at most 64 arguments");
        let all = if aas.len() == 64 { u64::MAX } else { (1u64 << aas.len()) - 1 };
        let Some((b0, brest)) = bs.split_first() else { return used == all && k(env) };
        if let Pat::Seq { var, min } = b0 {
            self.gen_subsets(*var, *min as usize, brest, aas, used, 0, 0, env, k)
        } else {
            for j in 0..aas.len() {
                if used & (1 << j) != 0 || aas[j].is_seq() {
                    continue;
                }
                if self.gen(b0, &aas[j], env, &mut |env| self.gen_ac(brest, aas, used | (1 << j), env, k)) {
                    return true;
                }
            }
            false
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn gen_subsets(
        &self,
        var: Option<u16>,
        min: usize,
        brest: &[Pat],
        aas: &[Pat],
        used: u64,
        chosen: u64,
        from: usize,
        env: &mut GEnv,
        k: GK,
    ) -> bool {
        let run: Vec<Pat> = (0..aas.len()).filter(|j| chosen & (1 << j) != 0).map(|j| aas[j].clone()).collect();
        if run.iter().map(Pat::min_len).sum::<usize>() >= min
            && self.bind(var, Piece::Many(run), env, &mut |env| self.gen_ac(brest, aas, used | chosen, env, k))
        {
            return true;
        }
        for j in from..aas.len() {
            if (used | chosen) & (1 << j) == 0
                && self.gen_subsets(var, min, brest, aas, used, chosen | (1 << j), j + 1, env, k)
            {
                return true;
            }
        }
        false
    }
}

/// A readable form of a pattern.
pub fn show(st: &Heap, p: &Pat) -> String {
    let v = |var: &Option<u16>| var.map_or(String::new(), |v| format!("x{v}"));
    match p {
        Pat::Blank { var, ty } => format!("{}_{}", v(var), ty.map_or(String::new(), |t| format!(":{t:?}"))),
        Pat::Seq { var, min } => format!("{}{}", v(var), if *min == 0 { "___" } else { "__" }),
        Pat::Lit(t) => st.show(*t),
        Pat::App { head, args, info } => {
            let a: Vec<String> = args.iter().map(|q| show(st, q)).collect();
            format!("{}{}({})", st.show(*head), if info.ac { "~" } else { "" }, a.join(", "))
        }
        Pat::Alt(ps) => ps.iter().map(|q| show(st, q)).collect::<Vec<_>>().join(" | "),
        Pat::Cond(q, _) => format!("{} /; c", show(st, q)),
    }
}
