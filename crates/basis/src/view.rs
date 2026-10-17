//! Виды изделия из деталей: спереди и аксонометрия (2D-проекции габаритов).

use crate::design::{Kind, Part};

/// Деталь на виде спереди: прямоугольник в мм (X вправо, Y вверх).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrontRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub kind: Kind,
    /// Индекс в списке деталей.
    pub part: usize,
}

/// Вид спереди: проекции деталей, от дальних к ближним (рисовать по порядку).
/// Без `doors` скрыты двери и фасады ящиков.
pub fn front_view(parts: &[Part], doors: bool) -> Vec<FrontRect> {
    let mut v: Vec<(f64, FrontRect)> = parts
        .iter()
        .enumerate()
        .filter(|(_, p)| doors || !matches!(p.kind, Kind::Door | Kind::Drawer))
        .map(|(i, p)| (p.origin[2], FrontRect { x: p.origin[0], y: p.origin[1], w: p.size[0], h: p.size[1], kind: p.kind, part: i }))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.into_iter().map(|(_, r)| r).collect()
}

/// Габарит вида: ([мин x, мин y], [макс x, макс y]).
pub fn bounds(rects: &[FrontRect]) -> Option<([f64; 2], [f64; 2])> {
    rects.iter().fold(None, |acc, r| {
        let (lo, hi) = acc.unwrap_or(([f64::MAX; 2], [f64::MIN; 2]));
        Some(([lo[0].min(r.x), lo[1].min(r.y)], [hi[0].max(r.x + r.w), hi[1].max(r.y + r.h)]))
    })
}

/// Грань детали на 3D-виде: четырёхугольник в экранных мм (Y вверх).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub pts: [[f64; 2]; 4],
    pub kind: Kind,
    /// Освещённость: 1 — фронт, ближе к 1 — светлее (верх), меньше — боковины.
    pub light: f64,
    pub part: usize,
}

/// Аксонометрия «спереди-справа-сверху»: поворот на 30° вокруг вертикали и наклон 25°.
/// Грани упорядочены от дальних к ближним — рисовать по порядку.
pub fn iso_view(parts: &[Part], doors: bool) -> Vec<Face> {
    let (yaw, tilt) = (-30f64.to_radians(), 25f64.to_radians());
    let rot = |p: [f64; 3]| {
        let (x1, z1) = (p[0] * yaw.cos() + p[2] * yaw.sin(), -p[0] * yaw.sin() + p[2] * yaw.cos());
        [x1, p[1] * tilt.cos() - z1 * tilt.sin(), p[1] * tilt.sin() + z1 * tilt.cos()]
    };
    // Грани параллелепипеда: нормаль и 4 угла (индексы битов: X, Y, Z → 0 = мин, 1 = макс).
    const FACES: [([f64; 3], [usize; 4]); 6] = [
        ([0.0, 0.0, 1.0], [4, 5, 7, 6]),
        ([0.0, 0.0, -1.0], [0, 2, 3, 1]),
        ([1.0, 0.0, 0.0], [1, 3, 7, 5]),
        ([-1.0, 0.0, 0.0], [0, 4, 6, 2]),
        ([0.0, 1.0, 0.0], [2, 6, 7, 3]),
        ([0.0, -1.0, 0.0], [0, 1, 5, 4]),
    ];
    let mut boxes: Vec<IsoBox> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        if !doors && matches!(p.kind, Kind::Door | Kind::Drawer) {
            continue;
        }
        let corner = |k: usize| rot(std::array::from_fn(|a| p.origin[a] + if k >> a & 1 == 1 { p.size[a] } else { 0.0 }));
        let center = rot(std::array::from_fn(|a| p.origin[a] + p.size[a] / 2.0));
        let faces = FACES
            .iter()
            .filter(|(n, _)| rot(*n)[2] > 1e-9)
            .map(|(n, idx)| Face {
                pts: idx.map(|k| {
                    let c = corner(k);
                    [c[0], c[1]]
                }),
                kind: p.kind,
                light: if n[1] > 0.0 { 1.15 } else if n[0] != 0.0 { 0.8 } else { 1.0 },
                part: i,
            })
            .collect();
        boxes.push((center[2], faces, p.origin, std::array::from_fn(|a| p.origin[a] + p.size[a])));
    }
    paint_order(&boxes).into_iter().flat_map(|i| boxes[i].1.clone()).collect()
}

/// Деталь для 3D: (глубина центра, грани, мин. угол, макс. угол).
type IsoBox = (f64, Vec<Face>, [f64; 3], [f64; 3]);

/// Порядок рисования параллелепипедов при взгляде справа-сверху-спереди: A раньше B,
/// если A целиком левее, ниже или дальше B. Циклы (их проекции не пересекаются) разрываем по глубине центра.
fn paint_order(boxes: &[IsoBox]) -> Vec<usize> {
    let n = boxes.len();
    let behind = |a: usize, b: usize| (0..3).any(|k| boxes[a].3[k] <= boxes[b].2[k] + 1e-6);
    let before: Vec<Vec<usize>> = (0..n).map(|a| (0..n).filter(|&b| a != b && behind(a, b) && !behind(b, a)).collect()).collect();
    let mut pending = vec![0usize; n];
    before.iter().flatten().for_each(|&b| pending[b] += 1);
    let mut done = vec![false; n];
    let mut order = Vec::with_capacity(n);
    while order.len() < n {
        // Из готовых (все «задние» уже нарисованы) берём самый дальний; если готовых нет — самый дальний вообще.
        let pick = (0..n)
            .filter(|&i| !done[i])
            .min_by(|&a, &b| (pending[a] > 0).cmp(&(pending[b] > 0)).then(boxes[a].0.total_cmp(&boxes[b].0)))
            .expect("есть ненарисованные");
        done[pick] = true;
        order.push(pick);
        for &b in &before[pick] {
            pending[b] = pending[b].saturating_sub(1);
        }
    }
    order
}
