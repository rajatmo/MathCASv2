#! mathcas 2
# Redraws saved gravitational-wave spectra of three benchmark points (a port of two saved plot files of a research
# project: the summed spectra, and each spectrum's two parts).
#
# The spectra come from the reheating computation of the same project (three benchmark points with heavy neutrino
# masses M_1 = 1e11, 1e10 and 1e9 GeV). The plot files hold them as curves; this script reads the curves from CSV files
# (two columns, f [Hz] and Omega h^2) in a folder gw_spectra/ under the current folder:
#   sdplot1sum_1.csv, sdplot2sum_1.csv, sdplot3sum_1.csv    the summed spectra of BP1, BP2, BP3
#   sdplotK_1.csv, sdplotK_2.csv (K = 1, 2, 3)             the two parts of each (the "s" and "d" contributions)
# and draws the two figures of the files on the project's axes (f from 1 to 10^15.5 Hz, Omega h^2 from 1e-45 to 1e-8):
# gw_spectra_combined.pdf/.png and gw_spectra_separate.pdf/.png. It prints each spectrum's peak.
# Run: mc-repl examples/gw_spectra_replot.mc  (a fraction of a second).

ruby = "#e0115f"
indigo = "#560591"
olive = "#808000"
bp_labels = [r"BP1 ($M_1 = 10^{11}$ GeV)", r"BP2 ($M_1 = 10^{10}$ GeV)", r"BP3 ($M_1 = 10^{9}$ GeV)"]
fn path(name) = string_join(["gw_spectra/", name, ".csv"])
have = not is_failure(import_data(path("sdplot1sum_1")))

# the largest Omega h^2 of a curve and its frequency
fn peak(c) = { let k = ordering(map(pt => pt[2], c))[-1]; c[k] }

if not have then say("no gw_spectra/ folder: supply the curves as described in the header") else {
  let sums = map(k => import_data(path(string_join(["sdplot", to_string(k), "sum_1"]))), range(3))
  let parts = flat_map(k => map(j => import_data(path(string_join(["sdplot", to_string(k), "_", to_string(j)]))), [1, 2]), range(3))
  for k in range(3) { say("BP", k, ": ", length(sums[k]), " points, peak Omega h^2 = ", peak(sums[k])[2], " at f = ", peak(sums[k])[1], " Hz") }
  let combined = list_log_log_plot(sums, joined: true, colors: [ruby, indigo, olive],
                                   plot_range: [[1, 10.0^15.5], [1.0e-45, 1.0e-8]],
                                   x_label: r"$f^0$ (Hz)", y_label: r"$\Omega_{\mathrm{GW}}h^2$", legend: bp_labels,
                                   legend_position: bottom_left)
  export_data("gw_spectra_combined.pdf", combined)
  export_data("gw_spectra_combined.png", combined)
  let separate = list_log_log_plot(parts, joined: true, colors: [ruby, ruby, indigo, indigo, olive, olive],
                                   line_style: [dotted, dashed, dotted, dashed, dotted, dashed],
                                   plot_range: [[1, 10.0^15.5], [1.0e-45, 1.0e-8]],
                                   x_label: r"$f^0$ (Hz)", y_label: r"$\Omega_{\mathrm{GW}}h^2$",
                                   legend: flat_map(l => [string_join([l, ", s"]), string_join([l, ", d"])], ["BP1", "BP2", "BP3"]),
                                   legend_position: bottom_left)
  export_data("gw_spectra_separate.pdf", separate)
  export_data("gw_spectra_separate.png", separate)
  null
}
