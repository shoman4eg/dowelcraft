//! Проект для БАЗИС-Раскроя: XML `<PROJECT version="2" subver="7">` в windows-1251.
//! Это формат, в котором Мебельщик передаёт заказ в Раскрой. Структура повторяет
//! эталоны из `reference/raskroy-project-*.xml`.
//!
//! shortcut: не выгружаются сверловка, пазы, вырезы, фурнитура и цены (элементы пустые или QTY=0);
//! у фигурных деталей контур заготовки равен контуру детали. Добавлять по мере появления эталонов.

use std::f64::consts::{FRAC_PI_2, TAU};
use std::fmt::Write;

use crate::contour::Seg;
use crate::panel::{Edge, Panel, RectSide, Texture};

/// GUID профиля «Гладкий торец» из эталонов.
const SMOOTH_EDGE: &str = "{5F07E058-1810-4848-ACE4-33BE7EF0C93E}";

/// Число как в эталонах: не больше 7 знаков после точки, без хвостовых нулей.
fn num(v: f64) -> String {
    let s = format!("{:.7}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `Название\rАртикул` → (название, артикул).
fn split_mat(m: &str) -> (&str, &str) {
    m.split_once('\r').unwrap_or((m, ""))
}

struct X {
    s: String,
    depth: usize,
}

impl X {
    fn open(&mut self, tag: &str) {
        let _ = write!(self.s, "{}<{tag}>\r\n", "  ".repeat(self.depth));
        self.depth += 1;
    }
    fn close(&mut self, tag: &str) {
        self.depth -= 1;
        let _ = write!(self.s, "{}</{tag}>\r\n", "  ".repeat(self.depth));
    }
    fn leaf(&mut self, tag: &str, v: impl std::fmt::Display) {
        let _ = write!(self.s, "{}<{tag}>{v}</{tag}>\r\n", "  ".repeat(self.depth));
    }
    fn text(&mut self, tag: &str, v: &str) {
        self.leaf(tag, esc(v));
    }
    fn qty0(&mut self, tag: &str) {
        self.open(tag);
        self.leaf("QTY", 0);
        self.close(tag);
    }
    fn xyz(&mut self, tag: &str, p: [f64; 2]) {
        self.open(tag);
        self.leaf("X", num(p[0]));
        self.leaf("Y", num(p[1]));
        self.leaf("Z", 0);
        self.close(tag);
    }
}

/// Общая часть описания материала. `sheet` — формат листа (для кромки — длина рулона и ширина ленты).
fn material_body(x: &mut X, full: &str, thickness: f64, sheet: (f64, f64), core: bool) {
    let (name, code) = split_mat(full);
    x.text("NAME", name);
    x.text("CODE", code);
    x.qty0("PATH");
    x.leaf("THICKNESS", num(thickness));
    x.leaf("isotropic", "false");
    x.text("SUPPLIER", "");
    x.open("PRICE");
    x.leaf("VALUE", 0);
    x.text("CURRENCY", "Рубль");
    x.text("SIGN", "руб.");
    x.close("PRICE");
    x.leaf("MESUREUNIT", 1);
    x.leaf("OVERRUN", 1);
    if core {
        x.text("APPEARANCEEDGE", name);
        x.text("APPEARANCEFACE", name);
        x.text("APPEARANCEBACK", name);
    } else {
        x.text("APPEARANCE", name);
    }
    x.open("SHEETDATA");
    x.open("RECTANGLE");
    x.leaf("LENGTH", num(sheet.0));
    x.leaf("WIDTH", num(sheet.1));
    x.close("RECTANGLE");
    x.leaf("DENSITY", 1);
    x.leaf("TEXTURED", "true");
    x.close("SHEETDATA");
}

fn len(p: [f64; 2], q: [f64; 2]) -> f64 {
    (q[0] - p[0]).hypot(q[1] - p[1])
}

/// Дуга в виде Раскроя: локальная система с центром в начале координат; дуга в ней всегда
/// идёт против часовой стрелки, а обход по часовой задаётся зеркалом по X и Z.
fn arc(x: &mut X, i: usize, a: [f64; 2], b: Option<[f64; 2]>, c: [f64; 2], ccw: bool) -> f64 {
    let r = len(a, c);
    let sx = if ccw { 1.0 } else { -1.0 };
    let local = |p: [f64; 2]| (p[1] - c[1]).atan2(sx * (p[0] - c[0]));
    let mut start = local(a).rem_euclid(TAU);
    if start < 1e-9 {
        start = TAU;
    }
    let mut sweep = b.map_or(TAU, |b| (local(b) - local(a)).rem_euclid(TAU));
    if sweep < 1e-9 {
        sweep = TAU;
    }
    x.open("CONTITEMARC");
    x.leaf("INDEX", i);
    x.leaf("PERIMETER", num(r * sweep));
    x.xyz("C", c);
    x.xyz("X", [c[0] + sx * r, c[1]]);
    x.xyz("Y", [c[0], c[1] + r]);
    x.leaf("RX", num(r));
    x.leaf("RY", num(r));
    x.leaf("TYPE", 1);
    x.leaf("ANGLESTART", num(start));
    x.leaf("ANGLEEND", num(start + sweep));
    x.leaf("ANGLESTEP", "0.0872665");
    x.open("TF");
    for (tag, tx) in [("DIR", -sx * c[0]), ("INV", c[0])] {
        let ty = if tag == "DIR" { -c[1] } else { c[1] };
        let m = [[sx, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, sx, 0.0], [tx, ty, 0.0, 1.0]];
        x.open(tag);
        for (r, row) in m.iter().enumerate() {
            for (k, v) in row.iter().enumerate() {
                x.leaf(&format!("A{r}{k}"), num(*v));
            }
        }
        x.close(tag);
    }
    x.close("TF");
    x.close("CONTITEMARC");
    r * sweep
}

fn contour(x: &mut X, tag: &str, segs: &[Seg]) {
    // Периметр пишется до элементов — считаем его заранее во временный буфер.
    let mut body = X { s: String::new(), depth: x.depth + 2 };
    let mut perimeter = 0.0;
    let mut n = 0;
    for s in segs {
        perimeter += match *s {
            Seg::Line { a, b } => {
                body.open("CONTITEMPLINE");
                body.leaf("INDEX", n);
                body.leaf("PERIMETER", num(len(a, b)));
                body.open("BODY");
                for (k, p) in [a, b].into_iter().enumerate() {
                    body.open("VERTEXPLINE");
                    body.leaf("INDEX", k);
                    body.leaf("X", num(p[0]));
                    body.leaf("Y", num(p[1]));
                    body.leaf("Z", 0);
                    body.close("VERTEXPLINE");
                }
                body.close("BODY");
                body.close("CONTITEMPLINE");
                len(a, b)
            }
            Seg::Arc { a, b, c, ccw } => arc(&mut body, n, a, Some(b), c, ccw),
            Seg::Circle { c, r } => arc(&mut body, n, [c[0] + r, c[1]], None, c, true),
        };
        n += 1;
    }
    x.open(tag);
    x.open("ITEMSEXT");
    x.leaf("QTY", n);
    x.leaf("PERIMETER", num(perimeter));
    x.s.push_str(&body.s);
    x.close("ITEMSEXT");
    x.close(tag);
}

fn edge(x: &mut X, tag: &str, index: usize, e: Option<&Edge>) {
    x.open(tag);
    x.leaf("INDEX", index);
    x.leaf("GUID", SMOOTH_EDGE);
    x.text("NAME", "Гладкий торец");
    x.qty0("PARAM");
    if let Some(e) = e {
        x.open("MATERIAL");
        material_body(x, &e.material, e.thickness, (0.0, e.width), false);
        x.close("MATERIAL");
    }
    x.leaf("TEXTUREANG", 0);
    x.leaf("MATERIALTYPE", "EDGE_MATERIAL");
    x.leaf("ROTATE", "false");
    x.close(tag);
}

fn board(x: &mut X, p: &Panel, sheet: (f64, f64)) {
    let rect = p.is_rect();
    let tag = if rect { "BOARDSMP" } else { "BOARDFIG" };
    let (bx, by) = p.blank();
    let edge_of = |i: usize| p.edges.iter().find(|e| e.segment == i);
    x.open(tag);
    x.text("NAME", &p.name);
    x.text("CODE", &p.designation);
    x.leaf("QTY", 1);
    x.leaf("SIZEX", num(p.size.0));
    x.leaf("SIZEY", num(p.size.1));
    x.leaf("THICKNESS", num(p.thickness));
    x.leaf("BLANKSIZEX", num(bx));
    x.leaf("BLANKSIZEY", num(by));
    x.open("BOARDMATERIALCORE");
    material_body(x, &p.material, p.thickness, sheet, true);
    x.close("BOARDMATERIALCORE");
    match p.texture {
        Texture::Horizontal => x.leaf("TEXTUREANG", 0),
        Texture::Vertical => x.leaf("TEXTUREANG", num(FRAC_PI_2)),
        Texture::Default => {}
    }
    x.leaf("COREMATERIALSQUARE", format!("{:.3}", bx * by / 1e6));
    // У прямоугольных деталей длина реза — всегда по X, у фигурных — вдоль текстуры.
    let along_y = !rect && (p.texture == Texture::Vertical || (p.texture == Texture::Default && by > bx));
    let (l, w) = if along_y { (by, bx) } else { (bx, by) };
    x.leaf("LENGTHCORECUTTING", num(l));
    x.leaf("WIDTHCORECUTTING", num(w));
    for t in ["NOTCH", "MILLING", "SAWCUT", "SAWCUT_1"] {
        x.qty0(t);
    }
    contour(x, "BOARDCONTOUR", &p.segs);
    x.open("DRILLING");
    x.close("DRILLING");
    if rect {
        // Контур заготовки: тот же прямоугольник, стянутый до размеров заготовки.
        let fit = |q: [f64; 2]| [if q[0] > EPS_HALF { bx } else { 0.0 }, if q[1] > EPS_HALF { by } else { 0.0 }];
        let core: Vec<Seg> = p
            .segs
            .iter()
            .map(|s| match *s {
                Seg::Line { a, b } => Seg::Line { a: fit(a), b: fit(b) },
                s => s,
            })
            .collect();
        contour(x, "BOARDCONTOURCORE", &core);
        for (t, pos) in [("CNLB", "LEFTBOTTOM"), ("CNLT", "LEFTTOP"), ("CNRT", "RIGHTTOP"), ("CNRB", "RIGHTBOTTOM")] {
            x.open(t);
            x.open("CORNERSIMPLE");
            x.leaf("POS", pos);
            x.close("CORNERSIMPLE");
            x.close(t);
        }
        for (t, side) in [("EDGEBOT", RectSide::Bottom), ("EDGELEFT", RectSide::Left), ("EDGETOP", RectSide::Top), ("EDGERIGHT", RectSide::Right)] {
            let i = (0..4).find(|&i| p.rect_side(i) == Some(side)).unwrap_or(0);
            edge(x, t, i, edge_of(i));
        }
    } else {
        contour(x, "BOARDCONTOURCORE", &p.segs);
        x.open("EDGES");
        let n = p.segs.len();
        x.leaf("QTY", n);
        for i in 0..n {
            edge(x, "EDGE", i, edge_of(i));
        }
        x.close("EDGES");
    }
    x.close(tag);
}

/// Половина минимального размера — порог «эта вершина на дальней стороне габарита».
const EPS_HALF: f64 = 0.5;

/// Проект Раскроя в виде строки (кодировку в windows-1251 делает [`project_xml`]).
pub fn project_string(name: &str, panels: &[Panel], sheet: impl Fn(&str) -> (f64, f64)) -> String {
    let mut x = X { s: String::new(), depth: 0 };
    x.s.push_str("<?xml version=\"1.0\" encoding=\"windows-1251\"?>\r\n");
    let _ = write!(x.s, "<PROJECT version=\"2\" subver=\"7\" name=\"{}\" code=\"\">\r\n", esc(name));
    x.depth = 1;

    let mut cores: Vec<(&str, f64)> = Vec::new();
    let mut edges: Vec<&Edge> = Vec::new();
    for p in panels {
        if !cores.iter().any(|&(m, t)| m == p.material && t == p.thickness) {
            cores.push((&p.material, p.thickness));
        }
        for e in &p.edges {
            if !edges.iter().any(|o| o.material == e.material && o.thickness == e.thickness) {
                edges.push(e);
            }
        }
    }
    x.open("MATERIALROOTNODE");
    x.open("COREBOARDMAT");
    x.leaf("QTY", cores.len());
    for &(m, t) in &cores {
        x.open("MATERIALCORE");
        material_body(&mut x, m, t, sheet(m), true);
        x.close("MATERIALCORE");
    }
    x.close("COREBOARDMAT");
    x.open("EDGEBOARDMAT");
    x.leaf("QTY", edges.len());
    for e in &edges {
        x.open("MATERIALOUTER");
        material_body(&mut x, &e.material, e.thickness, (0.0, e.width), false);
        x.close("MATERIALOUTER");
    }
    x.close("EDGEBOARDMAT");
    x.close("MATERIALROOTNODE");

    let (smp, fig): (Vec<&Panel>, Vec<&Panel>) = panels.iter().partition(|p| p.is_rect());
    x.open("BOARDROOTNODE");
    for (tag, list) in [("SMPBOARDS", &smp), ("FIGBOARDS", &fig)] {
        x.open(tag);
        x.leaf("QTY", list.len());
        for p in list.iter() {
            board(&mut x, p, sheet(&p.material));
        }
        x.close(tag);
    }
    x.qty0("BENTBOARDS");
    x.close("BOARDROOTNODE");

    x.open("BINDINGROOTNODE");
    for t in ["AXISSYM", "FASTENINGANGLES", "ARBITRARYBND", "COMPANENTRYBND"] {
        x.qty0(t);
    }
    x.close("BINDINGROOTNODE");
    x.depth = 0;
    x.s.push_str("</PROJECT>\r\n");
    x.s
}

/// Проект Раскроя в windows-1251; символы вне кодировки заменяются числовыми ссылками XML.
pub fn project_xml(name: &str, panels: &[Panel], sheet: impl Fn(&str) -> (f64, f64)) -> Vec<u8> {
    encoding_rs::WINDOWS_1251.encode(&project_string(name, panels, sheet)).0.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_like_reference() {
        assert_eq!(num(1140.00004), "1140.00004");
        assert_eq!(num(FRAC_PI_2), "1.5707963");
        assert_eq!(num(600.0), "600");
        assert_eq!(num(-0.0), "0");
    }

    #[test]
    fn cw_arc_matches_reference() {
        // Дуга из эталона «Стоевая с радиусом прав.»: центр (100.02, 976.667), от (0, 976.667) по часовой.
        let mut x = X { s: String::new(), depth: 0 };
        let c = [100.02, 976.667];
        let end = [84.487, 1075.454];
        arc(&mut x, 0, [0.0, 976.667], Some(end), c, false);
        assert!(x.s.contains("<ANGLESTART>6.2831853</ANGLESTART>"), "{}", x.s);
        assert!(x.s.contains("<ANGLEEND>7.698"), "{}", x.s);
        assert!(x.s.contains("<A30>100.02</A30>") && x.s.contains("<A31>-976.667</A31>"));
        assert!(x.s.contains("<X>\r\n") && x.s.contains("<X>0</X>"));
    }
}
