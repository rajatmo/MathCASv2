# mc-strings

The string library: natives of the evaluator for splitting, replacing, slicing, searching, changing case, taking
strings apart into characters and back, padding, trimming, string patterns and reading a string as source.

## Modules
- `search`, `scan`: `string_split`, `string_replace`, `string_contains` and kin; searching with string patterns
  (`string_cases`, `string_position`, ...).
- `shape`: `string_take`, `string_drop`, `characters`, `from_characters`, padding and trimming.
- `pattern`, `strpat`, `regex`: string patterns as terms (`string_expression`), their reading as patterns over text, and
  `regular_expression`.
- `read`: `to_expression`, a string read as MathCAS source.
- `ctx`: the symbols, argument checks, failures and fuel shared by every native.

## Design
- `install` registers every function of `FUNCTIONS`; `string_join`, `string_length`, `to_string` and `split` belong to
  `mc-lists`.
- Lengths and positions count Unicode scalar values, never bytes, and never split a character; grapheme clusters are
  not segmented.
- A symbol or compound term where a string is expected leaves the call unevaluated; any other atom gives
  `failure(argument, f, k)` and a position outside the string `failure(index, i, n)`. A list in the first argument
  maps the function over it.
- Search strings are literal; several are tried in order at each position, the leftmost occurrence wins and
  occurrences do not overlap.
- `to_expression` reads with `mc-syntax` and leaves evaluation to the caller; names are not resolved through modules.
- Every native charges fuel for the bytes it scans or builds and polls the interrupt; results above 2^30 bytes give
  `failure(limit, memory, call)`.

## Tests
- Integration tests in `tests/` cover string functions and string patterns on the assembled engine (`mc-engine`).
- Run them with `cargo test --release -p mc-strings`.
