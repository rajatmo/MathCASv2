#! mathcas 2
# Gravitational waves from Higgs decays during a first-order electroweak phase transition (a port of a research
# notebook).
#
# The Higgs potential with a dimension-six operator, V(H, T) = c2(T) H^2 + c4(T) H^4 + c6 H^6 (cutoff Lambda =
# 600 GeV, thermal masses through c_h and lambda_1), has a first-order transition at the critical temperature T_c,
# where the broken minimum H_0(T) is degenerate with H = 0. The script finds the minima symbolically, solves
# V(H_0(T_c), T_c) = 0 for T_c, and evaluates the thermal Higgs, tau and b masses, the Higgs decay widths to b b and to
# a heavy neutral lepton N and tau (m_N = 40 GeV, Yukawa 1e-3), the graviton bremsstrahlung rate of H -> b b h, and
# the resulting energy spectrum of gravitational waves today at T = 30 GeV. It prints the numbers of the notebook
# and draws its five figures (PDF and PNG files in the current folder).
# Run: mc-repl examples/gw_ew_fopt.mc  (about a second; the stage times are printed at the end).

# --- constants (GeV units)

hbar = 6.626e-34
e_charge = 1.602e-19
mp = 2.4e18          # reduced Planck mass
mh = 125             # Higgs mass
v0 = 246             # Higgs vacuum expectation value
g1 = 0.357           # U(1)_Y coupling
g2 = 0.652           # SU(2)_L coupling
g3 = 1.221           # SU(3)_c coupling
m_top = 172.69
m_b = 4.24
m_tau = 1.78
cutoff = 600         # Lambda, the scale of the H^6 operator
gstar = 427/4
g0 = 43/11
rho_c_over_h2 = 8.1e-47
t_today = 6.626e-13
gev_to_hz = 1.0e9*e_charge/hbar
hz_to_gev = 1/gev_to_hz
y_top = sqrt(2)*m_top/v0
y_b = sqrt(2)*m_b/v0
c_h = 1/16*(4*mh^2/v0^2 + g1^2 + 3*g2^2 + 4*y_top^2 - 12*v0^2/cutoff^2)
lambda1 = 1/cutoff^2
mu_h = sqrt(mh^2/2 - 3*v0^4/(4*cutoff^2))
lambda_h = (mh^2 - 3*v0^4/cutoff^2)/(2*v0^2)
xi = (g0/gstar)^(1/3)
say("y_t = ", n(y_top), ", y_b = ", n(y_b), ", c_h = ", n(c_h), ", mu_h = ", n(mu_h), ", lambda_h = ", n(lambda_h))

# --- 1. the minima of c2 x^2 + c4 x^4 + c6 x^6, symbolically (the notebook's derivation)

stage_symbolic = timing({
  let pot = c2*x^2 + c4*x^4 + c6*x^6
  let sols = solve(d(pot, x) == 0, x)
  say("stationary points: ", sols)
  # the broken minimum: the positive root with +sqrt(c4^2 - 3 c2 c6)
  let xmin = x /. sols[4]
  say("V at the minimum: ", simplify(pot /. x -> xmin))
  say("expansion about it: ", series(pot, x, xmin, 3))
})

# --- 2. the potential's coefficients, the broken minimum H_0(T), the depth V_0(T) and the critical temperature

c6 = 1/(8*cutoff^2)
fn c4(t) = lambda_h/4 + lambda1/4*t^2
fn c2(t) = -mu_h^2/2 + c_h/2*t^2
fn v_higgs(h, t) = c2(t)*h^2 + c4(t)*h^4 + c6*h^6
fn root_disc(t) = sqrt(c4(t)^2 - 3*c2(t)*c6)
# the depth of the broken minimum: zero at T_c
fn v_depth(t) = (-c4(t) + root_disc(t))*(-c4(t)^2 + 6*c2(t)*c6 + c4(t)*root_disc(t))/(27*c6^2)

stage_tc = timing(tc /. find_root(v_depth(tc), [tc, 10]))
t_c = stage_tc[2]
# H_0(T): the broken minimum below T_c, zero above (at T_c itself the step is undecided, as in the notebook)
fn h0_broken(t) = sqrt((-c4(t) + root_disc(t))/(3*c6))
fn h0(t) = h0_broken(t)*heaviside_theta(t_c - t)
say("T_c = ", t_c, " GeV, H_0(T_c) = ", h0_broken(t_c), " GeV")

# --- 3. thermal masses, decay widths and the graviton bremsstrahlung rate

# m_H^2 = d^2V/dH^2 at the minimum
fn m_higgs(t) = sqrt(2*c2(t) + 12*c4(t)*h0(t)^2 + 30*c6*h0(t)^4)
fn n_higgs(t) = (m_higgs(t)*t/(2*pi))^(3/2)*exp(-m_higgs(t)/t)
fn m_lepton(t) = sqrt((m_tau/v0*h0(t))^2 + (3/32*g2^2 + 1/32*g1^2)*t^2)
fn m_quark(t) = sqrt((m_b/v0*h0(t))^2 + (1/6*g3^2 + 3/32*g2^2 + 1/288*g1^2 + 1/16*y_top^2)*t^2)

# H -> b b
fn width_bb(t) = 3/(8*pi)*m_quark(t)^2/h0(t)^2*m_higgs(t)*max(0, 1 - (2*m_quark(t)/m_higgs(t))^2)^(3/2)
# H -> N tau, z = m_N/T
fn a_h(mn, z) = (m_higgs(mn/z)/mn)^2
fn a_l(mn, z) = (m_lepton(mn/z)/mn)^2
fn kallen(a, b, c) = (a - b - c)^2 - 4*b*c
fn width_ntau(mn, y, z) =
  mn/(8*pi)*y*sqrt(kallen(1, a_h(mn, z), a_l(mn, z)))*(a_h(mn, z) - a_l(mn, z) - 1)/(2*a_h(mn, z)^(3/2))
fn width_total(t) = width_ntau(40, 1.0e-3, 40/t) + width_bb(t)

# dGamma(H -> b b h)/d ln E_g at graviton energy eg, zero above the kinematic end m_H (1/2 - 2 y^2)
fn rate_bbh(eg, t) = {
  let mh_t = m_higgs(t)
  let x = eg/mh_t
  let y = m_quark(t)/mh_t
  if eg >= mh_t*(1/2 - 2*y^2) then 0.0 else {
    let al = sqrt(1 - 4*y^2/(1 - 2*x))
    y_b/(64*pi^3)*eg*(mh_t/mp)^2*((1 - 2*x)*al/x*(8*x*y^2 + 2*x*(x - 1) - 8*y^4 - 2*y^2 + 1) +
      4*y^2*((5 - 8*x)*y^2 - (x - 1)^2 - 4*y^4)/x*log((1 + al)/(1 - al)))
  }
}
# graviton energy at the decay for today's frequency f (Hz)
fn energy_of(f, t) = 2*pi*f*hz_to_gev*t/(xi*t_today)
# the gravitational-wave spectrum today from decays at temperature t
fn omega_gw(f, t) = 1/rho_c_over_h2*(xi*t_today/t)^4*2*pi*f*hz_to_gev*rate_bbh(energy_of(f, t), t)/width_total(t)*n_higgs(t)

stage_values = timing({
  say("kinematic end at T = 60 GeV: E_g < ", m_higgs(60)*(1/2 - 2*(m_quark(60)/m_higgs(60))^2), " GeV")
  say("its frequency today: ", n(12.32*xi*t_today/60*gev_to_hz), " Hz")
  for t in [10, 30, 60, 68] {
    say("T = ", t, ": m_H = ", m_higgs(t), ", m_tau = ", m_lepton(t), ", m_b = ", m_quark(t), ", Gamma_bb = ", width_bb(t),
        ", Gamma_Ntau = ", width_ntau(40, 1.0e-3, 40/t))
  }
  say("dGamma_bbh/dlnE / Gamma at f = 1e10 Hz, T = 30 GeV: ", rate_bbh(energy_of(1.0e10, 30), 30)/width_total(30))
  say("Omega_GW h^2 at f = 1e10 Hz, T = 30 GeV: ", omega_gw(1.0e10, 30))
})

# --- 4. the figures

ruby = "#e0115f"
olive = "#808000"
indigo = "#560591"
fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }

stage_plots = timing({
  save("higgs_potential",
       plot([v_higgs(h, 1.1*t_c)/1.0e6, v_higgs(h, t_c)/1.0e6, v_higgs(h, 0.9*t_c)/1.0e6], [h, 0, 250],
            colors: [ruby, olive, indigo], x_label: r"$H$ (GeV)", y_label: r"$V_T(H)\times 10^6$ GeV$^4$",
            legend: [r"$T = 1.1\,T_c$", r"$T = T_c$", r"$T = 0.9\,T_c$"], legend_title: r"$T_c = 68.35$ GeV",
            legend_position: top_left))
  # the thermal masses, with T_c marked by a grid line
  save("debye_masses",
       log_plot([m_higgs(t), m_lepton(t), m_quark(t)], [t, 0.01, 150], colors: [ruby, olive, indigo],
                x_label: r"$T$ (GeV)", y_label: r"$m$ (GeV)",
                legend: [r"$m_H(T)$", r"$m_\tau(T)$", r"$m_b(T)$"], legend_title: r"$T_c = 68.35$ GeV",
                legend_position: bottom_right, grid_lines: [[[t_c, rgb_color("#db7093")]], none],
                epilog: [rgb_color("#db7093"), text(r"$T_c$", [t_c + 6, log10(3.0)])]))
  save("total_decay_rates",
       plot([width_ntau(40, 1.0e-3, 40/t)/width_total(t), width_bb(t)/width_total(t)], [t, 0.01, 0.95*t_c],
            colors: [indigo, olive], x_label: r"$T$ (GeV)", y_label: "Branching fractions",
            legend: [r"$\mathrm{Br}_{H\to N\tau}$", r"$\mathrm{Br}_{H\to \bar{b}b}$"], legend_title: r"$m_N = 40$ GeV",
            legend_position: bottom_right))
  save("partial_decay_rates",
       log_plot(rate_bbh(energy_of(1.0e10, t), t)/width_total(t), [t, 0.01, t_c], colors: [indigo],
                x_label: r"$T$ (GeV)", y_label: r"$\frac{1}{\Gamma_{\mathrm{tot}}}\frac{d\Gamma^{1\to 3}}{d\ln E}$",
                legend: [r"$H\to \bar{b}bh$"], legend_title: r"$f = 10^{10}$ Hz", legend_position: bottom_left))
  save("omega_gw",
       log_log_plot(omega_gw(f, 30), [f, 1.0e4, 1.0e14], colors: [indigo], x_label: r"$f$ (Hz)",
                    y_label: r"$\Omega_{\mathrm{GW}}h^2$", legend: [r"$H\to \bar{b}bh$"], legend_title: r"$T_{\mathrm{decay}} = 30$ GeV",
                    legend_position: top_left))
})

stages = [stage_symbolic, stage_tc, stage_values, stage_plots]
say("stage times (s): symbolic ", stage_symbolic[1], ", T_c ", stage_tc[1], ", values ", stage_values[1], ", plots ",
    stage_plots[1], "; total ", sum(map(st => st[1], stages)))
