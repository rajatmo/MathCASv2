//! Binary data (round 29): `byte_array`, `binary_write` and `binary_read_list`.
//!
//! `byte_array([b1, b2, ...])` is a list of bytes (integers 0 to 255) held as that term; any other argument leaves
//! it unevaluated. `binary_read_list(source, type, count)` reads a file (a path string) or a `byte_array` as a
//! list of values of a type: `"Byte"` (the default), `"Character8"` (one-character strings), `"Integer8"`,
//! `"Integer16"`, `"Integer32"`, `"Integer64"`, the `"UnsignedInteger..."` forms, `"Real32"`, `"Real64"`; little
//! endian unless `byte_order: 1` is given. A list of types is read in turn, cycling, and gives one list per
//! cycle. `binary_write(file, data, type)` writes bytes, integers or reals the same way (and a string as its
//! bytes) and gives the file name. A file that cannot be read is `failure(io, file, message)`.
use crate::failure;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};

/// `byte_array(list)`.
pub(crate) struct ByteArray;
/// `binary_read_list(source, type, count)`.
pub(crate) struct ReadList;
/// `binary_write(file, data, type)`.
pub(crate) struct Write;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Byte,
    Char,
    Int(usize),
    Uint(usize),
    Real(usize),
}

impl Kind {
    fn named(s: &str) -> Option<Kind> {
        Some(match s {
            "Byte" => Kind::Byte,
            "Character8" => Kind::Char,
            "Integer8" => Kind::Int(1),
            "Integer16" => Kind::Int(2),
            "Integer32" => Kind::Int(4),
            "Integer64" => Kind::Int(8),
            "UnsignedInteger8" => Kind::Uint(1),
            "UnsignedInteger16" => Kind::Uint(2),
            "UnsignedInteger32" => Kind::Uint(4),
            "UnsignedInteger64" => Kind::Uint(8),
            "Real32" => Kind::Real(4),
            "Real64" => Kind::Real(8),
            _ => return None,
        })
    }

    fn size(self) -> usize {
        match self {
            Kind::Byte | Kind::Char => 1,
            Kind::Int(n) | Kind::Uint(n) | Kind::Real(n) => n,
        }
    }
}

/// The bytes of a `byte_array([...])` term, or of a list of byte values.
fn bytes_of<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<u8>> {
    let h = cx.heap();
    let list = match h.view(t) {
        View::App { head, args: [l] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "byte_array") => *l,
        _ => t,
    };
    cx.as_list(list)?.iter().map(|&x| if let View::Int(v) = h.view(x) { u8::try_from(v).ok() } else { None }).collect()
}

impl Native for ByteArray {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        // a list of bytes is its own value; a list holding anything else is not a byte array
        let h = cx.heap();
        let _ = t;
        let &[l] = args else { return Ok(Reply::Decline) };
        let Some(items) = cx.as_list(l) else { return Ok(Reply::Decline) };
        let numeric = items.iter().all(|&x| matches!(h.view(x), View::Int(_) | View::Real(_)));
        if numeric && bytes_of(cx, l).is_none() {
            return Ok(Reply::Done(crate::bad_argument(h, "byte_array", 1)));
        }
        Ok(Reply::Decline)
    }
}

fn kinds<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<Kind>> {
    let h = cx.heap();
    let one = |x: T<'h>| match h.view(x) {
        View::Str(s) => Kind::named(s),
        _ => None,
    };
    match cx.as_list(t) {
        Some(l) if !l.is_empty() => l.iter().map(|&x| one(x)).collect(),
        Some(_) => None,
        None => one(t).map(|k| vec![k]),
    }
}

fn decode<'h>(h: &'h Heap, k: Kind, b: &[u8], big: bool) -> T<'h> {
    let mut v = b.to_vec();
    if big {
        v.reverse();
    }
    match k {
        Kind::Byte => h.int(b[0] as i64),
        Kind::Char => h.string(&(b[0] as char).to_string()),
        Kind::Int(n) => {
            let mut x = 0i64;
            for (i, &byte) in v.iter().enumerate() {
                x |= (byte as i64) << (8 * i);
            }
            let bits = 8 * n as u32;
            if bits < 64 && x >> (bits - 1) & 1 == 1 {
                x -= 1i64 << bits;
            }
            h.int(x)
        }
        Kind::Uint(n) => {
            let mut x = 0u64;
            for (i, &byte) in v.iter().enumerate() {
                x |= (byte as u64) << (8 * i);
            }
            let _ = n;
            match i64::try_from(x) {
                Ok(i) => h.int(i),
                // above the machine integer range: a machine real
                Err(_) => h.real(x as f64),
            }
        }
        Kind::Real(4) => h.real(f32::from_le_bytes([v[0], v[1], v[2], v[3]]) as f64),
        Kind::Real(_) => h.real(f64::from_le_bytes([v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]])),
    }
}

/// The arguments without the trailing options, and whether `byte_order: 1` (big endian) was given.
fn split<'a, 'h>(h: &'h Heap, f: &str, args: &'a [T<'h>]) -> Result<(&'a [T<'h>], bool), T<'h>> {
    let (rest, o) = crate::split_options(h, args);
    let opts = crate::Opts::parse(h, o, &["byte_order"])?;
    let big = match opts.pairs.iter().find(|p| p.0 == "byte_order").map(|p| h.view(p.1)) {
        None | Some(View::Int(-1)) => false,
        Some(View::Int(1)) => true,
        Some(_) => return Err(crate::bad_argument(h, f, 0)),
    };
    Ok((rest, big))
}

impl Native for ReadList {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (args, big) = match split(h, "binary_read_list", args) {
            Ok(x) => x,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let Some((&src, rest)) = args.split_first() else { return Ok(Reply::Decline) };
        let rest = rest.to_vec();
        let data = if let Some(b) = bytes_of(cx, src) {
            b
        } else {
            match h.view(src) {
                View::Str(p) => match std::fs::read(p) {
                    Ok(b) => b,
                    Err(e) => return Ok(Reply::Done(failure(h, "io", &[src, h.string(&e.to_string())]))),
                },
                _ => return Ok(Reply::Decline),
            }
        };
        let ks = match rest.first() {
            None => vec![Kind::Byte],
            Some(&t) => match kinds(cx, t) {
                Some(k) => k,
                None => return Ok(Reply::Done(crate::bad_argument(h, "binary_read_list", 2))),
            },
        };
        let limit = match rest.get(1).map(|&c| h.view(c)) {
            None => usize::MAX,
            Some(View::Int(n)) if n >= 0 => n as usize,
            Some(_) => return Ok(Reply::Done(crate::bad_argument(h, "binary_read_list", 3))),
        };
        let mut pos = 0;
        let mut items: Vec<T<'h>> = vec![];
        let mut cycles: Vec<T<'h>> = vec![];
        let mut count = 0;
        'outer: while count < limit {
            let mut cycle = vec![];
            for &k in &ks {
                let n = k.size();
                if pos + n > data.len() {
                    break 'outer;
                }
                cycle.push(decode(h, k, &data[pos..pos + n], big));
                pos += n;
            }
            count += 1;
            if ks.len() == 1 {
                items.push(cycle[0]);
            } else {
                cycles.push(cx.list(&cycle));
            }
        }
        if ks.len() == 1 {
            if ks[0] == Kind::Char {
                return Ok(Reply::Done(cx.list(&items)));
            }
            return Ok(Reply::Done(cx.list(&items)));
        }
        Ok(Reply::Done(cx.list(&cycles)))
    }
}

impl Native for Write {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (args, big) = match split(h, "binary_write", args) {
            Ok(x) => x,
            Err(f) => return Ok(Reply::Done(f)),
        };
        let (file, data) = match args {
            [f, d, ..] => (*f, *d),
            _ => return Ok(Reply::Decline),
        };
        let View::Str(path) = h.view(file) else { return Ok(Reply::Done(crate::bad_argument(h, "binary_write", 1))) };
        let k = match args.get(2) {
            None => Kind::Byte,
            Some(&t) => match kinds(cx, t).as_deref() {
                Some([k]) => *k,
                _ => return Ok(Reply::Done(crate::bad_argument(h, "binary_write", 3))),
            },
        };
        let items: Vec<T<'h>> = match h.view(data) {
            View::Str(s) => s.bytes().map(|b| h.int(b as i64)).collect(),
            _ => match bytes_of(cx, data).map(|b| b.into_iter().map(|x| h.int(x as i64)).collect::<Vec<_>>()) {
                Some(v) => v,
                None => match cx.as_list(data) {
                    Some(l) => l.to_vec(),
                    None => vec![data],
                },
            },
        };
        let mut out: Vec<u8> = vec![];
        for x in items {
            let (iv, rv) = match h.view(x) {
                View::Int(i) => (Some(i), i as f64),
                View::Real(r) => (None, r),
                _ => return Ok(Reply::Done(crate::bad_argument(h, "binary_write", 2))),
            };
            let mut b: Vec<u8> = match (k, iv) {
                (Kind::Byte | Kind::Char | Kind::Uint(1) | Kind::Int(1), Some(i)) if (-128..=255).contains(&i) => vec![i as u8],
                (Kind::Int(n) | Kind::Uint(n), Some(i)) => i.to_le_bytes()[..n].to_vec(),
                (Kind::Real(4), _) => (rv as f32).to_le_bytes().to_vec(),
                (Kind::Real(_), _) => rv.to_le_bytes().to_vec(),
                _ => return Ok(Reply::Done(crate::bad_argument(h, "binary_write", 2))),
            };
            if big {
                b.reverse();
            }
            out.extend(b);
        }
        Ok(Reply::Done(match std::fs::write(path, out) {
            Ok(()) => file,
            Err(e) => failure(h, "io", &[file, h.string(&e.to_string())]),
        }))
    }
}
