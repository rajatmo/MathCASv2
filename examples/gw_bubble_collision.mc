#! mathcas 2
# The gravitational-wave spectrum of a first-order phase transition from bubble collisions, sound waves and
# magnetohydrodynamic turbulence (a port of a research notebook).
#
# The three standard fits, with the efficiency factors kappa_col, kappa_sw, kappa_turb = eps kappa_sw (eps = 0.05),
# the suppression factor of the sound waves Upsilon, and beta/H = T S'(T) given per benchmark point by T_n and
# S' = d(S_3/T)/dT. The script prints the notebook's values for its two benchmark points and draws the spectrum of a
# strong, slow transition (v_w = 0.8, T_n = 1e10 GeV, alpha = 1, beta/H = 0.01) from 1e-5 to 1e14 Hz, with the
# detector curves when they are supplied (CSV files thiea.csv, muares.csv, lisa.csv, bbo.csv, decigo.csv, et.csv and
# ce.csv in a folder detector_bounds/ under the current folder; two columns, f [Hz] and Omega h^2).
# Run: mc-repl examples/gw_bubble_collision.mc  (a fraction of a second; the plot is written as PDF and PNG files).

gstar = 106
cs = 1/sqrt(3)
eps_turb = 0.05      # the fraction of the bulk motion in turbulence

# the two benchmark points: T_n (GeV), alpha, S'
bp1 = [42.68, 0.573, 1.285]
bp2 = [51.44, 0.23, 2.83]
say("T_reh/T_n = ", [55.05/42.68, 155.18/51.44])

fn beta_over_h(t, st) = t*st
fn kappa_a(vw, al) = 6.9*vw^(6/5)*al/(1.36 - 0.037*sqrt(al) + al)
fn kappa_b(al) = al^(2/5)/(0.017 + (0.997 + al)^(2/5))
fn kappa_col(al) = (0.715*al + 4/27*sqrt(1.5*al))/(1 + 0.715*al)
fn kappa_sw(vw, al) = cs^(11/5)*kappa_a(vw, al)*kappa_b(al)/((cs^(11/5) - vw^(11/5))*kappa_b(al) + vw*cs^(6/5)*kappa_a(vw, al))
fn kappa_turb(vw, al) = eps_turb*kappa_sw(vw, al)

# the peak frequencies today (Hz)
fn f_col(vw, tn, st) = 1.65e-6*(gstar/100)^(1/6)*(tn/100)*0.62/(1.8 - 0.1*vw + vw^2)*beta_over_h(tn, st)
fn f_sw(vw, tn, st) = 1.9e-5/vw*beta_over_h(tn, st)*(tn/100)*(gstar/100)^(1/6)
fn f_turb(vw, tn, st) = 1.42*f_sw(vw, tn, st)
fn h_star(tn) = 1.65e-5*(tn/100)*(gstar/100)^(1/6)

# the root-mean-square fluid velocity and the sound-wave suppression factor
fn u_f(vw, al) = sqrt(3*kappa_sw(vw, al)*al/(4*(1 + al)))
fn upsilon(vw, al, tn, st) = 1 - 1/sqrt(1 + 2*vw*(8*pi)^(1/3)/u_f(vw, al)/beta_over_h(tn, st))

# the spectral shapes and the three contributions to Omega h^2
fn s_sw(f, vw, tn, st) = (f/f_sw(vw, tn, st))^3*(7/(4 + 3*(f/f_sw(vw, tn, st))^2))^(7/2)
fn s_turb(f, vw, tn, st) = (f/f_turb(vw, tn, st))^3/((1 + f/f_turb(vw, tn, st))^(11/3)*(1 + 8*pi*f/h_star(tn)))
fn omega_col(f, vw, tn, al, st) =
  1.67e-5*(100/gstar)^(1/3)*(kappa_col(al)*al/(1 + al))^2*(0.11*vw^3/(0.42 + vw^2))*
    (0.38*(f/f_col(vw, tn, st))^2.8/(1 + 2.8*(f/f_col(vw, tn, st))^3.8))/beta_over_h(tn, st)^2
fn omega_sw(f, vw, tn, al, st) =
  1.23e-5/gstar^(1/3)*(kappa_sw(vw, al)*al/(1 + al))^2*vw*s_sw(f, vw, tn, st)*upsilon(vw, al, tn, st)/beta_over_h(tn, st)
fn omega_turb(f, vw, tn, al, st) =
  1.55e-3/gstar^(1/3)*(kappa_turb(vw, al)*al/(1 + al))^(3/2)*vw*s_turb(f, vw, tn, st)/beta_over_h(tn, st)
fn omega_total(f, vw, tn, al, st) = omega_col(f, vw, tn, al, st) + omega_sw(f, vw, tn, al, st) + omega_turb(f, vw, tn, al, st)

stage_values = timing({
  say("Omega h^2 at 0.1 Hz, v_w = 0.1: ", [omega_total(0.1, 0.1, 56.3, bp1[2], bp1[3]), omega_total(0.1, 0.1, 41.14, bp2[2], bp2[3])])
  # (the notebook also lists a third point whose alpha and S' it does not define)
  say("Upsilon at v_w = 0.54: ", [upsilon(0.54, bp1[2], 39.8, bp1[3]), upsilon(0.54, bp2[2], 50.2, bp2[3])])
  say("Omega_col h^2 (1000 Hz, v_w = 0.8, T_n = 500, alpha = 0.4, S' = 1/2) = ", omega_col(1000, 0.8, 500, 0.4, 1/2))
  say("strong transition, Omega h^2 at 1 Hz and 1e6 Hz: ", [omega_total(1.0, 0.8, 1.0e10, 1, 1.0e-12), omega_total(1.0e6, 0.8, 1.0e10, 1, 1.0e-12)])
})

# --- the spectrum of a strong transition, with the detector curves when present

names = ["bbo", "lisa", "decigo", "muares", "ce", "et", "thiea"]
labels = ["BBO", "LISA", "DECIGO", r"$\mu$ARES", "CE", "ET", "THEIA"]
detectors = map(nm => import_data(string_join(["detector_bounds/", nm, ".csv"])), names)
have = not is_failure(detectors[1])
spectrum = map(k => {
  let f = 10.0^(k/20)
  [f, omega_total(f, 0.8, 1.0e10, 1.0, 1.0e-12)]
}, range(-100, 280))

# the notebook's label positions, in natural logs of f and Omega h^2 (the epilog of a log plot takes log10 values)
marks = [[-0.1, -36.5], [-9, -20.5], [-1.7, -29.5], [-12.5, -26], [3.25, -28.7], [3.2, -24.7], [-18.0, -26]]
window = [[1.0e-5, 1.0e14], [1.0e-30, 1.0e-2]]

stage_plot = timing({
  let spec = list_log_log_plot([spectrum], joined: true, plot_range: window, x_label: r"$f$ (Hz)",
                               y_label: r"$\Omega_{\mathrm{GW}}h^2$", legend: [r"$\Omega_{\mathrm{GW}}h^2$"],
                               legend_title: r"$v_w = 0.8$, $T_n = 10^{10}$ GeV, $\alpha = 1$, $\beta/H = 0.01$",
                               legend_position: bottom_right)
  # the detector curves dashed and filled up to the top, named in place, under the spectrum
  let p = if have then show(list_log_log_plot(detectors, joined: true, line_style: map(nm => dashed, names),
                                              filling: map(k => k -> top, range(7)), plot_range: window,
                                              x_label: r"$f$ (Hz)", y_label: r"$\Omega_{\mathrm{GW}}h^2$",
                                              epilog: map(k => text(labels[k], marks[k]/log(10.0)), range(7))), spec)
          else spec
  export_data("gw_bubble_collision.pdf", p)
  export_data("gw_bubble_collision.png", p)
})
say("stage times (s): values ", stage_values[1], ", plot ", stage_plot[1])