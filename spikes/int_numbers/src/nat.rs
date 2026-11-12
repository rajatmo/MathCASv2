//! Natural numbers as little-endian 64-bit limbs, normalised (no high zero limbs; zero is the empty vector).
//!
//! Multiplication picks schoolbook, Karatsuba or the three-prime NTT by operand length (thresholds measured by the
//! driver's crossover sweep). Division is schoolbook (Knuth D) for short quotients or divisors and Newton
//! reciprocal iteration otherwise, so that division, square root and radix conversion cost a constant number of
//! multiplications and inherit the NTT's speed.
use std::cmp::Ordering;
use std::sync::atomic::{AtomicUsize, Ordering as AO};

/// Karatsuba below this many limbs (of the shorter operand) falls back to schoolbook.
pub static KARA_THRESH: AtomicUsize = AtomicUsize::new(24);
/// The NTT is used when the shorter operand has at least this many limbs.
pub static NTT_THRESH: AtomicUsize = AtomicUsize::new(1500);
/// Newton division when both the divisor and the quotient have at least this many limbs.
pub static NEWTON_THRESH: AtomicUsize = AtomicUsize::new(150);

/// Multiplication thresholds for one top-level call, read once so a sweep can force an algorithm.
#[derive(Clone, Copy, Debug)]
pub struct Algo {
    pub kara: usize,
    pub ntt: usize,
}

impl Algo {
    pub fn auto() -> Algo {
        Algo { kara: KARA_THRESH.load(AO::Relaxed), ntt: NTT_THRESH.load(AO::Relaxed) }
    }
    pub fn school() -> Algo {
        Algo { kara: usize::MAX, ntt: usize::MAX }
    }
    pub fn kara_only() -> Algo {
        Algo { kara: KARA_THRESH.load(AO::Relaxed), ntt: usize::MAX }
    }
    pub fn ntt_only() -> Algo {
        Algo { kara: KARA_THRESH.load(AO::Relaxed), ntt: 1 }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default, Hash)]
pub struct Nat {
    pub limbs: Vec<u64>,
}

fn trim(v: &mut Vec<u64>) {
    while v.last() == Some(&0) {
        v.pop();
    }
}

fn trimmed(v: &[u64]) -> &[u64] {
    let mut n = v.len();
    while n > 0 && v[n - 1] == 0 {
        n -= 1;
    }
    &v[..n]
}

// ---------------------------------------------------------------- slice primitives

/// acc += x (acc.len() >= x.len()), carrying through acc; returns the carry out of acc.
pub fn add_into(acc: &mut [u64], x: &[u64]) -> u64 {
    let mut carry = 0u64;
    for (i, &xi) in x.iter().enumerate() {
        let (s1, c1) = acc[i].overflowing_add(xi);
        let (s2, c2) = s1.overflowing_add(carry);
        acc[i] = s2;
        carry = (c1 | c2) as u64;
    }
    let mut i = x.len();
    while carry != 0 && i < acc.len() {
        let (s, c) = acc[i].overflowing_add(carry);
        acc[i] = s;
        carry = c as u64;
        i += 1;
    }
    carry
}

/// acc -= x (acc.len() >= x.len()); returns the borrow out of acc.
pub fn sub_into(acc: &mut [u64], x: &[u64]) -> u64 {
    let mut borrow = 0u64;
    for (i, &xi) in x.iter().enumerate() {
        let (s1, b1) = acc[i].overflowing_sub(xi);
        let (s2, b2) = s1.overflowing_sub(borrow);
        acc[i] = s2;
        borrow = (b1 | b2) as u64;
    }
    let mut i = x.len();
    while borrow != 0 && i < acc.len() {
        let (s, b) = acc[i].overflowing_sub(borrow);
        acc[i] = s;
        borrow = b as u64;
        i += 1;
    }
    borrow
}

/// out[..a.len()] += a * m; returns the carry limb.
#[inline]
pub fn addmul_1(out: &mut [u64], a: &[u64], m: u64) -> u64 {
    let mut carry = 0u64;
    for (o, &x) in out.iter_mut().zip(a) {
        let t = x as u128 * m as u128 + *o as u128 + carry as u128;
        *o = t as u64;
        carry = (t >> 64) as u64;
    }
    carry
}

/// out = a * b by schoolbook; out has a.len() + b.len() limbs and must be zero on entry.
pub fn mul_school(a: &[u64], b: &[u64], out: &mut [u64]) {
    let la = a.len();
    for (j, &bj) in b.iter().enumerate() {
        // bounded by the input; the long products that could take seconds go through Karatsuba or the NTT
        out[j + la] = addmul_1(&mut out[j..j + la], a, bj);
    }
}

pub fn add_vec(a: &[u64], b: &[u64]) -> Vec<u64> {
    let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut r = Vec::with_capacity(a.len() + 1);
    r.extend_from_slice(a);
    r.push(0);
    add_into(&mut r, b);
    r
}

/// |x - y| into out (x.len() <= y.len() = out.len()); returns true if x < y.
fn abs_diff(x: &[u64], y: &[u64], out: &mut [u64]) -> bool {
    let n = y.len();
    // compare x (zero-extended) with y from the top
    let mut less = false;
    for i in (0..n).rev() {
        let xi = if i < x.len() { x[i] } else { 0 };
        if xi != y[i] {
            less = xi < y[i];
            break;
        }
    }
    let (big, small, big_is_y) = if less { (y, x, true) } else { (x, y, false) };
    out[..big.len()].copy_from_slice(big);
    out[big.len()..].fill(0);
    sub_into(out, small);
    big_is_y
}

/// Scratch limbs needed by `kara_eq` for operands of n limbs: a level uses 6 ceil(n/2) + 1 and recurses on
/// ceil(n/2) limbs, a geometric sum below 6n + 4 log2(n) + 8.
fn kara_scratch(n: usize) -> usize {
    8 * n + 64
}

/// out (2n limbs) = a b for a.len() = b.len() = n, by subtractive Karatsuba with all temporaries in `scratch`:
/// a b = z0 + (z0 + z2 - (a0 - a1)(b0 - b1)) B + z2 B^2, which keeps every recursive product at ceil(n/2) limbs
/// (the additive form's carries would make them ceil(n/2) + 1 and unbalance the recursion).
fn kara_eq(a: &[u64], b: &[u64], out: &mut [u64], scratch: &mut [u64], kara: usize) {
    let n = a.len();
    if n < kara {
        out.fill(0);
        mul_school(a, b, out);
        return;
    }
    let h = n / 2;
    let hl = n - h;
    let (a0, a1) = a.split_at(h);
    let (b0, b1) = b.split_at(h);
    {
        let (z0, z2) = out.split_at_mut(2 * h);
        kara_eq(a0, b0, z0, scratch, kara);
        kara_eq(a1, b1, z2, scratch, kara);
    }
    let (da, rest) = scratch.split_at_mut(hl);
    let (db, rest) = rest.split_at_mut(hl);
    let (zm, rest) = rest.split_at_mut(2 * hl);
    let (t, rest) = rest.split_at_mut(2 * hl + 1);
    let neg_a = abs_diff(a0, a1, da);
    let neg_b = abs_diff(b0, b1, db);
    kara_eq(da, db, zm, rest, kara);
    // t = z0 + z2 -+ zm, which is a0 b1 + a1 b0 >= 0 and fits 2 hl + 1 limbs
    t.fill(0);
    t[..2 * h].copy_from_slice(&out[..2 * h]);
    add_into(t, &out[2 * h..]);
    if neg_a == neg_b {
        sub_into(t, zm);
    } else {
        add_into(t, zm);
    }
    add_into(&mut out[h..], trimmed(t));
}

/// Product of two limb slices with exactly a.len() + b.len() limbs (high limbs may be zero).
pub fn mul_slices(a: &[u64], b: &[u64], al: Algo) -> Vec<u64> {
    let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let len = a.len() + b.len();
    let (ta, tb) = (trimmed(a), trimmed(b));
    if ta.is_empty() || tb.is_empty() {
        return vec![0; len];
    }
    let (a, b) = if ta.len() >= tb.len() { (ta, tb) } else { (tb, ta) };
    let mut r = if b.len() < al.kara {
        let mut r = vec![0u64; a.len() + b.len()];
        mul_school(a, b, &mut r);
        r
    } else if b.len() >= al.ntt {
        crate::ntt::mul(a, b)
    } else if a.len() > b.len() {
        // unbalanced: b times consecutive b-sized pieces of a (the last piece recurses with its own shape)
        let mut r = vec![0u64; a.len() + b.len()];
        let mut i = 0;
        while i < a.len() {
            if crate::stopped() {
                break;
            }
            let end = (i + b.len()).min(a.len());
            let p = mul_slices(&a[i..end], b, al);
            add_into(&mut r[i..], trimmed(&p));
            i = end;
        }
        r
    } else {
        if b.len() > 1024 && crate::stopped() {
            return vec![0; len];
        }
        let mut r = vec![0u64; 2 * a.len()];
        let mut scratch = vec![0u64; kara_scratch(a.len())];
        kara_eq(a, b, &mut r, &mut scratch, al.kara.max(2));
        r
    };
    r.resize(len, 0);
    r
}

/// Square or product with a reused transform when both operands are the same slice.
fn mul_or_square(a: &[u64], b: &[u64], al: Algo) -> Vec<u64> {
    if std::ptr::eq(a, b) && a.len() >= al.ntt {
        return crate::ntt::mul(a, a);
    }
    mul_slices(a, b, al)
}

// ---------------------------------------------------------------- division

/// (q, r) of u / d for a single-limb divisor d > 0.
pub fn divrem_1(u: &[u64], d: u64) -> (Vec<u64>, u64) {
    let mut q = vec![0u64; u.len()];
    let mut rem = 0u128;
    for i in (0..u.len()).rev() {
        let cur = (rem << 64) | u[i] as u128;
        q[i] = (cur / d as u128) as u64;
        rem = cur % d as u128;
    }
    trim(&mut q);
    (q, rem as u64)
}

fn shl_slice(a: &[u64], s: u32, extra: bool) -> Vec<u64> {
    let mut r = Vec::with_capacity(a.len() + 1);
    if s == 0 {
        r.extend_from_slice(a);
        if extra {
            r.push(0);
        }
        return r;
    }
    let mut carry = 0u64;
    for &x in a {
        r.push((x << s) | carry);
        carry = x >> (64 - s);
    }
    if extra || carry != 0 {
        r.push(carry);
    }
    r
}

/// Knuth's algorithm D on 64-bit limbs (u >= v, v.len() >= 2, both trimmed); O(len(q) len(v)).
fn divrem_knuth(u: &[u64], v: &[u64]) -> (Vec<u64>, Vec<u64>) {
    let s = v[v.len() - 1].leading_zeros();
    let vn = shl_slice(v, s, false);
    let mut un = shl_slice(u, s, true);
    let n = vn.len();
    let m = un.len() - n - 1;
    let mut q = vec![0u64; m + 1];
    const B: u128 = 1 << 64;
    let (vtop, vnext) = (vn[n - 1] as u128, vn[n - 2] as u128);
    for j in (0..=m).rev() {
        let num = ((un[j + n] as u128) << 64) | un[j + n - 1] as u128;
        let mut qhat = num / vtop;
        let mut rhat = num % vtop;
        // at most two corrections (Knuth's theorem B)
        while qhat >= B || qhat * vnext > ((rhat << 64) | un[j + n - 2] as u128) {
            qhat -= 1;
            rhat += vtop;
            if rhat >= B {
                break;
            }
        }
        let mut borrow = 0u64;
        let mut carry = 0u64;
        for i in 0..n {
            let p = qhat * vn[i] as u128 + carry as u128;
            carry = (p >> 64) as u64;
            let (t1, b1) = un[i + j].overflowing_sub(p as u64);
            let (t2, b2) = t1.overflowing_sub(borrow);
            un[i + j] = t2;
            borrow = (b1 | b2) as u64;
        }
        let (t1, b1) = un[j + n].overflowing_sub(carry);
        let (t2, b2) = t1.overflowing_sub(borrow);
        un[j + n] = t2;
        if b1 | b2 {
            // qhat was one too large: add v back
            qhat -= 1;
            let c = add_into(&mut un[j..j + n], &vn);
            un[j + n] = un[j + n].wrapping_add(c);
        }
        q[j] = qhat as u64;
    }
    trim(&mut q);
    let mut r = un[..n].to_vec();
    if s > 0 {
        for i in 0..n {
            let hi = if i + 1 < n { r[i + 1] << (64 - s) } else { 0 };
            r[i] = (r[i] >> s) | hi;
        }
    }
    trim(&mut r);
    (q, r)
}

impl Nat {
    pub fn zero() -> Nat {
        Nat { limbs: vec![] }
    }
    pub fn from_u64(v: u64) -> Nat {
        Nat::from_limbs(vec![v])
    }
    pub fn from_u128(v: u128) -> Nat {
        Nat::from_limbs(vec![v as u64, (v >> 64) as u64])
    }
    pub fn from_limbs(mut v: Vec<u64>) -> Nat {
        trim(&mut v);
        Nat { limbs: v }
    }
    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }
    pub fn len(&self) -> usize {
        self.limbs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.limbs.is_empty()
    }
    pub fn bits(&self) -> u64 {
        match self.limbs.last() {
            None => 0,
            Some(&t) => 64 * self.limbs.len() as u64 - t.leading_zeros() as u64,
        }
    }
    pub fn to_u128(&self) -> Option<u128> {
        match self.limbs.len() {
            0 => Some(0),
            1 => Some(self.limbs[0] as u128),
            2 => Some(self.limbs[0] as u128 | (self.limbs[1] as u128) << 64),
            _ => None,
        }
    }
    pub fn bit(&self, i: u64) -> bool {
        let w = (i / 64) as usize;
        w < self.limbs.len() && (self.limbs[w] >> (i % 64)) & 1 == 1
    }
    /// Is any of the lowest n bits set?
    pub fn low_bits_nonzero(&self, n: u64) -> bool {
        let w = (n / 64) as usize;
        if self.limbs[..w.min(self.limbs.len())].iter().any(|&x| x != 0) {
            return true;
        }
        w < self.limbs.len() && !n.is_multiple_of(64) && self.limbs[w] & ((1u64 << (n % 64)) - 1) != 0
    }
    pub fn add(&self, o: &Nat) -> Nat {
        Nat::from_limbs(add_vec(&self.limbs, &o.limbs))
    }
    pub fn add_u64(&self, v: u64) -> Nat {
        self.add(&Nat::from_u64(v))
    }
    /// self - o; panics if o > self.
    pub fn sub(&self, o: &Nat) -> Nat {
        assert!(self.limbs.len() >= o.limbs.len(), "Nat::sub underflow");
        let mut r = self.limbs.clone();
        let b = sub_into(&mut r, &o.limbs);
        assert_eq!(b, 0, "Nat::sub underflow");
        Nat::from_limbs(r)
    }
    pub fn sub_u64(&self, v: u64) -> Nat {
        self.sub(&Nat::from_u64(v))
    }
    pub fn mul(&self, o: &Nat) -> Nat {
        self.mul_with(o, Algo::auto())
    }
    pub fn mul_with(&self, o: &Nat, al: Algo) -> Nat {
        Nat::from_limbs(mul_or_square(&self.limbs, &o.limbs, al))
    }
    pub fn square(&self) -> Nat {
        Nat::from_limbs(mul_or_square(&self.limbs, &self.limbs, Algo::auto()))
    }
    pub fn mul_u64(&self, m: u64) -> Nat {
        let mut r = vec![0u64; self.limbs.len() + 1];
        let c = addmul_1(&mut r, &self.limbs, m);
        let n = self.limbs.len();
        r[n] = c;
        Nat::from_limbs(r)
    }
    pub fn shl(&self, n: u64) -> Nat {
        if self.is_zero() {
            return Nat::zero();
        }
        let mut r = vec![0u64; (n / 64) as usize];
        r.extend(shl_slice(&self.limbs, (n % 64) as u32, false));
        Nat::from_limbs(r)
    }
    /// floor(self / 2^n)
    pub fn shr(&self, n: u64) -> Nat {
        let w = (n / 64) as usize;
        if w >= self.limbs.len() {
            return Nat::zero();
        }
        let s = (n % 64) as u32;
        let src = &self.limbs[w..];
        let mut r = Vec::with_capacity(src.len());
        if s == 0 {
            r.extend_from_slice(src);
        } else {
            for i in 0..src.len() {
                let hi = if i + 1 < src.len() { src[i + 1] << (64 - s) } else { 0 };
                r.push((src[i] >> s) | hi);
            }
        }
        Nat::from_limbs(r)
    }
    /// self mod 2^n
    pub fn low_bits(&self, n: u64) -> Nat {
        let w = (n / 64) as usize;
        if w >= self.limbs.len() {
            return self.clone();
        }
        let mut r = self.limbs[..=w].to_vec();
        let s = n % 64;
        r[w] &= if s == 0 { 0 } else { (1u64 << s) - 1 };
        Nat::from_limbs(r)
    }
    pub fn pow2(n: u64) -> Nat {
        Nat::from_u64(1).shl(n)
    }
    /// base^e by squaring, with the power of two in base split off as a shift (10^n = 5^n 2^n halves the squarings'
    /// size).
    pub fn pow(base: u64, e: u64) -> Nat {
        if base == 0 {
            return if e == 0 { Nat::from_u64(1) } else { Nat::zero() };
        }
        let tz = base.trailing_zeros() as u64;
        let odd = base >> tz;
        let mut r = Nat::from_u64(1);
        let mut i = 64 - e.leading_zeros();
        while i > 0 {
            i -= 1;
            if crate::stopped() {
                break;
            }
            r = r.square();
            if (e >> i) & 1 == 1 {
                r = r.mul_u64(odd);
            }
        }
        r.shl(tz * e)
    }

    pub fn divrem_u64(&self, d: u64) -> (Nat, u64) {
        assert!(d != 0, "division by zero");
        let (q, r) = divrem_1(&self.limbs, d);
        (Nat { limbs: q }, r)
    }

    /// (floor(self / d), self mod d)
    pub fn divrem(&self, d: &Nat) -> (Nat, Nat) {
        assert!(!d.is_zero(), "division by zero");
        if self.cmp(d) == Ordering::Less {
            return (Nat::zero(), self.clone());
        }
        if d.limbs.len() == 1 {
            let (q, r) = self.divrem_u64(d.limbs[0]);
            return (q, Nat::from_u64(r));
        }
        let t = NEWTON_THRESH.load(AO::Relaxed);
        let ql = self.limbs.len() - d.limbs.len() + 1;
        let dl = d.limbs.len();
        if dl > ql + 4 && ql >= 2 {
            return divrem_truncated(self, d, ql);
        }
        if d.limbs.len() < t || ql < t {
            let (q, r) = divrem_knuth(&self.limbs, &d.limbs);
            return (Nat { limbs: q }, Nat { limbs: r });
        }
        divrem_newton(self, d)
    }

    pub fn div(&self, d: &Nat) -> Nat {
        self.divrem(d).0
    }

    /// floor(sqrt(self))
    pub fn isqrt(&self) -> Nat {
        isqrt(self)
    }

    /// Decimal digits, most significant first, by divide and conquer over powers 10^(19 2^k).
    pub fn to_decimal(&self) -> String {
        to_decimal(self)
    }
}

impl PartialOrd for Nat {
    fn partial_cmp(&self, o: &Nat) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Nat {
    fn cmp(&self, o: &Nat) -> Ordering {
        self.limbs.len().cmp(&o.limbs.len()).then_with(|| {
            for i in (0..self.limbs.len()).rev() {
                match self.limbs[i].cmp(&o.limbs[i]) {
                    Ordering::Equal => continue,
                    c => return c,
                }
            }
            Ordering::Equal
        })
    }
}

/// Division with a divisor much longer than the quotient: only the divisor's top limbs matter. Dropping
/// k = 64 (dl - ql - 2) bits from both operands, q0 = floor((a >> k) / (d >> k)) is within one of floor(a / d): the
/// truncations change a / d by less than 2^k / d below and by a factor (1 + 2^(k+1) / d) above, that is by less
/// than one unit of the quotient. One product q0 d gives the remainder and the correction.
fn divrem_truncated(a: &Nat, d: &Nat, ql: usize) -> (Nat, Nat) {
    let k = 64 * (d.limbs.len() - ql - 2) as u64;
    let mut q = a.shr(k).div(&d.shr(k));
    let mut qd = q.mul(d);
    // bounded: the analysis allows one step each way; the cap only guards it
    for _ in 0..4 {
        if qd > *a {
            q = q.sub_u64(1);
            qd = qd.sub(d);
        } else {
            break;
        }
    }
    let mut r = a.sub(&qd);
    for _ in 0..4 {
        if r >= *d {
            q = q.add_u64(1);
            r = r.sub(d);
        } else {
            break;
        }
    }
    debug_assert!(r < *d);
    (q, r)
}

/// floor((2^(2n) - 1) / b) for n = bits(b): a reciprocal with n + 1 bits, exact.
///
/// Recursion on the top half of b gives a reciprocal with about n/2 correct bits; one Newton step
/// v + v (2^(2n) - b v) / 2^(2n) squares the relative error to O(2^-n), so the result is within a small constant of
/// the exact value, and a bounded correction loop makes it exact.
fn recip(b: &Nat) -> Nat {
    let n = b.bits();
    let target = Nat::pow2(2 * n).sub_u64(1);
    if b.limbs.len() < NEWTON_THRESH.load(AO::Relaxed).max(2) {
        // below the threshold divrem itself uses schoolbook (or the one-limb path), never Newton
        return target.divrem(b).0;
    }
    let h = n.div_ceil(2);
    let l = n - h;
    let vh = recip(&b.shr(l));
    // v0 = vh 2^l; its low l bits are zero, so products use vh and a shift
    let bv0 = b.mul(&vh).shl(l);
    let two2n = Nat::pow2(2 * n);
    // e = 2^(2n) - b v0 (signed). The correction v0 e / 2^(2n) = vh e / 2^(2n - l) needs only the top bits of e:
    // dropping k = n - 64 bits changes it by less than vh 2^k / 2^(2n - l) < 2^-63.
    let (neg, e) = if bv0 <= two2n { (false, two2n.sub(&bv0)) } else { (true, bv0.sub(&two2n)) };
    let k = n.saturating_sub(64);
    let delta = vh.mul(&e.shr(k)).shr(2 * n - l - k);
    let v0 = vh.shl(l);
    // v = v0 +- delta, and b v from b v0 with one short product (delta has about l bits)
    let bd = b.mul(&delta);
    let (mut v, mut p) = if !neg {
        (v0.add(&delta), bv0.add(&bd))
    } else if delta < v0 {
        (v0.sub(&delta), bv0.sub(&bd))
    } else {
        (Nat::from_u64(1), b.clone())
    };
    // the error analysis bounds this by about 20 steps; the cap only guards against a mistake in it
    for _ in 0..64 {
        if p > target {
            v = v.sub_u64(1);
            p = p.sub(b);
        } else if target.sub(&p) >= *b {
            v = v.add_u64(1);
            p = p.add(b);
        } else {
            return v;
        }
    }
    Nat { limbs: divrem_knuth(&target.limbs, &b.limbs).0 }
}

/// A divisor with its reciprocal, for repeated divisions by the same number (radix conversion divides by each power
/// of ten many times; recomputing the reciprocal each time would double the conversion's cost).
pub struct Divisor {
    d: Nat,
    /// floor((2^(2n) - 1) / d), or empty when d is short enough for schoolbook division
    v: Option<Nat>,
    n: u64,
}

impl Divisor {
    pub fn new(d: &Nat) -> Divisor {
        assert!(!d.is_zero(), "division by zero");
        let v = (d.limbs.len() >= NEWTON_THRESH.load(AO::Relaxed).max(2)).then(|| recip(d));
        Divisor { d: d.clone(), v, n: d.bits() }
    }

    /// (floor(x / d), x mod d) for x < 2^(2n): q = floor(top(x) v / 2^(2n - s)) with the low s = n - 64 bits of x
    /// dropped, which costs a product of n + 64 by n bits instead of 2n by n. The dropped bits and the reciprocal's
    /// floor make q at most 3 too small and never too large; a bounded loop corrects it.
    fn divrem_2n(&self, x: &Nat) -> (Nat, Nat) {
        let Some(v) = &self.v else { return x.divrem(&self.d) };
        let n = self.n;
        debug_assert!(x.bits() <= 2 * n);
        let s = n.saturating_sub(64);
        let mut q = x.shr(s).mul(v).shr(2 * n - s);
        let mut r = x.sub(&q.mul(&self.d));
        for _ in 0..8 {
            if r < self.d {
                break;
            }
            q = q.add_u64(1);
            r = r.sub(&self.d);
        }
        debug_assert!(r < self.d);
        (q, r)
    }

    /// (floor(a / d), a mod d) for any a: chunks of n bits from the top, each chunk with the running remainder below
    /// 2^(2n).
    pub fn divrem(&self, a: &Nat) -> (Nat, Nat) {
        let n = self.n;
        let ab = a.bits();
        if ab <= 2 * n {
            return self.divrem_2n(a);
        }
        let chunks = ab.div_ceil(n);
        let mut q_limbs = vec![0u64; (chunks * n).div_ceil(64) as usize + 1];
        let mut r = Nat::zero();
        for c in (0..chunks).rev() {
            if crate::stopped() {
                break;
            }
            let piece = a.shr(c * n).low_bits(n);
            let cur = r.shl(n).add(&piece);
            let (qd, rd) = self.divrem_2n(&cur);
            // qd < 2^n, so the digits do not overlap: add each at its bit offset
            let sh = qd.shl((c * n) % 64);
            let w = ((c * n) / 64) as usize;
            add_into(&mut q_limbs[w..], &sh.limbs);
            r = rd;
        }
        (Nat::from_limbs(q_limbs), r)
    }
}

/// Newton division: one reciprocal of d (see `Divisor`).
fn divrem_newton(a: &Nat, d: &Nat) -> (Nat, Nat) {
    Divisor::new(d).divrem(a)
}

/// floor(sqrt(x)): recursion on the top half of the bits gives a seed above the root with about a quarter of the
/// bits correct relative to x; one Newton step from above leaves an O(1) error that a bounded loop removes.
fn isqrt(x: &Nat) -> Nat {
    if let Some(v) = x.to_u128() {
        return Nat::from_u128(isqrt_u128(v));
    }
    let b = x.bits();
    let s = b / 4;
    let rh = isqrt(&x.shr(2 * s));
    // (rh + 1)^2 > x >> 2s, so (rh + 1) 2^s > sqrt(x)
    let r0 = rh.add_u64(1).shl(s);
    let mut r = r0.add(&x.div(&r0)).shr(1);
    for _ in 0..64 {
        let sq = r.square();
        if sq <= *x {
            // r was reached from above, so r + 1 is too large unless the step undershot
            if r.add_u64(1).square() > *x {
                return r;
            }
            r = r.add_u64(1);
        } else {
            // r^2 - x < 2r + 1 (r is within one of the root) needs one step; otherwise take another Newton step
            let over = sq.sub(x);
            if over < r.shl(1).add_u64(1) {
                r = r.sub_u64(1);
            } else {
                r = r.add(&x.div(&r)).shr(1);
            }
        }
    }
    unreachable!("isqrt correction did not converge")
}

pub fn isqrt_u128(v: u128) -> u128 {
    if v == 0 {
        return 0;
    }
    let mut r = (v as f64).sqrt() as u128;
    // f64 has 53 bits; fix the estimate both ways (at most a few steps)
    while r.checked_mul(r).is_none_or(|s| s > v) {
        r -= 1;
    }
    while (r + 1).checked_mul(r + 1).is_some_and(|s| s <= v) {
        r += 1;
    }
    r
}

const TEN19: u64 = 10_000_000_000_000_000_000;

/// Below this many limbs the conversion divides by 10^19 repeatedly (quadratic, no big divisions).
const DEC_BASE_LIMBS: usize = 40;

fn to_decimal(x: &Nat) -> String {
    if x.is_zero() {
        return "0".to_string();
    }
    // pows[k] = 10^(19 2^k), up to the first one above sqrt(x)
    let mut pows = vec![Nat::from_u64(TEN19)];
    while pows.last().unwrap().bits() * 2 <= x.bits() + 1 {
        let next = pows.last().unwrap().square();
        pows.push(next);
    }
    let divs: Vec<Divisor> = pows.iter().map(Divisor::new).collect();
    let mut out = String::with_capacity((x.bits() as f64 * std::f64::consts::LOG10_2) as usize + 20);
    dec_top(x, &pows, &divs, &mut out);
    out
}

/// x without leading zeros.
fn dec_top(x: &Nat, pows: &[Nat], divs: &[Divisor], out: &mut String) {
    if x.limbs.len() <= DEC_BASE_LIMBS {
        let s = dec_small(x);
        out.push_str(s.trim_start_matches('0'));
        if s.trim_start_matches('0').is_empty() {
            out.push('0');
        }
        return;
    }
    // the largest k with pows[k] <= x: x = q 10^(19 2^k) + r with q < 10^(19 2^k) (pows[k+1] > x or close to it)
    let mut k = 0;
    while k + 1 < pows.len() && pows[k + 1] <= *x {
        k += 1;
    }
    let (q, r) = divs[k].divrem(x);
    dec_top(&q, pows, divs, out);
    dec_pad(&r, k, divs, out);
}

/// x < 10^(19 2^k), written with exactly 19 2^k digits.
fn dec_pad(x: &Nat, k: usize, divs: &[Divisor], out: &mut String) {
    let width = 19usize << k;
    if x.limbs.len() <= DEC_BASE_LIMBS || k == 0 {
        let s = dec_small(x);
        for _ in s.len()..width {
            out.push('0');
        }
        out.push_str(&s);
        return;
    }
    if crate::stopped() {
        return;
    }
    let (q, r) = divs[k - 1].divrem(x);
    dec_pad(&q, k - 1, divs, out);
    dec_pad(&r, k - 1, divs, out);
}

/// Quadratic conversion for short numbers (no leading zeros except "0" for zero... the caller pads or trims).
fn dec_small(x: &Nat) -> String {
    let mut parts = vec![];
    let mut cur = x.limbs.clone();
    while !cur.is_empty() {
        let (q, r) = divrem_1(&cur, TEN19);
        parts.push(r);
        cur = q;
    }
    let mut s = String::new();
    match parts.pop() {
        None => s.push('0'),
        Some(top) => s.push_str(&top.to_string()),
    }
    for p in parts.iter().rev() {
        s.push_str(&format!("{p:019}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rng;

    fn rand_nat(rng: &mut Rng, limbs: usize) -> Nat {
        Nat::from_limbs((0..limbs).map(|_| rng.next_u64()).collect())
    }

    fn to_nb(x: &Nat) -> num_bigint::BigUint {
        let mut v = vec![];
        for &l in &x.limbs {
            v.push(l as u32);
            v.push((l >> 32) as u32);
        }
        num_bigint::BigUint::new(v)
    }

    #[test]
    fn mul_algorithms_agree() {
        let mut rng = Rng(1);
        for &(la, lb) in &[(1, 1), (5, 3), (33, 33), (40, 40), (97, 97), (100, 33), (100, 99), (301, 301), (700, 650), (1001, 1001), (2000, 1999), (3000, 900)] {
            let a = rand_nat(&mut rng, la);
            let b = rand_nat(&mut rng, lb);
            let s = a.mul_with(&b, Algo::school());
            assert_eq!(a.mul_with(&b, Algo::kara_only()), s, "{la}x{lb} kara");
            assert_eq!(a.mul_with(&b, Algo::ntt_only()), s, "{la}x{lb} ntt");
            assert_eq!(to_nb(&s), to_nb(&a) * to_nb(&b));
        }
        // all-ones operands: every carry path at its maximum
        for n in [32usize, 33, 77, 500, 1600] {
            let a = Nat::from_limbs(vec![u64::MAX; n]);
            let s = a.mul_with(&a, Algo::school());
            assert_eq!(a.mul_with(&a, Algo::kara_only()), s, "{n} kara");
            assert_eq!(a.mul_with(&a, Algo::ntt_only()), s, "{n} ntt");
            assert_eq!(a.square(), s);
        }
    }

    #[test]
    fn division_matches_reference() {
        let mut rng = Rng(2);
        for &(la, lb) in &[(3, 2), (10, 4), (400, 200), (900, 300), (1000, 999), (2500, 400), (800, 790)] {
            let a = rand_nat(&mut rng, la);
            let b = rand_nat(&mut rng, lb);
            let (q, r) = a.divrem(&b);
            assert!(r < b);
            assert_eq!(q.mul(&b).add(&r), a, "{la}/{lb}");
            let (nq, nr) = (to_nb(&a) / to_nb(&b), to_nb(&a) % to_nb(&b));
            assert_eq!((to_nb(&q), to_nb(&r)), (nq, nr));
        }
        // divisors much longer than the quotient (the truncated path), including a remainder just below d
        for &(ql, dl) in &[(2usize, 40usize), (10, 300), (200, 3000)] {
            let d = rand_nat(&mut rng, dl);
            let q = rand_nat(&mut rng, ql);
            for a in [q.mul(&d), q.mul(&d).add(&d.sub_u64(1)), q.mul(&d).add(&rand_nat(&mut rng, dl / 2))] {
                let (qq, rr) = a.divrem(&d);
                assert!(rr < d);
                assert_eq!(qq.mul(&d).add(&rr), a);
            }
        }
        // divisors with special shapes: powers of two minus one, single high bit
        let a = Nat::pow2(40000).sub_u64(1);
        for b in [Nat::pow2(12345).sub_u64(1), Nat::pow2(20000), Nat::pow2(19999).add_u64(1)] {
            let (q, r) = a.divrem(&b);
            assert!(r < b);
            assert_eq!(q.mul(&b).add(&r), a);
        }
    }

    #[test]
    fn isqrt_is_floor() {
        let mut rng = Rng(3);
        for limbs in [1usize, 2, 3, 10, 100, 600] {
            let x = rand_nat(&mut rng, limbs);
            let r = x.isqrt();
            assert!(r.square() <= x);
            assert!(r.add_u64(1).square() > x);
        }
        // perfect squares and their neighbours
        let y = rand_nat(&mut rng, 300);
        let sq = y.square();
        assert_eq!(sq.isqrt(), y);
        assert_eq!(sq.sub_u64(1).isqrt(), y.sub_u64(1));
    }

    #[test]
    fn decimal_matches_reference() {
        let mut rng = Rng(4);
        for limbs in [0usize, 1, 2, 39, 41, 200, 1500] {
            let x = rand_nat(&mut rng, limbs);
            assert_eq!(x.to_decimal(), to_nb(&x).to_string(), "{limbs}");
        }
        let t = Nat::pow(10, 5000);
        assert_eq!(t.to_decimal(), format!("1{}", "0".repeat(5000)));
        assert_eq!(t.sub_u64(1).to_decimal(), "9".repeat(5000));
    }
}
