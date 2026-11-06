#! mathcas 2
# Gravitational waves from a first-order phase transition (FOPT) and from the bremsstrahlung of a decaying scalar
# during it (a port of a research notebook).
#
# A scalar phi of mass m_phi decays to a pair psi psi (mass m_psi) and, through graviton bremsstrahlung, to psi psi h
# with the rate dGamma_3/dlnE. While the transition proceeds, the fraction of space still in the false phase is
# P(T) = exp(log(0.71) (T/T_p)^-(beta/H)), so the temperature as a function of the converted fraction eps is
# T(eps) = T_p (log 0.71 / log eps)^(H/beta). The graviton energy fraction of today's frequency f0 obeys
# d Omega_GW h^2 / d eps = zeta(3)/pi^2 (2 pi f0) (xi T_0)^3 h^2/rho_c B(x(eps), m_psi/m_phi), x = E/m_phi,
# integrated over eps in [q, 1 - q]; to it the script adds the usual bubble-collision, sound-wave and turbulence
# spectra of the transition (with wall speed v_w, strength alpha_*, beta/H and T_n = T_p). It prints three
# order-of-magnitude numbers of the notebook, the spectra of four benchmark points at decades of f0, and draws the
# progress P(T) and two spectrum figures (PDF and PNG files in the current folder).
#
# Detector sensitivity curves: put CSV files (two columns f [Hz], Omega h^2) named lisa.csv, bbo.csv, decigo.csv,
# et.csv, ce.csv, rescav.csv and bbn.csv into a folder detector_bounds/ under the current folder to draw them with
# the spectra; without them the figures show the spectra alone.
# Run: mc-repl examples/gw_fopt.mc  (about a second; the stage times are printed at the end).

# --- constants (GeV units), as in the notebook

hbar = 6.626e-34          # Planck's constant (J s), used for the GeV to Hz conversion
e_charge = 1.602e-19      # elementary charge (C)
mp = 2.4e18               # reduced Planck mass
gstar = 427/4             # relativistic degrees of freedom at the transition
g0 = 43/11                # entropy degrees of freedom today
rho_c_over_h2 = 8.1e-47   # critical density over h^2 (GeV^4)
t_today = 6.626e-13       # the notebook's present temperature T_0 (GeV)
cs = 1/sqrt(3)            # speed of sound in the plasma
gev_to_hz = 1.0e9*e_charge/hbar
hz_to_gev = 1/gev_to_hz
xi = (g0/gstar)^(1/3)

# --- the progress of the transition: P(T) and its inverse T(eps)

fn progress(t, tp, b) = exp(log(0.71)/(t/tp)^b)
fn t_of_eps(tp, b, ep) = tp*(log(0.71)/log(ep))^(1/b)

# --- the bubble-collision, sound-wave and turbulence spectra (b = beta/H, tn = T_n, as = alpha_*)

fn omega_bubble(f, vw, as, tn, b) = {
  let ka = 6.9*vw^(6/5)*as/(1.36 - 0.037*sqrt(as) + as)
  let kb = as^(2/5)/(0.017 + (0.997 + as)^(2/5))
  let kcol = (0.715*as + 4/27*sqrt(1.5*as))/(1 + 0.715*as)
  let ksw = cs^(11/5)*ka*kb/((cs^(11/5) - vw^(11/5))*kb + vw*cs^(6/5)*ka)
  let kturb = 0.05*ksw
  let hstar = 1.65e-5*(gstar/100)^(1/6)*(tn/100)
  let fcol = 1.65e-6*(gstar/100)^(1/6)*(tn/100)*abs(0.62/(1.8 - 0.1*vw + vw^2))*b
  let fsw = 1.9e-5/vw*(gstar/100)^(1/6)*(tn/100)*b
  let fturb = 1.42*fsw
  let s_col = 3.8*(f/fcol)^2.8/(1 + 2.8*(f/fcol)^3.8)
  let s_sw = (f/fsw)^3*(7/(4 + 3*(f/fsw)^2))^(7/2)
  let s_turb = (f/fturb)^3/((1 + f/fturb)^(11/3)*(1 + 8*pi*f/hstar))
  let upsilon = 1 - (1 + 2*(8*pi)^(1/3)*vw/sqrt(3/4*ksw*as/(1 + as))/b)^(-1/2)
  1.67e-5*(100/gstar)^(1/3)/b^2*(kcol*as/(1 + as))^2*(0.11*vw^3/(0.42 + vw^2))*s_col +
    1.23e-5/gstar^(1/3)/b*(ksw*as/(1 + as))^2*vw*upsilon*s_sw +
    1.55e-3/gstar^(1/3)/b*(kturb*as/(1 + as))^(3/2)*vw*s_turb
}

# --- the bremsstrahlung spectrum: dGamma_3/dlnE over Gamma_2 at x = E/m_phi, y = m_psi/m_phi, below the kinematic
# end x = (1 - 4 y^2)/2 (zero above it)

fn factor2(x, y) = {
  let al = sqrt(1 - 4*y^2/(1 - 2*x))
  al*(1 - 2*x)*(8*x*y^2 + 2*x*(x - 1) - 8*y^4 - 2*y^2 + 1) +
    4*y^2*log((1 + al)/(1 - al))*((5 - 8*x)*y^2 - (x - 1)^2 - 4*y^4)
}
fn branching(x, y, mphi) = (mphi/mp)^2/(8*pi^2)*factor2(x, y)/(1 - 4*y^2)^(3/2)

# d Omega_GW h^2 / d eps at frequency f0 (Hz), below the kinematic end
fn d_omega(f0, ep, mphi, mpsi, tp, b) = {
  let k = 2*pi*f0*hz_to_gev
  zeta(3)/pi^2*k*xi^3*t_today^3/rho_c_over_h2*branching(k/mphi*t_of_eps(tp, b, ep)/(xi*t_today), mpsi/mphi, mphi)
}
# Omega_GW h^2 (f0) = the integral of d_omega over eps in [q, 1 - q]. (The notebook integrates the same equation
# as a differential equation for each f0.) x grows with eps; above eps_end, where x reaches the kinematic end, the integrand
# vanishes, so the integral runs to min(eps_end, 1 - q) over a smooth integrand.
fn omega_brems(f0, mphi, mpsi, tp, b; q = 1.0e-3) = {
  let k = 2*pi*f0*hz_to_gev
  let x_end = (1 - 4*(mpsi/mphi)^2)/2
  let t_end = x_end*mphi*xi*t_today/k
  let eps_end = exp(log(0.71)*(tp/t_end)^b)
  let hi = min(eps_end, 1 - q)
  if hi <= q then 0.0 else n_integrate(d_omega(f0, ep, mphi, mpsi, tp, b), [ep, q, hi])
}

# the approximate (constant-rate) spectrum of the notebook, cut at f0 = m_phi/(4 pi) (xi T_0/T_p)
fn omega_approx(f0, mphi, tp) =
  zeta(3)/pi^2*2*pi*f0*hz_to_gev*xi^3*t_today^3/rho_c_over_h2*(mphi/mp)^2/(8*pi^2)*
    (1.0e-100 + heaviside_theta(mphi*gev_to_hz/(4*pi)*xi*t_today/tp - f0))

# --- 1. three numbers of the notebook: Omega h^2 at f0 = 1e8 Hz for m_phi = 1e13 GeV, the frequency scale, and the
# rounded estimate

stage_numbers = timing({
  let pref = zeta(3)/(8*pi^4)/rho_c_over_h2*2*pi*(xi*t_today)^3
  say("Omega h^2 (m_phi = 1e13, f0 = 1e8 Hz) = ", n((1.0e13/mp)^2*pref*1.0e8*hz_to_gev))
  say("frequency scale = ", n(gev_to_hz/pref), " Hz")
  say("estimate (m_phi/M_P)^2 f0/1.9e17 = ", n((1.0e13/mp)^2*1.0e8/1.9e17))
  # Delta N_eff of the gravitons (from an earlier version of the notebook), T = 1e7 GeV, m_phi = 1e11 GeV
  say("Delta N_eff = ", n(8/7*(11/4)^(4/3)*xi^4*gstar/2*30*zeta(3)/(16*pi^6)*(1.0e11/1.0e7)*(1.0e11/mp)^2))
})

# --- 2. the progress of the transition, T_p = 1e7 GeV, beta/H = 50: P_s against T from eps = 10^-1e-4 to 10^-100

stage_progress = timing({
  # eps = 10^-k on the notebook's grid of k, the whole list at once: T(eps) with log(eps) = -k log(10)
  let ks = range(0.0001, 100, 0.001)
  let data = transpose([1.0e7*(log(0.71)/(-log(10.0)*ks))^(1/50), 10.0^-ks])
  say("progress: ", length(data), " points, first ", data[1], ", last ", data[-1])
  # dashed grid lines at T_p and T_n, labelled beside them; beta/H as the legend's title
  # (the notebook's range ends at the data; it is widened here to show T_n)
  let p = list_line_plot([data], colors: ["#800020"], plot_range: [[8.7e6, 1.65e7], [0, 1]],
                         grid_lines: [[[1.0e7, dashed], [1.6e7, dashed]], [0, 1]],
                         epilog: [text(r"$T_p = 10^7$ GeV", [1.04e7, 0.5]), text(r"$T_n = 1.6\times 10^7$ GeV", [1.5e7, 0.5])],
                         x_label: r"$T$ (GeV)", y_label: r"$P_s(T)$", legend: [r"$P_s(T)$"],
                         legend_title: r"$\beta/H|_{T_p} = 50$", legend_position: bottom_right)
  export_data("progress_of_transition.pdf", p)
  export_data("progress_of_transition.png", p)
})

# --- 3. the spectra of the benchmark points [m_phi, m_psi, beta/H, T_p, v_w, alpha_*]

bps = [[1.0e13, 1.0e7, 50, 1.0e7, 0.5, 0.1], [1.0e13, 1.0e7, 50, 1.0e5, 0.5, 0.1], [1.0e9, 1.0e7, 50, 1.0e4, 0.5, 0.1],
       [1.0e13, 1.0e7, 100, 1.0e5, 0.65, 0.03]]
fn spectrum(bp, f0) = [omega_brems(f0, bp[1], bp[2], bp[4], bp[3]), omega_bubble(f0, bp[5], bp[6], bp[4], bp[3])]

stage_values = timing({
  for k in range(4) {
    say("BP", k, " ", bps[k], ":")
    for j in range(-5, 19, 3) {
      say("  f0 = 1e", j, " Hz: [brems, bubble] = ", spectrum(bps[k], 10.0^j))
    }
  }
})

# each spectrum (bremsstrahlung plus bubbles) on 481 frequencies, 20 per decade from 1e-5 to 1e19 Hz
freqs = map(j => 10.0^(j/20), range(-100, 380))
fn curve(bp) = map(f0 => [f0, sum(spectrum(bp, f0))], freqs)
stage_curves = timing(map(curve, bps))
curves = stage_curves[2]

# --- 4. the figures: the detector curves (when supplied) and the spectra on log axes

bound_names = ["lisa", "bbo", "decigo", "et", "ce", "rescav", "bbn"]
bound_labels = ["LISA", "BBO", "DECIGO", "ET", "CE", "Res. Cav.", "BBN"]
bounds = map(nm => import_data(string_join(["detector_bounds/", nm, ".csv"])), bound_names)
have_bounds = not is_failure(bounds[1])
say(if have_bounds then "detector curves read" else "no detector_bounds/ folder: spectra alone")

ruby = "#e0115f"
olive = "#808000"
indigo = "#560591"
burgundy = "#800020"
# the detector curves dashed (Res. Cav. dotted, BBN in ruby), each region filled up to the top of the plot (but Res.
# Cav. and BBN) and named in place, at the notebook's positions (the epilog of a log plot is in log10 coordinates)
bound_colors = ["#3d99cc", "#f2a024", "#74b336", "#eb6235", "#9382d9", "#000000", ruby]
bound_styles = [dashed, dashed, dashed, dashed, dashed, dotted, dashed]
bound_marks = [["LISA", [1.0e-3, 1.0e-8]], ["BBO", [10, 1.0e-18]], ["DECIGO", [0.1, 1.0e-11]], ["ET", [100, 1.0e-8]],
               ["CE", [100, 1.0e-14]], ["Res. Cav.", [1.0e11, 1.0e-13]], ["BBN", [1.0e14, 1.0e-4]]]
plot_window = [[1.0e-5, 1.0e19], [1.0e-45, 1.0]]
bound_plot = if have_bounds then
  list_log_log_plot(bounds, joined: true, colors: bound_colors, line_style: bound_styles, plot_range: plot_window,
                    x_label: r"$f_0$ (Hz)", y_label: r"$h^2\Omega_{\mathrm{GW}}$", filling: map(k => k -> top, range(5)),
                    epilog: flat_map(k => [rgb_color(bound_colors[k]), text(bound_marks[k][1], log10(bound_marks[k][2]))], range(7)))
  else null

fn figure(name, cvs, colors, labels) = {
  let spectra = list_log_log_plot(cvs, joined: true, colors: colors, plot_range: plot_window,
                                  x_label: r"$f_0$ (Hz)", y_label: r"$h^2\Omega_{\mathrm{GW}}$", legend: labels,
                                  legend_position: bottom_left)
  # the detector plot under the spectra, as the notebook overlays them
  let p = if have_bounds then show(bound_plot, spectra) else spectra
  export_data(string_join([name, ".pdf"]), p)
  export_data(string_join([name, ".png"]), p)
}
stage_plots = timing({
  # the three benchmark points of the paper (the notebook's GW-2) and the fourth (GW-1)
  figure("gw_spectra_bp123", curves[1:3], [ruby, olive, indigo], ["BP1", "BP2", "BP3"])
  figure("gw_spectrum_bp4", [curves[4]], [burgundy], ["BP4"])
  # the constant-rate approximation against the full rate (computed in the notebook, not shown there)
  let approx = list_log_log_plot([curves[2], map(f0 => [f0, omega_approx(f0, 1.0e13, 1.0e5)], freqs)], joined: true,
                                 colors: [olive, burgundy], line_style: [solid, dashed],
                                 plot_range: [[1.0e-5, 1.0e19], [1.0e-45, 1.0]],
                                 x_label: r"$f_0$ (Hz)", y_label: r"$h^2\Omega_{\mathrm{GW}}$",
                                 legend: ["BP2", "BP2, constant rate"], legend_position: bottom_left)
  export_data("gw_spectrum_approx.pdf", approx)
  export_data("gw_spectrum_approx.png", approx)
})

stages = [stage_numbers, stage_progress, stage_values, stage_curves, stage_plots]
say("stage times (s): numbers ", stage_numbers[1], ", progress ", stage_progress[1], ", values ", stage_values[1],
    ", spectra ", stage_curves[1], ", plots ", stage_plots[1], "; total ", sum(map(st => st[1], stages)))
