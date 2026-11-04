//! `to_expression(s)`: a string read as MathCAS source.
//!
//! The text is read and lowered by `mc-syntax` exactly as a statement typed at the top level, then handed back
//! to the evaluator, which evaluates it as the value of the call (decision S.4): `to_expression("1 + 2")` is 3,
//! `to_expression("x^2")` is `x^2`. Several statements become one `block`, so the value is the last one's and
//! `let` names stay local; no statements give `null`. Names are **not** resolved through modules and imports: the
//! reader of a file or session (`mc-load`) does that at read time, and a string read at run time has no such
//! context, so a bare name is the bare symbol and `M.f` the qualified one. A syntax error gives
//! `failure(syntax, "line:col: message")` for the first error.
use crate::ctx::{charge, take, Lib};
use mc_core::{Store, T};
use mc_eval::{NativeCx, Reply, Res};

/// `to_expression(s)`.
pub(crate) fn to_expression<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[s] = args else { return Ok(Reply::Decline) };
    let src = take!(lib.string(h, t, s, 1));
    // reading is linear in the text (a few times a copy)
    charge(cx, src.len().saturating_mul(4))?;
    match mc_syntax::read(h, src) {
        Ok(terms) => Ok(match terms.as_slice() {
            [] => Reply::Done(cx.null()),
            [one] => Reply::Again(*one),
            _ => Reply::Again(h.call(h.sym("block"), &terms)),
        }),
        Err(diags) => {
            let msg = diags.first().map_or_else(|| "unreadable text".to_string(), |d| d.render(src));
            Ok(Reply::Done(lib.fail(h, h.sym("syntax"), &[h.string(&msg)])))
        }
    }
}
