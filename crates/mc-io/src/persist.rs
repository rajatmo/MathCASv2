//! `put(value, file)` and `get(file)`: a value saved to a text file and read back in a later session (decision
//! io-num.14, gap G75).
//!
//! The file is the value in the language's own input form after a first line `#! mathcas 2 value`; `get` reads it
//! back with `to_expression` (so the value is evaluated again, in the session that reads it). Machine reals print
//! with the shortest digits that read back to the same number, so tables and interpolations come back bit for bit.
//!
//! A solution of `n_dsolve` names data its session keeps (`interpolating_function(name, domain, steps, key)`), which
//! another session does not have: `put` writes each such function as the `interpolation` through its values at 16
//! points per step of the solution (at least 2049 points, uniformly spaced), so the restored function is a
//! not-a-knot cubic spline through the solution, accurate to about 1e-10 relative for smooth solutions, that can be
//! called, differentiated and plotted.
use crate::failure;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};

/// The first line of a value file.
pub(crate) const HEADER: &str = "#! mathcas 2 value";

/// `put(value, file)`.
pub(crate) struct Put;

/// `get(file)`.
pub(crate) struct Get;

fn path_of<'h>(h: &'h Heap, f: &str, file: T<'h>) -> Result<String, T<'h>> {
    match h.view(file) {
        View::Str(p) => Ok(p.to_string()),
        _ => Err(crate::bad_argument(h, f, 2)),
    }
}

impl Native for Put {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[value, file] = args else { return Ok(Reply::Decline) };
        let path = match path_of(h, "put", file) {
            Ok(p) => p,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let value = self_contained(cx, value)?;
        let text = cx.eval(h.call(h.sym("to_string"), &[value]))?;
        let View::Str(text) = h.view(text) else {
            return Ok(Reply::Done(failure(h, "io", &[file, h.string("the value has no printed form")])));
        };
        let body = format!("{HEADER}\n{text}\n");
        Ok(Reply::Done(match std::fs::write(&path, body) {
            Ok(()) => file,
            Err(e) => failure(h, "io", &[file, h.string(&e.to_string())]),
        }))
    }
}

impl Native for Get {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let &[file] = args else { return Ok(Reply::Decline) };
        let path = match h.view(file) {
            View::Str(p) => p.to_string(),
            _ => return Ok(Reply::Done(crate::bad_argument(h, "get", 1))),
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return Ok(Reply::Done(failure(h, "io", &[file, h.string(&e.to_string())]))),
        };
        let body: String = text.lines().filter(|l| !l.starts_with("#!")).collect::<Vec<_>>().join("\n");
        if body.trim().is_empty() {
            return Ok(Reply::Done(cx.null()));
        }
        Ok(Reply::Again(h.call(h.sym("to_expression"), &[h.string(&body)])))
    }
}

/// The value with every `interpolating_function(...)` (data kept by the session) replaced by `interpolation` data.
fn self_contained<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    if h.packed(t).is_some() {
        return Ok(t);
    }
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    if head == h.sym_term(h.sym("interpolating_function")) {
        return Ok(sampled(cx, t)?.unwrap_or(t));
    }
    let mut out = Vec::with_capacity(args.len());
    let mut changed = false;
    for &a in args {
        let b = self_contained(cx, a)?;
        changed |= b != a;
        out.push(b);
    }
    let head2 = self_contained(cx, head)?;
    Ok(if changed || head2 != head { h.app(head2, &out) } else { t })
}

/// `interpolation` through an interpolating function sampled on its domain (`None` when it does not evaluate).
fn sampled<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let View::App { args, .. } = h.view(f) else { return Ok(None) };
    let (Some(&dom), steps) = (args.get(1), args.get(2).and_then(|&s| match h.view(s) {
        View::Int(v) => Some(v.max(0) as usize),
        _ => None,
    })) else {
        return Ok(None);
    };
    let Some(&[lo, hi]) = cx.as_list(dom) else { return Ok(None) };
    let (View::Real(lo), View::Real(hi)) = (h.view(lo), h.view(hi)) else { return Ok(None) };
    if !(lo.is_finite() && hi.is_finite() && hi > lo) {
        return Ok(None);
    }
    let n = (16 * steps.unwrap_or(0)).clamp(2048, 1 << 18);
    let mut pts = Vec::with_capacity(n + 1);
    for k in 0..=n {
        if k % 1024 == 0 {
            cx.tick(1024)?;
        }
        let x = if k == n { hi } else { lo + (hi - lo) * k as f64 / n as f64 };
        let v = cx.eval(h.app(f, &[h.real(x)]))?;
        let View::Real(y) = h.view(v) else { return Ok(None) };
        pts.push(h.reals(&[x, y]));
    }
    let data = cx.list(&pts);
    let r = cx.eval(h.call(h.sym("interpolation"), &[data]))?;
    Ok(matches!(h.view(r), View::App { head, .. } if head == h.sym_term(h.sym("interpolated"))).then_some(r))
}
