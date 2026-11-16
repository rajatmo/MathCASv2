# mc-syntax

The concrete syntax of MathCAS: lexer, parser, lowering to core terms and a printer back to text, generic over
`mc_api::Store`, with the editor support the notebook needs.

## Modules
- `lexer`, `parser`, `ast`, `prec`, `span`: source text to tokens with spans, tokens to a syntax tree (every node
  carries its span), binding powers shared with the printer, and positions and diagnostics.
- `lower`, `heads`, `scope`: the syntax tree to core terms through any `Store`, the head symbols lowering produces, and
  lexical bindings shared by lowering and the printer.
- `print`, `ident`: the printer, and the identifier, keyword and string-escape rules shared with the lexer.
- `colour`, `select`, `lint`: colouring classes of every token with bracket pairs and errors, extend selection, and
  editor warnings that are not errors.
- `nat`, `stack`: small natural-number arithmetic for literals, and running the recursive phases on a stack of known
  size.

## Design
- `parse` returns the tree with every lexical and syntax error and its position; `read` parses and lowers and fails
  if anything is wrong, so nothing of an erroneous input is evaluated.
- For every term except balls, non-finite machine reals and truncated power series, the printer's text reads back to
  the identical term.
- The lexer and printer share identifier rules, and the parser and printer share binding powers, so what one writes
  the other reads.
- The recursive phases run on a stack of known size, so deeply nested input does not overflow.
- The only dependency is `mc-api`.

## Tests
- Unit tests in `src`, a documentation example, and integration tests in `tests/` for errors, lowering, colouring,
  selection, juxtaposition, long text, series and a fuzz test over generated input.
- Run them with `cargo test --release -p mc-syntax`.
