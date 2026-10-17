//! Собственная модель изделия: параметрические корпуса и команды для их изменения.
//!
//! Проект хранит параметры (габариты, полки, перегородки, двери), а детали строятся из них
//! функцией [`build`] в виде [`Panel`]. Поэтому спецификация, раскрой, карты, смета и XML
//! для Раскроя работают с собственными моделями так же, как с моделями Базиса.
//! Все изменения идут через [`Session::execute`]: команда с JSON-параметрами.
//! Тот же вход будут вызывать CLI, окно и MCP.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::contour::Seg;
use crate::panel::{Edge, Panel, Texture};

/// Метка формата в файле проекта `.bcraft`.
pub const FORMAT: &str = "basis-craft/1";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    #[serde(default)]
    pub article: String,
    pub thickness: f64,
    /// Формат листа, мм.
    #[serde(default = "default_sheet")]
    pub sheet: (f64, f64),
    /// Коэффициент расхода для сметы.
    #[serde(default = "default_coef")]
    pub coef: f64,
    /// Обрезка края листа, мм; нет — общая из параметров раскроя (у столешниц обычно 0).
    #[serde(default)]
    pub trim: Option<f64>,
    /// Пропил, мм; нет — общий.
    #[serde(default)]
    pub kerf: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EdgeMaterial {
    pub name: String,
    #[serde(default)]
    pub article: String,
    pub thickness: f64,
    /// Ширина ленты, мм.
    #[serde(default)]
    pub width: f64,
}

fn default_sheet() -> (f64, f64) {
    (2800.0, 2070.0)
}
fn default_coef() -> f64 {
    1.2
}
fn default_gap() -> f64 {
    2.0
}

/// Полка: в какой секции (между перегородками, слева направо с 0) и на какой высоте
/// её нижняя грань от верхней грани дна, мм.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Shelf {
    #[serde(default)]
    pub section: usize,
    pub height: f64,
}

/// Корпус: боковины, дно и крыша между боковинами, цоколь, перегородки, полки,
/// накладная задняя стенка и накладные двери. Оси: X — вправо, Y — вверх, Z — к фасаду.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cabinet {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// Положение левого нижнего заднего угла, мм.
    #[serde(default)]
    pub position: [f64; 3],
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    /// Материал корпуса (имя из `materials`).
    pub material: String,
    /// Кромка для лицевых торцов и дверей (имя из `edges`).
    #[serde(default)]
    pub edge: Option<String>,
    /// Высота цоколя, мм; 0 — без цоколя.
    #[serde(default)]
    pub plinth: f64,
    /// Материал задней стенки; нет — без стенки.
    #[serde(default)]
    pub back: Option<String>,
    /// Левые грани перегородок от левого края корпуса, мм.
    #[serde(default)]
    pub partitions: Vec<f64>,
    #[serde(default)]
    pub shelves: Vec<Shelf>,
    /// Число дверей по ширине; 0 — открытый корпус.
    #[serde(default)]
    pub doors: u32,
    /// Материал дверей; нет — материал корпуса.
    #[serde(default)]
    pub door_material: Option<String>,
    /// Зазор между дверями и по краям, мм.
    #[serde(default = "default_gap")]
    pub door_gap: f64,
    /// Выдвижные ящики: высоты фасадов снизу вверх, мм. Двери закрывают высоту над ними.
    #[serde(default)]
    pub drawers: Vec<f64>,
    /// Длина направляющих (глубина короба), мм; нет — глубина корпуса, округлённая вниз до 50 мм.
    #[serde(default)]
    pub drawer_depth: Option<f64>,
    /// Секция с ящиками (слева направо с 0), если есть перегородки.
    #[serde(default)]
    pub drawer_section: usize,
}

/// Столешница: лежит поверх корпусов, задаётся положением левого нижнего заднего угла.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Countertop {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub material: String,
    #[serde(default)]
    pub position: [f64; 3],
    pub length: f64,
    pub depth: f64,
    /// Кромка лицевого торца.
    #[serde(default)]
    pub edge: Option<String>,
}

/// Угловой Г-образный модуль в углу комнаты: крыло вдоль X (`width_x`) и крыло вдоль Z (`width_z`)
/// от общего заднего угла `position`. Стены — с задних сторон крыльев, фасады смотрят во внутренний угол.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Corner {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub position: [f64; 3],
    pub width_x: f64,
    pub width_z: f64,
    pub height: f64,
    pub depth: f64,
    pub material: String,
    #[serde(default)]
    pub edge: Option<String>,
    #[serde(default)]
    pub plinth: f64,
    #[serde(default)]
    pub back: Option<String>,
    /// Высоты полок от верха дна, мм.
    #[serde(default)]
    pub shelves: Vec<f64>,
    /// Двери на обоих крыльях.
    #[serde(default)]
    pub doors: bool,
    #[serde(default)]
    pub door_material: Option<String>,
    #[serde(default = "default_gap")]
    pub door_gap: f64,
}

/// Свободная деталь: плита с произвольным контуром — так в проект попадают детали из моделей Базиса.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FreePanel {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// Номер позиции из исходной модели.
    #[serde(default)]
    pub pos: String,
    pub material: String,
    /// Контур с габаритом от (0, 0).
    pub segs: Vec<Seg>,
    pub texture: Texture,
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Присадка из исходной модели.
    #[serde(default)]
    pub holes: Vec<crate::panel::Hole>,
    /// Габарит в пространстве: минимальный угол и размеры по X, Y, Z, мм.
    pub origin: [f64; 3],
    pub size: [f64; 3],
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    pub format: String,
    pub name: String,
    #[serde(default)]
    pub materials: Vec<Material>,
    #[serde(default)]
    pub edges: Vec<EdgeMaterial>,
    #[serde(default)]
    pub cabinets: Vec<Cabinet>,
    #[serde(default)]
    pub countertops: Vec<Countertop>,
    #[serde(default)]
    pub corners: Vec<Corner>,
    #[serde(default)]
    pub panels: Vec<FreePanel>,
}

impl Project {
    pub fn new(name: &str) -> Self {
        Project {
            format: FORMAT.into(),
            name: name.into(),
            materials: Vec::new(),
            edges: Vec::new(),
            cabinets: Vec::new(),
            countertops: Vec::new(),
            corners: Vec::new(),
            panels: Vec::new(),
        }
    }

    pub fn load(json: &str) -> Result<Self, String> {
        let p: Project = serde_json::from_str(json).map_err(|e| format!("проект: {e}"))?;
        if p.format != FORMAT {
            return Err(format!("проект: неизвестный формат «{}»", p.format));
        }
        build(&p)?;
        Ok(p)
    }

    pub fn save(&self) -> String {
        serde_json::to_string_pretty(self).expect("проект сериализуется")
    }

    pub fn material(&self, name: &str) -> Result<&Material, String> {
        self.materials.iter().find(|m| m.name == name).ok_or_else(|| format!("нет материала «{name}»"))
    }

    fn edge(&self, name: &Option<String>) -> Result<Option<&EdgeMaterial>, String> {
        name.as_ref()
            .map(|n| self.edges.iter().find(|e| &e.name == n).ok_or_else(|| format!("нет кромки «{n}»")))
            .transpose()
    }
}

/// Название и артикул в формате Базиса `Название\rАртикул`.
fn full(name: &str, article: &str) -> String {
    if article.is_empty() { name.into() } else { format!("{name}\r{article}") }
}

/// Назначение детали Базиса по её имени — только для цвета на видах.
/// shortcut: угадывание по словам «фасад», «дверь», «задняя стенка»; уточнить, если найдётся признак в модели.
pub fn guess_kind(p: &Panel) -> Kind {
    let name = p.name.to_lowercase();
    if name.contains("фасад") || name.contains("двер") {
        Kind::Door
    } else if name.contains("задн") && name.contains("стен") && p.thickness <= 6.0 {
        Kind::Back
    } else {
        Kind::Body
    }
}

/// Проект из деталей модели Базиса: каждая плита — свободная деталь, материалы — из встреченных
/// (формат листа и коэффициент по умолчанию). Плиты без положения в пространстве пропускаются.
pub fn import(name: &str, panels: &[Panel]) -> Project {
    let mut p = Project::new(name);
    // (материал Базиса, толщина) → имя материала проекта. Один материал разной толщины — разные материалы.
    let mut seen: Vec<((&str, f64), String)> = Vec::new();
    for (i, src) in panels.iter().enumerate() {
        let Some((lo, hi)) = src.bounds else { continue };
        let (mat_name, article) = src.material.split_once('\r').unwrap_or((&src.material, ""));
        let key = match seen.iter().find(|(k, _)| *k == (src.material.as_str(), src.thickness)) {
            Some((_, name)) => name.clone(),
            None => {
                let taken = p.materials.iter().any(|m| m.name == mat_name);
                let key = if taken { format!("{mat_name} ({})", src.thickness) } else { mat_name.to_string() };
                p.materials.push(Material { name: key.clone(), article: article.into(), thickness: src.thickness, sheet: default_sheet(), coef: default_coef(), trim: None, kerf: None });
                seen.push(((src.material.as_str(), src.thickness), key.clone()));
                key
            }
        };
        p.panels.push(FreePanel {
            id: i as u32 + 1,
            name: src.name.clone(),
            pos: src.pos.clone(),
            material: key,
            segs: src.segs.clone(),
            texture: src.texture,
            edges: src.edges.clone(),
            holes: src.holes.clone(),
            origin: lo,
            size: std::array::from_fn(|k| hi[k] - lo[k]),
        });
    }
    p
}

/// Назначение детали — для отображения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Body,
    Back,
    Door,
    /// Фасад ящика.
    Drawer,
}

/// Деталь изделия: плита для отчётов и её габаритный параллелепипед в координатах проекта.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub cabinet: u32,
    pub kind: Kind,
    pub panel: Panel,
    /// Минимальный угол, мм.
    pub origin: [f64; 3],
    /// Размеры по X, Y, Z, мм.
    pub size: [f64; 3],
}

/// Стороны прямоугольного контура: низ, лево, верх, право — в порядке сегментов, как у Базиса.
const BOTTOM: usize = 0;
const LEFT: usize = 1;
const TOP: usize = 2;
const RIGHT: usize = 3;

fn rect(name: &str, mat: &Material, sx: f64, sy: f64, texture: Texture, edged: &[usize], edge: Option<&EdgeMaterial>) -> Panel {
    poly(name, mat, &[[sx, 0.0], [0.0, 0.0], [0.0, sy], [sx, sy]], texture, edged, edge)
}

/// Плита с многоугольным контуром: сегмент i идёт от вершины i к i+1. Габарит — от (0, 0).
fn poly(name: &str, mat: &Material, pts: &[[f64; 2]], texture: Texture, edged: &[usize], edge: Option<&EdgeMaterial>) -> Panel {
    let segs: Vec<Seg> = (0..pts.len()).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % pts.len()] }).collect();
    let size = pts.iter().fold((0.0, 0.0), |s: (f64, f64), p| (s.0.max(p[0]), s.1.max(p[1])));
    let edges = edge
        .map(|e| {
            edged
                .iter()
                .map(|&segment| Edge {
                    material: full(&e.name, &e.article),
                    thickness: e.thickness,
                    width: e.width,
                    allowance: 0.0,
                    // Размер детали в модели — чистовой, кромка подрезает заготовку.
                    clip: true,
                    segment,
                })
                .collect()
        })
        .unwrap_or_default();
    Panel {
        name: name.into(),
        pos: String::new(),
        designation: String::new(),
        material: full(&mat.name, &mat.article),
        thickness: mat.thickness,
        segs,
        size,
        texture,
        edges,
        bounds: None,
        holes: Vec::new(),
    }
}

/// Строит детали всех корпусов. Ошибка — если параметры не дают корректного изделия.
pub fn build(p: &Project) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    for c in &p.cabinets {
        cabinet(p, c, &mut parts).map_err(|e| format!("корпус {} «{}»: {e}", c.id, c.name))?;
    }
    for c in &p.corners {
        corner(p, c, &mut parts).map_err(|e| format!("угловой модуль {} «{}»: {e}", c.id, c.name))?;
    }
    for fp in &p.panels {
        let m = p.material(&fp.material).map_err(|e| format!("деталь {} «{}»: {e}", fp.id, fp.name))?;
        let (_, hi) = crate::contour::bounds(&fp.segs).ok_or_else(|| format!("деталь {} «{}»: пустой контур", fp.id, fp.name))?;
        let panel = Panel {
            name: fp.name.clone(),
            pos: fp.pos.clone(),
            designation: String::new(),
            material: full(&m.name, &m.article),
            thickness: m.thickness,
            segs: fp.segs.clone(),
            size: (hi[0], hi[1]),
            texture: fp.texture,
            edges: fp.edges.clone(),
            bounds: None,
            holes: fp.holes.clone(),
        };
        parts.push(Part { cabinet: 0, kind: guess_kind(&panel), panel, origin: fp.origin, size: fp.size });
    }
    for ct in &p.countertops {
        countertop(p, ct, &mut parts).map_err(|e| format!("столешница {} «{}»: {e}", ct.id, ct.name))?;
    }
    // Номера позиций — по порядку деталей в проекте; у импортированных деталей — свои.
    for (i, part) in parts.iter_mut().enumerate() {
        if part.panel.pos.is_empty() {
            part.panel.pos = (i + 1).to_string();
        }
    }
    Ok(parts)
}

fn cabinet(p: &Project, c: &Cabinet, out: &mut Vec<Part>) -> Result<(), String> {
    let m = p.material(&c.material)?;
    let edge = p.edge(&c.edge)?;
    let t = m.thickness;
    let (w, h, d) = (c.width, c.height, c.depth);
    let inner_w = w - 2.0 * t;
    let inner_h = h - c.plinth - 2.0 * t;
    if !(t > 0.0 && d > 0.0 && inner_w > 0.0 && inner_h > 0.0 && c.plinth >= 0.0) {
        return Err(format!("габариты {w}×{h}×{d} с плитой {t} мм и цоколем {} мм не дают внутреннего объёма", c.plinth));
    }
    let o = c.position;
    let mut add = |kind: Kind, panel: Panel, origin: [f64; 3], size: [f64; 3]| {
        out.push(Part { cabinet: c.id, kind, panel, origin: [o[0] + origin[0], o[1] + origin[1], o[2] + origin[2]], size });
    };
    // Вертикальные детали: контур X — глубина (от задней кромки к лицевой), Y — высота, текстура вдоль высоты.
    let vertical = |name: &str, height: f64| rect(name, m, d, height, Texture::Vertical, &[RIGHT], edge);
    // Горизонтальные: X — длина, Y — глубина (лицевая кромка — верх контура), текстура вдоль длины.
    let horizontal = |name: &str, length: f64| rect(name, m, length, d, Texture::Horizontal, &[TOP], edge);

    add(Kind::Body, vertical("Боковина левая", h), [0.0, 0.0, 0.0], [t, h, d]);
    add(Kind::Body, vertical("Боковина правая", h), [w - t, 0.0, 0.0], [t, h, d]);
    add(Kind::Body, horizontal("Дно", inner_w), [t, c.plinth, 0.0], [inner_w, t, d]);
    add(Kind::Body, horizontal("Крыша", inner_w), [t, h - t, 0.0], [inner_w, t, d]);
    if c.plinth > 0.0 {
        add(Kind::Body, rect("Цоколь", m, inner_w, c.plinth, Texture::Horizontal, &[], None), [t, 0.0, d - t], [inner_w, c.plinth, t]);
    }

    // Перегородки и секции между ними.
    let mut xs = c.partitions.clone();
    xs.sort_by(f64::total_cmp);
    let mut sections = Vec::new();
    let mut left = t;
    for &x in &xs {
        if x < left || x + t > w - t {
            return Err(format!("перегородка на {x} мм не помещается (свободно от {left} до {} мм)", w - 2.0 * t));
        }
        sections.push((left, x - left));
        add(Kind::Body, vertical("Перегородка", inner_h), [x, c.plinth + t, 0.0], [t, inner_h, d]);
        left = x + t;
    }
    sections.push((left, w - t - left));
    if let Some(&(_, sw)) = sections.iter().find(|s| s.1 <= 0.0) {
        return Err(format!("перегородки оставляют секцию шириной {sw} мм"));
    }

    let &(dsx, dsw) = sections
        .get(c.drawer_section)
        .ok_or_else(|| format!("ящики: нет секции {} (всего {})", c.drawer_section, sections.len()))?;
    // Длина направляющих кратна 50 мм; верх зоны ящиков — над верхним фасадом, от верха дна.
    let drawer_depth = c.drawer_depth.unwrap_or(((d - 10.0) / 50.0).floor() * 50.0);
    let drawer_zone = if c.drawers.is_empty() {
        0.0
    } else {
        c.door_gap + c.drawers.iter().map(|hf| hf + c.door_gap).sum::<f64>() - t
    };
    for s in &c.shelves {
        let &(sx, sw) = sections
            .get(s.section)
            .ok_or_else(|| format!("нет секции {} (всего {})", s.section, sections.len()))?;
        if s.height < 0.0 || s.height + t > inner_h {
            return Err(format!("полка на высоте {} мм не помещается (внутри {inner_h} мм)", s.height));
        }
        if s.section == c.drawer_section && s.height < drawer_zone {
            return Err(format!("полка на высоте {} мм попадает в зону ящиков (до {drawer_zone} мм)", s.height));
        }
        add(Kind::Body, horizontal("Полка", sw), [sx, c.plinth + t + s.height, 0.0], [sw, t, d]);
    }

    if let Some(name) = &c.back {
        let bm = p.material(name)?;
        let bh = h - c.plinth;
        add(Kind::Back, rect("Задняя стенка", bm, w, bh, Texture::Default, &[], None), [0.0, c.plinth, -bm.thickness], [w, bh, bm.thickness]);
    }

    let dm = match &c.door_material {
        Some(n) => p.material(n)?,
        None => m,
    };
    let g = c.door_gap;
    // Ящики: фасады снизу вверх с зазором g, короб под шариковые направляющие (13 мм на сторону).
    let mut front_y = c.plinth + g;
    // Фасад ящика закрывает секцию до середины соседних перегородок (у крайних секций — до края корпуса).
    let fx0 = if c.drawer_section == 0 { 0.0 } else { dsx - t / 2.0 };
    let fx1 = if c.drawer_section + 1 == sections.len() { w } else { dsx + dsw + t / 2.0 };
    for (i, &hf) in c.drawers.iter().enumerate() {
        let fw = fx1 - fx0 - 2.0 * g;
        let bw = dsw - 2.0 * RAIL_GAP;
        let bh = hf - 40.0;
        let bd = drawer_depth;
        if !(hf > 0.0 && fw > 0.0 && bw > 2.0 * t && bh >= 60.0 && bd >= 250.0 && bd <= d) {
            return Err(format!("ящик {} (фасад {hf} мм, короб {bw:.0}×{bh:.0}×{bd:.0}) не строится: нужен фасад ≥ 100 мм и глубина ≥ 250 мм", i + 1));
        }
        let facade = rect("Фасад ящика", dm, fw, hf, Texture::Horizontal, &[BOTTOM, LEFT, TOP, RIGHT], edge);
        add(Kind::Drawer, facade, [fx0 + g, front_y, d], [fw, hf, dm.thickness]);
        let (bx, by, bz) = (dsx + RAIL_GAP, front_y + 20.0, d - bd);
        // Короб: боковины (X — глубина, Y — высота), стенки между боковинами, дно под коробом.
        let side = |name| rect(name, m, bd, bh, Texture::Horizontal, &[TOP], edge);
        add(Kind::Body, side("Боковина ящика"), [bx, by, bz], [t, bh, bd]);
        add(Kind::Body, side("Боковина ящика"), [bx + bw - t, by, bz], [t, bh, bd]);
        let wall = |name| rect(name, m, bw - 2.0 * t, bh, Texture::Horizontal, &[TOP], edge);
        add(Kind::Body, wall("Передняя стенка ящика"), [bx + t, by, d - t], [bw - 2.0 * t, bh, t]);
        add(Kind::Body, wall("Задняя стенка ящика"), [bx + t, by, bz], [bw - 2.0 * t, bh, t]);
        let bm = match &c.back {
            Some(n) => p.material(n)?,
            None => m,
        };
        add(Kind::Body, rect("Дно ящика", bm, bw, bd, Texture::Default, &[], None), [bx, by - bm.thickness, bz], [bw, bm.thickness, bd]);
        front_y += hf + g;
    }

    if c.doors > 0 {
        let n = c.doors as f64;
        let (dw, dh) = ((w - (n + 1.0) * g) / n, h - front_y - g);
        if !(dw > 0.0 && dh > 0.0 && g >= 0.0) {
            return Err(format!("{} дверей с зазором {g} мм не помещаются над ящиками", c.doors));
        }
        for i in 0..c.doors {
            let x = g + i as f64 * (dw + g);
            let door = rect("Дверь", dm, dw, dh, Texture::Vertical, &[BOTTOM, LEFT, TOP, RIGHT], edge);
            add(Kind::Door, door, [x, front_y, d], [dw, dh, dm.thickness]);
        }
    } else if front_y > h {
        return Err(format!("ящики выше корпуса: фасады до {front_y} мм при высоте {h} мм"));
    }
    Ok(())
}

fn corner(p: &Project, c: &Corner, out: &mut Vec<Part>) -> Result<(), String> {
    let m = p.material(&c.material)?;
    let edge = p.edge(&c.edge)?;
    let t = m.thickness;
    let (h, d, g) = (c.height, c.depth, c.door_gap);
    let (ax, bz) = (c.width_x - t, c.width_z - t);
    let inner_h = h - c.plinth - 2.0 * t;
    if !(ax >= d + 100.0 && bz >= d + 100.0 && inner_h > 0.0 && c.plinth >= 0.0) {
        return Err(format!("крылья {}×{} должны быть хотя бы на 100 мм больше глубины {d} плюс плита; внутри по высоте {inner_h} мм", c.width_x, c.width_z));
    }
    let o = c.position;
    let mut add = |kind: Kind, panel: Panel, origin: [f64; 3], size: [f64; 3]| {
        out.push(Part { cabinet: c.id, kind, panel, origin: [o[0] + origin[0], o[1] + origin[1], o[2] + origin[2]], size });
    };
    // Г-образные горизонтали в плане (X, Z): лицевые кромки — внутренний угол (сегменты 3 и 4).
    // shortcut: на видах Г-деталь рисуется своим габаритом (квадратом) — для точного 3D нужны многогранники.
    let l_shape = |name| poly(name, m, &[[ax, 0.0], [0.0, 0.0], [0.0, bz], [d, bz], [d, d], [ax, d]], Texture::Horizontal, &[3, 4], edge);
    let side = |name| rect(name, m, d, h, Texture::Vertical, &[RIGHT], edge);
    add(Kind::Body, side("Боковина крыла X"), [ax, 0.0, 0.0], [t, h, d]);
    add(Kind::Body, side("Боковина крыла Z"), [0.0, 0.0, bz], [d, h, t]);
    add(Kind::Body, l_shape("Дно угловое"), [0.0, c.plinth, 0.0], [ax, t, bz]);
    add(Kind::Body, l_shape("Крыша угловая"), [0.0, h - t, 0.0], [ax, t, bz]);
    for &sh in &c.shelves {
        if sh < 0.0 || sh + t > inner_h {
            return Err(format!("полка на высоте {sh} мм не помещается (внутри {inner_h} мм)"));
        }
        add(Kind::Body, l_shape("Полка угловая"), [0.0, c.plinth + t + sh, 0.0], [ax, t, bz]);
    }
    if c.plinth > 0.0 {
        add(Kind::Body, rect("Цоколь", m, ax - d + t, c.plinth, Texture::Horizontal, &[], None), [d - t, 0.0, d - t], [ax - d + t, c.plinth, t]);
        add(Kind::Body, rect("Цоколь", m, bz - d, c.plinth, Texture::Horizontal, &[], None), [d - t, 0.0, d], [t, c.plinth, bz - d]);
    }
    if let Some(name) = &c.back {
        let bm = p.material(name)?;
        let bh = h - c.plinth;
        add(Kind::Back, rect("Задняя стенка", bm, c.width_x, bh, Texture::Default, &[], None), [0.0, c.plinth, -bm.thickness], [c.width_x, bh, bm.thickness]);
        add(Kind::Back, rect("Задняя стенка", bm, c.width_z, bh, Texture::Default, &[], None), [-bm.thickness, c.plinth, 0.0], [bm.thickness, bh, c.width_z]);
    }
    if c.doors {
        let dm = match &c.door_material {
            Some(n) => p.material(n)?,
            None => m,
        };
        let dh = h - c.plinth - 2.0 * g;
        // Дверь крыла X начинается за дверью крыла Z, чтобы они не сталкивались во внутреннем углу.
        let (dwx, dwz) = (c.width_x - d - dm.thickness - 2.0 * g, c.width_z - d - 2.0 * g);
        if !(dwx > 0.0 && dwz > 0.0 && dh > 0.0) {
            return Err("двери не помещаются".into());
        }
        let door = |w| rect("Дверь", dm, w, dh, Texture::Vertical, &[BOTTOM, LEFT, TOP, RIGHT], edge);
        add(Kind::Door, door(dwx), [d + dm.thickness + g, c.plinth + g, d], [dwx, dh, dm.thickness]);
        add(Kind::Door, door(dwz), [d, c.plinth + g, d + g], [dm.thickness, dh, dwz]);
    }
    Ok(())
}

/// Зазор под шариковую направляющую с каждой стороны короба, мм.
const RAIL_GAP: f64 = 13.0;

fn countertop(p: &Project, ct: &Countertop, out: &mut Vec<Part>) -> Result<(), String> {
    let m = p.material(&ct.material)?;
    if !(ct.length > 0.0 && ct.depth > 0.0) {
        return Err(format!("размеры {}×{} должны быть больше нуля", ct.length, ct.depth));
    }
    let panel = rect("Столешница", m, ct.length, ct.depth, Texture::Horizontal, &[TOP], p.edge(&ct.edge)?);
    out.push(Part { cabinet: 0, kind: Kind::Body, panel, origin: ct.position, size: [ct.length, m.thickness, ct.depth] });
    Ok(())
}

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

/// Состояние редактирования: проект и история для отмены.
pub struct Session {
    pub project: Project,
    undo: Vec<Project>,
    redo: Vec<Project>,
}

/// Справка по командам: имя → описание параметров. Её же покажут CLI и MCP.
pub const COMMANDS: &[(&str, &str)] = &[
    ("project.get", "весь проект в JSON"),
    ("project.rename", "{name}"),
    ("material.add", "{name, thickness, article?, sheet?: [длина, ширина], coef?, trim?: обрезка края, kerf?: пропил}"),
    ("material.set", "{name, ...поля} — изменить материал"),
    ("edge.add", "{name, thickness, article?, width?}"),
    ("cabinet.add", "{width, height, depth, material, name?, edge?, plinth?, back?, doors?, door_material?, door_gap?, drawers?: [высоты фасадов снизу вверх], drawer_depth?, drawer_section?, position?: [x,y,z]} → {id}. Навесной модуль — plinth 0 и position с высотой, например [0, 1400, 250]"),
    ("cabinet.set", "{id, ...поля корпуса} — изменить параметры"),
    ("cabinet.remove", "{id}"),
    ("shelf.add", "{cabinet, height, section?}"),
    ("shelf.remove", "{cabinet, index}"),
    ("partition.add", "{cabinet, x}"),
    ("partition.remove", "{cabinet, index}"),
    ("countertop.add", "{material, length, depth, position?: [x,y,z] левого нижнего заднего угла, edge?, name?} → {id}"),
    ("countertop.set", "{id, ...поля}"),
    ("countertop.remove", "{id}"),
    ("corner.add", "{width_x, width_z, height, depth, material, edge?, plinth?, back?, shelves?: [высоты], doors?: true, door_material?, door_gap?, position?: [x,y,z] заднего угла, name?} → {id}. Г-образный угловой модуль: крылья вдоль X и Z от угла комнаты"),
    ("corner.set", "{id, ...поля}"),
    ("corner.remove", "{id}"),
    ("panel.set", "{id, name?, material?, origin?: [x,y,z]} — свободная деталь (из импортированной модели Базиса)"),
    ("panel.remove", "{id}"),
    ("parts", "список деталей: имя, материал, длина, ширина, толщина, корпус"),
    ("undo", "отменить последнее изменение"),
    ("redo", "вернуть отменённое"),
];

fn field<'a>(p: &'a Value, key: &str) -> Result<&'a Value, String> {
    p.get(key).ok_or_else(|| format!("нет параметра «{key}»"))
}
fn f(p: &Value, key: &str) -> Result<f64, String> {
    field(p, key)?.as_f64().ok_or_else(|| format!("«{key}» должно быть числом"))
}
fn s<'a>(p: &'a Value, key: &str) -> Result<&'a str, String> {
    field(p, key)?.as_str().ok_or_else(|| format!("«{key}» должно быть строкой"))
}
fn idx(p: &Value, key: &str) -> Result<usize, String> {
    field(p, key)?.as_u64().map(|v| v as usize).ok_or_else(|| format!("«{key}» должно быть целым ≥ 0"))
}

/// Накладывает поля `patch` на объект `base` (кроме `except`) и разбирает результат обратно.
fn patch<T: Serialize + for<'de> Deserialize<'de>>(base: &T, patch: &Value, except: &str) -> Result<T, String> {
    let mut v = serde_json::to_value(base).map_err(|e| e.to_string())?;
    let (Some(obj), Some(changes)) = (v.as_object_mut(), patch.as_object()) else {
        return Err("параметры должны быть объектом".into());
    };
    for (k, val) in changes {
        if k != except {
            obj.insert(k.clone(), val.clone());
        }
    }
    serde_json::from_value(v).map_err(|e| format!("параметры: {e}"))
}

impl Session {
    pub fn new(project: Project) -> Self {
        Session { project, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Выполняет команду. Изменение применяется, только если изделие после него строится.
    pub fn execute(&mut self, cmd: &str, p: &Value) -> Result<Value, String> {
        match cmd {
            "project.get" => return serde_json::to_value(&self.project).map_err(|e| e.to_string()),
            "parts" => {
                let parts = build(&self.project)?;
                return Ok(Value::Array(
                    parts
                        .iter()
                        .map(|part| {
                            let (l, w) = part.panel.cut_size();
                            json!({
                                "pos": part.panel.pos, "name": part.panel.name, "cabinet": part.cabinet,
                                "material": crate::panel::material_name(&part.panel.material),
                                "length": l, "width": w, "thickness": part.panel.thickness,
                            })
                        })
                        .collect(),
                ));
            }
            "undo" | "redo" => {
                let (from, to) = if cmd == "undo" { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
                let prev = from.pop().ok_or("нечего отменять/возвращать")?;
                to.push(std::mem::replace(&mut self.project, prev));
                return Ok(Value::Null);
            }
            _ => {}
        }
        let mut next = self.project.clone();
        let out = apply(&mut next, cmd, p)?;
        build(&next)?;
        self.undo.push(std::mem::replace(&mut self.project, next));
        self.redo.clear();
        Ok(out)
    }
}

fn cabinet_mut<'a>(pr: &'a mut Project, p: &Value, key: &str) -> Result<&'a mut Cabinet, String> {
    let id = idx(p, key)? as u32;
    pr.cabinets.iter_mut().find(|c| c.id == id).ok_or_else(|| format!("нет корпуса {id}"))
}

/// Элемент проекта с номером и именем: столешница, угловой модуль.
trait Item: Serialize + for<'de> Deserialize<'de> {
    fn id(&self) -> u32;
    fn name(&mut self) -> &mut String;
}
impl Item for Countertop {
    fn id(&self) -> u32 {
        self.id
    }
    fn name(&mut self) -> &mut String {
        &mut self.name
    }
}
impl Item for FreePanel {
    fn id(&self) -> u32 {
        self.id
    }
    fn name(&mut self) -> &mut String {
        &mut self.name
    }
}
impl Item for Corner {
    fn id(&self) -> u32 {
        self.id
    }
    fn name(&mut self) -> &mut String {
        &mut self.name
    }
}

/// Команды `*.add` (→ {id}), `*.set` {id, ...поля}, `*.remove` {id} над списком элементов.
fn item<T: Item>(list: &mut Vec<T>, cmd: &str, p: &Value, what: &str) -> Result<Value, String> {
    if cmd.ends_with(".add") {
        let id = list.iter().map(Item::id).max().map_or(1, |m| m + 1);
        let mut v = p.clone();
        v.as_object_mut().ok_or("параметры должны быть объектом")?.insert("id".into(), json!(id));
        let mut x: T = serde_json::from_value(v).map_err(|e| format!("{what}: {e}"))?;
        if x.name().is_empty() {
            *x.name() = format!("{what} {id}");
        }
        list.push(x);
        return Ok(json!({ "id": id }));
    }
    let id = idx(p, "id")? as u32;
    let i = list.iter().position(|x| x.id() == id).ok_or_else(|| format!("{what}: нет номера {id}"))?;
    if cmd.ends_with(".set") {
        list[i] = patch(&list[i], p, "id")?;
    } else {
        list.remove(i);
    }
    Ok(Value::Null)
}

fn apply(pr: &mut Project, cmd: &str, p: &Value) -> Result<Value, String> {
    match cmd {
        "project.rename" => pr.name = s(p, "name")?.into(),
        "material.add" => {
            let m: Material = serde_json::from_value(p.clone()).map_err(|e| format!("материал: {e}"))?;
            if pr.materials.iter().any(|x| x.name == m.name) {
                return Err(format!("материал «{}» уже есть", m.name));
            }
            if !(m.thickness > 0.0 && m.sheet.0 > 0.0 && m.sheet.1 > 0.0 && m.coef > 0.0) {
                return Err("толщина, формат листа и коэффициент должны быть больше нуля".into());
            }
            pr.materials.push(m);
        }
        "material.set" => {
            let name = s(p, "name")?;
            let i = pr.materials.iter().position(|m| m.name == name).ok_or_else(|| format!("нет материала «{name}»"))?;
            pr.materials[i] = patch(&pr.materials[i], p, "name")?;
        }
        "edge.add" => {
            let e: EdgeMaterial = serde_json::from_value(p.clone()).map_err(|e| format!("кромка: {e}"))?;
            if pr.edges.iter().any(|x| x.name == e.name) {
                return Err(format!("кромка «{}» уже есть", e.name));
            }
            pr.edges.push(e);
        }
        "cabinet.add" => {
            let id = pr.cabinets.iter().map(|c| c.id).max().map_or(1, |m| m + 1);
            let mut v = p.clone();
            v.as_object_mut().ok_or("параметры должны быть объектом")?.insert("id".into(), json!(id));
            let mut c: Cabinet = serde_json::from_value(v).map_err(|e| format!("корпус: {e}"))?;
            if c.name.is_empty() {
                c.name = format!("Корпус {id}");
            }
            pr.cabinets.push(c);
            return Ok(json!({ "id": id }));
        }
        "cabinet.set" => {
            let c = cabinet_mut(pr, p, "id")?;
            *c = patch(&*c, p, "id")?;
        }
        "cabinet.remove" => {
            let id = idx(p, "id")? as u32;
            let n = pr.cabinets.len();
            pr.cabinets.retain(|c| c.id != id);
            if pr.cabinets.len() == n {
                return Err(format!("нет корпуса {id}"));
            }
        }
        "shelf.add" => {
            let shelf = Shelf { section: p.get("section").and_then(Value::as_u64).unwrap_or(0) as usize, height: f(p, "height")? };
            cabinet_mut(pr, p, "cabinet")?.shelves.push(shelf);
        }
        "shelf.remove" => {
            let i = idx(p, "index")?;
            let c = cabinet_mut(pr, p, "cabinet")?;
            if i >= c.shelves.len() {
                return Err(format!("нет полки {i}"));
            }
            c.shelves.remove(i);
        }
        "partition.add" => {
            let x = f(p, "x")?;
            cabinet_mut(pr, p, "cabinet")?.partitions.push(x);
        }
        "partition.remove" => {
            let i = idx(p, "index")?;
            let c = cabinet_mut(pr, p, "cabinet")?;
            if i >= c.partitions.len() {
                return Err(format!("нет перегородки {i}"));
            }
            c.partitions.remove(i);
        }
        "countertop.add" | "countertop.set" | "countertop.remove" => return item(&mut pr.countertops, cmd, p, "Столешница"),
        "corner.add" | "corner.set" | "corner.remove" => return item(&mut pr.corners, cmd, p, "Угловой модуль"),
        "panel.set" | "panel.remove" => return item(&mut pr.panels, cmd, p, "Деталь"),
        _ => return Err(format!("неизвестная команда «{cmd}»")),
    }
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wardrobe() -> Session {
        let mut s = Session::new(Project::new("Шкаф"));
        s.execute("material.add", &json!({"name": "ЛДСП Белый 16", "article": "W980", "thickness": 16})).unwrap();
        s.execute("material.add", &json!({"name": "ХДФ 3", "thickness": 3, "coef": 1.1})).unwrap();
        s.execute("edge.add", &json!({"name": "ПВХ 0,4 белая", "thickness": 0.4, "width": 19})).unwrap();
        let id = s
            .execute("cabinet.add", &json!({"width": 800, "height": 2000, "depth": 600, "material": "ЛДСП Белый 16",
                "edge": "ПВХ 0,4 белая", "plinth": 80, "back": "ХДФ 3", "doors": 2}))
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        s.execute("partition.add", &json!({"cabinet": id, "x": 392})).unwrap();
        s.execute("shelf.add", &json!({"cabinet": id, "height": 400, "section": 0})).unwrap();
        s.execute("shelf.add", &json!({"cabinet": id, "height": 800, "section": 1})).unwrap();
        s
    }

    fn by_name<'a>(parts: &'a [Part], name: &str) -> Vec<&'a Part> {
        parts.iter().filter(|p| p.panel.name == name).collect()
    }

    #[test]
    fn wardrobe_parts_and_sizes() {
        let s = wardrobe();
        let parts = build(&s.project).unwrap();
        // 2 боковины, дно, крыша, цоколь, перегородка, 2 полки, задняя стенка, 2 двери.
        assert_eq!(parts.len(), 11);
        let side = by_name(&parts, "Боковина левая")[0];
        assert_eq!(side.panel.size, (600.0, 2000.0));
        // Лицевая кромка 0,4 подрезает заготовку по глубине; длина вдоль текстуры — высота.
        assert_eq!(side.panel.cut_size(), (2000.0, 599.6));
        assert_eq!(by_name(&parts, "Дно")[0].panel.size, (768.0, 600.0));
        assert_eq!(by_name(&parts, "Перегородка")[0].panel.size, (600.0, 2000.0 - 80.0 - 32.0));
        let shelves = by_name(&parts, "Полка");
        // Секции: 16..392 (376 мм) и 408..784 (376 мм).
        assert_eq!(shelves[0].panel.size.0, 376.0);
        assert_eq!(shelves[1].origin[0], 408.0);
        let doors = by_name(&parts, "Дверь");
        assert_eq!(doors.len(), 2);
        assert_eq!(doors[0].panel.size, (397.0, 1916.0));
        assert_eq!(doors[0].panel.edges.len(), 4);
        assert_eq!(by_name(&parts, "Задняя стенка")[0].panel.thickness, 3.0);
        assert_eq!(parts[0].panel.pos, "1");
    }

    #[test]
    fn invalid_changes_are_rejected_and_project_stays_valid() {
        let mut s = wardrobe();
        let before = s.project.clone();
        assert!(s.execute("shelf.add", &json!({"cabinet": 1, "height": 5000})).is_err());
        assert!(s.execute("partition.add", &json!({"cabinet": 1, "x": 790})).is_err());
        assert!(s.execute("cabinet.set", &json!({"id": 1, "material": "нет такого"})).is_err());
        assert!(s.execute("cabinet.set", &json!({"id": 1, "width": 20})).is_err());
        assert!(s.execute("shelf.add", &json!({"cabinet": 1, "height": 100, "section": 7})).is_err());
        assert!(s.execute("bogus", &json!({})).is_err());
        assert_eq!(s.project, before);
    }

    #[test]
    fn set_undo_redo() {
        let mut s = wardrobe();
        s.execute("cabinet.set", &json!({"id": 1, "width": 1000, "doors": 3})).unwrap();
        assert_eq!(by_name(&build(&s.project).unwrap(), "Дверь").len(), 3);
        s.execute("undo", &json!({})).unwrap();
        assert_eq!(s.project.cabinets[0].width, 800.0);
        s.execute("redo", &json!({})).unwrap();
        assert_eq!(s.project.cabinets[0].width, 1000.0);
        assert!(s.execute("redo", &json!({})).is_err());
    }

    #[test]
    fn drawers_countertop_and_wall_module() {
        let mut s = wardrobe();
        s.execute("cabinet.add", &json!({"name": "Стол с ящиками", "width": 600, "height": 820, "depth": 560,
            "material": "ЛДСП Белый 16", "edge": "ПВХ 0,4 белая", "plinth": 100, "back": "ХДФ 3", "drawers": [140, 280, 280]})).unwrap();
        let parts = build(&s.project).unwrap();
        let mine: Vec<&Part> = parts.iter().filter(|p| p.cabinet == 2).collect();
        let count = |n: &str| mine.iter().filter(|p| p.panel.name == n).count();
        assert_eq!((count("Фасад ящика"), count("Боковина ящика"), count("Дно ящика")), (3, 6, 3));
        // Короб: 600 − 2·16 − 2·13 = 542 по ширине, направляющие 550 (глубина 560 → кратно 50).
        let bottom = mine.iter().find(|p| p.panel.name == "Дно ящика").unwrap();
        assert_eq!(bottom.panel.size, (542.0, 550.0));
        assert_eq!(bottom.panel.thickness, 3.0);
        let side = mine.iter().find(|p| p.panel.name == "Боковина ящика").unwrap();
        assert_eq!(side.panel.size, (550.0, 100.0));
        // Фасады не выходят за корпус: 100 + 2 + 140+2 + 280+2 + 280+2 = 808 ≤ 820.
        let top = mine.iter().filter(|p| p.kind == Kind::Drawer).map(|p| p.origin[1] + p.size[1]).fold(0.0, f64::max);
        assert!(top <= 820.0, "{top}");
        // Слишком высокие ящики и полка в зоне ящиков отклоняются.
        assert!(s.execute("cabinet.set", &json!({"id": 2, "drawers": [400, 400]})).is_err());
        assert!(s.execute("shelf.add", &json!({"cabinet": 2, "height": 100})).is_err());
        // Ящики в правой секции корпуса с перегородкой: короб по ширине секции.
        s.execute("cabinet.add", &json!({"name": "Тумба", "width": 1000, "height": 820, "depth": 560, "material": "ЛДСП Белый 16",
            "partitions": [392], "drawers": [200, 200], "drawer_section": 1, "doors": 0})).unwrap();
        let parts = build(&s.project).unwrap();
        let tumba: Vec<&Part> = parts.iter().filter(|p| p.cabinet == 3).collect();
        let bottom = tumba.iter().find(|p| p.panel.name == "Дно ящика").unwrap();
        // Секция 408…984 (576 мм) − 2·13 = 550; короб начинается у перегородки + 13.
        assert_eq!((bottom.panel.size.0, bottom.origin[0]), (550.0, 421.0));
        let facade = tumba.iter().find(|p| p.kind == Kind::Drawer).unwrap();
        // От середины перегородки (400) до правого края (1000), минус зазоры по 2.
        assert_eq!((facade.origin[0], facade.size[0]), (402.0, 596.0));
        assert!(s.execute("cabinet.set", &json!({"id": 3, "drawer_section": 5})).is_err());
        s.execute("cabinet.remove", &json!({"id": 3})).unwrap();

        s.execute("countertop.add", &json!({"material": "ЛДСП Белый 16", "length": 1400, "depth": 600,
            "position": [0, 2000, 0], "edge": "ПВХ 0,4 белая"})).unwrap();
        s.execute("cabinet.add", &json!({"name": "Навесной", "width": 600, "height": 720, "depth": 300,
            "material": "ЛДСП Белый 16", "doors": 1, "position": [0, 1400, 300]})).unwrap();
        let parts = build(&s.project).unwrap();
        let ct = parts.iter().find(|p| p.panel.name == "Столешница").unwrap();
        assert_eq!((ct.panel.size, ct.origin[1]), ((1400.0, 600.0), 2000.0));
        let wall = parts.iter().filter(|p| p.cabinet == 3).map(|p| p.origin[1]).fold(f64::MAX, f64::min);
        assert_eq!(wall, 1400.0);
        assert!(s.execute("countertop.set", &json!({"id": 1, "length": -5})).is_err());
    }

    #[test]
    fn iso_view_shows_three_faces_per_box() {
        let parts = build(&wardrobe().project).unwrap();
        let faces = iso_view(&parts, true);
        // Спереди-справа-сверху видны фронт, правая грань и верх каждой детали.
        assert_eq!(faces.len(), parts.len() * 3);
        assert_eq!(iso_view(&parts, false).len(), (parts.len() - 2) * 3);
    }

    #[test]
    fn corner_module() {
        let mut s = wardrobe();
        s.execute("corner.add", &json!({"width_x": 1000, "width_z": 900, "height": 820, "depth": 560, "material": "ЛДСП Белый 16",
            "edge": "ПВХ 0,4 белая", "plinth": 100, "back": "ХДФ 3", "shelves": [300], "doors": true, "position": [3000, 0, 0]})).unwrap();
        let parts: Vec<Part> = build(&s.project).unwrap().into_iter().filter(|p| p.origin[0] >= 2990.0 || p.panel.name.contains("углов")).collect();
        let bottom = parts.iter().find(|p| p.panel.name == "Дно угловое").unwrap();
        // Г-контур из 6 отрезков: это фигурная деталь (BOARDFIG), лицевая кромка на двух внутренних сторонах.
        assert_eq!(bottom.panel.segs.len(), 6);
        assert!(!bottom.panel.is_rect());
        assert_eq!(bottom.panel.size, (984.0, 884.0));
        assert_eq!(bottom.panel.edges.iter().map(|e| e.segment).collect::<Vec<_>>(), vec![3, 4]);
        assert_eq!(parts.iter().filter(|p| p.kind == Kind::Door).count(), 2);
        assert_eq!(parts.iter().filter(|p| p.kind == Kind::Back).count(), 2);
        // Двери не пересекаются во внутреннем углу.
        let doors: Vec<&Part> = parts.iter().filter(|p| p.kind == Kind::Door).collect();
        let overlap = (0..3).all(|k| doors[0].origin[k] < doors[1].origin[k] + doors[1].size[k] && doors[1].origin[k] < doors[0].origin[k] + doors[0].size[k]);
        assert!(!overlap, "двери пересекаются");
        assert!(s.execute("corner.set", &json!({"id": 1, "width_x": 600})).is_err(), "крыло меньше глубины");
        let xml = crate::raskroy::project_string("т", &parts.iter().map(|p| p.panel.clone()).collect::<Vec<_>>(), |_| (2800.0, 2070.0));
        assert_eq!(xml.matches("<BOARDFIG>").count(), 3, "дно, крыша, полка");
        s.execute("corner.remove", &json!({"id": 1})).unwrap();
        assert!(s.project.corners.is_empty());
    }

    #[test]
    fn save_load_roundtrip() {
        let s = wardrobe();
        let back = Project::load(&s.project.save()).unwrap();
        assert_eq!(back, s.project);
        assert!(Project::load(r#"{"format":"x","name":"a"}"#).is_err());
    }

    #[test]
    fn reports_work_on_own_model() {
        let parts = build(&wardrobe().project).unwrap();
        let panels: Vec<Panel> = parts.into_iter().map(|p| p.panel).collect();
        let rows = crate::panel::materials(&panels, |_| 1.0);
        let ldsp = rows.iter().find(|r| r.material.starts_with("ЛДСП")).unwrap();
        assert!(ldsp.qty > 4.0 && ldsp.qty < 8.0, "{}", ldsp.qty);
        let xml = crate::raskroy::project_string("Шкаф", &panels, |_| (2800.0, 2070.0));
        assert_eq!(xml.matches("<BOARDSMP>").count(), 11);
    }
}
