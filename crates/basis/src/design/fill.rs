//! Панель «в проём» (как клавиша «~» в Базисе): по точке на виде спереди находятся ближайшие
//! границы проёма, и панель занимает его целиком за вычетом отступов. Двери и фасады — не границы.

use super::{Kind, Part, Plane};

/// Границы проёма вокруг точки и итоговая панель.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    /// Размеры контура (как у `panel.add`).
    pub size: [f64; 2],
    /// Минимальный угол габарита.
    pub position: [f64; 3],
}

fn blocks(p: &Part) -> bool {
    !matches!(p.kind, Kind::Door | Kind::Drawer)
}

/// Ближайшие границы по оси `axis` (0 — X, 1 — Y) от `at` среди деталей, чья проекция на другую ось
/// вида спереди накрывает точку. Детали, накрывающие точку по `axis`, — не границы (полка не ограничивает
/// проём по ширине, задняя стенка — ни по чему). Возвращает (ближняя слева/снизу, ближняя справа/сверху).
fn bounds(parts: &[Part], at: [f64; 2], axis: usize) -> (Option<&Part>, Option<&Part>) {
    let other = 1 - axis;
    let across = |p: &&Part| p.origin[other] <= at[other] && at[other] <= p.origin[other] + p.size[other];
    let cands: Vec<&Part> = parts.iter().filter(|p| blocks(p)).filter(across).collect();
    let lo = cands.iter().filter(|p| p.origin[axis] + p.size[axis] <= at[axis]).max_by(|a, b| (a.origin[axis] + a.size[axis]).total_cmp(&(b.origin[axis] + b.size[axis])));
    let hi = cands.iter().filter(|p| p.origin[axis] >= at[axis]).min_by(|a, b| a.origin[axis].total_cmp(&b.origin[axis]));
    (lo.copied(), hi.copied())
}

/// Общий диапазон по Z двух границ — глубина проёма.
fn depth(a: &Part, b: &Part) -> Result<(f64, f64), String> {
    let (z0, z1) = (a.origin[2].max(b.origin[2]), (a.origin[2] + a.size[2]).min(b.origin[2] + b.size[2]));
    if z1 > z0 { Ok((z0, z1)) } else { Err(format!("границы «{}» и «{}» не перекрываются по глубине", a.panel.name, b.panel.name)) }
}

fn need<'a>(b: Option<&'a Part>, side: &str, at: [f64; 2]) -> Result<&'a Part, String> {
    b.ok_or_else(|| format!("нет границы проёма {side} от точки {at:?}"))
}

/// Панель в проёме у точки `at` (X, Y на виде спереди, мм). `gaps` — отступы от границ в осях контура:
/// горизонтальная — [слева, справа, сзади, спереди], вертикальная — [сзади, спереди, снизу, сверху],
/// фронтальная — [слева, справа, снизу, сверху]. Горизонтальная ложится нижней пластью на высоту `at.y`,
/// вертикальная — левой пластью на `at.x`, фронтальная — вровень с передом проёма.
pub fn fill(parts: &[Part], plane: Plane, at: [f64; 2], thickness: f64, gaps: [f64; 4]) -> Result<Fill, String> {
    let [g0, g1, g2, g3] = gaps;
    let check = |size: [f64; 2]| {
        if size[0] > 0.0 && size[1] > 0.0 { Ok(size) } else { Err(format!("проём за вычетом отступов пуст: {size:?}")) }
    };
    match plane {
        Plane::Horizontal => {
            let (l, r) = bounds(parts, at, 0);
            let (l, r) = (need(l, "слева", at)?, need(r, "справа", at)?);
            let (z0, z1) = depth(l, r)?;
            let x0 = l.origin[0] + l.size[0];
            let size = check([r.origin[0] - x0 - g0 - g1, z1 - z0 - g2 - g3])?;
            Ok(Fill { size, position: [x0 + g0, at[1], z0 + g2] })
        }
        Plane::Vertical => {
            let (b, t) = bounds(parts, at, 1);
            let (b, t) = (need(b, "снизу", at)?, need(t, "сверху", at)?);
            let (z0, z1) = depth(b, t)?;
            let y0 = b.origin[1] + b.size[1];
            let size = check([z1 - z0 - g0 - g1, t.origin[1] - y0 - g2 - g3])?;
            Ok(Fill { size, position: [at[0], y0 + g2, z0 + g0] })
        }
        Plane::Front => {
            let (l, r) = bounds(parts, at, 0);
            let (b, t) = bounds(parts, at, 1);
            let (l, r, b, t) = (need(l, "слева", at)?, need(r, "справа", at)?, need(b, "снизу", at)?, need(t, "сверху", at)?);
            let (_, z1) = depth(l, r)?;
            let (x0, y0) = (l.origin[0] + l.size[0], b.origin[1] + b.size[1]);
            let size = check([r.origin[0] - x0 - g0 - g1, t.origin[1] - y0 - g2 - g3])?;
            Ok(Fill { size, position: [x0 + g0, y0 + g2, z1 - thickness] })
        }
    }
}
