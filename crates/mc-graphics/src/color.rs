//! Colours: the colour terms (`rgb_color`, `hue`, `gray_level` and the named colours), reading them as [`Rgba`],
//! and conversion between the colour models.
use crate::num::number;
use mc_api::{Store, View, T};

/// A colour with straight (not premultiplied) alpha, every component in [0, 1].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    /// red
    pub r: f64,
    /// green
    pub g: f64,
    /// blue
    pub b: f64,
    /// opacity
    pub a: f64,
}

impl Rgba {
    /// Black, the default colour of lines, points and text.
    pub const BLACK: Rgba = Rgba::rgb(0.0, 0.0, 0.0);
    /// White, the default background.
    pub const WHITE: Rgba = Rgba::rgb(1.0, 1.0, 1.0);

    /// An opaque colour.
    pub const fn rgb(r: f64, g: f64, b: f64) -> Rgba {
        Rgba { r, g, b, a: 1.0 }
    }

    /// The colour with every component clamped to [0, 1] (components outside, and NaN, are clipped).
    pub fn clamped(self) -> Rgba {
        let c = |v: f64| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
        Rgba { r: c(self.r), g: c(self.g), b: c(self.b), a: c(self.a) }
    }

    /// The components as bytes (red, green, blue, alpha), rounded.
    pub fn bytes(self) -> [u8; 4] {
        let c = self.clamped();
        let b = |v: f64| (v * 255.0).round() as u8;
        [b(c.r), b(c.g), b(c.b), b(c.a)]
    }

    /// `#rrggbb` (the opacity is not part of it).
    pub fn hex(self) -> String {
        let [r, g, b, _] = self.bytes();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// The colour `t` of the way from `self` to `other` (0: `self`, 1: `other`), opacity included.
    pub fn mix(self, other: Rgba, t: f64) -> Rgba {
        let m = |a: f64, b: f64| a + (b - a) * t;
        Rgba { r: m(self.r, other.r), g: m(self.g, other.g), b: m(self.b, other.b), a: m(self.a, other.a) }
    }

    /// The same colour with its opacity multiplied by `k`.
    pub fn faded(self, k: f64) -> Rgba {
        Rgba { a: self.a * k, ..self }
    }

    /// The luminance as a gray level (ITU-R BT.601 weights).
    pub fn gray(self) -> f64 {
        0.299 * self.r + 0.587 * self.g + 0.114 * self.b
    }

    /// Hue, saturation and brightness, each in [0, 1] (hue 0 is red, a gray has hue 0).
    pub fn hsb(self) -> (f64, f64, f64) {
        let c = self.clamped();
        let max = c.r.max(c.g).max(c.b);
        let min = c.r.min(c.g).min(c.b);
        let d = max - min;
        let s = if max > 0.0 { d / max } else { 0.0 };
        if d == 0.0 {
            return (0.0, s, max);
        }
        let h = if max == c.r {
            ((c.g - c.b) / d).rem_euclid(6.0)
        } else if max == c.g {
            (c.b - c.r) / d + 2.0
        } else {
            (c.r - c.g) / d + 4.0
        };
        (h / 6.0, s, max)
    }

    /// The colour of hue `h` (cyclic: 0 and 1 are red), saturation `s` and brightness `v`.
    pub fn from_hsb(h: f64, s: f64, v: f64) -> Rgba {
        let h = h.rem_euclid(1.0) * 6.0;
        let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let k = h.floor();
        let f = h - k;
        let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
        let (r, g, b) = match k as i64 {
            0 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        Rgba::rgb(r, g, b)
    }
}

/// The named colours: symbols that stand for a colour wherever one is expected.
pub const NAMED: &[(&str, Rgba)] = &[
    ("black", Rgba::rgb(0.0, 0.0, 0.0)),
    ("white", Rgba::rgb(1.0, 1.0, 1.0)),
    ("gray", Rgba::rgb(0.5, 0.5, 0.5)),
    ("light_gray", Rgba::rgb(0.85, 0.85, 0.85)),
    ("dark_gray", Rgba::rgb(0.25, 0.25, 0.25)),
    ("red", Rgba::rgb(1.0, 0.0, 0.0)),
    ("green", Rgba::rgb(0.0, 1.0, 0.0)),
    ("blue", Rgba::rgb(0.0, 0.0, 1.0)),
    ("cyan", Rgba::rgb(0.0, 1.0, 1.0)),
    ("magenta", Rgba::rgb(1.0, 0.0, 1.0)),
    ("yellow", Rgba::rgb(1.0, 1.0, 0.0)),
    ("orange", Rgba::rgb(1.0, 0.5, 0.0)),
    ("purple", Rgba::rgb(0.5, 0.0, 0.5)),
    ("brown", Rgba::rgb(0.6, 0.4, 0.2)),
    ("pink", Rgba::rgb(1.0, 0.5, 0.5)),
    ("light_red", Rgba::rgb(1.0, 0.85, 0.85)),
    ("light_green", Rgba::rgb(0.88, 1.0, 0.88)),
    ("light_blue", Rgba::rgb(0.87, 0.94, 1.0)),
    ("light_yellow", Rgba::rgb(1.0, 1.0, 0.85)),
    ("light_orange", Rgba::rgb(1.0, 0.9, 0.8)),
    ("light_purple", Rgba::rgb(0.94, 0.88, 0.94)),
    ("light_brown", Rgba::rgb(0.94, 0.91, 0.88)),
    ("light_pink", Rgba::rgb(1.0, 0.925, 0.94)),
    ("light_cyan", Rgba::rgb(0.9, 1.0, 1.0)),
    ("light_magenta", Rgba::rgb(1.0, 0.9, 1.0)),
    ("dark_red", Rgba::rgb(0.55, 0.0, 0.0)),
    ("dark_green", Rgba::rgb(0.0, 0.39, 0.0)),
    ("dark_blue", Rgba::rgb(0.0, 0.0, 0.55)),
    ("dark_yellow", Rgba::rgb(0.6, 0.6, 0.0)),
    ("dark_orange", Rgba::rgb(0.8, 0.4, 0.0)),
    ("dark_purple", Rgba::rgb(0.35, 0.0, 0.35)),
    ("dark_brown", Rgba::rgb(0.4, 0.26, 0.13)),
    ("dark_pink", Rgba::rgb(0.8, 0.35, 0.45)),
    ("dark_cyan", Rgba::rgb(0.0, 0.55, 0.55)),
    ("dark_magenta", Rgba::rgb(0.55, 0.0, 0.55)),
    // the standard colours: softer hues that read on light and dark backgrounds alike
    ("standard_red", Rgba::rgb(0.86, 0.24, 0.24)),
    ("standard_green", Rgba::rgb(0.30, 0.66, 0.30)),
    ("standard_blue", Rgba::rgb(0.24, 0.48, 0.80)),
    ("standard_yellow", Rgba::rgb(0.93, 0.76, 0.18)),
    ("standard_orange", Rgba::rgb(0.95, 0.55, 0.18)),
    ("standard_purple", Rgba::rgb(0.58, 0.38, 0.74)),
    ("standard_brown", Rgba::rgb(0.62, 0.43, 0.27)),
    ("standard_pink", Rgba::rgb(0.93, 0.50, 0.66)),
    ("standard_cyan", Rgba::rgb(0.24, 0.72, 0.80)),
    ("standard_magenta", Rgba::rgb(0.80, 0.30, 0.70)),
    ("standard_gray", Rgba::rgb(0.55, 0.55, 0.55)),
    ("transparent", Rgba { r: 1.0, g: 1.0, b: 1.0, a: 0.0 }),
];

/// The colour named `name`.
pub fn named(name: &str) -> Option<Rgba> {
    NAMED.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// What a term is as a colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Colour {
    /// a colour with numeric components
    Value(Rgba),
    /// a colour term with a component that is not a number yet (`rgb_color(a, 0, 0)`), or a symbol that is not a
    /// colour name: it may become a colour
    Symbolic,
    /// not a colour: a colour head with the wrong arguments, a string, a number
    Bad,
}

/// The colour heads.
pub const HEADS: [&str; 8] = ["rgb_color", "hue", "gray_level", "cmyk_color", "lab_color", "xyz_color", "luv_color", "lch_color"];

/// Read `t` as a colour: a named colour, `rgb_color(r, g, b)` or `rgb_color(r, g, b, a)`, `hue(h)`,
/// `hue(h, s, b)` or `hue(h, s, b, a)`, `gray_level(g)` or `gray_level(g, a)`.
pub fn colour<S: Store + ?Sized>(h: &S, t: T<'_>) -> Colour {
    match h.view(t) {
        View::Sym(s) => match named(&h.sym_name(s)) {
            Some(c) => Colour::Value(c),
            None => Colour::Symbolic,
        },
        // "#rrggbb", as plot options write colours
        View::Str(s) => match hex_color(s) {
            Some(c) => Colour::Value(c),
            None => Colour::Bad,
        },
        View::App { head, args } => {
            let View::Sym(f) = h.view(head) else { return Colour::Bad };
            let name = h.sym_name(f);
            if name == "theme_color" && args.len() == 1 {
                // the theme's accent colours, as the standard palette
                let k = match h.view(args[0]) {
                    View::Str(s) => s.strip_prefix("Accent").and_then(|d| d.parse::<usize>().ok()),
                    _ => None,
                };
                return match k {
                    Some(k @ 1..=6) => Colour::Value(NAMED.iter().filter(|(n, _)| n.starts_with("standard_")).nth(k - 1).map_or(Rgba::BLACK, |c| c.1)),
                    _ => Colour::Value(Rgba::rgb(0.24, 0.48, 0.80)),
                };
            }
            if name == "light_dark_switched" && !args.is_empty() {
                // the light theme's colour (pictures are written for a light page; chrome follows the theme)
                return colour(h, args[0]);
            }
            if !HEADS.contains(&name.as_str()) {
                return Colour::Bad;
            }
            let mut v = [0.0; 5];
            let mut symbolic = false;
            for (k, &a) in args.iter().take(5).enumerate() {
                match number(h, a) {
                    Some(x) => v[k] = x,
                    None if is_symbolic(h, a) => symbolic = true,
                    None => return Colour::Bad,
                }
            }
            let shape_ok = match name.as_str() {
                "rgb_color" => matches!(args.len(), 3 | 4),
                "hue" => matches!(args.len(), 1..=4),
                "cmyk_color" => matches!(args.len(), 4 | 5),
                "lab_color" | "xyz_color" | "luv_color" | "lch_color" => matches!(args.len(), 3 | 4),
                _ => matches!(args.len(), 1 | 2),
            };
            if !shape_ok {
                return Colour::Bad;
            }
            if symbolic {
                return Colour::Symbolic;
            }
            let c = match (name.as_str(), args.len()) {
                ("rgb_color", n) => Rgba { r: v[0], g: v[1], b: v[2], a: if n == 4 { v[3] } else { 1.0 } },
                ("hue", 1) => Rgba::from_hsb(v[0], 1.0, 1.0),
                ("hue", 2) => Rgba::from_hsb(v[0], v[1], 1.0),
                ("hue", n) => Rgba { a: if n == 4 { v[3] } else { 1.0 }, ..Rgba::from_hsb(v[0], v[1], v[2]) },
                ("cmyk_color", n) => {
                    let k = 1.0 - v[3];
                    Rgba { r: (1.0 - v[0]) * k, g: (1.0 - v[1]) * k, b: (1.0 - v[2]) * k, a: if n == 5 { v[4] } else { 1.0 } }
                }
                ("lab_color", n) => Rgba { a: if n == 4 { v[3] } else { 1.0 }, ..from_lab(v[0], v[1], v[2]) },
                ("lch_color", n) => {
                    let t = 2.0 * std::f64::consts::PI * v[2];
                    Rgba { a: if n == 4 { v[3] } else { 1.0 }, ..from_lab(v[0], v[1] * t.cos(), v[1] * t.sin()) }
                }
                ("xyz_color", n) => Rgba { a: if n == 4 { v[3] } else { 1.0 }, ..from_xyz([v[0], v[1], v[2]]) },
                ("luv_color", n) => Rgba { a: if n == 4 { v[3] } else { 1.0 }, ..from_luv(v[0], v[1], v[2]) },
                (_, n) => Rgba { a: if n == 2 { v[1] } else { 1.0 }, ..Rgba::rgb(v[0], v[0], v[0]) },
            };
            Colour::Value(c)
        }
        _ => Colour::Bad,
    }
}

// ---- colour spaces (sRGB with its D65 white, adapted to D50 for XYZ; CIE 1976 L*a*b* and L*u*v* with L in [0, 1] and the chromatic
// coordinates divided by 100, so that the components are of order one; IEC 61966-2-1 for the sRGB transfer curve)

/// The white point of L*a*b* and L*u*v*: D50 in XYZ.
pub const WHITE: [f64; 3] = [D50.0 / D50.1, 1.0, (1.0 - D50.0 - D50.1) / D50.1];

fn linear(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn gamma(c: f64) -> f64 {
    if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.max(0.0).powf(1.0 / 2.4) - 0.055 }
}

/// The chromaticities used: sRGB's primaries and its D65 white (0.31271, 0.32902), and the D50 white (0.34567,
/// 0.35850), to five digits as CIE 15 tabulates them.
const PRIMARIES: [(f64, f64); 3] = [(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)];
const D65: (f64, f64) = (0.31271, 0.32902);
const D50: (f64, f64) = (0.34567, 0.35850);

/// A 3 x 3 matrix by rows.
type Mat3 = [[f64; 3]; 3];

fn xyz_of((x, y): (f64, f64)) -> [f64; 3] {
    [x / y, 1.0, (1.0 - x - y) / y]
}

fn mul3(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut c = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            c[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    c
}

fn apply3(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn inv3(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let d = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let c = |i: usize, j: usize| {
        let (r0, r1) = ((i + 1) % 3, (i + 2) % 3);
        let (c0, c1) = ((j + 1) % 3, (j + 2) % 3);
        m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0]
    };
    // the adjugate is the transposed cofactor matrix
    [0, 1, 2].map(|i| [0, 1, 2].map(|j| c(j, i) / d))
}

/// The linear sRGB to XYZ (D50) matrix: the primaries scaled to the D65 white, then adapted to D50 with the
/// Bradford transform (Lam 1985).
fn rgb_to_xyz() -> &'static (Mat3, Mat3) {
    static M: std::sync::OnceLock<(Mat3, Mat3)> = std::sync::OnceLock::new();
    M.get_or_init(|| {
        let cols = PRIMARIES.map(xyz_of);
        let p = [0, 1, 2].map(|i| [cols[0][i], cols[1][i], cols[2][i]]);
        let s = apply3(&inv3(&p), xyz_of(D65));
        let m65 = [0, 1, 2].map(|i| [0, 1, 2].map(|j| p[i][j] * s[j]));
        let b = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];
        let (ws, wd) = (apply3(&b, xyz_of(D65)), apply3(&b, xyz_of(D50)));
        let scale = [[wd[0] / ws[0], 0.0, 0.0], [0.0, wd[1] / ws[1], 0.0], [0.0, 0.0, wd[2] / ws[2]]];
        let a = mul3(&inv3(&b), &mul3(&scale, &b));
        let m = mul3(&a, &m65);
        (m, inv3(&m))
    })
}

/// sRGB to CIE XYZ relative to the D50 white (the print white L*a*b* is given against).
pub fn to_xyz(c: Rgba) -> [f64; 3] {
    apply3(&rgb_to_xyz().0, [linear(c.r), linear(c.g), linear(c.b)])
}

/// CIE XYZ (D50) to sRGB (not clamped: colours outside the gamut keep components outside [0, 1]).
pub fn from_xyz(v: [f64; 3]) -> Rgba {
    let [r, g, b] = apply3(&rgb_to_xyz().1, v);
    Rgba::rgb(gamma(r), gamma(g), gamma(b))
}

const EPS: f64 = 216.0 / 24389.0;
const KAPPA: f64 = 24389.0 / 27.0;

/// sRGB to L*a*b* (L in [0, 1], a and b divided by 100).
pub fn to_lab(c: Rgba) -> [f64; 3] {
    let xyz = to_xyz(c);
    let f = |t: f64| if t > EPS { t.cbrt() } else { (KAPPA * t + 16.0) / 116.0 };
    let [fx, fy, fz] = [f(xyz[0] / WHITE[0]), f(xyz[1] / WHITE[1]), f(xyz[2] / WHITE[2])];
    [(116.0 * fy - 16.0) / 100.0, 5.0 * (fx - fy), 2.0 * (fy - fz)]
}

/// L*a*b* (scaled as [`to_lab`]) to sRGB.
pub fn from_lab(l: f64, a: f64, b: f64) -> Rgba {
    let fy = (100.0 * l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 5.0, fy - b / 2.0);
    let inv = |f: f64| if f * f * f > EPS { f * f * f } else { (116.0 * f - 16.0) / KAPPA };
    let y = if 100.0 * l > KAPPA * EPS { fy * fy * fy } else { 100.0 * l / KAPPA };
    from_xyz([inv(fx) * WHITE[0], y, inv(fz) * WHITE[2]])
}

fn uv_of([x, y, z]: [f64; 3]) -> (f64, f64) {
    let d = x + 15.0 * y + 3.0 * z;
    if d <= 0.0 { (0.0, 0.0) } else { (4.0 * x / d, 9.0 * y / d) }
}

/// sRGB to L*u*v* (L in [0, 1], u and v divided by 100).
pub fn to_luv(c: Rgba) -> [f64; 3] {
    let xyz = to_xyz(c);
    let yr = xyz[1] / WHITE[1];
    let l = if yr > EPS { 116.0 * yr.cbrt() - 16.0 } else { KAPPA * yr };
    let (u, v) = uv_of(xyz);
    let (un, vn) = uv_of(WHITE);
    [l / 100.0, 13.0 * l * (u - un) / 100.0, 13.0 * l * (v - vn) / 100.0]
}

/// L*u*v* (scaled as [`to_luv`]) to sRGB.
pub fn from_luv(l: f64, u: f64, v: f64) -> Rgba {
    let l = 100.0 * l;
    if l <= 0.0 {
        return Rgba::BLACK;
    }
    let (un, vn) = uv_of(WHITE);
    let (up, vp) = (100.0 * u / (13.0 * l) + un, 100.0 * v / (13.0 * l) + vn);
    let y = if l > KAPPA * EPS { ((l + 16.0) / 116.0).powi(3) } else { l / KAPPA };
    let x = y * 9.0 * up / (4.0 * vp);
    let z = y * (12.0 - 3.0 * up - 20.0 * vp) / (4.0 * vp);
    from_xyz([x, y, z])
}

/// CMYK components of an sRGB colour (the least black that reproduces it).
pub fn to_cmyk(c: Rgba) -> [f64; 4] {
    let k = 1.0 - c.r.max(c.g).max(c.b);
    if k >= 1.0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    [(1.0 - c.r - k) / (1.0 - k), (1.0 - c.g - k) / (1.0 - k), (1.0 - c.b - k) / (1.0 - k), k]
}

/// The CIE 1976 colour difference: the Euclidean distance in L*a*b* (scaled as [`to_lab`]).
pub fn distance(a: Rgba, b: Rgba) -> f64 {
    let (p, q) = (to_lab(a), to_lab(b));
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

/// The colour difference formulas: CIE 1976 (Euclidean in L*a*b*), CIE 1994 (graphic arts weights), CIEDE2000
/// (Sharma, Wu and Dalal 2005), CMC l:c (Clarke, McDonald and Rigg 1984, l = c = 1), and the lightness, chroma
/// and hue differences. On L*a*b* scaled as [`to_lab`]: the formulas' constants are for L from 0 to 100, so the
/// colours are scaled up and the difference down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Difference {
    /// CIE 1976
    Cie76,
    /// CIE 1994
    Cie94,
    /// CIEDE2000
    Cie2000,
    /// CMC l:c
    Cmc,
    /// |L1 - L2|
    DeltaL,
    /// |C1 - C2|
    DeltaC,
    /// the hue difference sqrt(dE^2 - dL^2 - dC^2)
    DeltaH,
}

impl Difference {
    /// The formula of a name (`"CIE76"`, `"CIE94"`, `"CIE2000"`, `"CMC"`, `"DeltaL"`, `"DeltaC"`, `"DeltaH"`).
    pub fn named(s: &str) -> Option<Difference> {
        Some(match s {
            "CIE76" | "Euclidean" => Difference::Cie76,
            "CIE94" => Difference::Cie94,
            "CIE2000" | "CIEDE2000" => Difference::Cie2000,
            "CMC" => Difference::Cmc,
            "DeltaL" => Difference::DeltaL,
            "DeltaC" => Difference::DeltaC,
            "DeltaH" => Difference::DeltaH,
            _ => return None,
        })
    }
}

/// The difference of two colours by formula `f` (the first colour is the standard where the formula is not
/// symmetric).
pub fn difference(a: Rgba, b: Rgba, f: Difference) -> f64 {
    let (p, q) = (to_lab(a).map(|v| 100.0 * v), to_lab(b).map(|v| 100.0 * v));
    let (dl, da, db) = (p[0] - q[0], p[1] - q[1], p[2] - q[2]);
    let (c1, c2) = (p[1].hypot(p[2]), q[1].hypot(q[2]));
    let dc = c1 - c2;
    let de = (dl * dl + da * da + db * db).sqrt();
    let dh2 = (da * da + db * db - dc * dc).max(0.0);
    let v = match f {
        Difference::Cie76 => de,
        Difference::DeltaL => dl.abs(),
        Difference::DeltaC => dc.abs(),
        Difference::DeltaH => dh2.sqrt(),
        Difference::Cie94 => {
            let (sc, sh) = (1.0 + 0.045 * c1, 1.0 + 0.015 * c1);
            (dl * dl + (dc / sc).powi(2) + dh2 / (sh * sh)).sqrt()
        }
        Difference::Cmc => {
            let (l, c) = (1.0, 1.0);
            let h1 = p[2].atan2(p[1]).to_degrees().rem_euclid(360.0);
            let t = if (164.0..=345.0).contains(&h1) {
                0.56 + (0.2 * (h1 + 168.0).to_radians().cos()).abs()
            } else {
                0.36 + (0.4 * (h1 + 35.0).to_radians().cos()).abs()
            };
            let f4 = (c1.powi(4) / (c1.powi(4) + 1900.0)).sqrt();
            let sl = if p[0] < 16.0 { 0.511 } else { 0.040975 * p[0] / (1.0 + 0.01765 * p[0]) };
            let sc = 0.0638 * c1 / (1.0 + 0.0131 * c1) + 0.638;
            let sh = sc * (f4 * t + 1.0 - f4);
            ((dl / (l * sl)).powi(2) + (dc / (c * sc)).powi(2) + dh2 / (sh * sh)).sqrt()
        }
        Difference::Cie2000 => ciede2000(p, q),
    };
    v / 100.0
}

fn ciede2000(p: [f64; 3], q: [f64; 3]) -> f64 {
    let c_bar = (p[1].hypot(p[2]) + q[1].hypot(q[2])) / 2.0;
    let g = 0.5 * (1.0 - (c_bar.powi(7) / (c_bar.powi(7) + 25f64.powi(7))).sqrt());
    let (a1, a2) = ((1.0 + g) * p[1], (1.0 + g) * q[1]);
    let (c1, c2) = (a1.hypot(p[2]), a2.hypot(q[2]));
    let hue = |b: f64, a: f64| if a == 0.0 && b == 0.0 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
    let (h1, h2) = (hue(p[2], a1), hue(q[2], a2));
    let dl = q[0] - p[0];
    let dc = c2 - c1;
    let dh = if c1 * c2 == 0.0 {
        0.0
    } else if (h2 - h1).abs() <= 180.0 {
        h2 - h1
    } else if h2 - h1 > 180.0 {
        h2 - h1 - 360.0
    } else {
        h2 - h1 + 360.0
    };
    let dhh = 2.0 * (c1 * c2).sqrt() * (dh / 2.0).to_radians().sin();
    let l_bar = (p[0] + q[0]) / 2.0;
    let cp_bar = (c1 + c2) / 2.0;
    let h_bar = if c1 * c2 == 0.0 {
        h1 + h2
    } else if (h1 - h2).abs() <= 180.0 {
        (h1 + h2) / 2.0
    } else if h1 + h2 < 360.0 {
        (h1 + h2 + 360.0) / 2.0
    } else {
        (h1 + h2 - 360.0) / 2.0
    };
    let t = 1.0 - 0.17 * (h_bar - 30.0).to_radians().cos() + 0.24 * (2.0 * h_bar).to_radians().cos()
        + 0.32 * (3.0 * h_bar + 6.0).to_radians().cos()
        - 0.20 * (4.0 * h_bar - 63.0).to_radians().cos();
    let d_theta = 30.0 * (-((h_bar - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (cp_bar.powi(7) / (cp_bar.powi(7) + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (l_bar - 50.0).powi(2) / (20.0 + (l_bar - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cp_bar;
    let sh = 1.0 + 0.015 * cp_bar * t;
    let rt = -(2.0 * d_theta).to_radians().sin() * rc;
    ((dl / sl).powi(2) + (dc / sc).powi(2) + (dhh / sh).powi(2) + rt * (dc / sc) * (dhh / sh)).sqrt()
}

/// Whether `t` may still become a number: a symbol or a compound term that is not a list (`a`, `x + 1`).
pub fn is_symbolic<S: Store + ?Sized>(h: &S, t: T<'_>) -> bool {
    match h.view(t) {
        View::Sym(_) => true,
        View::App { head, .. } => !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::LIST),
        _ => false,
    }
}

/// A colour written `#rrggbb` (hexadecimal red, green, blue).
pub fn hex_color(s: &str) -> Option<Rgba> {
    let short = s.strip_prefix('#').filter(|d| d.len() == 3 && d.is_ascii()).map(|d| d.chars().flat_map(|c| [c, c]).collect::<String>());
    let d = match &short {
        Some(d) => d.as_str(),
        None => s.strip_prefix('#').filter(|d| d.len() == 6 && d.is_ascii())?,
    };
    let c = |i: usize| u8::from_str_radix(&d[i..i + 2], 16).ok().map(|v| v as f64 / 255.0);
    Some(Rgba::rgb(c(0)?, c(2)?, c(4)?))
}

/// The chromaticity (x, y) of a standard illuminant (CIE 15: A, B, C, D50, D55, D65, D75, E, F1 to F12; 2 degree
/// observer).
pub fn illuminant(name: &str) -> Option<[f64; 3]> {
    let xy = match name.to_ascii_uppercase().as_str() {
        "A" => (0.44757, 0.40745),
        "B" => (0.34842, 0.35161),
        "C" => (0.31006, 0.31616),
        "D50" => D50,
        "D55" => (0.33242, 0.34743),
        "D65" => D65,
        "D75" => (0.29902, 0.31485),
        "E" => (1.0 / 3.0, 1.0 / 3.0),
        "F1" => (0.31310, 0.33727),
        "F2" => (0.37208, 0.37529),
        "F3" => (0.40910, 0.39430),
        "F4" => (0.44018, 0.40329),
        "F5" => (0.31379, 0.34531),
        "F6" => (0.37790, 0.38835),
        "F7" => (0.31292, 0.32933),
        "F8" => (0.34588, 0.35875),
        "F9" => (0.37417, 0.37281),
        "F10" => (0.34609, 0.35986),
        "F11" => (0.38052, 0.37713),
        "F12" => (0.43695, 0.40441),
        _ => return None,
    };
    Some(xyz_of(xy))
}

/// XYZ relative to the D50 white, adapted to the white `white` (XYZ with y = 1) by the Bradford transform (Lam 1985):
/// the colour D50 white goes to `white`.
pub fn adapt_to_white(v: [f64; 3], white: [f64; 3]) -> [f64; 3] {
    let b = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];
    let (ws, wd) = (apply3(&b, WHITE), apply3(&b, white));
    let scale = [[wd[0] / ws[0], 0.0, 0.0], [0.0, wd[1] / ws[1], 0.0], [0.0, 0.0, wd[2] / ws[2]]];
    apply3(&mul3(&inv3(&b), &mul3(&scale, &b)), v)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsb_round_trips() {
        for c in [Rgba::rgb(1.0, 0.0, 0.0), Rgba::rgb(0.2, 0.6, 0.4), Rgba::rgb(0.5, 0.5, 0.5), Rgba::rgb(0.1, 0.2, 0.9)] {
            let (h, s, v) = c.hsb();
            let back = Rgba::from_hsb(h, s, v);
            for (a, b) in [(c.r, back.r), (c.g, back.g), (c.b, back.b)] {
                assert!((a - b).abs() < 1e-12, "{c:?} -> {back:?}");
            }
        }
        assert_eq!(Rgba::from_hsb(1.0 / 3.0, 1.0, 1.0).bytes(), [0, 255, 0, 255]);
        assert_eq!(Rgba::from_hsb(-1.0 / 3.0, 1.0, 1.0).bytes(), [0, 0, 255, 255]);
    }

    #[test]
    fn hex_colors() {
        assert_eq!(hex_color("#560591").map(|c| c.bytes()), Some([86, 5, 145, 255]));
        assert_eq!(hex_color("#FFffff"), Some(Rgba::WHITE));
        for bad in ["560591", "#5605", "#5605911", "#56059g", "#\u{e9}0591", ""] {
            assert_eq!(hex_color(bad), None, "{bad}");
        }
        assert_eq!(Rgba::rgb(1.0, 0.5, 0.0).hex(), "#ff8000");
    }
}
