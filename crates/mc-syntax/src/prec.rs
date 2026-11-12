//! Binding powers of 04-syntax.md section 3.1, shared by the parser and the printer. Higher binds tighter.

/// `=`, `:=` (right associative); also the level of a whole statement, argument or list element.
pub const ASSIGN: u8 = 1;
/// `|>`; also the level of lambda bodies and of the open forms `if ... then ... else` and `x => e`.
pub const PIPE: u8 = 2;
/// `->`, `:>` (right associative)
pub const RULE: u8 = 3;
/// `when` (left associative)
pub const WHEN: u8 = 4;
/// `|`
pub const ALT: u8 = 5;
pub const OR: u8 = 6;
pub const AND: u8 = 7;
/// Prefix `not`.
pub const NOT: u8 = 8;
/// Comparisons; they do not chain.
pub const CMP: u8 = 9;
/// `+`, `-`
pub const ADD: u8 = 10;
/// `*`, `/`, juxtaposition
pub const MUL: u8 = 11;
/// Prefix `-`.
pub const NEG: u8 = 12;
/// `^` (right associative)
pub const POW: u8 = 13;
/// Calls, indexing, prime marks, `x @ p`.
pub const POSTFIX: u8 = 14;
/// Atoms: names, literals, brackets.
pub const ATOM: u8 = 15;
