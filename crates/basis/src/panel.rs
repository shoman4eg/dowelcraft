//! Детали (плиты, Type 4002) из дерева модели и спецификация по ним.

use crate::b3d::{File, Node, Value};
use crate::contour::{self, Seg};

/// Плита в дереве `Document/Model`.
pub const TYPE_PANEL: i64 = 4002;

/// Направление текстуры (`TexDir`): 1 — вдоль X контура, 2 — вдоль Y, нет — по умолчанию.
/// Сверено с эталонными проектами Раскроя: горизонтальные детали — угол 0, стойки — π/2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Texture {
    Default,
    Horizontal,
    Vertical,
}

/// Сторона прямоугольного габарита в осях контура.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RectSide {
    Bottom,
    Left,
    Top,
    Right,
}

/// Сторона в осях спецификации.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Стороны вдоль длины.
    L1,
    L2,
    /// Стороны вдоль ширины.
    W1,
    W2,
    /// Криволинейный или наклонный участок.
    Other,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    /// Материал в формате Базиса `Название\rАртикул`.
    pub material: String,
    pub thickness: f64,
    /// Ширина ленты (`Width`), 0 — не задана.
    pub width: f64,
    pub allowance: f64,
    /// `ClipPanel`: панель подрезается на толщину кромки.
    pub clip: bool,
    /// Индекс сегмента контура (`Elem`).
    pub segment: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub name: String,
    /// Номер позиции (`ArtPos`).
    pub pos: String,
    /// Обозначение (`Des`).
    pub designation: String,
    pub material: String,
    pub thickness: f64,
    /// Контур, сдвинутый так, что габарит начинается в (0, 0).
    pub segs: Vec<Seg>,
    /// Габарит контура по X и Y, мм.
    pub size: (f64, f64),
    pub texture: Texture,
    pub edges: Vec<Edge>,
    /// Габарит в пространстве модели ([мин], [макс]), мм: X вправо, Y вверх, Z к зрителю.
    /// Есть у плит из моделей Базиса; у своих проектов положение хранит `design::Part`.
    pub bounds: Option<([f64; 3], [f64; 3])>,
    /// Присадка: отверстия крепежа, попавшие в эту плиту.
    pub holes: Vec<Hole>,
}

/// Отверстие в координатах плиты: X, Y — как у контура (от 0), Z — от тыльной пласти (0) к лицевой (толщина).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hole {
    pub pos: [f64; 3],
    /// Единичное направление сверления.
    pub dir: [f64; 3],
    pub diameter: f64,
    pub depth: f64,
}

const EPS: f64 = 0.01;

impl Panel {
    /// Сторона габарита, на которой лежит отрезок контура.
    pub fn rect_side(&self, seg: usize) -> Option<RectSide> {
        let Seg::Line { a, b } = *self.segs.get(seg)? else { return None };
        let (sx, sy) = self.size;
        let on = |v: f64, at: f64| (v - at).abs() < EPS;
        if on(a[1], b[1]) && on(a[1], 0.0) {
            Some(RectSide::Bottom)
        } else if on(a[1], b[1]) && on(a[1], sy) {
            Some(RectSide::Top)
        } else if on(a[0], b[0]) && on(a[0], 0.0) {
            Some(RectSide::Left)
        } else if on(a[0], b[0]) && on(a[0], sx) {
            Some(RectSide::Right)
        } else {
            None
        }
    }

    /// Контур — прямоугольник из четырёх отрезков по габариту.
    pub fn is_rect(&self) -> bool {
        self.segs.len() == 4 && (0..4).all(|i| self.rect_side(i).is_some())
    }

    /// Длина спецификации идёт по Y контура: текстура вертикальная, а без текстуры — Y длиннее.
    fn along_y(&self) -> bool {
        match self.texture {
            Texture::Vertical => true,
            Texture::Horizontal => false,
            Texture::Default => self.size.1 > self.size.0,
        }
    }

    pub fn grain(&self) -> bool {
        self.texture != Texture::Default
    }

    pub fn side(&self, e: &Edge) -> Side {
        let r = self.rect_side(e.segment);
        match (r, self.along_y()) {
            (Some(RectSide::Bottom), false) | (Some(RectSide::Left), true) => Side::L1,
            (Some(RectSide::Top), false) | (Some(RectSide::Right), true) => Side::L2,
            (Some(RectSide::Left), false) | (Some(RectSide::Bottom), true) => Side::W1,
            (Some(RectSide::Right), false) | (Some(RectSide::Top), true) => Side::W2,
            (None, _) => Side::Other,
        }
    }

    fn trim(&self, side: RectSide) -> f64 {
        // Сторона может состоять из нескольких сегментов — берём самую толстую кромку.
        self.edges
            .iter()
            .filter(|e| self.rect_side(e.segment) == Some(side))
            .map(|e| if e.clip { e.thickness } else { 0.0 } - e.allowance)
            .fold(0.0, f64::max)
    }

    /// Заготовка в осях контура: габарит минус толщина подрезающих (`ClipPanel`) кромок плюс припуск.
    /// shortcut: кромка на криволинейных участках не учитывается — у фигурных деталей Раскрой даёт чуть меньший размер.
    pub fn blank(&self) -> (f64, f64) {
        (
            self.size.0 - self.trim(RectSide::Left) - self.trim(RectSide::Right),
            self.size.1 - self.trim(RectSide::Bottom) - self.trim(RectSide::Top),
        )
    }

    /// Пильный размер в осях спецификации: (длина, ширина).
    pub fn cut_size(&self) -> (f64, f64) {
        let (x, y) = self.blank();
        if self.along_y() { (y, x) } else { (x, y) }
    }
}

#[derive(Debug, Default)]
pub struct Extract {
    pub panels: Vec<Panel>,
    /// Плиты, чей контур не удалось разобрать (имя).
    pub skipped: Vec<String>,
    /// Отверстия крепежа, не попавшие ни в одну плиту.
    pub holes_lost: usize,
}

/// Наибольший зазор между началом отверстия и плитой, мм (толщина планки петли, навеса).
const GAP: f64 = 5.0;

pub fn panels(file: &File) -> Extract {
    let mut out = Extract::default();
    let mut frames = Vec::new();
    if let Some(model) = file.section("Document").and_then(|d| d.get("Model")) {
        walk(model, &mut out, &mut frames, &IDENTITY);
        out.holes_lost = drill(file, model, &mut out.panels, &frames);
    }
    out
}

/// Отверстия крепежа → плиты. Описание крепежа (`Document/FurnList`, по `FastID`) хранит отверстия
/// в его координатах; экземпляр (Type 3001) ставит их в модель. Отверстие достаётся плите, в которую
/// луч сверления входит не дальше `GAP` от начала (шуруп проходит сквозь планку петли, навеса и т.п.);
/// точка входа становится началом отверстия. Отдельные отверстия (Type 3000) — так же.
/// shortcut: проверка «внутри плиты» по габариту контура — у фигурных плит возможны лишние попадания; эталона присадки с парной моделью нет.
fn drill(file: &File, model: &Node, panels: &mut [Panel], frames: &[Xform]) -> usize {
    let defs: Vec<(i64, &Node)> = file
        .section("Document")
        .and_then(|d| d.get("FurnList"))
        .map(|l| l.children.iter().filter_map(|f| Some((f.int("FastID")?, f))).collect())
        .unwrap_or_default();
    let mut world: Vec<([f64; 3], [f64; 3], f64, f64)> = Vec::new();
    fn holes(n: &Node) -> impl Iterator<Item = ([f64; 3], [f64; 3], f64, f64)> + '_ {
        n.children.iter().filter(|h| h.name == "Hole").map(|h| {
            let g = |k: &str| h.f64(k).unwrap_or(0.0);
            ([g("X"), g("Y"), g("Z")], [g("DirX"), g("DirY"), g("DirZ")], 2.0 * g("Radius"), g("Depth"))
        })
    }
    fn collect(n: &Node, parent: &Xform, defs: &[(i64, &Node)], out: &mut Vec<([f64; 3], [f64; 3], f64, f64)>) {
        let xf = if n.get("Type").is_some() { parent.then(n) } else { *parent };
        let mut put = |p: [f64; 3], d: [f64; 3], dia: f64, depth: f64| {
            let r = &xf.r;
            out.push((xf.apply(p), std::array::from_fn(|i| r[i][0] * d[0] + r[i][1] * d[1] + r[i][2] * d[2]), dia, depth));
        };
        match n.int("Type") {
            Some(3001) => {
                if let Some((_, def)) = n.int("FastID").and_then(|id| defs.iter().find(|(f, _)| *f == id))
                    && let Some(hs) = def.get("Holes")
                {
                    holes(hs).for_each(|(p, d, dia, depth)| put(p, d, dia, depth));
                }
            }
            Some(3000) => {
                let g = |k: &str| n.f64(k).unwrap_or(0.0);
                put([g("X"), g("Y"), g("Z")], [g("DirX"), g("DirY"), g("DirZ")], 2.0 * g("Radius"), g("Depth"));
            }
            _ => {}
        }
        for c in &n.children {
            collect(c, &xf, defs, out);
        }
    }
    collect(model, &IDENTITY, &defs, &mut world);

    let mut lost = 0;
    'hole: for (p, d, diameter, depth) in world {
        let inv = |f: &Xform, v: [f64; 3]| -> [f64; 3] { std::array::from_fn(|j| (0..3).map(|i| f.r[i][j] * v[i]).sum()) };
        for (panel, f) in panels.iter_mut().zip(frames) {
            let Some((lo, hi)) = panel.bounds else { continue };
            if (0..3).any(|k| p[k] < lo[k] - 1.0 || p[k] > hi[k] + 1.0) {
                continue;
            }
            let l = inv(f, std::array::from_fn(|k| p[k] - f.t[k]));
            let ld = inv(f, d);
            let hi = [panel.size.0, panel.size.1, panel.thickness];
            // Вход луча в габарит плиты (метод плит): параметр входа и выхода по каждой оси.
            let (mut t0, mut t1) = (f64::MIN, f64::MAX);
            for k in 0..3 {
                if ld[k].abs() < 1e-9 {
                    if l[k] < -0.5 || l[k] > hi[k] + 0.5 {
                        t1 = f64::MIN;
                    }
                } else {
                    let (a, b) = ((0.0 - l[k]) / ld[k], (hi[k] - l[k]) / ld[k]);
                    t0 = t0.max(a.min(b));
                    t1 = t1.min(a.max(b));
                }
            }
            if (-0.5..=GAP).contains(&t0) && t1 - t0 > 0.5 {
                let pos = std::array::from_fn(|k| l[k] + ld[k] * t0.max(0.0));
                panel.holes.push(Hole { pos, dir: ld, diameter, depth: (depth - t0.max(0.0)).max(0.0) });
                continue 'hole;
            }
        }
        lost += 1;
    }
    lost
}

/// Преобразование «локальные → мировые» координаты: поворот и смещение.
#[derive(Clone, Copy)]
struct Xform {
    r: [[f64; 3]; 3],
    t: [f64; 3],
}

const IDENTITY: Xform = Xform { r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] };

impl Xform {
    fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let r = &self.r;
        std::array::from_fn(|i| r[i][0] * p[0] + r[i][1] * p[1] + r[i][2] * p[2] + self.t[i])
    }

    /// `Trans` объекта: X, Y, Z и кватернион Rx, Ry, Rz, Rw относительно родителя.
    fn then(&self, n: &Node) -> Xform {
        let Some(tr) = n.get("Trans") else { return *self };
        let g = |k: &str, d: f64| tr.f64(k).unwrap_or(d);
        let (x, y, z, w) = (g("Rx", 0.0), g("Ry", 0.0), g("Rz", 0.0), g("Rw", 1.0));
        let q = [
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
            [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
            [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
        ];
        let r = std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| self.r[i][k] * q[k][j]).sum()));
        let local = [g("X", 0.0), g("Y", 0.0), g("Z", 0.0)];
        Xform { r, t: self.apply(local) }
    }
}

fn walk(n: &Node, out: &mut Extract, frames: &mut Vec<Xform>, parent: &Xform) {
    let xf = if n.get("Type").is_some() { parent.then(n) } else { *parent };
    // Плиты нулевой толщины — изображения внутри фурнитуры (заглушки, крепления), в раскрой не идут.
    if n.int("Type") == Some(TYPE_PANEL) && n.f64("Thick").is_some_and(|t| t > 0.0) {
        match panel(n, &xf) {
            Some((p, frame)) => {
                out.panels.push(p);
                frames.push(frame);
            }
            None => out.skipped.push(n.str("Name").unwrap_or_default().to_string()),
        }
    }
    for c in &n.children {
        walk(c, out, frames, &xf);
    }
}

/// Плита и её система координат (с началом в углу габарита контура, как у `segs`).
fn panel(n: &Node, xf: &Xform) -> Option<(Panel, Xform)> {
    let segs = contour::parse(n.blob("Contour")?)?;
    let (lo, hi) = contour::bounds(&segs)?;
    // Контур лежит в плоскости XY плиты, толщина откладывается в +Z (сверено по шкафу: полки встают между боковинами).
    let thick = n.f64("Thick").unwrap_or(0.0);
    let corners = (0..8).map(|i| xf.apply([if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { 0.0 } else { thick }]));
    let bounds = corners.fold(([f64::MAX; 3], [f64::MIN; 3]), |(a, b), p| (std::array::from_fn(|k| a[k].min(p[k])), std::array::from_fn(|k| b[k].max(p[k]))));
    let sh = |p: [f64; 2]| [p[0] - lo[0], p[1] - lo[1]];
    let segs = segs
        .into_iter()
        .map(|s| match s {
            Seg::Line { a, b } => Seg::Line { a: sh(a), b: sh(b) },
            Seg::Arc { a, b, c, ccw } => Seg::Arc { a: sh(a), b: sh(b), c: sh(c), ccw },
            Seg::Circle { c, r } => Seg::Circle { c: sh(c), r },
        })
        .collect();
    let edges = n
        .get("Butts")
        .map(|b| &b.children[..])
        .unwrap_or_default()
        .iter()
        .filter_map(|b| {
            Some(Edge {
                material: b.str("Mat").unwrap_or_default().to_string(),
                thickness: b.f64("Thick").unwrap_or(0.0),
                width: b.f64("Width").unwrap_or(0.0),
                allowance: b.f64("Allowance").unwrap_or(0.0),
                clip: matches!(b.get("Clip").map(|c| &c.value), Some(Value::True)),
                segment: b.int("Elem")? as usize,
            })
        })
        .collect();
    let frame = Xform { r: xf.r, t: xf.apply([lo[0], lo[1], 0.0]) };
    let panel = Panel {
        name: n.str("Name").unwrap_or_default().to_string(),
        pos: n.str("ArtPos").unwrap_or_default().to_string(),
        designation: n.str("Des").unwrap_or_default().to_string(),
        material: n.str("Mat").unwrap_or_default().to_string(),
        thickness: n.f64("Thick").unwrap_or(0.0),
        segs,
        size: (hi[0] - lo[0], hi[1] - lo[1]),
        texture: match n.int("TexDir") {
            Some(1) => Texture::Horizontal,
            Some(2) => Texture::Vertical,
            _ => Texture::Default,
        },
        edges,
        bounds: Some(bounds),
        holes: Vec::new(),
    };
    Some((panel, frame))
}

/// Строка спецификации: одинаковые детали сведены в одну позицию.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecRow {
    pub panel: Panel,
    pub count: usize,
}

/// Группировка по материалу, номеру позиции, имени, размерам и кромке.
pub fn spec(panels: &[Panel]) -> Vec<SpecRow> {
    let key = |p: &Panel| {
        let (l, w) = p.cut_size();
        let edges: Vec<_> = p.edges.iter().map(|e| (format!("{:?}", p.side(e)), e.material.clone(), mm(e.thickness))).collect();
        (p.material.clone(), mm(p.thickness), p.pos.clone(), p.name.clone(), mm(l), mm(w), p.grain(), edges)
    };
    let mut rows: Vec<SpecRow> = Vec::new();
    for p in panels {
        match rows.iter_mut().find(|r| key(&r.panel) == key(p)) {
            Some(r) => r.count += 1,
            None => rows.push(SpecRow { panel: p.clone(), count: 1 }),
        }
    }
    rows.sort_by(|a, b| {
        (&a.panel.material, pos_num(&a.panel.pos), &a.panel.name).cmp(&(&b.panel.material, pos_num(&b.panel.pos), &b.panel.name))
    });
    rows
}

/// Фурнитура модели Базиса: (наименование `Название\rАртикул`, количество), по имени.
/// Считаются крепёж (3001) и покупные изделия (4001, 4004) — покупное целиком, без содержимого.
/// Объекты с флагом `BazisFlags & 2` и всё внутри них в смету не идут — сверено со сметой
/// `reference/pairs/sanuzly` (так исключаются, например, чашки петель внутри блоков петель).
pub fn hardware(file: &File) -> Vec<(String, usize)> {
    fn walk(n: &Node, out: &mut Vec<(String, usize)>, excluded: bool) {
        let ty = n.int("Type");
        let excluded = excluded || (ty.is_some() && n.int("BazisFlags").is_some_and(|f| f & 2 != 0));
        if matches!(ty, Some(3001 | 4001 | 4004)) && !excluded {
            let name = n.str("Name").unwrap_or("Без имени").to_string();
            match out.iter_mut().find(|(m, _)| *m == name) {
                Some(row) => row.1 += 1,
                None => out.push((name, 1)),
            }
            if ty != Some(3001) {
                return;
            }
        }
        for c in &n.children {
            walk(c, out, excluded);
        }
    }
    let mut out = Vec::new();
    if let Some(model) = file.section("Document").and_then(|d| d.get("Model")) {
        walk(model, &mut out, false);
    }
    out.sort();
    out
}

/// Строка расчёта материалов, как в смете Базиса.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialRow {
    /// Материал в формате `Название\rАртикул`.
    pub material: String,
    /// «м2» для плит, «м» для кромки.
    pub unit: &'static str,
    pub coef: f64,
    pub qty: f64,
}

/// Расход материалов: плиты — сумма габаритов (м²), кромка — длина кромкованных сегментов (м);
/// умножается на коэффициент материала и округляется вверх: м² до 0,1, м до целого.
/// Сверено со сметой Базиса `reference/pairs/sanuzly`.
pub fn materials(panels: &[Panel], coef: impl Fn(&str) -> f64) -> Vec<MaterialRow> {
    let mut sums: Vec<(String, &'static str, f64)> = Vec::new();
    let mut add = |m: &str, unit: &'static str, v: f64| match sums.iter_mut().find(|(n, u, _)| n == m && *u == unit) {
        Some(row) => row.2 += v,
        None => sums.push((m.to_string(), unit, v)),
    };
    for p in panels {
        add(&p.material, "м2", p.size.0 * p.size.1 / 1e6);
        for e in p.edges.iter().filter(|e| e.thickness > 0.0) {
            add(&e.material, "м", p.segs.get(e.segment).map_or(0.0, contour::length) / 1e3);
        }
    }
    let mut rows: Vec<MaterialRow> = sums
        .into_iter()
        .map(|(material, unit, v)| {
            let k = coef(&material);
            // Отсекаем шум float перед округлением вверх: 2.0000000001 → 2.0.
            let up = |x: f64, step: f64| ((x / step - 1e-9).ceil() * step * 1e6).round() / 1e6;
            let qty = if unit == "м" { up(v * k, 1.0) } else { up(v * k, 0.1) };
            MaterialRow { material, unit, coef: k, qty }
        })
        .collect();
    rows.sort_by(|a, b| a.material.cmp(&b.material));
    rows
}

/// Сравнение размеров с точностью 0,01 мм (в моделях шум вида 1868.0000000001).
fn mm(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

fn pos_num(s: &str) -> (u64, String) {
    (s.trim().parse().unwrap_or(u64::MAX), s.to_string())
}

/// Название материала без артикула.
pub fn material_name(m: &str) -> &str {
    m.split('\r').next().unwrap_or(m)
}

/// Значение узла — только для отладочного вывода дерева.
pub fn describe(v: &Value) -> String {
    match v {
        Value::Obj => String::new(),
        Value::Blob(b) => format!("<{} байт>", b.len()),
        Value::Str(s) => format!("{s:?}"),
        v => format!("{v:?}"),
    }
}
