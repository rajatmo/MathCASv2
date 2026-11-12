//! MathCAS v2 differential equations and differential algebra (round 31).
//!
//! The library is MathCAS source (`lib/diffalg.mc`, [`SOURCE`]) that calls `d`, `solve`, `d_solve`, `integrate` and
//! the polynomial functions through the evaluator: Lie point symmetries of ordinary and partial differential
//! equations (determining equations and their solution), the symmetry algebra, reduction and integration by a
//! symmetry, conservation laws by multipliers, differential polynomials under rankings, the Rosenfeld-Groebner and
//! Thomas decompositions and the Painleve test. Methods, literature and decisions: `NOTES.md` (g31.1, g31.2, ...).
#![warn(missing_docs)]

/// The library's source text.
pub const SOURCE: &str = include_str!("../lib/diffalg.mc");

/// The name of the source in messages.
pub const ORIGIN: &str = "lib/diffalg.mc";

/// The public functions of this crate with a one-line description each.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("symmetry_determining_equations", "symmetry_determining_equations(eqs, u, x): the determining equations of the Lie point symmetries of the differential equations eqs for u of x, in the infinitesimals xi, eta"),
    ("lie_point_symmetries", "lie_point_symmetries(eqs, u, x): a basis of the Lie point symmetries of eqs, each the list [xi_1, ..., xi_p, eta_1, ..., eta_q] of the generator's coefficients"),
    ("symmetry_commutator", "symmetry_commutator(v, w, vars): the commutator [v, w] of two vector fields given by their components over vars"),
    ("canonical_coordinates", "canonical_coordinates([xi, eta], [x, y]): canonical coordinates [r, s] of the generator: xi r_x + eta r_y = 0, xi s_x + eta s_y = 1"),
    ("symmetry_reduce_order", "symmetry_reduce_order(eq, y, x, gen, v, r): the equation of one order less for v(r) = ds/dr in the canonical coordinates of the symmetry gen, with r and v in terms of x, y, y'"),
    ("symmetry_d_solve", "symmetry_d_solve(eq, y, x): the solutions of a first- or second-order ordinary differential equation found from a Lie point symmetry (canonical coordinates, quadrature)"),
    ("similarity_reduction", "similarity_reduction(eq, u, [x, t], gen, f, z): the ordinary differential equation for f(z) of the solutions invariant under the symmetry gen = [xi, tau, eta], with the invariant form of u and z"),
    ("conservation_law_multipliers", "conservation_law_multipliers(eq, u, x): a basis of the multipliers Lambda (of x, u and the derivatives up to order: q) with E_u(Lambda eq) == 0; degree: d for polynomial multipliers"),
    ("conservation_laws", "conservation_laws(eq, u, x): the conservation laws from the multipliers: [Lambda, Phi] (first integrals) for an ordinary differential equation, [Lambda, density, flux] for an evolution equation in [x, t]"),
    ("differential_leader", "differential_leader(p, u, x): the leader of the differential polynomial p (ranking: orderly or elimination)"),
    ("differential_initial", "differential_initial(p, u, x): the coefficient of the highest power of the leader of the differential polynomial p"),
    ("differential_separant", "differential_separant(p, u, x): the derivative of the differential polynomial p with respect to its leader"),
    ("differential_reduce", "differential_reduce(p, ps, u, x): the full (Ritt) remainder of the differential polynomial p by the differential polynomials ps"),
    ("delta_polynomial", "delta_polynomial(p, q, u, x): the Delta-polynomial of two differential polynomials whose leaders are derivatives of the same function"),
    ("rosenfeld_groebner", "rosenfeld_groebner(eqs, u, x): the regular differential chains [[equations], [inequations]] whose intersection is the radical differential ideal of eqs"),
    ("thomas_decomposition", "thomas_decomposition(eqs, u, x): the solutions of a system of ordinary differential equations as a disjoint union of simple systems [[equations], [inequations]]"),
    ("painleve_test", "painleve_test(eq, y, x): the Painleve test (leading orders, resonances, compatibility conditions) of a polynomial ordinary differential equation, one result per dominant balance"),
    ("lie_commutator_table", "lie_commutator_table(gens, vars): the table of the commutators [v_i, v_j] of the generators as combinations of v(1), v(2), ..."),
];
