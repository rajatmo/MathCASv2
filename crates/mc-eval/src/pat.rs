//! Patterns (semantics 6.1), their compilation, and the instance-of relation that orders rules (6.3).
//!
//! Pattern variables are small integers local to one rule. A variable that occurs twice must bind identical terms
//! (non-linear patterns). Sequence variables bind runs of arguments (semantics 1.3: there is no splicing).
//!
//! Patterns hold their terms (literals, heads) as [`Raw`] handles: a compiled pattern lives in a rule table, which is
//! a root container traced at safe points (A.2). A pattern built outside a table is valid until the next safe point.
//!
//! **Compilation** (when a pattern is defined or compiled) checks a pattern against the definition-time rules (decision F.6) and prepares it:
//! - a condition may use only variables bound inside its own subpattern (6.1); a condition declares the variables it
//!   reads ([`cond`]), so a reference to an outer variable is a definition-time error, not a run-time surprise;
//! - alternatives bind the same variables; a variable is either single or sequence; sequence variables appear only
//!   as arguments;
//! - nested patterns under an associative head with the same head are flattened, and the arguments of an AC pattern
//!   are put in **class order** (literals, fixed-head patterns, typed variables, other single patterns, sequence
//!   variables; definition order within a class; semantics 6.2);
//! - variable-free subpatterns become literal terms, canonicalised, so identity is enough to match them.
//!
//! Patterns are bounded ([`MAX_PATTERN_NODES`], [`MAX_PATTERN_DEPTH`]): the matcher and the instance check recurse
//! on pattern structure, never on subject terms (decision F.7).
use crate::eval::{CondCx, Res};
use mc_core::{Heap, Raw, Store, Sym, View, T};
use std::cell::Cell;
use std::sync::Arc;

/// Most nodes a pattern may have.
pub const MAX_PATTERN_NODES: usize = 10_000;
/// Deepest a pattern may nest.
pub const MAX_PATTERN_DEPTH: usize = 256;
/// Most pattern variables of one rule.
pub const MAX_VARS: usize = 1024;

/// Types usable in `x_ : T` (semantics 16, the atomic part).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ty {
    /// any integer
    Integer,
    /// a rational that is not an integer
    Rational,
    /// a machine real
    MachineReal,
    /// a ball
    Ball,
    /// `integer | rational | machine_real | ball`
    Real,
    /// a number (here: a real; complex numbers are not covered yet)
    Number,
    /// a string
    String,
    /// a symbol
    Symbol,
    /// an inexact real: a machine real or a ball (decision p6core.20)
    Float,
    /// a complex number `complex(re, im)` (decision p6core.20)
    Complex,
    /// a compound term with this head
    Head(Sym),
}

/// A condition's code: it reads its declared variables through [`CondCx::one`] and [`CondCx::seq`], may evaluate
/// terms through [`CondCx::eval`], and must not perform effects (semantics 6.1, A.6).
pub type CondFn = Arc<dyn Fn(&mut CondCx<'_, '_, '_>) -> Res<bool> + Send + Sync>;

/// A condition with the variables it reads.
#[derive(Clone)]
pub struct Cond {
    pub(crate) vars: Box<[u16]>,
    pub(crate) code: CondCode,
}

/// What a condition runs.
#[derive(Clone)]
pub(crate) enum CondCode {
    /// native code
    Native(CondFn),
    /// a term: the symbols `names[i]` are replaced by the value of variable `vars[i]` (a sequence variable by the
    /// list of its terms), and the result is evaluated; the condition holds when that gives `true`
    Term { body: Raw, names: Box<[Sym]> },
}

/// Facts about a compound pattern node, filled in by compilation.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct AppInfo {
    /// The head is associative and commutative: match modulo AC.
    pub ac: bool,
    /// The head is associative only (`flat`): an argument pattern may match a run of arguments wrapped in the head.
    /// Such a pattern counts as one with a sequence variable (`has_seq`): its length is not fixed.
    pub flat: bool,
    /// Some argument is a sequence variable.
    pub has_seq: bool,
    /// Number of single (non-sequence) argument patterns.
    pub n_single: u16,
    /// Minimum number of subject arguments.
    pub min: u16,
    /// Some argument is an optional argument (`Pat::Opt`).
    pub has_opt: bool,
    /// The head is [`CALL`]: the pattern of a call whose head is itself a pattern or a compound term
    /// (`f(x_)(y_)` is `call$(f(x_), y_)`, C9B2.4).
    pub curried: bool,
}

/// The head of the pattern of a call with a compound or pattern head: `call$(h, a1, ..., an)` matches `h(a1, ...,
/// an)` (and the evaluator's view `call$(h, a1, ...)` of such a call, under which its rules are stored).
pub const CALL: &str = "call$";

/// `p` as a pattern whose head may be compound: a literal call with a compound head becomes `call$(head, args...)`
/// of literals (C9B2.4), anything else stays.
pub(crate) fn curry_lit(h: &Heap, p: Pat) -> Pat {
    if let Pat::Lit(t) = &p {
        if let View::App { head, args } = h.view(t.get(h)) {
            if !matches!(h.view(head), View::Sym(_)) {
                let mut all = vec![Pat::Lit(Raw::new(head))];
                all.extend(args.iter().map(|&a| Pat::Lit(Raw::new(a))));
                return Pat::App { head: Raw::new(h.sym_term(h.sym(CALL))), args: all, info: AppInfo::default() };
            }
        }
    }
    p
}

/// The mark `longest(p, k)` / `shortest(p, k)` puts on a run pattern: which lengths are tried first, and its priority
/// among the marked runs of one argument list (higher first; ties left to right). Decision r30.4.
pub type Mark = (bool, i64);

/// A pattern.
#[derive(Clone)]
pub enum Pat {
    /// `_`, `x_`, `x_ : T`
    Blank {
        /// the variable, if named
        var: Option<u16>,
        /// the type, if any
        ty: Option<Ty>,
    },
    /// `xs__` (min 1) or `xs___` (min 0), `xs__ : T` (every argument of the run of type `T`); only as an
    /// argument.
    Seq {
        /// the variable, if named
        var: Option<u16>,
        /// the least number of arguments taken
        min: u8,
        /// the type of every argument of the run, if any (C9B2.3)
        ty: Option<Ty>,
        /// `longest(xs__, k)` (true), `shortest(xs__, k)` (false) with priority k (decision r30.4)
        mark: Option<Mark>,
    },
    /// Exactly this term.
    Lit(Raw),
    /// A compound pattern.
    App {
        /// the head (any term)
        head: Raw,
        /// the argument patterns
        args: Vec<Pat>,
        /// filled in by compilation
        info: AppInfo,
    },
    /// `p | q`: tried in order; all alternatives bind the same variables.
    Alt(Vec<Pat>),
    /// `x @ p`: matches what `p` matches, binding the whole term to the variable.
    Named(u16, Box<Pat>),
    /// `p /; c`
    Cond(Box<Pat>, Cond),
    /// `optional(x_, d)` among the arguments of a call (not a definition's own arguments, which define a rule
    /// per case): the argument matching `p` if there is one, else nothing, with `var` bound to `d` (C9B.13)
    Opt {
        /// the pattern of the argument when present (a named blank, possibly typed)
        p: Box<Pat>,
        /// its variable
        var: u16,
        /// the value of the variable when the argument is absent
        default: Raw,
    },
    /// `key_value_pattern([p1, ..., pk])`: an association (or a list of rules) with k distinct entries matching
    /// `p1`, ..., `pk`, in any order (C9B.11)
    KeyValue(Vec<Pat>),
    /// `repeated(p, ...)`: a run of `min` to `max` consecutive arguments, each matching `p` (variables in `p`
    /// bind the same term in every repetition); only as an argument of a head that is not associative-commutative
    /// (C9B.4).
    Repeat {
        /// the pattern of each argument
        p: Box<Pat>,
        /// the least number of arguments taken
        min: u32,
        /// the most (`u32::MAX`: no limit)
        max: u32,
        /// `longest(repeated(p), k)`, `shortest(...)` (decision r30.4)
        mark: Option<Mark>,
    },
}

/// `_`
pub fn blank() -> Pat {
    Pat::Blank { var: None, ty: None }
}
/// `x_` for variable `v`
pub fn var(v: u16) -> Pat {
    Pat::Blank { var: Some(v), ty: None }
}
/// `x_ : ty` for variable `v`
pub fn typed(v: u16, ty: Ty) -> Pat {
    Pat::Blank { var: Some(v), ty: Some(ty) }
}
/// `xs__` for variable `v`
pub fn seq1(v: u16) -> Pat {
    Pat::Seq { var: Some(v), min: 1, ty: None, mark: None }
}
/// `xs___` for variable `v`
pub fn seq0(v: u16) -> Pat {
    Pat::Seq { var: Some(v), min: 0, ty: None, mark: None }
}
/// Exactly the term `t`.
pub fn lit(t: T<'_>) -> Pat {
    Pat::Lit(Raw::new(t))
}
/// `head(args...)` for any head term.
pub fn app(head: T<'_>, args: Vec<Pat>) -> Pat {
    Pat::App { head: Raw::new(head), args, info: AppInfo::default() }
}
/// `f(args...)` for a symbol head.
pub fn call(h: &Heap, f: Sym, args: Vec<Pat>) -> Pat {
    app(h.sym_term(f), args)
}
/// `p1 | p2 | ...`
pub fn alt(ps: Vec<Pat>) -> Pat {
    Pat::Alt(ps)
}
/// `x @ p` for variable `v`
pub fn named(v: u16, p: Pat) -> Pat {
    Pat::Named(v, Box::new(p))
}
/// `p /; c`, where `c` reads the variables `vars` (by position: `cx.one(0)` is `vars[0]`).
pub fn cond(p: Pat, vars: &[u16], f: impl Fn(&mut CondCx<'_, '_, '_>) -> Res<bool> + Send + Sync + 'static) -> Pat {
    Pat::Cond(Box::new(p), Cond { vars: vars.into(), code: CondCode::Native(Arc::new(f)) })
}
/// `p /; c` with the condition a term in which symbol `names[i]` stands for variable `vars[i]`.
pub(crate) fn cond_term(p: Pat, vars: &[u16], names: &[Sym], body: T<'_>) -> Pat {
    Pat::Cond(Box::new(p), Cond { vars: vars.into(), code: CondCode::Term { body: Raw::new(body), names: names.into() } })
}

/// A definition-time error (semantics 6.1, 6.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefError {
    /// A rule's left side must be a compound term with a symbol head.
    NotCompound,
    /// An argument-head rule must attach to the head of one of the left side's arguments at depth one (6.5).
    BadAttachment,
    /// A condition reads a variable not bound inside its subpattern (6.1).
    ConditionScope(u16),
    /// The alternatives of `p | q` bind different variables.
    AlternativeVariables,
    /// A variable is used both as a single and as a sequence variable.
    VariableKind(u16),
    /// A sequence variable outside an argument position.
    SequencePosition,
    /// The pattern exceeds [`MAX_PATTERN_NODES`], [`MAX_PATTERN_DEPTH`] or [`MAX_VARS`].
    TooLarge,
    /// A construct the evaluator does not support yet (the name of the construct).
    Unsupported(&'static str),
    /// A type expression that is not one of the supported atomic types.
    UnknownType,
    /// The symbol is protected (decision r17fix.4).
    Protected(Sym),
}

impl std::fmt::Display for DefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DefError::NotCompound => write!(f, "a rule's left side must be compound with a symbol head"),
            DefError::BadAttachment => write!(f, "argument-head rules attach to the head of a depth-one argument"),
            DefError::ConditionScope(v) => write!(f, "a condition reads variable {v}, not bound inside its pattern"),
            DefError::AlternativeVariables => write!(f, "alternatives must bind the same variables"),
            DefError::VariableKind(v) => write!(f, "variable {v} is used as a single and as a sequence variable"),
            DefError::SequencePosition => write!(f, "sequence variables may only be arguments"),
            DefError::TooLarge => write!(f, "pattern too large"),
            DefError::Unsupported(what) => write!(f, "not supported yet: {what}"),
            DefError::UnknownType => write!(f, "unknown type"),
            DefError::Protected(s) => write!(f, "symbol {} is protected", s.0),
        }
    }
}

impl std::error::Error for DefError {}

impl Pat {
    /// Is this a sequence variable?
    pub fn is_seq(&self) -> bool {
        matches!(self, Pat::Seq { .. } | Pat::Repeat { .. } | Pat::Opt { .. })
    }
    /// Minimum number of arguments this argument pattern consumes.
    pub fn min_len(&self) -> usize {
        match self {
            Pat::Seq { min, .. } => *min as usize,
            Pat::Repeat { min, .. } => *min as usize,
            Pat::Opt { .. } => 0,
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
            Pat::App { args, .. } | Pat::Alt(args) | Pat::KeyValue(args) => args.iter().map(Pat::n_vars).max().unwrap_or(0),
            Pat::Cond(p, _) => p.n_vars(),
            Pat::Named(v, p) => p.n_vars().max(*v as usize + 1),
            Pat::Repeat { p, .. } | Pat::Opt { p, .. } => p.n_vars(),
        }
    }
    /// The pattern without conditions and names at its top: the shape of what it matches.
    pub(crate) fn shape(&self) -> &Pat {
        let mut p = self;
        loop {
            match p {
                Pat::Cond(q, _) | Pat::Named(_, q) => p = q,
                _ => return p,
            }
        }
    }
    /// The class of an argument under an AC head (semantics 6.2).
    /// An optional argument has the class of its pattern: it stands where it was written among them, which the
    /// case with the argument present keeps (C9B2.2).
    pub(crate) fn ac_class(&self) -> u8 {
        match self.shape() {
            Pat::Opt { p, .. } => p.ac_class(),
            Pat::Lit(_) => 0,
            Pat::App { .. } => 1,
            Pat::Blank { ty: Some(_), .. } => 2,
            Pat::Seq { .. } | Pat::Repeat { .. } => 4,
            _ => 3,
        }
    }
    /// The head of every compound term this pattern matches, if it is fixed.
    pub(crate) fn fixed_head<'h>(&self, h: &'h Heap) -> Option<T<'h>> {
        match self.shape() {
            // with an optional argument it also matches a term that is not a call of the head (C9B2.2)
            Pat::App { info, .. } if info.has_opt => None,
            Pat::App { head, .. } => Some(head.get(h)),
            Pat::Lit(t) => match h.view(t.get(h)) {
                View::App { head, .. } => Some(head),
                _ => None,
            },
            Pat::Blank { ty: Some(Ty::Head(s)), .. } => Some(h.sym_term(*s)),
            _ => None,
        }
    }

    /// Every handle the pattern holds (for tracing).
    pub(crate) fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        match self {
            Pat::Lit(t) => visit(t),
            Pat::App { head, args, .. } => {
                visit(head);
                args.iter_mut().for_each(|a| a.trace(visit));
            }
            Pat::Alt(ps) | Pat::KeyValue(ps) => ps.iter_mut().for_each(|a| a.trace(visit)),
            Pat::Cond(p, c) => {
                if let CondCode::Term { body, .. } = &mut c.code {
                    visit(body);
                }
                p.trace(visit)
            }
            Pat::Named(_, p) | Pat::Repeat { p, .. } => p.trace(visit),
            Pat::Opt { p, default, .. } => {
                visit(default);
                p.trace(visit)
            }
            Pat::Blank { .. } | Pat::Seq { .. } => {}
        }
    }
}

/// What compilation needs from the evaluator: which heads are associative and commutative, and how to build a
/// canonical compound term (flattened and sorted under such heads).
pub(crate) trait Canon {
    fn is_ac<'h>(&self, h: &'h Heap, head: T<'h>) -> bool;
    /// Associative only (see [`AppInfo::flat`]).
    fn is_flat<'h>(&self, h: &'h Heap, head: T<'h>) -> bool;
    fn make<'h>(&self, h: &'h Heap, head: T<'h>, args: &[T<'h>]) -> T<'h>;
}

/// Kinds of variable use, for the consistency check.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Use {
    None,
    One,
    Seq,
}

/// Check the definition-time rules (iteratively: size and depth first, so the recursive passes are bounded).
fn validate(p: &Pat) -> Result<usize, DefError> {
    let mut nodes = 0usize;
    let mut uses = vec![Use::None; 0];
    // (pattern, depth, may be a sequence variable)
    let mut stack = vec![(p, 0usize, false)];
    while let Some((q, d, arg)) = stack.pop() {
        nodes += 1;
        if nodes > MAX_PATTERN_NODES || d > MAX_PATTERN_DEPTH {
            return Err(DefError::TooLarge);
        }
        let mut note = |v: Option<u16>, u: Use| -> Result<(), DefError> {
            let Some(v) = v else { return Ok(()) };
            if v as usize >= MAX_VARS {
                return Err(DefError::TooLarge);
            }
            if uses.len() <= v as usize {
                uses.resize(v as usize + 1, Use::None);
            }
            match uses[v as usize] {
                Use::None => uses[v as usize] = u,
                w if w != u => return Err(DefError::VariableKind(v)),
                _ => {}
            }
            Ok(())
        };
        match q {
            Pat::Blank { var, .. } => note(*var, Use::One)?,
            Pat::Seq { var, .. } => {
                if !arg {
                    return Err(DefError::SequencePosition);
                }
                note(*var, Use::Seq)?
            }
            Pat::Lit(_) => {}
            Pat::App { args, .. } => stack.extend(args.iter().map(|a| (a, d + 1, true))),
            Pat::Alt(ps) => {
                let first = ps.first().map(bound_vars).unwrap_or_default();
                if ps.iter().any(|a| bound_vars(a) != first) {
                    return Err(DefError::AlternativeVariables);
                }
                stack.extend(ps.iter().map(|a| (a, d + 1, false)));
            }
            Pat::Cond(inner, c) => {
                let bound = bound_vars(inner);
                if let Some(&v) = c.vars.iter().find(|v| bound.binary_search(v).is_err()) {
                    return Err(DefError::ConditionScope(v));
                }
                stack.push((inner, d + 1, false));
            }
            Pat::Named(v, inner) => {
                note(Some(*v), Use::One)?;
                stack.push((inner, d + 1, false));
            }
            Pat::Repeat { p: inner, .. } => {
                if !arg {
                    return Err(DefError::SequencePosition);
                }
                stack.push((inner, d + 1, false));
            }
            Pat::KeyValue(ps) => stack.extend(ps.iter().map(|a| (a, d + 1, false))),
            Pat::Opt { p: inner, .. } => {
                if !arg {
                    return Err(DefError::SequencePosition);
                }
                // an optional run (a sequence variable that may be absent) stands in the argument position itself
                stack.push((inner, d + 1, matches!(**inner, Pat::Seq { .. })));
            }
        }
    }
    Ok(nodes)
}

/// The number of conditions inside a pattern below its top (conditions on argument patterns, `f(x_ when c)`); the
/// conditions at the top (a rule's `when` on the whole left side) are not counted.
pub(crate) fn inner_conditions(p: &Pat) -> usize {
    let mut n = 0;
    let mut stack = vec![p.strip()];
    while let Some(q) = stack.pop() {
        match q {
            Pat::App { args, .. } | Pat::Alt(args) | Pat::KeyValue(args) => stack.extend(args.iter()),
            Pat::Cond(inner, _) => {
                n += 1;
                stack.push(inner)
            }
            Pat::Repeat { p: inner, .. } | Pat::Opt { p: inner, .. } | Pat::Named(_, inner) => stack.push(inner),
            _ => {}
        }
    }
    n
}

/// The variables a pattern binds, sorted (called on validated-depth patterns only).
fn bound_vars(p: &Pat) -> Vec<u16> {
    let mut out = vec![];
    let mut stack = vec![p];
    while let Some(q) = stack.pop() {
        match q {
            Pat::Blank { var: Some(v), .. } | Pat::Seq { var: Some(v), .. } => out.push(*v),
            Pat::App { args, .. } | Pat::Alt(args) | Pat::KeyValue(args) => stack.extend(args.iter()),
            Pat::Cond(inner, _) | Pat::Repeat { p: inner, .. } | Pat::Opt { p: inner, .. } => stack.push(inner),
            Pat::Named(v, inner) => {
                out.push(*v);
                stack.push(inner)
            }
            _ => {}
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The definition-time checks alone (variable kinds, alternatives, condition scope, sequence positions, size), for
/// callers that compile a pattern later (`mc-lists`' matching, C9B.10).
pub fn check(p: &Pat) -> Result<(), DefError> {
    validate(p).map(|_| ())
}

/// Check and prepare a pattern for matching (see the module documentation).
pub(crate) fn compile(h: &Heap, p: Pat, cn: &dyn Canon) -> Result<Pat, DefError> {
    validate(&p)?;
    let p = prepare(h, p, cn);
    if repeat_under_ac(&p) {
        return Err(DefError::Unsupported("repeated(...) as an argument of an associative-commutative head"));
    }
    Ok(p)
}

/// Does a `repeated` pattern stand as an argument of an AC call (which the AC matcher does not take)? Optional
/// arguments are taken there (C9B2.2).
fn repeat_under_ac(p: &Pat) -> bool {
    let mut stack = vec![p];
    while let Some(q) = stack.pop() {
        match q {
            Pat::App { args, info, .. } => {
                if info.ac && args.iter().any(|a| matches!(a, Pat::Repeat { .. })) {
                    return true;
                }
                stack.extend(args.iter());
            }
            Pat::Alt(ps) | Pat::KeyValue(ps) => stack.extend(ps.iter()),
            Pat::Cond(inner, _) | Pat::Named(_, inner) | Pat::Repeat { p: inner, .. } | Pat::Opt { p: inner, .. } => stack.push(inner),
            _ => {}
        }
    }
    false
}

/// Flatten, order and fold (recursive on pattern depth, which [`validate`] bounded).
fn prepare(h: &Heap, p: Pat, cn: &dyn Canon) -> Pat {
    match p {
        Pat::App { head, args, .. } => {
            let hd = head.get(h);
            let ac = cn.is_ac(h, hd);
            let spliced = ac || cn.is_flat(h, hd);
            let mut out = Vec::with_capacity(args.len());
            for a in args {
                let a = prepare(h, a, cn);
                if spliced {
                    // associativity: f(a, f(b, c)) is f(a, b, c), for patterns as for terms
                    match a {
                        Pat::App { head: g, args: inner, .. } if g == head => {
                            out.extend(inner);
                            continue;
                        }
                        Pat::Lit(t) => match h.view(t.get(h)) {
                            View::App { head: g, args: inner } if g == hd => {
                                out.extend(inner.iter().map(|&u| Pat::Lit(Raw::new(u))));
                                continue;
                            }
                            _ => out.push(Pat::Lit(t)),
                        },
                        a => out.push(a),
                    }
                } else {
                    out.push(a);
                }
            }
            let curried = hd == h.sym_term(h.sym(CALL));
            if !curried && out.iter().all(|a| matches!(a, Pat::Lit(_))) {
                // invariant: every element is a literal (checked just above)
                let ts: Vec<T> = out.iter().map(|a| if let Pat::Lit(t) = a { t.get(h) } else { unreachable!() }).collect();
                return Pat::Lit(Raw::new(cn.make(h, hd, &ts)));
            }
            if ac {
                // stable: definition order within a class
                out.sort_by_key(Pat::ac_class);
            }
            let n_single = out.iter().filter(|a| !a.is_seq()).count() as u16;
            let min = out.iter().map(Pat::min_len).sum::<usize>() as u16;
            let flat = !ac && !curried && cn.is_flat(h, hd);
            let has_seq = flat || out.iter().any(Pat::is_seq);
            let has_opt = out.iter().any(|a| matches!(a, Pat::Opt { .. }));
            Pat::App { head, args: out, info: AppInfo { ac, flat, has_seq, n_single, min, has_opt, curried } }
        }
        Pat::Alt(ps) => Pat::Alt(ps.into_iter().map(|q| prepare(h, q, cn)).collect()),
        Pat::Cond(q, c) => Pat::Cond(Box::new(prepare(h, *q, cn)), c),
        Pat::Named(v, q) => Pat::Named(v, Box::new(prepare(h, *q, cn))),
        Pat::Repeat { p: q, min, max, mark } => Pat::Repeat { p: Box::new(prepare(h, *q, cn)), min, max, mark },
        Pat::KeyValue(ps) => Pat::KeyValue(ps.into_iter().map(|q| prepare(h, q, cn)).collect()),
        Pat::Opt { p: q, var, default } => Pat::Opt { p: Box::new(prepare(h, *q, cn)), var, default },
        p => p,
    }
}

/// Does term `t` have type `ty`?
#[inline]
pub(crate) fn has_type<'h>(h: &'h Heap, t: T<'h>, ty: Ty) -> bool {
    let v = h.view(t);
    match ty {
        Ty::Integer => matches!(v, View::Int(_) | View::BigInt { .. }),
        Ty::Rational => matches!(v, View::Rational { .. }),
        Ty::MachineReal => matches!(v, View::Real(_)),
        Ty::Ball => matches!(v, View::Ball { .. }),
        Ty::Real | Ty::Number => {
            matches!(v, View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. })
        }
        Ty::String => matches!(v, View::Str(_)),
        Ty::Symbol => matches!(v, View::Sym(_)),
        Ty::Float => matches!(v, View::Real(_) | View::Ball { .. }),
        Ty::Complex => is_complex(h, t),
        Ty::Head(s) => matches!(v, View::App { head, .. } if head == h.sym_term(s)),
    }
}

/// Is `t` a complex number `complex(re, im)` with real number parts?
pub fn is_complex<'h>(h: &'h Heap, t: T<'h>) -> bool {
    let View::App { head, args: [re, im] } = h.view(t) else { return false };
    let number = |a: T<'h>| {
        matches!(h.view(a), View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. })
    };
    number(*re) && number(*im) && matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "complex")
}

/// Is every term of type `a` of type `b`?
fn subtype(a: Ty, b: Ty) -> bool {
    let real = |t: Ty| matches!(t, Ty::Integer | Ty::Rational | Ty::MachineReal | Ty::Ball | Ty::Real | Ty::Float);
    a == b
        || (b == Ty::Real && real(a))
        || (b == Ty::Number && (real(a) || a == Ty::Number))
        || (b == Ty::Float && matches!(a, Ty::MachineReal | Ty::Ball))
}

/// Identical up to a renaming of pattern variables (semantics 6.3: such a definition replaces the old rule).
/// Conditions are equal only if they are the same code with the same variables.
pub(crate) fn same_up_to_renaming(a: &Pat, b: &Pat) -> bool {
    fn var_ok(x: Option<u16>, y: Option<u16>, fwd: &mut Vec<Option<u16>>, back: &mut Vec<Option<u16>>) -> bool {
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
    }
    fn go(a: &Pat, b: &Pat, fwd: &mut Vec<Option<u16>>, back: &mut Vec<Option<u16>>) -> bool {
        match (a, b) {
            (Pat::Blank { var: v1, ty: t1 }, Pat::Blank { var: v2, ty: t2 }) => t1 == t2 && var_ok(*v1, *v2, fwd, back),
            (Pat::Seq { var: v1, min: m1, ty: t1, mark: k1 }, Pat::Seq { var: v2, min: m2, ty: t2, mark: k2 }) => {
                m1 == m2 && t1 == t2 && k1 == k2 && var_ok(*v1, *v2, fwd, back)
            }
            (Pat::Lit(s), Pat::Lit(t)) => s == t,
            (Pat::App { head: h1, args: a1, .. }, Pat::App { head: h2, args: a2, .. }) => {
                h1 == h2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| go(x, y, fwd, back))
            }
            (Pat::Alt(a1), Pat::Alt(a2)) | (Pat::KeyValue(a1), Pat::KeyValue(a2)) => {
                a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| go(x, y, fwd, back))
            }
            (Pat::Named(v, p), Pat::Named(w, q)) => var_ok(Some(*v), Some(*w), fwd, back) && go(p, q, fwd, back),
            (Pat::Opt { p, default: d1, .. }, Pat::Opt { p: q, default: d2, .. }) => d1 == d2 && go(p, q, fwd, back),
            (Pat::Repeat { p, min: m1, max: x1, mark: k1 }, Pat::Repeat { p: q, min: m2, max: x2, mark: k2 }) if k1 == k2 => {
                m1 == m2 && x1 == x2 && go(p, q, fwd, back)
            }
            (Pat::Cond(p, c), Pat::Cond(q, d)) => {
                let same_code = match (&c.code, &d.code) {
                    (CondCode::Native(f), CondCode::Native(g)) => Arc::ptr_eq(f, g),
                    (CondCode::Term { body: b1, names: n1 }, CondCode::Term { body: b2, names: n2 }) => b1 == b2 && n1 == n2,
                    _ => false,
                };
                // the same code reading corresponding variables (they are bound inside, so mapped by then)
                same_code
                    && c.vars.len() == d.vars.len()
                    && go(p, q, fwd, back)
                    && c.vars.iter().zip(d.vars.iter()).all(|(x, y)| fwd.get(*x as usize).copied().flatten() == Some(*y))
            }
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

/// Steps after which the instance check gives up with "not proven".
pub(crate) const INSTANCE_BUDGET: u32 = 1 << 20;

/// Is `a` an instance of `b`, that is, does `b` match every term `a` matches? Conditions are ignored (semantics
/// 6.3). The check matches `b` against `a` read as a term whose variables are unknowns: a variable of `b` may bind a
/// piece of `a`, a typed variable only a piece whose type is known to fit, a sequence variable of `b` a run of `a`'s
/// arguments (including `a`'s own sequence variables), and nothing else matches a variable of `a`.
///
/// Sound, not complete: `true` is a proof, `false` may also mean "not provable this way" (for example `f(y_, zs___)`
/// against `f(ws__)`), in which case the two rules stay in definition order. The search is bounded by
/// [`INSTANCE_BUDGET`] steps; past it the answer is `false`, which keeps the check sound.
pub(crate) fn instance_of(h: &Heap, a: &Pat, b: &Pat) -> bool {
    let g = Gen { h, steps: Cell::new(0) };
    let mut env: GEnv = vec![None; b.n_vars()];
    g.gen(b, a, &mut env, &mut |_| true)
}

struct Gen<'a> {
    h: &'a Heap,
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
    /// Is every term each pattern of `run` matches of type `ty`?
    fn run_has_type(&self, run: &[Pat], ty: Ty) -> bool {
        run.iter().all(|a| match a {
            Pat::Seq { ty: Some(t), .. } => subtype(*t, ty),
            Pat::Seq { .. } | Pat::Repeat { .. } | Pat::Opt { .. } => false,
            a => self.a_has_type(a, ty),
        })
    }

    fn a_has_type(&self, a: &Pat, ty: Ty) -> bool {
        match a {
            Pat::Lit(t) => has_type(self.h, t.get(self.h), ty),
            Pat::Blank { ty: Some(ta), .. } => subtype(*ta, ty),
            Pat::App { head, .. } => matches!(ty, Ty::Head(s) if head.get(self.h) == self.h.sym_term(s)),
            _ => false,
        }
    }

    /// The argument patterns of a literal compound with head `head`, if `t` is one.
    fn lit_args(&self, t: Raw, head: Raw) -> Option<Vec<Pat>> {
        match self.h.view(t.get(self.h)) {
            View::App { head: th, args } if Raw::new(th) == head => Some(args.iter().map(|&u| Pat::Lit(Raw::new(u))).collect()),
            _ => None,
        }
    }

    /// `b` generalises `a`; on success call `k` with the bindings.
    fn gen(&self, b: &Pat, a: &Pat, env: &mut GEnv, k: GK) -> bool {
        if self.tick() {
            return false;
        }
        let a = a.shape();
        match b.strip() {
            Pat::Named(v, p) => self.gen(p, a, env, &mut |env| self.bind(Some(*v), Piece::One(a.clone()), env, k)),
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
                    Pat::Lit(t) => match self.lit_args(*t, *head) {
                        Some(v) => {
                            expanded = v;
                            &expanded
                        }
                        None => return false,
                    },
                    _ => return false,
                };
                if info.ac {
                    if aargs.len() > 64 {
                        return false; // not proven: the multiset search below uses 64-bit sets
                    }
                    self.gen_ac(bargs, aargs, 0, env, k)
                } else {
                    self.gen_seq(bargs, aargs, env, k)
                }
            }
            Pat::Seq { .. } | Pat::Cond(..) | Pat::Repeat { .. } | Pat::KeyValue(_) | Pat::Opt { .. } => false,
        }
    }

    fn gen_seq(&self, bs: &[Pat], aas: &[Pat], env: &mut GEnv, k: GK) -> bool {
        let Some((b0, brest)) = bs.split_first() else { return aas.is_empty() && k(env) };
        if let Pat::Seq { var, min, ty, .. } = b0 {
            for len in 0..=aas.len() {
                let run = &aas[..len];
                if run.iter().map(Pat::min_len).sum::<usize>() < *min as usize {
                    continue;
                }
                if ty.is_some_and(|ty| !self.run_has_type(run, ty)) {
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
        let all = if aas.len() == 64 { u64::MAX } else { (1u64 << aas.len()) - 1 };
        let Some((b0, brest)) = bs.split_first() else { return used == all && k(env) };
        if let Pat::Seq { var, min, ty, .. } = b0 {
            if ty.is_some() {
                return false; // sound: a typed run under an AC head is not compared
            }
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
        if self.tick() {
            return false;
        }
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
