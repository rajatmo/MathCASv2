# `mc-strings` (roadmap package 1C, branch `lib/r4-numtext`, commit 7934fb3)

String functions, Unicode-correct. Depends on `mc-api`, `mc-core`, `mc-eval`, `mc-syntax` (dev: `mc-engine`,
`mc-bench`). API: `install(k, h)`, `FUNCTIONS` (15 entries).

## What was built

- `ctx`: symbols, argument checks, failures, options, fuel.
- `search`: `string_split`, `string_replace`, `string_contains`, `string_starts`, `string_ends`.
- `shape`: `string_take`, `string_drop`, `characters`, `from_characters`, `string_reverse`, `to_upper`, `to_lower`,
  `string_pad`, `string_trim`.
- `read`: `to_expression`.

Forms: `string_split(s)` at white space, `(s, sep | [seps])` keeping empty pieces (k separators give k + 1), `(s, sep,
n)` at most n pieces; `string_replace(s, a -> b | a :> b | [rules])`, `(s, rules, n)`, `(s, a, b)`; `string_take` /
`string_drop` with `k` (negative from the end) or `[i, j]` / `[i]` (1-based, negative from the end; `[i, i-1]` empty);
`string_contains`/`starts`/`ends` with one string or a list; `string_pad(s, n[, pad], side: right|left|center)`;
`string_trim(s[, sub])`.

## Decisions

- **S.1** Characters are Unicode scalar values, never bytes (no grapheme segmentation); case uses the full mapping
  (`straße` to `STRASSE`; final sigma lowers to `ς`).
- **S.2** A symbol or compound where a string is expected leaves the call unevaluated; another atom is
  `failure(argument, f, k)`; out-of-range positions `failure(index, i, n)`; a list in the string position maps (except
  `from_characters`); results over 2^30 bytes `failure(limit, memory, call)`; unknown options
  `failure(unknown_option, name)`.
- **S.3** Search strings are literal; several are tried in order, leftmost occurrence wins, no overlaps; separators and
  search strings must be nonempty.
- **S.4** `to_expression` reads with `mc_syntax::read` and hands the term to the caller to evaluate; several statements
  become one `block`, empty text `null`; a syntax error is `failure(syntax, "line:col: message")`; names are not
  resolved through modules or imports (no read-time context at run time).
- **S.5** Fuel charged by bytes; scans poll the interrupt every 64 KB.

## Tests

`tests/strings.rs` (9, about 150 cases): splitting, replacing, take/drop (Unicode, index failures), searching, case and
characters with reversal, padding and trimming, `to_expression` (including a definition read at run time), long
strings (300 KB, 10^5 pieces, a size-limit failure), identical at 1 and 4 threads under collection stress.

## Known gaps

- `mc-lists`' `split` duplicates `string_split`'s basic forms (one name per concept would retire or alias `split`).
- `string_join`, `string_length`, `to_string` stay in `mc-lists`.
- No patterns (regular or term) in search strings.
