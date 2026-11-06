#! mathcas 2
# Gravitational waves from reheating assisted by right-handed neutrinos (a port of two research notebooks: the
# paper's plots and the working notebook they grew from).
#
# After inflation the inflaton (V = lam Mp^4 (phi/Mp)^k, an oscillating condensate with equation of state
# w = (k - 2)/(k + 2)) decays through Yukawa couplings into right-handed neutrinos N1 (coupling y_phiNN) and into
# radiation (y_phiff); N1 decays into leptons and Higgs with the rate fixed by the Casas-Ibarra parametrisation of the
# neutrino Yukawa matrix (light masses and mixing from oscillation data, complex angle z11 + i z21, heavy masses
# M1, h M1, h^2 M1). The script
# 1. solves the Boltzmann equations for Y_phi = ln(a^3 rho_phi), Y_N = ln(a^3 n_N), Y_R = ln(a^4 rho_R) in
#    z = ln(a/a_end), and finds reheating (rho_phi or rho_N equal to rho_R), T_max and T_RH;
# 2. the gravitational waves from inflaton scattering (graviton bremsstrahlung of the condensate): rho_GW(a_RH) and
#    the spectrum Omega_GW h^2(f) today (k = 2; the working notebook's k = 4 and 6 benchmarks are not ported: with
#    the bare N1 mass of the paper the inflaton stops decaying once its frequency falls below 2 M1 and the
#    reheating condition is never met);
# 3. the waves from the inflaton decay phi -> N N h at each frequency (the decay is kinematically open while
#    E_g/m_phi + 2 (M1/m_phi)^2 < 1/2), and from the decay of N1 itself;
# 4. the paper's figures: densities and temperature for the benchmarks BP1-BP3, the allowed region in the
#    frequency-scale factor plane, T_max and T_RH against M1, the combined spectra, and the N1-decay spectra.
# Run: mc-repl examples/gw_rhn_reheating.mc  (about 25 s; PDF and PNG files in the current folder; stage times at
# the end).
# The notebooks also overlay detector sensitivity curves digitised from other papers; those data files are not part
# of this example (pass them as extra data sets to the spectrum plots).

# --- constants (GeV units)

mp = 2.4e18                       # reduced Planck mass
vh = 246                          # Higgs vacuum expectation value
g_n = 2                           # N1 degrees of freedom
gstar = 427/4
g0 = 43/11
t0 = 6.626e-13                    # CMB temperature today
rho_c_h2 = 8.1e-47                # critical density over h^2
xi = n((g0/gstar)^(1/3))
alpha = n(gstar*pi^2/30)
gev_to_hz = 1.0e9*1.602e-19/6.626e-34
hz_to_gev = 1/gev_to_hz
z_end = n(log(1.0e20))            # the notebooks integrate to a/a_end = 10^20

# --- the inflaton

fn w_phi(k) = (k - 2)/(k + 2)
fn phi_end(k) = sqrt(3/8)*mp*log(1/2 + k/3*(k + sqrt(k^2 + 3)))
fn rho_phi_end(k, lam) = 3/2*lam*mp^4*(phi_end(k)/mp)^k
# the mass from V'' at the amplitude where V = rho_phi, and the oscillation frequency of the condensate
fn m_phi(k, lam, rho) = sqrt(k*(k - 1)*lam*mp^2*(rho/(lam*mp^4))^((k - 2)/k))
fn omega_phi(k, lam, rho) = m_phi(k, lam, rho)*sqrt(pi/2)*k*gamma(1/2 + 1/k)/(gamma(1/k)*sqrt(k*(k - 1)))
# the scattering coefficients Sigma_k of the graviton production rate
sigma = assoc(2 -> 0.125, 4 -> 0.141, 6 -> 0.146)

# --- the neutrino Yukawa matrix (Casas-Ibarra) and the N1 width

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
mnu = n([0, sqrt(7.42e-23), sqrt(2.52e-21 + 7.42e-23)])     # light masses (GeV), m1 = 0
fn rmat(z11, z21) = {
  let th = z11 + im*z21
  [[0, cos(th), sin(th)], [0, -sin(th), cos(th)], [1, 0, 0]]
}
fn y_nu(m1, z11, z21, h) = n(-im*sqrt(2)/vh*dot(upmns, diagonal_matrix(sqrt(mnu)), transpose(rmat(z11, z21)),
                                                diagonal_matrix(sqrt([m1, h*m1, h^2*m1]))))
fn h2_11(m1, z11, z21, h) = { let y = y_nu(m1, z11, z21, h); re(dot(conjugate_transpose(y), y)[1, 1]) }
fn gamma_n(m1, h, z11, z21) = h2_11(m1, z11, z21, h)/(8*pi)*m1

# --- Bessel K terms of the N1 equilibrium densities
# K0, K1 by the polynomial approximations of Abramowitz-Stegun 9.8.5-9.8.8 (relative error below 3e-7), both
# ranges at clamped arguments: log K2 and K1/K2 stay finite for x = M1/T up to 10^13 in machine precision, and the
# right-hand side compiles   # GAP G38 (bessel_k in compiled code), G42 (bessel_k_scaled in n_dsolve)
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
  let w = step(x - 2)
  (1 - w)*log(k0s(xs) + 2*xk1s(xs)/xs^2) + w*(-xl - log(xl)/2 + log(k0l(xl) + 2*k1l(xl)/xl))
}
fn k_ratio(x) = {
  let xs = min(x, 2.0)
  let xl = max(x, 2.0)
  let w = step(x - 2)
  (1 - w)*(xk1s(xs)/xs)/(k0s(xs) + 2*xk1s(xs)/xs^2) + w*k1l(xl)/(k0l(xl) + 2*k1l(xl)/xl)
}

# --- rates

fn gamma_yuk(k, lam, mn, y, rho) = {
  let om = omega_phi(k, lam, rho)
  y^2/(8*pi)*om*(k + 2)*(k - 1)*(om/m_phi(k, lam, rho))^2*max(1 - 4*mn^2/om^2, 0)^(3/2)
}
# energy of an N1 from a decay at rest of the condensate (energy m/2 at a_end, redshifted towards M1)
fn e_n(mn, m, z) = sqrt(mn^2 + exp(-2*z)*(m^2/4 - mn^2))
fn temp(yr, z) = (30/(gstar*pi^2))^(1/4)*exp(yr/4 - z)

# --- 1. the Boltzmann equations
#
# P = [k, lam, M1, h, y_phiNN, y_phiff, z11, z21]. The notebooks solve to z = ln 10^20 at 60-100 digits; after
# reheating the inflaton and N1 fall by factors e^-10^6 and more, so here the solution stops (an event) once both
# are below e^-100 of the radiation, and for larger z rho_phi = rho_N = 0 and Y_R keeps its last value.
fn reheat(p) = {
  let k = p[1]
  let lam = p[2]
  let mn = p[3]
  let gn = gamma_n(mn, p[4], p[7], p[8])
  let w = w_phi(k)
  let rp = exp(yp(z) - 3*z)
  let rr = exp(yr(z) - 4*z)
  let om = omega_phi(k, lam, rp)
  let en = e_n(mn, om, z)
  let hub = sqrt(rp + rr + en*exp(yn(z) - 3*z))/(sqrt(3)*mp)
  let x = mn/temp(yr(z), z)
  let kr = k_ratio(x)
  # log(n_eq/n_N) and log(rho_eq/rho_N): finite where both densities underflow
  let lneq = log(g_n*mn^3/(2*pi^2)) + log_k2(x) - log(x) - yn(z) + 3*z
  let lreq = lneq + log(1 + kr) - log(en)
  let s = n_dsolve([
      yp'(z) == -((gamma_yuk(k, lam, mn, p[5], rp) + gamma_yuk(k, lam, mn, p[6], rp))/hub*(1 + w) + 3*w),
      yn'(z) == -(1 - exp(lneq))*gn/hub*kr + (1 + w)*gamma_yuk(k, lam, mn, p[5], rp)/hub*exp(yp(z) - yn(z))/om,
      yr'(z) == (1 - exp(lreq))*en*exp(yn(z) - yr(z) + z)*gn/hub*kr
                + (1 + w)*gamma_yuk(k, lam, mn, p[6], rp)/hub*exp(yp(z) - yr(z) + z),
      yp(0) == n(log(rho_phi_end(k, lam))), yn(0) == 0, yr(0) == 0], [yp, yn, yr], [z, 0, z_end],
    events: [max(yp(z), yn(z) + log(en)) - yr(z) + z + 100 == 0], event_action: stop)
  let fp = yp /. s
  let fnn = yn /. s
  let fr = yr /. s
  let zs = fr[2][2]
  # reheating: rho_phi = rho_R or rho_N = rho_R, whichever is later; T_max where dT/dz = 0
  let z1 = z /. find_root(fp(z) - fr(z) + z, [z, 10])
  let z2 = z /. find_root(fnn(z) + log(e_n(mn, m_phi(k, lam, exp(fp(z) - 3*z)), z)) - fr(z) + z, [z, 10])
  let zrh = max(if is_number(z2) then z2 else 0, z1)
  # T_max: the largest T on a grid, refined where dT/dz = 0
  let zg = max_by(range(0, min(zrh, zs), 0.02), u => fr(u)/4 - u)
  let zr = z /. find_root(d(fr(z), z) - 4, [z, zg])
  let zmax = if is_number(zr) and abs(zr - zg) < 0.05 then zr else zg
  assoc("p" -> p, "yp" -> fp, "yn" -> fnn, "yr" -> fr, "zs" -> zs, "zrh" -> zrh, "zmax" -> zmax,
        "tmax" -> temp(fr(zmax), zmax), "trh" -> temp(fr(zrh), zrh), "rho_rh" -> exp(fr(zrh) - 4*zrh))
}
# the solution at any z in [0, ln 10^20]
fn y_phi(r, z: Real) = if z <= r["zs"] then r["yp"](z) else -1.0e300
fn y_n(r, z: Real) = if z <= r["zs"] then r["yn"](z) else -1.0e300
fn y_r(r, z: Real) = r["yr"](min(z, r["zs"]))
fn rho_phi(r, z: Real) = exp(y_phi(r, z) - 3*z)
fn rho_r(r, z: Real) = exp(y_r(r, z) - 4*z)
fn rho_n(r, z: Real) = {
  let p = r["p"]
  e_n(p[3], m_phi(p[1], p[2], rho_phi(r, z)), z)*exp(y_n(r, z) - 3*z)
}
fn t_of(r, z: Real) = temp(y_r(r, z), z)
fn hubble(r, z: Real) = {
  let p = r["p"]
  let rp = rho_phi(r, z)
  sqrt(rp + rho_r(r, z) + e_n(p[3], omega_phi(p[1], p[2], rp), z)*exp(y_n(r, z) - 3*z))/(sqrt(3)*mp)
}

# --- 2. waves from inflaton scattering
# d ln(a^4 rho_GW)/dz = Sigma_k omega rho_phi^2/(4 pi Mp^4 H rho_GW) with a^4 rho_GW = 1 at z = 0 is a quadrature:
# a^4 rho_GW(z) = 1 + int_0^z Sigma_k omega e^(2 Y_phi - 2z')/(4 pi Mp^4 H) dz'
fn scattering(r) = {
  let p = r["p"]
  let k = p[1]
  let lam = p[2]
  let zrh = r["zrh"]
  let q = n_integrate(sigma[k]/(4*pi)*omega_phi(k, lam, rho_phi(r, u))*exp(2*y_phi(r, u) - 2*u)/(mp^4*hubble(r, u)),
                      [u, 0, zrh])
  let trh = r["trh"]
  let fe = exp(-zrh)*xi*t0/trh*omega_phi(k, lam, rho_phi(r, 0))/(2*pi)*gev_to_hz
  let frh = xi*t0/trh*omega_phi(k, lam, rho_phi(r, zrh))/(2*pi)*gev_to_hz
  assoc("rho_gws_rh" -> (1 + q)*exp(-4*zrh), "fe" -> fe, "frh" -> frh)
}
# Omega_GW h^2 per ln f today; k = 2: a peak shape (1/2) sqrt(f_e/f) cut at f_e and f_RH
fn omega_s2(r, g, f) = 1/rho_c_h2*(xi*t0/r["trh"])^4*heaviside_theta(f - g["fe"])*
                       exp(-5/4*sqrt(alpha)*(f/g["frh"])^2)*1/2*sqrt(g["fe"]/f)*g["rho_gws_rh"]
# --- 3. waves from the decays phi -> N N h and N1 -> l H h, frequency by frequency
# the graviton energy at z of a wave seen today at f
fn e_g(r, f, z) = 2*pi*f*hz_to_gev*r["trh"]/(xi*t0)*exp(r["zrh"] - z)
# the decay of the inflaton: open where E_g/m_phi + 2 (M1/m_phi)^2 < 1/2; a^4 rho_GW starts at 10^5 where it
# opens (as in the notebook) and grows by the source, a quadrature from z_D to z_RH
fn decay_point(r, f) = {
  let p = r["p"]
  let k = p[1]
  let lam = p[2]
  let mn = p[3]
  let zrh = r["zrh"]
  let mph = u => m_phi(k, lam, rho_phi(r, u))
  let zd = max(z /. find_root(e_g(r, f, z)/mph(z) + 2*(mn/mph(z))^2 - 1/2, [z, 1]), 0)
  if zd <= zrh then {
    let src = u => {
      let m = mph(u)
      let xg = e_g(r, f, u)/m
      let dg = p[5]^2/(64*pi^3)*(m/mp)^2*m*(1 - 4*xg + 6*xg^2 - 4*xg^3)
      heaviside_theta(1/2 - (xg + 2*(mn/m)^2))*xg/hubble(r, u)*dg*(1 + w_phi(k))*exp(y_phi(r, u) + u)
    }
    let a4 = 1.0e5 + n_integrate(src(u), [u, zd, zrh])
    1/rho_c_h2*(xi*t0/r["trh"])^4*a4*exp(-4*zrh)
  } else {
    let td = t_of(r, zd)
    1/rho_c_h2*(xi*t0/td)^4*2*pi*f*hz_to_gev*td/(xi*t0)*1.0e5*exp(-4*zd)
  }
}
# the decay of N1: open where E_g < M1 (x_n < 1)
fn rhn_decay_point(r, f) = {
  let p = r["p"]
  let k = p[1]
  let lam = p[2]
  let mn = p[3]
  let zrh = r["zrh"]
  let hh = h2_11(mn, p[7], p[8], p[4])
  let zd = max(z /. find_root(e_g(r, f, z)/mn - 1, [z, 10]), 0)
  if zd <= zrh then {
    let src = u => {
      let eg = e_g(r, f, u)
      let xn = eg/mn
      let dg = max(mn^2*hh/(768*mp^2*pi^3*xn)*(2 - xn)*(1 - 2*xn)^2, 0)
      dg/hubble(r, u)*e_n(mn, m_phi(k, lam, rho_phi(r, u)), u)/mn*eg*exp(y_n(r, u) + u)
    }
    let a4 = 1 + n_integrate(src(u), [u, zd, zrh])
    1/rho_c_h2*(xi*t0/r["trh"])^4*2*pi*f*hz_to_gev*r["trh"]/(xi*t0)*a4*exp(-4*zrh)
  } else {
    let td = t_of(r, zd)
    1/rho_c_h2*(xi*t0/td)^4*2*pi*f*hz_to_gev*td/(xi*t0)*exp(-4*zd)
  }
}
# the frequency grid of the notebooks: decades from 10^e1 to 10^e2, then steps 0.1, 0.01 and 0.1 in the exponent
fn freq_grid(e) = unique(n(join(table(10^u, u, range(e[1], e[2], 1)), table(10^u, u, range(e[2], e[3], 0.1)),
                                table(10^u, u, range(e[3], e[4], 0.01)), table(10^u, u, range(e[4], e[5], 0.1)))))
# scattering and decay spectra on the grid; above f_nominal the scattering part is kept only where it is below
# the decay part (the notebook's filter), and the sum is formed the same way
fn spectra(r, g, e, fnom) = {
  let fs = freq_grid(e)
  let s = map(f => [f, omega_s2(r, g, f)], fs)
  let dd = map(f => [f, decay_point(r, f)], fs)
  let keep = map_thread((a, b) => a[1] <= fnom or a[2] > b[2], [dd, s])
  assoc("s" -> s, "d" -> dd,
        "s_filtered" -> map_thread((a, b, kp) => [a[1], if kp then b[2] else 0], [dd, s, keep]),
        "sum" -> map_thread((a, b, kp) => [a[1], if kp then a[2] + b[2] else a[2]], [dd, s, keep]))
}

# --- the benchmarks

bp1 = [2, 2/10^11, 1.0e11, 100, 1.0e-6, 0, 2.83, 0.0035]
bp2 = [2, 2/10^11, 1.0e10, 1000, 1.0e-6, 0, 2.83, 0.006]
bp3 = [2, 2/10^11, 1.0e9, 1.0e5, 1.0e-6, 0, 2.83, 0.012]

say("Y_phi(0) = ", n(log(rho_phi_end(2, 2/10^11))), ", m_phi(k = 2) = ", m_phi(2, 2/10^11, 1.0), " GeV")
say("Gamma_N(M1 = 1e11, BP1) = ", gamma_n(1.0e11, 100, 2.83, 0.0035), " GeV")

stage_reheat = timing(map(reheat, [bp1, bp2, bp3]))
r1 = stage_reheat[2][1]
r2 = stage_reheat[2][2]
r3 = stage_reheat[2][3]
for r in [r1, r2, r3] {
  say("M1 = ", r["p"][3], ": z_max = ", r["zmax"], ", T_max = ", r["tmax"], ", z_RH = ", r["zrh"], ", rho_RH = ",
      r["rho_rh"], ", T_RH = ", r["trh"])
}
say("BP1 [Y_phi, Y_N, Y_R] at z = 1, 10: ", [y_phi(r1, 1.0), y_n(r1, 1.0), y_r(r1, 1.0)], ", ",
    [y_phi(r1, 10.0), y_n(r1, 10.0), y_r(r1, 10.0)])

stage_scatter = timing(map(scattering, [r1, r2, r3]))
g1 = stage_scatter[2][1]
g2 = stage_scatter[2][2]
g3 = stage_scatter[2][3]
say("BP1 scattering: rho_GW(a_RH) = ", g1["rho_gws_rh"], " GeV^4, f_e = ", g1["fe"], " Hz, f_RH = ", g1["frh"],
    " Hz; Omega h^2 at 10^8, 10^12, 10^13 Hz: ", map(f => omega_s2(r1, g1, f), [1.0e8, 1.0e12, 1.0e13]))

stage_spectra = timing([spectra(r1, g1, [0, 14, 15, 15.2, 16], 1.0e12), spectra(r2, g2, [0, 13.5, 14.2, 14.5, 16], 1.0e12),
                        spectra(r3, g3, [0, 14, 15, 15.2, 16], 1.0e12)])
sp1 = stage_spectra[2][1]
sp2 = stage_spectra[2][2]
sp3 = stage_spectra[2][3]
say("BP1 decay spectrum (", length(sp1["d"]), " frequencies) at 1, 10^4, 10^9, 10^14 Hz: ",
    map(i => sp1["d"][i], [1, 5, 10, 15]))

# T_max and T_RH against M1 (BP1 otherwise); for M1 = 10^13 GeV > m_phi/2 the inflaton cannot decay into N1 pairs
# and (with y_phiff = 0) never reheats: that point is left out (the notebook prints a meaningless T there)
stage_scan = timing(drop_failures(table({
  let r = reheat([2, 2/10^11, 10.0^e, 100, 1.0e-6, 0, 2.83, 0.0035])
  [10.0^e, r["tmax"], r["trh"]]
}, e, range(4, 12))))
scan = stage_scan[2]
say("M1, T_max, T_RH: ", scan)

# the N1 decay spectra for M1 = 10^12, 10^11 (theta = 2.83 + 0.0035 i) and 10^9 GeV (2.83 + 0.012 i)
stage_rhn = timing({
  let rr12 = reheat([2, 2/10^11, 1.0e12, 100, 1.0e-6, 0, 2.83, 0.0035])
  let rr9 = reheat([2, 2/10^11, 1.0e9, 100, 1.0e-6, 0, 2.83, 0.012])
  let ir = (r, e) => map(f => [f, rhn_decay_point(r, f)], freq_grid(e))
  [ir(rr12, [4, 11, 12, 12.7, 13]), ir(r1, [4, 11, 12, 12.7, 13]), ir(rr9, [4, 10, 11, 11.5, 12]),
   rr12["trh"], rr9["trh"]]
})
ir12 = stage_rhn[2][1]
ir11 = stage_rhn[2][2]
ir9 = stage_rhn[2][3]
trh12 = stage_rhn[2][4]
trh9 = stage_rhn[2][5]
say("N1 decay: T_RH(M1 = 1e12) = ", trh12, ", T_RH(M1 = 1e9) = ", trh9, "; M1 = 1e11 spectrum at points 1, 5, 8: ",
    map(i => ir11[i], [1, 5, 8]))

# --- 4. figures

ruby = "#e0115f"
burgundy = "#800020"
olive = "#808000"
indigo = "#560591"
pale_violet = "#db7093"
fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }

# the curves of the solution are tabulated on a grid in z and drawn as list plots (the accessors above branch on z,
# which the adaptive sampling of plot would call thousands of times); points and labels of the epilog sit at
# log10 of the value on a log axis (GAP G44). Labels: `\ln A` for the notebooks' `\ln \mathcal{A}`   # GAP G43 (\mathcal)
zgrid = range(0, 45, 0.05)
# the densities and T of a solution on the grid, evaluated on whole lists (an interpolating function takes a list
# of times): [rho_phi, rho_N, rho_R, T], each a list of [z, value] with positive values
fn curves(r) = {
  let p = r["p"]
  let zin = select(zgrid, u => u <= r["zs"])
  let yp_v = r["yp"](zin)
  let rp = exp(yp_v - 3*zin)
  let rn = e_n(p[3], m_phi(p[1], p[2], rp), zin)*exp(r["yn"](zin) - 3*zin)
  let yr_v = r["yr"](map(u => min(u, r["zs"]), zgrid))
  let pairs = (xs, vs) => select(transpose([xs, vs]), q => q[2] > 0)
  [pairs(zin, rp), pairs(zin, rn), pairs(zgrid, exp(yr_v - 4*zgrid)), pairs(zgrid, temp(yr_v, zgrid))]
}
pale_violet_c = rgb_color([219, 112, 147]/255)
burgundy_c = rgb_color([128, 0, 32]/255)
fn density_plot_of(r, cv, name, title, zline) = save(name,
  list_log_plot([cv[1], cv[2], cv[3]], joined: true,
                plot_range: [[0, 45], [1, 1.0e70]], colors: [indigo, ruby, olive], x_label: r"$\ln A$",
                y_label: r"$\rho$ (GeV$^4$)", legend: [r"$\rho_\phi$", r"$\rho_{N_1}$", r"$\rho_R$"], legend_title: title,
                grid_lines: [[[zline, pale_violet_c]], none],
                epilog: [point_size(0.015), burgundy_c, point([r["zrh"], log10(r["rho_rh"])])]))
fn temperature_plot_of(r, cv, name, title, zline) = save(name,
  list_log_plot(cv[4], joined: true, plot_range: [[0, 45], [1, 1.0e11]], colors: [indigo],
                x_label: r"$\ln A$", y_label: "T (GeV)", plot_label: title, grid_lines: [[[zline, pale_violet_c]], none],
                epilog: [point_size(0.015), burgundy_c, point([r["zrh"], log10(r["trh"])])]))

stage_plots = timing([
  # densities and temperature for BP1 and BP3, with the reheating point
  {
    let cv1 = curves(r1)
    let cv3 = curves(r3)
    [density_plot_of(r1, cv1, "rhn_densities_bp1", r"BP1 ($M_1 = 10^{11}$ GeV)", 4.3),
     temperature_plot_of(r1, cv1, "rhn_temperature_bp1", r"BP1 ($M_1 = 10^{11}$ GeV)", 4.3),
     density_plot_of(r3, cv3, "rhn_densities_bp3", r"BP3 ($M_1 = 10^{9}$ GeV)", 8.5),
     temperature_plot_of(r3, cv3, "rhn_temperature_bp3", r"BP3 ($M_1 = 10^{9}$ GeV)", 8.5)]
  },
  # the allowed region in the frequency-scale factor plane (BP1): f_min from H, f_max from the decay kinematics, the
  # region between them before reheating filled; lines at reheating, where rho_phi has fallen by 10^20, and 1 Hz
  {
    let mph = m_phi(2, 2/10^11, 1.0)
    let fdmin = u => hubble(r1, u)/(2*pi)*exp(r1["zrh"] - u)*xi*t0/r1["trh"]*gev_to_hz
    let fdmax = u => (1/2 - 2*(1.0e11/mph)^2)*mph/(2*pi*hz_to_gev)*xi*t0/r1["trh"]*exp(u - r1["zrh"])
    let z20 = z /. find_root(r1["yp"](z) - 3*z - (r1["yp"](r1["zrh"]) - 3*r1["zrh"]) + log(1.0e20), [z, r1["zrh"]])
    let zz = range(10, 22, 0.02)
    save("rhn_frequency_plane",
         list_log_plot([map(u => [u, fdmin(u)], zz), map(u => [u, fdmax(u)], zz),
                        map(u => [u, fdmax(u)], select(zz, u => u <= r1["zrh"]))], joined: true,
                       plot_range: [[10, 22], [0.1, 1.0e22]], filling: [1 -> [3]], colors: [indigo, olive, olive],
                       line_style: [dashed, solid, solid], x_label: r"$\ln A$", y_label: r"$f^0$ (Hz)",
                       legend: [r"$f^0_{min}$", r"$f^0_{max}$"], legend_title: r"$M_1 = 10^{11}$ GeV",
                       legend_position: top_left,
                       grid_lines: [[[r1["zrh"], rgb_color([224, 17, 95]/255)], [z20, burgundy_c]],
                                    [[1, rgb_color([86, 5, 145]/255)]]]))
  },
  # T_max and T_RH against M1
  save("rhn_tmax_trh", list_log_log_plot([map(q => [q[1], q[2]], scan), map(q => [q[1], q[3]], scan)], joined: true,
                                         colors: [indigo, ruby], x_label: r"$M_1$ (GeV)", y_label: "T (GeV)",
                                         legend: [r"$T_{max}$", r"$T_{RH}$"], legend_position: top_left)),
  # the spectra: scattering (dotted, filtered), decay (dashed) per benchmark, and the sums
  save("rhn_gw_spectra",
       list_log_log_plot(map(pt => select(pt, q => q[2] > 0),
                             [sp1["s_filtered"], sp1["d"], sp2["s_filtered"], sp2["d"], sp3["s_filtered"], sp3["d"]]),
                         joined: true, plot_range: [[1, 10^15.5], [1.0e-45, 1.0e-8]],
                         colors: [ruby, ruby, indigo, indigo, olive, olive],
                         line_style: [dotted, dashed, dotted, dashed, dotted, dashed], x_label: r"$f^0$ (Hz)",
                         y_label: r"$\Omega_{GW} h^2$",
                         legend: [r"BP1 ($M_1 = 10^{11}$ GeV), scattering", "BP1, decay", r"BP2 ($M_1 = 10^{10}$ GeV), scattering",
                                  "BP2, decay", r"BP3 ($M_1 = 10^{9}$ GeV), scattering", "BP3, decay"],
                         legend_position: bottom_left)),
  save("rhn_gw_spectra_sum",
       list_log_log_plot(map(pt => select(pt, q => q[2] > 0), [sp1["sum"], sp2["sum"], sp3["sum"]]), joined: true,
                         plot_range: [[1, 10^15.5], [1.0e-45, 1.0e-8]], colors: [ruby, indigo, olive],
                         line_thickness: [2.5], x_label: "f (Hz)", y_label: r"$\Omega_{GW} h^2$",
                         legend: ["BP1", "BP2", "BP3"], legend_position: bottom_left)),
  # the N1 decay against the inflaton decay
  save("rhn_gw_n1_decay",
       list_log_log_plot(map(pt => select(pt, q => q[2] > 0), [sp1["d"], ir11, ir12, ir9]), joined: true,
                         plot_range: [[1.0e4, 10^15.5], [1.0e-45, 1.0e-8]], colors: [ruby, indigo, olive, burgundy],
                         x_label: "f (Hz)", y_label: r"$\Omega_{GW} h^2$",
                         legend: ["inflaton decay", r"$N_1$ decay, $M_1 = 10^{11}$ GeV", r"$N_1$ decay, $M_1 = 10^{12}$ GeV",
                                  r"$N_1$ decay, $M_1 = 10^{9}$ GeV"], legend_position: bottom_left)),
  # the k = 2 scattering spectrum of BP1
  save("rhn_gw_scattering_bp1", plot(omega_s2(r1, g1, f), [f, 1.0e5, 1.0e15], x_scale: log, y_scale: log,
                                    colors: [indigo], x_label: "f (Hz)", y_label: r"$\Omega_{GW} h^2(f)$"))])

stages = [stage_reheat, stage_scatter, stage_spectra, stage_scan, stage_rhn, stage_plots]
say("stage times (s): reheating BP1-3 ", stage_reheat[1], ", scattering ", stage_scatter[1], ", decay spectra ",
    stage_spectra[1], ", M1 scan ", stage_scan[1], ", N1 decay ", stage_rhn[1], ", plots ",
    stage_plots[1], "; total ", sum(map(st => st[1], stages)))
