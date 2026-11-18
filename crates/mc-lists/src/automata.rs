//! The evolution of a one-dimensional cellular automaton whose rule is a number (`cellular_automaton(30, ...)`,
//! `[n, k, r]`, `[n, [k, weights], r]`), the helper `nl_ca_evolve` of the library definition of
//! `cellular_automaton`: the same steps as the definition's loop, on machine integers.
//!
//! `nl_ca_evolve(n, k, w, offsets, row, background, periodic, steps)` gives `[rows, backgrounds]`: the rows at steps
//! 0 to `steps` and the background cell at each step. The new cell is the digit of `n` in base `k` at the position
//! `sum w_j c_(i + o_j)` (the neighbourhood cells `c` at the offsets `o_j`), `mod(floor(n/k^d), k)`; a periodic row
//! wraps around, otherwise the row is extended by the background, which evolves by the same rule. The call stays
//! (and the definition's loop runs) unless every number is a machine integer, `n`, the cells and the weights are not
//! negative and `k >= 2`.
use crate::Lib;
use mc_api::{packed::Packed, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The machine integers of a list (packed or not), `None` when it has another element.
fn ints<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<i64>> {
    let h = cx.heap();
    if let Some(Packed::Ints { dims, data }) = h.packed(t) {
        return (dims.len() == 1).then(|| data.to_vec());
    }
    cx.as_list(t)?.iter().map(|&e| match h.view(e) {
        View::Int(v) => Some(v),
        _ => None,
    }).collect()
}

/// The digit of `n` in base `k` at position `d` (`d >= 0`): floor(n/k^d) mod k.
fn digit(n: i64, k: i64, d: i64) -> i64 {
    let mut x = n;
    let mut d = d;
    while d > 0 && x > 0 {
        x /= k;
        d -= 1;
    }
    x % k
}

pub(crate) fn ca_evolve<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[n, k, w, offs, row, bg, periodic, steps] = args else { return Ok(Reply::Decline) };
    let int = |t: T<'h>| match h.view(t) {
        View::Int(v) => Some(v),
        _ => None,
    };
    let (Some(n), Some(k), Some(bg), Some(steps)) = (int(n), int(k), int(bg), int(steps)) else { return Ok(Reply::Decline) };
    let periodic = if cx.is_true(periodic) {
        true
    } else if cx.is_false(periodic) {
        false
    } else {
        return Ok(Reply::Decline);
    };
    let (Some(w), Some(offs), Some(mut state)) = (ints(cx, w), ints(cx, offs), ints(cx, row)) else { return Ok(Reply::Decline) };
    if n < 0 || k < 2 || bg < 0 || !(0..=1_000_000).contains(&steps) || w.len() != offs.len() || offs.is_empty() {
        return Ok(Reply::Decline);
    }
    if w.iter().any(|&x| x < 0) || state.iter().any(|&x| x < 0) || offs.iter().any(|&o| o.unsigned_abs() > 1_000_000) {
        return Ok(Reply::Decline);
    }
    let rr = offs.iter().map(|o| o.unsigned_abs() as usize).max().unwrap_or(0);
    let len = state.len();
    if periodic && len < rr {
        return Ok(Reply::Decline);
    }
    let cells = len.saturating_mul(steps as usize + 1);
    if cells > 50_000_000 {
        return Ok(Reply::Decline);
    }
    // the position of the digit; a sum that leaves i64 has no digit in a machine n (it is 0)
    let position = |nb: &mut dyn Iterator<Item = i64>| -> Option<i64> {
        let mut d = 0i64;
        for (c, &wj) in nb.zip(&w) {
            d = d.checked_add(c.checked_mul(wj)?)?;
        }
        Some(d)
    };
    let rule = |d: Option<i64>| d.map_or(0, |d| digit(n, k, d));
    let mut data = Vec::with_capacity(cells);
    data.extend_from_slice(&state);
    let mut bgs = Vec::with_capacity(steps as usize + 1);
    bgs.push(bg);
    let mut bgv = bg;
    let mut ext = vec![0i64; len + 2 * rr];
    let mut next = vec![0i64; len];
    for _ in 0..steps {
        cx.tick(len as u64 / 8 + 1)?;
        // the row extended by rr cells on each side
        for j in 0..rr {
            ext[j] = if periodic { state[len - rr + j] } else { bgv };
            ext[rr + len + j] = if periodic { state[j] } else { bgv };
        }
        ext[rr..rr + len].copy_from_slice(&state);
        for (i, cell) in next.iter_mut().enumerate() {
            *cell = rule(position(&mut offs.iter().map(|&o| ext[(i + rr).wrapping_add_signed(o as isize)])));
        }
        std::mem::swap(&mut state, &mut next);
        data.extend_from_slice(&state);
        bgv = rule(position(&mut std::iter::repeat_n(bgv, offs.len())));
        bgs.push(bgv);
    }
    let rows = if len == 0 { cx.list(&vec![cx.list(&[]); steps as usize + 1]) } else { h.int_array(&[steps as usize + 1, len], &data) };
    let bgs = h.int_array(&[bgs.len()], &bgs);
    Ok(Reply::Done(cx.list(&[rows, bgs])))
}

/// `nl_ca_window(rows, backgrounds)`: `[lo, hi]`, the first and last column (from 1) where some row differs from
/// its step's background, `[]` when none does; the rows and backgrounds machine integers (else the call stays).
pub(crate) fn ca_window<'h>(_lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[rows, bgs] = args else { return Ok(Reply::Decline) };
    let Some(bgs) = ints(cx, bgs) else { return Ok(Reply::Decline) };
    let rows: Vec<Vec<i64>> = match h.packed(rows) {
        Some(Packed::Ints { dims, data }) if dims.len() == 2 => data.chunks(dims[1] as usize).map(<[i64]>::to_vec).collect(),
        _ => {
            let Some(rs) = cx.as_list(rows) else { return Ok(Reply::Decline) };
            let mut out = Vec::with_capacity(rs.len());
            for &r in rs {
                let Some(v) = ints(cx, r) else { return Ok(Reply::Decline) };
                out.push(v);
            }
            out
        }
    };
    if rows.len() != bgs.len() {
        return Ok(Reply::Decline);
    }
    cx.tick(rows.iter().map(Vec::len).sum::<usize>() as u64 / 16 + 1)?;
    let (mut lo, mut hi) = (usize::MAX, 0usize);
    for (r, &b) in rows.iter().zip(&bgs) {
        if let Some(i) = r.iter().position(|&c| c != b) {
            lo = lo.min(i);
        }
        if let Some(i) = r.iter().rposition(|&c| c != b) {
            hi = hi.max(i);
        }
    }
    Ok(Reply::Done(if lo == usize::MAX {
        cx.list(&[])
    } else {
        cx.list(&[h.int(lo as i64 + 1), h.int(hi as i64 + 1)])
    }))
}

#[cfg(test)]
mod tests {
    use super::digit;

    #[test]
    fn digits_of_a_rule_number() {
        // rule 30 = 00011110 in base 2
        let bits: Vec<i64> = (0..8).map(|d| digit(30, 2, d)).collect();
        assert_eq!(bits, [0, 1, 1, 1, 1, 0, 0, 0]);
        assert_eq!(digit(30, 2, 1000), 0);
        // 2049 in base 3 is 2210220
        let t: Vec<i64> = (0..8).map(|d| digit(2049, 3, d)).collect();
        assert_eq!(t, [0, 2, 2, 0, 1, 2, 2, 0]);
    }
}
