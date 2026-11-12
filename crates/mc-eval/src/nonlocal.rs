//! Non-local results (item 9B): `throw` and `catch`, `sow` and `reap`; and the pattern head
//! `repeated` (matched by `crate::matcher`, C9B.4).
//!
//! | term | value |
//! |---|---|
//! | `throw(v)`, `throw(v, tag)` | `failure(thrown, v)`, `failure(thrown, v, tag)` |
//! | `catch(e)` | `e` evaluated (held); the value `v` of the first untagged `throw(v)` it reached, else its value |
//! | `catch(e, form)` | the value of the first `throw(v, tag)` whose `tag` matches the pattern `form` |
//! | `catch(e, form, f)` | `f(v, tag)` for that throw |
//! | `sow(e)`, `sow(e, tag)`, `sow(e, [tag, ...])` | `e`; collected by the innermost enclosing `reap` whose pattern matches the tag (once per tag of a list) |
//! | `reap(e)` | `[value of e, [[e1, e2, ...]]]`, the values sown untagged or with any tag, grouped by tag in order of first use (`[value, []]` when none) |
//! | `reap(e, p)`, `reap(e, [p1, ...])` | the groups of the tags matching `p` (per pattern for a list) |
//! | `reap(e, p, f)` | `f(tag, values)` for each group instead of the list of values |
//!
//! **Throw is a failure value** (semantics 10, decision C9B.2): a thrown value travels as `failure(thrown, ...)`,
//! so it ends blocks and loops and propagates through strict calls exactly like any failure, and needs no new
//! control mechanism in the evaluator. Terms that keep failures (lists, the results of `map` and other natives)
//! hold it where it was made; `catch` then takes the first thrown failure in its value, depth first and left to
//! right, which is the first throw reached in evaluation order (arguments are evaluated left to right, innermost
//! first). A throw that `catch` does not match leaves it as the result, the failure going on to an outer catch.
//! The one difference from an exception: work after a throw inside a term that keeps failures still runs (its
//! effects happen), though its result is discarded.
//!
//! **Sow is an effect** collected per `reap` on a stack the two natives share (decision C9B.3). Both are
//! marked `effect`, so they run in order on the frontier, never speculatively. The stack holds handles of the
//! running evaluation only (a `reap` pops its frame before it returns, also when its body aborts), so it needs no
//! tracing: no safe point falls inside an evaluation.
use crate::defs::{Attributes, Hold};
use crate::eval::{Kernel, Res};
use crate::native::{Native, NativeCx, Reply};
use mc_core::{Heap, Raw, Store, View, T};
use parking_lot::Mutex;
use std::sync::Arc;

/// The tag of a thrown failure: `failure(thrown, v)` or `failure(thrown, v, tag)`.
const THROWN: &str = "thrown";

/// `failure(thrown, v[, tag])` as `(v, tag)`.
fn thrown<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<(T<'h>, Option<T<'h>>)> {
    let h = cx.heap();
    if h.packed(t).is_some() {
        return None;
    }
    let View::App { head, args } = h.view(t) else { return None };
    if head != h.sym_term(cx.ev.k.names.failure) {
        return None;
    }
    match args {
        [k, v] if is_thrown_tag(h, *k) => Some((*v, None)),
        [k, v, tag] if is_thrown_tag(h, *k) => Some((*v, Some(*tag))),
        _ => None,
    }
}

fn is_thrown_tag(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::Sym(s) if h.sym_name(s) == THROWN)
}

/// The first thrown failure in `t`, depth first and left to right (heads before arguments); `t` itself first.
/// Charges fuel per subterm visited; terms without the failure symbol are skipped whole.
fn first_thrown<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let fb = h.mask(h.sym_term(cx.ev.k.names.failure));
    let mut stack = vec![t];
    let mut n = 0u64;
    while let Some(u) = stack.pop() {
        if h.mask(u) & fb == 0 || h.packed(u).is_some() {
            continue;
        }
        n += 1;
        if n.is_multiple_of(256) {
            cx.tick(1)?;
        }
        if thrown(cx, u).is_some() {
            return Ok(Some(u));
        }
        if let View::App { head, args } = h.view(u) {
            stack.extend(args.iter().rev().copied());
            stack.push(head);
        }
    }
    Ok(None)
}

/// Does `tag` match the pattern term `p`?
fn tag_matches<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: T<'h>, tag: T<'h>) -> Res<bool> {
    let h = cx.heap();
    if p == tag {
        return Ok(true);
    }
    let Ok(lowered) = crate::syntax::pattern(h, p) else { return Ok(false) };
    let Ok(pat) = cx.compile_pattern(lowered.pat) else { return Ok(false) };
    cx.matches(&pat, tag)
}

/// Whether a pattern term is `_` (matches every tag, also the missing one).
fn is_any(h: &Heap, p: T<'_>) -> bool {
    matches!(h.view(p), View::App { head, args: [] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "any"))
}

/// `throw(v)`, `throw(v, tag)`.
struct Throw;

impl Native for Throw {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let f = cx.ev.k.names.failure;
        let k = h.sym_term(h.sym(THROWN));
        Ok(match args {
            [v] => Reply::Done(h.call(f, &[k, *v])),
            [v, tag] => Reply::Done(h.call(f, &[k, *v, *tag])),
            _ => Reply::Decline,
        })
    }
}

/// `catch(e)`, `catch(e, form)`, `catch(e, form, f)`.
struct Catch;

impl Native for Catch {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if args.is_empty() || args.len() > 3 {
            return Ok(Reply::Decline);
        }
        let r = cx.eval(args[0])?;
        let Some(f) = first_thrown(cx, r)? else { return Ok(Reply::Done(r)) };
        // invariant: first_thrown returns thrown failures only
        let (v, tag) = thrown(cx, f).expect("a thrown failure");
        let caught = match (tag, args.get(1)) {
            (None, None) => true,
            (Some(tag), Some(&form)) => tag_matches(cx, form, tag)?,
            _ => false,
        };
        if !caught {
            return Ok(Reply::Done(f));
        }
        Ok(match (args.get(2), tag) {
            (Some(&g), Some(tag)) => Reply::Again(cx.heap().app(g, &[v, tag])),
            _ => Reply::Done(v),
        })
    }
}

/// One running `reap`: its patterns (none: every tag) and what was sown to it, `(tag, value)` in order.
struct Frame {
    pats: Option<Vec<Raw>>,
    items: Vec<(Option<Raw>, Raw)>,
}

/// The stack of running `reap`s, shared by `sow` and `reap`.
type Harvest = Arc<Mutex<Vec<Frame>>>;

/// `sow(e)`, `sow(e, tag)`, `sow(e, [tags])`.
struct Sow(Harvest);

impl Native for Sow {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (e, tags): (T<'h>, Vec<Option<T<'h>>>) = match args {
            [e] => (*e, vec![None]),
            [e, tag] => match cx.as_list(*tag) {
                Some(ts) => (*e, ts.iter().map(|&t| Some(t)).collect()),
                None => (*e, vec![Some(*tag)]),
            },
            _ => return Ok(Reply::Decline),
        };
        cx.effect()?;
        let depth = self.0.lock().len();
        for tag in tags {
            // the innermost reap whose pattern matches the tag takes the value
            for i in (0..depth).rev() {
                let pats: Option<Vec<T<'h>>> = self.0.lock()[i].pats.as_ref().map(|ps| ps.iter().map(|p| p.get(h)).collect());
                let takes = match (&pats, tag) {
                    (None, _) => true,
                    (Some(ps), None) => ps.iter().any(|&p| is_any(h, p)),
                    (Some(ps), Some(tag)) => {
                        let mut any = false;
                        for &p in ps {
                            if is_any(h, p) || tag_matches(cx, p, tag)? {
                                any = true;
                                break;
                            }
                        }
                        any
                    }
                };
                if takes {
                    self.0.lock()[i].items.push((tag.map(Raw::new), Raw::new(e)));
                    break;
                }
            }
        }
        Ok(Reply::Done(e))
    }
}

/// `reap(e)`, `reap(e, p)`, `reap(e, [p, ...])`, `reap(e, p, f)`.
struct Reap(Harvest);

impl Native for Reap {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if args.is_empty() || args.len() > 3 {
            return Ok(Reply::Decline);
        }
        let h = cx.heap();
        cx.effect()?;
        // the patterns: none (every tag), one, or a list of them (results grouped per pattern)
        let (pats, listed): (Option<Vec<T<'h>>>, bool) = match args.get(1) {
            None => (None, false),
            Some(&p) => match cx.as_list(p) {
                Some(ps) => (Some(ps.to_vec()), true),
                None => (Some(vec![p]), false),
            },
        };
        let depth = {
            let mut st = self.0.lock();
            st.push(Frame { pats: pats.as_ref().map(|ps| ps.iter().map(|&p| Raw::new(p)).collect()), items: vec![] });
            st.len()
        };
        let r = cx.eval(args[0]);
        let frame = {
            let mut st = self.0.lock();
            let f = st.pop();
            st.truncate(depth - 1);
            f
        };
        let value = r?;
        // invariant: the frame pushed above is on top when the body returns (inner reaps pop their own)
        let frame = frame.expect("the reap's frame");
        let items: Vec<(Option<T<'h>>, T<'h>)> = frame.items.iter().map(|(t, v)| (t.map(|t| t.get(h)), v.get(h))).collect();
        let f = args.get(2).copied();
        let group = |cx: &mut NativeCx<'_, '_, 'h>, p: Option<T<'h>>| -> Res<T<'h>> {
            // distinct tags in order of first use, each with its values
            let mut tags: Vec<Option<T<'h>>> = vec![];
            let mut vals: Vec<Vec<T<'h>>> = vec![];
            for &(tag, v) in &items {
                let ok = match (p, tag) {
                    (None, _) => true,
                    (Some(p), None) => is_any(h, p),
                    (Some(p), Some(tag)) => is_any(h, p) || tag_matches(cx, p, tag)?,
                };
                if !ok {
                    continue;
                }
                match tags.iter().position(|&t| t == tag) {
                    Some(i) => vals[i].push(v),
                    None => {
                        tags.push(tag);
                        vals.push(vec![v]);
                    }
                }
            }
            let mut out = Vec::with_capacity(tags.len());
            for (tag, vs) in tags.into_iter().zip(vals) {
                let l = cx.list(&vs);
                out.push(match f {
                    Some(f) => {
                        let tag = tag.unwrap_or_else(|| cx.null());
                        let call = h.app(f, &[tag, l]);
                        cx.eval(call)?
                    }
                    None => l,
                });
            }
            Ok(cx.list(&out))
        };
        let groups = match &pats {
            None => group(cx, None)?,
            Some(ps) if listed => {
                let mut gs = Vec::with_capacity(ps.len());
                for &p in ps {
                    gs.push(group(cx, Some(p))?);
                }
                cx.list(&gs)
            }
            Some(ps) => group(cx, Some(ps[0]))?,
        };
        Ok(Reply::Done(cx.list(&[value, groups])))
    }
}

/// `repeated(p, ...)` and `key_value_pattern(...)`: patterns (C9B.4, C9B.11), terms that only matching
/// reads; the native declines, so the call stays as written.
struct Repeated;

impl Native for Repeated {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        Ok(Reply::Decline)
    }
}

/// Register `throw`, `catch`, `sow`, `reap` and the pattern head `repeated`.
pub(crate) fn register(k: &mut Kernel, h: &Heap) {
    let harvest: Harvest = Arc::new(Mutex::new(vec![]));
    let a = |hold| Attributes { hold, ..Attributes::default() };
    // catch receives the thrown failure of its body (held), so it handles failures
    k.register(h.sym("throw"), a(Hold::None), Box::new(Throw));
    k.register(h.sym("catch"), Attributes { handles_failures: true, ..a(Hold::First) }, Box::new(Catch));
    k.register(h.sym("sow"), Attributes { effect: true, ..a(Hold::None) }, Box::new(Sow(harvest.clone())));
    k.register(h.sym("reap"), Attributes { effect: true, ..a(Hold::First) }, Box::new(Reap(harvest)));
    k.register(h.sym("repeated"), a(Hold::None), Box::new(Repeated));
    k.register(h.sym("key_value_pattern"), a(Hold::None), Box::new(Repeated));
}
