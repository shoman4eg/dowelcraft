//! Сетка для 3D-вида: панель — тело выдавливания контура на толщину, в координатах проекта.
//! Дуги разбиваются с отклонением хорды не больше [`TOLERANCE`]; рёбра — контур обеих пластей
//! и вертикали в концах сегментов (как «Без невидимых» в Базисе: без рёбер разбиения дуг).

use crate::contour::{self, Seg};
use crate::design::{Kind, Part};

/// Наибольшее отклонение хорды от дуги, мм.
pub const TOLERANCE: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// Индекс детали в списке `parts`.
    pub part: u32,
}

#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    /// Треугольники, по три индекса.
    pub indices: Vec<u32>,
    /// Рёбра: пары точек и индекс детали.
    pub lines: Vec<([f32; 3], [f32; 3], u32)>,
    /// Назначение детали, по индексу детали.
    pub kinds: Vec<Kind>,
    /// Детали, чей контур не удалось построить: (индекс, причина).
    pub errors: Vec<(usize, String)>,
}

/// Точки сегмента от начала до конца (без конечной), дуги — с разбиением.
fn points(s: &Seg, out: &mut Vec<[f64; 2]>) {
    out.extend(contour::tessellate(s, TOLERANCE));
}

/// Замкнутый контур как многоугольник: точки и начала сегментов (там ставятся вертикальные рёбра).
#[derive(Debug, Clone)]
struct Ring {
    pts: Vec<[f64; 2]>,
    corners: Vec<usize>,
}

impl Ring {
    fn reverse(&mut self) {
        self.pts.reverse();
        // Те же точки углов в обратном порядке обхода.
        let n = self.pts.len();
        self.corners = self.corners.iter().map(|&c| n - 1 - c).collect();
    }
}

/// Контуры панели: первый — внешний (против часовой стрелки), остальные — вырезы (по часовой).
fn rings(segs: &[Seg]) -> Result<Vec<Ring>, String> {
    let mut rings: Vec<Ring> = contour::loops(segs)?
        .into_iter()
        .map(|lp| {
            let mut r = Ring { pts: Vec::new(), corners: Vec::new() };
            for (i, rev) in lp {
                r.corners.push(r.pts.len());
                points(&if rev { contour::reversed(&segs[i]) } else { segs[i] }, &mut r.pts);
            }
            r
        })
        .filter(|r| r.pts.len() >= 3)
        .collect();
    let outer = (0..rings.len()).max_by(|&a, &b| area(&rings[a].pts).abs().total_cmp(&area(&rings[b].pts).abs())).ok_or("пустой контур")?;
    rings.swap(0, outer);
    for (i, r) in rings.iter_mut().enumerate() {
        if (area(&r.pts) > 0.0) != (i == 0) {
            r.reverse();
        }
    }
    Ok(rings)
}

/// Внешний контур с вырезами — один многоугольник: каждый вырез соединён с внешним контуром
/// мостиком из видимой вершины (вершины мостика повторяются).
fn merge(rings: &[Ring]) -> Vec<[f64; 2]> {
    let mut poly = rings[0].pts.clone();
    let mut holes: Vec<&Ring> = rings[1..].iter().collect();
    // От самого правого выреза: мостики не пересекают ещё не вставленные вырезы.
    holes.sort_by(|a, b| {
        let mx = |r: &Ring| r.pts.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        mx(b).total_cmp(&mx(a))
    });
    for (k, hole) in holes.iter().enumerate() {
        let hi = (0..hole.pts.len()).max_by(|&a, &b| hole.pts[a][0].total_cmp(&hole.pts[b][0])).unwrap_or(0);
        let h = hole.pts[hi];
        let crosses = |a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]| {
            let o = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
            let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
            d1 * d2 < -1e-12 && d3 * d4 < -1e-12
        };
        let edges = |pts: &[[f64; 2]]| (0..pts.len()).map(|i| (pts[i], pts[(i + 1) % pts.len()])).collect::<Vec<_>>();
        let mut blockers = edges(&poly);
        for other in &holes[k..] {
            blockers.extend(edges(&other.pts));
        }
        let visible = |j: usize| !blockers.iter().any(|&(c, d)| crosses(h, poly[j], c, d));
        let dist = |j: usize| (poly[j][0] - h[0]).hypot(poly[j][1] - h[1]);
        // Ближайшая видимая вершина: обычно видна просто ближайшая, иначе — по возрастанию расстояния.
        let nearest = (0..poly.len()).min_by(|&a, &b| dist(a).total_cmp(&dist(b))).unwrap_or(0);
        let j = if visible(nearest) {
            nearest
        } else {
            let mut order: Vec<usize> = (0..poly.len()).collect();
            order.sort_by(|&a, &b| dist(a).total_cmp(&dist(b)));
            order.into_iter().find(|&j| visible(j)).unwrap_or(0)
        };
        let mut next = poly[..=j].to_vec();
        next.extend(hole.pts[hi..].iter().chain(&hole.pts[..=hi]));
        next.extend_from_slice(&poly[j..]);
        poly = next;
    }
    poly
}

fn area(p: &[[f64; 2]]) -> f64 {
    (0..p.len()).map(|i| {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        a[0] * b[1] - b[0] * a[1]
    }).sum::<f64>() / 2.0
}

/// Триангуляция простого многоугольника отсечением ушей. Вершины против часовой стрелки.
/// Вершину-кандидата проверяют только вогнутые вершины (только они могут лежать внутри уха),
/// обход идёт по кругу — O(n·r), где r — число вогнутых вершин.
fn triangulate(p: &[[f64; 2]]) -> Vec<[usize; 3]> {
    let n = p.len();
    if n < 3 {
        return Vec::new();
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let convex = |prev: &[usize], next: &[usize], i: usize| cross(p[prev[i]], p[i], p[next[i]]) > 1e-12;
    let mut reflex: Vec<bool> = (0..n).map(|i| !convex(&prev, &next, i)).collect();
    let mut alive = vec![true; n];
    // Сетка ячеек с вогнутыми вершинами: проверяются только ячейки под рамкой уха.
    // Вогнутая вершина может стать выпуклой, но не наоборот — устаревшие записи отсеивает `reflex[j]`.
    let (lo, hi) = p.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), q| ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])]));
    let cells = ((n as f64).sqrt().ceil() as usize).clamp(1, 256);
    let cell = |v: f64, k: usize| (((v - lo[k]) / (hi[k] - lo[k]).max(1e-9) * cells as f64) as usize).min(cells - 1);
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); cells * cells];
    for j in (0..n).filter(|&j| reflex[j]) {
        grid[cell(p[j][1], 1) * cells + cell(p[j][0], 0)].push(j);
    }
    let mut out = Vec::with_capacity(n - 2);
    let (mut left, mut i, mut stall) = (n, 0, 0);
    while left > 3 {
        let (a, c) = (prev[i], next[i]);
        let ear = !reflex[i] && {
            let (pa, pb, pc) = (p[a], p[i], p[c]);
            let (x0, x1) = (cell(pa[0].min(pb[0]).min(pc[0]), 0), cell(pa[0].max(pb[0]).max(pc[0]), 0));
            let (y0, y1) = (cell(pa[1].min(pb[1]).min(pc[1]), 1), cell(pa[1].max(pb[1]).max(pc[1]), 1));
            !(y0..=y1).flat_map(|y| (x0..=x1).map(move |x| y * cells + x)).flat_map(|g| &grid[g]).any(|&j| {
                let q = p[j];
                alive[j] && reflex[j] && j != a && j != c && j != i && q != pa && q != pb && q != pc
                    && cross(pa, pb, q) >= 0.0 && cross(pb, pc, q) >= 0.0 && cross(pc, pa, q) >= 0.0
            })
        };
        // Нет ушей за полный круг — вершина вырожденная (на одной прямой с соседями): убираем её.
        if ear || stall > left {
            if ear {
                out.push([a, i, c]);
            }
            alive[i] = false;
            next[a] = c;
            prev[c] = a;
            left -= 1;
            stall = 0;
            reflex[a] = !convex(&prev, &next, a);
            reflex[c] = !convex(&prev, &next, c);
            i = c;
        } else {
            stall += 1;
            i = next[i];
        }
    }
    if cross(p[prev[i]], p[i], p[next[i]]).abs() > 0.0 {
        out.push([prev[i], i, next[i]]);
    }
    out
}

/// Сетка деталей. Детали с `skip` (например, двери при их скрытии) пропускаются.
pub fn build(parts: &[Part], skip: impl Fn(&Part) -> bool) -> Mesh {
    let mut m = Mesh { kinds: parts.iter().map(|p| p.kind).collect(), ..Mesh::default() };
    for (pi, part) in parts.iter().enumerate() {
        let Some(f) = part.panel.frame else { continue };
        if skip(part) {
            continue;
        }
        let rings = match rings(&part.panel.segs) {
            Ok(r) => r,
            Err(e) => {
                m.errors.push((pi, e));
                continue;
            }
        };
        let pts = merge(&rings);
        let t = part.panel.thickness;
        let w = |p: [f64; 2], z: f64| f.apply([p[0], p[1], z]).map(|v| v as f32);
        let n = |v: [f64; 3]| f.dir(v).map(|v| v as f32);
        let pi = pi as u32;
        let tris = triangulate(&pts);
        // Пласти: лицевая (z = t) смотрит по +Z панели, тыльная — против.
        for (z, normal, flip) in [(t, [0.0, 0.0, 1.0], false), (0.0, [0.0, 0.0, -1.0], true)] {
            let base = m.vertices.len() as u32;
            m.vertices.extend(pts.iter().map(|&p| Vertex { pos: w(p, z), normal: n(normal), part: pi }));
            for tri in &tris {
                let [a, b, c] = tri.map(|i| base + i as u32);
                m.indices.extend(if flip { [a, c, b] } else { [a, b, c] });
            }
        }
        // Торцы: четырёхугольник на каждую хорду, нормаль наружу — справа по обходу
        // (внешний контур против часовой стрелки, вырезы — по часовой).
        for ring in &rings {
            let (pts, k) = (&ring.pts, ring.pts.len());
            for i in 0..k {
                let (a, b) = (pts[i], pts[(i + 1) % k]);
                let len = (b[0] - a[0]).hypot(b[1] - a[1]);
                if len < 1e-9 {
                    continue;
                }
                let normal = n([(b[1] - a[1]) / len, -(b[0] - a[0]) / len, 0.0]);
                let base = m.vertices.len() as u32;
                for (p, z) in [(a, 0.0), (b, 0.0), (b, t), (a, t)] {
                    m.vertices.push(Vertex { pos: w(p, z), normal, part: pi });
                }
                m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            for z in [0.0, t] {
                m.lines.extend((0..k).map(|i| (w(pts[i], z), w(pts[(i + 1) % k], z), pi)));
            }
            m.lines.extend(ring.corners.iter().map(|&c| (w(pts[c], 0.0), w(pts[c], t), pi)));
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Frame;
    use crate::panel::{Panel, Texture};

    fn part(segs: Vec<Seg>, size: (f64, f64)) -> Part {
        let panel = Panel {
            name: "т".into(),
            pos: String::new(),
            designation: String::new(),
            material: String::new(),
            thickness: 16.0,
            segs,
            size,
            texture: Texture::Default,
            edges: Vec::new(),
            frame: Some(Frame::new([100.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0])),
            holes: Vec::new(),
        };
        Part { owner: 1, id: 0, kind: Kind::Body, panel, origin: [0.0; 3], size: [0.0; 3] }
    }

    fn tri_area(m: &Mesh, normal: [f32; 3]) -> f64 {
        m.indices
            .chunks(3)
            .filter(|t| m.vertices[t[0] as usize].normal == normal)
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| m.vertices[t[i] as usize].pos.map(f64::from));
                let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
                let x = crate::geom::cross(u, v);
                (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt() / 2.0
            })
            .sum()
    }

    #[test]
    fn l_shaped_panel_is_exact_solid() {
        // Г-образная горизонтальная панель (как дно углового модуля), по часовой стрелке.
        let pts = [[984.0, 0.0], [0.0, 0.0], [0.0, 884.0], [560.0, 884.0], [560.0, 560.0], [984.0, 560.0]];
        let segs = (0..6).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % 6] }).collect();
        let m = build(&[part(segs, (984.0, 884.0))], |_| false);
        let expect = 984.0 * 560.0 + 560.0 * 324.0;
        // Горизонтальная панель: лицевая пласть смотрит вниз (−Y), тыльная — вверх.
        assert!((tri_area(&m, [0.0, -1.0, 0.0]) - expect).abs() < 1e-3);
        assert!((tri_area(&m, [0.0, 1.0, 0.0]) - expect).abs() < 1e-3);
        // 6 торцов, рёбра: 6 + 6 по пластям и 6 вертикалей.
        assert_eq!(m.lines.len(), 18);
        let ys: Vec<f32> = m.vertices.iter().map(|v| v.pos[1]).collect();
        assert!(ys.iter().all(|&y| y == 0.0 || y == -16.0));
        assert!(m.vertices.iter().all(|v| v.pos[0] >= 100.0 && v.pos[0] <= 1084.0));
    }

    #[test]
    fn arc_follows_tolerance() {
        // Полукруглый край радиусом 300: хорды не дальше 0,1 мм от дуги, вертикальные рёбра — только в концах сегментов.
        let segs = vec![
            Seg::Line { a: [600.0, 0.0], b: [0.0, 0.0] },
            Seg::Line { a: [0.0, 0.0], b: [0.0, 300.0] },
            Seg::Arc { a: [0.0, 300.0], b: [600.0, 300.0], c: [300.0, 300.0], ccw: false },
            Seg::Line { a: [600.0, 300.0], b: [600.0, 0.0] },
        ];
        let m = build(&[part(segs, (600.0, 600.0))], |_| false);
        let r = &rings(&m_segs()).unwrap()[0];
        assert_eq!(r.corners.len(), 4);
        // Точки дуги (верхняя полуокружность с концами): все на окружности, хорда даёт отклонение ≤ допуска.
        let on_arc = r.pts.iter().filter(|p| p[1] >= 300.0 - 1e-9 && ((p[0] - 300.0).hypot(p[1] - 300.0) - 300.0).abs() < 1e-9).count();
        let n = on_arc - 1;
        assert!(n > 10, "{n}");
        let chord = 2.0 * 300.0 * (std::f64::consts::PI / n as f64 / 2.0).sin();
        assert!(300.0 - (300.0f64.powi(2) - (chord / 2.0).powi(2)).sqrt() <= TOLERANCE);
        assert_eq!(m.lines.iter().filter(|l| l.0[1] != l.1[1]).count(), 4);
        let expect = 600.0 * 300.0 + std::f64::consts::PI * 300.0 * 300.0 / 2.0;
        // Хорды внутри дуги: площадь меньше не более чем на длину дуги × допуск.
        let lost = expect - tri_area(&m, [0.0, 1.0, 0.0]);
        assert!((0.0..=std::f64::consts::PI * 300.0 * TOLERANCE).contains(&lost), "{lost}");

        fn m_segs() -> Vec<Seg> {
            vec![
                Seg::Line { a: [600.0, 0.0], b: [0.0, 0.0] },
                Seg::Line { a: [0.0, 0.0], b: [0.0, 300.0] },
                Seg::Arc { a: [0.0, 300.0], b: [600.0, 300.0], c: [300.0, 300.0], ccw: false },
                Seg::Line { a: [600.0, 300.0], b: [600.0, 0.0] },
            ]
        }
    }

    /// На всех плитах «Кухни 24.04» триангуляция покрывает контур целиком: площадь треугольников
    /// одной пласти равна площади многоугольника.
    #[test]
    fn kitchen_panels_triangulate_fully() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/pairs/kuhnya-24-04/_Кухня 24.04.b3d");
        let Ok(data) = std::fs::read(&path) else { return };
        let ex = crate::panel::panels(&crate::b3d::parse(&data).unwrap());
        let mut holes = 0;
        for p in &ex.panels {
            let rs = rings(&p.segs).unwrap_or_else(|e| panic!("{}: {e}", p.name));
            holes += rs.len() - 1;
            let pts = merge(&rs);
            let tris = triangulate(&pts);
            let want: f64 = rs.iter().map(|r| area(&r.pts)).sum();
            let sum: f64 = tris.iter().map(|t| area(&[pts[t[0]], pts[t[1]], pts[t[2]]])).sum();
            assert!((sum - want).abs() < 1e-6 * want.max(1.0), "{}: {sum} ≠ {want}", p.name);
            assert!(tris.iter().all(|t| area(&[pts[t[0]], pts[t[1]], pts[t[2]]]) >= -1e-9), "{}: вывернутый треугольник", p.name);
        }
        eprintln!("вырезов: {holes}");
    }

    #[test]
    fn hole_is_cut_out() {
        // Квадрат 100×100 с круглым вырезом r = 10 в центре (окружность — отдельный контур).
        let mut segs: Vec<Seg> = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]
            .windows(2)
            .map(|w| Seg::Line { a: w[0], b: w[1] })
            .collect();
        segs.push(Seg::Line { a: [0.0, 100.0], b: [0.0, 0.0] });
        segs.push(Seg::Circle { c: [50.0, 50.0], r: 10.0 });
        let rs = rings(&segs).unwrap();
        assert_eq!(rs.len(), 2);
        assert!(area(&rs[1].pts) < 0.0, "вырез обходится по часовой");
        let pts = merge(&rs);
        let tris = triangulate(&pts);
        let sum: f64 = tris.iter().map(|t| area(&[pts[t[0]], pts[t[1]], pts[t[2]]])).sum();
        let want = 10000.0 + area(&rs[1].pts);
        assert!((sum - want).abs() < 1e-6, "{sum} ≠ {want}");
        // Ни один треугольник не лежит внутри выреза.
        assert!(tris.iter().all(|t| {
            let c = [0, 1].map(|k| (pts[t[0]][k] + pts[t[1]][k] + pts[t[2]][k]) / 3.0);
            (c[0] - 50.0).hypot(c[1] - 50.0) > 9.9
        }));
        let m = build(&[part(segs, (100.0, 100.0))], |_| false);
        // Торцы выреза смотрят к центру выреза.
        let inner: Vec<&Vertex> = m.vertices.iter().filter(|v| (v.pos[0] - 150.0).hypot(v.pos[2] - 50.0) < 10.5).collect();
        assert!(!inner.is_empty());
        assert!(inner.iter().filter(|v| v.normal[1] == 0.0).all(|v| {
            let to_center = [150.0 - v.pos[0], 50.0 - v.pos[2]];
            to_center[0] * v.normal[0] + to_center[1] * v.normal[2] > 0.0
        }));
    }
}
