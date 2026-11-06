#! mathcas 2
# A first gravitational-wave spectrum from reheating with detector sensitivities (a port of a research notebook).
#
# From the Boltzmann-equation results f_e = 2.93e-17 GeV, f_RH = 9e-8 GeV (as frequencies), rho_GW(a_RH) =
# 1.05e21 GeV^4 and T_RH = 4.17e9 GeV, the spectrum today d(Omega_GW h^2)/d ln f = (1/2) sqrt(f_e/f) rho_GW(a_RH),
# cut below f_e and above f_RH and redshifted. The notebook overlays sensitivity curves read from text files
# (Bounds/cavity.txt with log10 values, LIGO.txt, CE.txt, DECIGO.txt: two columns f, Omega h^2); put them in a
# folder Bounds/ next to the working folder to draw them too, otherwise only the spectrum is drawn.
# Run: mc-repl examples/preliminary_gw_spectrum.mc  (PDF and PNG in the current folder)

gev_to_hz = 1.0e9*1.602e-19/6.626e-34
fe = 2.93e-17*gev_to_hz
frh = 9.0e-8*gev_to_hz
rho_gw_rh = 1.05e21
t_rh = 4.17e9
g_rh = 427/4
g0 = 43/11
xi = n((g0/g_rh)^(1/3))
alpha = n(g_rh*pi^2/30)
t0 = 6.626e-13
redshift = xi^4*(t0/t_rh)^4
density_factor = 1/8.1e-47          # h^2/rho_c

fn omega(f) = density_factor*heaviside_theta(f - fe)*exp(-5*sqrt(alpha)/4*f^2/frh^2)*redshift*1/2*sqrt(fe/f)*rho_gw_rh
say("f_e = ", fe, " Hz, f_RH = ", frh, " Hz; prefactor ", density_factor*redshift, "; Omega h^2 just above f_e: ",
    omega(fe*(1 + 1.0e-12)))

# the sensitivity curves, where the files are present (cavity: log10 of both columns); a missing file gives a
# failure, and the curves with one are dropped   # GAP G52 (no file-exists test)
fn bound(file, logs) = {
  let d = import_data(string_join(["Bounds/", file]))
  sort(if logs then 10.0^d else d)
}
bounds = drop_failures(map(b => [b[1], bound(b[2], b[3]), b[4]],
                           [["Resonant cavity", "cavity.txt", true, "#ff8c00"], ["LIGO", "LIGO.txt", false, "#d00000"],
                            ["CE", "CE.txt", false, "#00a000"], ["DECIGO", "DECIGO.txt", false, "#c8b400"]]))
say(length(bounds), " sensitivity curves found")

fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }
fs = map(e => 10.0^e, range(log10(fe/100), log10(100*frh), 0.02))
spectrum = select(map(f => [f, omega(f)], fs), q => q[2] > 1.0e-50)
stage = timing(save("preliminary_gw_spectrum",
  list_log_log_plot(join([spectrum], map(b => b[2], bounds)), joined: true,
                    plot_range: [[1.0e4, 1.0e20], [1.0e-50, 1]], colors: join(["#1f3fbf"], map(b => b[3], bounds)),
                    x_label: "f (Hz)", y_label: r"$\frac{d(\Omega_{GW} h^2)}{d \ln f}$",
                    legend: join([r"$T_{RH} = 4.17 \times 10^9$ GeV"], map(b => b[1], bounds)), legend_position: top_left)))
say("plot (s): ", stage[1])
