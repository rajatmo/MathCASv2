//! The bit layout of a 32-bit term handle ([`mc_api::T`]).
//!
//! - Bit 31 set: an **immediate**. Bit 30 set: a symbol, its index in the low 30 bits. Bit 30 clear: a small integer,
//!   two's complement in the low 30 bits (decision 1b.3).
//! - Bit 31 clear: a **stored** term. Bit 30 set: young, an index into the young arena. Bit 30 clear: old, an index
//!   into the old region.
//!
//! Young indices of the last young segment are never handed out, so `0x7FFF_FFFE` and `0x7FFF_FFFF` are free for
//! the evaluator's holes (I1); the latter is [`HOLE`].

/// A handle id the store never hands out: the evaluator marks elements a speculative task left unfinished with it.
/// It must never be viewed or passed to the store.
pub const HOLE: u32 = 0x7FFF_FFFF;

pub(crate) const IMM: u32 = 1 << 31;
/// Within immediates: a symbol.
pub(crate) const SYMB: u32 = 1 << 30;
/// Within stored terms: young.
pub(crate) const YOUNG: u32 = 1 << 30;
/// The low 30 bits: symbol index, small integer, or young index.
pub(crate) const LOW: u32 = (1 << 30) - 1;

/// The smallest integer held as an immediate.
pub const SMALL_MIN: i64 = -(1 << 29);
/// The largest integer held as an immediate.
pub const SMALL_MAX: i64 = (1 << 29) - 1;

#[inline]
pub(crate) fn is_imm(b: u32) -> bool {
    b & IMM != 0
}

#[inline]
pub(crate) fn is_young(b: u32) -> bool {
    b & (IMM | YOUNG) == YOUNG
}

#[inline]
pub(crate) fn is_old(b: u32) -> bool {
    b & (IMM | YOUNG) == 0
}

#[inline]
pub(crate) fn is_sym(b: u32) -> bool {
    b & (IMM | SYMB) == IMM | SYMB
}

#[inline]
pub(crate) fn sym_bits(index: u32) -> u32 {
    debug_assert!(index <= LOW);
    IMM | SYMB | index
}

/// The immediate of a small integer; the caller checks the range.
#[inline]
pub(crate) fn small_bits(v: i64) -> u32 {
    debug_assert!((SMALL_MIN..=SMALL_MAX).contains(&v));
    IMM | (v as u32 & LOW)
}

/// The value of a small-integer immediate (sign-extends the low 30 bits).
#[inline]
pub(crate) fn small_value(b: u32) -> i64 {
    (((b << 2) as i32) >> 2) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_integers_round_trip() {
        for v in [0, 1, -1, SMALL_MIN, SMALL_MAX, 12345, -98765] {
            let b = small_bits(v);
            assert!(is_imm(b) && !is_sym(b));
            assert_eq!(small_value(b), v);
        }
        assert!(is_sym(sym_bits(7)) && is_young(YOUNG | 5) && is_old(5));
    }
}
