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
    let [xs, f] = *args else {
        return Ok(Reply::Decline);
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
    for (&x, &v) in xs.iter().zip(&tests) {
        if cx.is_true(v) {
            out.push(x);
        } else if lib.is_failure(h, v) {
            return Ok(Reply::Done(v));
        }
    }
    Ok(lib.rebuilt(h, head, &out))
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

/// `count(xs, p)`: how many elements of `xs` match the pattern `p`.
pub(crate) fn count<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    if let [x, p, spec] = *args {
        let Some(levels) = crate::levels::level_spec(h, spec) else { return Ok(Reply::Decline) };
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
