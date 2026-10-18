//! PDF without a browser: A4 landscape pages from our SVG (cutting maps, drawings) with an embedded
//! TrueType font (Cyrillic). Font handling and PDF assembly are ported from QuadFurn (github.com/rbm13s/QuadFurn,
//! `src/export.rs`) with the author's permission; the SVG → PDF translation is our own, for the SVG subset
//! that `report` and `drawing` generate (rect, line, polyline, circle, text, g, hatching).

use std::collections::BTreeMap;

use ab_glyph::{Font, FontArc, GlyphId};

/// If given a TrueType Collection (`.ttc`, `ttcf` header), extracts the first face (face 0)
/// into a standalone TrueType (`.ttf`) buffer usable for `ab_glyph::FontArc`, `egui::FontData` and `/FontFile2` in PDF.
pub fn ensure_ttf_bytes(bytes: Vec<u8>) -> Vec<u8> {
    if bytes.len() < 16 || &bytes[0..4] != b"ttcf" {
        return bytes;
    }
    let num_fonts = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if num_fonts == 0 {
        return bytes;
    }
    let face0_offset = u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if face0_offset + 12 > bytes.len() {
        return bytes;
    }
    let num_tables =
        u16::from_be_bytes([bytes[face0_offset + 4], bytes[face0_offset + 5]]) as usize;
    let dir_end = face0_offset + 12 + num_tables * 16;
    if dir_end > bytes.len() {
        return bytes;
    }

    let mut out = Vec::with_capacity(bytes.len() / 2);
    out.extend_from_slice(&bytes[face0_offset..face0_offset + 12]);
    let records_pos = out.len();
    out.resize(records_pos + num_tables * 16, 0);

    for i in 0..num_tables {
        let rec_src = face0_offset + 12 + i * 16;
        let tag = &bytes[rec_src..rec_src + 4];
        let checksum = &bytes[rec_src + 4..rec_src + 8];
        let orig_offset = u32::from_be_bytes([
            bytes[rec_src + 8],
            bytes[rec_src + 9],
            bytes[rec_src + 10],
            bytes[rec_src + 11],
        ]) as usize;
        let length = u32::from_be_bytes([
            bytes[rec_src + 12],
            bytes[rec_src + 13],
            bytes[rec_src + 14],
            bytes[rec_src + 15],
        ]) as usize;

        if orig_offset + length > bytes.len() {
            return bytes;
        }
        let new_offset = out.len() as u32;
        let rec_dst = records_pos + i * 16;
        out[rec_dst..rec_dst + 4].copy_from_slice(tag);
        out[rec_dst + 4..rec_dst + 8].copy_from_slice(checksum);
        out[rec_dst + 8..rec_dst + 12].copy_from_slice(&new_offset.to_be_bytes());
        out[rec_dst + 12..rec_dst + 16].copy_from_slice(&(length as u32).to_be_bytes());

        out.extend_from_slice(&bytes[orig_offset..orig_offset + length]);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    out
}

/// TrueType font for PDF: embedded whole, text is written as glyph numbers (Identity-H).
struct PdfTrueTypeFont {
    raw_bytes: Vec<u8>,
    font: FontArc,
    upm: f32,
    used_glyphs: BTreeMap<u16, (char, i32)>,
}

/// System fonts with Cyrillic: Windows, macOS, Linux.
const REGULAR: &[&str] = &[
    "C:\\Windows\\Fonts\\arial.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
];
const BOLD: &[&str] = &[
    "C:\\Windows\\Fonts\\arialbd.ttf",
    "C:\\Windows\\Fonts\\segoeuib.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
];

impl PdfTrueTypeFont {
    fn load(candidates: &[&str]) -> Result<Self, String> {
        for path in candidates {
            if let Ok(raw) = std::fs::read(path) {
                let bytes = ensure_ttf_bytes(raw);
                if let Ok(font) = FontArc::try_from_vec(bytes.clone()) {
                    let upm = font.units_per_em().unwrap_or(2048.0).max(1.0);
                    return Ok(Self {
                        raw_bytes: bytes,
                        font,
                        upm,
                        used_glyphs: BTreeMap::new(),
                    });
                }
            }
        }
        Err("Не удалось найти системный шрифт TrueType (Segoe UI / Arial) для экспорта в PDF".to_string())
    }

    fn glyph_width_1000(&self, gid: GlyphId) -> i32 {
        ((self.font.h_advance_unscaled(gid) * 1000.0) / self.upm).round() as i32
    }

    /// Measures string width in points (pt) for the given font size
    fn measure_text_pt(&self, text: &str, font_size_pt: f32) -> f32 {
        let mut total_1000 = 0i32;
        for ch in text.chars() {
            let gid = self.font.glyph_id(ch);
            total_1000 += self.glyph_width_1000(gid);
        }
        (total_1000 as f32) * (font_size_pt / 1000.0)
    }

    /// Encodes a string as 16-bit hex Glyph IDs (`<001A002B...>`) for the PDF `Tj` operator
    /// and records used glyphs for the `/W` and `/ToUnicode` tables
    fn encode_hex_gids(&mut self, text: &str) -> String {
        let mut hex = String::with_capacity(text.len() * 4 + 2);
        hex.push('<');
        for ch in text.chars() {
            let gid = self.font.glyph_id(ch);
            let w1000 = self.glyph_width_1000(gid);
            self.used_glyphs.entry(gid.0).or_insert((ch, w1000));
            hex.push_str(&format!("{:04X}", gid.0));
        }
        hex.push('>');
        hex
    }

    /// Builds the glyph widths array `/W [ gid [w] ... ]` for the `/CIDFontType2` dictionary
    fn build_widths_array(&self) -> String {
        let mut out = String::from("[");
        for (&gid, &(_, w)) in &self.used_glyphs {
            out.push_str(&format!(" {} [{}]", gid, w));
        }
        out.push_str(" ]");
        out
    }

    /// Builds the `/ToUnicode` CMap stream for correct text copying and search in PDF
    fn build_tounicode_cmap(&self) -> Vec<u8> {
        let mut cmap = String::new();
        cmap.push_str("/CIDInit /ProcSet findresource begin\n");
        cmap.push_str("12 dict begin\n");
        cmap.push_str("begincmap\n");
        cmap.push_str("/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
        cmap.push_str("/CMapName /Adobe-Identity-UCS def\n");
        cmap.push_str("/CMapType 2 def\n");
        cmap.push_str("1 begincodespacerange\n");
        cmap.push_str("<0000> <FFFF>\n");
        cmap.push_str("endcodespacerange\n");

        let entries: Vec<(u16, char)> = self
            .used_glyphs
            .iter()
            .map(|(&gid, &(ch, _))| (gid, ch))
            .collect();

        for chunk in entries.chunks(100) {
            cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
            for &(gid, ch) in chunk {
                let code = ch as u32;
                if code <= 0xFFFF {
                    cmap.push_str(&format!("<{:04X}> <{:04X}>\n", gid, code));
                } else {
                    let mut utf16 = [0u16; 2];
                    let encoded = ch.encode_utf16(&mut utf16);
                    if encoded.len() == 2 {
                        cmap.push_str(&format!(
                            "<{:04X}> <{:04X}{:04X}>\n",
                            gid, encoded[0], encoded[1]
                        ));
                    }
                }
            }
            cmap.push_str("endbfchar\n");
        }

        cmap.push_str("endcmap\n");
        cmap.push_str("CMapName currentdict /CMap defineresource pop\n");
        cmap.push_str("end\nend\n");
        cmap.into_bytes()
    }
}

/// A4 landscape, points; SVG pages are in mm (viewBox 0 0 297 210).
const PAGE_W: f32 = 841.89;
const PAGE_H: f32 = 595.28;
const K: f32 = PAGE_W / 297.0;

/// SVG element: name, attributes, text (for `<text>`); closing tag — `close`.
struct Tag {
    name: String,
    attrs: BTreeMap<String, String>,
    close: bool,
    self_closing: bool,
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// Parses a tag `<name a="v" …>` from the slice between `<` and `>`.
fn tag(body: &str) -> Tag {
    let close = body.starts_with('/');
    let self_closing = body.ends_with('/');
    let body = body.trim_start_matches('/').trim_end_matches('/');
    let name: String = body.chars().take_while(|c| !c.is_whitespace()).collect();
    let mut attrs = BTreeMap::new();
    let mut rest = &body[name.len()..];
    while let Some(eq) = rest.find("=\"") {
        let key = rest[..eq].trim().to_string();
        let after = &rest[eq + 2..];
        let Some(end) = after.find('"') else { break };
        attrs.insert(key, unescape(&after[..end]));
        rest = &after[end + 1..];
    }
    Tag { name, attrs, close, self_closing }
}

fn color(v: &str) -> Option<[f32; 3]> {
    let h = v.strip_prefix('#')?;
    let p = |s: &str| u8::from_str_radix(s, 16).ok().map(|x| x as f32 / 255.0);
    match h.len() {
        3 => Some([p(&h[0..1].repeat(2))?, p(&h[1..2].repeat(2))?, p(&h[2..3].repeat(2))?]),
        6 => Some([p(&h[0..2])?, p(&h[2..4])?, p(&h[4..6])?]),
        _ => None,
    }
}

/// SVG page → PDF command stream. Fonts: /F1 regular, /F2 bold.
fn page(svg: &str, reg: &mut PdfTrueTypeFont, bold: &mut PdfTrueTypeFont) -> String {
    let mut out = String::new();
    let px = |x: f32| x * K;
    let py = |y: f32| PAGE_H - y * K;
    // Inherited group attributes (<svg>, <g>): bold, etc.
    let mut stack: Vec<BTreeMap<String, String>> = vec![BTreeMap::new()];
    let mut in_defs = false;
    let mut i = 0;
    while let Some(start) = svg[i..].find('<') {
        let start = i + start;
        let Some(end) = svg[start..].find('>') else { break };
        let end = start + end;
        let t = tag(&svg[start + 1..end]);
        i = end + 1;
        let inherited = |k: &str, t: &Tag, stack: &[BTreeMap<String, String>]| t.attrs.get(k).cloned().or_else(|| stack.iter().rev().find_map(|m| m.get(k).cloned()));
        let f = |k: &str| t.attrs.get(k).and_then(|v| v.trim_end_matches("mm").parse::<f32>().ok()).unwrap_or(0.0);
        match (t.name.as_str(), t.close) {
            ("defs", false) => in_defs = !t.self_closing,
            ("defs", true) => in_defs = false,
            (_, _) if in_defs => {}
            ("g" | "svg", false) => {
                if !t.self_closing {
                    stack.push(t.attrs.clone());
                }
            }
            ("g" | "svg", true) => {
                stack.pop();
            }
            ("rect", false) => {
                let (x, y) = (f("x"), f("y"));
                let w = t.attrs.get("width").and_then(|v| v.trim_end_matches("mm").parse().ok()).unwrap_or(297.0);
                let h = t.attrs.get("height").and_then(|v| v.trim_end_matches("mm").parse().ok()).unwrap_or(210.0);
                let r = format!("{:.2} {:.2} {:.2} {:.2} re", px(x), py(y + h), w * K, h * K);
                let fill = t.attrs.get("fill").cloned().unwrap_or_else(|| "#000".into());
                if let Some(c) = color(&fill) {
                    out += &format!("{:.3} {:.3} {:.3} rg {r} f\n", c[0], c[1], c[2]);
                } else if fill.starts_with("url(#") {
                    // 45° hatching with 2 mm spacing, clipped to the rectangle (like a pattern in SVG).
                    let c = if fill.contains("offcut") { [0.2, 0.2, 0.8] } else { [0.9, 0.2, 0.2] };
                    out += &format!("q {r} W n {:.3} {:.3} {:.3} RG {:.2} w\n", c[0], c[1], c[2], 0.25 * K);
                    let mut s = -h;
                    while s < w {
                        out += &format!("{:.2} {:.2} m {:.2} {:.2} l\n", px(x + s), py(y + h), px(x + s + h), py(y));
                        s += 2.0 * std::f32::consts::SQRT_2;
                    }
                    out += "S Q\n";
                }
                if let Some(c) = t.attrs.get("stroke").and_then(|v| color(v)) {
                    out += &format!("{:.3} {:.3} {:.3} RG {:.2} w {r} S\n", c[0], c[1], c[2], f("stroke-width").max(0.1) * K);
                }
            }
            ("line" | "polyline", false) => {
                let pts: Vec<(f32, f32)> = if t.name == "line" {
                    vec![(f("x1"), f("y1")), (f("x2"), f("y2"))]
                } else {
                    t.attrs.get("points").map(|p| p.split_whitespace().filter_map(|xy| { let (a, b) = xy.split_once(',')?; Some((a.parse().ok()?, b.parse().ok()?)) }).collect()).unwrap_or_default()
                };
                let Some(c) = t.attrs.get("stroke").and_then(|v| color(v)) else { continue };
                if pts.len() < 2 {
                    continue;
                }
                let dash = t.attrs.get("stroke-dasharray").map(|d| format!("[{}] 0 d", d.split_whitespace().map(|v| format!("{:.2}", v.parse::<f32>().unwrap_or(1.0) * K)).collect::<Vec<_>>().join(" "))).unwrap_or_else(|| "[] 0 d".into());
                out += &format!("{:.3} {:.3} {:.3} RG {:.2} w {dash}\n", c[0], c[1], c[2], f("stroke-width").max(0.1) * K);
                out += &format!("{:.2} {:.2} m", px(pts[0].0), py(pts[0].1));
                for p in &pts[1..] {
                    out += &format!(" {:.2} {:.2} l", px(p.0), py(p.1));
                }
                out += " S [] 0 d\n";
                // Dimension line arrows (marker-start / marker-end).
                let arrow = |tip: (f32, f32), from: (f32, f32)| {
                    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
                    let l = (dx * dx + dy * dy).sqrt().max(1e-6);
                    let (ux, uy) = (dx / l, dy / l);
                    let (bx, by) = (tip.0 - ux * 1.6, tip.1 - uy * 1.6);
                    format!("{:.2} {:.2} m {:.2} {:.2} l {:.2} {:.2} l f\n", px(tip.0), py(tip.1), px(bx - uy * 0.45), py(by + ux * 0.45), px(bx + uy * 0.45), py(by - ux * 0.45))
                };
                out += &format!("{:.3} {:.3} {:.3} rg\n", c[0], c[1], c[2]);
                if t.attrs.contains_key("marker-start") {
                    out += &arrow(pts[0], pts[1]);
                }
                if t.attrs.contains_key("marker-end") {
                    out += &arrow(pts[pts.len() - 1], pts[pts.len() - 2]);
                }
            }
            ("circle", false) => {
                let (cx, cy, r) = (px(f("cx")), py(f("cy")), f("r") * K);
                let m = 0.5523 * r;
                let path = format!(
                    "{:.2} {cy:.2} m {:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c {:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c",
                    cx + r, cx + r, cy + m, cx + m, cy + r, cy + r, cx - m, cy + r, cx - r, cy + m, cx - r, cx - r, cy - m, cx - m, cy - r, cy - r, cx + m, cy - r, cx + r, cy - m, cx + r
                );
                if let Some(c) = t.attrs.get("fill").and_then(|v| color(v)) {
                    out += &format!("{:.3} {:.3} {:.3} rg {path} f\n", c[0], c[1], c[2]);
                }
                if let Some(c) = t.attrs.get("stroke").and_then(|v| color(v)) {
                    out += &format!("{:.3} {:.3} {:.3} RG {:.2} w {path} S\n", c[0], c[1], c[2], f("stroke-width").max(0.1) * K);
                }
            }
            ("text", false) if !t.self_closing => {
                let Some(close) = svg[i..].find("</text>") else { continue };
                let body = unescape(&svg[i..i + close]);
                i += close + "</text>".len();
                let size = f("font-size").max(0.5) * K;
                let is_bold = inherited("font-weight", &t, &stack).is_some_and(|w| w == "bold");
                let font = if is_bold { &mut *bold } else { &mut *reg };
                let width = font.measure_text_pt(&body, size);
                let shift = match inherited("text-anchor", &t, &stack).as_deref() {
                    Some("middle") => -width / 2.0,
                    Some("end") => -width,
                    _ => 0.0,
                };
                // dominant-baseline="middle": baseline is a third of the font size lower.
                let down = if t.attrs.get("dominant-baseline").is_some_and(|v| v == "middle") { size * 0.35 } else { 0.0 };
                let fill = t.attrs.get("fill").and_then(|v| color(v)).unwrap_or([0.0; 3]);
                let (x, y) = (px(f("x")), py(f("y")));
                // rotate(a cx cy): PDF Y axis points up — the angle flips sign.
                let angle = t.attrs.get("transform").and_then(|tr| tr.strip_prefix("rotate(")).and_then(|r| r.split_whitespace().next()).and_then(|a| a.parse::<f32>().ok()).unwrap_or(0.0);
                let (s, c) = (-angle.to_radians()).sin_cos();
                let (ox, oy) = (x + c * shift + s * down, y + s * shift - c * down);
                let hex = font.encode_hex_gids(&body);
                out += &format!("BT /{} {size:.2} Tf {:.3} {:.3} {:.3} rg {c:.4} {s:.4} {:.4} {c:.4} {ox:.2} {oy:.2} Tm {hex} Tj ET\n", if is_bold { "F2" } else { "F1" }, fill[0], fill[1], fill[2], -s);
            }
            _ => {}
        }
    }
    out
}

/// SVG pages (A4 landscape) → one PDF with embedded fonts.
pub fn from_svg_pages(pages: &[String]) -> Result<Vec<u8>, String> {
    let mut reg = PdfTrueTypeFont::load(REGULAR)?;
    let mut bold = PdfTrueTypeFont::load(BOLD).or_else(|_| PdfTrueTypeFont::load(REGULAR))?;
    let streams: Vec<Vec<u8>> = pages.iter().map(|p| page(p, &mut reg, &mut bold).into_bytes()).collect();
    let total_pages = streams.len();
    let first_page_obj_id = 13usize;
    let total_objects = first_page_obj_id + total_pages * 2 - 1;
    let mut pdf: Vec<u8> = Vec::with_capacity(reg.raw_bytes.len() + bold.raw_bytes.len() + 98304);
    pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets: Vec<usize> = vec![0; total_objects + 1];
    let write_obj = |pdf: &mut Vec<u8>, offsets: &mut [usize], id: usize, body: &[u8]| {
        offsets[id] = pdf.len();
        pdf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };
    let write_stream_obj = |pdf: &mut Vec<u8>, offsets: &mut [usize], id: usize, extra_dict: &str, data: &[u8]| {
        offsets[id] = pdf.len();
        pdf.extend_from_slice(format!("{id} 0 obj\n<< /Length {} {extra_dict} >>\nstream\n", data.len()).as_bytes());
        pdf.extend_from_slice(data);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");
    };
    write_obj(&mut pdf, &mut offsets, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..total_pages).map(|i| format!("{} 0 R", first_page_obj_id + i * 2)).collect();
    write_obj(&mut pdf, &mut offsets, 2, format!("<< /Type /Pages /Kids [ {} ] /Count {total_pages} >>", kids.join(" ")).as_bytes());
    let mut write_font_family = |base_id: usize, font_name: &str, ttf: &PdfTrueTypeFont| {
        let (cid_id, desc_id, touni_id, file_id) = (base_id + 1, base_id + 2, base_id + 3, base_id + 4);
        write_obj(&mut pdf, &mut offsets, base_id, format!("<< /Type /Font /Subtype /Type0 /BaseFont /{font_name} /Encoding /Identity-H /DescendantFonts [ {cid_id} 0 R ] /ToUnicode {touni_id} 0 R >>").as_bytes());
        write_obj(&mut pdf, &mut offsets, cid_id, format!("<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{font_name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {desc_id} 0 R /CIDToGIDMap /Identity /DW 600 /W {} >>", ttf.build_widths_array()).as_bytes());
        write_obj(&mut pdf, &mut offsets, desc_id, format!("<< /Type /FontDescriptor /FontName /{font_name} /Flags 32 /FontBBox [-500 -300 1500 1000] /ItalicAngle 0 /Ascent 900 /Descent -250 /CapHeight 700 /StemV 80 /FontFile2 {file_id} 0 R >>").as_bytes());
        write_stream_obj(&mut pdf, &mut offsets, touni_id, "", &ttf.build_tounicode_cmap());
        write_stream_obj(&mut pdf, &mut offsets, file_id, &format!("/Length1 {}", ttf.raw_bytes.len()), &ttf.raw_bytes);
    };
    write_font_family(3, "DowelCraftRegular", &reg);
    write_font_family(8, "DowelCraftBold", &bold);
    for (i, data) in streams.iter().enumerate() {
        let page_obj_id = first_page_obj_id + i * 2;
        write_obj(&mut pdf, &mut offsets, page_obj_id, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.2} {PAGE_H:.2}] /Resources << /Font << /F1 3 0 R /F2 8 0 R >> >> /Contents {} 0 R >>", page_obj_id + 1).as_bytes());
        write_stream_obj(&mut pdf, &mut offsets, page_obj_id + 1, "", data);
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", total_objects + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    for off in &offsets[1..=total_objects] {
        pdf.extend_from_slice(format!("{off:010} 00000 n \r\n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n", total_objects + 1).as_bytes());
    Ok(pdf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_page_becomes_pdf() {
        let svg = r##"<svg viewBox="0 0 297 210"><g font-weight="bold"><text x="10" y="12" font-size="6">Заказ «1»</text></g><rect x="10" y="20" width="50" height="30" fill="url(#waste)" stroke="#000" stroke-width="0.3"/><line x1="0" y1="0" x2="10" y2="0" stroke="#000" stroke-width="0.2" marker-end="url(#a)"/><circle cx="5" cy="5" r="2" fill="none" stroke="#000"/><text x="50" y="50" font-size="3" text-anchor="middle" transform="rotate(-90 50 50)">500</text></svg>"##;
        let Ok(pdf) = from_svg_pages(&[svg.to_string(), svg.to_string()]) else { return }; // no system font — nothing to check
        assert!(pdf.starts_with(b"%PDF-1.4") && pdf.ends_with(b"%%EOF\n"));
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("/Count 2") && s.contains("BT /F2 17.01 Tf") && s.contains(" re W n"), "жирный текст и штриховка");
    }
}
