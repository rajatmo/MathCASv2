//! Further message digests of `hash`, each implemented from its specification:
//!
//! - MD2 (RFC 1319), MD4 (RFC 1320);
//! - RIPEMD-160 (Dobbertin, Bosselaers and Preneel, "RIPEMD-160: a strengthened version of RIPEMD", FSE 1996);
//! - SHA-3 and the original Keccak padding (FIPS 202: the Keccak-f[1600] permutation, the sponge with the domain
//!   bits 01 for SHA-3 and none for Keccak);
//! - BLAKE2b and BLAKE2s with any digest length (RFC 7693; the length is a parameter of the hash, so a shorter digest
//!   is not a prefix of a longer one), and the extendable-output BLAKE2Xb and BLAKE2Xs (Aumasson, Neves,
//!   Wilcox-O'Hearn and Winnerlein, "BLAKE2X", 2016: a root hash with the output length in its parameter block,
//!   then one hash of the root per 32 or 64 output bytes with the block's index as the node offset).

/// MD2's substitution of the bytes, from the digits of pi (RFC 1319, section 3.2).
const MD2_S: [u8; 256] = [
    41, 46, 67, 201, 162, 216, 124, 1, 61, 54, 84, 161, 236, 240, 6, 19, 98, 167, 5, 243, 192, 199, 115, 140, 152, 147,
    43, 217, 188, 76, 130, 202, 30, 155, 87, 60, 253, 212, 224, 22, 103, 66, 111, 24, 138, 23, 229, 18, 190, 78, 196,
    214, 218, 158, 222, 73, 160, 251, 245, 142, 187, 47, 238, 122, 169, 104, 121, 145, 21, 178, 7, 63, 148, 194, 16,
    137, 11, 34, 95, 33, 128, 127, 93, 154, 90, 144, 50, 39, 53, 62, 204, 231, 191, 247, 151, 3, 255, 25, 48, 179, 72,
    165, 181, 209, 215, 94, 146, 42, 172, 86, 170, 198, 79, 184, 56, 210, 150, 164, 125, 182, 118, 252, 107, 226, 156,
    116, 4, 241, 69, 157, 112, 89, 100, 113, 135, 32, 134, 91, 207, 101, 230, 45, 168, 2, 27, 96, 37, 173, 174, 176,
    185, 246, 28, 70, 97, 105, 52, 64, 126, 15, 85, 71, 163, 35, 221, 81, 175, 58, 195, 92, 249, 206, 186, 197, 234,
    38, 44, 83, 13, 110, 133, 40, 132, 9, 211, 223, 205, 244, 65, 129, 77, 82, 106, 220, 55, 200, 108, 193, 171, 250,
    36, 225, 123, 8, 12, 189, 177, 74, 120, 136, 149, 139, 227, 99, 232, 109, 233, 203, 213, 254, 59, 0, 29, 57, 242,
    239, 183, 14, 102, 88, 208, 228, 166, 119, 114, 248, 235, 117, 75, 10, 49, 68, 80, 180, 143, 237, 31, 26, 219,
    153, 141, 51, 159, 17, 131, 20,
];

/// MD2 (RFC 1319).
pub fn md2(msg: &[u8]) -> Vec<u8> {
    let pad = 16 - msg.len() % 16;
    let mut m = msg.to_vec();
    m.extend(std::iter::repeat_n(pad as u8, pad));
    // the checksum block
    let mut c = [0u8; 16];
    let mut l = 0u8;
    for block in m.chunks(16) {
        for j in 0..16 {
            c[j] ^= MD2_S[(block[j] ^ l) as usize];
            l = c[j];
        }
    }
    m.extend_from_slice(&c);
    let mut x = [0u8; 48];
    for block in m.chunks(16) {
        for j in 0..16 {
            x[16 + j] = block[j];
            x[32 + j] = x[16 + j] ^ x[j];
        }
        let mut t = 0u8;
        for j in 0..18u8 {
            for v in x.iter_mut() {
                *v ^= MD2_S[t as usize];
                t = *v;
            }
            t = t.wrapping_add(j);
        }
    }
    x[..16].to_vec()
}

/// The message padded as for MD4, MD5 and RIPEMD-160: a 1 bit, zeros, the bit length as 8 little-endian bytes.
fn pad_le(msg: &[u8]) -> Vec<u8> {
    let mut m = msg.to_vec();
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    m.extend_from_slice(&(msg.len() as u64).wrapping_mul(8).to_le_bytes());
    m
}

fn words_le(block: &[u8]) -> [u32; 16] {
    let mut x = [0u32; 16];
    for (i, b) in block.chunks(4).enumerate() {
        x[i] = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    }
    x
}

/// MD4 (RFC 1320).
pub fn md4(msg: &[u8]) -> Vec<u8> {
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    for block in pad_le(msg).chunks(64) {
        let x = words_le(block);
        let [mut a, mut b, mut c, mut d] = h;
        let f = |x: u32, y: u32, z: u32| (x & y) | (!x & z);
        let g = |x: u32, y: u32, z: u32| (x & y) | (x & z) | (y & z);
        let hh = |x: u32, y: u32, z: u32| x ^ y ^ z;
        for i in [0, 4, 8, 12] {
            a = a.wrapping_add(f(b, c, d)).wrapping_add(x[i]).rotate_left(3);
            d = d.wrapping_add(f(a, b, c)).wrapping_add(x[i + 1]).rotate_left(7);
            c = c.wrapping_add(f(d, a, b)).wrapping_add(x[i + 2]).rotate_left(11);
            b = b.wrapping_add(f(c, d, a)).wrapping_add(x[i + 3]).rotate_left(19);
        }
        let k2 = 0x5A827999u32;
        for i in 0..4 {
            a = a.wrapping_add(g(b, c, d)).wrapping_add(x[i]).wrapping_add(k2).rotate_left(3);
            d = d.wrapping_add(g(a, b, c)).wrapping_add(x[i + 4]).wrapping_add(k2).rotate_left(5);
            c = c.wrapping_add(g(d, a, b)).wrapping_add(x[i + 8]).wrapping_add(k2).rotate_left(9);
            b = b.wrapping_add(g(c, d, a)).wrapping_add(x[i + 12]).wrapping_add(k2).rotate_left(13);
        }
        let k3 = 0x6ED9EBA1u32;
        for i in [0, 2, 1, 3] {
            a = a.wrapping_add(hh(b, c, d)).wrapping_add(x[i]).wrapping_add(k3).rotate_left(3);
            d = d.wrapping_add(hh(a, b, c)).wrapping_add(x[i + 8]).wrapping_add(k3).rotate_left(9);
            c = c.wrapping_add(hh(d, a, b)).wrapping_add(x[i + 4]).wrapping_add(k3).rotate_left(11);
            b = b.wrapping_add(hh(c, d, a)).wrapping_add(x[i + 12]).wrapping_add(k3).rotate_left(15);
        }
        h = [h[0].wrapping_add(a), h[1].wrapping_add(b), h[2].wrapping_add(c), h[3].wrapping_add(d)];
    }
    h.iter().flat_map(|w| w.to_le_bytes()).collect()
}

const RMD_R: [usize; 80] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 7, 4, 13, 1, 10, 6, 15, 3, 12, 0, 9, 5, 2, 14, 11, 8, 3, 10,
    14, 4, 9, 15, 8, 1, 2, 7, 0, 6, 13, 11, 5, 12, 1, 9, 11, 10, 0, 8, 12, 4, 13, 3, 7, 15, 14, 5, 6, 2, 4, 0, 5, 9, 7,
    12, 2, 10, 14, 1, 3, 8, 11, 6, 15, 13,
];
const RMD_RP: [usize; 80] = [
    5, 14, 7, 0, 9, 2, 11, 4, 13, 6, 15, 8, 1, 10, 3, 12, 6, 11, 3, 7, 0, 13, 5, 10, 14, 15, 8, 12, 4, 9, 1, 2, 15, 5,
    1, 3, 7, 14, 6, 9, 11, 8, 12, 2, 10, 0, 4, 13, 8, 6, 4, 1, 3, 11, 15, 0, 5, 12, 2, 13, 9, 7, 10, 14, 12, 15, 10, 4,
    1, 5, 8, 7, 6, 2, 13, 14, 0, 3, 9, 11,
];
const RMD_S: [u32; 80] = [
    11, 14, 15, 12, 5, 8, 7, 9, 11, 13, 14, 15, 6, 7, 9, 8, 7, 6, 8, 13, 11, 9, 7, 15, 7, 12, 15, 9, 11, 7, 13, 12, 11,
    13, 6, 7, 14, 9, 13, 15, 14, 8, 13, 6, 5, 12, 7, 5, 11, 12, 14, 15, 14, 15, 9, 8, 9, 14, 5, 6, 8, 6, 5, 12, 9, 15,
    5, 11, 6, 8, 13, 12, 5, 12, 13, 14, 11, 8, 5, 6,
];
const RMD_SP: [u32; 80] = [
    8, 9, 9, 11, 13, 15, 15, 5, 7, 7, 8, 11, 14, 14, 12, 6, 9, 13, 15, 7, 12, 8, 9, 11, 7, 7, 12, 7, 6, 15, 13, 11, 9,
    7, 15, 11, 8, 6, 6, 14, 12, 13, 5, 14, 13, 13, 7, 5, 15, 5, 8, 11, 14, 14, 6, 14, 6, 9, 12, 9, 12, 5, 15, 8, 8, 5,
    12, 9, 12, 5, 14, 6, 8, 13, 6, 5, 15, 13, 11, 11,
];

/// RIPEMD-160.
pub fn ripemd160(msg: &[u8]) -> Vec<u8> {
    let f = |j: usize, x: u32, y: u32, z: u32| match j / 16 {
        0 => x ^ y ^ z,
        1 => (x & y) | (!x & z),
        2 => (x | !y) ^ z,
        3 => (x & z) | (y & !z),
        _ => x ^ (y | !z),
    };
    let k = [0x00000000u32, 0x5A827999, 0x6ED9EBA1, 0x8F1BBCDC, 0xA953FD4E];
    let kp = [0x50A28BE6u32, 0x5C4DD124, 0x6D703EF3, 0x7A6D76E9, 0x00000000];
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    for block in pad_le(msg).chunks(64) {
        let x = words_le(block);
        let [mut a, mut b, mut c, mut d, mut e] = h;
        let [mut ap, mut bp, mut cp, mut dp, mut ep] = h;
        for j in 0..80 {
            let t = a.wrapping_add(f(j, b, c, d)).wrapping_add(x[RMD_R[j]]).wrapping_add(k[j / 16]).rotate_left(RMD_S[j]).wrapping_add(e);
            (a, e, d, c, b) = (e, d, c.rotate_left(10), b, t);
            let t = ap
                .wrapping_add(f(79 - j, bp, cp, dp))
                .wrapping_add(x[RMD_RP[j]])
                .wrapping_add(kp[j / 16])
                .rotate_left(RMD_SP[j])
                .wrapping_add(ep);
            (ap, ep, dp, cp, bp) = (ep, dp, cp.rotate_left(10), bp, t);
        }
        let t = h[1].wrapping_add(c).wrapping_add(dp);
        h[1] = h[2].wrapping_add(d).wrapping_add(ep);
        h[2] = h[3].wrapping_add(e).wrapping_add(ap);
        h[3] = h[4].wrapping_add(a).wrapping_add(bp);
        h[4] = h[0].wrapping_add(b).wrapping_add(cp);
        h[0] = t;
    }
    h.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// The Keccak-f[1600] permutation (FIPS 202, section 3), the lanes indexed x + 5 y.
fn keccak_f(a: &mut [u64; 25]) {
    // rotation offsets of rho and the round constants of iota, generated as FIPS 202 defines them
    let mut rot = [0u32; 25];
    let (mut x, mut y) = (1usize, 0usize);
    for t in 0..24u32 {
        rot[x + 5 * y] = ((t + 1) * (t + 2) / 2) % 64;
        (x, y) = (y, (2 * x + 3 * y) % 5);
    }
    let mut lfsr: u8 = 1;
    for _ in 0..24 {
        // theta
        let c: [u64; 5] = std::array::from_fn(|x| a[x] ^ a[x + 5] ^ a[x + 10] ^ a[x + 15] ^ a[x + 20]);
        for x in 0..5 {
            let d = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
            for y in 0..5 {
                a[x + 5 * y] ^= d;
            }
        }
        // rho and pi
        let mut b = [0u64; 25];
        for x in 0..5 {
            for y in 0..5 {
                b[y + 5 * ((2 * x + 3 * y) % 5)] = a[x + 5 * y].rotate_left(rot[x + 5 * y]);
            }
        }
        // chi
        for y in 0..5 {
            for x in 0..5 {
                a[x + 5 * y] = b[x + 5 * y] ^ (!b[(x + 1) % 5 + 5 * y] & b[(x + 2) % 5 + 5 * y]);
            }
        }
        // iota: the round constant from the LFSR x^8 + x^6 + x^5 + x^4 + 1
        let mut rc = 0u64;
        for j in 0..7 {
            if lfsr & 1 != 0 {
                rc ^= 1u64 << ((1u32 << j) - 1);
            }
            lfsr = if lfsr & 0x80 != 0 { (lfsr << 1) ^ 0x71 } else { lfsr << 1 };
        }
        a[0] ^= rc;
    }
}

/// The Keccak sponge with capacity twice the output: `domain` 0x06 for SHA-3, 0x01 for the original Keccak.
pub fn keccak(msg: &[u8], out: usize, domain: u8) -> Vec<u8> {
    let rate = 200 - 2 * out;
    let mut st = [0u64; 25];
    let mut m = msg.to_vec();
    m.push(domain);
    while !m.len().is_multiple_of(rate) {
        m.push(0);
    }
    let last = m.len() - 1;
    m[last] ^= 0x80;
    for block in m.chunks(rate) {
        for (i, w) in block.chunks(8).enumerate() {
            st[i] ^= u64::from_le_bytes([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]]);
        }
        keccak_f(&mut st);
    }
    st.iter().flat_map(|w| w.to_le_bytes()).take(out).collect()
}

const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

const IV32: [u32; 8] = [0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19];
const IV64: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

/// BLAKE2s of `msg` with the 32-byte parameter block `p` (digest length in byte 0, no key).
fn blake2s_param(msg: &[u8], p: &[u8; 32]) -> Vec<u8> {
    let mut h: [u32; 8] = std::array::from_fn(|i| IV32[i] ^ u32::from_le_bytes([p[4 * i], p[4 * i + 1], p[4 * i + 2], p[4 * i + 3]]));
    let blocks = msg.len().div_ceil(64).max(1);
    for bi in 0..blocks {
        let lo = bi * 64;
        let chunk = &msg[lo.min(msg.len())..(lo + 64).min(msg.len())];
        let mut block = [0u8; 64];
        block[..chunk.len()].copy_from_slice(chunk);
        let last = bi + 1 == blocks;
        let t = (lo + chunk.len()) as u64;
        let m: [u32; 16] = std::array::from_fn(|i| u32::from_le_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]));
        let mut v = [0u32; 16];
        v[..8].copy_from_slice(&h);
        v[8..].copy_from_slice(&IV32);
        v[12] ^= t as u32;
        v[13] ^= (t >> 32) as u32;
        if last {
            v[14] = !v[14];
        }
        let g = |v: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, x: u32, y: u32| {
            v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
            v[d] = (v[d] ^ v[a]).rotate_right(16);
            v[c] = v[c].wrapping_add(v[d]);
            v[b] = (v[b] ^ v[c]).rotate_right(12);
            v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
            v[d] = (v[d] ^ v[a]).rotate_right(8);
            v[c] = v[c].wrapping_add(v[d]);
            v[b] = (v[b] ^ v[c]).rotate_right(7);
        };
        for s in SIGMA.iter() {
            g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }
        for i in 0..8 {
            h[i] ^= v[i] ^ v[i + 8];
        }
    }
    h.iter().flat_map(|w| w.to_le_bytes()).take(p[0] as usize).collect()
}

/// BLAKE2b of `msg` with the 64-byte parameter block `p` (digest length in byte 0, no key).
fn blake2b_param(msg: &[u8], p: &[u8; 64]) -> Vec<u8> {
    let mut h: [u64; 8] = std::array::from_fn(|i| {
        IV64[i] ^ u64::from_le_bytes(std::array::from_fn(|k| p[8 * i + k]))
    });
    let blocks = msg.len().div_ceil(128).max(1);
    for bi in 0..blocks {
        let lo = bi * 128;
        let chunk = &msg[lo.min(msg.len())..(lo + 128).min(msg.len())];
        let mut block = [0u8; 128];
        block[..chunk.len()].copy_from_slice(chunk);
        let last = bi + 1 == blocks;
        let t = (lo + chunk.len()) as u128;
        let m: [u64; 16] = std::array::from_fn(|i| u64::from_le_bytes(std::array::from_fn(|k| block[8 * i + k])));
        let mut v = [0u64; 16];
        v[..8].copy_from_slice(&h);
        v[8..].copy_from_slice(&IV64);
        v[12] ^= t as u64;
        v[13] ^= (t >> 64) as u64;
        if last {
            v[14] = !v[14];
        }
        let g = |v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64| {
            v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
            v[d] = (v[d] ^ v[a]).rotate_right(32);
            v[c] = v[c].wrapping_add(v[d]);
            v[b] = (v[b] ^ v[c]).rotate_right(24);
            v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
            v[d] = (v[d] ^ v[a]).rotate_right(16);
            v[c] = v[c].wrapping_add(v[d]);
            v[b] = (v[b] ^ v[c]).rotate_right(63);
        };
        for r in 0..12 {
            let s = &SIGMA[r % 10];
            g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }
        for i in 0..8 {
            h[i] ^= v[i] ^ v[i + 8];
        }
    }
    h.iter().flat_map(|w| w.to_le_bytes()).take(p[0] as usize).collect()
}

/// BLAKE2s with a digest of `out` bytes (1 to 32).
pub fn blake2s(msg: &[u8], out: usize) -> Vec<u8> {
    let mut p = [0u8; 32];
    p[0] = out as u8;
    p[2] = 1;
    p[3] = 1;
    blake2s_param(msg, &p)
}

/// BLAKE2b with a digest of `out` bytes (1 to 64).
pub fn blake2b(msg: &[u8], out: usize) -> Vec<u8> {
    let mut p = [0u8; 64];
    p[0] = out as u8;
    p[2] = 1;
    p[3] = 1;
    blake2b_param(msg, &p)
}

/// BLAKE2Xs with `out` bytes of output (1 to 65535).
pub fn blake2xs(msg: &[u8], out: usize) -> Vec<u8> {
    let xof = (out as u16).to_le_bytes();
    let mut p = [0u8; 32];
    p[0] = 32;
    p[2] = 1;
    p[3] = 1;
    p[12..14].copy_from_slice(&xof);
    let root = blake2s_param(msg, &p);
    let mut res = Vec::with_capacity(out);
    let mut i = 0u32;
    while res.len() < out {
        let mut q = [0u8; 32];
        q[0] = (out - res.len()).min(32) as u8;
        q[4..8].copy_from_slice(&32u32.to_le_bytes());
        q[8..12].copy_from_slice(&i.to_le_bytes());
        q[12..14].copy_from_slice(&xof);
        q[15] = 32;
        res.extend(blake2s_param(&root, &q));
        i += 1;
    }
    res
}

/// BLAKE2Xb with `out` bytes of output (1 to 2^32 - 2).
pub fn blake2xb(msg: &[u8], out: usize) -> Vec<u8> {
    let xof = (out as u32).to_le_bytes();
    let mut p = [0u8; 64];
    p[0] = 64;
    p[2] = 1;
    p[3] = 1;
    p[12..16].copy_from_slice(&xof);
    let root = blake2b_param(msg, &p);
    let mut res = Vec::with_capacity(out);
    let mut i = 0u32;
    while res.len() < out {
        let mut q = [0u8; 64];
        q[0] = (out - res.len()).min(64) as u8;
        q[4..8].copy_from_slice(&64u32.to_le_bytes());
        q[8..12].copy_from_slice(&i.to_le_bytes());
        q[12..16].copy_from_slice(&xof);
        q[17] = 64;
        res.extend(blake2b_param(&root, &q));
        i += 1;
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn md2_md4_ripemd() {
        assert_eq!(hex(&md2(b"")), "8350e5a3e24c153df2275c9f80692773");
        assert_eq!(hex(&md2(b"abc")), "da853b0d3f88d99b30283a69e6ded6bb");
        assert_eq!(hex(&md2(b"abcdefghijklmnopqrstuvwxyz")), "4e8ddff3650292ab5a4108c3aa47940b");
        let mut seen = [false; 256];
        for &s in &MD2_S {
            seen[s as usize] = true;
        }
        assert!(seen.iter().all(|&b| b), "the substitution is a permutation");
        assert_eq!(hex(&md4(b"")), "31d6cfe0d16ae931b73c59d7e0c089c0");
        assert_eq!(hex(&md4(b"abc")), "a448017aaf21d8525fc10ae87aa6729d");
        assert_eq!(hex(&md4(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890")), "e33b4ddc9c38f2199c3e7b164fcc0536");
        assert_eq!(hex(&ripemd160(b"")), "9c1185a5c5e9fc54612808977ee8f548b2258d31");
        assert_eq!(hex(&ripemd160(b"abc")), "8eb208f7e05d987a9b044a8e98c6b087f15a0bfc");
        assert_eq!(hex(&ripemd160(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")), "12a053384a9c0c88e405a06c27dcf49ada62eb2b");
    }

    #[test]
    fn sha3_keccak() {
        assert_eq!(hex(&keccak(b"", 32, 0x06)), "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a");
        assert_eq!(hex(&keccak(b"abc", 32, 0x06)), "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532");
        assert_eq!(
            hex(&keccak(b"abc", 64, 0x06)),
            "b751850b1a57168a5693cd924b6b096e08f621827444f70d884f5d0240d2712e10e116e9192af3c91a7ec57647e3934057340b4cf408d5a56592f8274eec53f0"
        );
        assert_eq!(hex(&keccak(b"", 32, 0x01)), "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
        // a message longer than the rate (136 bytes for 256-bit outputs)
        let long = vec![b'a'; 200];
        assert_eq!(keccak(&long, 32, 0x06).len(), 32);
    }

    #[test]
    fn blake2() {
        assert_eq!(
            hex(&blake2b(b"abc", 64)),
            "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
        );
        assert_eq!(hex(&blake2s(b"abc", 32)), "508c5e8c327c14e2e1a72ba34eeb452f37458b209ed63a294d999b4c86675982");
        assert_eq!(hex(&blake2b(b"", 64))[..16], *"786a02f742015903");
        // a different length is a different hash, not a prefix
        assert_ne!(hex(&blake2b(b"abc", 32)), hex(&blake2b(b"abc", 64))[..64]);
        // more than one block
        assert_eq!(blake2s(&[7u8; 200], 32).len(), 32);
    }
}
