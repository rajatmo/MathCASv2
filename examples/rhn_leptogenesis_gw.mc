#! mathcas 2
# Leptogenesis and gravitational waves from inflaton decay into right-handed neutrinos, in the scale factor a
# (a port of two research notebooks by the same group: the original and its tidied copy).
#
# 1. The Casas-Ibarra neutrino Yukawa matrix (oscillation data, complex angle z11 + i z21, heavy masses M1, 100 M1,
#    10^4 M1): H = Y^dagger Y, the light masses back from the seesaw, the flavour projections K_alpha, the CP
#    asymmetry eps of N1 decays and its width Gamma_N.
# 2. Leptogenesis: the inflaton (k = 2) decays into N1 pairs; N1 decays and inverse decays with the asymmetry eps
#    and the washout by the lepton equilibrium densities; the B-L asymmetry Y_B-L = (28/79) n_B-L/s late after
#    reheating (a from 1 to 10^16, comoving densities a^3 rho_phi, a^4 rho_R, a^3 n_N).
# 3. Gravitational waves: graviton bremsstrahlung in the inflaton scattering (Gamma_h = 2 m_phi rho_phi/(64 pi Mp^4))
#    and the decay phi -> N N h (spectrum in the graviton energy, open below m_phi/2), for y_phiNN = 10^-7 and 10^-5;
#    the reheating point, the scattering spectrum (the analytic shape with the computed a^4 rho_GW) and the decay
#    spectrum frequency by frequency, and their sum.
# Run: mc-repl examples/rhn_leptogenesis_gw.mc  (PDF and PNG files in the current folder; stage times at the end).
# The notebooks also scan Y_B-L over random angles and draw the contour Y_B-L = 8.73e-11 in (z11, z21); one point of
# that scan is computed here (`yield_bl`), a scan is a `table` over it.

# --- constants

mp = 2.4e18
vh = 246
gstar = 106
g_l = 2
g_n = 2
m_e = 0.511e-3
m_mu = 105.7e-3
m_tau = 1.78
k = 2
lam = 2/10^11
w = (k - 2)/(k + 2)

# --- 1. the Yukawa matrix

s12 = sqrt(0.304)
s23 = sqrt(0.573)
s13 = sqrt(0.02219)
c12 = sqrt(1 - 0.304)
c23 = sqrt(1 - 0.573)
c13 = sqrt(1 - 0.02219)
dcp = 1.09*pi
upmns = n([[c12*c13, s12*c13, s13*exp(-im*dcp)],
           [-s12*c23 - c12*s23*s13*exp(im*dcp), c12*c23 - s12*s23*s13*exp(im*dcp), s23*c13],
           [s12*s23 - c12*c23*s13*exp(im*dcp), -c12*s23 - s12*c23*s13*exp(im*dcp), c23*c13]])
fn sqrt_mnu(m) = diagonal_matrix([sqrt(m), sqrt(sqrt(7.42e-23 + m^2)), sqrt(sqrt(2.52e-21 + 7.42e-23 + m^2))])
fn heavy(m1) = [m1, 100*m1, 10^4*m1]
fn rmat(z11, z21) = {
  let th = z11 + im*z21
  [[0, cos(th), sin(th)], [0, -sin(th), cos(th)], [1, 0, 0]]
}
fn ynu(m1, z11, z21) = n(-im*sqrt(2)/vh*dot(upmns, sqrt_mnu(0), transpose(rmat(z11, z21)),
                                              diagonal_matrix(sqrt(heavy(m1)))))
fn h2(m1, z11, z21) = { let y = ynu(m1, z11, z21); dot(conjugate_transpose(y), y) }
# flavour projections K_alpha = |Y_alpha1|^2/H_11
fn kproj(m1, z11, z21) = { let y = ynu(m1, z11, z21); map(a => abs(y[a, 1])^2, [1, 2, 3])/re(h2(m1, z11, z21)[1, 1]) }
# the CP asymmetry of N1 decays, summed over the lepton flavours
fn loop_f(xx) = sqrt(xx)*(1 + 1/(1 - xx) - (1 + xx)*(1/xx - 1/(2*xx^2) + 1/(3*xx^3)))
fn eps_flavours(m1, z11, z21) = {
  let y = ynu(m1, z11, z21)
  let yd = conjugate_transpose(y)
  let hh = dot(yd, y)
  let ms = heavy(m1)
  map(a => sum(map(j => im_part(hh[1, j]*yd[1, a]*y[a, j])*loop_f(ms[j]^2/ms[1]^2)
                        + im_part(hh[j, 1]*yd[1, a]*y[a, j])/(1 - ms[j]^2/ms[1]^2), [2, 3])), [1, 2, 3])/
    (8*pi*re(hh[1, 1]))
}
fn eps_cp(m1, z11, z21) = sum(eps_flavours(m1, z11, z21))
fn gamma_n(m1, z11, z21) = abs(h2(m1, z11, z21)[1, 1])/(8*pi)*m1

stage_ci = timing({
  say("H(1e8, 1, 1) = ", h2(1.0e8, 1, 1))
  let yy = ynu(5.0e9, 2.83, 0.24)
  say("light masses back (eV): ", 1.0e9*abs(eigenvalues(vh^2/2*dot(yy, diagonal_matrix(1/heavy(5.0e9)), transpose(yy)))))
  say("K(1e8, 1, 1) = ", kproj(1.0e8, 1, 1), ", K(4.5e9, 2.83, 0.24) = ", kproj(4.5e9, 2.83, 0.24))
  let ef = eps_flavours(1.0e11, 2.5, 1.463)
  say("eps(1e11, 2.5 + 1.463 i) = ", sum(ef), "; e + mu, tau: ", [ef[1] + ef[2], ef[3]])
  say("Gamma_N(1e8, 2.5 + 1.463 i) = ", gamma_n(1.0e8, 2.5, 1.463), " GeV")
})

# --- Bessel K terms (as in examples/gw_rhn_reheating.mc: Abramowitz-Stegun 9.8.5-9.8.8, branch-free, so that the
# right-hand sides compile and stay finite at x = M1/T up to 10^13)   # GAP G38, G42
fn poly(c, v) = fold((acc, a) => acc*v + a, 0, reverse(c))
fn step(u) = (1 + u/(abs(u) + 1.0e-300))/2
fn i0s(x) = poly([1, 3.5156229, 3.0899424, 1.2067492, 0.2659732, 0.0360768, 0.0045813], (x/3.75)^2)
fn i1s(x) = x*poly([0.5, 0.87890594, 0.51498869, 0.15084934, 0.02658733, 0.00301532, 0.00032411], (x/3.75)^2)
fn k0s(x) = -log(x/2)*i0s(x) + poly([-0.57721566, 0.42278420, 0.23069756, 0.03488590, 0.00262698, 0.00010750,
                                     0.0000074], (x/2)^2)
fn xk1s(x) = x*log(x/2)*i1s(x) + poly([1, 0.15443144, -0.67278579, -0.18156897, -0.01919402, -0.00110404,
                                       -0.00004686], (x/2)^2)
fn k0l(x) = poly([1.25331414, -0.07832358, 0.02189568, -0.01062446, 0.00587872, -0.00251540, 0.00053208], 2/x)
fn k1l(x) = poly([1.25331414, 0.23498619, -0.03655620, 0.01504268, -0.00780353, 0.00325614, -0.00068245], 2/x)
fn log_k2(x) = {
  let xs = min(x, 2.0)
  let xl = max(x, 2.0)
  let s = step(x - 2)
  (1 - s)*log(k0s(xs) + 2*xk1s(xs)/xs^2) + s*(-xl - log(xl)/2 + log(k0l(xl) + 2*k1l(xl)/xl))
}
fn k_ratio(x) = {
  let xs = min(x, 2.0)
  let xl = max(x, 2.0)
  let s = step(x - 2)
  (1 - s)*(xk1s(xs)/xs)/(k0s(xs) + 2*xk1s(xs)/xs^2) + s*k1l(xl)/(k0l(xl) + 2*k1l(xl)/xl)
}
# log of x^2 K2(x)
fn log_x2k2(x) = 2*log(x) + log_k2(x)

# --- the inflaton

phi_end = sqrt(3/8)*mp*log(1/2 + k/3*(k + sqrt(k^2 + 3)))
e_end = n(3/2*lam*phi_end^k/mp^(k - 4))           # a^3 rho_phi at a_end = 1
m_phi = n(sqrt(k*(k - 1)*lam*mp^2))               # k = 2: constant
fn temp_of(lr, u) = (30/(pi^2*gstar))^(1/4)*exp(lr/4 - u)     # R = a^4 rho_R, u = ln a
log_y_end = n(log(1.0e16))
say("a^3 rho_phi(a_end) = ", e_end, " GeV^4, m_phi = ", m_phi, " GeV")

# --- 2. leptogenesis (the original notebook's first system): u = ln a; L_phi = ln(a^3 rho_phi), L_R = ln(a^4 rho_R),
# L_N = ln(a^3 n_N), and n_B-L a^3 = q. The notebook integrates a from 1 to 10^16 at 50 digits; here the logarithms
# keep the densities finite in machine precision; the solution stops once rho_phi and rho_N are below e^-100 of rho_R
# (they fall as e^-(Gamma t) afterwards). Washout: Y_N,eq/(2 Y_l,eq) with the three charged leptons.
fn yield_bl(y1, y2, mn, z11, z21) = {
  let gn = gamma_n(mn, z11, z21)
  let eps = eps_cp(mn, z11, z21)
  let a = exp(u)
  let en = sqrt(mn^2 + (m_phi^2/4 - mn^2)/a^2)                # N1 energy
  let hub = sqrt((exp(lp(u)) + exp(lr(u) - u) + en*exp(ln(u)))/(3*mp^2*a^3))
  let t = temp_of(lr(u), u)
  let x = mn/t
  let kr = k_ratio(x)
  let c = 45/(4*pi^4*gstar)
  # ln(a^3 n_eq) = ln(Y_N,eq s a^3) with s = (2 pi^2/45) 104 T^3
  let lneq = log(c*g_n) + log_x2k2(x) + log(2*pi^2/45*104) + 3*log(t) + 3*u
  # Y_N,eq/(2 Y_l,eq), each Y_l,eq through ln((m_l/T)^2 K2(m_l/T))
  let wash = exp(log(g_n) + log_x2k2(x) - log(2*g_l)
                 - log(exp(log_x2k2(m_e/t)) + exp(log_x2k2(m_mu/t)) + exp(log_x2k2(m_tau/t))))
  let gphi_nn = y1^2/(8*pi)*m_phi
  let gphi = (y1^2 + y2^2)/(8*pi)*m_phi
  let s = n_dsolve([
      lp'(u) == -(1 + w)*gphi/hub,
      lr'(u) == kr*gn/hub*(exp(ln(u)) - exp(lneq))*en*a/exp(lr(u)),
      ln'(u) == -kr*gn/hub*(1 - exp(lneq - ln(u))) + exp(lp(u) - ln(u))/(a^(3*w)*m_phi)*gphi_nn/hub*(1 + w),
      q'(u) == (-eps*(exp(ln(u)) - exp(lneq))*kr*gn - wash*kr*gn*q(u))/hub,
      lp(0) == log(e_end), lr(0) == log(e_end/m_phi/1.0e50), ln(0) == log(e_end/m_phi/1.0e20), q(0) == 1.0e5],
    [lp, lr, ln, q], [u, 0, log_y_end], events: [max(lp(u), ln(u) + log(en)) + u - lr(u) + 100 == 0],
    event_action: stop)
  # after the stop the inflaton and N1 are gone and the washout (Y_N,eq) is negligible: R and n_B-L a^3 stay
  let us = (lr /. s)[2][2]
  let yb = uu => {
    let uc = min(uu, us)
    let tt = temp_of(lr(uc) /. s, uu)
    28/79*(q(uc) /. s)/(exp(3*uu)*2*pi^2/45*104*tt^3)
  }
  [s, yb]
}

stage_lepto = timing(yield_bl(1.0e-5, 0, 1.0e8, 2.83, 0.5))
lep = stage_lepto[2]
say("leptogenesis (y_phiNN = 1e-5, M1 = 1e8, 2.83 + 0.5 i): eps = ", eps_cp(1.0e8, 2.83, 0.5), ", T(a = 1.5) = ",
    temp_of(lr(log(1.5)) /. lep[1], log(1.5)), " GeV, Y_B-L(10^12) = ", 79/28*lep[2](log(1.0e12)),
    " (without 28/79), Y_B-L(10^14) = ", lep[2](log(1.0e14)))

# --- 3. gravitational waves (the tidied notebook's system with Gamma_h): L_phi, L_R, L_N = ln(a^3 rho_N), and
# L_G = ln(a^4 rho_GW) from scattering; stops when rho_phi and rho_N are below e^-100 of rho_R (afterwards R and the
# waves are frozen).
mn = 1.0e11
z11 = 2.83
z21 = 0.24
gn_gw = gamma_n(mn, z11, z21)
fn hub_of(lp, lr, ln, u) = sqrt((exp(lp) + exp(lr - u) + exp(ln))/(3*mp^2*exp(3*u)))
fn waves(y1, y2) = {
  let a = exp(u)
  let hub = hub_of(lp(u), lr(u), ln(u), u)
  let t = temp_of(lr(u), u)
  let x = mn/t
  let kr = k_ratio(x)
  # ln(a^3 rho_N,eq) = ln(a^3 M^4/pi^2 (3 K2/x^2 + K1/x))
  let lreq = log(mn^4/pi^2) + log_k2(x) + log(3/x^2 + kr/x) + 3*u
  let gnn = y1^2/(8*pi)*m_phi
  let gff = y2^2/(8*pi)*m_phi
  let gh = 2*m_phi*exp(lp(u) - 3*u)/(64*pi*mp^4)
  let s = n_dsolve([
      lp'(u) == -3*w - (1 + w)*(gnn + gff + gh)/hub,
      lr'(u) == ((1 + w)*gff/hub*exp(lp(u)) + kr*gn_gw/hub*(exp(ln(u)) - exp(lreq)))*a/exp(lr(u)),
      ln'(u) == -kr*gn_gw/hub*(1 - exp(lreq - ln(u))) + (1 + w)*exp(lp(u) - ln(u))*gnn/hub,
      lg'(u) == (1 + w)*gh*exp(lp(u))*a/(hub*exp(lg(u))),
      lp(0) == log(e_end), lr(0) == log(1.0e20), ln(0) == log(1.0e20), lg(0) == log(e_end/1.0e30)],
    [lp, lr, ln, lg], [u, 0, log_y_end],
    events: [max(lp(u), ln(u)) + u - lr(u) + 100 == 0], event_action: stop)
  let fp = lp /. s
  let fr = lr /. s
  let fg = lg /. s
  # reheating: a^3 rho_phi/a^3 = a^4 rho_R/a^4
  let urh = uu /. find_root(fp(uu) + uu - fr(uu), [uu, log(10.0)])
  assoc("s" -> s, "urh" -> urh, "yrh" -> exp(urh), "trh" -> temp_of(fr(urh), urh),
        "rho_rh" -> exp(fr(urh) - 4*urh), "a4_gw" -> exp(fg(urh)), "y1" -> y1)
}
stage_waves = timing([waves(1.0e-7, 0), waves(1.0e-5, 0)])
wa = stage_waves[2][1]
wb = stage_waves[2][2]
for r in [wa, wb] {
  say("y_phiNN = ", r["y1"], ": a_RH = ", r["yrh"], ", T_RH = ", r["trh"], " GeV, rho_RH = ", r["rho_rh"],
      " GeV^4, a^4 rho_GW(a_RH)/a_RH^4 = ", r["a4_gw"]/r["yrh"]^4)
}

# today: Omega_GW (h^2 left out as in the notebook) from rho_GW(a_RH); frequencies f in Hz, energies E = 2 pi hbar f
hbar = 6.58e-25
om_r = 5.4e-5
rho_c = 8.0992e-47
rho_g = om_r*rho_c
t0 = 0.000232667860043e-9
# the graviton energy at a of a wave seen today at f, redshifted from reheating with rho_R(a_RH)/rho_gamma today
fn e_at(r, f, uu) = r["yrh"]/exp(uu)*2*pi*f*hbar*(r["rho_rh"]/rho_g)^(1/4)
# scattering: the analytic shape of the notebook (cut at f_e and f_RH) normalised by the computed rho_GW(a_RH)
fn omega_scat(r, f) = {
  let fgev = f*hbar
  let fe = m_phi/(2*pi)*0.332*t0/r["trh"]/(e_end/r["rho_rh"])^(1/3)
  let frh = m_phi/(2*pi)*0.332*t0/r["trh"]
  heaviside_theta(fgev - fe)/rho_c*r["a4_gw"]/r["yrh"]^4*sqrt(fe/fgev)*0.332^4*(t0/r["trh"])^4*
    exp(-5*sqrt(pi^2/30*106)/4*fgev^2/frh^2)
}
# decay phi -> N N h: d(a^4 rho)/du = (1 + w) dGamma/dE x_a a^3 rho_phi a/H while E < m_phi/2, a quadrature over the
# solution from a_end to a_RH (the notebook solves it as an ODE per frequency, seeded with a^4 rho = rho_end/10^50).
# The graviton energy at a is E(a) = (a_RH/a) E_RH (redshifted = true, as the notebook writes it); the notebook's
# stored spectra agree with E = E_RH at every a (redshifted = false: the a-dependence was lost in its session), so
# both are computed.
fn omega_decay(r, f, redshifted) = {
  let s = r["s"]
  let src = uu => {
    let ee = if redshifted then e_at(r, f, uu) else e_at(r, f, r["urh"])
    let xa = ee/m_phi
    let dg = r["y1"]^2/(64*pi^3)*(m_phi/mp)^2/xa*(1 - 4*xa + 6*xa^2 - 4*xa^3)
    heaviside_theta(m_phi/2 - ee)*(1 + w)*dg*xa*exp((lp(uu) /. s) + uu)/hub_of(lp(uu) /. s, lr(uu) /. s, ln(uu) /. s, uu)
  }
  # the decay is open from where E falls below m_phi/2 (E decreases as 1/a)
  let uo = if redshifted then max(log(e_at(r, f, 0)/(m_phi/2)), 0) else 0
  let a4 = e_end/1.0e50 + if uo < r["urh"] then n_integrate(src(uu), [uu, uo, r["urh"]]) else 0
  a4/r["yrh"]^4*2*pi*f*hbar*(r["rho_rh"]/rho_g)^(1/4)*om_r/r["rho_rh"]
}
lfs = join(range(5, 15), range(15.1, 15.6, 0.1), range(15.61, 15.68, 0.01))
lfs1 = join(range(5, 13), range(13.1, 13.5, 0.1), range(13.51, 13.7, 0.01))
stage_spectra = timing({
  let sd = map(lf => [lf, omega_decay(wa, 10.0^lf, true)], lfs)
  let sd1 = map(lf => [lf, omega_decay(wb, 10.0^lf, true)], lfs1)
  let ss = map(lf => [lf, omega_scat(wa, 10.0^lf)], lfs)
  let ss1 = map(lf => [lf, omega_scat(wb, 10.0^lf)], lfs1)
  [sd, sd1, ss, ss1]
})
sd = stage_spectra[2][1]
sd1 = stage_spectra[2][2]
ss = stage_spectra[2][3]
ss1 = stage_spectra[2][4]
say("decay, E redshifted: Omega(10^10 Hz) = ", omega_decay(wa, 1.0e10, true), " (y = 1e-7), ",
    omega_decay(wb, 1.0e10, true), " (y = 1e-5); Omega(10^13 Hz, y = 1e-5) = ", omega_decay(wb, 1.0e13, true))
say("decay, E = E_RH (the notebook's values): Omega(10^10 Hz) = ", omega_decay(wa, 1.0e10, false), " (y = 1e-7), ",
    omega_decay(wb, 1.0e10, false), " (y = 1e-5); Omega(10^13 Hz, y = 1e-5) = ", omega_decay(wb, 1.0e13, false))
say("scattering: Omega(10^10 Hz) = ", omega_scat(wb, 1.0e10), " (y = 1e-5); f where E_RH = m_phi/2: ",
    m_phi/2/(2*pi*hbar*(wb["rho_rh"]/rho_g)^(1/4)), " Hz")

# --- figures

blue = "#1f3fbf"
red = "#c0202a"
fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }
fn positive(pts) = select(pts, q => q[2] > 0)
stage_plots = timing([
  save("rhn_lepto_yield",
       plot(lep[2](log(a)), [a, 1, 1.0e14], x_scale: log, y_scale: log, colors: [blue], x_label: r"$a/a_{end}$",
            y_label: r"$Y_{B-L}$", grid_lines: [none, [8.7e-11]])),
  save("rhn_gw_densities",
       {
         let s = wb["s"]
         let us = range(0, 36.8, 0.05)
         let cut = uu => min(uu, (lp /. s)[2][2])
         list_log_log_plot(map(pts => positive(pts),
                              [map(uu => [exp(uu), if uu <= (lp /. s)[2][2] then exp((lp(uu) /. s) - 3*uu) else 0], us),
                               map(uu => [exp(uu), exp((lr(cut(uu)) /. s) - 4*uu)], us),
                               map(uu => [exp(uu), if uu <= (lp /. s)[2][2] then exp((ln(uu) /. s) - 3*uu) else 0], us)]),
                           joined: true, plot_range: [[1, 1.0e16], [1.0e-30, 1.0e62]], colors: [blue, red, "#808000"],
                           x_label: r"$a/a_{end}$", y_label: r"$\rho$ (GeV$^4$)",
                           legend: [r"$\rho_\phi$", r"$\rho_R$", r"$\rho_{N}$"], legend_position: bottom_left)
       }),
  save("rhn_gw_spectrum",
       list_log_plot(map(positive, [map(q => [q[1], q[2] + ss1[position(lfs1, q[1])[1][1]][2]], sd1),
                                    map(q => [q[1], q[2] + ss[position(lfs, q[1])[1][1]][2]], sd), ss1, ss, sd1, sd]),
                     joined: true, plot_range: [[5, 16], [1.0e-35, 1.0e-15]], colors: [blue, red, blue, red, blue, red],
                     line_style: [solid, solid, dotted, dotted, dashed, dashed], x_label: r"$\log_{10} f$",
                     y_label: r"$\Omega_{GW}$",
                     legend: [r"$T_{RH} = 4.18 \times 10^9$ GeV ($y_{\phi NN} = 10^{-5}$)",
                              r"$T_{RH} = 4.18 \times 10^7$ GeV ($y_{\phi NN} = 10^{-7}$)", "scattering", "", "decay"],
                     legend_position: top_left))])

stages = [stage_ci, stage_lepto, stage_waves, stage_spectra, stage_plots]
say("stage times (s): Yukawa matrix and asymmetry ", stage_ci[1], ", leptogenesis ", stage_lepto[1], ", reheating and waves ",
    stage_waves[1], ", spectra ", stage_spectra[1], ", plots ", stage_plots[1], "; total ", sum(map(st => st[1], stages)))
