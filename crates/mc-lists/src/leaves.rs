//! `nl_complex_leaves(a)`: a rectangular array of machine numbers with every leaf written as a machine complex
//! number, the value of `complex(re(x), im_part(x))` at each leaf x (a real r becomes r + 0.0 i, a complex number
//! stays): the last step of the library's `fourier`, without a call per entry. The call stays for any other leaf.
use crate::Lib;
use mc_api::{packed::Packed, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The leaves of a rectangular array of machine numbers as (re, im) pairs, with its dimensions.
fn collect<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, depth: usize, dims: &mut Vec<usize>, parts: &mut Vec<f64>) -> Option<()> {
    let h = cx.heap();
    match h.packed(t) {
        Some(Packed::Reals { dims: d, data }) => {
            check(dims, depth, d, parts.is_empty())?;
            for &x in data {
                parts.extend_from_slice(&[x, 0.0]);
            }
            return Some(());
        }
        Some(Packed::Complexes { dims: d, data }) => {
            check(dims, depth, d, parts.is_empty())?;
            parts.extend_from_slice(data);
            return Some(());
        }
        Some(_) => return None,
        None => {}
    }
    if let Some(items) = cx.as_list(t) {
        if dims.len() == depth {
            // a new level after the first leaf: not rectangular
            if !parts.is_empty() {
                return None;
            }
            dims.push(items.len());
        } else if dims.get(depth) != Some(&items.len()) {
            return None;
        }
        if items.is_empty() {
            return None;
        }
        for &it in items {
            collect(cx, it, depth + 1, dims, parts)?;
        }
        return Some(());
    }
    // a leaf: the array is as deep here as at its first leaf
    if dims.len() != depth {
        return None;
    }
    match h.view(t) {
        View::Real(x) => parts.extend_from_slice(&[x, 0.0]),
        View::App { head, args: &[a, b] } if head == h.sym_term(h.sym(mc_api::names::COMPLEX)) => match (h.view(a), h.view(b)) {
            (View::Real(x), View::Real(y)) => parts.extend_from_slice(&[x, y]),
            _ => return None,
        },
        _ => return None,
    }
    Some(())
}

/// The dimensions `d` of a packed block met at `depth`: they continue (or must equal) those seen so far.
fn check(dims: &mut Vec<usize>, depth: usize, d: &[u64], first: bool) -> Option<()> {
    let d: Vec<usize> = d.iter().map(|&x| x as usize).collect();
    if dims.len() == depth {
        if !first {
            return None;
        }
        dims.extend_from_slice(&d);
        Some(())
    } else {
        (dims.len() == depth + d.len() && dims[depth..] == d[..]).then_some(())
    }
}

pub(crate) fn complex_leaves<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let &[a] = args else { return Ok(Reply::Decline) };
    let (mut dims, mut parts) = (vec![], vec![]);
    if collect(cx, a, 0, &mut dims, &mut parts).is_none() || dims.is_empty() || dims.iter().product::<usize>() * 2 != parts.len() {
        return Ok(Reply::Decline);
    }
    cx.tick(parts.len() as u64 / 32 + 1)?;
    Ok(Reply::Done(cx.heap().complex_array(&dims, &parts)))
}
