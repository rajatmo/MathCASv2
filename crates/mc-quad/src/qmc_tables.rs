//! The generating vector of the lattice sequence and the Sobol parameters, computed by this crate's own searches
//! (decision quad-1.6) and checked in as tables. The searches are the `#[ignore]` tests below; run
//!
//! ```text
//! cargo test -p mc-quad --release --lib qmc_tables::search::lattice -- --ignored --nocapture
//! cargo test -p mc-quad --release --lib qmc_tables::search::sobol -- --ignored --nocapture
//! ```
//!
//! (with `QMC_TABLE_OUT` naming a file, the table goes there) and paste their output over the tables.
//!
//! - **Lattice**: an embedded (extensible) rank-1 lattice sequence in base 2 for up to 2^20 points, built component
//!   by component (Dick, Kuo and Sloan 2013, section 5): each z_j is the best of 256 random odd candidates for the
//!   criterion P_2 with product weights γ_j = (1 + (j - 1)/8)^-2, summed in logarithm over the sizes 2^10 ... 2^20,
//!   so that every power of two in that range is a good lattice.
//! - **Sobol**: the primitive polynomials over GF(2) in order of degree, then of their coefficients; the initial
//!   direction numbers m_1, ..., m_s of each dimension are chosen, as in Joe and Kuo (2008), to minimise the
//!   t-values of the two-dimensional projections with every earlier dimension (here the sum of their squares over
//!   the first 2^1 ... 2^20 points), among all odd m_k < 2^k or 128 random choices of them.

/// The number of dimensions the tables cover.
pub(crate) const DIMS: usize = 64;

/// The generating vector (odd integers below 2^19; the first is 1).
pub(crate) const LATTICE: [u32; DIMS] = [
    1, 440767, 151265, 130403, 131245, 69717, 101369, 45399, 170131, 194477, 253987, 13199, 189341, 496705, 29113,
    479941, 263931, 513891, 390371, 136155, 443097, 180613, 258771, 435731, 406289, 295549, 481743, 223951, 365593,
    313581, 378495, 271611, 363759, 507329, 202375, 331457, 94589, 47831, 95801, 364785, 235889, 133637, 484323,
    170633, 309073, 365251, 23457, 210085, 227091, 15467, 226407, 292565, 449845, 64339, 269621, 119367, 464457,
    433119, 330245, 454135, 346349, 425129, 252895, 294399,
];

/// For dimensions 2, 3, ...: the degree s of the primitive polynomial, its middle coefficients a_1 ... a_{s-1}
/// as bits (a_1 the highest), and the initial direction numbers m_1 ... m_s.
pub(crate) const SOBOL: [(u32, u32, &[u32]); DIMS - 1] = [
    (1, 0, &[1]),
    (2, 1, &[1, 1]),
    (3, 1, &[1, 3, 1]),
    (3, 2, &[1, 1, 5]),
    (4, 1, &[1, 1, 1, 9]),
    (4, 4, &[1, 1, 3, 5]),
    (5, 2, &[1, 1, 1, 9, 17]),
    (5, 4, &[1, 3, 3, 5, 3]),
    (5, 7, &[1, 3, 5, 1, 23]),
    (5, 11, &[1, 3, 5, 1, 31]),
    (5, 13, &[1, 1, 5, 13, 27]),
    (5, 14, &[1, 3, 5, 15, 15]),
    (6, 1, &[1, 1, 1, 7, 7, 39]),
    (6, 13, &[1, 1, 3, 9, 15, 43]),
    (6, 16, &[1, 3, 7, 15, 21, 27]),
    (6, 19, &[1, 3, 7, 3, 11, 1]),
    (6, 22, &[1, 3, 3, 15, 31, 49]),
    (6, 25, &[1, 1, 1, 1, 7, 7]),
    (7, 1, &[1, 1, 7, 9, 27, 41, 55]),
    (7, 4, &[1, 1, 1, 5, 21, 1, 77]),
    (7, 7, &[1, 1, 5, 11, 13, 47, 33]),
    (7, 8, &[1, 1, 7, 5, 1, 53, 67]),
    (7, 14, &[1, 1, 5, 5, 15, 61, 97]),
    (7, 19, &[1, 1, 1, 13, 11, 57, 29]),
    (7, 21, &[1, 3, 1, 15, 13, 11, 107]),
    (7, 28, &[1, 1, 7, 1, 29, 37, 117]),
    (7, 31, &[1, 3, 5, 1, 9, 55, 5]),
    (7, 32, &[1, 3, 7, 1, 19, 39, 7]),
    (7, 37, &[1, 3, 5, 13, 17, 61, 85]),
    (7, 41, &[1, 3, 7, 15, 27, 45, 25]),
    (7, 42, &[1, 3, 1, 3, 1, 61, 65]),
    (7, 50, &[1, 3, 3, 13, 23, 1, 75]),
    (7, 55, &[1, 1, 5, 13, 11, 43, 123]),
    (7, 56, &[1, 3, 3, 1, 17, 31, 73]),
    (7, 59, &[1, 1, 3, 9, 21, 35, 55]),
    (7, 62, &[1, 1, 7, 11, 9, 25, 127]),
    (8, 14, &[1, 3, 5, 13, 29, 23, 15, 95]),
    (8, 21, &[1, 3, 3, 15, 13, 47, 61, 69]),
    (8, 22, &[1, 1, 7, 13, 21, 25, 31, 231]),
    (8, 38, &[1, 3, 7, 3, 17, 47, 61, 31]),
    (8, 47, &[1, 3, 1, 13, 27, 1, 17, 57]),
    (8, 49, &[1, 1, 1, 13, 5, 19, 119, 253]),
    (8, 50, &[1, 3, 3, 7, 17, 13, 103, 97]),
    (8, 52, &[1, 1, 3, 3, 9, 1, 85, 115]),
    (8, 56, &[1, 1, 5, 1, 1, 37, 41, 211]),
    (8, 67, &[1, 3, 7, 11, 15, 45, 89, 31]),
    (8, 70, &[1, 3, 5, 13, 27, 25, 23, 193]),
    (8, 84, &[1, 1, 5, 15, 23, 55, 61, 153]),
    (8, 97, &[1, 1, 7, 15, 27, 15, 89, 163]),
    (8, 103, &[1, 3, 5, 7, 13, 19, 57, 245]),
    (8, 115, &[1, 3, 1, 5, 9, 23, 41, 99]),
    (8, 122, &[1, 1, 5, 1, 13, 55, 119, 29]),
    (9, 8, &[1, 1, 1, 5, 25, 21, 29, 141, 77]),
    (9, 13, &[1, 3, 3, 9, 29, 29, 87, 155, 151]),
    (9, 16, &[1, 1, 7, 7, 31, 51, 119, 13, 489]),
    (9, 22, &[1, 3, 1, 11, 9, 45, 89, 23, 33]),
    (9, 25, &[1, 3, 1, 13, 17, 41, 115, 111, 257]),
    (9, 44, &[1, 1, 3, 9, 3, 25, 101, 79, 357]),
    (9, 47, &[1, 1, 7, 11, 5, 13, 75, 195, 65]),
    (9, 52, &[1, 3, 7, 13, 1, 61, 111, 175, 407]),
    (9, 55, &[1, 1, 3, 5, 31, 15, 105, 49, 23]),
    (9, 59, &[1, 1, 1, 3, 15, 51, 117, 201, 11]),
    (9, 62, &[1, 3, 3, 5, 21, 57, 113, 223, 323]),
];

#[cfg(test)]
mod search {
    use mc_random::Stream;
    use rayon::prelude::*;

    const M: u32 = 20;
    const N: usize = 1 << M;

    /// The table's source: to the file `QMC_TABLE_OUT` names, else to standard output.
    fn emit(text: &str) {
        match std::env::var_os("QMC_TABLE_OUT") {
            Some(p) => std::fs::write(p, text).expect("table file"),
            None => println!("{text}"),
        }
    }

    fn weight(j: usize) -> f64 {
        1.0 / (1.0 + (j as f64 - 1.0) / 8.0).powi(2)
    }

    #[test]
    #[ignore]
    fn lattice() {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(3).build().unwrap();
        // ω(x) = 2 π^2 B2(x), B2(x) = x^2 - x + 1/6
        let pi2 = std::f64::consts::PI * std::f64::consts::PI;
        let omega: Vec<f64> = (0..N)
            .map(|i| {
                let x = i as f64 / N as f64;
                2.0 * pi2 * (x * x - x + 1.0 / 6.0)
            })
            .collect();
        let mut p = vec![1.0f64; N];
        let mut z = vec![1u32];
        let mut rng = Stream::from_key(0x1A77_1CE5);
        // the first component
        let g = weight(1);
        for k in 0..N {
            p[k] *= 1.0 + g * omega[k];
        }
        for j in 2..=super::DIMS {
            let g = weight(j);
            let cands: Vec<u32> = (0..256).map(|_| ((rng.next_u64() % (N as u64 / 4)) * 2 + 1) as u32).collect();
            let crit = |c: u32| -> f64 {
                let mut t = [0.0f64; 21];
                for k in 0..N {
                    let v = p[k] * (1.0 + g * omega[(k as u64 * c as u64) as usize & (N - 1)]);
                    let tz = if k == 0 { 20 } else { (k.trailing_zeros()).min(20) as usize };
                    t[tz] += v;
                }
                let mut score = 0.0;
                for m in 10..=M {
                    let s: f64 = t[(M - m) as usize..].iter().sum();
                    let pm = s / (1u64 << m) as f64 - 1.0;
                    score += pm.max(1e-300).ln();
                }
                score
            };
            let scores: Vec<f64> = pool.install(|| cands.par_iter().map(|&c| crit(c)).collect());
            let best = (0..cands.len()).min_by(|&a, &b| scores[a].total_cmp(&scores[b])).unwrap();
            let c = cands[best];
            for k in 0..N {
                p[k] *= 1.0 + g * omega[(k as u64 * c as u64) as usize & (N - 1)];
            }
            z.push(c);
            eprintln!("lattice dim {j}: {c}");
        }
        emit(&format!("pub(crate) const LATTICE: [u32; DIMS] = {z:?};"));
    }

    /// The primitive polynomials of degree s (as bits a_1 ... a_{s-1}), in increasing order.
    fn primitive(s: u32) -> Vec<u32> {
        let order = (1u64 << s) - 1;
        let mut primes = vec![];
        let mut n = order;
        let mut q = 2;
        while q * q <= n {
            if n.is_multiple_of(q) {
                primes.push(q);
                while n.is_multiple_of(q) {
                    n /= q;
                }
            }
            q += 1;
        }
        if n > 1 {
            primes.push(n);
        }
        let poly = |a: u32| -> u64 { (1u64 << s) | ((a as u64) << 1) | 1 };
        // x^e mod p
        let powmod = |p: u64, mut e: u64| -> u64 {
            let mulmod = |a: u64, b: u64| -> u64 {
                let mut r = 0u64;
                let mut a = a;
                let mut b = b;
                while b != 0 {
                    if b & 1 == 1 {
                        r ^= a;
                    }
                    b >>= 1;
                    a <<= 1;
                    if a >> s & 1 == 1 {
                        a ^= p;
                    }
                }
                r
            };
            let mut base = if s == 1 { 1 } else { 2 }; // x mod p (x = 1 mod x + 1)
            let mut r = 1u64;
            while e > 0 {
                if e & 1 == 1 {
                    r = mulmod(r, base);
                }
                base = mulmod(base, base);
                e >>= 1;
            }
            r
        };
        (0..1u32 << (s - 1))
            .filter(|&a| {
                let p = poly(a);
                powmod(p, order) == 1 && primes.iter().all(|&q| powmod(p, order / q) != 1)
            })
            .collect()
    }

    /// The 32 direction numbers of a dimension (as integers whose bit 31 is the first digit).
    fn directions(s: u32, a: u32, m: &[u32]) -> [u32; 32] {
        let mut mm: Vec<u64> = m.iter().map(|&v| v as u64).collect();
        for k in s as usize..32 {
            let mut v = mm[k - s as usize] ^ (mm[k - s as usize] << s);
            for i in 1..s {
                if a >> (s - 1 - i) & 1 == 1 {
                    v ^= mm[k - i as usize] << i;
                }
            }
            mm.push(v);
        }
        let mut out = [0u32; 32];
        for b in 0..32 {
            out[b] = (mm[b] << (31 - b)) as u32;
        }
        out
    }

    /// The t-value of the projection of the first 2^m points onto two dimensions.
    fn t_value(vi: &[u32; 32], vj: &[u32; 32], m: u32) -> u32 {
        // row r of a dimension: the m-bit mask of the columns b < m whose digit r is 1
        let rows = |v: &[u32; 32]| -> Vec<u32> {
            (0..m).map(|r| (0..m).fold(0u32, |acc, b| acc | (((v[b as usize] >> (31 - r)) & 1) << b))).collect()
        };
        let (ri, rj) = (rows(vi), rows(vj));
        let mut kstar = m;
        for a in 0..=m {
            // a basis in echelon form (by leading bit)
            let mut basis = [0u32; 32];
            let insert = |mut v: u32, basis: &mut [u32; 32]| -> bool {
                while v != 0 {
                    let lead = 31 - v.leading_zeros();
                    if basis[lead as usize] == 0 {
                        basis[lead as usize] = v;
                        return true;
                    }
                    v ^= basis[lead as usize];
                }
                false
            };
            for &row in ri.iter().take(a as usize) {
                insert(row, &mut basis);
            }
            let mut b = 0;
            while a + b < m && insert(rj[b as usize], &mut basis) {
                b += 1;
            }
            kstar = kstar.min(a + b);
        }
        m - kstar
    }

    #[test]
    #[ignore]
    fn sobol() {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(3).build().unwrap();
        let mut polys: Vec<(u32, u32)> = vec![];
        let mut s = 1;
        while polys.len() < super::DIMS - 1 {
            for a in primitive(s) {
                polys.push((s, a));
            }
            s += 1;
        }
        polys.truncate(super::DIMS - 1);
        let first: [u32; 32] = std::array::from_fn(|b| 1u32 << (31 - b));
        let mut dirs = vec![first];
        let mut rng = Stream::from_key(0x0005_0B01);
        let mut rows = vec![];
        for &(s, a) in &polys {
            let total: u64 = (1..=s).map(|k| 1u64 << (k - 1)).product();
            let cands: Vec<Vec<u32>> = if total <= 128 {
                (0..total)
                    .map(|mut c| {
                        (1..=s)
                            .map(|k| {
                                let n = 1u64 << (k - 1);
                                let v = (c % n) * 2 + 1;
                                c /= n;
                                v as u32
                            })
                            .collect()
                    })
                    .collect()
            } else {
                (0..128)
                    .map(|_| (1..=s).map(|k| ((rng.next_u64() % (1u64 << (k - 1))) * 2 + 1) as u32).collect())
                    .collect()
            };
            let score = |m: &Vec<u32>| -> u64 {
                let v = directions(s, a, m);
                dirs.iter().map(|d| (1..=M).map(|mm| (t_value(d, &v, mm) as u64).pow(2)).sum::<u64>()).sum()
            };
            let scores: Vec<u64> = pool.install(|| cands.par_iter().map(score).collect());
            let best = (0..cands.len()).min_by_key(|&k| scores[k]).unwrap();
            let m = cands[best].clone();
            dirs.push(directions(s, a, &m));
            eprintln!("sobol dim {}: s {s} a {a} m {m:?} score {}", dirs.len(), scores[best]);
            rows.push(format!("({s}, {a}, &{m:?})"));
        }
        emit(&format!("pub(crate) const SOBOL: [(u32, u32, &[u32]); DIMS - 1] = [\n    {},\n];", rows.join(",\n    ")));
    }

    /// The tables give what the searches' definitions say: odd lattice components, primitive polynomials, odd
    /// initial direction numbers m_k < 2^k.
    #[test]
    fn tables_are_well_formed() {
        assert_eq!(super::LATTICE[0], 1);
        assert!(super::LATTICE.iter().all(|&z| z % 2 == 1 && z < 1 << 19));
        let mut degree = 1;
        let mut seen = vec![];
        for &(s, a, m) in super::SOBOL.iter() {
            assert!(s >= degree);
            degree = s;
            assert!(primitive(s).contains(&a), "{s} {a}");
            assert!(!seen.contains(&(s, a)));
            seen.push((s, a));
            assert_eq!(m.len(), s as usize);
            assert!(m.iter().enumerate().all(|(k, &v)| v % 2 == 1 && v < 1 << (k + 1)));
        }
        // the 2-dimensional projection of dimensions 1 and 2 is a (0, m, 2)-net
        let first: [u32; 32] = std::array::from_fn(|b| 1u32 << (31 - b));
        let (s, a, m) = super::SOBOL[0];
        let v = directions(s, a, m);
        assert!((1..=M).all(|mm| t_value(&first, &v, mm) == 0));
    }
}
