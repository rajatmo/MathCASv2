//! One equation in one unknown x that holds x only in exponentials `e^(n x)` (n a nonzero integer), with conditions
//! that bound the real part of x to a strip `a <= re(x) <= b` (strict or not): the substitution y = e^x maps the
//! strip onto the annulus `e^a <= |y| <= e^b` and the equation onto one in y without branch points (every
//! `e^(n x)` becomes `y^n`). The roots y in the annulus (a bounded region, solved by the isolation of all roots
//! there) give the families `x = log(y) + 2 pi i c(1)`: exp is onto the punctured plane with period 2 pi i, and
//! re(log(y) + 2 pi i k) = log|y|, so every member lies in the strip exactly when y lies in the annulus.
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// Does x occur in `t` only inside `e^(n x)` with a nonzero integer n?
fn only_exponentials<'h>(h: &'h impl Store, t: T<'h>, x: T<'h>) -> bool {
    if t == x {
        return false;
    }
    let View::App { head, args } = h.view(t) else { return true };
    if let (View::Sym(s), &[b, w]) = (h.view(head), args) {
        if h.sym_name(s) == names::POW && matches!(h.view(b), View::Sym(e) if h.sym_name(e) == "e") {
            if w == x {
                return true;
            }
            if let View::App { head: mh, args: &[n, y] } = h.view(w) {
                if y == x && matches!(h.view(mh), View::Sym(m) if h.sym_name(m) == names::MUL) && matches!(h.view(n), View::Int(k) if k != 0) {
                    return true;
                }
            }
        }
    }
    args.iter().all(|&a| only_exponentials(h, a, x)) && only_exponentials(h, head, x)
}

/// `cond` as a bound on `re(x)` mapped to the same bound on `abs(y)`: `l op re(x)` becomes `e^l op abs(y)`, with
/// the operator kept (strict stays strict). `None` for a condition of another form.
fn annulus_bound<'h>(cx: &mut NativeCx<'_, '_, 'h>, cond: T<'h>, rex: T<'h>, absy: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let View::App { head, args: &[l, r] } = h.view(cond) else { return Ok(None) };
    let View::Sym(s) = h.view(head) else { return Ok(None) };
    if !matches!(h.sym_name(s).as_str(), "less" | "less_equal" | "greater" | "greater_equal") {
        return Ok(None);
    }
    let real_number = |cx: &mut NativeCx<'_, '_, 'h>, v: T<'h>| -> Res<bool> {
        let n = cx.eval(h.call(h.sym("n"), &[v]))?;
        Ok(matches!(h.view(n), View::Real(f) if f.is_finite()) || matches!(h.view(n), View::Int(_)))
    };
    let e = h.sym_term(h.sym("e"));
    let mapped = if l == rex && real_number(cx, r)? {
        h.app(head, &[absy, h.call(h.sym(names::POW), &[e, r])])
    } else if r == rex && real_number(cx, l)? {
        h.app(head, &[h.call(h.sym(names::POW), &[e, l]), absy])
    } else {
        return Ok(None);
    };
    Ok(Some(cx.eval(mapped)?))
}

/// The solutions of `f == 0` (f the difference of the sides) under `conds` that bound re(x) on both sides, as
/// rules `[[x -> conditional_expression(log(y_j) + 2 pi i c(1), element(c(1), integers))], ...]`; `None` when the
/// form does not apply or the roots in the annulus are not found.
pub(crate) fn solve<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>, conds: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    if !only_exponentials(h, f, x) {
        return Ok(None);
    }
    let rex = h.call(h.sym("re"), &[x]);
    let y = h.sym_term(h.sym("solve`strip"));
    let absy = h.call(h.sym("abs"), &[y]);
    let mut bounds = vec![];
    let (mut lower, mut upper) = (false, false);
    for &c in conds {
        let Some(b) = annulus_bound(cx, c, rex, absy)? else { return Ok(None) };
        let View::App { head, args: &[l, _] } = h.view(b) else { return Ok(None) };
        let less = matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "less" | "less_equal"));
        // abs(y) on the left of `<` is an upper bound
        if (l == absy) == less {
            upper = true;
        } else {
            lower = true;
        }
        bounds.push(b);
    }
    if !(lower && upper) {
        return Ok(None);
    }
    // x = log(y): every e^(n x) becomes y^n
    let logy = h.call(h.sym("log"), &[y]);
    let g = cx.eval(h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[x, logy])]))?;
    let g = cx.eval(h.call(h.sym("together"), &[g]))?;
    if contains(h, g, x) || contains(h, g, logy) {
        return Ok(None);
    }
    // the roots of the numerator in the disk of the upper bounds (an entire function there when g has poles only
    // at 0), then those meeting the lower bounds and where the denominator does not vanish
    let num = cx.eval(h.call(h.sym("numerator"), &[g]))?;
    let den = cx.eval(h.call(h.sym("denominator"), &[g]))?;
    let (outer, inner): (Vec<T<'h>>, Vec<T<'h>>) = bounds.into_iter().partition(|&b| {
        let View::App { head, args: &[l, _] } = h.view(b) else { return false };
        let less = matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "less" | "less_equal"));
        (l == absy) == less
    });
    let mut parts = vec![h.call(h.sym("equal"), &[num, h.int(0)])];
    parts.extend(outer);
    let r = cx.eval(h.call(h.sym("solve"), &[h.call(h.sym("and"), &parts), y]))?;
    let Some(all) = cx.as_list(r) else { return Ok(None) };
    let all = all.to_vec();
    let mut items = vec![];
    for s in all {
        let mut tests = inner.clone();
        // (as a modulus, which comparisons decide through the root's enclosure)
        tests.push(h.call(h.sym("greater"), &[h.call(h.sym("abs"), &[den]), h.int(0)]));
        let mut keep = true;
        for t in tests {
            let v = cx.eval(h.call(h.sym("rewrite"), &[t, s]))?;
            if cx.is_false(v) {
                keep = false;
                break;
            }
            // a bound the enclosures do not decide (a root on the circle): no answer
            if !cx.is_true(v) {
                return Ok(None);
            }
        }
        if keep {
            items.push(s);
        }
    }
    let gen = h.sym_term(h.sym(crate::front::current().gen.as_deref().unwrap_or("c")));
    let c1 = h.app(gen, &[h.int(1)]);
    let two_pi_i = h.call(h.sym(names::MUL), &[h.int(2), h.sym_term(h.sym("pi")), h.sym_term(h.sym("im")), c1]);
    let cond = h.call(h.sym("element"), &[c1, h.sym_term(h.sym("integers"))]);
    let mut out = vec![];
    for s in items {
        let Some(&[rule]) = cx.as_list(s) else { return Ok(None) };
        let View::App { args: &[_, v], .. } = h.view(rule) else { return Ok(None) };
        // a family or a condition in y is not one root
        if contains(h, v, gen) || head_is(h, v, "conditional_expression") {
            return Ok(None);
        }
        let value = h.call(h.sym(names::ADD), &[h.call(h.sym("log"), &[v]), two_pi_i]);
        let value = h.call(h.sym("conditional_expression"), &[value, cond]);
        out.push(cx.list(&[h.call(h.sym(names::RULE), &[x, value])]));
    }
    Ok(Some(cx.eval(cx.list(&out))?))
}

fn head_is<'h>(h: &'h impl Store, t: T<'h>, name: &str) -> bool {
    matches!(h.view(t), View::App { head, .. } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == name))
}

fn contains<'h>(h: &'h impl Store, t: T<'h>, x: T<'h>) -> bool {
    if t == x {
        return true;
    }
    match h.view(t) {
        View::App { head, args } => contains(h, head, x) || args.iter().any(|&a| contains(h, a, x)),
        _ => false,
    }
}
