//! Symbols by name and evaluation with a final step: `symbol`, `symbol_name`, `with_cleanup`.
//!
//! `symbol("x")` is the symbol the reader makes of the name `x` (so `symbol("x") === x`); a string that is not one
//! name (a keyword, a number, an expression) is `failure(argument, symbol, 1)`. `symbol_name(x)` is the name of a
//! symbol without its module path.
//!
//! `with_cleanup(body, cleanup)` evaluates `body`, then `cleanup`, and gives the value of `body`; `with_cleanup(init,
//! body, cleanup)` evaluates `init` first. Failures are values (semantics 10), so the cleanup also runs when the body
//! gives a failure or throws (`catch(with_cleanup(throw(x), c))` runs `c` and gives `x`); the failure is the result.
//! An interrupt or an exhausted limit stops the whole evaluation, the cleanup with it.
use crate::ctx::Lib;
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// `symbol(name)`
pub(crate) fn symbol<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[s] = args else { return Ok(Reply::Decline) };
    let name = match h.view(s) {
        View::Str(name) => name.to_string(),
        View::Sym(_) | View::App { .. } => return Ok(Reply::Decline),
        _ => return Ok(Reply::Done(lib.bad_arg(h, t, 1))),
    };
    // the reader decides what a name is: the text must read as that one symbol
    match mc_syntax::read(h, &name) {
        Ok(terms) => match terms.as_slice() {
            [one] if matches!(h.view(*one), View::Sym(x) if h.sym_name(x) == name.as_str()) => Ok(Reply::Done(*one)),
            _ => Ok(Reply::Done(lib.bad_arg(h, t, 1))),
        },
        Err(_) => Ok(Reply::Done(lib.bad_arg(h, t, 1))),
    }
}

/// `symbol_name(x)`
pub(crate) fn symbol_name<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[x] = args else { return Ok(Reply::Decline) };
    match h.view(x) {
        View::Sym(s) => {
            let full = h.sym_name(s);
            let short = full.rsplit('.').next().unwrap_or(full.as_str());
            Ok(Reply::Done(h.string(short)))
        }
        View::App { .. } => Ok(Reply::Decline),
        _ => Ok(Reply::Done(lib.bad_arg(h, t, 1))),
    }
}

/// `with_cleanup(body, cleanup)`, `with_cleanup(init, body, cleanup)` (all arguments held).
pub(crate) fn with_cleanup<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let (init, body, cleanup) = match *args {
        [b, c] => (None, b, c),
        [i, b, c] => (Some(i), b, c),
        _ => return Ok(Reply::Decline),
    };
    if let Some(i) = init {
        cx.eval(i)?;
    }
    let v = cx.eval(body)?;
    cx.eval(cleanup)?;
    Ok(Reply::Done(v))
}
