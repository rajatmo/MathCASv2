#! mathcas 2
# Warm inflation and reheating driven by a strong sphaleron rate (a port of a research notebook).
#
# A quartic inflaton V(phi) = lam phi^4/4 is damped by the sphaleron friction Upsilon(T) = (alpha_s N_c)^5 T^3/(2 f^2)
# (the ratio Q = Upsilon/(3H)); the friction heats a radiation bath of temperature T and drives an axial chemical
# potential mu_A, and the Weinberg operator turns mu_A into a B-L asymmetry n_BL. With z the number of e-folds, the
# Boltzmann equations for phi, phi', rho_R(T), mu_A and n_BL run from the slow-roll attractor at z = 0 to z = 54
# (the end of warm inflation and the start of reheating). The script prints the attractor (Q0, T*, phi'*) and the
# solution at a few e-folds, and draws the energy densities, T and H, mu_A, the slow-roll parameters, the rates
# against H and the B-L yield as log plots (PDF and PNG files in the current folder).
# Run: mc-repl examples/reheating.mc  (about 0.2 s; the stage times are printed at the end).

# --- constants (GeV units) and the model's parameters

mp = 2.4e18          # reduced Planck mass
nc = 3               # colours
nf = 5               # light flavours
alpha_s = 0.026      # strong coupling at the inflation scale
gstar = 106.75       # relativistic degrees of freedom
yt = 0.47            # top Yukawa coupling
kappa_y = 1/100      # top Yukawa rate coefficient
kappa_ss = 270.0     # strong sphaleron rate coefficient
kappa_w = 3/1000     # Weinberg operator rate coefficient
v_ew = 174           # electroweak vacuum expectation value
m_nu = 0.05e-9       # neutrino mass
kappa_ws = 24        # weak sphaleron rate coefficient
alpha_w = 0.025      # weak coupling

lam = 4.0e-21        # inflaton self-coupling
f = 0.146e12         # axion decay constant
phi_star = 5.02*mp   # inflaton value at z = 0

# --- definitions

fn V(phi) = lam/4*phi^4
fn dV(phi) = lam*phi^3
fn eps_v(phi) = mp^2/2*(dV(phi)/V(phi))^2
fn upsilon(t) = (alpha_s*nc)^5*t^3/(2*f^2)
fn rho_r(t) = pi^2/30*gstar*t^4
fn hubble(t, phi, phid) = sqrt((phid^2/2 + V(phi) + rho_r(t))/(3*mp^2))
fn q(t, phi, phid) = upsilon(t)/(3*hubble(t, phi, phid))
fn eps_v_wi(t, phi, phid) = eps_v(phi)/q(t, phi, phid)

# the rates: top Yukawa, strong and weak sphalerons, Weinberg operator
fn gamma_t(t) = 9/2*kappa_y*yt^2*t
fn gamma_ss(t) = 12*kappa_ss*alpha_s^5*t
fn gamma_ws(t) = 18*kappa_ws*alpha_w^5*t
fn gamma_w(t) = 30*kappa_w*m_nu^2*t^3/v_ew^4
fn entropy(t) = 2*pi^2/45*gstar*t^3
# the efficiency of the Weinberg operator (its rate over H, cut by the strong sphaleron and top Yukawa rates where
# they are slower than H) and the B-L source term; defined as in the notebook, not used by its computations
fn efficiency(t, phi, phid) = {
  let h = hubble(t, phi, phid)
  gamma_w(t)/h*min(1, gamma_ss(t)/h)*min(1, gamma_t(t)/h)
}
fn s_bl(t, phi, phid) = 0.269*abs(phid)/f*t^2*efficiency(t, phi, phid)

# --- the attractor at z = 0: phi' from slow roll with friction Q0, T from 4 rho_R = 3 Q0 phi'^2, Q0 self-consistent

h0 = hubble(0, phi_star, 0)
fn phid0(q0) = -dV(phi_star)/(3*h0*(1 + q0))
# T from 4 rho_R(T) = 3 Q0 phi'^2, solved once symbolically: |root| as in the notebook, simplified for Q0 > 0 and
# phi' < 0 (the field rolls towards 0), which keeps abs and sign out of the residual and its derivative
t_root = assuming(qq > 0 and pp < 0, simplify(abs(t /. first(solve(4*rho_r(t) == 3*qq*pp^2, t)))))
fn t0(q0) = t_root /. [qq -> q0, pp -> phid0(q0)]
stage_init = timing(x /. find_root(x - q(t0(x), phi_star, phid0(x)), [x, 100]))
q0 = stage_init[2]
t_star = t0(q0)
phid_star = phid0(q0)
say("Q0 = ", q0, ", T* = ", t_star, ", phi'* = ", phid_star, ", mu_A,eq* = ", -phid_star/(2*f*nf))

# --- the Boltzmann equations in e-folds z (the radiation equation is written for d rho_R/dz, as in the notebook)

stage_solve = timing(n_dsolve([
    phi'(z) == phid(z)/hubble(T(z), phi(z), phid(z)),
    phid'(z) == -3*(1 + q(T(z), phi(z), phid(z)))*phid(z) - 6*q(T(z), phi(z), phid(z))*nf*f*muA(z)
                - dV(phi(z))/hubble(T(z), phi(z), phid(z)),
    d(rho_r(T(z)), z) == -4*rho_r(T(z)) + 3*q(T(z), phi(z), phid(z))*phid(z)*(phid(z) + 2*nf*f*muA(z)),
    muA'(z) == -3*muA(z)*(1 + 12*f^2*q(T(z), phi(z), phid(z))/T(z)^2*nf/nc)
               - 18*f*q(T(z), phi(z), phid(z))/(nc*T(z)^2)*phid(z),
    nBL'(z) == -3*nBL(z) - gamma_w(T(z))/hubble(T(z), phi(z), phid(z))*(2/9*muA(z)*T(z)^2 + 2/3*nBL(z)),
    phi(0) == phi_star, phid(0) == phid_star, T(0) == t_star, muA(0) == 0, nBL(0) == 0],
  [phi, phid, T, muA, nBL], [z, 0, 54]))
sol = stage_solve[2]

# the solution's functions of z
fn of_z(held e) = e /. sol
# the solution tabulated every 0.05 e-folds (z, phi, phi', T, mu_A, n_BL), as CSV: the notebook saves its solution
# to a file and reloads it for the plots; here the plots use the solution of this session
stage_save = timing(export_data("reheating_solution.csv",
                                table(of_z([z, phi(z), phid(z), T(z), muA(z), nBL(z)]), z, range(0, 54, 0.05))))
h_z = of_z(hubble(T(z), phi(z), phid(z)))
# eps_H = -H'/H, with H' from the derivatives of the solution's interpolating functions
eps_h = -d(h_z, z)/h_z

# phi, phi', T, mu_A, n_BL, n_BL/s, Q and H at a few e-folds
stage_values = timing({
  for zk in [1, 10, 20, 30, 40, 50, 54] {
    say("z = ", zk, ": ", of_z([phi(z), phid(z), T(z), muA(z), nBL(z), nBL(z)/entropy(T(z)), q(T(z), phi(z), phid(z)),
                               hubble(T(z), phi(z), phid(z))]) /. z -> zk)
  }
  say("eps_H(30) = ", eps_h /. z -> 30)
})

# --- plots: log value axes, the notebook's colours, each written as PDF and PNG (labels are plain text; the
# notebook's are typeset formulas)

indigo = "#560591"
olive = "#808000"
ruby = "#e0115f"
burgundy = "#800020"
fn save(name, p) = { export_data(string_join([name, ".pdf"]), p); export_data(string_join([name, ".png"]), p) }

stage_plots = timing([
  save("energy_densities",
       plot(of_z([phid(z)^2/2 + V(phi(z)), rho_r(T(z)), phid(z)^2/2]), [z, 0, 54], y_scale: log,
            colors: [indigo, olive, burgundy], x_label: "z", y_label: "ρ (GeV^4)",
            legend: ["ρ_φ", "ρ_R", "φ'^2/2"], legend_position: bottom_left)),
  save("temperature_hubble",
       plot([of_z(T(z)), h_z], [z, 0, 54], y_scale: log, colors: [indigo, olive], x_label: "z", y_label: "(GeV)",
            legend: ["T", "H"], legend_position: bottom_left)),
  save("axial_chemical_potential",
       plot(of_z([muA(z), -phid(z)/(2*f*nf)]), [z, 0, 54], y_scale: log, colors: [indigo, olive], x_label: "z",
            y_label: "μ (GeV)", legend: ["μ_A", "μ_A eq"], legend_position: bottom_left)),
  save("axial_chemical_potential_scaled",
       plot(of_z(muA(z)/T(z)), [z, 0, 54], y_scale: log, colors: [indigo], x_label: "z", y_label: "μ_A/T")),
  save("wi_parameters",
       plot([of_z(q(T(z), phi(z), phid(z))), of_z(eps_v(phi(z))), of_z(eps_v_wi(T(z), phi(z), phid(z))), eps_h],
            [z, 1, 54], y_scale: log, colors: [indigo, olive, ruby, burgundy], x_label: "z",
            legend: ["Q", "ε_V = (M_P^2/2)(V'/V)^2", "ε_V/Q", "ε_H"], legend_position: top_left)),
  # the rates over H, and the line Γ = H
  save("rates_vs_hubble",
       plot(append(map(g => of_z(g(T(z))), [gamma_t, gamma_ss, gamma_ws, gamma_w])/h_z, 1), [z, 1, 54], y_scale: log,
            colors: [indigo, olive, ruby, burgundy, "#808080"], line_style: [solid, solid, solid, solid, dotted],
            x_label: "z", y_label: "Γ/H",
            legend: ["top Yukawa", "strong sphaleron", "weak sphaleron", "Weinberg"], legend_position: top_left)),
  # the B-L yield |n_BL/s| and the observed 2.5e-10
  save("bl_asymmetry",
       plot([of_z(abs(nBL(z)/entropy(T(z)))), 2.5e-10], [z, 0, 54], y_scale: log, colors: [indigo, burgundy],
            line_style: [solid, dashed], x_label: "z", y_label: "Y_B-L"))])

stages = [stage_init, stage_solve, stage_save, stage_values, stage_plots]
say("stage times (s): attractor ", stage_init[1], ", Boltzmann equations ", stage_solve[1], ", table ", stage_save[1],
    ", values ", stage_values[1], ", plots ", stage_plots[1], "; total ", sum(map(st => st[1], stages)))
