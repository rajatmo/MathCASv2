# mc-fields

Finite fields: prime and extension fields F_(p^n) with Conway defining polynomials, exact arithmetic on elements,
embeddings between fields, norms, traces, minimal polynomials, primitive elements, discrete logarithms, roots, and
polynomials over finite fields with their factorisation.

## Modules
- `fp`: the prime field and polynomials over it, with Rabin's irreducibility test.
- `field`: F_p[x]/(f), its elements, element orders and Pohlig-Hellman discrete logarithms.
- `conway`, `conway_table`: Conway polynomials, by search and coset enumeration, with a stored table and fallbacks.
- `fqpoly`: polynomials over F_q and their factorisation (Cantor and Zassenhaus).
- `embed`: embeddings F_(p^m) -> F_(p^n) and their projections.
- `terms`: reading and writing fields, elements and polynomials as terms of the language.
- `natives`: the registered functions (`FUNCTIONS`), and wrappers that give `add`, `mul`, `pow`, `equal`, `element`,
  `factor`, `expand`, the polynomial gcd and quotient functions and others their meaning on field elements; any other
  call goes to the native the wrapper replaced.

## Design
- Fields have Conway defining polynomials, so embeddings between subfields are compatible.
- Wrappers only claim calls whose arguments are field elements or polynomials over a field; everything else passes
  through to the original native.
- Depends on `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-numtheory` and `mc-poly`.
- Literature: Lidl and Niederreiter, *Finite Fields*; von zur Gathen and Gerhard, *Modern Computer Algebra*; Heath and
  Loehr on generating Conway polynomials.

## Tests
- Unit tests in `src`; integration tests in `tests/`: field axioms, the functions through an engine session.
- Run: `cargo test --release -p mc-fields`
