#! mathcas 2
# Warm inflation with a sphaleron friction (a port of a research notebook).
#
# A quartic inflaton V = lam phi^4 rolls with the friction of the strong sphaleron rate,
# Upsilon_sph(T) = (alpha_s N_c)^5 T^3/(2 f^2), cut by its own back-reaction:
# Upsilon_eff = Upsilon_sph/(1 + 2 N_f N_c^4 alpha_s^5 T/H). In slow roll the radiation bath is in equilibrium with
# the friction, (Upsilon/(4H)) (V'/(3H + Upsilon))^2 = alpha T^4, which fixes T(phi). The script finds the end of
# inflation (eps_V/(1 + Q) = 1), the field phi* 55 e-folds before it (N = int (V/V')(1 + Q)/Mp^2 dphi), the
# temperature, Q and the amplitude A_s = H^3 T F(Q)/(4 pi^2 phi'^2) at phi*, and draws V(phi), Q(phi) and T, H
# against phi (PDF and PNG files in the current folder).
# Run: mc-repl examples/warm_inflation.mc  (the stage times are printed at the end).

# --- constants (GeV units)

mp = 2.4e18                 # reduced Planck mass
gstar = 427/4               # relativistic degrees of freedom
nc = 3                      # colours
nf = 5                      # light flavours
alpha_s = 0.0242            # strong coupling
alpha = gstar*pi^2/30       # rho_R = alpha T^4

# --- the model (lam and f are the parameters of every function below)

fn V(lam, phi) = lam*phi^4
fn dV(lam, phi) = 4*lam*phi^3
fn hubble(lam, phi) = sqrt(V(lam, phi)/(3*mp^2))
fn ups_sph(f, t) = (alpha_s*nc)^5*t^3/(2*f^2)
fn ups_eff(lam, f, t, phi) = ups_sph(f, t)/(1 + 2*nf*nc^4*alpha_s^5*t/hubble(lam, phi))
# the enhancement of the scalar power spectrum by the thermal bath
fn F(q) = 3.27e-4*q^7 + 168*q*(1/3*(1 + 9/25*q^2) + 2/3*tanh(1/(30*q)))

# T(phi) from (Upsilon/(4H)) (V'/(3H + Upsilon))^2 = alpha T^4, solved for log T (the notebook solves for T from
# T = 10^20; log T keeps the Newton steps inside the range of the equation over the 20 decades between the start
# and the root)
fn t_of(lam: Real, f: Real, phi: Real) = {
  let h = hubble(lam, phi)
  let resid = u => {
    let up = ups_eff(lam, f, exp(u), phi)
    log(up/(4*h)*(dV(lam, phi)/(3*h + up))^2) - log(alpha) - 4*u
  }
  exp(u /. find_root(resid(u), [u, log(1.0e14)]))
}
fn q_of(lam: Real, f: Real, phi: Real) = ups_eff(lam, f, t_of(lam, f, phi), phi)/(3*hubble(lam, phi))
fn dphi_of(lam: Real, f: Real, phi: Real) = -dV(lam, phi)/(3*hubble(lam, phi)*(1 + q_of(lam, f, phi)))
fn eps_v(lam: Real, f: Real, phi: Real) = mp^2/(2*(1 + q_of(lam, f, phi)))*(dV(lam, phi)/V(lam, phi))^2

# the end of inflation (eps_V/(1 + Q) = 1, started at phi = Mp) and the field 55 e-folds earlier
fn phi_end(lam: Real, f: Real) = p /. find_root(eps_v(lam, f, p) - 1, [p, mp])
fn efolds(lam: Real, f: Real, phis: Real) =
  n_integrate(V(lam, p)/dV(lam, p)*(1 + q_of(lam, f, p))/mp^2, [p, phi_end(lam, f), phis])
fn phi_star(lam: Real, f: Real) = p /. find_root(efolds(lam, f, p) - 55, [p, 10*mp])

# --- the benchmark of the notebook: lam = 10^-19, f = 0.518 10^12 GeV

lam0 = 1.0e-19
f0 = 0.518e12

stage_end = timing(phi_end(lam0, f0))
phie = stage_end[2]
stage_star = timing(phi_star(lam0, f0))
phis = stage_star[2]
stage_values = timing({
  let ts = t_of(lam0, f0, phis)
  let hs = hubble(lam0, phis)
  let qs = q_of(lam0, f0, phis)
  let as = 1/(4*pi^2)*hs^3*ts/dphi_of(lam0, f0, phis)^2*F(qs)
  [ts, hs, qs, as]
})
t_star = stage_values[2][1]
h_star = stage_values[2][2]
q_star = stage_values[2][3]
a_s = stage_values[2][4]
say("phi_end = ", phie, " GeV")
say("phi*/Mp = ", phis/mp)
say("T*/10^12 GeV = ", t_star/1.0e12, ", T*/H* = ", t_star/h_star)
say("Q* = ", q_star, ", A_s = ", a_s)

# --- the checks at the end of the notebook: T at phi = 2.18 10^19 GeV and the weak sphaleron rate there

stage_checks = timing({
  let t1 = t_of(lam0, f0, 2.18e19)
  let aw = 0.03
  say("T(2.18e19) = ", t1, " GeV; T^4 aw^5/log(1/aw) = ", t1^4*aw^5/log(1/aw), " (at T = 4.7736661827238234e13: ",
      4.7736661827238234e13^4*aw^5/log(1/aw), "); 1/log(1/aw) = ", 1/log(1/aw))
  say("T aw^5/log(1/aw) = ", t1*aw^5/log(1/aw), ", H(2.18e19) = ", hubble(lam0, 2.18e19),
      ", 1.66 sqrt(107) T^2/Mp = ", 1.66*sqrt(107)*t1^2/mp)
})

# --- plots: V(phi)/Mp^4 with phi_end and phi* marked, Q(phi), and T and H against phi (PDF and PNG)

indigo = "#560591"
ruby = "#e0115f"
olive = "#808000"
fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }

stage_plots = timing([
  # phi_end and phi* marked on the curve, labelled as in the notebook
  save("warm_inflation_potential",
       plot(V(lam0, p)/mp^4, [p, 0.5*phie, 1.2*phis], colors: [indigo], x_label: r"$\phi$ (GeV)",
            y_label: r"$V(\phi)/M_{pl}^4$", legend: [r"$\lambda = 10^{-19}$, $f = 0.518 \times 10^{12}$ GeV"],
            legend_position: top_left,
            epilog: [point_size(0.012), point([[phie, V(lam0, phie)/mp^4], [phis, V(lam0, phis)/mp^4]]),
                     text(r"$\phi_{end}$", scaled([0.1, 0.1])), text(r"$\phi_*$", scaled([0.81, 0.55]))])),
  save("warm_inflation_q",
       plot(q_of(lam0, f0, p), [p, phie, phis], colors: [indigo], x_label: r"$\phi$ (GeV)",
            y_label: r"$Q = \Upsilon/(3H)$")),
  save("warm_inflation_t_h",
       plot([hubble(lam0, p), t_of(lam0, f0, p)], [p, phie, phis], y_scale: log, colors: [indigo, ruby],
            x_label: r"$\phi$ (GeV)", y_label: "(GeV)", legend: ["H", "T"], legend_position: bottom_right))])

stages = [stage_end, stage_star, stage_values, stage_checks, stage_plots]
say("stage times (s): phi_end ", stage_end[1], ", phi* ", stage_star[1], ", values at phi* ", stage_values[1],
    ", checks ", stage_checks[1], ", plots ", stage_plots[1], "; total ", sum(map(st => st[1], stages)))
