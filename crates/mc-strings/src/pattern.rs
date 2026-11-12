//! String patterns as terms (round 29): `string_expression(p1, p2, ...)` concatenates its parts (literal strings
//! join into one string; any other part, such as a pattern, keeps the expression symbolic), and `longest(p)`,
//! `shortest(p)` mark a pattern as matching as much or as little as possible. The searching functions of this
//! crate take literal search strings only (decision S.3), so the markers stay inert here; they are the names a
//! pattern is built from.
use crate::ctx::Lib;
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// `string_expression(parts...)`: nested expressions flattened, neighbouring literal strings joined; a single
/// string when every part is literal.
pub(crate) fn string_expression<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let head = match h.view(t) {
        View::App { head, .. } => head,
        _ => return Ok(Reply::Decline),
    };
    let mut parts: Vec<T<'h>> = vec![];
    let mut stack: Vec<T<'h>> = args.iter().rev().copied().collect();
    let mut changed = false;
    let mut text = String::new();
    let mut open = false;
    while let Some(x) = stack.pop() {
        match h.view(x) {
            View::Str(s) => {
                if open {
                    changed = true;
                }
                text.push_str(s);
                open = true;
                continue;
            }
            View::App { head: hd, args: inner } if hd == head => {
                changed = true;
                stack.extend(inner.iter().rev().copied());
                continue;
            }
            _ => {}
        }
        if open {
            parts.push(h.string(&text));
            text.clear();
            open = false;
        }
        parts.push(x);
    }
    if open {
        parts.push(h.string(&text));
    }
    let _ = lib;
    if parts.len() == 1 && matches!(h.view(parts[0]), View::Str(_)) {
        return Ok(Reply::Done(parts[0]));
    }
    if parts.is_empty() {
        return Ok(Reply::Done(h.string("")));
    }
    Ok(if changed { Reply::Done(h.app(head, &parts)) } else { Reply::Decline })
}

/// `longest(p)`, `shortest(p)`: inert markers.
pub(crate) fn marker<'h>(_lib: &Lib, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
    Ok(Reply::Decline)
}
