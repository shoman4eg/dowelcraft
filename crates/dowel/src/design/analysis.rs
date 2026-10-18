//! «Анализ модели» (гл. 3.4 руководства Базиса): пересечения панелей, крепёж и фурнитура без панели,
//! деталь больше листа. Каждая проблема — с деталями, к которым относится (их можно выделить).

use serde::Serialize;

use super::{Part, Project, build, furniture_lost_holes, hanging_fasteners};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Problem {
    /// Вид: «Пересечение», «Крепёж», «Фурнитура», «Размер».
    pub kind: &'static str,
    pub message: String,
    /// Индексы деталей в списке `build`.
    pub parts: Vec<usize>,
}

/// Объём пересечения габаритов двух деталей, мм³ (0 — касаются или не пересекаются).
fn overlap(a: &Part, b: &Part) -> Option<([f64; 3], [f64; 3])> {
    let lo: [f64; 3] = std::array::from_fn(|k| a.origin[k].max(b.origin[k]));
    let hi: [f64; 3] = std::array::from_fn(|k| (a.origin[k] + a.size[k]).min(b.origin[k] + b.size[k]));
    // Допуск 0,01 мм: касание по грани — не пересечение.
    (0..3).all(|k| hi[k] - lo[k] > 0.01).then_some((lo, hi))
}

/// Точка проекта внутри тела детали (по её контуру и толщине).
fn inside(p: &Part, w: [f64; 3]) -> bool {
    let Some(f) = p.panel.frame else { return false };
    let l = f.local(w);
    if l[2] < -1e-6 || l[2] > p.panel.thickness + 1e-6 {
        return false;
    }
    // Точка в многоугольнике контура (дуги — хордами).
    let Ok(loops) = crate::contour::loops(&p.panel.segs) else { return false };
    let mut crossings = 0;
    for lp in loops {
        let pts: Vec<[f64; 2]> = lp
            .into_iter()
            .flat_map(|(i, rev)| crate::contour::tessellate(&if rev { crate::contour::reversed(&p.panel.segs[i]) } else { p.panel.segs[i] }, 0.1))
            .collect();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a[1] > l[1]) != (b[1] > l[1]) && l[0] < a[0] + (l[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                crossings += 1;
            }
        }
    }
    crossings % 2 == 1
}

pub fn analyze(p: &Project) -> Result<Vec<Problem>, String> {
    let parts = build(p)?;
    let mut out = Vec::new();
    for i in 0..parts.len() {
        for j in i + 1..parts.len() {
            let Some((lo, hi)) = overlap(&parts[i], &parts[j]) else { continue };
            // Деталь в пазу другой (задняя стенка в пазу боковины) — не пересечение: вся общая область в пазу.
            let corners: Vec<[f64; 3]> = (0..8).map(|c| std::array::from_fn(|k| if c >> k & 1 == 0 { lo[k] } else { hi[k] })).collect();
            let in_groove = |p: &Part| p.panel.frame.is_some_and(|f| p.panel.grooves.iter().any(|g| corners.iter().all(|&c| g.contains(f.local(c), 0.01))));
            if in_groove(&parts[i]) || in_groove(&parts[j]) {
                continue;
            }
            // У фигурных деталей габарит больше тела: пересечение — если центр общей области внутри обеих.
            let c: [f64; 3] = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
            let figured = !parts[i].panel.is_rect() || !parts[j].panel.is_rect();
            if figured && !(inside(&parts[i], c) && inside(&parts[j], c)) {
                continue;
            }
            let d: Vec<String> = (0..3).map(|k| format!("{:.1}", hi[k] - lo[k])).collect();
            out.push(Problem {
                kind: "Пересечение",
                message: format!("«{} {}» и «{} {}» пересекаются на {} мм", parts[i].panel.pos, parts[i].panel.name, parts[j].panel.pos, parts[j].panel.name, d.join(" × ")),
                parts: vec![i, j],
            });
        }
    }
    for (_, msg) in hanging_fasteners(p) {
        out.push(Problem { kind: "Крепёж", message: msg, parts: Vec::new() });
    }
    for (name, n) in furniture_lost_holes(p) {
        out.push(Problem { kind: "Фурнитура", message: format!("«{name}»: {n} отверстий не попали ни в одну панель"), parts: Vec::new() });
    }
    for (i, part) in parts.iter().enumerate() {
        let name = crate::panel::material_name(&part.panel.material);
        let Some(m) = p.materials.iter().find(|m| m.name == name && m.thickness == part.panel.thickness) else { continue };
        let trim = m.trim.unwrap_or(10.0);
        let (sl, sw) = (m.sheet.0 - 2.0 * trim, m.sheet.1 - 2.0 * trim);
        let (l, w) = part.panel.cut_size();
        // С текстурой деталь не поворачивают: длина — вдоль длины листа.
        let fits = if part.panel.grain() { l <= sl && w <= sw } else { (l <= sl && w <= sw) || (w <= sl && l <= sw) };
        if !fits {
            out.push(Problem {
                kind: "Размер",
                message: format!("«{} {}» {l:.1}×{w:.1} не помещается на лист {}×{} (обрезка {trim} мм)", part.panel.pos, part.panel.name, m.sheet.0, m.sheet.1),
                parts: vec![i],
            });
        }
    }
    Ok(out)
}
