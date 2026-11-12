//! Jacobi functions at shifts by the quarter periods (round 33): `jacobi_pq(x + a K(m) + b i
//! K(1 - m), m)` for integers a, b (and a half-integer a at x = 0) is a constant times another Jacobi function of x
//! (DLMF 22.4.3, Table 22.4.3; DLMF 22.4.1-2 for the periods), so the exact value or the reduction to x is returned
//! instead of a call that no rule matched (`jacobi_cd(x + 4 K(m), m)` is `jacobi_cd(x, m)`, `jacobi_ns(K(m), m)` is 1).
//!
//! Method: the four functions s = sn, c = cn, d = dn and n = 1 form a vector v(x) whose shifts are the signed
//! permutations (DLMF 22.4.3, multiplied through by the common factor)
//!   K:    v(x + K)   = (c, -k' s, k', d)      up to the common factor 1/d(x),
//!   iK':  v(x + iK') = (1, -i d, -i k c, k s) up to the common factor 1/(k s(x)),
//! with k = sqrt(m), k' = sqrt(1 - m), so that every pq = v_p/v_q at the shifted argument is a monomial in k, k', i
//! times the ratio of two entries of v(x): composing the maps a and b times (a, b mod 4) gives it without a table
//! of 12 x 16 formulas. At x = 0, v = (0, 1, 1, 1), and K/2 has v = (1/sqrt(1 + k'), sqrt(k')/sqrt(1 + k'), sqrt(k'),
//! 1) (DLMF 22.5.?: the half-period values from the duplication formulas).
use mc_api::{names, Store, View, T};
use mc_arith::num::Rat;
use mc_arith::{Arith, Num, Real};
use mc_core::Heap;

/// A monomial sign * i^ipow * k^kp2/2-style constant: k^ke k'^kpe with k = m^(1/2), k' = (1 - m)^(1/2).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Mono {
    sign: i32,
    ipow: i32,
    ke: i32,
    kpe: i32,
}

impl Mono {
    const ONE: Mono = Mono { sign: 1, ipow: 0, ke: 0, kpe: 0 };
    fn mul(self, o: Mono) -> Mono {
        Mono { sign: self.sign * o.sign, ipow: (self.ipow + o.ipow).rem_euclid(4), ke: self.ke + o.ke, kpe: self.kpe + o.kpe }
    }
    fn recip(self) -> Mono {
        Mono { sign: self.sign, ipow: (-self.ipow).rem_euclid(4), ke: -self.ke, kpe: -self.kpe }
    }
}

const K_SHIFT: [(Mono, usize); 4] = [
    (Mono::ONE, 1),
    (Mono { sign: -1, ipow: 0, ke: 0, kpe: 1 }, 0),
    (Mono { sign: 1, ipow: 0, ke: 0, kpe: 1 }, 3),
    (Mono::ONE, 2),
];
const I_SHIFT: [(Mono, usize); 4] = [
    (Mono::ONE, 3),
    (Mono { sign: 1, ipow: 3, ke: 0, kpe: 0 }, 2),
    (Mono { sign: 1, ipow: 3, ke: 1, kpe: 0 }, 1),
    (Mono { sign: 1, ipow: 0, ke: 1, kpe: 0 }, 0),
];

fn apply(state: [(Mono, usize); 4], map: &[(Mono, usize); 4]) -> [(Mono, usize); 4] {
    let mut out = state;
    for i in 0..4 {
        let (mono, old) = map[i];
        out[i] = (mono.mul(state[old].0), state[old].1);
    }
    out
}

/// The exact value of a coefficient as a rational pair (num, den) of an exact real.
fn real_coeff(n: &Num) -> Option<Rat> {
    match n {
        Num::Real(Real::Exact(q)) => Some(q.clone()),
        _ => None,
    }
}

/// k = a/b with b | 2: twice the value as an integer.
fn twice(q: &Rat) -> Option<i64> {
    let two = q.mul(&Rat::from_i64(2));
    two.is_int().then(|| two.num.to_i64()).flatten()
}

/// x, a, b with u = x + a K(m) + b i K(1 - m): a, b as twice their values (a2 is odd for a half-integer).
fn decompose<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, u: T<'h>, m: T<'h>) -> Option<(T<'h>, i64, i64)> {
    let one = ar.int(1);
    let mp = ar.sub(one, m);
    let ek = h.sym_term(h.sym("elliptic_k"));
    let terms: Vec<T<'h>> = match h.view(u) {
        View::App { head, args } if head == h.sym_term(h.sym(names::ADD)) => args.to_vec(),
        _ => vec![u],
    };
    let (mut a, mut b) = (Rat::from_i64(0), Rat::from_i64(0));
    let mut rest = vec![];
    for &t in &terms {
        let mut coeff: Vec<T<'h>> = vec![];
        let mut ellip: Option<T<'h>> = None;
        let parts: Vec<T<'h>> = match h.view(t) {
            View::App { head, args } if head == h.sym_term(h.sym(names::MUL)) => args.to_vec(),
            _ => vec![t],
        };
        for &p in &parts {
            match h.view(p) {
                View::App { head, args } if head == ek && args.len() == 1 && ellip.is_none() => ellip = Some(args[0]),
                _ => coeff.push(p),
            }
        }
        let numeric = coeff.iter().all(|&c| ar.num(c).is_some());
        match ellip {
            Some(arg) if numeric && (arg == m || arg == mp) => {
                let c = ar.num(ar.mul(&coeff))?;
                if arg == m {
                    a = a.add(&real_coeff(&c)?);
                } else {
                    // purely imaginary coefficient: i c K'
                    match c {
                        Num::Complex(re, im) if matches!(&re, Real::Exact(q) if q.is_zero()) => b = b.add(&real_coeff(&Num::Real(im))?),
                        _ => return None,
                    }
                }
            }
            _ => rest.push(t),
        }
    }
    let x = if rest.is_empty() { ar.int(0) } else { ar.add(&rest) };
    Some((x, twice(&a)?, twice(&b).filter(|v| v % 2 == 0)?))
}

fn letter(i: usize) -> char {
    ['s', 'c', 'd', 'n'][i]
}

fn slot(c: char) -> usize {
    match c {
        's' => 0,
        'c' => 1,
        'd' => 2,
        _ => 3,
    }
}

/// The name of the function p/q.
fn name_of(p: char, q: char) -> String {
    match (p, q) {
        ('s', 'n') => "jacobi_sn".into(),
        ('c', 'n') => "jacobi_cn".into(),
        ('d', 'n') => "jacobi_dn".into(),
        _ => format!("jacobi_{p}{q}"),
    }
}

fn mono_term<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, mono: Mono, m: T<'h>) -> T<'h> {
    let mut f: Vec<T<'h>> = vec![];
    if mono.sign < 0 {
        f.push(ar.int(-1));
    }
    match mono.ipow {
        1 => f.push(ar.complex(h.int(0), h.int(1))),
        2 => f.push(ar.int(-1)),
        3 => f.push(ar.complex(h.int(0), h.int(-1))),
        _ => {}
    }
    if mono.ke != 0 {
        f.push(ar.pow(m, ar.rational(h.int(mono.ke as i64), h.int(2))));
    }
    if mono.kpe != 0 {
        f.push(ar.pow(ar.sub(ar.int(1), m), ar.rational(h.int(mono.kpe as i64), h.int(2))));
    }
    ar.mul(&f)
}

/// p/q at u when u = x + a K + b i K' (a half-integer allowed at x = 0); `None` when u has no such shift (or none
/// at all).
pub(crate) fn reduce<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, p: char, q: char, u: T<'h>, m: T<'h>) -> Option<T<'h>> {
    let (x, a2, b2) = decompose(h, ar, u, m)?;
    if a2 == 0 && b2 == 0 {
        return None;
    }
    let x_zero = matches!(h.view(x), View::Int(0));
    let half = a2.rem_euclid(2) == 1;
    if half && !x_zero {
        return None;
    }
    let a = a2.div_euclid(2).rem_euclid(4);
    let b = (b2 / 2).rem_euclid(4);
    let mut state = [(Mono::ONE, 0usize), (Mono::ONE, 1), (Mono::ONE, 2), (Mono::ONE, 3)];
    for _ in 0..a {
        state = apply(state, &K_SHIFT);
    }
    for _ in 0..b {
        state = apply(state, &I_SHIFT);
    }
    let (ip, iq) = (slot(p), slot(q));
    let (mp, mq) = (state[ip], state[iq]);
    let c = mp.0.mul(mq.0.recip());
    if x_zero {
        // base vector: (0, 1, 1, 1), or the half-period one
        let one = ar.int(1);
        let kp = ar.pow(ar.sub(one, m), ar.rational(h.int(1), h.int(2)));
        let base: [T<'h>; 4] = if half {
            let s = ar.pow(ar.add(&[one, kp]), ar.rational(h.int(-1), h.int(2)));
            let d = ar.pow(ar.sub(one, m), ar.rational(h.int(1), h.int(4)));
            [s, ar.mul(&[d, s]), d, one]
        } else {
            [ar.int(0), one, one, one]
        };
        let (np, dq) = (base[mp.1], base[mq.1]);
        if matches!(h.view(dq), View::Int(0)) {
            return if matches!(h.view(np), View::Int(0)) { None } else { Some(h.sym_term(h.sym("complex_infinity"))) };
        }
        let r = ar.div(np, dq);
        return Some(ar.mul(&[mono_term(h, ar, c, m), r]));
    }
    // a general x: the constant times the function of x for the entries (old_p, old_q)
    let (lp, lq) = (letter(mp.1), letter(mq.1));
    let f = if mp.1 == mq.1 { ar.int(1) } else { h.call(h.sym(&name_of(lp, lq)), &[x, m]) };
    Some(ar.mul(&[mono_term(h, ar, c, m), f]))
}
