//! The generator: Philox4x32-10 (Salmon, Moraes, Dror, Shaw, "Parallel random numbers: as easy as 1, 2, 3",
//! SC 2011), a counter-based generator. A block of 128 random bits is a pure function `philox(key, counter)` of a
//! 64-bit key and a 128-bit counter, so a stream is just `(key, counter)` and independent streams are streams with
//! different keys (decision RNG.1).
//!
//! [`Stream`] is one such stream with the conversions of [`crate::draw`] on top; [`Stream::split`] derives the
//! stream of element `j` of a parallel construct (semantics 18.2 (5)) from the key alone.

/// Philox's multipliers and Weyl increments (the paper's constants for N = 4, W = 32).
const M0: u32 = 0xD251_1F53;
const M1: u32 = 0xCD9E_8D57;
const W0: u32 = 0x9E37_79B9;
const W1: u32 = 0xBB67_AE85;

#[inline(always)]
fn mulhilo(a: u32, b: u32) -> (u32, u32) {
    let p = a as u64 * b as u64;
    ((p >> 32) as u32, p as u32)
}

#[inline(always)]
fn round(c: [u32; 4], k: [u32; 2]) -> [u32; 4] {
    let (hi0, lo0) = mulhilo(M0, c[0]);
    let (hi1, lo1) = mulhilo(M1, c[2]);
    [hi1 ^ c[1] ^ k[0], lo1, hi0 ^ c[3] ^ k[1], lo0]
}

/// Philox4x32 with 10 rounds: the block for counter `c` under key `k` (words as in Random123: `c[0]` and `k[0]` are
/// the low words).
#[inline]
pub fn philox4x32_10(mut c: [u32; 4], mut k: [u32; 2]) -> [u32; 4] {
    c = round(c, k);
    for _ in 1..10 {
        k = [k[0].wrapping_add(W0), k[1].wrapping_add(W1)];
        c = round(c, k);
    }
    c
}

/// SplitMix64's finaliser (Steele, Lea, Flood, OOPSLA 2014; the constants of Stafford's "Mix13"): a bijection of
/// `u64` with good avalanche, used to derive keys.
#[inline]
pub fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// One random stream: a key and the position in it (decision RNG.2). Each block gives two 64-bit words, used low
/// word first; `spare` is the second word of the block before `ctr` while it is unused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stream {
    key: u64,
    ctr: u128,
    spare: Option<u64>,
}

impl Stream {
    /// The stream with this key, at its start.
    pub fn from_key(key: u64) -> Stream {
        Stream { key, ctr: 0, spare: None }
    }

    /// The key.
    pub fn key(&self) -> u64 {
        self.key
    }

    /// The number of 64-bit words drawn so far.
    pub fn position(&self) -> u128 {
        self.ctr * 2 - self.spare.is_some() as u128
    }

    /// The block at counter `ctr` as two words (low word first).
    #[inline]
    pub fn block(key: u64, ctr: u128) -> [u64; 2] {
        let c = [ctr as u32, (ctr >> 32) as u32, (ctr >> 64) as u32, (ctr >> 96) as u32];
        let r = philox4x32_10(c, [key as u32, (key >> 32) as u32]);
        [r[0] as u64 | (r[1] as u64) << 32, r[2] as u64 | (r[3] as u64) << 32]
    }

    /// The next 64 random bits.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        if let Some(w) = self.spare.take() {
            return w;
        }
        let [a, b] = Stream::block(self.key, self.ctr);
        self.ctr = self.ctr.wrapping_add(1);
        self.spare = Some(b);
        a
    }

    /// The stream of element `index` of a parallel construct whose base stream is `self` (decision RNG.3): its key
    /// is derived from this key and the index only, so it does not depend on this stream's position, on the order
    /// in which elements run, or on the number of threads. A construct takes its base with [`Stream::fork`] so that
    /// two constructs in one session get different element streams. Splits nest: `s.split(i).split(j)`.
    pub fn split(&self, index: u64) -> Stream {
        Stream::from_key(mix64(self.key ^ mix64(index.wrapping_add(0x6A09_E667_F3BC_C909))))
    }

    /// A new base stream taken from this one: one word is drawn (this stream advances) and becomes the key.
    pub fn fork(&mut self) -> Stream {
        Stream::from_key(self.next_u64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The known-answer vectors of Random123 1.09 (`examples/kat_vectors`, lines `philox4x32 10`).
    #[test]
    fn known_answers() {
        let cases: [([u32; 4], [u32; 2], [u32; 4]); 3] = [
            ([0, 0, 0, 0], [0, 0], [0x6627_e8d5, 0xe169_c58d, 0xbc57_ac4c, 0x9b00_dbd8]),
            ([0xffff_ffff; 4], [0xffff_ffff; 2], [0x408f_276d, 0x41c8_3b0e, 0xa20b_c7c6, 0x6d54_51fd]),
            (
                [0x243f_6a88, 0x85a3_08d3, 0x1319_8a2e, 0x0370_7344],
                [0xa409_3822, 0x299f_31d0],
                [0xd16c_fe09, 0x94fd_cceb, 0x5001_e420, 0x2412_6ea1],
            ),
        ];
        for (c, k, want) in cases {
            assert_eq!(philox4x32_10(c, k), want, "ctr {c:08x?} key {k:08x?}");
        }
    }

    #[test]
    fn words_follow_the_counter() {
        let mut s = Stream::from_key(42);
        let b0 = Stream::block(42, 0);
        let b1 = Stream::block(42, 1);
        assert_eq!([s.next_u64(), s.next_u64(), s.next_u64(), s.next_u64()], [b0[0], b0[1], b1[0], b1[1]]);
        assert_eq!(s.position(), 4);
        s.next_u64();
        assert_eq!(s.position(), 5);
    }

    #[test]
    fn splits_are_deterministic_and_distinct() {
        let mut base = Stream::from_key(7);
        // the position of the base does not matter
        let a: Vec<u64> = (0..8).map(|j| base.split(j).next_u64()).collect();
        base.next_u64();
        let b: Vec<u64> = (0..8).map(|j| base.split(j).next_u64()).collect();
        assert_eq!(a, b);
        // in any order, from any thread
        let c: Vec<u64> = std::thread::scope(|sc| {
            let hs: Vec<_> =
                (0..8u64).rev().map(|j| (j, sc.spawn(move || Stream::from_key(7).split(j).next_u64()))).collect();
            let mut v = vec![0; 8];
            for (j, hd) in hs {
                v[j as usize] = hd.join().unwrap();
            }
            v
        });
        assert_eq!(a, c);
        let mut sorted = a.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 8);
        // nested splits differ from flat ones; a fork advances the parent and gives a new base
        assert_ne!(Stream::from_key(7).split(1).split(2).next_u64(), Stream::from_key(7).split(2).split(1).next_u64());
        let mut p = Stream::from_key(7);
        let f1 = p.fork();
        let f2 = p.fork();
        assert_ne!(f1.split(0).next_u64(), f2.split(0).next_u64());
        assert_eq!(p.position(), 2);
    }
}
