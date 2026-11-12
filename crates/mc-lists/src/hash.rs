//! `hash(e)`, `hash(e, method)`, `hash(e, method, format)` (round 17): message digests.
//!
//! - **What is hashed**: a string's UTF-8 bytes; any other term the UTF-8 bytes of its `to_string` form (so equal
//!   terms have equal hashes, and the value does not depend on the session). `method: "Expression"` (the default)
//!   is the kernel's structural hash of the term, a 64-bit value stable across collections and sessions of one
//!   build.
//! - **Methods**: `"MD5"` (RFC 1321), `"SHA"` (SHA-1), `"SHA256"`, `"SHA512"`, `"SHA384"`, `"SHA224"` (FIPS 180-4),
//!   `"CRC32"` (ISO 3309 / ITU-T V.42, the polynomial 0xEDB88320 reflected), `"Adler32"` (RFC 1950), also
//!   `["SHA", 256]` and the like. Implemented from those specifications.
//! - **Formats**: `"Integer"` (the default: the digest as a big-endian unsigned integer), `"HexString"` (lower-case
//!   hexadecimal), `"DecimalString"`, `"ByteList"` (the digest's bytes as a list of integers).
use crate::ctx::Lib;
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Reply, Res};

/// A digest method.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Method {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Crc32,
    Adler32,
}

fn method_of(name: &str, bits: Option<i64>) -> Option<Method> {
    Some(match (name.to_ascii_uppercase().as_str(), bits) {
        ("MD5", None) => Method::Md5,
        ("SHA" | "SHA1" | "SHA-1", None) | ("SHA", Some(160)) => Method::Sha1,
        ("SHA224", None) | ("SHA", Some(224)) => Method::Sha224,
        ("SHA256", None) | ("SHA", Some(256)) => Method::Sha256,
        ("SHA384", None) | ("SHA", Some(384)) => Method::Sha384,
        ("SHA512", None) | ("SHA", Some(512)) => Method::Sha512,
        ("CRC32", None) => Method::Crc32,
        ("ADLER32", None) => Method::Adler32,
        _ => return None,
    })
}

/// MD5 (RFC 1321).
fn md5(msg: &[u8]) -> Vec<u8> {
    let s: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10,
        15, 21,
    ];
    // K[i] = floor(2^32 |sin(i + 1)|)
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32).collect();
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut m = msg.to_vec();
    let bitlen = (msg.len() as u64).wrapping_mul(8);
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    m.extend_from_slice(&bitlen.to_le_bytes());
    for chunk in m.chunks(64) {
        let w: Vec<u32> = chunk.chunks(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(w[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(s[i]));
        }
        h = [h[0].wrapping_add(a), h[1].wrapping_add(b), h[2].wrapping_add(c), h[3].wrapping_add(d)];
    }
    h.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// The message padded for the SHA family: a 1 bit, zeros, the bit length in `lenbytes` big-endian bytes.
fn sha_pad(msg: &[u8], block: usize, lenbytes: usize) -> Vec<u8> {
    let mut m = msg.to_vec();
    m.push(0x80);
    while m.len() % block != block - lenbytes {
        m.push(0);
    }
    let bits = (msg.len() as u128).wrapping_mul(8);
    m.extend_from_slice(&bits.to_be_bytes()[16 - lenbytes..]);
    m
}

/// SHA-1 (FIPS 180-4, 6.1).
fn sha1(msg: &[u8]) -> Vec<u8> {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    for chunk in sha_pad(msg, 64, 8).chunks(64) {
        let mut w = [0u32; 80];
        for (i, b) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, &wi) in w.iter().enumerate() {
            let (f, k) = match i / 20 {
                0 => ((b & c) | (!b & d), 0x5A827999),
                1 => (b ^ c ^ d, 0x6ED9EBA1),
                2 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().flat_map(|x| x.to_be_bytes()).collect()
}

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

/// SHA-256 and SHA-224 (FIPS 180-4, 6.2 and 6.3), from the initial hash value `h`, keeping `out` bytes.
fn sha256_with(msg: &[u8], mut h: [u32; 8], out: usize) -> Vec<u8> {
    for chunk in sha_pad(msg, 64, 8).chunks(64) {
        let mut w = [0u32; 64];
        for (i, b) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let [a, b, c, d, e, f, g, hh] = v;
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K256[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g];
        }
        for (x, y) in h.iter_mut().zip(v) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().flat_map(|x| x.to_be_bytes()).take(out).collect()
}

/// The SHA-512 round constants: the first 64 bits of the fractional parts of the cube roots of the first 80 primes.
const K512: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc, 0x3956c25bf348b538,
    0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118, 0xd807aa98a3030242, 0x12835b0145706fbe,
    0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2, 0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235,
    0xc19bf174cf692694, 0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
    0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5, 0x983e5152ee66dfab,
    0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4, 0xc6e00bf33da88fc2, 0xd5a79147930aa725,
    0x06ca6351e003826f, 0x142929670a0e6e70, 0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df, 0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30, 0xd192e819d6ef5218,
    0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8, 0x19a4c116b8d2d0c8, 0x1e376c085141ab53,
    0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8, 0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3, 0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b, 0xca273eceea26619c,
    0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178, 0x06f067aa72176fba, 0x0a637dc5a2c898a6,
    0x113f9804bef90dae, 0x1b710b35131c471b, 0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c, 0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

/// SHA-512 and SHA-384 (FIPS 180-4, 6.4 and 6.5).
fn sha512_with(msg: &[u8], mut h: [u64; 8], out: usize) -> Vec<u8> {
    for chunk in sha_pad(msg, 128, 16).chunks(128) {
        let mut w = [0u64; 80];
        for (i, b) in chunk.chunks(8).enumerate() {
            w[i] = u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..80 {
            let [a, b, c, d, e, f, g, hh] = v;
            let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K512[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            v = [t1.wrapping_add(s0.wrapping_add(maj)), a, b, c, d.wrapping_add(t1), e, f, g];
        }
        for (x, y) in h.iter_mut().zip(v) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().flat_map(|x| x.to_be_bytes()).take(out).collect()
}

/// CRC-32 (reflected polynomial 0xEDB88320, initial and final xor all ones).
fn crc32(msg: &[u8]) -> Vec<u8> {
    let mut c: u32 = !0;
    for &b in msg {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB88320 } else { c >> 1 };
        }
    }
    (!c).to_be_bytes().to_vec()
}

/// Adler-32 (RFC 1950).
fn adler32(msg: &[u8]) -> Vec<u8> {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in msg {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    ((b << 16) | a).to_be_bytes().to_vec()
}

/// The digest of `msg` by `m`.
fn digest(m: Method, msg: &[u8]) -> Vec<u8> {
    match m {
        Method::Md5 => md5(msg),
        Method::Sha1 => sha1(msg),
        Method::Sha224 => sha256_with(
            msg,
            [0xc1059ed8, 0x367cd507, 0x3070dd17, 0xf70e5939, 0xffc00b31, 0x68581511, 0x64f98fa7, 0xbefa4fa4],
            28,
        ),
        Method::Sha256 => sha256_with(
            msg,
            [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19],
            32,
        ),
        Method::Sha384 => sha512_with(
            msg,
            [
                0xcbbb9d5dc1059ed8, 0x629a292a367cd507, 0x9159015a3070dd17, 0x152fecd8f70e5939, 0x67332667ffc00b31,
                0x8eb44a8768581511, 0xdb0c2e0d64f98fa7, 0x47b5481dbefa4fa4,
            ],
            48,
        ),
        Method::Sha512 => sha512_with(
            msg,
            [
                0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1, 0x510e527fade682d1,
                0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
            ],
            64,
        ),
        Method::Crc32 => crc32(msg),
        Method::Adler32 => adler32(msg),
    }
}

/// The digest bytes as a decimal string (big-endian unsigned).
fn decimal(bytes: &[u8]) -> String {
    // repeated division of the base-256 digits by 10
    let mut digits: Vec<u8> = bytes.to_vec();
    let mut out = Vec::new();
    while digits.iter().any(|&d| d != 0) {
        let mut rem = 0u32;
        for d in digits.iter_mut() {
            let cur = rem * 256 + *d as u32;
            *d = (cur / 10) as u8;
            rem = cur % 10;
        }
        out.push(b'0' + rem as u8);
    }
    if out.is_empty() {
        out.push(b'0');
    }
    out.reverse();
    // invariant: only ASCII digits were pushed
    String::from_utf8(out).expect("ascii digits")
}

/// `hash(e[, method[, format]])` (see the module documentation).
pub(crate) fn hash<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (e, method, format) = match *args {
        [e] => (e, None, None),
        [e, m] => (e, Some(m), None),
        [e, m, f] => (e, Some(m), Some(f)),
        _ => return Ok(Reply::Decline),
    };
    let str_of = |x: T<'h>| match h.view(x) {
        View::Str(s) => Some(s.to_string()),
        _ => None,
    };
    let m = match method {
        None => None,
        Some(m) => match (str_of(m), cx.as_list(m)) {
            (Some(s), _) if s.eq_ignore_ascii_case("expression") => None,
            (Some(s), _) => match method_of(&s, None) {
                Some(m) => Some(m),
                None => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
            },
            (None, Some(&[n, b])) => match (str_of(n), h.view(b)) {
                (Some(s), View::Int(k)) => match method_of(&s, Some(k)) {
                    Some(m) => Some(m),
                    None => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
                },
                _ => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
            },
            _ => return Ok(Reply::Done(lib.bad_arg(h, t, 2))),
        },
    };
    let bytes: Vec<u8> = match m {
        // the kernel's structural hash of the term
        None => h.structural_hash(e).to_be_bytes().to_vec(),
        Some(m) => {
            let text = match str_of(e) {
                Some(s) => s,
                None => match str_of(cx.eval(h.call(h.sym("to_string"), &[e]))?) {
                    Some(s) => s,
                    None => return Ok(Reply::Decline),
                },
            };
            cx.tick(text.len() as u64 / 64 + 1)?;
            digest(m, text.as_bytes())
        }
    };
    let format = match format.map(str_of) {
        None => "Integer".to_string(),
        Some(Some(f)) => f,
        Some(None) => return Ok(Reply::Done(lib.bad_arg(h, t, 3))),
    };
    Ok(Reply::Done(match format.as_str() {
        "Integer" => {
            // big-endian bytes as little-endian limbs
            let words: Vec<u64> = bytes
                .rchunks(8)
                .map(|c| c.iter().fold(0u64, |w, &b| (w << 8) | b as u64))
                .collect();
            h.big_int(false, &words)
        }
        "HexString" => h.string(&bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()),
        "DecimalString" => h.string(&decimal(&bytes)),
        "ByteList" => {
            let v: Vec<T<'h>> = bytes.iter().map(|&b| h.int(b as i64)).collect();
            cx.list(&v)
        }
        _ => lib.bad_arg(h, t, 3),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn known_digests() {
        let fox = b"The quick brown fox jumps over the lazy dog";
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(&md5(fox)), "9e107d9d372bb6826bd81d3542a419d6");
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(&digest(Method::Sha256, fox)), "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592");
        assert_eq!(
            hex(&digest(Method::Sha512, b"abc")),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
        assert_eq!(hex(&digest(Method::Sha224, b"abc")), "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7");
        assert_eq!(
            hex(&digest(Method::Sha384, b"abc")),
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        );
        assert_eq!(hex(&crc32(fox)), "414fa339");
        assert_eq!(hex(&adler32(b"Wikipedia")), "11e60398");
        assert_eq!(decimal(&[0x01, 0x00]), "256");
        assert_eq!(decimal(&[]), "0");
    }
}
