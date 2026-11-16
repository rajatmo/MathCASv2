# mc-groups

Group theory and combinatorics: permutations, permutation groups, finitely presented groups, conjugacy classes and
character tables, posets, matroids and Young tableaux.

## Modules
- `perm`: permutations of {0, ..., n-1} as image vectors, with the right action.
- `chain`: stabiliser chains by the deterministic Schreier-Sims algorithm, in incremental form.
- `groups`: algorithms on permutation groups given by generators and a chain (order, orbits, stabilisers, membership).
- `backtrack`: backtrack search through a stabiliser chain.
- `named`: generators of the named groups; `classes`: conjugacy classes and character tables.
- `fp`: finitely presented groups: coset enumeration, rewriting systems, abelian invariants.
- `comb`: finite posets, matroids, Young tableaux.
- `api`: the natives, terms in and terms out.

## Design
- Permutations are terms `cycles([[1, 2, 3], [4, 5]])`: disjoint cycles, 1-based points, each cycle starting at its
  least point, cycles ordered by that point, no fixed points. Products apply the left factor first.
- A group is `permutation_group([g1, g2, ...])`; every algorithm runs on a base and strong generating set kept in a
  small cache.
- A finitely presented group is `finitely_presented_group([a, b, ...], [relators])` with words as lists of letters.
- Every native declines (stays unevaluated) on arguments it does not recognise.
- Depends only on `mc-core` and `mc-eval`.

## Tests
- An integration test in `tests/` runs the functions through an engine session (`mc-engine` and `mc-api` are
  development dependencies only).
- Run: `cargo test --release -p mc-groups`
