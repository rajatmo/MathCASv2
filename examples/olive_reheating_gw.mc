#! mathcas 2
# Gravitational waves from inflaton scattering during reheating: a check of a published estimate (a port of a
# research notebook).
#
# 1. The inflaton: the series of the Starobinsky and T-attractor potentials at the origin, the mass and the
#    self-coupling fixed by the CMB normalisation A_s = 2.1e-9 at N* = 55, and the field at the end of inflation.
# 2. The Boltzmann equations in the scale factor a (in units of a_end) for the inflaton (k = 2, w = 0), the
#    radiation and the gravitational waves, with the decay rate Gamma_phi fixed by T_RH = 10^10 GeV and the
#    graviton production rate Gamma_h = 2 rho_phi m_phi/(64 pi Mp^4), from rho_phi(1) = (5.5e15 GeV)^4 to a = 10^12;
#    the reheating point rho_phi = rho_R, the freeze-in point H = Gamma_phi and rho_GW there.
# 3. The frequency spectrum: the analytic integral of d rho/df over the band [f_e, f_RH] against rho_GW(a_RH), the
#    differential spectrum d rho/d ln f, the spectrum Omega_GW h^2 today from the numerical rho_GW(a_RH), and the
#    published formula for four reheating temperatures.
# Run: mc-repl examples/olive_reheating_gw.mc  (plots are written as PDF and PNG files in the current folder; the
# stage times are printed at the end).

fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }

# --- 1. the inflaton

stage_inflaton = timing({
  # Starobinsky and T-attractor potentials near the origin (the T-attractor for k = 2: series of tanh^k needs
  # symbolic powers of a series)   # GAP G10
  let v_star = 3/4*mphi^2*mp^2*(1 - exp(-sqrt(2/3)*phi/mp))^2
  let v_t = lam*mp^4*(sqrt(6)*tanh(phi/(sqrt(6)*mp)))^2
  say("Starobinsky V near 0: ", series(v_star, phi, 0, 3))
  say("T-attractor (k = 2) V near 0: ", series(v_t, phi, 0, 3))
  # the CMB normalisation: m_phi^2 = 24 pi^2 A_s Mp^2/N*^2, lam = 18 pi^2 A_s/(6^(k/2) N*^2)
  let cmb = [as -> 2.1e-9, ne -> 55, mp -> 2.4e18]
  say("m_phi = ", sqrt(24*pi^2*as/ne^2*mp^2) /. cmb, " GeV, lam(k = 4) = ", 18*pi^2*as/(6^(4/2)*ne^2) /. cmb)
  # the end of inflation: Starobinsky, T-attractor k = 2, T-attractor general k
  say("phi_end/Mp: Starobinsky ", n(sqrt(3/2)*log(2/11*(4 + 3*sqrt(3)))), ", k = 2 ",
      n(sqrt(3/2)*log(1/11*(14 + 5*sqrt(3)))), ", general k ", sqrt(3/8)*log(1/2 + k/3*(k + sqrt(k^2 + 3))))
})

# --- 2. the Boltzmann equations

mp = 2.4e18
g_rh = 427/4
g0 = 43/11
xi = n((g0/g_rh)^(1/3))
alpha = n(g_rh*pi^2/30)
m_phi = n(sqrt(24*pi^2*2.1e-9/55^2*mp^2))
t_rh = 1.0e10
rho_end = (5.5e15)^4
w = 0                                        # k = 2
gamma_phi = 5/(2*sqrt(3)*mp)*t_rh^2*sqrt(alpha)
say("xi = ", xi, ", alpha = ", alpha, ", Gamma_phi = ", gamma_phi, " GeV, Gamma_h/rho_phi = ", 2*m_phi/(64*pi*mp^4))

# The notebook integrates d rho/da from a = 1 to 10^12 at 200 digits. In machine precision the same equations are
# integrated in z = ln a with the densities in units that keep each of order one where it matters: rho_phi and
# rho_R in units of rho_end, rho_GW (some 40 orders smaller) in units of 10^-40 rho_end; rho_R starts at 0, so
# the absolute tolerance (accuracy_goal) applies in these units; rho_phi, which falls by e^-(Gamma t) after
# reheating, is integrated as its logarithm lp (no noise at the tolerance level once it is negligible).
gw_unit = 1.0e-40
fn hubble(lp, ur, ug) = sqrt(rho_end*(exp(lp) + ur + gw_unit*ug)/(3*mp^2))
gamma_h_per_rho = 2*m_phi/(64*pi*mp^4)
stage_solve = timing(n_dsolve([
    lp'(z) == -3*(1 + w) - gamma_phi/hubble(lp(z), ur(z), ug(z))*(1 + w),
    ur'(z) == -4*ur(z) + gamma_phi/hubble(lp(z), ur(z), ug(z))*(1 + w)*exp(lp(z)),
    ug'(z) == -4*ug(z) + gamma_h_per_rho*rho_end*exp(2*lp(z))/hubble(lp(z), ur(z), ug(z))*(1 + w)/gw_unit,
    lp(0) == 0, ur(0) == 0, ug(0) == 0], [lp, ur, ug], [z, 0, log(1.0e12)], precision_goal: 10, accuracy_goal: 14))
sol = stage_solve[2]
fn rho_phi(a) = rho_end*exp(lp(log(a))) /. sol
fn rho_r(a) = rho_end*ur(log(a)) /. sol
fn rho_gw(a) = rho_end*gw_unit*ug(log(a)) /. sol

# reheating (rho_phi = rho_R) and the freeze-in of the waves (H = Gamma_phi)
stage_points = timing({
  let zrh = z /. find_root((exp(lp(z)) - ur(z)) /. sol, [z, log(10.0)])
  let zfi = z /. find_root((hubble(lp(z), ur(z), ug(z)) - gamma_phi) /. sol, [z, zrh])
  [exp(zrh), exp(zfi), rho_gw(exp(zrh)), rho_gw(exp(zfi))]
})
a_rh = stage_points[2][1]
rho_gw_rh = stage_points[2][3]
say("a_RH = ", a_rh, ", a_GW,FI = ", stage_points[2][2], ", rho_GW(a_RH) = ", rho_gw_rh, ", rho_GW(a_FI) = ",
    stage_points[2][4])

indigo = "#560591"
olive = "#808000"
stage_plots1 = timing([
  save("olive_rho_gw", plot(rho_gw(a), [a, 1, 1.0e11], x_scale: log, y_scale: log, colors: [indigo],
                             x_label: r"$a$ (units of $a_e$)", y_label: r"$\rho$ (GeV$^4$)", legend: [r"$\rho_{GW}(a)$"])),
  save("olive_rho_r_phi", plot([rho_r(a), rho_phi(a)], [a, 1, 1.0e11], x_scale: log, y_scale: log,
                                colors: [indigo, olive], x_label: r"$a$ (units of $a_e$)", y_label: r"$\rho$ (GeV$^4$)",
                                legend: [r"$\rho_R(a)$", r"$\rho_\phi(a)$"], legend_position: bottom_left)),
  save("olive_a4_rho_gw", plot(a^4*rho_gw(a), [a, 1, 1.0e11], x_scale: log, y_scale: log, colors: [indigo],
                                x_label: r"$a$ (units of $a_e$)", y_label: r"$a^4 \rho$ ($a_e^4$ GeV$^4$)",
                                legend: [r"$a^4 \rho_{GW}(a)$"], legend_position: bottom_right))])

# --- 3. the frequency distribution

# the published spectrum d rho/df in the band [f_e, f_RH] (f_e = m_phi a_end/(2 pi a0), f_RH = m_phi a_RH/(2 pi a0))
# integrates to rho_GW(a_RH) up to the small term from the lower edge; then d rho/d ln f over rho_GW(a_RH)
stage_symbolic = timing(assuming(a0 > 0 and arh > 0 and aend > 0 and arh > aend and t0 > 0 and trh > 0 and al > 0 and
                                 m > 0 and x > 0 and mpl > 0 and rend > 0 and f > 0, {
  let drho_df = sqrt(3)*al^(3/2)*trh^6/(16*mpl^3)*(m/(2*pi*f))^(3/2)*(t0/trh)^(1/2)*sqrt(x)
  let rho_gw = sqrt(3)*aend^4*m*rend^(3/2)/(16*arh^4*mpl^3*pi)
  # x = (a_RH/a0)(T_RH/T0) (entropy conservation), T_RH^4 = (rho_end/alpha)(a_end/a_RH)^3
  let elim = e => rewrite(rewrite(e, x -> arh/a0*trh/t0), trh -> (rend/al*(aend/arh)^3)^(1/4))
  # the definite integral of a power with symbolic limits stays unevaluated, so the antiderivative is taken at
  # both ends   # GAP G31
  let anti = integrate(power_expand(drho_df), f)
  let band = rewrite(anti, f -> m/(2*pi)*arh/a0) - rewrite(anti, f -> m/(2*pi)*aend/a0)
  say("integral over the band: ", expand(simplify(elim(band))))
  say("rho_GW(a_RH):           ", rho_gw)
  say("(d rho/d ln f)/rho_GW:  ", simplify(elim(f*drho_df)/rho_gw))
  say("d rho/df:               ", simplify(elim(drho_df)))
  # the density x(f) that reproduces rho_GW(a_RH) as a band integral: d/d a_end of int_{c a_end}^{c a_RH} x(f) df
  # is -c x(c a_end) with c = m/(2 pi a0) (Leibniz' rule by hand), and a0 = a_end m/(2 pi f) puts f at the lower
  # edge   # GAP G9 (d of an integral with variable limits)
  let c = m/(2*pi*a0)
  say("x(f) = ", simplify(rewrite(-d(rho_gw, aend)/c, a0 -> aend*m/(2*pi*f))))
}))

# the numerical spectrum: factor (1/2) sqrt(f_e/f) times rho_GW(a_RH), cut below f_e and above f_RH, redshifted
ev_per_hz = 6.626e-34/1.602e-19                        # h/e
gev_to_hz = 1.0e9/ev_per_hz
t0_k = 6.626e-13                                       # T0 in GeV
fe = gev_to_hz*m_phi/(2*pi)*alpha^(1/3)*t_rh^(1/3)*xi*t0_k/rho_end^(1/3)
frh = gev_to_hz*xi*m_phi/(2*pi)*t0_k/t_rh
say("f_e = ", fe, " Hz, f_RH = ", frh, " Hz")
redshift = xi^4*(t0_k/t_rh)^4
density_factor = 1/8.1e-47                             # h^2/rho_c
fn omega(f) = density_factor*unit_step(f - fe)*exp(-5*sqrt(alpha)/4*f^2/frh^2)*redshift*1/2*sqrt(fe/f)*rho_gw_rh   # GAP G1
say("numerical prefactor: ", redshift*density_factor*rho_gw_rh/2, "; published estimate: ",
    redshift*density_factor*sqrt(3)*alpha^(4/3)*3.0e13*rho_end^(1/6)*t_rh^(16/3)/(32*pi*mp^3))

# the published formula (eq. 25) for T_RH = 10^10 ... 10^13 GeV with m_phi = 3e13 GeV
fn omega25(f, trh) = {
  let fe25 = gev_to_hz*3.0e13/(2*pi)*alpha^(1/3)*trh^(1/3)*xi*t0_k/rho_end^(1/3)
  let frh25 = gev_to_hz*xi*3.0e13/(2*pi)*t0_k/trh
  unit_step(f - fe25)*density_factor*sqrt(3)*alpha^(4/3)*trh^(16/3)*3.0e13*rho_end^(1/6)/(32*pi*mp^3)*sqrt(fe25/f)*
    (xi*t0_k/trh)^4*exp(-5*sqrt(alpha)/4*(f/frh25)^2)
}
stage_plots2 = timing([
  save("olive_omega_gw", plot(omega(f), [f, fe/100, 100*frh], x_scale: log, y_scale: log, colors: [indigo],
                               x_label: "f (Hz)", y_label: r"$d(\Omega_{GW} h^2)/d\ln f$")),
  save("olive_omega_gw_eq25",
       plot(map(tr => omega25(f, tr), [1.0e10, 1.0e11, 1.0e12, 1.0e13]), [f, 1.0e4, 1.0e16], x_scale: log,
            y_scale: log, x_label: "f (Hz)", y_label: r"$d(\Omega_{GW} h^2)/d\ln f$",
            legend: [r"$T_{RH} = 10^{10}$", r"$T_{RH} = 10^{11}$", r"$T_{RH} = 10^{12}$", r"$T_{RH} = 10^{13}$"],
            legend_position: bottom_left))])

stages = [stage_inflaton, stage_solve, stage_points, stage_plots1, stage_symbolic, stage_plots2]
say("stage times (s): inflaton ", stage_inflaton[1], ", Boltzmann equations ", stage_solve[1], ", reheating points ",
    stage_points[1], ", density plots ", stage_plots1[1], ", symbolic checks ", stage_symbolic[1], ", spectrum plots ",
    stage_plots2[1], "; total ", sum(map(st => st[1], stages)))
