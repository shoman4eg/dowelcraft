//! Крепёж и присадка: справочник крепежа, стыки панелей, расстановка и отверстия с координатами.
//!
//! Стык — торец панели B касается пласти панели A (схема «пласть – торец», табл. 28.1 руководства
//! Базиса). У стыка своя система координат: начало — на плоскости касания, посередине толщины B;
//! X — внутрь B, Y — по нормали B (к её лицевой пласти), Z — вдоль стыка. Отверстия крепежа заданы
//! в этой системе и зависят от толщин: координата = c + t·(коэф. толщины A) + tb·(коэф. толщины B).
//! Размеры справочника сверены с крепежом из моделей Базиса (`Document/FurnList`).

use serde::{Deserialize, Serialize};

use crate::design::Part;
use crate::panel::Hole;

/// Отверстие крепежа в системе стыка.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HoleDef {
    /// В какую панель: "a" — с пластью, "b" — с торцом.
    pub panel: char,
    pub at: [f64; 3],
    #[serde(default)]
    pub at_t: [f64; 3],
    #[serde(default)]
    pub at_tb: [f64; 3],
    /// Направление сверления.
    pub dir: [f64; 3],
    pub diameter: f64,
    pub depth: f64,
    /// Добавка к глубине на толщину A (сквозное через A — `depth: 0, depth_t: 1`).
    #[serde(default)]
    pub depth_t: f64,
}

/// Крепёж справочника.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Def {
    /// Название `Наименование\rАртикул` — как в смете Базиса.
    pub name: String,
    pub holes: Vec<HoleDef>,
    /// Отступ крайних точек от концов стыка, мм.
    pub offset: f64,
    /// Наибольший шаг между точками, мм.
    pub step: f64,
    /// Наименьшее и наибольшее число точек на стык.
    pub min: usize,
    pub max: usize,
    /// Что покупается на одну точку, если крепёж составной (эксцентрик + шкант); пусто — сам крепёж, 1 шт.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<(String, usize)>,
}

fn h(panel: char, at: [f64; 3], dir: [f64; 3], diameter: f64, depth: f64) -> HoleDef {
    HoleDef { panel, at, at_t: [0.0; 3], at_tb: [0.0; 3], dir, diameter, depth, depth_t: 0.0 }
}

/// Справочник по умолчанию.
pub fn library() -> Vec<Def> {
    let (x, nx, ny) = ([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]);
    vec![
        Def {
            name: "Конфирмат 7х50".into(),
            holes: vec![
                // Сквозное ⌀8 через пласть A от её наружной стороны; ⌀5 в торец B: 50 − t + 3 мм.
                HoleDef { at_t: [-1.0, 0.0, 0.0], depth_t: 1.0, ..h('a', [0.0; 3], x, 8.0, 0.0) },
                HoleDef { depth_t: -1.0, ..h('b', [0.0; 3], x, 5.0, 53.0) },
            ],
            offset: 50.0,
            step: 500.0,
            min: 2,
            max: 20,
            items: Vec::new(),
        },
        Def {
            name: "Эксцентрик Minifix 15".into(),
            holes: vec![
                h('b', [0.0; 3], x, 8.0, 34.0),
                // Чашка ⌀15×13 в пласти B в 34 мм от стыка, со стороны её лицевой пласти.
                HoleDef { at_tb: [0.0, 0.5, 0.0], ..h('b', [34.0, 0.0, 0.0], ny, 15.0, 13.0) },
                h('a', [0.0; 3], nx, 5.0, 13.0),
            ],
            offset: 50.0,
            step: 500.0,
            min: 2,
            max: 20,
            items: Vec::new(),
        },
        Def {
            name: "Шкант 8х30".into(),
            holes: vec![h('b', [0.0; 3], x, 8.0, 20.0), h('a', [0.0; 3], nx, 8.0, 13.0)],
            offset: 32.0,
            step: 300.0,
            min: 2,
            max: 30,
            items: Vec::new(),
        },
        Def {
            name: "Полкодержатель 5".into(),
            // Под полкой: центр на 2,5 мм ниже её нижней пласти, ⌀5×9 в пласть A.
            holes: vec![HoleDef { at_tb: [0.0, -0.5, 0.0], ..h('a', [0.0, -2.5, 0.0], nx, 5.0, 9.0) }],
            offset: 50.0,
            step: 10_000.0,
            min: 2,
            max: 2,
            items: Vec::new(),
        },
        Def {
            name: "Minifix 15 + шкант 8х30".into(),
            // В каждой точке — эксцентрик, рядом в 32 мм ближе к середине стыка — шкант.
            holes: vec![
                h('b', [0.0; 3], x, 8.0, 34.0),
                HoleDef { at_tb: [0.0, 0.5, 0.0], ..h('b', [34.0, 0.0, 0.0], ny, 15.0, 13.0) },
                h('a', [0.0; 3], nx, 5.0, 13.0),
                h('b', [0.0, 0.0, 32.0], x, 8.0, 20.0),
                h('a', [0.0, 0.0, 32.0], nx, 8.0, 13.0),
            ],
            offset: 50.0,
            step: 500.0,
            min: 2,
            max: 20,
            items: vec![("Эксцентрик Minifix 15".into(), 1), ("Шкант 8х30".into(), 1)],
        },
    ]
}

/// Стык: торец панели `b` на пласти панели `a`.
#[derive(Clone, Debug, PartialEq)]
pub struct Joint {
    pub a: usize,
    pub b: usize,
    /// Начало стыка (в системе стыка Z = 0) и оси X, Y, Z в координатах проекта.
    pub origin: [f64; 3],
    pub axes: [[f64; 3]; 3],
    pub length: f64,
}

const EPS: f64 = 1e-6;

fn thick_axis(p: &Part) -> Option<usize> {
    let f = p.panel.frame?;
    (0..3).find(|&k| f.z[k].abs() > 1.0 - 1e-9)
}

/// Стык двух деталей, если торец `b` касается пласти `a`. Только для деталей вдоль осей проекта.
pub fn joint(parts: &[Part], a: usize, b: usize) -> Option<Joint> {
    let (pa, pb) = (&parts[a], &parts[b]);
    let (ka, kb) = (thick_axis(pa)?, thick_axis(pb)?);
    let (alo, ahi) = (pa.origin, std::array::from_fn::<f64, 3, _>(|k| pa.origin[k] + pa.size[k]));
    let (blo, bhi) = (pb.origin, std::array::from_fn::<f64, 3, _>(|k| pb.origin[k] + pb.size[k]));
    let k = ka;
    if kb == k {
        return None;
    }
    let sign = if (blo[k] - ahi[k]).abs() < EPS {
        1.0
    } else if (alo[k] - bhi[k]).abs() < EPS {
        -1.0
    } else {
        return None;
    };
    let m = kb;
    let j = 3 - k - m;
    // Торец B целиком на пласти A по толщине B и с перекрытием по длине стыка.
    let (j0, j1) = (alo[j].max(blo[j]), ahi[j].min(bhi[j]));
    if j1 - j0 <= EPS || blo[m] < alo[m] - EPS || bhi[m] > ahi[m] + EPS {
        return None;
    }
    let mut origin = [0.0; 3];
    origin[k] = if sign > 0.0 { ahi[k] } else { alo[k] };
    origin[m] = (blo[m] + bhi[m]) / 2.0;
    origin[j] = j0;
    let unit = |i: usize, s: f64| { let mut v = [0.0; 3]; v[i] = s; v };
    let ny = pb.panel.frame.map_or(1.0, |f| f.z[m].signum());
    // Z — вдоль стыка в сторону роста координаты (точки отсчитываются от его начала); тройка может
    // быть и левой — для перевода координат это не важно.
    let (xa, ya, za) = (unit(k, sign), unit(m, ny), unit(j, 1.0));
    Some(Joint { a, b, origin, axes: [xa, ya, za], length: j1 - j0 })
}

/// Точки крепежа вдоль стыка длиной `len`: отступ от концов, шаг не больше `step`.
pub fn positions(def: &Def, len: f64) -> Vec<f64> {
    let span = len - 2.0 * def.offset;
    if span <= 0.0 {
        return vec![len / 2.0];
    }
    let n = (((span / def.step).ceil() as usize) + 1).clamp(def.min.max(2), def.max.max(2));
    (0..n).map(|i| def.offset + span * i as f64 / (n - 1) as f64).collect()
}

/// Отверстия крепежа `def` в точке `s` стыка: (индекс детали, отверстие в координатах детали).
/// Ошибка — если отверстие выходит за деталь (глубже толщины, мимо контура).
pub fn holes(def: &Def, parts: &[Part], jt: &Joint, s: f64) -> Result<Vec<(usize, Hole)>, String> {
    let (t, tb) = (parts[jt.a].panel.thickness, parts[jt.b].panel.thickness);
    let [x, y, z] = jt.axes;
    let world = |l: [f64; 3]| -> [f64; 3] { std::array::from_fn(|k| jt.origin[k] + x[k] * l[0] + y[k] * l[1] + z[k] * (l[2] + s)) };
    let wdir = |d: [f64; 3]| -> [f64; 3] { std::array::from_fn(|k| x[k] * d[0] + y[k] * d[1] + z[k] * d[2]) };
    // Смещение вдоль стыка — к его середине: у точек второй половины зеркально.
    let toward = if s > jt.length / 2.0 { -1.0 } else { 1.0 };
    let mut out = Vec::new();
    for hd in &def.holes {
        let idx = if hd.panel == 'a' { jt.a } else { jt.b };
        let p = &parts[idx];
        let f = p.panel.frame.ok_or("деталь без положения")?;
        let mut at: [f64; 3] = std::array::from_fn(|k| hd.at[k] + hd.at_t[k] * t + hd.at_tb[k] * tb);
        at[2] *= toward;
        let w = world(at);
        let d = wdir(hd.dir);
        let local = f.local(w).map(clean);
        let ld = [0, 1, 2].map(|i| clean(d[0] * [f.x, f.y, f.z][i][0] + d[1] * [f.x, f.y, f.z][i][1] + d[2] * [f.x, f.y, f.z][i][2]));
        let depth = hd.depth + hd.depth_t * t;
        let (sx, sy, th) = (p.panel.size.0, p.panel.size.1, p.panel.thickness);
        let inside = |q: [f64; 3]| q[0] >= -EPS && q[0] <= sx + EPS && q[1] >= -EPS && q[1] <= sy + EPS && q[2] >= -EPS && q[2] <= th + EPS;
        let end: [f64; 3] = std::array::from_fn(|k| local[k] + ld[k] * depth);
        if !inside(local) || !inside(end) {
            return Err(format!(
                "«{}»: отверстие ⌀{}×{} в «{}» выходит за деталь (вход {:?}, конец {:?})",
                def.name.split('\r').next().unwrap_or(&def.name), hd.diameter, depth, p.panel.name, local, end.map(clean)
            ));
        }
        out.push((idx, Hole { pos: local, dir: ld, diameter: hd.diameter, depth }));
    }
    Ok(out)
}

fn clean(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

/// Все стыки среди деталей `ids` (пары, где торец одной на пласти другой).
pub fn joints(parts: &[Part], ids: &[usize]) -> Vec<Joint> {
    let mut out = Vec::new();
    for &a in ids {
        for &b in ids {
            if a != b && let Some(j) = joint(parts, a, b) {
                out.push(j);
            }
        }
    }
    out
}
