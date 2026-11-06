#! mathcas 2
# A Landau-type finite-temperature potential for a first-order phase transition (a port of a research notebook).
#
# V(phi, T) = A (T^2 - T0^2) phi^2 - B T phi^3 + (lambda/4) phi^4. The script finds its extrema symbolically, the
# critical temperature T_c (two degenerate minima), the field phi_c of the broken minimum there and the strength
# phi_c/T_c, the masses in both phases, and evaluates them for A = 0.2, B = 0.02, T0 = 100 GeV, lambda = 0.01. It draws
# the potential scaled to its barrier at T = 1.01 T_c, T_c and 0.99 T_c with the minima, the tunnelling arrow and the
# barrier (landau_potential_scaled.pdf and .png). The notebook's bounce section sets up the O(3) bounce equation in
# dimensionless form, y'' + (2/rho) y' = y - y^2 + kappa y^3; the script solves it from rho = 0.01 for several
# starting values near the true vacuum at T = T_c/1.01 (overshoot and undershoot) and draws them (bounce_trials.pdf
# and .png).
# Run: mc-repl examples/landau_potential.mc  (about a second).

fn veff(p, t) = aa*(t^2 - t0^2)*p^2 - bb*t*p^3 + lam/4*p^4
positive = aa > 0 and bb > 0 and t0 > 0 and lam > 0 and aa*lam > bb^2

stage_symbolic = timing({
  let ext = solve(d(veff(p, t), p) == 0, p)
  say("extrema: ", ext)
  # T_c: V(phi, T_c) = (lambda/4) phi^2 (phi - phi_c)^2, two equal minima; matching the phi^2 and phi^3 terms
  let crit = solve([aa*(t^2 - t0^2) == lam/4*pc^2, bb*t == lam/2*pc], [t, pc])
  say("degenerate minima at ", crit)
  say("m^2 in the symmetric phase: ", d(veff(p, t), [p, 2]) /. p -> 0)
})

# the benchmark: phi_+ (the broken minimum) and phi_max (the barrier top) are the larger and smaller nonzero extrema
bench = [aa -> 0.2, bb -> 0.02, t0 -> 100.0, lam -> 0.01]
fn v(p, t) = veff(p, t) /. bench
fn phi_plus(t) = (3*bb*t + sqrt(9*bb^2*t^2 - 8*lam*aa*(t^2 - t0^2)))/(2*lam) /. bench
fn phi_max(t) = (3*bb*t - sqrt(9*bb^2*t^2 - 8*lam*aa*(t^2 - t0^2)))/(2*lam) /. bench
t_c = t0*sqrt(aa*lam/(aa*lam - bb^2)) /. bench
phi_c = 2*bb*t_c/lam /. bench
say("T_c = ", t_c, " GeV, phi_c = ", phi_c, " GeV, phi_c/T_c = ", phi_c/t_c)
# solve's direct answer for T_c, its roots checked against the equation (GAP G39: two spurious roots, unsimplified)
direct = map(s => (t /. s) /. bench, solve(veff(p, t) - veff(0, t) == 0 /. p -> (3*bb*t + sqrt(9*bb^2*t^2 - 8*lam*aa*(t^2 - t0^2)))/(2*lam), t))
say("solve's roots: ", direct, "; those that solve it: ", select(direct, r => r > 0 and abs(v(phi_plus(r), r) - v(0, r)) < 1.0e-9*phi_c^4*lam /. bench))
say("m_phi at T_c: symmetric phase ", sqrt(2*aa*(t_c^2 - t0^2)) /. bench, ", broken phase ",
    sqrt(2*aa*(t_c^2 - t0^2) - 6*bb*t_c*phi_c + 3*lam*phi_c^2) /. bench, " GeV")

# the potential at T = (1 - ep) T_c in units of phi_c, scaled to the barrier height at T_c
barrier = v(phi_max(t_c), t_c) - v(0, t_c)
fn vplot(x, ep) = v(x*phi_c, (1 - ep)*t_c)/barrier

stage_plot = timing({
  let tn = 0.99*t_c
  let xmin = phi_plus(tn)/phi_c
  let xbar = phi_max(tn)/phi_c
  # the three curves, with the minima, the tunnelling arrow, the barrier and the labels drawn over them
  let g = plot([vplot(x, -0.01), vplot(x, 0), vplot(x, 0.01)], [x, 0, 1.3], colors: ["#e0115f", "#808000", "#560591"],
    x_label: r"$\phi/\phi_c$", y_label: r"$V_{\mathrm{eff}}(\phi, T)$",
    epilog: [
      black, point_size(0.012), point([[0, 0], [xmin, vplot(xmin, 0.01)]]),
      arrow([[0, 0], [xmin, vplot(xmin, 0.01)]]),
      arrowheads([0.02, 0.02]), arrow([[xbar, 0], [xbar, vplot(xbar, 0.01)]]),
      text("Barrier", [0.34, 0.25]),
      rgb_color("#e0115f"), text(r"$T > T_c$", [0.9, 1.7]),
      rgb_color("#808000"), text(r"$T = T_c$", [1.0, 0.5]),
      rgb_color("#560591"), text(r"$T = T_n < T_c$", [1.05, -0.8]),
      black, text("Barrier tunneling from", [0.4, -1.0]), text("false vacuum to true vacuum", [0.4, -1.3])])
  export_data("landau_potential_scaled.pdf", g)
  export_data("landau_potential_scaled.png", g)
})
# --- the bounce equation at T = T_c/1.01: phi = m^2/(3 B T) y, r = rho/m with m^2 = 2 A (T^2 - T0^2), so that
# y'' + (2/rho) y' = y - y^2 + kappa y^3 with kappa = lambda m^2/(9 B^2 T^2) (the notebook's kappa has m for m^2)
tb = t_c/1.01
kappa = (lam*2*aa*(tb^2 - t0^2)/(9*bb^2*tb^2)) /. bench
y_true = (1 + sqrt(1 - 4*kappa))/(2*kappa)
say("T = T_c/1.01: kappa = ", kappa, ", true vacuum y_+ = ", y_true)
fn dy(y) = y - y^2 + kappa*y^3
eps0 = 1.0e-2
# from rho = 0.01 with the series start; stops where y' = 0 (undershoot) or y = 0 (overshoot)
fn trial(eta) = y /. n_dsolve([y''(r) + 2/r*y'(r) == dy(y(r)), y(eps0) == eta + dy(eta)*eps0^2/6, y'(eps0) == dy(eta)*eps0/3],
                              y, [r, eps0, 100], events: [y'(r) == 0, y(r) == 0])

stage_bounce = timing({
  let etas = map(k => y_true - 10.0^-k, [1, 2, 4, 6, 8])
  let sols = map(trial, etas)
  let ends = map(s => s[2][2], sols)
  say("trials from y_+ - 10^-k, k = 1, 2, 4, 6, 8, end at rho = ", ends, "; y there = ", map(k => sols[k](ends[k]), range(5)))
  let p = list_line_plot(map(k => map(r => [r, sols[k](r)], range(eps0, ends[k], (ends[k] - eps0)/300)), range(5)),
                         x_label: r"$\rho$", y_label: r"$y(\rho)$",
                         legend: map(k => string_join([r"$y_+ - 10^{-", to_string(k), "}$"]), [1, 2, 4, 6, 8]),
                         legend_position: bottom_left)
  export_data("bounce_trials.pdf", p)
  export_data("bounce_trials.png", p)
})

# the bounce itself: between the last undershoot and the first overshoot, where y(rho_end) - |y'(rho_end)| changes
# sign (y' = 0 at an undershoot's turning point, y = 0 with y' < 0 at an overshoot)
rule shoot(eta_) when is_number(eta) := {
  let s = trial(eta)
  let re = s[2][2]
  s(re) - abs(s'(re))
}
stage_shoot = timing(eta /. find_root(shoot(eta), [eta, y_true - 1.0e-3, y_true - 1.0e-2, y_true - 1.0e-4]))
eta_b = stage_shoot[2]
say("bounce: y(0) = y_+ - ", y_true - eta_b, ", phi(0) = ", ((2*aa*(tb^2 - t0^2))/(3*bb*tb) /. bench)*eta_b, " GeV")
say("stage times (s): symbolic ", stage_symbolic[1], ", potential plot ", stage_plot[1], ", bounce trials ", stage_bounce[1],
    ", shooting ", stage_shoot[1])
