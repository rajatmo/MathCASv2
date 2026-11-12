//! Number text for the printer of `mc-syntax`, from `mc-arith`: big integers by its divide-and-conquer decimal
//! conversion, balls with only their correct digits (decision B.11, 04-syntax section 4).
use mc_arith::num::Nat;
use mc_syntax::{DefaultNumberFormat, NumberFormat};

/// The [`NumberFormat`] of the engine.
pub struct ArithNumbers;

impl NumberFormat for ArithNumbers {
    fn big_int(&self, mag: &[u64]) -> String {
        Nat::from_limbs(mag.to_vec()).to_decimal()
    }

    fn ball(&self, neg: bool, words: &[u64]) -> String {
        // a payload that is not a ball cannot come from the store; print it raw rather than fail
        mc_arith::text::ball_words(neg, words).unwrap_or_else(|| DefaultNumberFormat.ball(neg, words))
    }
}
