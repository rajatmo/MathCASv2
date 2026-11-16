# mc-algebra

Algebraic numbers for the whole system: number fields Q[t]/(f), elements with certified embeddings, polynomials over
number fields, real algebraic numbers and the functions built on them (`minimal_polynomial`, `to_number_field`,
`root_sum`, `to_radicals`, the `extension:` option of `factor`, `count_roots`).

## Modules
- `field`: number fields in the power basis, arithmetic, inverses, traces, norms, characteristic and minimal polynomials.
- `embed`: which complex root of `f` the generator is, as an isolating disk refined on demand with ball arithmetic.
- `algnum`: algebraic numbers (a field element with an embedding), exact signs, primitive elements of towers.
- `kpoly`: polynomials over a number field: modular gcd with a Euclidean fallback, norms, Trager factorisation.
- `qpoly`: the dense rational polynomials underneath.
- `realroot`, `countroots`: real roots over real number fields, exact comparison and signs, root counting.
- `transroot`, `transroots`: roots of analytic functions as exact objects, certified root counts.
- `minpoly`, `rootsum`, `rootstruct`: minimal polynomials of expressions, sums of a function over the roots of a polynomial.
- `convert`, `natives`, `cx`: conversion between terms and algebraic numbers, the registered functions, the call context.

## Design
- An embedding is a certified isolating disk; every sign or comparison is decided by refining it until it is separated
  from zero, or proved exactly equal, never by a fixed-precision guess.
- Factorisations over a number field are checked by expansion before they are returned.
- Natives charge their work to the evaluator's fuel, through one shared per-call context.
- Depends on `mc-api`, `mc-arith` (numbers and canonical terms), `mc-core`, `mc-eval` (native interface), `mc-numeric`
  and `mc-poly`; `mc-engine` is used by tests only.

## Tests
- Unit tests in `src` for the field, embedding and polynomial layers.
- Integration tests in `tests/` run natives and root problems through a full engine session.
- A documentation example on the crate page.
- Run: `cargo test --release -p mc-algebra`
