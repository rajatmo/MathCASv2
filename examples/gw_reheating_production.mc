#! mathcas 2
# Gravitational waves produced during reheating by graviton bremsstrahlung of the decaying inflaton (a port of a
# research notebook).
#
# The inflaton potential (Starobinsky; the T-attractor family for comparison) fixes the inflaton mass m_phi from the
# amplitude of the CMB spectrum and the energy density at the end of inflation. Then the Boltzmann equations for the
# inflaton, radiation and graviton energy densities rho_phi, rho_R, rho_GW run in the scale factor a (a = 1 at the end
# of inflation) to a = 10^12, with the inflaton decay rate Gamma_phi fixed by T_RH = 10^10 GeV and the graviton
# production rate Gamma_h = rho_phi m_phi/(32 pi M_P^4). The script finds a_RH (where rho_phi = rho_R) and the
# graviton freeze-in point (H = Gamma_phi), checks the analytic spectrum of the paper the notebook follows
# (symbolically: its integral over frequency against rho_GW(a_RH), the factor turning rho_GW into a spectrum per
# ln f), and draws the densities and the spectra today (PDF and PNG files in the current folder).
# Run: mc-repl examples/gw_reheating_production.mc  (about a second; the stage times are printed at the end).

# --- 1. the inflaton potential, its mass and the energy density at the end of inflation

# Starobinsky, and the T-attractor of index k (here k = 2: GAP G10, the series for a symbolic power k)
say("Starobinsky V = ", series(3/4*m^2*mp^2*(1 - exp(-sqrt(2/3)*phi/mp))^2, phi, 0, 3))
say("T-attractor (k = 2) V = ", series(lam*mp^4*(sqrt(6)*tanh(phi/(sqrt(6)*mp)))^2, phi, 0, 3))

a_s = 2.1e-9         # amplitude of the scalar spectrum
n_e = 55             # e-folds
mp = 2.4e18          # reduced Planck mass (GeV)
m_phi = sqrt(24*pi^2*a_s/n_e^2*mp^2)
say("m_phi = ", m_phi, " GeV, lambda (k = 4) = ", 18*pi^2*a_s/(6^2*n_e^2))
fn v_starobinsky(phi) = 3/4*m_phi^2*mp^2*(1 - exp(-sqrt(2/3)*phi/mp))^2
phi_end = sqrt(3/2)*log(2/11*(4 + 3*sqrt(3)))*mp
rho_end = n(3/2*v_starobinsky(phi_end))
say("rho_phi at the end of inflation = ", rho_end, " GeV^4 (the paper takes (5.5e15)^4 = ", (5.5e15)^4, ")")

# --- 2. the Boltzmann equations, in u = ln a (the notebook writes them in a; with u the steps are even in log a)

g_rh = 427/4
g0 = 43/11
xi = (g0/g_rh)^(1/3)
alpha = g_rh*pi^2/30
t_rh = 1.0e10
gamma_phi = 5/(2*sqrt(3)*mp)*t_rh^2*sqrt(alpha)
fn gamma_h(rho_phi) = 2*rho_phi*m_phi/(64*pi*mp^4)
fn hubble(r1, r2, r3) = sqrt((r1 + r2 + r3)/(3*mp^2))
say("Gamma_phi = ", n(gamma_phi), " GeV, Gamma_h = ", gamma_h(1), " rho_phi")

# k = 2: w_phi = (k - 2)/(k + 2) = 0, the inflaton oscillates as matter
w_phi = 0
stage_solve = timing(n_dsolve([
    rphi'(u) == -3*(1 + w_phi)*rphi(u) - gamma_phi/hubble(rphi(u), rr(u), rgw(u))*(1 + w_phi)*rphi(u),
    rr'(u) == -4*rr(u) + gamma_phi/hubble(rphi(u), rr(u), rgw(u))*(1 + w_phi)*rphi(u),
    rgw'(u) == -4*rgw(u) + gamma_h(rphi(u))/hubble(rphi(u), rr(u), rgw(u))*(1 + w_phi)*rphi(u),
    rphi(0) == rho_end, rr(0) == 0, rgw(0) == 0],
  [rphi, rr, rgw], [u, 0, log(1.0e12)], precision_goal: 12, accuracy_goal: 40))
sol = stage_solve[2]
fn rho_phi(a) = rphi(log(a)) /. sol
fn rho_r(a) = rr(log(a)) /. sol
fn rho_gw(a) = rgw(log(a)) /. sol

# reheating (rho_phi = rho_R) and the graviton freeze-in (H = Gamma_phi)
stage_roots = timing([
  exp(u /. find_root((rphi(u) - rr(u)) /. sol, [u, log(10.0)])),
  exp(u /. find_root(hubble(rphi(u), rr(u), rgw(u)) - gamma_phi /. sol, [u, 16]))])
a_rh = stage_roots[2][1]
a_fi = stage_roots[2][2]
say("a_RH = ", a_rh, ", a_GW freeze-in = ", a_fi)
for a in [10, 1.0e3, 1.0e6, a_rh, 1.0e9, 1.0e11] {
  say("a = ", a, ": rho_phi = ", rho_phi(a), ", rho_R = ", rho_r(a), ", rho_GW = ", rho_gw(a))
}

# --- 3. the paper's analytic spectrum, checked symbolically

stage_symbolic = timing({
  # its spectrum per frequency and the graviton density at reheating (aend: the end of inflation, a0: today)
  let drho_df = sqrt(3)*al^(3/2)*trh^6/(16*mpl^3)*(m/(2*pi*f))^(3/2)*(t0/trh)^(1/2)*sqrt(x)
  let rho_gw_rh = sqrt(3)*aend^4*m*rhoe^(3/2)/(16*arh^4*mpl^3*pi)
  # x = (a_RH/a0)(T_RH/T0) and T_RH from rho_end (aend/a_RH)^3 = alpha T_RH^4
  let eliminate = [x -> arh/a0*trh/t0, trh -> (rhoe/al*(aend/arh)^3)^(1/4)]
  # the integral over f from m aend/(2 pi a0) to m a_RH/(2 pi a0): the antiderivative at both ends (GAP G29: the
  # definite integral with symbolic limits; GAP G31: the power of a quotient in f is split by power_expand first)
  let prim = integrate(power_expand(drho_df), f)
  let total = assuming(aend > 0 and arh > 0 and a0 > 0 and m > 0 and rhoe > 0 and al > 0 and mpl > 0,
                       simplify(power_expand(rewrite(rewrite((prim /. f -> m*arh/(2*pi*a0)) - (prim /. f -> m*aend/(2*pi*a0)),
                                                                 eliminate[1]), eliminate[2]))))
  say("integral of drho/df over the band = ", expand(total))
  say("rho_GW(a_RH)                      = ", rho_gw_rh)
  # the spectrum per ln f relative to rho_GW(a_RH): sqrt(aend m/(a0 f))/(2 sqrt(2 pi))
  say("(f drho/df)/rho_GW(a_RH) = ", assuming(aend > 0 and arh > 0 and a0 > 0 and m > 0 and rhoe > 0 and al > 0 and f > 0,
      simplify(power_expand(rewrite(rewrite(f*drho_df/rho_gw_rh, eliminate[1]), eliminate[2])))))
  # the integrand x(f) whose band integral is rho_GW(a_RH): differentiating in aend at fixed f = m aend/(2 pi a0)
  # (the Leibniz rule, written out: GAP G9) gives -x(f) f/aend = d rho_GW/d aend
  say("x(f) = ", simplify(-d(rho_gw_rh, aend)*aend/f))
})

# --- 4. the spectrum today

hbar = 6.626e-34
e_charge = 1.602e-19
gev_to_hz = 1.0e9*e_charge/hbar
t_today = 6.626e-13
rho_c_over_h2 = 8.1e-47
# the lowest and the reheating frequencies today
f_e = n(gev_to_hz*m_phi/(2*pi)*alpha^(1/3)*t_rh^(1/3)*xi*t_today/rho_end^(1/3))
f_rh = n(gev_to_hz*xi*m_phi/(2*pi)*t_today/t_rh)
redshift = n(xi^4*(t_today/t_rh)^4)
rho_gw_rh = rho_gw(a_rh)
say("f_e = ", f_e, " Hz, f_RH = ", f_rh, " Hz, redshift x h^2/rho_c = ", redshift/rho_c_over_h2)
say("rho_GW(a_RH) = ", rho_gw_rh, "; redshift x h^2/rho_c x rho_GW(a_RH)/2 = ", redshift/rho_c_over_h2*rho_gw_rh/2)
# Omega h^2 per ln f: rho_GW(a_RH) times (1/2) sqrt(f_e/f), cut below f_e and damped above f_RH
fn omega_numeric(f) = 1/rho_c_over_h2*unit_step(f - f_e)*exp(-5*sqrt(alpha)/4*(f/f_rh)^2)*redshift*sqrt(f_e/f)/2*rho_gw_rh  # GAP G1
for f in [1.0e7, 1.0e9, 1.0e11, 1.0e13] { say("Omega h^2 (f = ", f, " Hz) = ", omega_numeric(f)) }

# the paper's closed form (its equation 25) for T_RH = 1e10 ... 1e13 GeV, m_phi = 3e13 GeV, rho_end = (5.5e15)^4
fn omega_paper(f, trh) = {
  let me = 3.0e13
  let re = (5.5e15)^4
  let fe = gev_to_hz*me/(2*pi)*alpha^(1/3)*trh^(1/3)*xi*t_today/re^(1/3)
  let frh = gev_to_hz*xi*me/(2*pi)*t_today/trh
  unit_step(f - fe)/rho_c_over_h2*sqrt(3)*alpha^(4/3)*trh^(16/3)*me*re^(1/6)/(32*pi*mp^3)*(fe/f)^(1/2)*
    (xi*t_today/trh)^4*exp(-5*sqrt(alpha)/4*(f/frh)^2)
}
say("the paper's prefactor at T_RH = 1e10 GeV: ", n(redshift/rho_c_over_h2*sqrt(3)*alpha^(4/3)*3.0e13*(5.5e15)^(4/6)*
                                                     t_rh^(16/3)/(32*pi*mp^3)))

# --- 5. figures

fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }
stage_plots = timing({
  save("rho_gw", log_log_plot(rho_gw(a), [a, 1, 1.0e11], x_label: r"$a$ (units of $a_e$)", y_label: r"$\rho$ (GeV$^4$)",
                              legend: [r"$\rho_{\mathrm{GW}}(a)$"]))
  save("rho_r_phi", log_log_plot([rho_r(a), rho_phi(a)], [a, 1, 1.0e11], x_label: r"$a$ (units of $a_e$)",
                                 y_label: r"$\rho$ (GeV$^4$)", legend: [r"$\rho_R(a)$", r"$\rho_\phi(a)$"]))
  save("a4_rho_gw", log_log_plot(a^4*rho_gw(a), [a, 1, 1.0e11], x_label: r"$a$ (units of $a_e$)",
                                 y_label: r"$a^4\rho$ ($a_e^4$ GeV$^4$)", legend: [r"$a^4\rho_{\mathrm{GW}}(a)$"]))
  save("omega_gw_reheating", log_log_plot(omega_numeric(f), [f, f_e/100, 100*f_rh], x_label: r"$f$ (Hz)",
                                          plot_range: [[f_e/100, 100*f_rh], [1.0e-30, 1.0e-20]],
                                          y_label: r"$d(\Omega_{\mathrm{GW}}h^2)/d\ln f$"))
  save("omega_gw_paper", log_log_plot(map(trh => omega_paper(f, trh), [1.0e10, 1.0e11, 1.0e12, 1.0e13]), [f, 1.0e4, 1.0e16],
                                      plot_range: [[1.0e4, 1.0e16], [1.0e-30, 1.0e-15]],
                                      x_label: r"$f$ (Hz)", y_label: r"$d(\Omega_{\mathrm{GW}}h^2)/d\ln f$",
                                      legend: [r"$T_{\mathrm{RH}} = 10^{10}$", r"$T_{\mathrm{RH}} = 10^{11}$",
                                               r"$T_{\mathrm{RH}} = 10^{12}$", r"$T_{\mathrm{RH}} = 10^{13}$"],
                                      legend_position: bottom_left))
})

stages = [stage_solve, stage_roots, stage_symbolic, stage_plots]
say("stage times (s): Boltzmann equations ", stage_solve[1], ", roots ", stage_roots[1], ", symbolic ", stage_symbolic[1],
    ", plots ", stage_plots[1], "; total ", sum(map(st => st[1], stages)))
