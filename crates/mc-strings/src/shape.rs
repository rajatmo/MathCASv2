//! Taking strings apart and building them: `string_take`, `string_drop`, `characters`, `from_characters`,
//! `string_reverse`, `to_upper`, `to_lower`, `string_pad`, `string_trim`.
//!
//! Positions count characters (Unicode scalar values), 1-based, negative from the end (decision S.1).
use crate::ctx::{charge, take, too_large, Lib, MAX_BYTES};
use mc_core::{Heap, Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// The byte offset of every character boundary of `s`: `len + 1` offsets for `len` characters.
fn boundaries(s: &str) -> Vec<usize> {
    s.char_indices().map(|(i, _)| i).chain([s.len()]).collect()
}

/// The characters (as a range `lo..hi` of character positions, 0-based) a position spec selects in a string of
/// `n` characters: `k` the first k (the last -k for negative k); `[i, j]` characters i to j inclusive, negative
/// counting from the end, empty when j is the position before i; `[i]` character i. `Err` is the failure for an
/// index outside the string.
fn selected<'h>(lib: &Lib, h: &'h Heap, t: T<'h>, spec: T<'h>, n: usize) -> Result<(usize, usize), Reply<'h>> {
    let ni = n as i64;
    // a 1-based position (negative from the end) as a 0-based one, `None` outside 1..=n
    let pos = |i: i64| -> Option<i64> {
        let p = if i < 0 { ni + 1 + i } else { i };
        (1..=ni).contains(&p).then_some(p - 1)
    };
    if let Some(items) = lib.as_list(h, spec) {
        let idx: Vec<i64> = items.iter().map(|&x| lib.int(h, t, x, 2)).collect::<Result<_, _>>()?;
        return match *idx.as_slice() {
            [i] => match pos(i) {
                Some(p) => Ok((p as usize, p as usize + 1)),
                None => Err(lib.bad_index(h, h.int(i), n)),
            },
            [i, j] => {
                let lo = pos(i).ok_or_else(|| lib.bad_index(h, h.int(i), n))?;
                // the 1-based end, inclusive: at least lo (empty when it is the position before i)
                let end = if j < 0 { ni + 1 + j } else { j };
                if end < lo || end > ni {
                    return Err(lib.bad_index(h, h.int(j), n));
                }
                Ok((lo as usize, end as usize))
            }
            _ => Err(lib.bad_arg(h, t, 2)),
        };
    }
    let k = lib.int(h, t, spec, 2)?;
    let m = usize::try_from(k.unsigned_abs()).ok().filter(|&m| m <= n).ok_or_else(|| lib.bad_index(h, h.int(k), n))?;
    Ok(if k >= 0 { (0, m) } else { (n - m, n) })
}

/// `string_take(s, spec)`: the selected characters.
pub(crate) fn string_take<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[s, spec] = args else { return Ok(Reply::Decline) };
    let s = take!(lib.string(h, t, s, 1));
    charge(cx, s.len())?;
    let b = boundaries(s);
    let (lo, hi) = take!(selected(lib, h, t, spec, b.len() - 1));
    Ok(Reply::Done(h.string(&s[b[lo]..b[hi]])))
}

/// `string_drop(s, spec)`: the string without the selected characters.
pub(crate) fn string_drop<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[s, spec] = args else { return Ok(Reply::Decline) };
    let s = take!(lib.string(h, t, s, 1));
    charge(cx, s.len())?;
    let b = boundaries(s);
    let (lo, hi) = take!(selected(lib, h, t, spec, b.len() - 1));
    let mut out = String::with_capacity(s.len());
    out.push_str(&s[..b[lo]]);
    out.push_str(&s[b[hi]..]);
    Ok(Reply::Done(h.string(&out)))
}

/// `characters(s)`: the list of the characters of s, each a one-character string.
pub(crate) fn characters<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[s] = args else { return Ok(Reply::Decline) };
    let s = take!(lib.string(h, t, s, 1));
    // one term per character: charged per character, not per byte
    cx.tick(s.len() as u64 + 1)?;
    let mut buf = [0u8; 4];
    let items: Vec<T<'h>> = s.chars().map(|c| h.string(c.encode_utf8(&mut buf))).collect();
    Ok(Reply::Done(h.call(lib.s.list, &items)))
}

/// `from_characters([c1, ...])`: the string of the characters, each a one-character string.
pub(crate) fn from_characters<'h>(
    lib: &Lib,
    cx: &mut NativeCx<'_, '_, 'h>,
    t: T<'h>,
    args: &[T<'h>],
) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[xs] = args else { return Ok(Reply::Decline) };
    let Some(items) = lib.as_list(h, xs) else {
        return Ok(if matches!(h.view(xs), View::Sym(_) | View::App { .. }) {
            Reply::Decline
        } else {
            lib.bad_arg(h, t, 1)
        });
    };
    cx.tick(items.len() as u64 + 1)?;
    let mut out = String::with_capacity(items.len());
    for &c in items {
        match h.view(c) {
            View::Str(c) if c.chars().count() == 1 => out.push_str(c),
            _ => return Ok(lib.bad_arg(h, t, 1)),
        }
    }
    Ok(Reply::Done(h.string(&out)))
}

/// `string_reverse(s)`: the characters in reverse order.
pub(crate) fn string_reverse<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[s] = args else { return Ok(Reply::Decline) };
    let s = take!(lib.string(h, t, s, 1));
    charge(cx, s.len())?;
    Ok(Reply::Done(h.string(&s.chars().rev().collect::<String>())))
}

/// `to_upper(s)`: upper case by the Unicode case mapping (one character may become several: ß to SS).
pub(crate) fn to_upper<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[s] = args else { return Ok(Reply::Decline) };
    let s = take!(lib.string(h, t, s, 1));
    charge(cx, s.len())?;
    Ok(Reply::Done(h.string(&s.to_uppercase())))
}

/// `to_lower(s)`: lower case by the Unicode case mapping (a final sigma becomes ς).
pub(crate) fn to_lower<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let &[s] = args else { return Ok(Reply::Decline) };
    let s = take!(lib.string(h, t, s, 1));
    charge(cx, s.len())?;
    Ok(Reply::Done(h.string(&s.to_lowercase())))
}

/// Where `string_pad` adds its padding.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Center,
}

/// `string_pad(s, n)`, `string_pad(s, n, pad)`, with the named option `side: right | left | center`: s padded to n
/// characters with pad (default " ", repeated as needed) after it (right, the default), before it (left) or on
/// both sides (center, the extra character after). A string of n or more characters stays as it is.
pub(crate) fn string_pad<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (args, opts) = lib.options(h, args);
    let mut side = Side::Right;
    for (name, v) in opts {
        if h.sym_name(name) != "side" {
            return Ok(lib.unknown_option(h, name));
        }
        side = match h.view(v) {
            View::Sym(s) if h.sym_name(s) == "right" => Side::Right,
            View::Sym(s) if h.sym_name(s) == "left" => Side::Left,
            View::Sym(s) if h.sym_name(s) == "center" => Side::Center,
            _ => return Ok(lib.bad_arg(h, t, args.len() + 1)),
        };
    }
    let (s, n, pad) = match *args {
        [s, n] => (s, n, " "),
        [s, n, pad] => (s, n, take!(lib.needle(h, t, pad, 3))),
        _ => return Ok(Reply::Decline),
    };
    let s = take!(lib.string(h, t, s, 1));
    let n = match take!(lib.int(h, t, n, 2)) {
        n @ 0.. => n as u64,
        _ => return Ok(lib.bad_arg(h, t, 2)),
    };
    let len = s.chars().count() as u64;
    if len >= n {
        return Ok(Reply::Done(args[0]));
    }
    let missing = n - len;
    let pad_len = pad.chars().count() as u64;
    if missing.saturating_mul(pad.len() as u64 / pad_len) > MAX_BYTES as u64 {
        return Ok(too_large(lib, h, t));
    }
    charge(cx, (missing as usize).saturating_mul(4) + s.len())?;
    let fill = |k: u64| -> String { pad.chars().cycle().take(k as usize).collect() };
    let (before, after) = match side {
        Side::Right => (0, missing),
        Side::Left => (missing, 0),
        Side::Center => (missing / 2, missing - missing / 2),
    };
    Ok(Reply::Done(h.string(&format!("{}{s}{}", fill(before), fill(after)))))
}

/// `string_trim(s)`: s without leading and trailing white space; `string_trim(s, sub)`: without the occurrences of
/// the nonempty string sub repeated at either end.
pub(crate) fn string_trim<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (s, sub) = match *args {
        [s] => (take!(lib.string(h, t, s, 1)), None),
        [s, sub] => (take!(lib.string(h, t, s, 1)), Some(take!(lib.needle(h, t, sub, 2)))),
        _ => return Ok(Reply::Decline),
    };
    charge(cx, s.len())?;
    let r = match sub {
        None => s.trim(),
        Some(sub) => {
            let mut r = s;
            while let Some(x) = r.strip_prefix(sub) {
                r = x;
            }
            while let Some(x) = r.strip_suffix(sub) {
                r = x;
            }
            r
        }
    };
    Ok(Reply::Done(if r.len() == s.len() { args[0] } else { h.string(r) }))
}
