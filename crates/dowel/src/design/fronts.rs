//! Двери и ящики в проём (как «Установка дверей» и «Установка ящиков» в Базисе). Проём — по точке
//! на виде спереди ([`super::fill`]). Дверь с петлями и ящики с направляющими — блоки; петли и
//! направляющие — фурнитура, их отверстия садятся в двери, стойки и короба.
//! Размеры петли — по «Петля накладная с доводчиком» из моделей Базиса (`FurnList`).

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Block, FreePanel, Furniture, Kind, Object, Part, Project, build, place, rect};
use crate::geom::Frame;
use crate::panel::{Texture, WorldHole};

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
enum Mount {
    /// Накладная: закрывает торцы стоек.
    Overlay,
    /// Полунакладная: до середины толщины стоек.
    Half,
    /// Вкладная: внутри проёма, вровень с передом.
    Inset,
}

/// Границы проёма у точки: левая, правая, нижняя, верхняя детали.
fn opening<'a>(parts: &'a [Part], at: [f64; 2]) -> Result<[&'a Part; 4], String> {
    let blocks = |p: &&Part| !matches!(p.kind, Kind::Door | Kind::Drawer);
    let side = |axis: usize, lo: bool| -> Option<&'a Part> {
        let other = 1 - axis;
        let c = parts.iter().filter(blocks).filter(|p| p.origin[other] <= at[other] && at[other] <= p.origin[other] + p.size[other]);
        if lo {
            c.filter(|p| p.origin[axis] + p.size[axis] <= at[axis]).max_by(|a, b| (a.origin[axis] + a.size[axis]).total_cmp(&(b.origin[axis] + b.size[axis])))
        } else {
            c.filter(|p| p.origin[axis] >= at[axis]).min_by(|a, b| a.origin[axis].total_cmp(&b.origin[axis]))
        }
    };
    let need = |p: Option<&'a Part>, s: &str| p.ok_or_else(|| format!("нет границы проёма {s} от точки {at:?}"));
    Ok([need(side(0, true), "слева")?, need(side(0, false), "справа")?, need(side(1, true), "снизу")?, need(side(1, false), "сверху")?])
}

/// Края фасада по оси между границами `lo` и `hi`: накладной — по внешним граням, полунакладной — по серединам,
/// вкладной — по внутренним; минус зазор.
fn span(lo: &Part, hi: &Part, axis: usize, mount: Mount, gap: f64) -> (f64, f64) {
    let (lmin, lmax, hmin, hmax) = (lo.origin[axis], lo.origin[axis] + lo.size[axis], hi.origin[axis], hi.origin[axis] + hi.size[axis]);
    let (a, b) = match mount {
        Mount::Overlay => (lmin, hmax),
        Mount::Half => ((lmin + lmax) / 2.0, (hmin + hmax) / 2.0),
        Mount::Inset => (lmax, hmin),
    };
    (a + gap, b - gap)
}

/// Проём уже закрыт: двери или ящики, чей фасад на виде спереди перекрывает новый `[x0, x1] × [y0, y1]`.
fn occupied(parts: &[Part], x: (f64, f64), y: (f64, f64)) -> Result<(), String> {
    let hit = parts.iter().filter(|p| matches!(p.kind, Kind::Door | Kind::Drawer)).find(|p| {
        p.origin[0] < x.1 - 1.0 && x.0 + 1.0 < p.origin[0] + p.size[0] && p.origin[1] < y.1 - 1.0 && y.0 + 1.0 < p.origin[1] + p.size[1]
    });
    match hit {
        Some(p) => Err(format!("в проёме уже стоит «{}» — удалите её или выберите другой проём", p.panel.name)),
        None => Ok(()),
    }
}

/// Петля: чашка ⌀35×11,5 в тыльной пласти двери, центр в 22 мм от края; планка — 2 отверстия ⌀2×12 в стойке
/// в 37 мм от её переднего края, через 32 мм. Положение — на кромке двери (`edge_x`), на высоте петли,
/// на тыльной пласти (`door_back`); `inward` — +1, если дверь от кромки идёт вправо (петли слева).
fn hinge(edge_x: f64, inward: f64, y: f64, door_back: f64, stand_face: f64, stand_front: f64) -> Furniture {
    let hole = |pos: [f64; 3], dir: [f64; 3], diameter: f64, depth: f64| WorldHole { pos, dir, diameter, depth };
    let (px, pz) = (stand_face - edge_x, stand_front - 37.0 - door_back);
    let x1 = 50.0 * inward;
    Furniture {
        id: 0,
        name: "Петля накладная с доводчиком".into(),
        kind: "Петля".into(),
        source: String::new(),
        frame: Frame { origin: [edge_x, y, door_back], ..Frame::IDENTITY },
        holes: vec![
            hole([22.0 * inward, 0.0, 0.0], [0.0, 0.0, 1.0], 35.0, 11.5),
            hole([px, 16.0, pz], [-inward, 0.0, 0.0], 2.0, 12.0),
            hole([px, -16.0, pz], [-inward, 0.0, 0.0], 2.0, 12.0),
        ],
        lo: [x1.min(0.0), -30.0, -40.0],
        hi: [x1.max(0.0), 30.0, 0.0],
    }
}

/// Число петель по высоте двери (практика производителей: до 900 — 2, до 1600 — 3, до 2000 — 4, выше — 5).
fn hinge_count(h: f64) -> usize {
    if h <= 900.0 { 2 } else if h <= 1600.0 { 3 } else if h <= 2000.0 { 4 } else { 5 }
}

/// `door.add {at, material, mount?, double?, gap?, edge?, hinges?: "left"|"right"|"none"}` → {id блока}.
pub fn door(pr: &mut Project, p: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct A {
        at: [f64; 2],
        material: String,
        #[serde(default = "overlay")]
        mount: Mount,
        #[serde(default)]
        double: bool,
        #[serde(default = "two")]
        gap: f64,
        #[serde(default)]
        edge: Option<String>,
        #[serde(default = "left")]
        hinges: String,
    }
    fn overlay() -> Mount { Mount::Overlay }
    fn two() -> f64 { 2.0 }
    fn left() -> String { "left".into() }
    let a: A = serde_json::from_value(p.clone()).map_err(|e| format!("дверь: {e}"))?;
    let parts = build(pr)?;
    let [l, r, b, t] = opening(&parts, a.at)?;
    let m = pr.material(&a.material)?.clone();
    let edge = pr.edge(&a.edge)?.cloned();
    let (x0, x1) = span(l, r, 0, a.mount, a.gap);
    let (y0, y1) = span(b, t, 1, a.mount, a.gap);
    occupied(&parts, (x0, x1), (y0, y1))?;
    let front = (l.origin[2] + l.size[2]).min(r.origin[2] + r.size[2]);
    let z = if a.mount == Mount::Inset { front - m.thickness } else { front };
    let leaves = if a.double { 2 } else { 1 };
    let w = (x1 - x0 - a.gap * (leaves - 1) as f64) / leaves as f64;
    let h = y1 - y0;
    if !(w > 50.0 && h > 50.0) {
        return Err(format!("дверь {w:.1}×{h:.1} мм — слишком мала"));
    }
    let mut next = pr.next_id();
    let mut children = Vec::new();
    for i in 0..leaves {
        let x = x0 + i as f64 * (w + a.gap);
        let panel = rect("Дверь", &m, w, h, Texture::Vertical, &[0, 1, 2, 3], edge.as_ref());
        let frame = place(&panel, [x, y0, z], [w, h, m.thickness])?;
        let id = next;
        next += 1;
        children.push(Object::Panel(FreePanel {
            id, name: "Дверь".into(), pos: String::new(), designation: String::new(), material: m.name.clone(),
            segs: panel.segs, texture: Texture::Vertical, edges: panel.edges, holes: Vec::new(), grooves: Vec::new(), kind: Kind::Door, frame,
        }));
        // Петли: у двойной двери — у каждой створки со своей стороны, иначе — по выбору.
        let side_left = match (leaves, i, a.hinges.as_str()) {
            (2, 0, _) => Some(true),
            (2, _, _) => Some(false),
            (_, _, "none") => None,
            (_, _, "right") => Some(false),
            _ => Some(true),
        };
        if let Some(left_side) = side_left {
            let (edge_x, inward, stand) = if left_side { (x, 1.0, l) } else { (x + w, -1.0, r) };
            // Планка — на внутренней пласти стойки.
            let face = if left_side { stand.origin[0] + stand.size[0] } else { stand.origin[0] };
            let stand_front = stand.origin[2] + stand.size[2];
            let n = hinge_count(h);
            for k in 0..n {
                let y = y0 + 100.0 + (h - 200.0) * k as f64 / (n - 1) as f64;
                let mut f = hinge(edge_x, inward, y, z, face, stand_front);
                f.id = next;
                next += 1;
                children.push(Object::Furniture(f));
            }
        }
    }
    let id = next;
    pr.objects.push(Object::Block(Block { id, name: if a.double { "Дверь двойная".into() } else { "Дверь".into() }, frame: Frame::IDENTITY, children }));
    Ok(json!({ "id": id }))
}

/// `drawers.add {at, material, heights: [высоты фасадов снизу вверх], mount?, gap?, depth?, bottom?, edge?}` → {id блока}.
/// Короб под шариковые направляющие (13 мм на сторону), как у ящиков корпуса.
pub fn drawers(pr: &mut Project, p: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct A {
        at: [f64; 2],
        material: String,
        heights: Vec<f64>,
        #[serde(default = "overlay")]
        mount: Mount,
        #[serde(default = "two")]
        gap: f64,
        #[serde(default)]
        depth: Option<f64>,
        #[serde(default)]
        bottom: Option<String>,
        #[serde(default)]
        edge: Option<String>,
    }
    fn overlay() -> Mount { Mount::Overlay }
    fn two() -> f64 { 2.0 }
    let a: A = serde_json::from_value(p.clone()).map_err(|e| format!("ящики: {e}"))?;
    if a.heights.is_empty() {
        return Err("heights: нужна хотя бы одна высота фасада".into());
    }
    let parts = build(pr)?;
    let [l, r, b, t] = opening(&parts, a.at)?;
    let m = pr.material(&a.material)?.clone();
    let bm = match &a.bottom { Some(n) => pr.material(n)?.clone(), None => m.clone() };
    let edge = pr.edge(&a.edge)?.cloned();
    let tt = m.thickness;
    let (inner0, inner1) = (l.origin[0] + l.size[0], r.origin[0]);
    let (z0, z1) = (l.origin[2].max(r.origin[2]), (l.origin[2] + l.size[2]).min(r.origin[2] + r.size[2]));
    let depth = a.depth.unwrap_or(((z1 - z0 - 10.0) / 50.0).floor() * 50.0);
    let (fx0, fx1) = span(l, r, 0, a.mount, a.gap);
    let (fy0, fy1) = span(b, t, 1, a.mount, a.gap);
    let total: f64 = a.heights.iter().sum::<f64>() + a.gap * (a.heights.len() - 1) as f64;
    if total > fy1 - fy0 + 1e-6 {
        return Err(format!("фасады {total} мм не помещаются в проём {:.1} мм", fy1 - fy0));
    }
    occupied(&parts, (fx0, fx1), (fy0, fy0 + total))?;
    let bw = inner1 - inner0 - 2.0 * super::RAIL_GAP;
    if !(depth >= 250.0 && depth <= z1 - z0 && bw > 2.0 * tt) {
        return Err(format!("короб {bw:.0}×{depth:.0} не строится: нужна глубина ≥ 250 мм"));
    }
    let front_z = if a.mount == Mount::Inset { z1 - m.thickness } else { z1 };
    let mut next = pr.next_id();
    let mut children = Vec::new();
    let mut add = |name: &str, kind: Kind, mat: &super::Material, sx: f64, sy: f64, tex: Texture, edged: &[usize], origin: [f64; 3], size: [f64; 3]| -> Result<(), String> {
        let panel = rect(name, mat, sx, sy, tex, edged, edge.as_ref());
        let frame = place(&panel, origin, size)?;
        children.push(Object::Panel(FreePanel {
            id: next, name: name.into(), pos: String::new(), designation: String::new(), material: mat.name.clone(),
            segs: panel.segs, texture: tex, edges: panel.edges, holes: Vec::new(), grooves: Vec::new(), kind, frame,
        }));
        next += 1;
        Ok(())
    };
    let mut y = fy0;
    let mut runners = Vec::new();
    for &hf in &a.heights {
        let fw = fx1 - fx0;
        add("Фасад ящика", Kind::Drawer, &m, fw, hf, Texture::Horizontal, &[0, 1, 2, 3], [fx0, y, front_z], [fw, hf, m.thickness])?;
        let bh = hf - 40.0;
        if bh < 60.0 {
            return Err(format!("фасад {hf} мм мал для короба (нужно ≥ 100 мм)"));
        }
        let (bx, by, bz) = (inner0 + super::RAIL_GAP, y + 20.0, front_z - depth);
        add("Боковина ящика", Kind::Body, &m, depth, bh, Texture::Horizontal, &[2], [bx, by, bz], [tt, bh, depth])?;
        add("Боковина ящика", Kind::Body, &m, depth, bh, Texture::Horizontal, &[2], [bx + bw - tt, by, bz], [tt, bh, depth])?;
        add("Передняя стенка ящика", Kind::Body, &m, bw - 2.0 * tt, bh, Texture::Horizontal, &[2], [bx + tt, by, front_z - tt], [bw - 2.0 * tt, bh, tt])?;
        add("Задняя стенка ящика", Kind::Body, &m, bw - 2.0 * tt, bh, Texture::Horizontal, &[2], [bx + tt, by, bz], [bw - 2.0 * tt, bh, tt])?;
        add("Дно ящика", Kind::Body, &bm, bw, depth, Texture::Default, &[], [bx, by - bm.thickness, bz], [bw, bm.thickness, depth])?;
        // Направляющие (пара): шурупы ⌀4×12 в стойки на 37 и 261 мм от переднего края, на 10 мм выше низа короба.
        for (stand_face, dir) in [(inner0, -1.0), (inner1, 1.0)] {
            let holes = [37.0, 261.0f64.min(depth - 20.0)]
                .map(|d| WorldHole { pos: [stand_face, by - bm.thickness + 10.0, front_z - d], dir: [dir, 0.0, 0.0], diameter: 4.0, depth: 12.0 })
                .to_vec();
            runners.push(holes);
        }
        y += hf + a.gap;
    }
    for holes in runners {
        children.push(Object::Furniture(Furniture {
            id: next, name: format!("Направляющие шариковые {depth:.0} мм (половина пары)"), kind: "Ящик".into(), source: String::new(),
            frame: Frame::IDENTITY, holes, lo: [0.0; 3], hi: [0.0; 3],
        }));
        next += 1;
    }
    let id = next;
    pr.objects.push(Object::Block(Block { id, name: "Ящики".into(), frame: Frame::IDENTITY, children }));
    Ok(json!({ "id": id }))
}
