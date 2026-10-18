//! Parts list (CSV), cutting maps and “Panels for production” in BAZIS-Raskroy style
//! (reference — `reference/raskroy-karty-laura.pdf`).
//! shortcut: no QR code and no grouping of identical sheets into one map.

use std::fmt::Write;

use crate::i18n::t;
use crate::nest::{Params, Sheet};
use crate::panel::{MaterialRow, Side, SpecRow, material_name};

fn num(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r:.1}").replace('.', ",") }
}

fn csv_field(s: &str) -> String {
    if s.contains([';', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// CSV for Excel: UTF-8 with BOM, ";" separator, decimal comma.
/// L1/L2 — edge banding on long sides, W1/W2 — on short sides (thickness, mm).
pub fn spec_csv(rows: &[SpecRow]) -> String {
    let head = ["Material", "Pos.", "Designation", "Item name", "Length", "Width", "Thickness", "Quantity", "Grain", "L1", "L2", "W1", "W2", "Edge banding", "Shaped"];
    let mut s = format!("\u{feff}{}\r\n", head.map(t).join(";"));
    for r in rows {
        let p = &r.panel;
        let (l, w) = p.cut_size();
        let side = |sd: Side| {
            p.edges.iter().filter(|e| p.side(e) == sd).map(|e| num(e.thickness)).collect::<Vec<_>>().join("+")
        };
        let mut edge_mats: Vec<&str> = p.edges.iter().map(|e| material_name(&e.material)).collect();
        edge_mats.dedup();
        let fields = [
            material_name(&p.material).to_string(),
            p.pos.clone(),
            p.designation.clone(),
            p.name.clone(),
            num(l),
            num(w),
            num(p.thickness),
            r.count.to_string(),
            if p.grain() { t("yes") } else { String::new() },
            side(Side::L1),
            side(Side::L2),
            side(Side::W1),
            side(Side::W2),
            edge_mats.join(", "),
            if !p.is_rect() { t("yes") } else { String::new() },
        ];
        let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
        s.push_str(&line.join(";"));
        s.push_str("\r\n");
    }
    s
}

/// Material usage in Bazis cost estimate columns.
pub fn materials_csv(rows: &[MaterialRow]) -> String {
    let head = ["Article", "Material name", "Unit", "Factor", "Order quantity"];
    let mut s = format!("\u{feff}{}\r\n", head.map(t).join(";"));
    for r in rows {
        let (name, code) = r.material.split_once('\r').unwrap_or((&r.material, ""));
        let line = [code.to_string(), name.to_string(), t(r.unit), num(r.coef), num(r.qty)];
        s.push_str(&line.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(";"));
        s.push_str("\r\n");
    }
    s
}

pub(crate) fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Part label on the map.
pub struct Label {
    /// Part number.
    pub num: String,
    /// Part name ("Side", "Shelf"), printed under the number.
    pub name: String,
    /// Part cut size (length × width).
    pub size: (f64, f64),
    /// Edge banding present on sides L1, L2, W1, W2.
    pub edges: [bool; 4],
}

/// Map header.
pub struct Card<'a> {
    pub order: &'a str,
    pub material: &'a str,
    /// Map number (from 1) and total maps for the material.
    pub index: usize,
    pub total: usize,
}

/// Remainder that counts as a usable offcut rather than waste.
/// shortcut: fixed threshold; in BAZIS-Raskroy it is set in the nesting parameters.
const OFFCUT_MIN: (f64, f64) = (200.0, 300.0);

const FONT: &str = r#"font-family="GOST type A, ISOCPEUR, Arial Narrow, sans-serif" font-style="italic""#;

fn text(s: &mut String, x: f64, y: f64, size: f64, rotate: bool, body: &str) {
    let rot = if rotate { format!(r#" transform="rotate(-90 {x:.2} {y:.2})""#) } else { String::new() };
    let _ = write!(
        s,
        r#"<text x="{x:.2}" y="{y:.2}" font-size="{size:.2}" text-anchor="middle" dominant-baseline="middle"{rot}>{}</text>"#,
        xml(body)
    );
}

/// `name` shortened with "…" to fit `len` mm of text at font size `fs` (average glyph ≈ 0.55 em).
fn fit(name: &str, len: f64, fs: f64) -> String {
    let max = (len / (0.55 * fs)) as usize;
    if name.chars().count() <= max {
        return name.to_string();
    }
    let mut s: String = name.chars().take(max.saturating_sub(1)).collect();
    if !s.is_empty() {
        s.push('…');
    }
    s
}

/// Cutting map of one sheet in BAZIS-Raskroy style: A4 landscape page, units — mm.
pub fn card_svg(c: &Card, sheet: &Sheet, p: &Params, label: impl Fn(usize) -> Label) -> String {
    let (sw, sh) = p.sheet;
    let (ox, oy) = (10.0, 32.0);
    let k = (277.0 / sw).min(170.0 / sh);
    let tx = |x: f64| ox + x * k;
    let ty = |y: f64| oy + y * k;
    let mut s = String::new();
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="297mm" height="210mm" viewBox="0 0 297 210" {FONT}>
<defs>
<pattern id="waste" width="2" height="2" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="2" stroke="#e33" stroke-width="0.25"/></pattern>
<pattern id="offcut" width="2" height="2" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="2" stroke="#33c" stroke-width="0.25"/></pattern>
</defs>
<rect width="297" height="210" fill="#fff"/>
<g font-family="Arial Narrow, Arial, sans-serif" font-style="normal" font-weight="bold">
<text x="10" y="12" font-size="6">{}</text>
<text x="10" y="19" font-size="4.2">{}</text>
<text x="10" y="26" font-size="4.2" text-decoration="underline">{}</text>
<text x="215" y="12" font-size="3.8">{}</text>
<text x="215" y="18" font-size="3.8">{}</text>
<text x="215" y="24" font-size="3.8">{}</text>
</g>
<rect x="{ox}" y="{oy}" width="{:.2}" height="{:.2}" fill="url(#waste)" stroke="#000" stroke-width="0.3"/>
"##,
        xml(&t(format!("Order No.: {}", c.order))),
        xml(&t(format!("Material: {}", c.material))),
        t(format!("Sheet size for cutting {} x {}", num(sw), num(sh))),
        t(format!("Map {} / {}", c.index, c.total)),
        t("Sheets: 1"),
        t(format!("Panels: {}", sheet.parts.len())),
        sw * k,
        sh * k,
    );
    // Sheet edge trim: labels at the corners, as in BAZIS-Raskroy.
    text(&mut s, ox + 1.5, oy - 1.0, 1.6, false, &num(p.trim));
    text(&mut s, ox + sw * k - 1.5, oy + sh * k + 1.5, 1.6, false, &num(p.trim));

    for f in &sheet.free {
        let offcut = f.w.min(f.h) >= OFFCUT_MIN.0 && f.w.max(f.h) >= OFFCUT_MIN.1;
        if !offcut {
            continue;
        }
        let (x, y, w, h) = (tx(f.x), ty(f.y), f.w * k, f.h * k);
        let _ = write!(s, r##"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="#fff" stroke="#000" stroke-width="0.2"/><rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="url(#offcut)"/>"##);
        let fs = (h.min(w) / 5.0).clamp(1.2, 3.5);
        text(&mut s, x + w / 2.0, y + h / 2.0, fs, h > w, &t("Offcut"));
        text(&mut s, x + w / 2.0, y + h - fs, fs, false, &num(f.w));
        text(&mut s, x + w - fs, y + h / 2.0, fs, true, &num(f.h));
    }

    for d in &sheet.parts {
        let l = label(d.id);
        let (x, y, w, h) = (tx(d.x), ty(d.y), d.w * k, d.h * k);
        let _ = write!(s, r##"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="#fff" stroke="#e00" stroke-width="0.3"/>"##);
        // Edge banding, as in BAZIS-Raskroy: a black strip along the side inside the part. Unrotated, part length runs along the sheet.
        let [l1, l2, w1, w2] = l.edges;
        let sides = if d.rotated {
            [(l1, (x, y, x, y + h)), (l2, (x + w, y, x + w, y + h)), (w1, (x, y + h, x + w, y + h)), (w2, (x, y, x + w, y))]
        } else {
            [(l1, (x, y + h, x + w, y + h)), (l2, (x, y, x + w, y)), (w1, (x, y, x, y + h)), (w2, (x + w, y, x + w, y + h))]
        };
        for (on, (x1, y1, x2, y2)) in sides {
            if on {
                // Strip — the middle 60 % of the side, shifted 0.8 mm inward.
                let (mx, my) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
                let (ix, iy) = ((x + w / 2.0 - mx).signum() * 0.8 * (x1 == x2) as u8 as f64, (y + h / 2.0 - my).signum() * 0.8 * (y1 == y2) as u8 as f64);
                let (dx, dy) = ((x2 - x1) * 0.3, (y2 - y1) * 0.3);
                let _ = write!(
                    s,
                    r##"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="#000" stroke-width="0.5"/>"##,
                    mx - dx + ix,
                    my - dy + iy,
                    mx + dx + ix,
                    my + dy + iy
                );
            }
        }
        let (lw, lh) = if d.rotated { (l.size.1, l.size.0) } else { l.size };
        let fs = (w.min(h) / 4.0).clamp(1.0, 3.5);
        if w.min(h) < 9.0 {
            // Narrow part: a single "number size" line along the long side.
            let fs = (w.min(h) * 0.6).clamp(0.8, 3.0);
            let size = format!("{}x{}", num(lw), num(lh));
            let room = w.max(h) * 0.9 - 0.55 * fs * (l.num.chars().count() + size.chars().count() + 4) as f64;
            let name = fit(&l.name, room, fs);
            let body = [l.num.as_str(), &name, &size].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join("  ");
            text(&mut s, x + w / 2.0, y + h / 2.0, fs, h > w, &body);
        } else {
            let along = h > w * 1.5;
            let (len, across) = if along { (h, w) } else { (w, h) };
            if across >= 5.0 * fs {
                // Number above the middle, name below; "below" is +x for text rotated along a tall part.
                // Centred text keeps clear of the size label at the edge (≈1.5 fs) on both sides.
                let name = fit(&l.name, len - 4.0 * fs, fs * 0.8);
                let (dx, dy) = if name.is_empty() { (0.0, 0.0) } else if along { (fs * 0.6, 0.0) } else { (0.0, fs * 0.6) };
                text(&mut s, x + w / 2.0 - dx, y + h / 2.0 - dy, fs, along, &l.num);
                if !name.is_empty() {
                    text(&mut s, x + w / 2.0 + dx, y + h / 2.0 + dy, fs * 0.8, along, &name);
                }
            } else {
                // Low part: no room for two lines above the size, so "number name" on one line.
                let name = fit(&l.name, len - 4.0 * fs - 0.55 * fs * (l.num.chars().count() + 1) as f64, fs);
                let body = if name.is_empty() { l.num.clone() } else { format!("{} {}", l.num, name) };
                text(&mut s, x + w / 2.0, y + h / 2.0, fs, along, &body);
            }
            text(&mut s, x + w / 2.0, y + h - fs, fs, false, &num(lw));
            text(&mut s, x + w - fs, y + h / 2.0, fs, true, &num(lh));
        }
    }
    s.push_str("</svg>\n");
    s
}

/// “Panels for production” pages: table of parts by material, as in BAZIS-Raskroy.
pub fn production_html(order: &str, rows: &[SpecRow], number: impl Fn(&SpecRow) -> String) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i < rows.len() {
        let mat = &rows[i].panel.material;
        let _ = write!(
            s,
            r#"<section class="page prod"><h2>{}</h2><h3>{}</h3><h3>{}</h3><table><tr><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>"#,
            xml(&t("Panels for production")),
            xml(&t(format!("Order No. {}", order))),
            xml(&t(format!("Material: {}", material_name(mat)))),
            t("PART No."),
            t("Item name"),
            t("Length"),
            t("Width"),
            t("Qty"),
            t("Thick."),
            t("Grooves")
        );
        // Merge rows identical in the visible columns (edge banding isn't shown here).
        let mut lines: Vec<([String; 5], usize)> = Vec::new();
        while i < rows.len() && &rows[i].panel.material == mat {
            let r = &rows[i];
            let (l, w) = r.panel.cut_size();
            let cells = [number(r), r.panel.name.clone(), num(l), num(w), num(r.panel.thickness)];
            match lines.iter_mut().find(|(c, _)| *c == cells) {
                Some((_, n)) => *n += r.count,
                None => lines.push((cells, r.count)),
            }
            i += 1;
        }
        for ([no, name, l, w, t], n) in &lines {
            let _ = write!(
                s,
                "<tr><td>{}</td><td>{}</td><td>{l}</td><td>{w}</td><td>{n}</td><td>{t}</td><td></td></tr>",
                xml(no),
                xml(name)
            );
        }
        s.push_str("</table></section>\n");
    }
    s
}

/// HTML document for printing: maps as A4 landscape pages, then tables.
pub fn document_html(title: &str, cards: &[String], production: &str) -> String {
    let mut s = format!(
        r#"<!doctype html><html lang="{}"><head><meta charset="utf-8"><title>{}</title><style>
@page {{ size: A4 landscape; margin: 0 }}
body {{ margin: 0; background: #888 }}
.page {{ width: 297mm; height: 210mm; margin: 4mm auto; background: #fff; page-break-after: always; overflow: hidden }}
.page svg {{ display: block }}
.prod {{ box-sizing: border-box; padding: 12mm 20mm; font-family: Arial, sans-serif; font-size: 11pt; height: auto; min-height: 210mm }}
.prod h2, .prod h3 {{ text-align: center; margin: 2mm }}
.prod table {{ border-collapse: collapse; width: 100%; margin-top: 4mm }}
.prod th {{ border-bottom: 1px solid #000; text-align: left }}
.prod td, .prod th {{ padding: 1mm 2mm }}
@media print {{ body {{ background: none }} .page {{ margin: 0 }} }}
</style></head><body>
"#,
        if crate::i18n::russian() { "ru" } else { "en" },
        xml(title)
    );
    for c in cards {
        s.push_str(r#"<section class="page">"#);
        s.push_str(c);
        s.push_str("</section>\n");
    }
    s.push_str(production);
    s.push_str("</body></html>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_has_header_parts_and_offcut() {
        use crate::nest::{Item, nest};
        let p = Params { sheet: (2800.0, 2070.0), kerf: 4.0, trim: 10.0 };
        let lay = nest(&[Item { id: 0, length: 1000.0, width: 500.0, grain: true }], &p);
        let card = Card { order: "Order <1>", material: "Chipboard", index: 1, total: 1 };
        let svg = card_svg(&card, &lay.sheets[0], &p, |_| Label { num: "7".into(), name: "Side".into(), size: (1000.0, 500.0), edges: [true, false, false, false] });
        assert!(svg.contains("Order No.: Order &lt;1&gt;"));
        assert!(svg.contains("Map 1 / 1") && svg.contains("Panels: 1"));
        assert!(svg.contains("Offcut"), "a large sheet remainder is a usable offcut");
        assert_eq!(svg.matches(r##"stroke-width="0.5""##).count(), 1, "one edge-banded side");
        assert!(svg.contains(">Side</text>"), "part name under the number");
    }

    #[test]
    fn long_name_is_shortened() {
        assert_eq!(fit("Shelf", 100.0, 3.0), "Shelf");
        assert_eq!(fit("Partition, vertical", 11.5, 2.0), "Partition…");
        assert_eq!(fit("Shelf", 0.5, 3.0), "");
    }

    #[test]
    fn csv_escapes_separators() {
        assert_eq!(csv_field("a;b"), "\"a;b\"");
        assert_eq!(csv_field("x\"y"), "\"x\"\"y\"");
        assert_eq!(num(1868.0000001), "1868");
        assert_eq!(num(0.4), "0,4");
    }
}
