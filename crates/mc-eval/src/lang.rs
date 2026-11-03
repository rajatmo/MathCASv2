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
use crate::matcher::Binds;
use crate::native::{Native, NativeCx, Pattern, Reply};
use crate::pat::has_type;
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
    matches!(h.view(t), View::App { head, .. } if head == h.sym_term(cx.ev.k.names.failure))
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

/// `block(statements...)`.
struct Block;

impl Native for Block {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, stmts: &[T<'h>]) -> Res<Reply<'h>> {
        if stmts.is_empty() {
            return Ok(Reply::Done(cx.null()));
        }
        let mut cells = vec![];
        let escapes0 = cx.cx.escapes;
        let r = block_body(cx, stmts, &mut cells);
        let ev = cx.ev;
        match r {
            Ok(Reply::Done(v)) => {
                ev.end_block(cx.cx, &cells, escapes0, Some(v));
                Ok(Reply::Done(v))
            }
            Err(e) => {
                ev.end_block(cx.cx, &cells, escapes0, None);
                Err(e)
            }
            r => r,
        }
    }
}

/// The statements of a block; the cells it allocates go to `cells`. A last statement in a block without cells is
/// returned for the evaluator to evaluate (a tail call: deep recursion through blocks adds no native frames).
fn block_body<'h>(cx: &mut NativeCx<'_, '_, 'h>, stmts: &[T<'h>], cells: &mut Vec<i64>) -> Res<Reply<'h>> {
    let ev = cx.ev;
    let h = ev.heap;
    let sc = &ev.k.names.scopes;
    let (let_, var) = (h.sym_term(sc.let_), h.sym_term(sc.var));
    let mut pairs: Vec<(Sym, T<'h>)> = vec![];
    let mut last = cx.null();
    for (i, &s0) in stmts.iter().enumerate() {
        cx.tick(1)?;
        // a binding's name is not substituted (a later `let` of a name shadows the earlier one)
        let local = match h.view(s0) {
            View::App { head, args: &[p, v] } if head == let_ || head == var => Some((head == var, p, v)),
            _ => None,
        };
        if let Some((is_var, p, v)) = local {
            let Some(x) = sc.binder_name(h, p) else {
                return Ok(Reply::Done(failure(cx, "definition", &[h.string("a local binding must name a symbol")])));
            };
            let val = cx.eval(substitute(h, sc, v, &pairs))?;
            if is_failure(cx, val) {
                return Ok(Reply::Done(val));
            }
            if declared_type(h, sc, p).is_some_and(|ty| !has_type(h, val, ty)) {
                return Ok(Reply::Done(failure(cx, "type_mismatch", &[h.sym_term(x), val])));
            }
            let bound = if is_var {
                let (id, cell) = ev.alloc_cell(cx.cx, val);
                cells.push(id);
                cell
            } else {
                val
            };
            pairs.retain(|&(k, _)| k != x);
            pairs.push((x, bound));
            last = cx.null();
            continue;
        }
        let s = substitute(h, sc, s0, &pairs);
        if i + 1 == stmts.len() && cells.is_empty() {
            return Ok(Reply::Again(s));
        }
        last = cx.eval(s)?;
        if is_failure(cx, last) {
            return Ok(Reply::Done(last));
        }
    }
    Ok(Reply::Done(last))
}

/// `let(x, v)` or `var(x, v)` outside a block: the own value of `x`.
struct TopLocal;

impl Native for TopLocal {
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

/// `while(c, body)`.
struct While;

impl Native for While {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let &[c, body] = args else { return Ok(Reply::Decline) };
        loop {
            cx.tick(1)?;
            let v = cx.eval(c)?;
            if cx.is_false(v) {
                return Ok(Reply::Done(cx.null()));
            }
            if !cx.is_true(v) {
                return Ok(Reply::Done(if is_failure(cx, v) { v } else { failure(cx, "not_boolean", &[v]) }));
            }
            let r = cx.eval(body)?;
            if is_failure(cx, r) {
                return Ok(Reply::Done(r));
            }
        }
    }
}

/// `for(x, xs, body)`.
struct For;

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

impl Native for For {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let &[x, xs, body] = args else { return Ok(Reply::Decline) };
        let h = cx.heap();
        let sc = cx.ev.k.names.scopes;
        let Some(x) = sc.binder_name(h, x) else { return Ok(Reply::Decline) };
        let run = |cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>| -> Res<Option<T<'h>>> {
            cx.tick(1)?;
            let r = cx.eval(substitute(h, &sc, body, &[(x, v)]))?;
            Ok(is_failure(cx, r).then_some(r))
        };
        if let Some((a, b, step)) = int_range(cx, xs)? {
            let mut i = a;
            while (step > 0 && i <= b) || (step < 0 && i >= b) {
                if let Some(f) = run(cx, h.int(i))? {
                    return Ok(Reply::Done(f));
                }
                let Some(next) = i.checked_add(step) else { break };
                i = next;
            }
            return Ok(Reply::Done(cx.null()));
        }
        let xs = cx.eval(xs)?;
        if is_failure(cx, xs) {
            return Ok(Reply::Done(xs));
        }
        let Some(items) = cx.as_list(xs) else { return Ok(Reply::Done(failure(cx, "not_iterable", &[xs]))) };
        for &v in items {
            if let Some(f) = run(cx, v)? {
                return Ok(Reply::Done(f));
            }
        }
        Ok(Reply::Done(cx.null()))
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Rules as values: match, rewrite

/// A rule term compiled for matching: the pattern, the symbols of its variables and the right side.
struct Compiled<'h> {
    pat: Pattern,
    names: Vec<Sym>,
    rhs: T<'h>,
}

/// Compile `rule(l, r)` or `rule_delayed(l, r)`; `None` if `t` is not a rule, `Err` with a failure term if its
/// pattern is malformed.
fn compile<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Result<Option<Compiled<'h>>, T<'h>> {
    let h = cx.heap();
    let sc = &cx.ev.k.names.scopes;
    let View::App { head, args: &[l, r] } = h.view(t) else { return Ok(None) };
    if sym_of(h, head).is_none_or(|s| s != sc.rule && s != sc.rule_delayed) {
        return Ok(None);
    }
    let bad = |e: crate::pat::DefError| failure(cx, "definition", &[h.string(&e.to_string())]);
    let low = crate::syntax::pattern(h, l).map_err(bad)?;
    let pat = cx.ev.k.compile_pattern(h, low.pat).map_err(bad)?;
    Ok(Some(Compiled { pat, names: low.names, rhs: r }))
}

/// The rules of `t`: a rule, or a list of rules. `Ok(None)` if `t` is neither.
fn compile_all<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Result<Option<Vec<Compiled<'h>>>, T<'h>> {
    let items: Vec<T<'h>> = match cx.as_list(t) {
        Some(xs) => xs.to_vec(),
        None => vec![t],
    };
    let mut out = Vec::with_capacity(items.len());
    for r in items {
        match compile(cx, r)? {
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
    for r in rules {
        if ev.match_binds(cx.cx, &r.pat, t, b)? {
            let pairs: Vec<(Sym, T<'h>)> =
                r.names.iter().enumerate().map(|(v, &s)| (s, crate::subst::value(h, b, v as u16, nm.list))).collect();
            return Ok(Some(substitute(h, &nm.scopes, r.rhs, &pairs)));
        }
    }
    Ok(None)
}

/// `match(e, list(arms...))`.
struct Match;

impl Native for Match {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let &[e, arms] = args else { return Ok(Reply::Decline) };
        let Some(arms) = cx.as_list(arms) else { return Ok(Reply::Decline) };
        let mut b = Binds::default();
        for &arm in arms {
            cx.tick(1)?;
            let rule = match compile(cx, arm) {
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
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (e, rules, opts) = match *args {
            [e, r] => (e, r, None),
            [e, r, o] => (e, r, Some(o)),
            _ => return Ok(Reply::Decline),
        };
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
    k.register(h.sym("block"), a(Hold::All), Box::new(Block));
    k.register(h.sym("let"), fx(Hold::First), Box::new(TopLocal));
    k.register(h.sym("var"), fx(Hold::First), Box::new(TopLocal));
    k.register(h.sym("assign"), a(Hold::First), Box::new(Assign));
    k.register(h.sym("local_cell"), a(Hold::None), Box::new(LocalCell));
    k.register(h.sym("while"), a(Hold::All), Box::new(While));
    k.register(h.sym("for"), a(Hold::All), Box::new(For));
    k.register(h.sym("match"), a(Hold::Rest), Box::new(Match));
    k.register(h.sym("rewrite"), a(Hold::None), Box::new(Rewrite));
    k.register(h.sym("check_options"), a(Hold::None), Box::new(CheckOptions));
    k.register(h.sym("option_value"), a(Hold::None), Box::new(OptionValue));
    // a delayed rule holds its right side, a condition its test (they are evaluated when the rule fires)
    k.set_attributes(h.sym(mc_api::names::RULE_DELAYED), a(Hold::Rest));
    k.set_attributes(h.sym("condition"), a(Hold::Rest));
}
