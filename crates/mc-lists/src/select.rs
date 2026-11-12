//! Selection: `select` (by a predicate), `cases` and `count` (by a pattern).
use crate::ctx::{apply, group_cost, take, Lib};
use crate::matching::Matcher;
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// `select(xs, f)`: the elements `x` for which `f(x)` is `true`, in order, keeping the head of `xs`. The calls are
/// evaluated as a group (like `map`); a value other than `true` drops the element, a failure is the result
/// (decision I.11).
pub(crate) fn select<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, f, most) = match *args {
        [xs, f] => (xs, f, usize::MAX),
        // `select(xs, f, n)`: the first n (decision io-core.6)
        [xs, f, n] => match h.view(n) {
            View::Int(n @ 0..) => (xs, f, n as usize),
            _ => return Ok(Reply::Decline),
        },
        _ => return Ok(Reply::Decline),
    };
    // `select(xs, f -> "Index")`, `-> "Element"`, `-> "BitVectorMask"`, `-> ["Element", "Index"]` (a final rule may arrive
    // as options): what to give back for the elements that pass
    let f = match h.view(f) {
        View::App { head, args: &[r] } if head == h.sym_term(h.sym("options")) => r,
        _ => f,
    };
    let (f, prop) = match h.view(f) {
        View::App { head, args: &[p, q] } if head == h.sym_term(lib.s.rule) => (p, Some(q)),
        _ => (f, None),
    };
    let (head, xs) = take!(lib.operand(h, t, xs, 1));
    // an association's entries are tested by their values (decision p6core.22)
    let rule = h.sym_term(lib.s.rule);
    let by_value = head == h.sym_term(h.sym("assoc"))
        && xs.iter().all(|&x| matches!(h.view(x), View::App { head: r, args: [_, _] } if r == rule));
    let tested: Vec<T<'h>> = if by_value {
        xs.iter()
            .map(|&x| match h.view(x) {
                View::App { args: [_, v], .. } => *v,
                _ => x,
            })
            .collect()
    } else {
        xs.to_vec()
    };
    let tested = &tested;
    let cost = group_cost(cx, f);
    let tests = cx.group(xs.len(), cost, &|cx: &mut NativeCx<'_, '_, 'h>, i| {
        cx.tick(1)?;
        apply(cx, f, &[tested[i]])
    })?;
    let mut out = Vec::with_capacity(xs.len());
    let mut at = Vec::with_capacity(xs.len());
    let mut mask = Vec::with_capacity(xs.len());
    for (i, (&x, &v)) in xs.iter().zip(&tests).enumerate() {
        let pass = out.len() < most && cx.is_true(v);
        if pass {
            out.push(x);
            at.push(h.int(i as i64 + 1));
        } else if out.len() < most && lib.is_failure(h, v) {
            return Ok(Reply::Done(v));
        }
        mask.push(cx.bool(pass));
    }
    let Some(prop) = prop else { return Ok(lib.rebuilt(h, head, &out)) };
    let shown = |cx: &NativeCx<'_, '_, 'h>, q: T<'h>| -> Option<Reply<'h>> {
        match h.view(q) {
            View::Str("Element") => Some(lib.rebuilt(h, head, &out)),
            View::Str("Index") => Some(Reply::Done(cx.list(&at))),
            View::Str("BitVectorMask") => Some(Reply::Done(cx.list(&mask))),
            _ => None,
        }
    };
    let value = |r: Reply<'h>| match r {
        Reply::Done(v) | Reply::Again(v) => v,
        Reply::Decline => unreachable!("shown never declines"),
    };
    match cx.as_list(prop) {
        Some(qs) => {
            let mut rules = Vec::with_capacity(qs.len());
            for &q in qs {
                let Some(r) = shown(cx, q) else { return Ok(Reply::Decline) };
                rules.push(h.call(lib.s.rule, &[q, value(r)]));
            }
            Ok(Reply::Done(h.call(lib.s.assoc, &rules)))
        }
        None => Ok(shown(cx, prop).unwrap_or(Reply::Decline)),
    }
}

/// The pattern of `cases` and `count`, and the right side of a rule `p -> r` or `p :> r`.
fn pattern<'l, 'h>(
    lib: &'l Lib,
    cx: &NativeCx<'_, '_, 'h>,
    t: T<'h>,
    p: T<'h>,
    rules: bool,
) -> Result<(Matcher<'l, 'h>, Option<T<'h>>), Reply<'h>> {
    let h = cx.heap();
    let (p, rhs) = match h.view(p) {
        View::App { head, args: &[l, r] }
            if rules && (head == h.sym_term(lib.s.rule) || head == h.sym_term(lib.s.rule_delayed)) =>
        {
            (l, Some(r))
        }
        _ => (p, None),
    };
    match Matcher::new(lib, h, p) {
        Some(m) => Ok((m, rhs)),
        None => Err(Reply::Done(lib.bad_arg(h, t, 2))),
    }
}

/// `cases(xs, p)`: the elements of `xs` that match the pattern `p`, as a list; `cases(xs, p -> r)` (or `p :> r`):
/// for each match, `r` with the pattern's variables replaced, evaluated.
pub(crate) fn cases<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    // `cases(x, p, levels)`, `cases(x, p, levels, n)`: the parts at the levels, the first n (decision io-core.11)
    let (args, heads) = crate::ctx::option(h, args, 2, "heads");
    // with `heads: true` the default level is 1: the head and the elements
    let with_level;
    let args = match (args, heads) {
        (&[x, p], Some(v)) if v == cx.bool(true) => {
            with_level = [x, p, h.int(1)];
            &with_level[..]
        }
        _ => args,
    };
    if let [x, p, spec, ref more @ ..] = *args {
        let Some(levels) = crate::levels::level_spec(h, spec) else { return Ok(Reply::Decline) };
        // heads: true: the parts at the levels with the heads, as `level` lists them (like `count`)
        if heads.is_some_and(|v| v == cx.bool(true)) {
            let most = match more {
                [] => usize::MAX,
                [n] => match h.view(*n) {
                    View::Int(n @ 0..) => n as usize,
                    View::Sym(s) if h.sym_name(s) == "inf" => usize::MAX,
                    // up_to(k): at most k, which the count is already
                    _ => match crate::ctx::up_to_arg(h, *n).map(|k| h.view(k)) {
                        Some(View::Int(k @ 0..)) => k as usize,
                        _ => return Ok(Reply::Decline),
                    },
                },
                _ => return Ok(Reply::Decline),
            };
            let opts = h.call(h.sym("options"), &[h.call(h.sym("rule"), &[h.sym_term(h.sym("heads")), cx.bool(true)])]);
            let parts = cx.eval(h.call(h.sym("level"), &[x, spec, opts]))?;
            let Some(parts) = cx.as_list(parts) else { return Ok(Reply::Decline) };
            let (mut m, rhs) = take!(pattern(lib, cx, t, p, true));
            let mut out = vec![];
            for &u in parts {
                if out.len() < most && m.matches(cx, u)? {
                    out.push(match rhs {
                        None => u,
                        Some(r) => {
                            let r = m.substitute(cx, r)?;
                            cx.eval(r)?
                        }
                    });
                }
            }
            return Ok(Reply::Done(cx.list(&out)));
        }
        let most = match more {
            [] => usize::MAX,
            [n] => match h.view(*n) {
                View::Int(n @ 0..) => n as usize,
                View::Sym(s) if h.sym_name(s) == "inf" => usize::MAX,
                // up_to(k): at most k, which the count is already
                _ => match crate::ctx::up_to_arg(h, *n).map(|k| h.view(k)) {
                    Some(View::Int(k @ 0..)) => k as usize,
                    _ => return Ok(Reply::Decline),
                },
            },
            _ => return Ok(Reply::Decline),
        };
        return Ok(match cases_at(lib, cx, t, x, p, levels, most)? {
            Ok(out) => Reply::Done(cx.list(&out)),
            Err(r) => r,
        });
    }
    let [xs, p] = *args else {
        return Ok(Reply::Decline);
    };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let (mut m, rhs) = take!(pattern(lib, cx, t, p, true));
    let mut out = vec![];
    for &x in xs {
        if m.matches(cx, x)? {
            out.push(match rhs {
                None => x,
                Some(r) => {
                    let r = m.substitute(cx, r)?;
                    cx.eval(r)?
                }
            });
        }
    }
    Ok(Reply::Done(cx.list(&out)))
}

/// The parts of `x` at `levels` (inner parts first) that match the pattern `p` (or, for a rule `p -> r`, the
/// values of `r`), at most `most` of them.
pub(crate) fn cases_at<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    x: T<'h>,
    p: T<'h>,
    levels: crate::levels::Levels,
    most: usize,
) -> Res<Result<Vec<T<'h>>, Reply<'h>>> {
    let (mut m, rhs) = match pattern(lib, cx, t, p, true) {
        Ok(v) => v,
        Err(r) => return Ok(Err(r)),
    };
    let mut out = vec![];
    let mut visit = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, _: &[i64]| -> Res<Option<T<'h>>> {
        if out.len() < most && m.matches(cx, u)? {
            out.push(match rhs {
                None => u,
                Some(r) => {
                    let r = m.substitute(cx, r)?;
                    cx.eval(r)?
                }
            });
        }
        Ok(None)
    };
    Ok(match crate::levels::walk(cx, x, levels, &mut visit)? {
        Some(_) => Ok(out),
        None => Err(Reply::Decline),
    })
}

/// `count(xs, p)`: how many elements of `xs` match the pattern `p`.
pub(crate) fn count<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, heads) = crate::ctx::option(h, args, 2, "heads");
    if let [x, p, spec] = *args {
        let Some(levels) = crate::levels::level_spec(h, spec) else { return Ok(Reply::Decline) };
        // heads: true: the parts at the levels with the heads, as `level` lists them
        if heads.is_some_and(|v| v == cx.bool(true)) {
            let opts = h.call(h.sym("options"), &[h.call(h.sym("rule"), &[h.sym_term(h.sym("heads")), cx.bool(true)])]);
            let parts = cx.eval(h.call(h.sym("level"), &[x, spec, opts]))?;
            let Some(parts) = cx.as_list(parts) else { return Ok(Reply::Decline) };
            let (mut m, _) = take!(pattern(lib, cx, t, p, false));
            let mut n = 0i64;
            for &u in parts {
                if m.matches(cx, u)? {
                    n += 1;
                }
            }
            return Ok(Reply::Done(h.int(n)));
        }
        let (mut m, _) = take!(pattern(lib, cx, t, p, false));
        let mut n = 0i64;
        let mut visit = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, _: &[i64]| -> Res<Option<T<'h>>> {
            if m.matches(cx, u)? {
                n += 1;
            }
            Ok(None)
        };
        return Ok(match crate::levels::walk(cx, x, levels, &mut visit)? {
            Some(_) => Reply::Done(h.int(n)),
            None => Reply::Decline,
        });
    }
    let [xs, p] = *args else {
        return Ok(Reply::Decline);
    };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let (mut m, _) = take!(pattern(lib, cx, t, p, false));
    let mut n = 0i64;
    for &x in xs {
        if m.matches(cx, x)? {
            n += 1;
        }
    }
    Ok(Reply::Done(h.int(n)))
}

/// `drop_failures(xs)`: the elements of `xs` that neither are nor contain a failure (a table row
/// `[x, failure(...)]` is dropped whole). Each element is searched iteratively; shared subterms once.
pub(crate) fn drop_failures<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let [xs] = *args else { return Ok(Reply::Decline) };
    let xs = take!(lib.list_operand(h, t, xs, 1));
    let failure = h.sym_term(lib.s.failure);
    let mut kept = Vec::with_capacity(xs.len());
    for &x in xs {
        cx.tick(1)?;
        let mut stack = vec![x];
        let mut seen = std::collections::HashSet::new();
        let mut bad = false;
        while let Some(u) = stack.pop() {
            if !seen.insert(u) {
                continue;
            }
            if let View::App { head, args } = h.view(u) {
                if head == failure {
                    bad = true;
                    break;
                }
                // packed arrays hold only numbers
                if h.packed(u).is_none() {
                    stack.extend_from_slice(args);
                }
            }
        }
        if !bad {
            kept.push(x);
        }
    }
    Ok(Reply::Done(h.app(h.sym_term(lib.s.list), &kept)))
}
