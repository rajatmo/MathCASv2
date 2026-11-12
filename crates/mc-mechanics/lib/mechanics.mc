#! mathcas 2
# Analytical mechanics toolkit (roadmap 3.5 P1), evaluated after the other libraries, defined
# in every session. Helper functions are prefixed `m31_` and not part of the interface; pattern variables in rules
# are prefixed `m31` too, so a user's global value cannot shadow them. Methods and literature:
# crates/mc-mechanics/NOTES.md.

# --- coordinates ---
# A coordinate is a function of the independent variables, written x(t) (or u(x, t) for a field), or its bare name
# x, applied to the variables here. One coordinate may stand alone; several form a list.

# m31_vars(t): the independent variables as a list
fn m31_vars(t) = if is_list(t) then t else [t]

# m31_coord(q, vs): the coordinate q as a function of the variables vs, or none
fn m31_coord(q, vs) =
  if is_symbol(q) then apply(q, vs)
  else if not is_atom(q) and is_symbol(head(q)) and apply(list, q) === vs then q
  else none

# m31_coords(q, vs): the coordinates as a list of terms u(vs), or none when one is not of that form
fn m31_coords(q, vs) = {
  let cs = map(c => m31_coord(c, vs), if is_list(q) then q else [q])
  if any_true(cs, c => c === none) then none else cs
}

# m31_ok(q, t): q and t describe coordinates and independent variables
fn m31_ok(q, t) = all_true(m31_vars(t), is_symbol) and length(m31_vars(t)) > 0 and m31_coords(q, m31_vars(t)) !== none

# m31_comps(nv, k): the multi-indices of nv entries and total order k
fn m31_comps(nv, k) =
  if nv == 1 then [[k]]
  else flatten(map(i => map(r => prepend(r, i), m31_comps(nv - 1, k - i)), range(0, k)), 1)

# m31_dterm(c, al): the derivative of the coordinate c = u(vs) with the multi-index al
fn m31_dterm(c, al) = if total(al) == 0 then c else apply(apply(derivative, al)(head(c)), apply(list, c))

# m31_dtot(e, vs, al): the total derivative D^al e
fn m31_dtot(e, vs, al) = {
  var r = e
  for i in range(length(vs)) {
    if al[i] > 0 then r = d(r, [vs[i], al[i]])
  }
  r
}

# m31_jets(e, c, vs): the multi-indices al (order 0 first) whose derivative of c occurs in e, up to order 6 for one
# variable and 4 for several
fn m31_jets(e, c, vs) = {
  let top = if length(vs) == 1 then 6 else 4
  prepend(select(flatten(map(k => m31_comps(length(vs), k), range(top)), 1), al => not free_of(e, m31_dterm(c, al))), map(v => 0, vs))
}

# m31_el(L, c, vs): the variational derivative of the action of L with respect to the coordinate c,
# sum over al of (-1)^|al| D^al (dL/du_al)
fn m31_el(L, c, vs) = total(map(al => (-1)^total(al)*m31_dtot(d(L, m31_dterm(c, al)), vs, al), m31_jets(L, c, vs)))

# --- constraints ---
# m31_lhs(g): an equation g1 == g2 as the expression g1 - g2
fn m31_lhs(g) = if not is_atom(g) and head(g) === equal then g[1] - g[2] else g

# m31_velocity_free(g, cs, vs): g contains no derivative of a coordinate
fn m31_velocity_free(g, cs, vs) = all_true(cs, c => length(m31_jets(g, c, vs)) == 1)

# m31_force(g, lam, c, cs, vs): the generalised force of the constraint g with multiplier lam on the coordinate c:
# lam dg/dq for a holonomic constraint, lam dg/dq' for one in the velocities (d'Alembert's principle)
fn m31_force(g, lam, c, cs, vs) =
  if m31_velocity_free(g, cs, vs) or length(vs) > 1 then lam*d(g, c)
  else lam*d(g, m31_dterm(c, [1]))

# m31_multipliers(ms, k, vs): the k multipliers as functions of the variables (default lambda(1), lambda(2), ...)
fn m31_multipliers(ms, k, vs) =
  if ms === none then map(i => apply(lambda(i), vs), range(k))
  else map(lam => m31_coord(lam, vs), if is_list(ms) then ms else [ms])

# euler_lagrange_equations(L, q, t): the Euler-Lagrange equations dL/dq - d/dt dL/dq' + d^2/dt^2 dL/dq'' - ... == 0
# of the Lagrangian L for each coordinate q (x(t), or a field u(x, t) with t a list of variables); with
# constraints: [g1, ...] the multiplier terms and the constraint equations themselves.
fn euler_lagrange_equations(L, q, t; constraints = [], multipliers = none) when m31_ok(q, t) = {
  let vs = m31_vars(t)
  let cs = m31_coords(q, vs)
  let gs = map(m31_lhs, if is_list(constraints) then constraints else [constraints])
  let lams = m31_multipliers(multipliers, length(gs), vs)
  let eqs = map(c => (m31_tidy(m31_el(L, c, vs) + total(map(k => m31_force(gs[k], lams[k], c, cs, vs), range(length(gs))))) == 0), cs)
  join(eqs, map(g => (g == 0), gs))
}

# --- momenta, energy, Legendre transform ---

# m31_vels(cs): the velocities x'(t) of the coordinates x(t)
fn m31_vels(cs) = map(c => m31_dterm(c, [1]), cs)

# m31_list(x): x as a list
fn m31_list(x) = if is_list(x) then x else [x]

# m31_point(q, t): q and t describe coordinates of a system of points (one independent variable)
fn m31_point(q, t) = is_symbol(t) and m31_ok(q, t)

# generalized_momenta(L, q, t): the canonical momenta dL/dq' of the coordinates q
fn generalized_momenta(L, q, t) when m31_point(q, t) = map(v => d(L, v), m31_vels(m31_coords(q, [t])))

# energy_function(L, q, t): the energy (Jacobi integral) sum q' dL/dq' - L, conserved when L does not depend on t
fn energy_function(L, q, t) when m31_point(q, t) =
  m31_tidy(total(map(v => v*d(L, v), m31_vels(m31_coords(q, [t])))) - L)

# m31_first(sol): the first solution of solve, with conditional_expression unwrapped and integer constants set to 0
fn m31_first(sol) = rewrite(rewrite(sol[1], [conditional_expression(m31a_, m31b_) :> m31a]), c(m31k_) :> 0)

# m31_legendre(L, cs, ps): the Hamiltonian sum p v - L with the velocities v solved from p == dL/dv, or none
# when that system has no unique solution (a singular Lagrangian)
fn m31_legendre(L, cs, ps) = {
  let vels = m31_vels(cs)
  let ws = map(i => m31_v(i), range(length(vels)))
  let Lw = rewrite(L, map(i => vels[i] -> ws[i], range(length(vels))))
  let sol = solve(map(i => (ps[i] == d(Lw, ws[i])), range(length(ws))), ws)
  if not is_list(sol) or length(sol) != 1 or not free_of(map(r => r[2], sol[1]), m31_v) then none
  else m31_tidy(rewrite(total(map(i => ps[i]*ws[i], range(length(ws)))) - Lw, sol[1]))
}

# legendre_transform(L, q, p, t): the Hamiltonian of the Lagrangian L in the coordinates q and the momenta p
# (names or functions p(t)); legendre_transform(f, x, p): the Legendre transform p x - f with x solved from
# p == f'(x) (the first solution, real ones first)
fn legendre_transform(L, q, p, t) when m31_point(q, t) and length(m31_list(p)) == length(m31_list(q)) and m31_legendre(L, m31_coords(q, [t]), m31_list(p)) !== none =
  m31_legendre(L, m31_coords(q, [t]), m31_list(p))
fn legendre_transform(f, x, p) when is_symbol(x) and is_list(solve(p == d(f, x), x)) and length(solve(p == d(f, x), x)) > 0 =
  m31_tidy(rewrite(p*x - f, m31_first(solve(p == d(f, x), x))))

# --- Hamilton's equations, Poisson brackets ---

# m31_in(e, names, tag): e with each name (a symbol or a term x(t)) replaced by tag(i)
fn m31_in(e, names, tag) = rewrite(e, map(i => names[i] -> tag(i), range(length(names))))

# m31_out(e, names, tag): the inverse of m31_in
fn m31_out(e, names, tag) = rewrite(e, map(i => tag(i) -> names[i], range(length(names))))

# m31_fun(x, t): the name x as the function x(t) (a term x(t) as it is)
fn m31_fun(x, t) = if is_symbol(x) then x(t) else x

# hamilton_equations(H, q, p, t): Hamilton's equations q' == dH/dp, p' == -dH/dq for the coordinates q and momenta
# p (names, or functions x(t)), as equations in the functions of t
fn hamilton_equations(H, q, p, t) when is_symbol(t) and length(m31_list(p)) == length(m31_list(q)) = {
  let qs = m31_list(q)
  let ps = m31_list(p)
  let h = m31_in(m31_in(H, map(x => m31_fun(x, t), qs), m31_q), map(x => m31_fun(x, t), ps), m31_p)
  let h2 = m31_in(m31_in(h, qs, m31_q), ps, m31_p)
  let back = e => m31_out(m31_out(e, map(x => m31_fun(x, t), qs), m31_q), map(x => m31_fun(x, t), ps), m31_p)
  let qe = map(i => (d(m31_fun(qs[i], t), t) == back(d(h2, m31_p(i)))), range(length(qs)))
  let pe = map(i => (d(m31_fun(ps[i], t), t) == back(-d(h2, m31_q(i)))), range(length(ps)))
  join(qe, pe)
}

# poisson_bracket(f, g, q, p): the Poisson bracket sum df/dq dg/dp - df/dp dg/dq over the conjugate pairs
fn poisson_bracket(f, g, q, p) when length(m31_list(p)) == length(m31_list(q)) = {
  let qs = m31_list(q)
  let ps = m31_list(p)
  let fi = m31_in(m31_in(f, qs, m31_q), ps, m31_p)
  let gi = m31_in(m31_in(g, qs, m31_q), ps, m31_p)
  m31_tidy(m31_out(m31_out(total(map(i => d(fi, m31_q(i))*d(gi, m31_p(i)) - d(fi, m31_p(i))*d(gi, m31_q(i)), range(length(qs)))), qs, m31_q), ps, m31_p))
}

# --- Noether charges ---

# m31_explicit_dt(L, cs, t): the partial derivative of L with respect to the explicit t (coordinates and velocities
# held fixed)
fn m31_explicit_dt(L, cs, t) = {
  let vels = m31_vels(cs)
  let Lw = m31_in(m31_in(L, vels, m31_v), cs, m31_q)
  m31_out(m31_out(d(Lw, t), vels, m31_v), cs, m31_q)
}

# m31_noether(L, cs, t, etas, xi, F): [the charge, the residual of the symmetry condition]. The condition for the
# generator xi d/dt + sum eta_i d/dq_i with gauge term F (Noether 1918; Olver, Applications of Lie groups to
# differential equations, 4.4): pr X(L) + L D_t xi = D_t F, with eta_i^(1) = D_t eta_i - q_i' D_t xi; the charge
# sum p_i (eta_i - q_i' xi) + L xi - F.
fn m31_noether(L, cs, t, etas, xi, F) = {
  let vels = m31_vels(cs)
  let dxi = d(xi, t)
  let res = total(map(i => etas[i]*d(L, cs[i]) + (d(etas[i], t) - vels[i]*dxi)*d(L, vels[i]), range(length(cs)))) + xi*m31_explicit_dt(L, cs, t) + L*dxi - d(F, t)
  let charge = total(map(i => d(L, vels[i])*(etas[i] - vels[i]*xi), range(length(cs)))) + L*xi - F
  [m31_tidy(charge), simplify(expand(res))]
}

# noether_charge(L, q, t, eta): the conserved charge of the symmetry q_i -> q_i + epsilon eta_i of the Lagrangian
# L, with time_shift: xi for t -> t + epsilon xi and gauge: F when L changes by the total derivative epsilon dF/dt;
# failure(not_a_symmetry, ...) with the residual of the symmetry condition when it does not vanish
fn noether_charge(L, q, t, eta; time_shift = 0, gauge = 0) when m31_point(q, t) and length(m31_list(eta)) == length(m31_list(q)) = {
  let r = m31_noether(L, m31_coords(q, [t]), t, m31_list(eta), time_shift, gauge)
  if r[2] === 0 then r[1] else failure(not_a_symmetry, noether_charge, r[2])
}

# --- equilibria and small oscillations ---

# m31_rest(L, cs): L with the velocities set to 0 (minus the potential)
fn m31_rest(L, cs) = rewrite(L, map(v => v -> 0, m31_vels(cs)))

# m31_names(q, t): the coordinates as the user wrote them (names or terms x(t)), as a list
fn m31_names(q, t) = m31_list(q)

# equilibrium_points(L, q, t): the equilibria dV/dq == 0 of the potential V = -L at rest, as rules for the
# coordinates (written as in q)
fn equilibrium_points(L, q, t) when m31_point(q, t) = {
  let cs = m31_coords(q, [t])
  let names = m31_names(q, t)
  let w = m31_in(m31_rest(L, cs), cs, m31_q)
  let qs = map(i => m31_q(i), range(length(cs)))
  let sol = solve(map(x => (d(w, x) == 0), qs), qs)
  map(s => m31_out(s, names, m31_q), sol)
}

# m31_hessians(L, cs, q0): [M, K, G] at the point q0 with the velocities 0: the mass matrix d^2L/dv dv, the
# stiffness matrix -d^2L/dq dq and the gyroscopic matrix d^2L/dv dq - its transpose
fn m31_hessians(L, cs, q0) = {
  let vels = m31_vels(cs)
  let k0 = length(cs)
  let w = m31_in(m31_in(L, vels, m31_v), cs, m31_q)
  let at = e => rewrite(rewrite(e, map(i => m31_v(i) -> 0, range(k0))), map(i => m31_q(i) -> q0[i], range(k0)))
  let mm = table(simplify(at(d(w, m31_v(i), m31_v(j)))), [i, k0], [j, k0])
  let kk = table(simplify(at(-d(w, m31_q(i), m31_q(j)))), [i, k0], [j, k0])
  let mix = table(at(d(w, m31_v(i), m31_q(j))), [i, k0], [j, k0])
  [mm, kk, simplify(mix - transpose(mix))]
}

# m31_q0(at, k0): the equilibrium as a list of k0 values (default the origin; a list of values or of rules)
fn m31_q0(at, names, k0) =
  if at === none then map(i => 0, range(k0))
  else if is_list(at) and length(at) > 0 and not is_atom(at[1]) and head(at[1]) === `rule` then map(x => rewrite(x, at), names)
  else m31_list(at)

# m31_dyn(h, w): the dynamical matrix K - w^2 M + i w G of the linearised equations M q'' + G q' + K q = 0 for
# q = v e^(i w t)
fn m31_dyn(h, w) = h[2] - w^2*h[1] + im*w*h[3]

# normal_modes(L, q, t): [the frequencies, the mode vectors] of small oscillations about the equilibrium at the
# origin (or at: the equilibrium, values or rules). The frequencies w are the roots of det(K - w^2 M + i w G) = 0,
# an even polynomial in w (G is antisymmetric), solved for w^2 (with G = 0: w^2 the eigenvalues of M^-1 K); the
# modes are the null vectors of the dynamical matrix (complex with gyroscopic terms).
fn normal_modes(L, q, t; at = none) when m31_point(q, t) = {
  let cs = m31_coords(q, [t])
  let q0 = m31_q0(at, m31_names(q, t), length(cs))
  let h = m31_hessians(L, cs, q0)
  let chi = expand(rewrite(expand(det(m31_dyn(h, m31_w))), m31_w -> sqrt(m31_w2)))
  let roots = unique(map(s => simplify(rewrite(m31_w2, s)), solve(chi == 0, m31_w2)))
  let pairs = flatten(map(r => map(v => [simplify(sqrt(r)), map(simplify, v)], m31_null(simplify(m31_dyn(h, sqrt(r))))), roots), 1)
  [map(pr => pr[1], pairs), map(pr => pr[2], pairs)]
}

# m31_null(a): a basis of the null space of the singular matrix a; when null_space cannot tell the rank of a
# symbolic matrix (entries with radicals), the vector with last entry 1 solving the first n - 1 equations (the
# null vector of a matrix of rank n - 1)
fn m31_null(a) = {
  let ns = null_space(a)
  let k0 = length(a)
  if length(ns) > 0 then ns
  else if k0 == 1 then [[1]]
  else {
    let v = linear_solve(table(a[i, j], [i, k0 - 1], [j, k0 - 1]), table(-a[i, k0], [i, k0 - 1]))
    if is_failure(v) then [] else [append(map(simplify, v), 1)]
  }
}

# --- phase portraits and numerical integration ---

# m31_accels(L, cs, t): the accelerations solved from the Euler-Lagrange equations, as rules x''(t) -> a, or none
fn m31_accels(L, cs, t) = {
  let eqs = map(c => (m31_el(L, c, [t]) == 0), cs)
  let acc = map(c => m31_dterm(c, [2]), cs)
  let sol = solve(eqs, acc)
  if is_list(sol) and length(sol) == 1 then sol[1] else none
}

# phase_portrait(L, q, t, [x, a, b], [v, c, d]): the streamlines of the flow (q, q') -> (q', q'') of the
# Euler-Lagrange equation of one coordinate q in the phase plane, x the position and v the velocity axis; further
# arguments (options) go to stream_plot
fn phase_portrait(L, q, t, xr, vr, opts...) when m31_point(q, t) and not is_list(q) and is_list(xr) and is_list(vr) and m31_accels(L, m31_coords(q, [t]), t) !== none = {
  let c = m31_coords(q, [t])[1]
  let acc = rewrite(m31_dterm(c, [2]), m31_accels(L, [c], t))
  let back = e => rewrite(e, [m31_dterm(c, [1]) -> vr[1], c -> xr[1]])
  apply(stream_plot, join([[vr[1], back(acc)], xr, vr], opts))
}

# n_dsolve_motion(L, q, [t, t0, t1], q0, v0): the numerical solution (rules for the coordinate names, as n_dsolve
# gives them) of the Euler-Lagrange equations with q(t0) == q0, q'(t0) == v0; further arguments go to n_dsolve
fn n_dsolve_motion(L, q, tr, q0, v0, opts...) when is_list(tr) and length(tr) == 3 and m31_point(q, tr[1]) and length(m31_list(q0)) == length(m31_list(q)) and length(m31_list(v0)) == length(m31_list(q)) = {
  let t = tr[1]
  let cs = m31_coords(q, [t])
  let acc = m31_accels(L, cs, t)
  let eqs = if acc === none then map(c => (m31_el(L, c, [t]) == 0), cs) else map(r => (r[1] == r[2]), acc)
  let x0 = m31_list(q0)
  let w0 = m31_list(v0)
  let ics = join(map(i => (rewrite(cs[i], t -> tr[2]) == x0[i]), range(length(cs))), map(i => (rewrite(m31_dterm(cs[i], [1]), t -> tr[2]) == w0[i]), range(length(cs))))
  apply(n_dsolve, join([join(eqs, ics), map(head, cs), tr], opts))
}

# energy_drift(L, q, sol, [t, t0, t1]): the largest change |E(t) - E(t0)| of the energy function along the
# numerical solution sol at points + 1 equally spaced times
fn energy_drift(L, q, sol, tr; points = 200) when is_list(tr) and length(tr) == 3 and m31_point(q, tr[1]) = {
  let t = tr[1]
  let en = rewrite(energy_function(L, q, t), sol)
  let ts = map(i => n(tr[2] + (tr[3] - tr[2])*i/points), range(0, points))
  let es = map(s => rewrite(en, t -> s), ts)
  max(map(x => abs(x - es[1]), es))
}

# m31_tidy(e): e expanded, or simplified when the expanded form has radicals (relativistic and geodesic
# Lagrangians), which expand leaves as sums of unlike powers
fn m31_tidy(e) = {
  let x = expand(e)
  if length(select(cases(x, pow(_, _), inf), w => not is_integer(w[2]))) > 0 then simplify(x) else x
}
