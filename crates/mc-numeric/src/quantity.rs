//! Physical quantities (decision P6N.15): `quantity(m, unit)` with the unit a string or a
//! product of powers of strings (`"Meters"/"Seconds"`), a table of SI and common units written for this crate,
//! arithmetic with conversion, comparisons, and `unit_convert`, `quantity_magnitude`, `quantity_unit`.
//!
//! - A sum needs one dimension: the terms are converted to the unit of the first (`failure(incompatible_units)`
//!   otherwise); products and powers multiply magnitudes and units, and a product whose units cancel is a number.
//! - Conversion factors are exact rationals (or exact multiples of pi), so exact magnitudes stay exact.
//! - Temperatures in degrees Celsius and Fahrenheit convert affinely by `unit_convert` of the lone unit; in
//!   products they count as kelvin differences.
//! - Unknown unit names are dimensions of their own (factor 1): such quantities add only to the same unit.
use crate::formula::{Def, Fx};
use mc_api::{Store, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::Res;

type R<'h> = Res<Option<T<'h>>>;

/// Base dimensions: length, mass, time, current, temperature, amount, luminous intensity.
type Dims = [i64; 7];

/// (names, factor numerator, factor denominator, factor times pi?, dimensions); factor = SI value of one unit.
#[rustfmt::skip]
const UNITS: &[(&[&str], i128, i128, bool, Dims)] = &[
    (&["Meters", "m", "Meter"], 1, 1, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Kilometers", "km"], 1000, 1, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Centimeters", "cm"], 1, 100, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Millimeters", "mm"], 1, 1000, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Micrometers"], 1, 1_000_000, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Nanometers", "nm"], 1, 1_000_000_000, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Feet", "ft", "foot", "Foot"], 381, 1250, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Inches", "in", "Inch"], 127, 5000, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Yards", "yd"], 1143, 1250, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Miles", "mi"], 201168, 125, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["NauticalMiles"], 1852, 1, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["AstronomicalUnits", "AstronomicalUnit", "au"], 149_597_870_700, 1, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["LightYears", "ly"], 9_460_730_472_580_800, 1, false, [1, 0, 0, 0, 0, 0, 0]),
    (&["Kilograms", "kg"], 1, 1, false, [0, 1, 0, 0, 0, 0, 0]),
    (&["Grams", "g"], 1, 1000, false, [0, 1, 0, 0, 0, 0, 0]),
    (&["Milligrams", "mg"], 1, 1_000_000, false, [0, 1, 0, 0, 0, 0, 0]),
    (&["Micrograms", "ug"], 1, 1_000_000_000, false, [0, 1, 0, 0, 0, 0, 0]),
    (&["MetricTons", "Tonnes"], 1000, 1, false, [0, 1, 0, 0, 0, 0, 0]),
    (&["Pounds", "lb", "lbs"], 45_359_237, 100_000_000, false, [0, 1, 0, 0, 0, 0, 0]),
    (&["Ounces", "oz"], 45_359_237, 1_600_000_000, false, [0, 1, 0, 0, 0, 0, 0]),
    (&["Seconds", "s", "sec"], 1, 1, false, [0, 0, 1, 0, 0, 0, 0]),
    (&["Milliseconds", "ms"], 1, 1000, false, [0, 0, 1, 0, 0, 0, 0]),
    (&["Microseconds"], 1, 1_000_000, false, [0, 0, 1, 0, 0, 0, 0]),
    (&["Minutes", "min"], 60, 1, false, [0, 0, 1, 0, 0, 0, 0]),
    (&["Hours", "h", "hr"], 3600, 1, false, [0, 0, 1, 0, 0, 0, 0]),
    (&["Days", "d"], 86400, 1, false, [0, 0, 1, 0, 0, 0, 0]),
    (&["Weeks"], 604_800, 1, false, [0, 0, 1, 0, 0, 0, 0]),
    (&["Years", "yr"], 31_557_600, 1, false, [0, 0, 1, 0, 0, 0, 0]),
    (&["Amperes", "A"], 1, 1, false, [0, 0, 0, 1, 0, 0, 0]),
    (&["Kelvins", "K"], 1, 1, false, [0, 0, 0, 0, 1, 0, 0]),
    (&["DegreesCelsius", "Celsius"], 1, 1, false, [0, 0, 0, 0, 1, 0, 0]),
    (&["DegreesFahrenheit", "Fahrenheit"], 5, 9, false, [0, 0, 0, 0, 1, 0, 0]),
    (&["Moles", "mol"], 1, 1, false, [0, 0, 0, 0, 0, 1, 0]),
    (&["Candelas", "cd"], 1, 1, false, [0, 0, 0, 0, 0, 0, 1]),
    (&["Newtons", "N"], 1, 1, false, [1, 1, -2, 0, 0, 0, 0]),
    (&["Joules", "J"], 1, 1, false, [2, 1, -2, 0, 0, 0, 0]),
    (&["Kilojoules", "kJ"], 1000, 1, false, [2, 1, -2, 0, 0, 0, 0]),
    (&["Calories", "cal", "ThermochemicalCalories"], 523, 125, false, [2, 1, -2, 0, 0, 0, 0]),
    (&["LargeCalories", "Kilocalories", "kcal"], 4184, 1, false, [2, 1, -2, 0, 0, 0, 0]),
    (&["Electronvolts", "eV"], 801_088_317, 5_000_000_000_000_000_000_000_000_000, false, [2, 1, -2, 0, 0, 0, 0]),
    (&["Megaelectronvolts", "MeV"], 801_088_317, 5_000_000_000_000_000_000_000, false, [2, 1, -2, 0, 0, 0, 0]),
    (&["Watts", "W"], 1, 1, false, [2, 1, -3, 0, 0, 0, 0]),
    (&["Kilowatts", "kW"], 1000, 1, false, [2, 1, -3, 0, 0, 0, 0]),
    (&["Pascals", "Pa"], 1, 1, false, [-1, 1, -2, 0, 0, 0, 0]),
    (&["Bars", "bar"], 100_000, 1, false, [-1, 1, -2, 0, 0, 0, 0]),
    (&["Atmospheres", "atm"], 101_325, 1, false, [-1, 1, -2, 0, 0, 0, 0]),
    (&["Hertz", "Hz"], 1, 1, false, [0, 0, -1, 0, 0, 0, 0]),
    (&["Coulombs", "C"], 1, 1, false, [0, 0, 1, 1, 0, 0, 0]),
    (&["Volts", "V"], 1, 1, false, [2, 1, -3, -1, 0, 0, 0]),
    (&["Ohms"], 1, 1, false, [2, 1, -3, -2, 0, 0, 0]),
    (&["Farads", "F"], 1, 1, false, [-2, -1, 4, 2, 0, 0, 0]),
    (&["Teslas", "T"], 1, 1, false, [0, 1, -2, -1, 0, 0, 0]),
    (&["Sieverts", "Sv", "Grays"], 1, 1, false, [2, 0, -2, 0, 0, 0, 0]),
    (&["Liters", "L"], 1, 1000, false, [3, 0, 0, 0, 0, 0, 0]),
    (&["Milliliters", "mL"], 1, 1_000_000, false, [3, 0, 0, 0, 0, 0, 0]),
    (&["Gallons", "gal"], 473_176_473, 125_000_000_000, false, [3, 0, 0, 0, 0, 0, 0]),
    (&["Hectares"], 10_000, 1, false, [2, 0, 0, 0, 0, 0, 0]),
    (&["Acres"], 316_160_658, 78_125, false, [2, 0, 0, 0, 0, 0, 0]),
    (&["MetersPerSecond"], 1, 1, false, [1, 0, -1, 0, 0, 0, 0]),
    (&["KilometersPerHour"], 5, 18, false, [1, 0, -1, 0, 0, 0, 0]),
    (&["MilesPerHour", "mph"], 1397, 3125, false, [1, 0, -1, 0, 0, 0, 0]),
    (&["Radians", "rad"], 1, 1, false, [0, 0, 0, 0, 0, 0, 0]),
    (&["AngularDegrees", "Degrees", "deg"], 1, 180, true, [0, 0, 0, 0, 0, 0, 0]),
    (&["Percent", "%"], 1, 100, false, [0, 0, 0, 0, 0, 0, 0]),
    (&["PartsPerMillion", "ppm"], 1, 1_000_000, false, [0, 0, 0, 0, 0, 0, 0]),
    (&["Dozens", "dozens"], 12, 1, false, [0, 0, 0, 0, 0, 0, 0]),
    // physical constants as units (CODATA 2018 values; c, e, k and N_A are exact in the SI since 2019)
    (&["SpeedOfLight"], 299_792_458, 1, false, [1, 0, -1, 0, 0, 0, 0]),
    (&["GravitationalConstant"], 66_743, 1_000_000_000_000_000, false, [3, -1, -2, 0, 0, 0, 0]),
    (&["StandardAccelerationOfGravity"], 196_133, 20_000, false, [1, 0, -2, 0, 0, 0, 0]),
    (&["ElementaryCharge"], 801_088_317, 5_000_000_000_000_000_000_000_000_000, false, [0, 0, 1, 1, 0, 0, 0]),
    (&["BoltzmannConstant"], 1_380_649, 100_000_000_000_000_000_000_000_000_000, false, [2, 1, -2, 0, -1, 0, 0]),
    (&["AvogadroConstant"], 602_214_076_000_000_000_000_000, 1, false, [0, 0, 0, 0, 0, -1, 0]),
];

/// The heap and the arithmetic, all the unit analysis needs (so comparisons outside a native can use it).
struct U<'a, 'h> {
    h: &'h Heap,
    ar: &'a Arith<'h, Heap>,
}

impl<'h> U<'_, 'h> {
    fn sym(&self, name: &str) -> T<'h> {
        self.h.sym_term(self.h.sym(name))
    }
    fn int(&self, v: i64) -> T<'h> {
        self.h.int(v)
    }
    fn mul(&self, xs: &[T<'h>]) -> T<'h> {
        self.ar.mul(xs)
    }
    fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.div(a, b)
    }
    fn pow(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.pow(a, b)
    }
    fn int_of(&self, t: T<'h>) -> Option<i64> {
        match self.h.view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }
}

fn ux<'a, 'h>(fx: &'a Fx<'_, '_, '_, 'h>) -> U<'a, 'h> {
    U { h: fx.h, ar: &fx.ar }
}

/// The SI conversion of a unit term: (factor as a term, dimensions); unknown names are their own dimensions.
fn analyse<'h>(fx: &U<'_, 'h>, u: T<'h>, extra: &mut Vec<(String, i64)>) -> Option<(T<'h>, Dims)> {
    match fx.h.view(u) {
        View::Str(s) => {
            let s = s.to_string();
            match UNITS.iter().find(|e| e.0.contains(&s.as_str())) {
                Some(&(_, n, d, pi, dims)) => {
                    let num = fx.ar.int_from_decimal(&n.to_string())?;
                    let den = fx.ar.int_from_decimal(&d.to_string())?;
                    let mut f = fx.div(num, den);
                    if pi {
                        f = fx.mul(&[f, fx.sym("pi")]);
                    }
                    Some((f, dims))
                }
                None => {
                    extra.push((s, 1));
                    Some((fx.int(1), [0; 7]))
                }
            }
        }
        View::Int(1) => Some((fx.int(1), [0; 7])),
        View::App { head, args } if head == fx.sym("mul") => {
            let mut f = fx.int(1);
            let mut d = [0i64; 7];
            for &a in args {
                let (g, e) = analyse(fx, a, extra)?;
                f = fx.mul(&[f, g]);
                for k in 0..7 {
                    d[k] += e[k];
                }
            }
            Some((f, d))
        }
        View::App { head, args: &[b, e] } if head == fx.sym("pow") => {
            let k = fx.int_of(e)?;
            let mut sub = vec![];
            let (g, dims) = analyse(fx, b, &mut sub)?;
            for (s, p) in sub {
                extra.push((s, p * k));
            }
            Some((fx.pow(g, e), dims.map(|x| x * k)))
        }
        _ => None,
    }
}

struct Unit<'h> {
    factor: T<'h>,
    dims: Dims,
    other: Vec<(String, i64)>,
}

fn unit<'h>(fx: &Fx<'_, '_, '_, 'h>, u: T<'h>) -> Option<Unit<'h>> {
    unit_of(&ux(fx), u)
}

fn unit_of<'h>(fx: &U<'_, 'h>, u: T<'h>) -> Option<Unit<'h>> {
    let mut other = vec![];
    let (factor, dims) = analyse(fx, u, &mut other)?;
    // collect the powers of unknown names
    let mut merged: Vec<(String, i64)> = vec![];
    for (s, p) in other {
        match merged.iter_mut().find(|(t, _)| *t == s) {
            Some(e) => e.1 += p,
            None => merged.push((s, p)),
        }
    }
    merged.retain(|(_, p)| *p != 0);
    merged.sort();
    Some(Unit { factor, dims, other: merged })
}

fn compatible(a: &Unit<'_>, b: &Unit<'_>) -> bool {
    a.dims == b.dims && a.other == b.other
}

fn parts<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
    match fx.call_of(t, "quantity")? {
        &[m, u] => Some((m, u)),
        _ => None,
    }
}

/// `quantity(m, u)`, or m when u is 1.
fn make<'h>(fx: &mut Fx<'_, '_, '_, 'h>, m: T<'h>, u: T<'h>) -> Res<T<'h>> {
    let u = fx.eval(u)?;
    let m = fx.eval(m)?;
    Ok(if fx.is_int(u, 1) { m } else { fx.call("quantity", &[m, u]) })
}

/// Arithmetic and comparisons with quantities: `head` is add, mul or pow.
pub fn arith<'h>(fx: &mut Fx<'_, '_, '_, 'h>, head: &str, args: &[T<'h>]) -> R<'h> {
    match head {
        "add" => {
            // the terms in the smallest of their units (the common unit of a sum)
            let mut qs = vec![];
            for &a in args {
                let Some((m, u)) = parts(fx, a) else { return Ok(None) };
                let Some(un) = unit(fx, u) else { return Ok(None) };
                qs.push((m, u, un));
            }
            let mut best = 0;
            for k in 1..qs.len() {
                if !compatible(&qs[best].2, &qs[k].2) {
                    return Ok(Some(fx.failure(&["incompatible_units"])));
                }
                if fx.ar.compare(qs[k].2.factor, qs[best].2.factor) == Some(mc_arith::Cmp3::Less) {
                    best = k;
                }
            }
            let (u0, f0) = (qs[best].1, qs[best].2.factor);
            let terms: Vec<T<'h>> =
                qs.iter().map(|(m, u, un)| if *u == u0 { *m } else { fx.mul(&[*m, fx.div(un.factor, f0)]) }).collect();
            Ok(Some(make(fx, fx.add(&terms), u0)?))
        }
        "mul" => {
            let mut mags = vec![];
            let mut units = vec![];
            for &a in args {
                match parts(fx, a) {
                    Some((m, u)) => {
                        mags.push(m);
                        units.push(u);
                    }
                    None => mags.push(a),
                }
            }
            Ok(Some(make(fx, fx.mul(&mags), fx.mul(&units))?))
        }
        "pow" => {
            let Some((m, u)) = parts(fx, args[0]) else { return Ok(None) };
            if parts(fx, args[1]).is_some() {
                return Ok(None);
            }
            let u = unit_pow(fx, u, args[1])?;
            Ok(Some(make(fx, fx.pow(m, args[1]), u)?))
        }
        "sqrt" => {
            let [q] = args else { return Ok(None) };
            let half = fx.rat(1, 2);
            arith(fx, "pow", &[*q, half])
        }
        // functions of the magnitude that keep the unit
        "abs" | "re" | "im_part" | "conjugate" | "rationalize" | "floor" | "ceiling" | "round"
        | "integer_part" | "fractional_part" => {
            let Some((m, u)) = parts(fx, args[0]) else { return Ok(None) };
            let mut rest = vec![m];
            for &a in &args[1..] {
                match parts(fx, a) {
                    // round(q, step) with a step in a compatible unit: the step in the unit of q
                    Some(_) => match common(fx, args[0], a) {
                        Some((_, s)) => rest.push(s),
                        None => return Ok(Some(fx.failure(&["incompatible_units"]))),
                    },
                    None => rest.push(a),
                }
            }
            let v = fx.call(head, &rest);
            Ok(Some(make(fx, v, u)?))
        }
        // mod(q1, q2) in the unit of q1; quotient(q1, q2) a number
        "mod" | "quotient" => {
            let [a, b] = args else { return Ok(None) };
            let Some((m, u)) = parts(fx, *a) else { return Ok(None) };
            if parts(fx, *b).is_none() {
                return Ok(None);
            }
            let Some((_, s)) = common(fx, *a, *b) else { return Ok(Some(fx.failure(&["incompatible_units"]))) };
            let v = fx.call(head, &[m, s]);
            if head == "quotient" {
                return Ok(Some(fx.eval(v)?));
            }
            Ok(Some(make(fx, v, u)?))
        }
        // circular functions of an angle (a dimensionless unit such as degrees): of its value in radians
        "sin" | "cos" | "tan" | "cot" | "sec" | "csc" => {
            let [q] = args else { return Ok(None) };
            let Some((m, u)) = parts(fx, *q) else { return Ok(None) };
            let Some(un) = unit(fx, u) else { return Ok(None) };
            if un.dims != [0; 7] || !un.other.is_empty() {
                return Ok(None);
            }
            let x = fx.mul(&[m, un.factor]);
            Ok(Some(fx.eval(fx.call(head, &[x]))?))
        }
        // functions of the magnitude without a unit
        "sign" | "is_positive" | "is_negative" | "is_real" | "is_zero" => {
            let [q] = args else { return Ok(None) };
            let Some((m, _)) = parts(fx, *q) else { return Ok(None) };
            Ok(Some(fx.eval(fx.call(head, &[m]))?))
        }
        _ => Ok(None),
    }
}

/// A unit to a power: the exponents of the unit names multiplied (units are positive, so `("Meters"^2)^(1/2)` is
/// `"Meters"`).
fn unit_pow<'h>(fx: &mut Fx<'_, '_, '_, 'h>, u: T<'h>, e: T<'h>) -> Res<T<'h>> {
    let v = match fx.h.view(u) {
        View::App { head, args } if head == fx.sym("mul") => {
            let mut fs = Vec::with_capacity(args.len());
            for &a in args {
                fs.push(unit_pow(fx, a, e)?);
            }
            fx.mul(&fs)
        }
        View::App { head, args: &[b, k] } if head == fx.sym("pow") => {
            let ke = fx.eval(fx.mul(&[k, e]))?;
            fx.pow(b, ke)
        }
        _ => fx.pow(u, e),
    };
    fx.eval(v)
}

/// The magnitudes of two quantities in a common unit (the first's), for comparisons.
pub fn common<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: T<'h>, b: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let (ma, ua) = parts(fx, a)?;
    let (mb, ub) = parts(fx, b)?;
    let (x, y) = (unit(fx, ua)?, unit(fx, ub)?);
    if !compatible(&x, &y) {
        return None;
    }
    Some((ma, fx.mul(&[mb, fx.div(y.factor, x.factor)])))
}

/// The magnitudes of two terms in a common unit, for comparisons outside a native: quantities of compatible units
/// (the first's unit), or a quantity of a dimensionless unit against a number; `None` when neither is a quantity or
/// the units do not match.
pub fn common_magnitudes<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, a: T<'h>, b: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let u = U { h, ar };
    let q = u.sym("quantity");
    let split = |t: T<'h>| match h.view(t) {
        View::App { head, args: &[m, un] } if head == q => Some((m, un)),
        _ => None,
    };
    let (pa, pb) = (split(a), split(b));
    let one = h.int(1);
    let ((ma, ua), (mb, ub)) = match (pa, pb) {
        (None, None) => return None,
        (Some(x), Some(y)) => (x, y),
        (Some(x), None) => (x, (b, one)),
        (None, Some(y)) => ((a, one), y),
    };
    let (x, y) = (unit_of(&u, ua)?, unit_of(&u, ub)?);
    if !compatible(&x, &y) {
        return None;
    }
    let mb = if ub == ua { mb } else { ar.mul(&[mb, ar.div(y.factor, x.factor)]) };
    Some((ma, mb))
}

fn temperature(name: &str) -> Option<u8> {
    match name {
        "Kelvins" | "K" => Some(0),
        "DegreesCelsius" | "Celsius" => Some(1),
        "DegreesFahrenheit" | "Fahrenheit" => Some(2),
        _ => None,
    }
}

fn unit_convert<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some((m, u)) = parts(fx, a[0]) else { return Ok(None) };
    let Some(src) = unit(fx, u) else { return Ok(None) };
    let target = match a.get(1) {
        Some(&t) => match parts(fx, t) {
            Some((_, tu)) => tu,
            None => t,
        },
        None => {
            // SI base units
            let names = ["Meters", "Kilograms", "Seconds", "Amperes", "Kelvins", "Moles", "Candelas"];
            let mut fs = vec![];
            for (k, n) in names.iter().enumerate() {
                if src.dims[k] != 0 {
                    fs.push(fx.pow(fx.h.string(n), fx.int(src.dims[k])));
                }
            }
            for (s, p) in &src.other {
                fs.push(fx.pow(fx.h.string(s), fx.int(*p)));
            }
            fx.mul(&fs)
        }
    };
    // affine temperatures
    if let (View::Str(s), View::Str(t)) = (fx.h.view(u), fx.h.view(target)) {
        if let (Some(from), Some(to)) = (temperature(s), temperature(t)) {
            // to kelvin, then to the target
            let k = match from {
                0 => m,
                1 => fx.add(&[m, fx.rat(5463, 20)]),
                _ => fx.add(&[fx.mul(&[fx.rat(5, 9), m]), fx.rat(45967, 180)]),
            };
            let v = match to {
                0 => k,
                1 => fx.sub(k, fx.rat(5463, 20)),
                _ => fx.sub(fx.mul(&[fx.rat(9, 5), k]), fx.rat(45967, 100)),
            };
            return Ok(Some(make(fx, v, target)?));
        }
    }
    let Some(dst) = unit(fx, target) else { return Ok(None) };
    if !compatible(&src, &dst) {
        return Ok(Some(fx.failure(&["incompatible_units"])));
    }
    let v = fx.mul(&[m, fx.div(src.factor, dst.factor)]);
    Ok(Some(make(fx, v, target)?))
}

fn quantity_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    match a {
        // quantity("Meters") is one meter
        [u] if matches!(fx.h.view(*u), View::Str(_)) => Ok(Some(fx.call("quantity", &[fx.int(1), *u]))),
        [m, u] if fx.is_int(*u, 1) => Ok(Some(*m)),
        // a list of magnitudes: the list of quantities
        [m, u] if fx.as_list(*m).is_some() => {
            let items: Vec<T<'h>> = fx.as_list(*m).unwrap_or(&[]).iter().map(|&x| fx.call("quantity", &[x, *u])).collect();
            Ok(Some(fx.list(&items)))
        }
        // a quantity of a quantity: the inner magnitude in the product unit
        [m, u] => match parts(fx, *m) {
            Some((mm, mu)) => Ok(Some(fx.call("quantity", &[mm, fx.mul(&[mu, *u])]))),
            None => Ok(None),
        },
        _ => Ok(None),
    }
}

fn magnitude_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a.len() == 2 {
        let Some(q) = unit_convert(fx, a)? else { return Ok(None) };
        return Ok(Some(parts(fx, q).map_or(q, |(m, _)| m)));
    }
    Ok(parts(fx, a[0]).map(|(m, _)| m).or_else(|| fx.num(a[0]).map(|_| a[0])))
}

fn unit_e<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    Ok(parts(fx, a[0]).map(|(_, u)| u))
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    Def {
        name: "quantity",
        arities: &[1, 2],
        listable: false,
        exact: Some(quantity_e),
        numeric: Some(quantity_e),
        summary: "quantity(m, unit): the magnitude m in a unit (a string such as \"Meters\", or products and powers of them), with conversions in arithmetic",
    },
    Def {
        name: "unit_convert",
        arities: &[1, 2],
        listable: false,
        exact: Some(unit_convert),
        numeric: None,
        summary: "unit_convert(q, unit): the quantity q in another unit of the same dimension; unit_convert(q): in SI base units",
    },
    Def {
        name: "quantity_magnitude",
        arities: &[1, 2],
        listable: false,
        exact: Some(magnitude_e),
        numeric: None,
        summary: "quantity_magnitude(q): the magnitude of a quantity; quantity_magnitude(q, unit): its magnitude in that unit",
    },
    Def {
        name: "quantity_unit",
        arities: &[1],
        listable: false,
        exact: Some(unit_e),
        numeric: None,
        summary: "quantity_unit(q): the unit of a quantity",
    },
];
