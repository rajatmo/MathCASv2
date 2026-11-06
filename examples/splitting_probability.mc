#! mathcas 2
# Splitting probability of a particle crossing a rippled bubble wall (a port of a research notebook).
#
# The probability P(Omega) at energy Omega, scaled by (8 pi^2/y^2)(m/Delta m)^2, for a wall whose mass profile is
# modulated with n ripples of relative width kappa: the weak-modulation result integrates the ripple form factor
# W(q) against the splitting kernel H(r), the narrow-resonance formula keeps one resonant term, and the asymptotic
# value is 7/9. The script prints a few values and draws the three curves against Omega on log axes (PDF and PNG
# files in the current folder).
# Run: mc-repl examples/splitting_probability.mc  (the stage times are printed at the end).

m = 1.05
kappa = 0.1
nr = 100

# the splitting kernel and the ripple form factor
fn H(r) = (1 + 4*r - 2*r^2)*atanh(sqrt(1 - r)) - (1 + 2*r)*sqrt(1 - r)
fn W(q) = kappa^4/(q^2 - kappa^2)^2*cos(q/2*(2*nr + 1)*pi/kappa)^2

fn p_modulation(om: Real) = n_integrate(W(2*m^2/(om*r))*H(r), [r, 0, 1])
# zero below the threshold Omega = 2 m^2/kappa (where H would be taken beyond its range)
fn p_resonance(om: Real) = if 2*m^2/(om*kappa) < 1 then (2*nr + 1)*pi^2*m^2/(4*om*kappa)*H(2*m^2/(om*kappa)) else 0
p_inf = 7/9

stage_values = timing(map(om => [om, p_modulation(om), p_resonance(om)], [10.0, 30.0, 100.0, 300.0, 1000.0, 1500.0]))
say("[Omega, weak modulation, narrow resonance]: ", stage_values[2])

indigo = "#560591"
olive = "#808000"
burgundy = "#800020"
fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }
# the integral is tabulated on 60 points equally spaced in log Omega (adaptive sampling of an oscillating integral
# is wasteful) and drawn with the closed forms
oms = map(e => 10.0^e, range(1, log10(1500.0), (log10(1500.0) - 1)/59))   # GAP G50 (speed)
stage_plot = timing(save("splitting_probability",
  list_log_log_plot([map(om => [om, p_modulation(om)], oms), select(map(om => [om, p_resonance(om)], oms), q => q[2] > 0),
                     [[10, p_inf], [1500, p_inf]]], joined: true,
       colors: [indigo, olive, burgundy], line_style: [solid, solid, dashed], x_label: r"$\Omega$",
       y_label: r"$\frac{8\pi^2}{y^2}\left(\frac{\bar{m}}{\Delta m}\right)^2 P(\Omega)$",
       legend: [r"Weak $\Delta m$ modulation", "Narrow resonance formula", r"Asymptotic probability $P(\infty)$"],
       legend_position: bottom_right)))

say("stage times (s): values ", stage_values[1], ", plot ", stage_plot[1], "; total ", stage_values[1] + stage_plot[1])
