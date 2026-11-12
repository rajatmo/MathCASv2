//! Colour gradients: a colour for every value in [0, 1], through evenly spaced stops. The named gradients colour
//! density and contour plots, the `raster` primitive and colour bars; a list of colours is a gradient too.
use crate::color::{colour, hex_color, Colour, Rgba};
use mc_api::names;
use mc_api::{Store, View, T};

/// A gradient: colours at evenly spaced stops from 0 to 1, mixed linearly between them.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    /// at least one colour; the first is at 0, the last at 1
    stops: Vec<Rgba>,
}

/// The named gradients, each as its stops (`#rrggbb`), with what it is good for.
pub const NAMED: &[(&str, &[u32], &str)] = &[
    (
        "viridis",
        &[0x440154, 0x482878, 0x3E4A89, 0x31688E, 0x26828E, 0x1F9E89, 0x35B779, 0x6DCD59, 0xB4DE2C, 0xFDE725],
        "dark violet through blue and green to yellow; even steps of lightness, readable in gray and by colour-blind readers (the default)",
    ),
    ("thermal", &[0x0B0405, 0x4A0C1C, 0x9C1F22, 0xD9481C, 0xF28C28, 0xF9C74F, 0xFDF4C8], "black through deep red and orange to pale yellow, like heat"),
    ("cool_warm", &[0x3B4CC0, 0x7A9EF2, 0xC0D4F5, 0xEBE5E0, 0xF4B79B, 0xDE6A53, 0xB40426], "blue through light gray to red: for values on both sides of a middle"),
    ("ocean", &[0x081D58, 0x23468F, 0x2879B9, 0x3FA8C4, 0x86CFBA, 0xD3EEB5, 0xFFFFD9], "deep blue through teal to pale yellow"),
    ("earth", &[0x2D1E12, 0x5E3C1F, 0x8C6A2F, 0x9C9A4E, 0x7FB08A, 0xC9DCC4, 0xF7F5EE], "dark brown through ochre and sage green to off-white"),
    ("rainbow", &[0x6E40AA, 0x3F5FD8, 0x1AA3CF, 0x2BCB84, 0xA6D63B, 0xF2B635, 0xE8572C, 0xB8232D], "violet, blue, green, yellow, orange, red"),
    ("gray", &[0x000000, 0xFFFFFF], "black to white"),
];

impl Gradient {
    /// A gradient through `stops` (an empty list is the gray one).
    pub fn new(stops: Vec<Rgba>) -> Gradient {
        if stops.is_empty() {
            return Gradient::named("gray").unwrap_or(Gradient { stops: vec![Rgba::BLACK, Rgba::WHITE] });
        }
        Gradient { stops }
    }

    /// The gradient called `name` (one of [`NAMED`]).
    pub fn named(name: &str) -> Option<Gradient> {
        let (_, stops, _) = NAMED.iter().find(|(n, _, _)| *n == name)?;
        let c = |v: u32| Rgba::rgb(((v >> 16) & 255) as f64 / 255.0, ((v >> 8) & 255) as f64 / 255.0, (v & 255) as f64 / 255.0);
        Some(Gradient { stops: stops.iter().map(|&v| c(v)).collect() })
    }

    /// The default gradient of density and contour plots.
    pub fn default_gradient() -> Gradient {
        Gradient::named("viridis").unwrap_or_else(|| Gradient::new(vec![]))
    }

    /// The colour at `t` (clamped to [0, 1]; NaN is 0).
    pub fn at(&self, t: f64) -> Rgba {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let n = self.stops.len();
        if n == 1 {
            return self.stops[0];
        }
        let u = t * (n - 1) as f64;
        let k = (u.floor() as usize).min(n - 2);
        self.stops[k].mix(self.stops[k + 1], u - k as f64)
    }

    /// The stops.
    pub fn stops(&self) -> &[Rgba] {
        &self.stops
    }
}

/// The gradient for a colour scheme named in words (`"rainbow"`, `"temperature"`, `"sunset"`, `"ocean blue"`, ...):
/// the nearest of ours by what the words describe; [`Gradient::default_gradient`] when nothing
/// is near.
pub fn scheme(name: &str) -> Gradient {
    let n = name.to_ascii_lowercase().replace([' ', '_', '-'], "");
    let pick = if let Some(g) = NAMED.iter().find(|(k, _, _)| k.replace('_', "") == n) {
        g.0
    } else if n.contains("rainbow") || n.contains("spectrum") || n.contains("visible") {
        "rainbow"
    } else if n.contains("temperature") || n.contains("thermometer") || n.contains("redblue") || n.contains("bluered") || n.contains("coolwarm") {
        "cool_warm"
    } else if n.contains("sunset") || n.contains("heat") || n.contains("fire") || n.contains("solar") || n.contains("thermal") || n.contains("hot") {
        "thermal"
    } else if n.contains("aqua") || n.contains("ocean") || n.contains("blue") || n.contains("deepsea") || n.contains("sea") {
        "ocean"
    } else if n.contains("earth") || n.contains("terrain") || n.contains("avocado") || n.contains("brown") || n.contains("fruit") || n.contains("land") {
        "earth"
    } else if n.contains("gray") || n.contains("grey") || n.contains("monochrome") {
        "gray"
    } else {
        "viridis"
    };
    Gradient::named(pick).unwrap_or_else(Gradient::default_gradient)
}

/// A gradient written as a term: a gradient's name (a symbol or a string) or a list of colours (colour terms or
/// `"#rrggbb"` strings).
pub fn read<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<Gradient> {
    match h.view(t) {
        View::Sym(s) => Gradient::named(&h.sym_name(s)),
        View::Str(s) => Gradient::named(s),
        View::App { head, args } => {
            if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::LIST) || args.is_empty() {
                return None;
            }
            let stops = args
                .iter()
                .map(|&c| match (colour(h, c), h.view(c)) {
                    (Colour::Value(c), _) => Some(c),
                    (_, View::Str(s)) => hex_color(s),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Gradient::new(stops))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradients_run_through_their_stops() {
        let g = Gradient::named("gray").unwrap();
        assert_eq!(g.at(0.0), Rgba::BLACK);
        assert_eq!(g.at(1.0), Rgba::WHITE);
        assert!((g.at(0.25).r - 0.25).abs() < 1e-12);
        assert_eq!(g.at(-3.0), Rgba::BLACK);
        assert_eq!(g.at(f64::NAN), Rgba::BLACK);
        assert_eq!(g.at(7.0), Rgba::WHITE);
        let v = Gradient::named("viridis").unwrap();
        assert_eq!(v.at(0.0).hex(), "#440154");
        assert_eq!(v.at(1.0).hex(), "#fde725");
        for (name, stops, _) in NAMED {
            let g = Gradient::named(name).unwrap();
            assert_eq!(g.stops().len(), stops.len());
            // the stops are hit exactly
            for (k, &s) in stops.iter().enumerate() {
                let t = k as f64 / (stops.len() - 1) as f64;
                assert_eq!(g.at(t).hex(), format!("#{s:06x}"), "{name} {k}");
            }
        }
        assert!(Gradient::named("nope").is_none());
        let one = Gradient::new(vec![Rgba::WHITE]);
        assert_eq!(one.at(0.3), Rgba::WHITE);
    }
}
