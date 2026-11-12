//! MathCAS v2 analytical mechanics toolkit (roadmap 3.5 P1).
//!
//! Euler-Lagrange equations of point and field Lagrangians, Legendre transforms and Hamilton's equations, Poisson
//! brackets, Noether charges, constraints with Lagrange multipliers, small oscillations about an equilibrium, phase
//! portraits and the numerical integration of the equations of motion. Everything is MathCAS source
//! (`lib/mechanics.mc`, [`SOURCE`]) built on the calculus, solving, linear algebra, ODE and plot libraries, called
//! through the evaluator. Methods, literature and decisions: `NOTES.md` (decisions p31.1, p31.2, ...).
#![warn(missing_docs)]

/// The library's source text.
pub const SOURCE: &str = include_str!("../lib/mechanics.mc");

/// The name of the source in messages.
pub const ORIGIN: &str = "lib/mechanics.mc";

/// The public functions of this crate with a one-line description each.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("euler_lagrange_equations", "euler_lagrange_equations(L, q, t): the Euler-Lagrange equations of the Lagrangian L in the coordinates q (x(t), or fields u(x, t) with t a list of variables; any order of derivatives); options constraints: and multipliers:"),
    ("generalized_momenta", "generalized_momenta(L, q, t): the canonical momenta dL/dq' of the coordinates q"),
    ("energy_function", "energy_function(L, q, t): the energy sum q' dL/dq' - L of the Lagrangian L"),
    ("legendre_transform", "legendre_transform(L, q, p, t): the Hamiltonian of L in the coordinates q and momenta p; legendre_transform(f, x, p): the Legendre transform p x - f(x) with p = f'(x)"),
    ("hamilton_equations", "hamilton_equations(H, q, p, t): Hamilton's equations q' == dH/dp, p' == -dH/dq as equations in functions of t"),
    ("poisson_bracket", "poisson_bracket(f, g, q, p): the Poisson bracket of f and g in the coordinates q and conjugate momenta p"),
    ("noether_charge", "noether_charge(L, q, t, eta): the conserved charge of the symmetry q -> q + epsilon eta of L (options time_shift:, gauge:), checked by the symmetry condition"),
    ("equilibrium_points", "equilibrium_points(L, q, t): the equilibria of the system of Lagrangian L, as rules for the coordinates"),
    ("normal_modes", "normal_modes(L, q, t): [frequencies, mode vectors] of the small oscillations about the origin (option at: another equilibrium), gyroscopic terms included"),
    ("phase_portrait", "phase_portrait(L, q, t, [x, a, b], [v, c, d]): the streamlines of the motion of one coordinate in its phase plane (position x, velocity v)"),
    ("n_dsolve_motion", "n_dsolve_motion(L, q, [t, t0, t1], q0, v0): the numerical solution of the Euler-Lagrange equations from the positions q0 and velocities v0"),
    ("energy_drift", "energy_drift(L, q, sol, [t, t0, t1]): the largest change of the energy function along a numerical solution (a check of the integration)"),
];
