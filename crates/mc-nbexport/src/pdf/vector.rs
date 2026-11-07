//! SVG drawings as PDF vector graphics (round nb-svg): a plot's SVG is parsed with `usvg` (which resolves `<use>`,
//! styles and transforms into a tree of paths, groups and images) and written as the content stream of a form
//! `XObject`, so plots in an exported PDF stay sharp at any zoom and print size. Filled and stroked paths with
//! flat colours and opacity, clipping, transforms and embedded raster images (PNG) are drawn; gradients, patterns,
//! masks and filters (which `mc-graphics` never writes) are left out. The drawing keeps its own (light) colours.
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
}

/// Parse an SVG file's bytes into a drawing (`None`: not a readable SVG).
pub(crate) fn drawing(svg: &[u8], decode: &dyn Fn(&[u8]) -> Option<Picture>) -> Option<Drawing> {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg, &opt).ok()?;
    let size = (tree.size().width(), tree.size().height());
    let mut d = Drawing { size, content: String::new(), alphas: vec![], images: vec![] };
    group(tree.root(), Transform::identity(), 1.0, &mut d, decode);
    Some(d)
}

/// The operators of a group's children drawn with `ts` (the group's transform in the drawing) at `alpha`.
fn group(g: &usvg::Group, ts: Transform, alpha: f32, d: &mut Drawing, decode: &dyn Fn(&[u8]) -> Option<Picture>) {
    for child in g.children() {
        match child {
            usvg::Node::Group(sub) => {
                // filters and masks (never written by mc-graphics) are left out: the group is drawn without them
                let t = ts.pre_concat(sub.transform());
                let a = alpha * sub.opacity().get();
                d.content.push_str("q\n");
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

/// The colour operands of a flat paint (`None`: gradients and patterns, not drawn).
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
    let fill = p.fill().and_then(|f| Some((colour(f.paint())?, f.opacity().get(), f.rule())));
    let stroke = p.stroke().and_then(|s| Some((colour(s.paint())?, s)));
    let fill_ops = fill.map(|(c, a, rule)| {
        let mut o = opacity(alpha * a, &mut d.alphas);
        let _ = writeln!(o, "{c} rg");
        o.push_str(&shape);
        o.push_str(if rule == usvg::FillRule::EvenOdd { "f*\n" } else { "f\n" });
        o
    });
    let stroke_ops = stroke.map(|(c, s)| {
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
        o
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
}
