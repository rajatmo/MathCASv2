#! mathcas 2
# Core-language library: functions written in MathCAS itself, evaluated after the
# other libraries, defined in every session. Helper functions are prefixed `lang_` and not part of the interface.

# --- quantity arrays ---
# An array of quantities is a list of quantity(...) terms (arrays are lists in this language: no separate structured
# form), so every list function and the arithmetic of quantities apply to it.

# lang_quantities(v, u): every leaf of the nested list v as a quantity with the unit u
fn lang_quantities(v, u) = if is_list(v) then map(x => lang_quantities(x, u), v) else quantity(v, u)

# lang_columns(v, us): the innermost lists of v as quantities with the units us, one unit per position
fn lang_columns(v, us) = if is_list(v) and length(v) > 0 and is_list(v[1]) then map(r => lang_columns(r, us), v) else map(i => quantity(v[i], us[i]), range(length(v)))

# quantity_array(v, u): the array v with every entry a quantity of the unit u; quantity_array(v, [u1, ..., uk]): the
# entries at position i of each innermost list with the unit ui; quantity_array(qs): a list of quantities stays
fn quantity_array(v, u) when is_list(v) and not is_list(u) = lang_quantities(v, u)
fn quantity_array(v, us) when is_list(v) and is_list(us) and length(us) > 0 = lang_columns(v, us)
fn quantity_array(qs) when is_list(qs) = qs

# --- parts of uncertain numbers, operator forms ---
# around(x, dx)("Value") and ("Uncertainty"): the parts of an uncertain number
rule around(v_, u_)("Value") := v
rule around(v_, u_)("Uncertainty") := u
# operator_applied(f)(x, y) is f(y, x) (the two arguments of one call, as operator_applied(f)(x)(y) is)
rule operator_applied(f_)(x_, y_) := f(y, x)
