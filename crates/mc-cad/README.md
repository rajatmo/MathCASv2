# mc-cad

Real algebraic geometry: cylindrical algebraic decomposition, quantifier elimination over the reals and complex
numbers, `reduce` in several real variables, exact optimisation and integer problems. It provides `resolve`, `exists`,
`for_all`, `cylindrical_decomposition` and the several-variable path of `find_instance`.

## Modules
- `cad`: preprocessing (Groebner bases of equations, factorisation, equational constraints), lifting with certified
  sample points, and the truth of every cell, with partial lifting for quantified variables.
- `project`: projection by McCallum's operator with Brown's improvement, Lazard's operator as the fallback.
- `vs`: linear virtual substitution, the fast path for variables that occur linearly.
- `sos`: a sum-of-squares certificate for `for_all` of one polynomial inequality.
- `output`: solution formulas, cylindrical or by sign conditions.
- `complex`: quantifier elimination over the complex numbers by Groebner bases.
- `optimize`, `integers`, `witness`: `minimize` and `maximize`, problems over the integers, existential sentences that
  are not polynomial.
- `front`, `formula`, `poly`: terms to problems, formulas over polynomial atoms, polynomial helpers.
- `reals`: `CadReals`, the last tier of the decision service of `mc-decide`; `natives`: the registered functions.

## Design
- Sample points are certified: rationals in sectors, real algebraic numbers over number fields in sections
  (`mc_algebra::realroot`).
- Parallel lifting runs under a shared work budget (`par`).
- Depends on `mc-api`, `mc-arith`, `mc-core`, `mc-eval`, `mc-poly`, `mc-algebra` and `mc-decide`; `mc-engine` is used
  by tests only.

## Tests
- Integration tests in `tests/`: decomposition and quantifier elimination, parallel lifting, `reduce` over the reals.
- A documentation example on the crate page.
- Run: `cargo test --release -p mc-cad`
