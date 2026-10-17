//! Детали (плиты, Type 4002) из дерева модели и спецификация по ним.

use crate::b3d::{File, Node, Value};
use crate::contour::{self, Seg};

/// Плита в дереве `Document/Model`.
pub const TYPE_PANEL: i64 = 4002;

/// Направление текстуры (`TexDir`): 1 — вдоль X контура, 2 — вдоль Y, нет — по умолчанию.
/// Сверено с эталонными проектами Раскроя: горизонтальные детали — угол 0, стойки — π/2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq)]
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
}

pub fn panels(file: &File) -> Extract {
    let mut out = Extract::default();
    if let Some(model) = file.section("Document").and_then(|d| d.get("Model")) {
        walk(model, &mut out);
    }
    out
}

fn walk(n: &Node, out: &mut Extract) {
    // Плиты нулевой толщины — изображения внутри фурнитуры (заглушки, крепления), в раскрой не идут.
    if n.int("Type") == Some(TYPE_PANEL) && n.f64("Thick").is_some_and(|t| t > 0.0) {
        match panel(n) {
            Some(p) => out.panels.push(p),
            None => out.skipped.push(n.str("Name").unwrap_or_default().to_string()),
        }
    }
    for c in &n.children {
        walk(c, out);
    }
}

fn panel(n: &Node) -> Option<Panel> {
    let segs = contour::parse(n.blob("Contour")?)?;
    let (lo, hi) = contour::bounds(&segs)?;
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
    Some(Panel {
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
    })
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
