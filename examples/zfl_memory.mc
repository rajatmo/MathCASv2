#! mathcas 2
# Zero-frequency limit and memory of gravitational-wave signals (a port of two short research notebooks).
#
# 1. Two toy signals: a step held for a unit time (with memory) and the same with a sign flip halfway (without
#    memory); their power spectra |F(omega)|^2 are finite at omega = 0 only for the second.
# 2. A carrier with a randomly diffusing phase (phase diffusion rate Delta = 1 Hz): the phase and the signal for a
#    slow (10^-2 Hz) and a fast (10^2 Hz) carrier over 10^6 samples.
# 3. The soft-graviton spectrum of the decay phi -> N N with a bremsstrahlung graviton (number and energy spectra
#    against x = E_g/m_phi, for M_N/m_phi = 10^-1 and 10^-3) against the soft (inclusive) limit, and their relative
#    difference at x = 10^-10.
# Run: mc-repl examples/zfl_memory.mc  (PDF and PNG files in the current folder; stage times at the end).

fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }
blue = "#1f3fbf"
red = "#c0202a"

# --- 1. memory and the zero-frequency limit
# Fourier transform with the unitary convention, piece by piece (a box [a, b])   # GAP G45 (sums and sign factors)
fn box(a, b) = integrate(exp(im*w*t), [t, a, b])/sqrt(2*pi)
stage_spectra = timing({
  let ft1 = fourier_transform(unit_step(t)*unit_step(1 - t), t, w)
  let ft2 = box(1/2, 1) - box(0, 1/2)
  # |F|^2 for real omega   # GAP G46 (complex_expand of abs^2)
  let p1 = assuming(element(w, reals), simplify(complex_expand(ft1*conjugate(ft1))))
  let p2 = assuming(element(w, reals), simplify(complex_expand(ft2*conjugate(ft2))))
  say("|F|^2 with memory: ", p1, ", without: ", p2, "; at omega -> 0: ", limit(p1, w -> 0), ", ", limit(p2, w -> 0))
  [p1, p2]
})
pw1 = stage_spectra[2][1]
pw2 = stage_spectra[2][2]
stage_plots1 = timing(save("zfl_memory",
  graphics_row([
    plot([unit_step(t)*unit_step(1 - t), unit_step(t)*unit_step(1 - t)*sign(t - 1/2)], [t, -1, 2], colors: [blue, red],
         x_label: "t", y_label: "f(t)", legend: ["With Memory", "Without Memory"], legend_position: bottom_right),
    plot([pw1, pw2] /. w -> om, [om, 0.001, 10], colors: [blue, red], x_label: r"$\omega$",
         y_label: r"$\frac{dE}{d\omega}$", legend: ["With Memory", "Without Memory"], legend_position: top_right,
         grid_lines: [none, [0]])])))

# --- 2. phase coherence of a carrier with a diffusing phase
seed(7)
w1 = 2*pi/10^2
w2 = 2*pi*10^2
delta = 1.0
dt = 2*pi/(10*w2)
tt = 2*pi*10/w1
ns = round(tt/dt) + 1
n1 = round(2*2*pi/(w1*dt))
n2 = round(2*2*pi/(w2*dt))
say("samples ", ns, ", one period of each carrier ", n1/2, " and ", n2/2)
# the phase is a running sum of 10^6 Gaussian steps; only the plotted samples are exponentiated
# GAP G47 (running sums of packed lists), G48 (complex exp of packed lists)
stage_signal = timing({
  let dphi = random_variate(normal_distribution(0, sqrt(2*delta*dt)), ns - 1)
  let phi = accumulate(join([0.0], dphi))
  let ts = subdivide(0.0, tt, ns - 1)
  let s1 = exp(im*(w1*take(ts, n1) + take(phi, n1)))
  let s2 = exp(im*(w2*take(ts, n2) + take(phi, n2)))
  [s1, s2]
})
sig1 = stage_signal[2][1]
sig2 = stage_signal[2][2]
# the notebook puts the two carriers side by side; here each panel is its own file   # GAP G49 (graphics_row speed)
stage_plots2 = timing([
  save("zfl_phase_slow", list_line_plot(arg(sig1), colors: [blue], x_label: "t (samples)", y_label: r"phase $\phi(t)$",
                                        plot_label: r"$\Delta = 1$ Hz, $f = 10^{-2}$ Hz")),
  save("zfl_phase_fast", list_line_plot(arg(sig2), colors: [blue], x_label: "t (samples)", y_label: r"phase $\phi(t)$",
                                        plot_label: r"$\Delta = 1$ Hz, $f = 10^{2}$ Hz")),
  save("zfl_signal_slow", list_line_plot(re(sig1), colors: [blue], x_label: "t (samples)", y_label: "signal",
                                         plot_label: r"$\Delta = 1$ Hz, $f = 10^{-2}$ Hz")),
  save("zfl_signal_fast", list_line_plot(re(sig2), colors: [blue], x_label: "t (samples)", y_label: "signal",
                                         plot_label: r"$\Delta = 1$ Hz, $f = 10^{2}$ Hz"))])

# --- 3. soft gravitons in phi -> N N h (m_phi = Mpl in the normalised spectra)
fn gamma0(y) = yphi^2/(8*pi)*(1 - 4*y^2)^(3/2)
fn al(x, y) = sqrt(1 - 4*y^2/(1 - 2*x))
fn dgamma(x, y) = yphi^2/(64*pi^3)/x*(al(x, y)/4*(1 - 2*x)*(1 + (1 - 2*x)^2 + 4*(4*x - 1)*y^2 + 4*y^4) -
                                      2*y^2*(y^2*(8*x - 5) + (x - 1)^2 + 4*y^4)*log((1 + al(x, y))/(1 - al(x, y))))
fn dgamma_n(x, y) = dgamma(x, y)/gamma0(y)
fn beta(y) = sqrt(1 - 4*y^2)/(1 - 2*y^2)
fn fsoft(v) = (1 + v^2)/(4*v*sqrt(1 - v^2))*log((1 + v)/(1 - v))
fn dgamma_soft(y) = 1/(8*pi^2)*y^2*fsoft(beta(y))
stage_soft = timing({
  say("number spectrum (normalised): ", simplify(dgamma_n(x, y)))
  say("relative difference at y = 10^-3, x = 10^-10: ",
      n(abs(1/10^10*dgamma_n(1/10^10, 1/10^3) - dgamma_soft(1/10^3))/dgamma_soft(1/10^3)))
})
fn spectra_plot(y, name) = save(name, graphics_row([
  plot([dgamma_n(x, y), dgamma_soft(y)/x], [x, 1.0e-10, 1/2 - 2*y^2], x_scale: log, y_scale: log, colors: [blue, red],
       x_label: "x", y_label: r"$\frac{1}{\Gamma}\frac{d\Gamma}{dE}$", plot_label: "Number Spectrum",
       legend: ["exact", "soft limit"]),
  plot([x*dgamma_n(x, y), dgamma_soft(y)], [x, 1.0e-10, 1/2 - 2*y^2], x_scale: log, y_scale: log, colors: [blue, red],
       x_label: "x", y_label: r"$\frac{E}{\Gamma}\frac{d\Gamma}{dE}$", plot_label: "Energy Spectrum",
       legend: ["exact", "soft limit"])]))
stage_plots3 = timing([spectra_plot(1/10, "zfl_soft_spectrum_y1e-1"), spectra_plot(1/10^3, "zfl_soft_spectrum_y1e-3")])

stages = [stage_spectra, stage_plots1, stage_signal, stage_plots2, stage_soft, stage_plots3]
say("stage times (s): spectra ", stage_spectra[1], ", memory plots ", stage_plots1[1], ", random phase ",
    stage_signal[1], ", coherence plots ", stage_plots2[1], ", soft limit ", stage_soft[1], ", soft plots ",
    stage_plots3[1], "; total ", sum(map(st => st[1], stages)))
