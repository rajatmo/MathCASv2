#! mathcas 2
# A bubble wall from a first-order phase transition (a port of a research notebook): the thermal function J_B,
# an effective potential V(h, T), the O(3) bounce by shooting, its action S(x0), the nucleation condition S = 140,
# the O(4) wall profile and its oscillating tail. Run: mc-repl examples/wall_profile.mc  (about 20 s; plots are
# written as PNG files to the current folder).

# --- 1. the thermal function J_B and the effective potential

# J_B(x) = integral of y^2 log(1 - e^-sqrt(y^2 + x^2)) over y in [0, inf)
fn jb(x) = n_integrate(y^2*log(1 - exp(-sqrt(y^2 + x^2))), [y, 0, inf])
# its derivative in x: x * integral of y^2 / (r (e^r - 1)), r = sqrt(y^2 + x^2)
fn djb(x) = x*n_integrate(y^2/(sqrt(y^2 + x^2)*(exp(sqrt(y^2 + x^2)) - 1)), [y, 0, inf])

say("J_B(0) = ", jb(0), "   exact -pi^4/45 = ", n(-pi^4/45))
jbtab = table([x, jb(x)], x, range(0, 25, 0.02))
djbtab = table([x, djb(x)], x, range(0, 25, 0.02))
JB = interpolation(jbtab)
dJB = interpolation(djbtab)
say("tables: ", length(jbtab), " points; J_B(1.234) = ", JB(1.234), ", direct ", jb(1.234))

g = sqrt(0.7)
fn f(x, x0) = 1/(2*pi^2)*(g^4/16*x^4*(log(x/x0) - 1/4) + JB(min(g*x, 25.0)) - JB(0.0))
fn df(x, x0) = 1/(2*pi^2)*(g^4/4*x^3*log(max(x, 1.0e-9)/x0) + g*dJB(min(g*max(x, 1.0e-9), 25.0)))

say("f(50, 83.75) = ", n(f(50, 83.75)), ", df(50, 83.75) = ", n(df(50, 83.75)))
# df against a central difference of f
say("check: ", n((f(50.001, 83.75) - f(49.999, 83.75))/0.002))
export_data("veff.png", plot(f(x, 83.75), [x, 0.001, 150]))
export_data("jb.png", plot([JB(x), dJB(x)], [x, 0, 25]))

# --- 2. the O(3) bounce by shooting
z0 = 1.0e-4
zmax = 100.0

# x'' + 2/z x' = dV/dx, started from the series at z0; stops where x' = 0 (undershoot) or x = 0 (overshoot)
# (a solution that blows up before an event keeps the part computed, with a printed warning)
fn bounce(x0, xc) = n_dsolve([x''(z) + 2/z*x'(z) == df(x(z), x0), x(z0) == xc + df(xc, x0)/6*z0^2,
                              x'(z0) == df(xc, x0)/3*z0], x, [z, z0, zmax], events: [x'(z) == 0, x(z) == 0])

# where the solution ends, and the shooting function s(ze) - |s'(ze)|
fn domain_end(s) = (x /. s)[2][2]
rule shoot(x0_, a_) when is_number(a) := {
  let s = bounce(x0, a)
  let ze = domain_end(s)
  (x(ze) /. s) - abs(x'(ze) /. s)
}


# --- 3. the action S(x0), the nucleation condition S = 140, the O(4) wall profile
w0 = 1.0e-4
wmax = 20.0

# x_c(x0) by shooting over the grid, interpolated
fn xc_of(x0) = a /. find_root(shoot(x0, a), [a, 5, 1, 10])
# points where shooting fails (no sign change in [1, 10]) are left out
xctab = drop_failures(table([x0, xc_of(x0)], x0, range(10, 1000, 10)))
say(length(xctab), " of 100 shooting points")
xcInt = interpolation(xctab)

say("x_c(83.75) = ", xcInt(83.75))

# the O(3) action of the bounce: 4 pi int z^2 (x'^2/2 + V) dz over the solution's domain
fn action(b) = {
  let s = bounce(b, xcInt(b))
  let ze = domain_end(s)
  n_integrate(4*pi*z^2*((x'(z) /. s)^2/2 + f(x(z) /. s, b)), [z, z0, ze], precision_goal: 6)
}
setab = drop_failures(table([b, action(b)], b, map(p => p[1], xctab)))
say(length(setab), " action points")
SE = interpolation(setab)
x0n = x0 /. find_root(SE(x0) == 140, [x0, 100, setab[1][1], setab[-1][1]])
xcn = xcInt(x0n)
say("S = 140 at x0 = ", x0n, ", x_c = ", xcn)

# the O(4) wall profile: x'' + 3/w x' + V'(x) = 0
xsol = x /. n_dsolve([x''(w) + 3/w*x'(w) + df(x(w), x0n) == 0, x(w0) == xcn - w0^2/8*df(xcn, x0n),
                      x'(w0) == -w0/4*df(xcn, x0n)], x, [w, w0, wmax])
export_data("bounce.png", plot(x(z) /. bounce(x0n, xcn), [z, z0, domain_end(bounce(x0n, xcn))]))
export_data("wall_profile.png", plot(xsol(w)/x0n, [w, w0, wmax]))
export_data("action.png", list_log_log_plot(setab))

# --- 4. the oscillating tail: first zero of x/x0 - 1, first turning point, and the amplitude there
wc = w /. find_root(xsol(w)/x0n - 1, [w, 1, 0.05, 5.5])
wk = w /. find_root(xsol'(w)/x0n, [w, 5.5, 5.3, 5.7])
# the amplitude eps_k from the third-order expansion eps - 5/6 eps^2 + 3/16 eps^3 = x(wk)/x0 - 1
# (`e` is the constant e, hence `ep`; the equation is written as a difference, since `==` of two numbers is a
# boolean)
ek = ep /. find_root(ep - 5/6*ep^2 + 3/16*ep^3 - (xsol(wk)/x0n - 1), [ep, 0])
jk = ek^2 - 11/9*ek^4
jc = jk*(wk/wc)^3
say("first zero w_c = ", wc, ", first turning point w_k = ", wk)
say("eps_k = ", ek, ", j_k = ", jk, ", j_c = ", jc)

# the approximate envelope x0 (1 + eps(w) cos(theta(w))) against the exact profile
fn eps_of(w) = sqrt(jc)*(wc/w)^(3/2)
fn theta(w) = g^2*x0n/(2*sqrt(2)*pi)*((w - wk) - 23/24*jc*wc^3/wk^2)
export_data("envelope.png", plot([1 + eps_of(w)*cos(theta(w)), xsol(w)/x0n], [w, wc, wmax]))
