//! Part working drawings and the assembly drawing (A4 landscape, frame and title block per GOST 2.104
//! in simplified form). A part is its contour at a standard scale (GOST 2.302), overall dimensions, edge banding,
//! grooves, grain direction and holes with coordinates from the contour corner (0, 0) in a table.

use crate::i18n::t as tr;
use crate::contour::{self, Seg};
use crate::design::{Kind, Part};
use crate::panel::{Panel, Texture, material_name};
use crate::report::xml;

/// Standard reduction scales.
const SCALES: [f64; 10] = [1.0, 2.0, 2.5, 4.0, 5.0, 10.0, 15.0, 20.0, 25.0, 40.0];

fn n(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    let s = if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r:.1}") };
    s.replace('.', ",")
}

/// Frame, title block (185×55 in the bottom right corner) and its cells.
fn frame(title: &str, designation: &str, material: &str, qty: &str, scale: &str, sheet: &str, order: &str) -> String {
    let mut s = String::from(r##"<rect x="20" y="5" width="272" height="200" fill="none" stroke="#000" stroke-width="0.7"/>"##);
    let (x0, y0) = (107.0, 150.0);
    s += &format!(r##"<rect x="{x0}" y="{y0}" width="185" height="55" fill="none" stroke="#000" stroke-width="0.7"/>"##);
    // Horizontal and vertical lines of the title block.
    for (x1, y1, x2, y2) in [(x0, y0 + 15.0, x0 + 185.0, y0 + 15.0), (x0, y0 + 35.0, x0 + 185.0, y0 + 35.0), (x0 + 120.0, y0 + 15.0, x0 + 120.0, y0 + 55.0), (x0 + 150.0, y0 + 15.0, x0 + 150.0, y0 + 35.0), (x0 + 120.0, y0 + 25.0, x0 + 185.0, y0 + 25.0)] {
        s += &format!(r##"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="#000" stroke-width="0.5"/>"##);
    }
    let t = |x: f64, y: f64, size: f64, text: &str| format!(r##"<text x="{x}" y="{y}" font-size="{size}" font-family="Arial, sans-serif">{}</text>"##, xml(text));
    s += &t(x0 + 3.0, y0 + 10.0, 6.0, designation);
    s += &t(x0 + 3.0, y0 + 26.0, 5.0, title);
    s += &t(x0 + 3.0, y0 + 46.0, 4.0, material);
    s += &t(x0 + 122.0, y0 + 21.0, 2.5, &tr("Qty"));
    s += &t(x0 + 136.0, y0 + 22.0, 4.0, qty);
    s += &t(x0 + 152.0, y0 + 21.0, 2.5, &tr("Scale"));
    s += &t(x0 + 168.0, y0 + 22.0, 4.0, scale);
    s += &t(x0 + 122.0, y0 + 31.0, 2.5, &tr("Sheet"));
    s += &t(x0 + 136.0, y0 + 32.0, 4.0, sheet);
    s += &t(x0 + 122.0, y0 + 45.0, 3.5, order);
    s
}

/// Polyline of a contour element in part mm.
fn pts(s: &Seg) -> Vec<[f64; 2]> {
    let mut p = contour::tessellate(s, 0.05);
    p.push(contour::ends(s).1);
    p
}

/// Part working drawing: `qty` — number of identical parts, `number` — item number.
pub fn part_svg(p: &Panel, qty: usize, number: &str, order: &str, sheet: (usize, usize)) -> String {
    let (w, h) = p.size;
    // Drawing area: x 25…287, y 10…145 (above the title block; the hole table is left of the title block).
    let (ax, ay, aw, ah) = (40.0, 18.0, 160.0, 115.0);
    let scale = SCALES.into_iter().find(|k| w / k <= aw && h / k <= ah).unwrap_or(50.0);
    let k = 1.0 / scale;
    let (ox, oy) = (ax + (aw - w * k) / 2.0, ay + (ah - h * k) / 2.0 + h * k);
    let to = |q: [f64; 2]| (ox + q[0] * k, oy - q[1] * k);
    let mut s = String::from(r##"<svg xmlns="http://www.w3.org/2000/svg" width="297mm" height="210mm" viewBox="0 0 297 210">"##);
    let scale_txt = if scale == 1.0 { "1:1".to_string() } else { format!("1:{}", n(scale)) };
    s += &frame(&p.name, format!("{number} {}", p.designation).trim(), &tr(format!("{} — {} mm", material_name(&p.material), n(p.thickness))), &qty.to_string(), &scale_txt, &format!("{} / {}", sheet.0, sheet.1), order);
    for (i, sg) in p.segs.iter().enumerate() {
        let path: Vec<String> = pts(sg).into_iter().map(|q| { let (x, y) = to(q); format!("{x:.2},{y:.2}") }).collect();
        s += &format!(r##"<polyline points="{}" fill="none" stroke="#000" stroke-width="0.6"/>"##, path.join(" "));
        // Edge banding: a line along the edge inside the part and the thickness at the middle.
        if let Some(e) = p.edges.iter().find(|e| e.segment == i && e.thickness > 0.0) {
            let q = pts(sg);
            let (a, b) = (q[0], q[q.len() - 1]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = dx.hypot(dy).max(1e-9);
            // Inward normal: checked against the part center.
            let (mut nx, mut ny) = (-dy / l, dx / l);
            let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            if (w / 2.0 - mid[0]) * nx + (h / 2.0 - mid[1]) * ny < 0.0 {
                (nx, ny) = (-nx, -ny);
            }
            let off = 1.2 / k;
            let line: Vec<String> = q.iter().map(|&v| { let (x, y) = to([v[0] + nx * off, v[1] + ny * off]); format!("{x:.2},{y:.2}") }).collect();
            s += &format!(r##"<polyline points="{}" fill="none" stroke="#000" stroke-width="0.35" stroke-dasharray="3 1"/>"##, line.join(" "));
            let (tx, ty) = to([mid[0] + nx * 4.0 / k, mid[1] + ny * 4.0 / k]);
            s += &format!(r##"<text x="{tx:.2}" y="{ty:.2}" font-size="2.5" text-anchor="middle" font-family="Arial">{}</text>"##, n(e.thickness));
        }
    }
    // Grooves are dashed.
    for g in &p.grooves {
        let [a, b] = g.path;
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx.hypot(dy).max(1e-9);
        let (nx, ny) = (-dy / l, dx / l);
        for off in g.across {
            let (x1, y1) = to([a[0] + nx * off, a[1] + ny * off]);
            let (x2, y2) = to([b[0] + nx * off, b[1] + ny * off]);
            s += &format!(r##"<line x1="{x1:.2}" y1="{y1:.2}" x2="{x2:.2}" y2="{y2:.2}" stroke="#000" stroke-width="0.3" stroke-dasharray="4 1.5"/>"##);
        }
        // Width × depth next to the groove, a third of the way along it.
        let mid = (g.across[0] + g.across[1]) / 2.0;
        let (lx, ly) = to([a[0] + (b[0] - a[0]) / 3.0 + nx * mid, a[1] + (b[1] - a[1]) / 3.0 + ny * mid]);
        let label = tr(format!("groove {}×{}", n(g.across[1] - g.across[0]), n(g.z[1] - g.z[0])));
        s += &format!(r##"<text x="{:.2}" y="{:.2}" font-size="2.4" font-family="Arial">{}</text>"##, lx + 1.5, ly - 1.0, crate::report::xml(&label));
    }
    // Overall dimensions: width below, height on the left.
    let (x0, y0) = to([0.0, 0.0]);
    let (x1, y1) = to([w, h]);
    let dim = |s: &mut String, xa: f64, ya: f64, xb: f64, yb: f64, text: String, vertical: bool| {
        *s += &format!(r##"<line x1="{xa:.2}" y1="{ya:.2}" x2="{xb:.2}" y2="{yb:.2}" stroke="#000" stroke-width="0.25" marker-start="url(#a)" marker-end="url(#a)"/>"##);
        let (mx, my) = ((xa + xb) / 2.0, (ya + yb) / 2.0);
        if vertical {
            *s += &format!(r##"<text x="{:.2}" y="{my:.2}" font-size="3.5" text-anchor="middle" font-family="Arial" transform="rotate(-90 {:.2} {my:.2})">{text}</text>"##, mx - 1.5, mx - 1.5);
        } else {
            *s += &format!(r##"<text x="{mx:.2}" y="{:.2}" font-size="3.5" text-anchor="middle" font-family="Arial">{text}</text>"##, my - 1.2);
        }
    };
    s += r##"<defs><marker id="a" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="4" markerHeight="4" orient="auto-start-reverse"><path d="M0,2 L10,5 L0,8 z"/></marker></defs>"##;
    let yd = y0 + 8.0;
    s += &format!(r##"<line x1="{x0:.2}" y1="{y0:.2}" x2="{x0:.2}" y2="{:.2}" stroke="#000" stroke-width="0.2"/><line x1="{x1:.2}" y1="{y0:.2}" x2="{x1:.2}" y2="{:.2}" stroke="#000" stroke-width="0.2"/>"##, yd + 1.0, yd + 1.0);
    dim(&mut s, x0, yd, x1, yd, n(w), false);
    let xd = x0 - 8.0;
    s += &format!(r##"<line x1="{x0:.2}" y1="{y0:.2}" x2="{:.2}" y2="{y0:.2}" stroke="#000" stroke-width="0.2"/><line x1="{x0:.2}" y1="{y1:.2}" x2="{:.2}" y2="{y1:.2}" stroke="#000" stroke-width="0.2"/>"##, xd - 1.0, xd - 1.0);
    dim(&mut s, xd, y0, xd, y1, n(h), true);
    // Grain direction.
    if p.texture != Texture::Default {
        let (cx, cy) = (x1 + 6.0, (y0 + y1) / 2.0);
        let (dx, dy) = if p.texture == Texture::Horizontal { (8.0, 0.0) } else { (0.0, -8.0) };
        s += &format!(r##"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="#555" stroke-width="0.4" marker-start="url(#a)" marker-end="url(#a)"/><text x="{:.2}" y="{:.2}" font-size="2.2" fill="#555" font-family="Arial">{}</text>"##, cx - dx / 2.0, cy - dy / 2.0, cx + dx / 2.0, cy + dy / 2.0, cx + 2.0, cy + 6.0, crate::i18n::t("grain"));
    }
    // Holes: numbers on the drawing, coordinates in the table.
    let mut rows = String::new();
    for (i, hl) in p.holes.iter().enumerate() {
        let (x, y) = to([hl.pos[0], hl.pos[1]]);
        let face = hl.dir[2].abs() > 0.5;
        let side = tr(if !face { "edge" } else if hl.dir[2] < 0.0 { "face" } else { "back" });
        if face {
            let r = (hl.diameter / 2.0 * k).max(0.6);
            s += &format!(r##"<circle cx="{x:.2}" cy="{y:.2}" r="{r:.2}" fill="none" stroke="#000" stroke-width="0.3"/>"##);
        } else {
            let (ex, ey) = (x + hl.dir[0] * 3.0, y - hl.dir[1] * 3.0);
            s += &format!(r##"<line x1="{x:.2}" y1="{y:.2}" x2="{ex:.2}" y2="{ey:.2}" stroke="#000" stroke-width="0.4"/>"##);
        }
        s += &format!(r##"<text x="{:.2}" y="{:.2}" font-size="2" fill="#a00" font-family="Arial">{}</text>"##, x + 1.0, y - 1.0, i + 1);
        if i < 30 {
            let ty = 22.0 + i as f64 * 4.0;
            rows += &format!(r##"<text x="210" y="{ty}" font-size="2.6" font-family="Arial">{}</text><text x="220" y="{ty}" font-size="2.6" font-family="Arial">{}</text><text x="236" y="{ty}" font-size="2.6" font-family="Arial">{}</text><text x="252" y="{ty}" font-size="2.6" font-family="Arial">Ø{}×{}</text><text x="272" y="{ty}" font-size="2.6" font-family="Arial">{side}</text>"##, i + 1, n(hl.pos[0]), n(hl.pos[1]), n(hl.diameter), n(hl.depth));
        }
    }
    if !p.holes.is_empty() {
        for (x, h) in [(210, "№"), (220, "X"), (236, "Y"), (252, "Ø × depth"), (272, "side")] {
            s += &format!(r##"<text x="{x}" y="15" font-size="2.8" font-weight="bold" font-family="Arial">{}</text>"##, tr(h));
        }
        s += &rows;
        if p.holes.len() > 30 {
            s += &format!(r##"<text x="210" y="145" font-size="2.6" font-family="Arial">{}</text>"##, crate::i18n::t(format!("… {} more holes", p.holes.len() - 30)));
        }
    }
    s += "</svg>";
    s
}

/// Assembly drawing: front view with item numbers and an item table.
pub fn assembly_svg(parts: &[Part], order: &str) -> String {
    let rects = crate::view::front_view(parts, true);
    let mut s = String::from(r##"<svg xmlns="http://www.w3.org/2000/svg" width="297mm" height="210mm" viewBox="0 0 297 210">"##);
    let (ax, ay, aw, ah) = (30.0, 12.0, 160.0, 130.0);
    let bounds = crate::view::bounds(&rects);
    let scale = bounds.map_or(1.0, |(lo, hi)| SCALES.into_iter().chain([50.0, 75.0, 100.0]).find(|k| (hi[0] - lo[0]) / k <= aw && (hi[1] - lo[1]) / k <= ah).unwrap_or(200.0));
    s += &frame(&tr("Assembly drawing"), order, "", &parts.len().to_string(), &format!("1:{}", n(scale)), "1 / 1", order);
    if let Some((lo, _)) = bounds {
        let k = 1.0 / scale;
        let to = |x: f64, y: f64| (ax + (x - lo[0]) * k, ay + ah - (y - lo[1]) * k);
        for r in &rects {
            let (x0, y1) = to(r.x, r.y);
            let (x1, y0) = to(r.x + r.w, r.y + r.h);
            let fill = match r.kind {
                Kind::Door | Kind::Drawer => "none",
                _ => "#fff",
            };
            s += &format!(r##"<rect x="{x0:.2}" y="{y0:.2}" width="{:.2}" height="{:.2}" fill="{fill}" stroke="#000" stroke-width="0.3"/>"##, x1 - x0, y1 - y0);
        }
        // Items: a leader to the center of each part visible from the front (unique numbers).
        let mut seen = Vec::new();
        for r in rects.iter().rev() {
            let pos = &parts[r.part].panel.pos;
            if seen.contains(pos) || pos.is_empty() {
                continue;
            }
            seen.push(pos.clone());
            let (cx, cy) = to(r.x + r.w / 2.0, r.y + r.h / 2.0);
            s += &format!(r##"<circle cx="{cx:.2}" cy="{cy:.2}" r="2.6" fill="#fff" stroke="#000" stroke-width="0.25"/><text x="{cx:.2}" y="{:.2}" font-size="2.6" text-anchor="middle" font-family="Arial">{}</text>"##, cy + 0.9, xml(pos));
        }
        s += &format!(r##"<text x="30" y="148" font-size="3" font-family="Arial">{}</text>"##, crate::i18n::t(format!("Front view, scale 1:{}", n(scale))));
    }
    // Item table on the right.
    let rows = crate::panel::spec(&parts.iter().map(|p| p.panel.clone()).collect::<Vec<_>>());
    for (x, h) in [(200, "Pos."), (210, "Item name"), (280, "Qty")] {
        s += &format!(r##"<text x="{x}" y="15" font-size="2.8" font-weight="bold" font-family="Arial">{}</text>"##, tr(h));
    }
    for (i, r) in rows.iter().take(32).enumerate() {
        let y = 20.0 + i as f64 * 4.0;
        let name: String = r.panel.name.chars().take(22).collect();
        s += &format!(r##"<text x="200" y="{y}" font-size="2.6" font-family="Arial">{}</text><text x="210" y="{y}" font-size="2.6" font-family="Arial">{}</text><text x="282" y="{y}" font-size="2.6" font-family="Arial">{}</text>"##, xml(&r.panel.pos), xml(&name), r.count);
    }
    s += "</svg>";
    s
}

/// Drawing set: the assembly drawing and working drawings — one per item (identical parts merged).
pub fn drawings(parts: &[Part], order: &str) -> Vec<String> {
    let panels: Vec<Panel> = parts.iter().map(|p| p.panel.clone()).collect();
    let rows = crate::panel::spec(&panels);
    let mut out = vec![assembly_svg(parts, order)];
    let total = rows.len();
    for (i, r) in rows.iter().enumerate() {
        out.push(part_svg(&r.panel, r.count, &r.panel.pos, order, (i + 1, total)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::{Project, Session, build};
    use serde_json::json;

    #[test]
    fn part_drawing_has_sizes_holes_and_title() {
        let mut s = Session::new(Project::new("Order 7"));
        s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
        s.execute("edge.add", &json!({"name": "PVC 2", "thickness": 2})).unwrap();
        s.execute("cabinet.add", &json!({"width": 800, "height": 2000, "depth": 600, "material": "Chipboard 16", "edge": "PVC 2", "fastener": "Confirmat 7x50"})).unwrap();
        let parts = build(&s.project).unwrap();
        let side = parts.iter().find(|p| p.panel.name == "Left side").unwrap();
        let svg = part_svg(&side.panel, 2, &side.panel.pos, "Order 7", (1, 5));
        // Scale 1:20 (2000 mm in a 115 mm area), dimensions 600 and 2000, edge banding 2, holes in the table.
        assert!(svg.contains(">1:20<") && svg.contains(">600<") && svg.contains(">2000<"), "{svg}");
        assert!(svg.contains(">2<"), "edge banding thickness");
        assert!(svg.contains("face") && svg.contains("Ø8×16"), "through Ø8 confirmat holes in the table");
        assert!(svg.contains("Left side") && svg.contains("Chipboard 16"));
        let all = drawings(&parts, "Order 7");
        assert_eq!(all.len(), 1 + crate::panel::spec(&parts.iter().map(|p| p.panel.clone()).collect::<Vec<_>>()).len());
        assert!(all[0].contains("Assembly drawing"));
    }
}
