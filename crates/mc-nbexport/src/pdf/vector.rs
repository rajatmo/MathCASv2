//! SVG drawings as PDF vector graphics (round nb-svg): a plot's SVG is parsed with `usvg` (which resolves `<use>`,
//! styles and transforms into a tree of paths, groups and images) and written as the content stream of a form
//! `XObject`, so plots in an exported PDF stay sharp at any zoom and print size. Filled and stroked paths with
//! flat colours, linear and radial gradients (PDF shadings padded at their ends; a varying stop opacity becomes a
//! soft mask) and opacity, clipping, luminance and alpha masks (soft masks), transforms and embedded raster images
//! (PNG) are drawn; patterns and filters are left out, and repeated or reflected gradients are drawn padded. The
//! drawing keeps its own colours.
use super::writer::{num, Picture};
use usvg::tiny_skia_path::{PathSegment, Transform};
use std::fmt::Write as _;

/// A drawing ready for the writer: its content (in its own units, y down from the top-left corner: the writer's
/// form matrix flips it), its size, the opacities its graphics states need and the raster images it draws (named
/// `/Im<k>` in the content, `k` counted from the first image of this drawing: the writer renumbers them).
pub(crate) struct Drawing {
    /// width and height in the drawing's units (SVG user units = notebook points)
    pub(crate) size: (f32, f32),
    /// the content stream, with `/GS<alpha in thousandths>` graphics states and `/Img<k>` images
    pub(crate) content: String,
    /// the opacities used, in thousandths
    pub(crate) alphas: Vec<u16>,
    /// the raster images drawn, in order of their `k`
    pub(crate) images: Vec<Picture>,
    /// shading dictionaries, drawn as `/Shd<k> sh`
    pub(crate) shadings: Vec<String>,
    /// soft masks set as `/Msk<k> gs`: the mask's content (drawing coordinates) and whether it is a luminance mask
    /// (an alpha mask otherwise)
    pub(crate) masks: Vec<(String, bool)>,
}

/// Parse an SVG file's bytes into a drawing (`None`: not a readable SVG).
pub(crate) fn drawing(svg: &[u8], decode: &dyn Fn(&[u8]) -> Option<Picture>) -> Option<Drawing> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg, &opt).ok()?;
    let size = (tree.size().width(), tree.size().height());
    let mut d = Drawing { size, content: String::new(), alphas: vec![], images: vec![], shadings: vec![], masks: vec![] };
    group(tree.root(), Transform::identity(), 1.0, &mut d, decode);
    Some(d)
}

/// The operators of a group's children drawn with `ts` (the group's transform in the drawing) at `alpha`.
fn group(g: &usvg::Group, ts: Transform, alpha: f32, d: &mut Drawing, decode: &dyn Fn(&[u8]) -> Option<Picture>) {
    for child in g.children() {
        match child {
            usvg::Node::Group(sub) => {
                // filters are left out: the group is drawn without them
                let t = ts.pre_concat(sub.transform());
                let a = alpha * sub.opacity().get();
                d.content.push_str("q\n");
                if let Some(m) = sub.mask() {
                    let k = mask(m, t, d, decode);
                    let _ = writeln!(d.content, "/Msk{k} gs");
                }
                if let Some(clip) = sub.clip_path() {
                    let mut ops = String::new();
                    clip_ops(clip.root(), t.pre_concat(clip.transform()), &mut ops);
                    if ops.is_empty() {
                        // an empty clip hides everything
                        d.content.push_str("0 0 m h W n\n");
                    } else {
                        d.content.push_str(&ops);
                        d.content.push_str("W n\n");
                    }
                }
                group(sub, t, a, d, decode);
                d.content.push_str("Q\n");
            }
            usvg::Node::Path(p) => path(p, ts, alpha, d),
            usvg::Node::Image(im) => image(im, ts, d, decode),
            usvg::Node::Text(t) => group(t.flattened(), ts, alpha, d, decode),
        }
    }
}

/// The path operators of `p` with its points mapped by `ts` (no painting operator).
fn segments(p: &usvg::tiny_skia_path::Path, ts: Transform, out: &mut String) {
    let map = |x: f32, y: f32| {
        let mut pt = [usvg::tiny_skia_path::Point::from_xy(x, y)];
        ts.map_points(&mut pt);
        (pt[0].x, pt[0].y)
    };
    let mut cur = (0.0f32, 0.0f32);
    for s in p.segments() {
        let _ = match s {
            PathSegment::MoveTo(a) => {
                cur = (a.x, a.y);
                let (x, y) = map(a.x, a.y);
                writeln!(out, "{} {} m", num(x), num(y))
            }
            PathSegment::LineTo(a) => {
                cur = (a.x, a.y);
                let (x, y) = map(a.x, a.y);
                writeln!(out, "{} {} l", num(x), num(y))
            }
            PathSegment::QuadTo(c, a) => {
                let c1 = map(cur.0 + 2.0 / 3.0 * (c.x - cur.0), cur.1 + 2.0 / 3.0 * (c.y - cur.1));
                let c2 = map(a.x + 2.0 / 3.0 * (c.x - a.x), a.y + 2.0 / 3.0 * (c.y - a.y));
                cur = (a.x, a.y);
                let (x, y) = map(a.x, a.y);
                writeln!(out, "{} {} {} {} {} {} c", num(c1.0), num(c1.1), num(c2.0), num(c2.1), num(x), num(y))
            }
            PathSegment::CubicTo(c1, c2, a) => {
                cur = (a.x, a.y);
                let ((p, q), (r, s), (x, y)) = (map(c1.x, c1.y), map(c2.x, c2.y), map(a.x, a.y));
                writeln!(out, "{} {} {} {} {} {} c", num(p), num(q), num(r), num(s), num(x), num(y))
            }
            PathSegment::Close => writeln!(out, "h"),
        };
    }
}

/// The union of a clip path's shapes as path operators in drawing coordinates.
fn clip_ops(g: &usvg::Group, ts: Transform, out: &mut String) {
    for child in g.children() {
        match child {
            usvg::Node::Path(p) if p.is_visible() => segments(p.data(), ts, out),
            usvg::Node::Group(sub) => clip_ops(sub, ts.pre_concat(sub.transform()), out),
            usvg::Node::Text(t) => clip_ops(t.flattened(), ts, out),
            _ => {}
        }
    }
}

/// The colour operands of a flat paint (`None`: gradients, drawn as shadings, and patterns, not drawn).
fn colour(paint: &usvg::Paint) -> Option<String> {
    match paint {
        usvg::Paint::Color(c) => {
            let f = |v: u8| num(f32::from(v) / 255.0);
            Some(format!("{} {} {}", f(c.red), f(c.green), f(c.blue)))
        }
        _ => None,
    }
}

/// The operator selecting the graphics state of opacity `a` (empty for opaque), noted in `alphas`.
fn opacity(a: f32, alphas: &mut Vec<u16>) -> String {
    let k = (a.clamp(0.0, 1.0) * 1000.0).round() as u16;
    if k >= 1000 {
        return String::new();
    }
    if !alphas.contains(&k) {
        alphas.push(k);
    }
    format!("/GS{k} gs\n")
}

/// A mask drawn as a soft mask of the drawing (its content clipped to the mask's rectangle); gives its `k`.
fn mask(m: &usvg::Mask, ts: Transform, d: &mut Drawing, decode: &dyn Fn(&[u8]) -> Option<Picture>) -> usize {
    let outer = std::mem::take(&mut d.content);
    let rect = usvg::tiny_skia_path::PathBuilder::from_rect(m.rect().to_rect());
    segments(&rect, ts, &mut d.content);
    d.content.push_str("W n\n");
    if let Some(inner) = m.mask() {
        let k = mask(inner, ts, d, decode);
        let _ = writeln!(d.content, "/Msk{k} gs");
    }
    group(m.root(), ts, 1.0, d, decode);
    let content = std::mem::replace(&mut d.content, outer);
    d.masks.push((content, m.kind() == usvg::MaskType::Luminance));
    d.masks.len() - 1
}

/// A PDF function of `t` in [0, 1] interpolating `stops` (offset, components) linearly, flat beyond the ends.
fn stops_function(stops: &[(f32, Vec<f32>)]) -> String {
    let comps = |v: &[f32]| v.iter().map(|x| num(*x)).collect::<Vec<_>>().join(" ");
    let mut s: Vec<(f32, Vec<f32>)> = Vec::with_capacity(stops.len() + 2);
    let mut last = 0.0f32;
    for (o, c) in stops {
        last = o.clamp(last, 1.0);
        s.push((last, c.clone()));
    }
    if s.is_empty() {
        s.push((0.0, vec![0.0]));
    }
    if s[0].0 > 0.0 {
        let c = s[0].1.clone();
        s.insert(0, (0.0, c));
    }
    if s[s.len() - 1].0 < 1.0 || s.len() == 1 {
        let c = s[s.len() - 1].1.clone();
        s.push((1.0, c));
    }
    let piece = |a: &[f32], b: &[f32]| format!("<< /FunctionType 2 /Domain [0 1] /C0 [{}] /C1 [{}] /N 1 >>", comps(a), comps(b));
    if s.len() == 2 {
        return piece(&s[0].1, &s[1].1);
    }
    let pieces: Vec<String> = s.windows(2).map(|w| piece(&w[0].1, &w[1].1)).collect();
    let bounds: Vec<String> = s[1..s.len() - 1].iter().map(|x| num(x.0)).collect();
    let encode = vec!["0 1"; pieces.len()].join(" ");
    format!("<< /FunctionType 3 /Domain [0 1] /Functions [{}] /Bounds [{}] /Encode [{encode}] >>", pieces.join(" "), bounds.join(" "))
}

/// The shading dictionary of a gradient in colour space `space` with `stops` (offset, components).
fn shading(paint: &usvg::Paint, space: &str, stops: &[(f32, Vec<f32>)]) -> Option<String> {
    let f = stops_function(stops);
    match paint {
        usvg::Paint::LinearGradient(g) => Some(format!(
            "<< /ShadingType 2 /ColorSpace /{space} /Coords [{} {} {} {}] /Function {f} /Extend [true true] >>",
            num(g.x1()),
            num(g.y1()),
            num(g.x2()),
            num(g.y2())
        )),
        usvg::Paint::RadialGradient(g) => Some(format!(
            "<< /ShadingType 3 /ColorSpace /{space} /Coords [{} {} {} {} {} {}] /Function {f} /Extend [true true] >>",
            num(g.fx()),
            num(g.fy()),
            num(g.fr().get()),
            num(g.cx()),
            num(g.cy()),
            num(g.r().get())
        )),
        _ => None,
    }
}

/// The operators painting the area of `shape` (path operators in drawing coordinates) with a gradient given in
/// the path's coordinates `ts`, at opacity `a` (`None`: not a gradient).
fn gradient_area(paint: &usvg::Paint, a: f32, shape: &str, even_odd: bool, ts: Transform, d: &mut Drawing) -> Option<String> {
    let base: &usvg::BaseGradient = match paint {
        usvg::Paint::LinearGradient(g) => g,
        usvg::Paint::RadialGradient(g) => g,
        _ => return None,
    };
    let stops = base.stops();
    let rgb: Vec<(f32, Vec<f32>)> = stops
        .iter()
        .map(|s| {
            let c = s.color();
            (s.offset().get(), vec![f32::from(c.red) / 255.0, f32::from(c.green) / 255.0, f32::from(c.blue) / 255.0])
        })
        .collect();
    let alphas: Vec<(f32, Vec<f32>)> = stops.iter().map(|s| (s.offset().get(), vec![s.opacity().get()])).collect();
    let m = ts.pre_concat(base.transform());
    let cm = format!("{} {} {} {} {} {} cm", num(m.sx), num(m.ky), num(m.kx), num(m.sy), num(m.tx), num(m.ty));
    let clip = if even_odd { "W* n" } else { "W n" };
    let k = d.shadings.len();
    d.shadings.push(shading(paint, "DeviceRGB", &rgb)?);
    let first = alphas.first().map_or(1.0, |x| x.1[0]);
    let uniform = alphas.iter().all(|x| (x.1[0] - first).abs() < 1e-3);
    let mut o = opacity(a * if uniform { first } else { 1.0 }, &mut d.alphas);
    if !uniform {
        // the stops' opacity as the luminance of a gray shading over the same area
        let j = d.shadings.len();
        d.shadings.push(shading(paint, "DeviceGray", &alphas)?);
        d.masks.push((format!("{shape}{clip}\nq {cm} /Shd{j} sh Q\n"), true));
        let _ = writeln!(o, "/Msk{} gs", d.masks.len() - 1);
    }
    let _ = writeln!(o, "{shape}{clip}\n{cm} /Shd{k} sh");
    Some(o)
}

/// A filled and/or stroked path.
fn path(p: &usvg::Path, ts: Transform, alpha: f32, d: &mut Drawing) {
    if !p.is_visible() {
        return;
    }
    let mut shape = String::new();
    segments(p.data(), ts, &mut shape);
    if shape.is_empty() {
        return;
    }
    // the stroke width scales with the transform (uniform in what mc-graphics writes)
    let scale = ((ts.sx * ts.sy - ts.kx * ts.ky).abs()).sqrt();
    let fill_ops = p.fill().and_then(|f| {
        let (a, even_odd) = (alpha * f.opacity().get(), f.rule() == usvg::FillRule::EvenOdd);
        let Some(c) = colour(f.paint()) else {
            return gradient_area(f.paint(), a, &shape, even_odd, ts, d);
        };
        let mut o = opacity(a, &mut d.alphas);
        let _ = writeln!(o, "{c} rg");
        o.push_str(&shape);
        o.push_str(if even_odd { "f*\n" } else { "f\n" });
        Some(o)
    });
    let stroke_ops = p.stroke().and_then(|s| {
        let Some(c) = colour(s.paint()) else {
            // a gradient stroke: its outline (dashed first) filled with the gradient
            let mut st = s.to_tiny_skia();
            let src = match st.dash.take() {
                Some(dash) => p.data().dash(&dash, 1.0)?,
                None => p.data().clone(),
            };
            let outline = src.stroke(&st, scale.max(1.0) * 4.0)?;
            let mut area = String::new();
            segments(&outline, ts, &mut area);
            return gradient_area(s.paint(), alpha * s.opacity().get(), &area, false, ts, d);
        };
        let mut o = opacity(alpha * s.opacity().get(), &mut d.alphas);
        let cap = match s.linecap() {
            usvg::LineCap::Butt => 0,
            usvg::LineCap::Round => 1,
            usvg::LineCap::Square => 2,
        };
        let join = match s.linejoin() {
            usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => 0,
            usvg::LineJoin::Round => 1,
            usvg::LineJoin::Bevel => 2,
        };
        let _ = writeln!(o, "{c} RG {} w {cap} J {join} j {} M", num(s.width().get() * scale), num(s.miterlimit().get()));
        if let Some(dash) = s.dasharray() {
            let v: Vec<String> = dash.iter().map(|x| num(x * scale)).collect();
            let _ = writeln!(o, "[{}] {} d", v.join(" "), num(s.dashoffset() * scale));
        }
        o.push_str(&shape);
        o.push_str("S\n");
        Some(o)
    });
    let order = p.paint_order();
    let (first, second) = if order == usvg::PaintOrder::StrokeAndFill { (stroke_ops, fill_ops) } else { (fill_ops, stroke_ops) };
    for ops in [first, second].into_iter().flatten() {
        d.content.push_str("q\n");
        d.content.push_str(&ops);
        d.content.push_str("Q\n");
    }
}

/// A raster image: PNG data drawn over its size in its transform.
fn image(im: &usvg::Image, ts: Transform, d: &mut Drawing, decode: &dyn Fn(&[u8]) -> Option<Picture>) {
    if !im.is_visible() {
        return;
    }
    let usvg::ImageKind::PNG(bytes) = im.kind() else { return };
    let Some(mut pic) = decode(bytes) else { return };
    pic.smooth = im.rendering_mode() != usvg::ImageRendering::OptimizeSpeed;
    let (w, h) = (im.size().width(), im.size().height());
    // the unit square of the image, its first row at the top: (0, 1) -> (0, 0) and (1, 0) -> (w, h) in y-down units
    let m = ts.pre_concat(Transform::from_row(w, 0.0, 0.0, -h, 0.0, h));
    let k = d.images.len();
    d.images.push(pic);
    let _ = writeln!(
        d.content,
        "q {} {} {} {} {} {} cm /Img{k} Do Q",
        num(m.sx),
        num(m.ky),
        num(m.kx),
        num(m.sy),
        num(m.tx),
        num(m.ty)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_images(_: &[u8]) -> Option<Picture> {
        None
    }

    #[test]
    fn paths_clips_and_opacity_become_operators() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100" height="50" viewBox="0 0 100 50">
<defs><path id="g1" d="M0 0L10 0L10 10Z"/></defs>
<path class="mcf-bg" d="M0 0L100 0L100 50L0 50Z" fill="#ffffff"/>
<clipPath id="c1"><rect x="10" y="5" width="80" height="40"/></clipPath><g clip-path="url(#c1)">
<path d="M0 40L100 10" fill="none" stroke="#ff0000" stroke-opacity="0.5" stroke-width="2" stroke-linecap="round" stroke-dasharray="4 2"/>
</g>
<g fill="#262626"><use xlink:href="#g1" transform="translate(20 30) scale(0.5)"/></g>
</svg>"##;
        let d = drawing(svg, &no_images).unwrap();
        assert_eq!(d.size, (100.0, 50.0));
        let c = &d.content;
        // the background, the clip rectangle, the dashed half-transparent red line, the glyph moved and scaled
        assert!(c.contains("1 1 1 rg\n0 0 m\n100 0 l\n100 50 l\n0 50 l\nh\nf\n"), "{c}");
        assert!(c.contains("10 5 m\n90 5 l\n90 45 l\n10 45 l\nh\nW n\n"), "{c}");
        assert!(c.contains("/GS500 gs\n1 0 0 RG 2 w 1 J 0 j 4 M\n[4 2] 0 d\n0 40 m\n100 10 l\nS\n"), "{c}");
        assert!(c.contains("0.15 0.15 0.15 rg\n20 30 m\n25 30 l\n25 35 l\nh\nf\n"), "{c}");
        assert_eq!(d.alphas, vec![500]);
        assert_eq!(c.matches("q\n").count(), c.matches("Q\n").count());
    }

    #[test]
    fn gradients_become_shadings_and_masks_soft_masks() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 100 50">
<defs>
<linearGradient id="l" x1="0" y1="0" x2="100" y2="0" gradientUnits="userSpaceOnUse">
<stop offset="0.2" stop-color="#ff0000"/><stop offset="0.5" stop-color="#00ff00"/><stop offset="1" stop-color="#0000ff"/>
</linearGradient>
<radialGradient id="r" cx="50" cy="25" r="20" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="#000000" stop-opacity="1"/><stop offset="1" stop-color="#000000" stop-opacity="0"/>
</radialGradient>
<mask id="m"><rect x="0" y="0" width="50" height="50" fill="#ffffff"/></mask>
</defs>
<rect x="0" y="0" width="100" height="20" fill="url(#l)"/>
<circle cx="50" cy="25" r="20" fill="url(#r)"/>
<g mask="url(#m)"><rect x="0" y="30" width="100" height="20" fill="#0000ff"/></g>
<path d="M0 45L100 45" stroke="url(#l)" stroke-width="4" fill="none"/>
</svg>"##;
        let d = drawing(svg, &no_images).unwrap();
        let c = &d.content;
        // the linear fill: clipped to the rectangle, three stops padded to [0, 1] in a stitching function
        assert!(c.contains("W n\n1 0 0 1 0 0 cm /Shd0 sh"), "{c}");
        assert!(d.shadings[0].starts_with("<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0]"), "{}", d.shadings[0]);
        assert!(d.shadings[0].contains("/Bounds [0.2 0.5]"), "{}", d.shadings[0]);
        // the radial fill fading out: a colour shading under a soft mask of a gray one
        assert!(d.shadings.iter().any(|s| s.contains("/ShadingType 3 /ColorSpace /DeviceGray")), "{:?}", d.shadings);
        assert!(d.masks.iter().any(|(m, lum)| *lum && m.contains("sh")), "{:?}", d.masks);
        // the mask group: set before its content, the content of the mask a white rectangle clipped to its area
        assert!(c.contains("/Msk") && c.contains("0 0 1 rg"), "{c}");
        // the gradient stroke: its outline filled with the shading
        assert_eq!(c.matches(" sh\n").count(), 3, "{c}");
        assert_eq!(c.matches("q\n").count(), c.matches("Q\n").count());
    }
}
