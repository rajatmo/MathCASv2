//! Structural hashes. Every hash depends only on a term's structure and its symbols' names, never on handles, so it
//! is stable across collections (the contract's `structural_hash`, and the key of rule indexes, A.2).
//!
//! - Compound terms use spike 4's polynomial hash (decision 4.3): `start * K^n + sum a_i K^(n-1-i)`, which combines
//!   across a split, so very wide nodes hash in parallel. The combiner carries the combined length (A.8, I1.8).
//! - Payloads (big integers, balls, strings) are hashed in full with four independent lanes (I3.4).
use rayon::prelude::*;

/// Hash in parallel? On a pool thread outside a scratch scope: the jobs read the nodes of the term, and a scratch
/// term can be read only by the thread of its scope (decision G.2).
#[inline]
fn par_here() -> bool {
    rayon::current_thread_index().is_some() && !crate::scratch::scratch_active()
}

pub(crate) const K: u64 = 0x9E37_79B9_7F4A_7C15;

/// Arguments from which a compound term is hashed on the pool (when built inside one).
pub(crate) const PAR_ARGS: usize = 1 << 15;
const PAR_CHUNK: usize = 1 << 13;

/// SplitMix64's finaliser.
#[inline]
pub(crate) fn finalize(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline]
pub(crate) fn mix(h: u64, v: u64) -> u64 {
    (h.rotate_left(5) ^ v).wrapping_mul(K)
}

/// FNV-1a over bytes (symbol names).
pub(crate) fn bytes_hash(s: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in s {
        h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
    }
    h
}

pub(crate) fn sym_hash(name: &str) -> u64 {
    finalize(mix(0x51, bytes_hash(name.as_bytes())))
}

#[inline]
pub(crate) fn small_int_hash(v: i64) -> u64 {
    mix(0x17, v as u64)
}

pub(crate) fn word_hash(neg: bool, mag: u64) -> u64 {
    finalize(mix(mix(0x18, neg as u64), mag))
}

pub(crate) fn real_hash(bits: u64) -> u64 {
    finalize(mix(0x52, bits))
}

pub(crate) fn rat_hash(num: u64, den: u64) -> u64 {
    finalize(mix(mix(0x22, num), den))
}

/// The hash of a payload: its kind tag, flag word (sign, or byte length of a string), length, and every word.
pub(crate) fn payload_hash(tag: u64, flag: u32, words: &[u64]) -> u64 {
    let seed = mix(mix(0x4E55_4D42, tag), (flag as u64) << 32 | words.len() as u64);
    finalize(mix(seed, words_hash(words)))
}

/// Four independent lanes (the multiplications pipeline), then combined (I3: about 0.3 ns per word).
fn words_hash(words: &[u64]) -> u64 {
    let mut l = [0x243F_6A88_85A3_08D3u64, 0x1319_8A2E_0370_7344, 0xA409_3822_299F_31D0, 0x082E_FA98_EC4E_6C89];
    let (chunks, rest) = words.as_chunks::<4>();
    for c in chunks {
        for j in 0..4 {
            l[j] = (l[j] ^ c[j]).wrapping_mul(K).rotate_left(29);
        }
    }
    for (j, &w) in rest.iter().enumerate() {
        l[j] = (l[j] ^ w).wrapping_mul(K).rotate_left(29);
    }
    mix(mix(mix(l[0], l[1]), l[2]), l[3])
}

/// `K^n`.
fn kpow(mut n: usize) -> u64 {
    let (mut r, mut b) = (1u64, K);
    while n > 0 {
        if n & 1 != 0 {
            r = r.wrapping_mul(b);
        }
        b = b.wrapping_mul(b);
        n >>= 1;
    }
    r
}

/// A part of a compound term's polynomial: hash, symbol mask, number of arguments.
type Part = (u64, u64, usize);

fn fold(args: &[u32], meta: &impl Fn(u32) -> (u64, u64)) -> Part {
    let (mut h, mut m) = (0u64, 0u64);
    for &a in args {
        let (ah, am) = meta(a);
        h = h.wrapping_mul(K).wrapping_add(ah);
        m |= am;
    }
    (h, m, args.len())
}

/// Hash and symbol mask of a compound term from its head's and arguments' `(hash, mask)`. Wide terms built on a
/// pool thread are hashed in parallel; the result is the same either way.
pub(crate) fn app_hash(head: (u64, u64), args: &[u32], meta: impl Fn(u32) -> (u64, u64) + Sync) -> (u64, u64) {
    let (p, m, _) = if args.len() >= PAR_ARGS && par_here() {
        args.par_chunks(PAR_CHUNK).map(|c| fold(c, &meta)).reduce(
            || (0, 0, 0),
            // the combined length must be carried (A.8): resetting it makes the hash depend on the reduction tree
            |(h1, m1, n1), (h2, m2, n2)| (h1.wrapping_mul(kpow(n2)).wrapping_add(h2), m1 | m2, n1 + n2),
        )
    } else {
        fold(args, &meta)
    };
    let start = 0x33u64.wrapping_mul(K).wrapping_add(head.0);
    (finalize(start.wrapping_mul(kpow(args.len())).wrapping_add(p)), m | head.1)
}

/// The polynomial part `sum h(i) K^(n-1-i)` of `n` argument hashes, in parallel for wide terms built on a pool
/// thread (the same value as the sequential fold, as in [`app_hash`]).
fn poly(n: usize, h: &(impl Fn(usize) -> u64 + Sync)) -> u64 {
    let seq = |r: std::ops::Range<usize>| r.fold(0u64, |acc, i| acc.wrapping_mul(K).wrapping_add(h(i)));
    if n >= PAR_ARGS && par_here() {
        let chunks = n.div_ceil(PAR_CHUNK);
        let (p, _) = (0..chunks)
            .into_par_iter()
            .map(|c| {
                let r = c * PAR_CHUNK..((c + 1) * PAR_CHUNK).min(n);
                (seq(r.clone()), r.len())
            })
            .reduce(|| (0, 0), |(h1, n1), (h2, n2)| (h1.wrapping_mul(kpow(n2)).wrapping_add(h2), n1 + n2));
        p
    } else {
        seq(0..n)
    }
}

/// The hash of a compound term from its head's hash, its argument count and [`poly`] of its arguments.
#[inline]
fn close(head: u64, n: usize, p: u64) -> u64 {
    let start = 0x33u64.wrapping_mul(K).wrapping_add(head);
    finalize(start.wrapping_mul(kpow(n)).wrapping_add(p))
}

/// The hash of a machine integer term, whichever tier holds it (immediate or one-limb `Word`).
#[inline]
pub(crate) fn machine_int_hash(v: i64) -> u64 {
    if (crate::SMALL_MIN..=crate::SMALL_MAX).contains(&v) {
        small_int_hash(v)
    } else {
        word_hash(v < 0, v.unsigned_abs())
    }
}

/// Hash and symbol mask of a packed array: exactly those of the compound `list(...)` it stands for ([`app_hash`] of
/// the rows, recursively), computed from the leaves without building them. `list` is the `(hash, mask)` of the
/// symbol `list`, `ints` the leaf kind, `dims` the dimensions (outermost first), `data` the leaves as words.
pub(crate) fn packed_hash(list: (u64, u64), ints: bool, dims: &[u64], data: &[u64]) -> (u64, u64) {
    let leaf = |w: u64| if ints { machine_int_hash(w as i64) } else { real_hash(w) };
    // invariant: a packed array has rank 1 to 16 (checked when it is built)
    let (&last, outer) = dims.split_last().expect("a packed array has a rank");
    let d = last as usize;
    let rows = data.len() / d;
    let row_hash = |r: usize| close(list.0, d, poly(d, &|i| leaf(data[r * d + i])));
    if outer.is_empty() {
        return (row_hash(0), list.1);
    }
    // the hashes of the innermost rows, then level by level outwards
    let mut level: Vec<u64> = if rows >= 64 && par_here() {
        (0..rows).into_par_iter().with_min_len(64).map(row_hash).collect()
    } else {
        (0..rows).map(row_hash).collect()
    };
    for &k in outer.iter().rev() {
        let k = k as usize;
        level = (0..level.len() / k).map(|r| close(list.0, k, poly(k, &|i| level[r * k + i]))).collect();
    }
    (level[0], list.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_and_sequential_hashes_agree() {
        let args: Vec<u32> = (0..(3 * PAR_ARGS as u32 + 17)).collect();
        let meta = |a: u32| (finalize(a as u64), 1u64 << (a % 64));
        let seq = app_hash((5, 0), &args, meta);
        let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
        for _ in 0..8 {
            assert_eq!(pool.install(|| app_hash((5, 0), &args, meta)), seq);
        }
    }

    #[test]
    fn packed_hashes_are_the_compound_hashes() {
        let list = (finalize(77), 1u64 << 9);
        // a vector of reals, wide enough for the parallel fold, against app_hash over the leaves' hashes
        let n = 2 * PAR_ARGS + 5;
        let data: Vec<u64> = (0..n as u64).map(|i| (i as f64 * 0.5).to_bits()).collect();
        let ids: Vec<u32> = (0..n as u32).collect();
        let want = app_hash(list, &ids, |i| (real_hash(data[i as usize]), 0));
        assert_eq!(packed_hash(list, false, &[n as u64], &data), want);
        let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
        assert_eq!(pool.install(|| packed_hash(list, false, &[n as u64], &data)), want);
        // a 3 x 100 matrix of integers across the tiers: rows first
        let ints: Vec<i64> = (0..300).map(|i| (i - 150) * (1 << 25)).collect();
        let words: Vec<u64> = ints.iter().map(|&v| v as u64).collect();
        let rows: Vec<u64> = (0..3)
            .map(|r| {
                let ids: Vec<u32> = (0..100).collect();
                app_hash(list, &ids, |i| (machine_int_hash(ints[r * 100 + i as usize]), 0)).0
            })
            .collect();
        let want = app_hash(list, &[0, 1, 2], |i| (rows[i as usize], list.1));
        assert_eq!(packed_hash(list, true, &[3, 100], &words), want);
        assert_eq!(pool.install(|| packed_hash(list, true, &[3, 100], &words)), want);
    }
}
