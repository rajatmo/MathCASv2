//! The language forms (semantics 6.7, 04 sections 3.4 to 3.6): closures, blocks with immutable and mutable locals,
//! loops, `match`, and rule application as a function (`rewrite`). Each is a native over the term shapes
//! `mc-syntax` lowers to (its decisions C.5, C.7, C.8, C.10).
//!
//! | term | value |
//! |---|---|
//! | `fn(list(params), body)` | the closure itself (held); applying it is the evaluator's ([`crate::eval`]) |
//! | `block(s1, ..., sn)` | statements in order; `let(x, v)` binds `x` to the value of `v` in the rest of the block, `var(x, v)` to a new cell ([`crate::cells`]); the value of the last statement (`null` if it is a binding); a statement whose value is a failure ends the block with it |
//! | `let(x, v)`, `var(x, v)` outside a block | define the own value of `x` (a top-level binding) |
//! | `assign(x, v)` | writes cell `x` (or, for a symbol, defines its own value); the value `v` |
//! | `local_cell(id)` | the cell's value |
//! | `while(c, body)` | `null` once `c` is `false`; the failure if `c` or `body` fails; `failure(not_boolean, c)` if `c` is neither |
//! | `for(x, xs, body)` | `body` for each element of the list `xs` (or of `range(...)` with machine-integer bounds, without building it), `null` |
//! | `match(e, [p :> r, ...])` | `r` of the first arm whose pattern matches `e`, else `failure(no_match, e)` |
//! | `rewrite(e, rules)` | every subterm, outermost first, replaced by the first rule that matches it, in one pass |
//! | `rewrite(e, rules, strategy: fixed_point)` | passes until nothing changes (evaluating after each) |
//! | `rewrite(e, rules, strategy: innermost)` | passes in which arguments are rewritten before their call, until nothing changes |
//! | `check_options(options(...), [names])`, `option_value(options(...), name, default)` | named-argument support for `fn` options (04 section 7) |
//!
//! Rules are `rule(l, r)` (right side evaluated when the rule is made) or `rule_delayed(l, r)` (held); substitution
//! into right sides is hygienic (6.6). Every loop charges fuel per iteration and every walk over a term is iterative.
use crate::defs::{Attributes, Hold};
use crate::eval::{Kernel, Res};
use crate::machine::{Frame, FormOut};
use crate::matcher::Binds;
use crate::native::{Native, NativeCx, Pattern, Reply};
use crate::pat::{has_type, Pat};
use crate::subst::substitute;
use mc_core::{Heap, Store, Sym, View, T};
use rustc_hash::FxHashMap;

fn sym_of(h: &Heap, t: T<'_>) -> Option<Sym> {
    match h.view(t) {
        View::Sym(s) => Some(s),
        _ => None,
    }
}

/// Is `t` a failure term?
fn is_failure<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> bool {
    let h = cx.heap();
    h.packed(t).is_none() && matches!(h.view(t), View::App { head, .. } if head == h.sym_term(cx.ev.k.names.failure))
}

/// `failure(tag, details...)`
fn failure<'h>(cx: &NativeCx<'_, '_, 'h>, tag: &str, details: &[T<'h>]) -> T<'h> {
    let h = cx.heap();
    let mut args = vec![h.sym_term(h.sym(tag))];
    args.extend_from_slice(details);
    h.call(cx.ev.k.names.failure, &args)
}

/// The type a local binding is declared with, `let(typed(x, T), v)`, if it is an atomic type.
fn declared_type(h: &Heap, sc: &crate::subst::Scopes, p: T<'_>) -> Option<crate::pat::Ty> {
    match h.view(p) {
        View::App { head, args: &[_, ty] } if head == h.sym_term(sc.typed) => crate::syntax::type_of(h, ty).ok(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Closures and blocks

/// `fn(list(params), body)`: a closure is a normal form.
struct Closure;

impl Native for Closure {
    fn call<'h>(&self, _cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        Ok(Reply::Done(t))
    }
}

/// `let(x, v)` or `var(x, v)` outside a block: the own value of `x`.
struct TopLocal;

impl Native for TopLocal {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let sc = cx.ev.k.names.scopes;
        let [p, v] = args else { return Ok(Reply::Decline) };
        let Some(x) = sc.binder_name(h, *p) else { return Ok(Reply::Decline) };
        if declared_type(h, &sc, *p).is_some_and(|ty| !has_type(h, *v, ty)) {
            return Ok(Reply::Done(failure(cx, "type_mismatch", &[h.sym_term(x), *v])));
        }
        cx.set_own(x, *v)?;
        Ok(Reply::Done(cx.null()))
    }
}

/// `assign(target, v)`.
struct Assign;

impl Native for Assign {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [x, v] = args else { return Ok(Reply::Decline) };
        match h.view(*x) {
            View::App { head, args: &[id] } if head == h.sym_term(cx.ev.k.names.local_cell) => {
                let View::Int(id) = h.view(id) else { return Ok(Reply::Decline) };
                let ev = cx.ev;
                if !ev.write_cell(cx.cx, id, *v)? {
                    return Ok(Reply::Decline);
                }
            }
            View::Sym(s) => cx.set_own(s, *v)?,
            _ => return Ok(Reply::Decline),
        }
        Ok(Reply::Done(*v))
    }
}

/// `local_cell(id)`: the cell's value.
struct LocalCell;

impl Native for LocalCell {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[id] = args else { return Ok(Reply::Decline) };
        let View::Int(id) = h.view(id) else { return Ok(Reply::Decline) };
        let ev = cx.ev;
        Ok(ev.read_cell(cx.cx, id).map_or(Reply::Decline, Reply::Done))
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Loops

/// The integers of `range(n)` (1 to n), `range(a, b)` or `range(a, b, step)` when the bounds are machine integers.
fn int_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, xs: T<'h>) -> Res<Option<(i64, i64, i64)>> {
    let h = cx.heap();
    let View::App { head, args } = h.view(xs) else { return Ok(None) };
    if !(1..=3).contains(&args.len()) || sym_of(h, head).is_none_or(|s| h.sym_name(s) != "range") {
        return Ok(None);
    }
    let mut v = [1i64, 0, 1];
    for (i, &a) in args.iter().enumerate() {
        let View::Int(n) = h.view(cx.eval(a)?) else { return Ok(None) };
        v[i] = n;
    }
    Ok(match args.len() {
        1 => Some((1, v[0], 1)),
        _ if v[2] == 0 => None,
        _ => Some((v[0], v[1], v[2])),
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Forms run as frames: block, while, for

/// The language forms the evaluator runs as frames of its stack ([`crate::machine`]): each statement, condition and
/// iteration is evaluated above the form's frame, so recursion through a block or a loop body adds no Rust frames
/// (decision 1a.3). The form's argument modes hold everything; its replies are a native's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Form {
    /// `block(statements...)`
    Block,
    /// `while(c, body)`
    While,
    /// `for(x, xs, body)`
    For,
}

/// A block in progress: the statement being evaluated, the bindings so far (substituted into later statements),
/// the cells it allocated, the escape count at its start.
pub(crate) struct BlockState<'h> {
    stmts: &'h [T<'h>],
    i: usize,
    pairs: Vec<(Sym, T<'h>)>,
    cells: Vec<i64>,
    escapes0: u32,
    /// the value of statement `i` is bound: `var` or `let`, the binding's pattern and name
    bind: Option<(bool, T<'h>, Sym)>,
    /// the value of the last statement (`null` after a binding)
    last: T<'h>,
}

/// A `while` loop in progress: waiting for its condition or for its body.
pub(crate) struct WhileState<'h> {
    c: T<'h>,
    body: T<'h>,
    in_body: bool,
}

/// A `for` loop in progress: the name bound, the body, what remains to iterate.
pub(crate) struct ForState<'h> {
    x: Sym,
    body: T<'h>,
    iter: Iter<'h>,
}

/// What a `for` loop iterates.
enum Iter<'h> {
    /// machine integers from `next` to `last` by `step` (`None` once a step would overflow)
    Range { next: Option<i64>, last: i64, step: i64 },
    /// the elements of a list from `j`
    List { items: &'h [T<'h>], j: usize },
}

/// Start form `form` on the arguments of its call.
pub(crate) fn start<'h>(cx: &mut NativeCx<'_, '_, 'h>, form: Form, args: &'h [T<'h>]) -> Res<FormOut<'h>> {
    match form {
        Form::Block => {
            let h = cx.heap();
            let sc = &cx.ev.k.names.scopes;
            match *args {
                [] => Ok(FormOut::Reply(Reply::Done(cx.null()))),
                // one statement that binds nothing: the block is that statement (a tail call, H.12)
                [s] if !matches!(h.view(s), View::App { head, args: &[_, _] } if head == h.sym_term(sc.let_) || head == h.sym_term(sc.var)) => {
                    cx.tick(1)?;
                    Ok(FormOut::Reply(Reply::Again(s)))
                }
                _ => {
                    let null = cx.null();
                    let mut st = cx.cx.blocks.pop().unwrap_or_else(|| {
                        Box::new(BlockState { stmts: args, i: 0, pairs: vec![], cells: vec![], escapes0: 0, bind: None, last: null })
                    });
                    st.stmts = args;
                    st.escapes0 = cx.cx.escapes;
                    st.last = null;
                    block_next(cx, st)
                }
            }
        }
        Form::While => {
            let &[c, body] = args else { return Ok(FormOut::Reply(Reply::Decline)) };
            cx.tick(1)?;
            Ok(FormOut::Eval(Frame::While(WhileState { c, body, in_body: false }), c))
        }
        Form::For => for_start(cx, args),
    }
}

/// Resume a form's frame with the value `v` it waited for.
pub(crate) fn resume<'h>(cx: &mut NativeCx<'_, '_, 'h>, frame: Frame<'h>, v: T<'h>) -> Res<FormOut<'h>> {
    match frame {
        Frame::Block(st) => block_resume(cx, st, v),
        Frame::While(st) => while_resume(cx, st, v),
        Frame::For(st) => {
            if is_failure(cx, v) {
                return Ok(FormOut::Reply(Reply::Done(v)));
            }
            for_next(cx, st)
        }
        // invariant: evaluation levels resume in the machine; only form frames reach here
        Frame::Eval(_) => unreachable!("an evaluation level is not a form"),
    }
}

/// A form's frame is unwound by an abort: a block frees its cells unless they escaped.
pub(crate) fn abort<'h>(cx: &mut NativeCx<'_, '_, 'h>, frame: Frame<'h>) {
    if let Frame::Block(st) = frame {
        end_block(cx, st, None);
    }
}

/// A block's result: its cells are freed unless the value or an effect may hold them.
fn block_done<'h>(cx: &mut NativeCx<'_, '_, 'h>, st: Box<BlockState<'h>>, v: T<'h>) -> Res<FormOut<'h>> {
    end_block(cx, st, Some(v));
    Ok(FormOut::Reply(Reply::Done(v)))
}

/// Blocks kept for reuse per task (their buffers keep their capacity).
const BLOCK_POOL: usize = 32;

/// The end of a block: its cells, then its state back to the task's pool.
fn end_block<'h>(cx: &mut NativeCx<'_, '_, 'h>, mut st: Box<BlockState<'h>>, v: Option<T<'h>>) {
    let ev = cx.ev;
    ev.end_block(cx.cx, &st.cells, st.escapes0, v);
    if cx.cx.blocks.len() < BLOCK_POOL {
        st.i = 0;
        st.pairs.clear();
        st.cells.clear();
        st.bind = None;
        cx.cx.blocks.push(st);
    }
}

/// Run a block from statement `st.i`: bind or evaluate it. The last statement of a block without cells is returned
/// for the evaluator to evaluate (a tail call, decision H.12).
fn block_next<'h>(cx: &mut NativeCx<'_, '_, 'h>, mut st: Box<BlockState<'h>>) -> Res<FormOut<'h>> {
    let ev = cx.ev;
    let h = ev.heap;
    let sc = &ev.k.names.scopes;
    let Some(&s0) = st.stmts.get(st.i) else {
        let v = st.last;
        return block_done(cx, st, v);
    };
    if let Err(a) = cx.tick(1) {
        abort(cx, Frame::Block(st));
        return Err(a);
    }
    // a binding's name is not substituted (a later `let` of a name shadows the earlier one)
    match h.view(s0) {
        View::App { head, args: &[p, v] } if head == h.sym_term(sc.let_) || head == h.sym_term(sc.var) => {
            let Some(x) = sc.binder_name(h, p) else {
                let f = failure(cx, "definition", &[h.string("a local binding must name a symbol")]);
                return block_done(cx, st, f);
            };
            st.bind = Some((head == h.sym_term(sc.var), p, x));
            let v = substitute(h, sc, v, &st.pairs);
            Ok(FormOut::Eval(Frame::Block(st), v))
        }
        _ => {
            let s = substitute(h, sc, s0, &st.pairs);
            if st.i + 1 == st.stmts.len() && st.cells.is_empty() {
                return Ok(FormOut::Reply(Reply::Again(s)));
            }
            st.bind = None;
            Ok(FormOut::Eval(Frame::Block(st), s))
        }
    }
}

/// A block's statement has value `v`.
fn block_resume<'h>(cx: &mut NativeCx<'_, '_, 'h>, mut st: Box<BlockState<'h>>, v: T<'h>) -> Res<FormOut<'h>> {
    let ev = cx.ev;
    let h = ev.heap;
    // a statement whose value is a failure ends the block with it (H.9)
    if is_failure(cx, v) {
        return block_done(cx, st, v);
    }
    match st.bind.take() {
        Some((is_var, p, x)) => {
            if declared_type(h, &ev.k.names.scopes, p).is_some_and(|ty| !has_type(h, v, ty)) {
                let f = failure(cx, "type_mismatch", &[h.sym_term(x), v]);
                return block_done(cx, st, f);
            }
            let bound = if is_var {
                let (id, cell) = ev.alloc_cell(cx.cx, v);
                st.cells.push(id);
                cell
            } else {
                v
            };
            st.pairs.retain(|&(k, _)| k != x);
            st.pairs.push((x, bound));
            st.last = cx.null();
        }
        None => st.last = v,
    }
    st.i += 1;
    block_next(cx, st)
}

/// A `while` loop's condition or body has value `v`.
fn while_resume<'h>(cx: &mut NativeCx<'_, '_, 'h>, mut st: WhileState<'h>, v: T<'h>) -> Res<FormOut<'h>> {
    if st.in_body {
        if is_failure(cx, v) {
            return Ok(FormOut::Reply(Reply::Done(v)));
        }
        cx.tick(1)?;
        st.in_body = false;
        let c = st.c;
        return Ok(FormOut::Eval(Frame::While(st), c));
    }
    if cx.is_false(v) {
        return Ok(FormOut::Reply(Reply::Done(cx.null())));
    }
    if !cx.is_true(v) {
        let f = if is_failure(cx, v) { v } else { failure(cx, "not_boolean", &[v]) };
        return Ok(FormOut::Reply(Reply::Done(f)));
    }
    st.in_body = true;
    let body = st.body;
    Ok(FormOut::Eval(Frame::While(st), body))
}

/// Start `for(x, xs, body)`: a machine-integer `range(...)` is iterated without building it, anything else is
/// evaluated and must be a list.
fn for_start<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &'h [T<'h>]) -> Res<FormOut<'h>> {
    let &[x, xs, body] = args else { return Ok(FormOut::Reply(Reply::Decline)) };
    let h = cx.heap();
    let Some(x) = cx.ev.k.names.scopes.binder_name(h, x) else { return Ok(FormOut::Reply(Reply::Decline)) };
    let iter = match int_range(cx, xs)? {
        Some((a, b, step)) => Iter::Range { next: Some(a), last: b, step },
        None => {
            let xs = cx.eval(xs)?;
            if is_failure(cx, xs) {
                return Ok(FormOut::Reply(Reply::Done(xs)));
            }
            let Some(items) = cx.as_list(xs) else {
                return Ok(FormOut::Reply(Reply::Done(failure(cx, "not_iterable", &[xs]))));
            };
            Iter::List { items, j: 0 }
        }
    };
    for_next(cx, ForState { x, body, iter })
}

/// The next iteration of a `for` loop, or its end (`null`).
fn for_next<'h>(cx: &mut NativeCx<'_, '_, 'h>, mut st: ForState<'h>) -> Res<FormOut<'h>> {
    let h = cx.heap();
    let v = match &mut st.iter {
        Iter::Range { next, last, step } => match *next {
            Some(i) if (*step > 0 && i <= *last) || (*step < 0 && i >= *last) => {
                *next = i.checked_add(*step);
                h.int(i)
            }
            _ => return Ok(FormOut::Reply(Reply::Done(cx.null()))),
        },
        Iter::List { items, j } => match items.get(*j) {
            Some(&v) => {
                *j += 1;
                v
            }
            None => return Ok(FormOut::Reply(Reply::Done(cx.null()))),
        },
    };
    cx.tick(1)?;
    let body = substitute(h, &cx.ev.k.names.scopes, st.body, &[(st.x, v)]);
    Ok(FormOut::Eval(Frame::For(st), body))
}

// ---------------------------------------------------------------------------------------------------------------
// Rules as values: match, rewrite

/// A rule term compiled for matching: the pattern, the symbols of its variables and the right side.
struct Compiled<'h> {
    pat: Pattern,
    names: Vec<Sym>,
    rhs: T<'h>,
    /// For `rewrite`, when the left side is a call of an associative-commutative head without a sequence variable:
    /// the same pattern with an implicit rest (`f(p, q, rest__)`), the rest's variable and the head, so the rule also
    /// rewrites part of a longer call (`a + b + c` with `a + b -> r` gives `r + c`).
    part: Option<(Pattern, u16, T<'h>)>,
}

/// Compile `rule(l, r)` or `rule_delayed(l, r)`; `None` if `t` is not a rule, `Err` with a failure term if its
/// pattern is malformed.
fn compile<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, parts: bool) -> Result<Option<Compiled<'h>>, T<'h>> {
    let h = cx.heap();
    let sc = &cx.ev.k.names.scopes;
    let View::App { head, args: &[l, r] } = h.view(t) else { return Ok(None) };
    if sym_of(h, head).is_none_or(|s| s != sc.rule && s != sc.rule_delayed) {
        return Ok(None);
    }
    let bad = |e: crate::pat::DefError| failure(cx, "definition", &[h.string(&e.to_string())]);
    let low = crate::syntax::pattern(h, l).map_err(bad)?;
    // a left side without pattern constructs lowers to one literal term (decided by a symbol-mask test, so it depends
    // on symbol numbering): take a literal call apart, so an AC call still gets its implicit rest below
    let lhs = parts.then(|| match &low.pat {
        Pat::Lit(t) => match h.view(t.get(h)) {
            View::App { head, args } => crate::pat::app(head, args.iter().map(|&a| crate::pat::lit(a)).collect()),
            _ => low.pat.clone(),
        },
        p => p.clone(),
    });
    let pat = cx.ev.k.compile_pattern(h, low.pat).map_err(bad)?;
    // the left side as written is a call without sequence variables (compiled, an all-literal call is one literal
    // term, so the head's properties are read from the version with the rest)
    let part = match lhs {
        Some(Pat::App { head, mut args, info }) if !args.iter().any(Pat::is_seq) => {
            let rest = pat.nvars as u16;
            args.push(Pat::Seq { var: Some(rest), min: 1 });
            let p = cx.ev.k.compile_pattern(h, Pat::App { head, args, info }).map_err(bad)?;
            matches!(&p.pat, Pat::App { info, .. } if info.ac).then(|| (p, rest, head.get(h)))
        }
        _ => None,
    };
    Ok(Some(Compiled { pat, names: low.names, rhs: r, part }))
}

/// The rules of `t`: a rule, or a list of rules. `Ok(None)` if `t` is neither.
fn compile_all<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Result<Option<Vec<Compiled<'h>>>, T<'h>> {
    let items: Vec<T<'h>> = match cx.as_list(t) {
        Some(xs) => xs.to_vec(),
        None => vec![t],
    };
    let mut out = Vec::with_capacity(items.len());
    for r in items {
        match compile(cx, r, true)? {
            Some(c) => out.push(c),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// The replacement the first of `rules` that matches `t` gives, not evaluated.
fn apply<'h>(cx: &mut NativeCx<'_, '_, 'h>, rules: &[Compiled<'h>], t: T<'h>, b: &mut Binds<'h>) -> Res<Option<T<'h>>> {
    let ev = cx.ev;
    let h = ev.heap;
    let nm = &ev.k.names;
    let replace = |b: &Binds<'h>, r: &Compiled<'h>| {
        let pairs: Vec<(Sym, T<'h>)> =
            r.names.iter().enumerate().map(|(v, &s)| (s, crate::subst::value(h, b, v as u16, nm.list))).collect();
        substitute(h, &nm.scopes, r.rhs, &pairs)
    };
    for r in rules {
        if ev.match_binds(cx.cx, &r.pat, t, b)? {
            return Ok(Some(replace(b, r)));
        }
        // part of a longer call of the same AC head: the replacement joins the arguments it did not use
        if let Some((p, rest, head)) = &r.part {
            if ev.match_binds(cx.cx, p, t, b)? {
                let mut args = vec![replace(b, r)];
                args.extend_from_slice(b.seq(*rest));
                return Ok(Some(h.app(*head, &args)));
            }
        }
    }
    Ok(None)
}

/// `match(e, list(arms...))`.
struct Match;

impl Native for Match {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let &[e, arms] = args else { return Ok(Reply::Decline) };
        let Some(arms) = cx.as_list(arms) else { return Ok(Reply::Decline) };
        let mut b = Binds::default();
        for &arm in arms {
            cx.tick(1)?;
            let rule = match compile(cx, arm, false) {
                Ok(Some(r)) => r,
                Ok(None) => return Ok(Reply::Decline),
                Err(f) => return Ok(Reply::Done(f)),
            };
            if let Some(r) = apply(cx, std::slice::from_ref(&rule), e, &mut b)? {
                return Ok(Reply::Again(r));
            }
        }
        Ok(Reply::Done(failure(cx, mc_api::names::failure::NO_MATCH, &[e])))
    }
}

/// The order in which a rewriting pass visits subterms.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Order {
    /// a term before its parts; a replaced term is not entered
    Outermost,
    /// the parts of a term before the term (rebuilt from the rewritten parts)
    Innermost,
}

/// One pass of `rules` over `t`: the rewritten term and whether any rule applied.
fn pass<'h>(cx: &mut NativeCx<'_, '_, 'h>, rules: &[Compiled<'h>], t: T<'h>, order: Order) -> Res<(T<'h>, bool)> {
    let h = cx.heap();
    let mut b = Binds::default();
    let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    let mut changed = false;
    let mut parts = vec![];
    while let Some((u, expanded)) = stack.pop() {
        if !expanded {
            if done.contains_key(&u) {
                continue;
            }
            cx.tick(1)?;
            if order == Order::Outermost {
                if let Some(r) = apply(cx, rules, u, &mut b)? {
                    changed = true;
                    done.insert(u, r);
                    continue;
                }
            }
            match h.view(u) {
                View::App { head, args } => {
                    stack.push((u, true));
                    stack.push((head, false));
                    stack.extend(args.iter().map(|&a| (a, false)));
                }
                _ => {
                    let r = if order == Order::Innermost { apply(cx, rules, u, &mut b)? } else { None };
                    changed |= r.is_some();
                    done.insert(u, r.unwrap_or(u));
                }
            }
            continue;
        }
        // invariant: only compounds are pushed for expansion
        let View::App { head, args } = h.view(u) else { unreachable!("only compounds are expanded") };
        parts.clear();
        parts.extend(args.iter().map(|a| done[a]));
        let mut v = if done[&head] == head && *parts == *args { u } else { h.app(done[&head], &parts) };
        if order == Order::Innermost {
            if let Some(r) = apply(cx, rules, v, &mut b)? {
                changed = true;
                v = r;
            }
        }
        done.insert(u, v);
    }
    Ok((done[&t], changed))
}

/// `rewrite(e, rules)`, `rewrite(e, rules, options(strategy -> s))`.
struct Rewrite;

impl Native for Rewrite {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (e, rules, opts) = match *args {
            [e, r] => (e, r, None),
            [e, r, o] => (e, r, Some(o)),
            _ => return Ok(Reply::Decline),
        };
        // a list of rule lists (`solve`'s solutions): one result per list (decision p6core.13)
        if let Some(sets) = cx.as_list(rules).filter(|rs| !rs.is_empty() && rs.iter().all(|&r| cx.as_list(r).is_some())) {
            let head = h.sym_term(h.sym("rewrite"));
            let calls: Vec<T<'h>> = sets
                .iter()
                .map(|&set| match opts {
                    Some(o) => h.app(head, &[e, set, o]),
                    None => h.app(head, &[e, set]),
                })
                .collect();
            cx.tick(calls.len() as u64)?;
            return Ok(Reply::Again(cx.list(&calls)));
        }
        let mut strategy = "once".to_string();
        if let Some(o) = opts {
            let Some(items) = options(h, o) else { return Ok(Reply::Decline) };
            for (k, v) in items {
                match (h.sym_name(k).as_str(), sym_of(h, v).map(|s| h.sym_name(s))) {
                    ("strategy", Some(s)) if ["once", "outermost", "fixed_point", "innermost"].contains(&s.as_str()) => {
                        strategy = s
                    }
                    ("strategy", _) => return Ok(Reply::Done(failure(cx, "bad_option", &[h.sym_term(k), v]))),
                    _ => return Ok(Reply::Done(failure(cx, "unknown_option", &[h.sym_term(k)]))),
                }
            }
        }
        let rules = match compile_all(cx, rules) {
            Ok(Some(r)) => r,
            Ok(None) => return Ok(Reply::Decline),
            Err(f) => return Ok(Reply::Done(f)),
        };
        let order = if strategy == "innermost" { Order::Innermost } else { Order::Outermost };
        if strategy == "once" || strategy == "outermost" {
            let (r, changed) = pass(cx, &rules, e, order)?;
            return Ok(if changed { Reply::Again(r) } else { Reply::Done(e) });
        }
        // to a fixed point: each pass evaluated, until a pass changes nothing (fuel bounds the number of passes)
        let mut cur = e;
        loop {
            let (r, changed) = pass(cx, &rules, cur, order)?;
            if !changed {
                return Ok(Reply::Done(cur));
            }
            let next = cx.eval(r)?;
            if next == cur || is_failure(cx, next) {
                return Ok(Reply::Done(next));
            }
            cur = next;
        }
    }
}

/// The `(name, value)` pairs of `options(rule(name, value), ...)`.
fn options<'h>(h: &'h Heap, o: T<'h>) -> Option<Vec<(Sym, T<'h>)>> {
    let View::App { head, args } = h.view(o) else { return None };
    if sym_of(h, head).is_none_or(|s| h.sym_name(s) != "options") {
        return None;
    }
    args.iter()
        .map(|&r| match h.view(r) {
            View::App { head, args: &[k, v] } if sym_of(h, head).is_some_and(|s| h.sym_name(s) == mc_api::names::RULE) => {
                sym_of(h, k).map(|k| (k, v))
            }
            _ => None,
        })
        .collect()
}

/// `check_options(options(...), list("name", ...))`: `null`, or `failure(unknown_option, name)`.
struct CheckOptions;

impl Native for CheckOptions {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[o, known] = args else { return Ok(Reply::Decline) };
        let (Some(items), Some(known)) = (options(h, o), cx.as_list(known)) else { return Ok(Reply::Decline) };
        for (k, _) in items {
            let name = h.sym_name(k);
            if !known.iter().any(|&n| matches!(h.view(n), View::Str(s) if s == name)) {
                return Ok(Reply::Done(failure(cx, "unknown_option", &[h.sym_term(k)])));
            }
        }
        Ok(Reply::Done(cx.null()))
    }
}

/// `option_value(options(...), "name", default)`: the value given for the option, else the default.
struct OptionValue;

impl Native for OptionValue {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[o, name, default] = args else { return Ok(Reply::Decline) };
        let (Some(items), View::Str(name)) = (options(h, o), h.view(name)) else { return Ok(Reply::Decline) };
        // the last occurrence of a repeated option wins
        let v = items.iter().rev().find(|(k, _)| h.sym_name(*k) == name).map_or(default, |&(_, v)| v);
        Ok(Reply::Done(v))
    }
}

/// Register the language natives.
pub(crate) fn register(k: &mut Kernel, h: &Heap) {
    let a = |hold| Attributes { hold, ..Attributes::default() };
    let fx = |hold| Attributes { hold, effect: true, ..Attributes::default() };
    k.register(h.sym("fn"), a(Hold::All), Box::new(Closure));
    k.register_form(h.sym("block"), a(Hold::All), Form::Block);
    // assignments keep failures (decision p6core.25)
    let keep = |at: Attributes| Attributes { handles_failures: true, ..at };
    k.register(h.sym("let"), keep(fx(Hold::First)), Box::new(TopLocal));
    k.register(h.sym("var"), keep(fx(Hold::First)), Box::new(TopLocal));
    k.register(h.sym("assign"), keep(a(Hold::First)), Box::new(Assign));
    k.register(h.sym("local_cell"), a(Hold::None), Box::new(LocalCell));
    k.register_form(h.sym("while"), a(Hold::All), Form::While);
    k.register_form(h.sym("for"), a(Hold::All), Form::For);
    k.register(h.sym("match"), a(Hold::Rest), Box::new(Match));
    k.register(h.sym("rewrite"), a(Hold::None), Box::new(Rewrite));
    k.register(h.sym("check_options"), a(Hold::None), Box::new(CheckOptions));
    k.register(h.sym("option_value"), a(Hold::None), Box::new(OptionValue));
    // a delayed rule holds its right side, a condition its test (they are evaluated when the rule fires)
    k.set_attributes(h.sym(mc_api::names::RULE_DELAYED), a(Hold::Rest));
    k.set_attributes(h.sym("condition"), a(Hold::Rest));
}
