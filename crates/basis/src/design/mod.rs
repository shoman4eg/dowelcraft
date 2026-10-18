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
use crate::geom::Frame;
use crate::panel::{BlockInfo, Edge, Hole, Panel, Texture};

pub use crate::view::{Face, FrontRect, bounds, front_view, iso_view};

mod commands;
pub mod fill;
pub mod shape;
pub mod actions;
pub mod fronts;
pub mod analysis;
pub use commands::{COMMANDS, Session};

/// Метка формата в файле проекта `.bcraft`.
pub const FORMAT: &str = "basis-craft/2";
/// Прежний формат: без дерева объектов (свободные детали — габаритом). Читается, если в нём нет свободных деталей.
const FORMAT_V1: &str = "basis-craft/1";

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
    /// Картинка текстуры для 3D (путь к файлу).
    #[serde(default)]
    pub texture: Option<String>,
    /// Размер плитки текстуры на детали, мм (длина вдоль волокон × ширина); нет — 1000 × 1000.
    #[serde(default)]
    pub texture_size: Option<[f64; 2]>,
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
    /// Крепёж стыков корпуса (название из справочника); полки — на полкодержатели. Нет — без присадки.
    #[serde(default)]
    pub fastener: Option<String>,
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
    /// Крепёж стыков модуля; полки — на полкодержатели.
    #[serde(default)]
    pub fastener: Option<String>,
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
/// Панель в дереве проекта: контур, материал, кромка, отверстия и точное положение.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FreePanel {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// Номер позиции (у импортированных — из исходной модели).
    #[serde(default)]
    pub pos: String,
    #[serde(default)]
    pub designation: String,
    pub material: String,
    /// Контур в осях панели, габарит от (0, 0).
    pub segs: Vec<Seg>,
    pub texture: Texture,
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Отверстия в координатах панели.
    #[serde(default)]
    pub holes: Vec<Hole>,
    /// Пазы (из модели Базиса).
    #[serde(default)]
    pub grooves: Vec<crate::panel::Groove>,
    /// Назначение — для цвета на видах.
    #[serde(default)]
    pub kind: Kind,
    /// Положение относительно родителя; толщина — вдоль +Z панели.
    pub frame: Frame,
}

/// Блок (сборочная единица): именованная группа объектов со своим положением.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Block {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub frame: Frame,
    #[serde(default)]
    pub children: Vec<Object>,
}

/// Модель фурнитуры (из `.fr3d`): в раскрой не идёт, сверлит свои отверстия в панелях, к которым
/// прижата, считается в смете по названию. Отверстия и габарит хранятся в проекте — файл может пропасть.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Furniture {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub kind: String,
    /// Файл, из которого поставлена модель.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub frame: Frame,
    #[serde(default)]
    pub holes: Vec<crate::panel::WorldHole>,
    /// Габарит в осях модели.
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

/// Объект дерева проекта.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Object {
    Panel(FreePanel),
    Block(Block),
    Furniture(Furniture),
}

impl Object {
    pub fn id(&self) -> u32 {
        match self {
            Object::Panel(p) => p.id,
            Object::Block(b) => b.id,
            Object::Furniture(f) => f.id,
        }
    }
}

/// Толщина панели дерева (из её материала).
fn panels_thickness(p: &Project, id: u32) -> Result<f64, String> {
    let fp = p.panels().into_iter().find(|f| f.id == id).ok_or_else(|| format!("нет панели {id}"))?;
    Ok(p.material(&fp.material)?.thickness)
}

/// Все панели дерева, в глубину.
fn walk_panels<'a>(objects: &'a [Object], out: &mut Vec<&'a FreePanel>) {
    for o in objects {
        match o {
            Object::Panel(p) => out.push(p),
            Object::Block(b) => walk_panels(&b.children, out),
            Object::Furniture(_) => {}
        }
    }
}

/// Объект с номером `id` где угодно в дереве.
fn find_mut(objects: &mut [Object], id: u32) -> Option<&mut Object> {
    for o in objects {
        if o.id() == id {
            return Some(o);
        }
        if let Object::Block(b) = o
            && let Some(found) = find_mut(&mut b.children, id)
        {
            return Some(found);
        }
    }
    None
}

/// Убирает объект с номером `id` из дерева.
fn remove(objects: &mut Vec<Object>, id: u32) -> Option<Object> {
    if let Some(i) = objects.iter().position(|o| o.id() == id) {
        return Some(objects.remove(i));
    }
    objects.iter_mut().find_map(|o| match o {
        Object::Block(b) => remove(&mut b.children, id),
        Object::Panel(_) | Object::Furniture(_) => None,
    })
}

fn max_id(objects: &[Object]) -> u32 {
    objects
        .iter()
        .map(|o| match o {
            Object::Panel(p) => p.id,
            Object::Furniture(f) => f.id,
            Object::Block(b) => b.id.max(max_id(&b.children)),
        })
        .max()
        .unwrap_or(0)
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
    /// Дерево панелей и блоков (свободные детали, импорт из Базиса).
    #[serde(default)]
    pub objects: Vec<Object>,
    /// Крепёж между панелями дерева.
    #[serde(default)]
    pub fasteners: Vec<FastenerInst>,
    /// Свой справочник крепежа; пусто — справочник по умолчанию ([`crate::fasteners::library`]).
    #[serde(default)]
    pub fastener_defs: Vec<crate::fasteners::Def>,
}

/// Крепёж на стыке двух панелей дерева: торец `b` на пласти `a` (порядок не важен — стык ищется в обе стороны).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FastenerInst {
    pub id: u32,
    /// Название крепежа из справочника.
    pub kind: String,
    pub a: u32,
    pub b: u32,
    /// Точки вдоль стыка, мм от его начала; нет — по правилу крепежа (отступ и шаг).
    #[serde(default)]
    pub positions: Option<Vec<f64>>,
    /// Чашка эксцентрика снаружи корпуса (по умолчанию — внутри).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outside: bool,
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
            objects: Vec::new(),
            fasteners: Vec::new(),
            fastener_defs: Vec::new(),
        }
    }

    /// Справочник крепежа проекта.
    pub fn defs(&self) -> Vec<crate::fasteners::Def> {
        if self.fastener_defs.is_empty() { crate::fasteners::library() } else { self.fastener_defs.clone() }
    }

    /// Крепёж по названию (без артикула или полностью).
    pub fn def(&self, name: &str) -> Result<crate::fasteners::Def, String> {
        self.defs()
            .into_iter()
            .find(|d| d.name == name || d.name.split('\r').next() == Some(name))
            .ok_or_else(|| format!("нет крепежа «{name}» в справочнике"))
    }

    pub fn load(json: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| format!("проект: {e}"))?;
        let format = v.get("format").and_then(Value::as_str).unwrap_or_default();
        if format == FORMAT_V1 && v.get("panels").and_then(Value::as_array).is_some_and(|a| !a.is_empty()) {
            return Err("проект старого формата с деталями из модели Базиса: импортируйте модель заново".into());
        }
        if format != FORMAT && format != FORMAT_V1 {
            return Err(format!("проект: неизвестный формат «{format}»"));
        }
        let mut p: Project = serde_json::from_value(v).map_err(|e| format!("проект: {e}"))?;
        p.format = FORMAT.into();
        build(&p)?;
        Ok(p)
    }

    /// Все панели дерева объектов, в глубину.
    pub fn panels(&self) -> Vec<&FreePanel> {
        let mut out = Vec::new();
        walk_panels(&self.objects, &mut out);
        out
    }

    /// Свободный номер: номера общие для корпусов, угловых модулей, столешниц и объектов дерева.
    pub fn next_id(&self) -> u32 {
        let c = self.cabinets.iter().map(|c| c.id);
        let k = self.corners.iter().map(|c| c.id);
        let t = self.countertops.iter().map(|c| c.id);
        let f = self.fasteners.iter().map(|f| f.id);
        c.chain(k).chain(t).chain(f).chain([max_id(&self.objects)]).max().unwrap_or(0) + 1
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

/// Проект из деталей модели Базиса: плиты — панели дерева с их точным положением, блоки Базиса — блоки.
/// Материалы — из встреченных (формат листа и коэффициент по умолчанию). Плиты без положения пропускаются.
/// `blocks` и `panel_block` — структура блоков из [`crate::panel::Extract`].
pub fn import(name: &str, panels: &[Panel], blocks: &[BlockInfo], panel_block: &[Option<usize>]) -> Project {
    let mut p = Project::new(name);
    // (материал Базиса, толщина) → имя материала проекта. Один материал разной толщины — разные материалы.
    let mut seen: Vec<((&str, f64), String)> = Vec::new();
    // Дерево собирается в списках детей: 0 — корень, далее — блоки в порядке появления.
    let mut kids: Vec<Vec<Object>> = vec![Vec::new()];
    let mut made: Vec<(usize, usize)> = Vec::new();
    let mut next_block = panels.len() as u32 + 1;
    let mut block_ids: Vec<(u32, String)> = vec![(0, String::new())];
    for (i, src) in panels.iter().enumerate() {
        let Some(frame) = src.frame else { continue };
        let (mat_name, article) = src.material.split_once('\r').unwrap_or((&src.material, ""));
        let key = match seen.iter().find(|(k, _)| *k == (src.material.as_str(), src.thickness)) {
            Some((_, name)) => name.clone(),
            None => {
                let taken = p.materials.iter().any(|m| m.name == mat_name);
                let key = if taken { format!("{mat_name} ({})", src.thickness) } else { mat_name.to_string() };
                p.materials.push(Material { name: key.clone(), article: article.into(), thickness: src.thickness, sheet: default_sheet(), coef: default_coef(), trim: None, kerf: None, texture: None, texture_size: None });
                seen.push(((src.material.as_str(), src.thickness), key.clone()));
                key
            }
        };
        // Цепочка блоков от корня до блока плиты; недостающие блоки создаются при первой встрече,
        // поэтому порядок деталей при обходе дерева совпадает с порядком в модели.
        let mut chain = Vec::new();
        let mut b = panel_block.get(i).copied().flatten();
        while let Some(bi) = b {
            chain.push(bi);
            b = blocks.get(bi).and_then(|x| x.parent);
        }
        let mut parent = 0;
        for &bi in chain.iter().rev() {
            parent = match made.iter().find(|(k, _)| *k == bi) {
                Some(&(_, slot)) => slot,
                None => {
                    let slot = kids.len();
                    kids.push(Vec::new());
                    block_ids.push((next_block, blocks.get(bi).map(|x| x.name.clone()).unwrap_or_default()));
                    next_block += 1;
                    kids[parent].push(Object::Block(Block { id: 0, name: String::new(), frame: Frame::IDENTITY, children: Vec::new() }));
                    made.push((bi, slot));
                    // Номер слота запоминаем в id заглушки, имя и детей подставим при сборке.
                    if let Some(Object::Block(stub)) = kids[parent].last_mut() {
                        stub.id = slot as u32;
                    }
                    slot
                }
            };
        }
        kids[parent].push(Object::Panel(FreePanel {
            id: i as u32 + 1,
            name: src.name.clone(),
            pos: src.pos.clone(),
            designation: src.designation.clone(),
            material: key,
            segs: src.segs.clone(),
            texture: src.texture,
            edges: src.edges.clone(),
            holes: src.holes.clone(),
            grooves: src.grooves.clone(),
            kind: guess_kind(src),
            frame,
        }));
    }
    fn assemble(slot: usize, kids: &mut Vec<Vec<Object>>, ids: &[(u32, String)]) -> Vec<Object> {
        let list = std::mem::take(&mut kids[slot]);
        list.into_iter()
            .map(|o| match o {
                Object::Block(stub) => {
                    let s = stub.id as usize;
                    let (id, name) = ids[s].clone();
                    Object::Block(Block { id, name, frame: Frame::IDENTITY, children: assemble(s, kids, ids) })
                }
                panel => panel,
            })
            .collect()
    }
    p.objects = assemble(0, &mut kids, &block_ids);
    p
}

/// Назначение детали — для отображения.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Body,
    Back,
    Door,
    /// Фасад ящика.
    Drawer,
}

/// Деталь изделия: плита для отчётов (с положением `panel.frame`) и её габарит в координатах проекта.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// Номер объекта верхнего уровня: корпуса, углового модуля, столешницы или объекта дерева.
    pub owner: u32,
    /// Номер панели дерева объектов; 0 — деталь шаблона (корпуса и т.п.) или модели Базиса.
    pub id: u32,
    pub kind: Kind,
    pub panel: Panel,
    /// Минимальный угол габарита, мм.
    pub origin: [f64; 3],
    /// Размеры габарита по X, Y, Z, мм.
    pub size: [f64; 3],
}

impl Part {
    /// Деталь с положением: габарит считается из положения, контура и толщины.
    fn new(owner: u32, kind: Kind, panel: Panel) -> Result<Part, String> {
        let (lo, hi) = panel.bounds().ok_or_else(|| format!("деталь «{}» без положения", panel.name))?;
        Ok(Part { owner, id: 0, kind, panel, origin: lo, size: std::array::from_fn(|k| hi[k] - lo[k]) })
    }

    /// Деталь генератора, заданная габаритом: плоскость определяется по тому, вдоль какой оси толщина.
    /// Фронтальная — контур X→X, Y→Y, толщина к зрителю; горизонтальная — X→X, Y→+Z (лицевая кромка — верх
    /// контура), толщина вниз; вертикальная — X→+Z (лицевая кромка — правая), Y→Y, толщина влево.
    fn placed(owner: u32, kind: Kind, mut panel: Panel, origin: [f64; 3], size: [f64; 3]) -> Result<Part, String> {
        panel.frame = Some(place(&panel, origin, size)?);
        let part = Part::new(owner, kind, panel)?;
        let same = (0..3).all(|k| (part.origin[k] - origin[k]).abs() < 1e-6 && (part.size[k] - size[k]).abs() < 1e-6);
        if !same {
            return Err(format!("деталь «{}»: положение не совпало с габаритом {origin:?} {size:?}", part.panel.name));
        }
        Ok(part)
    }
}

/// Положение панели по её габариту в проекте (см. [`Part::placed`]).
fn place(panel: &Panel, o: [f64; 3], size: [f64; 3]) -> Result<Frame, String> {
    let eq = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let (t, (sx, sy)) = (panel.thickness, panel.size);
    let (x, y, z) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    if eq(size[2], t) && eq(size[0], sx) && eq(size[1], sy) {
        Ok(Frame::new(o, x, y))
    } else if eq(size[1], t) && eq(size[0], sx) && eq(size[2], sy) {
        Ok(Frame::new([o[0], o[1] + t, o[2]], x, z))
    } else if eq(size[0], t) && eq(size[2], sx) && eq(size[1], sy) {
        Ok(Frame::new([o[0] + t, o[1], o[2]], z, y))
    } else {
        Err(format!("деталь «{}» {sx}×{sy}×{t} не ложится в габарит {size:?}", panel.name))
    }
}

/// Плоскость новой панели.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Plane {
    /// Фронтальная: в плоскости XY (ширина × высота).
    Front,
    /// Горизонтальная: в плоскости XZ (ширина × глубина).
    Horizontal,
    /// Вертикальная (профильная): в плоскости ZY (глубина × высота).
    Vertical,
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
        frame: None,
        holes: Vec::new(),
        grooves: Vec::new(),
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
    for o in &p.objects {
        objects(p, o.id(), std::slice::from_ref(o), &Frame::IDENTITY, &mut parts)?;
    }
    for ct in &p.countertops {
        countertop(p, ct, &mut parts).map_err(|e| format!("столешница {} «{}»: {e}", ct.id, ct.name))?;
    }
    drill(p, &mut parts)?;
    // Номера позиций — по порядку деталей в проекте; у импортированных деталей — свои.
    for (i, part) in parts.iter_mut().enumerate() {
        if part.panel.pos.is_empty() {
            part.panel.pos = (i + 1).to_string();
        }
    }
    Ok(parts)
}

/// Точки и стыки крепежа: (название крепежа, стык, точки). Для шаблонов — по их параметру «Крепёж»,
/// для панелей дерева — по экземплярам `fasteners`. Стык, которого больше нет, пропускается (его покажет анализ).
pub fn fastener_joints(p: &Project, parts: &[Part]) -> Result<Vec<(crate::fasteners::Def, crate::fasteners::Joint, Vec<f64>)>, String> {
    use crate::fasteners::{joints, positions};
    let mut out = Vec::new();
    let templates = p.cabinets.iter().map(|c| (c.id, &c.fastener)).chain(p.corners.iter().map(|c| (c.id, &c.fastener)));
    for (owner, fastener) in templates {
        let Some(name) = fastener else { continue };
        let main = p.def(name)?;
        let shelf = p.def("Полкодержатель 5").ok();
        // Короба ящиков, задние стенки и фасады — без крепежа корпуса.
        let ids: Vec<usize> = (0..parts.len())
            .filter(|&i| parts[i].owner == owner && parts[i].kind == Kind::Body && !parts[i].panel.name.contains("ящика"))
            .collect();
        for j in joints(parts, &ids) {
            let def = match (&shelf, parts[j.b].panel.name.starts_with("Полка")) {
                (Some(s), true) => s.clone(),
                _ => main.clone(),
            };
            let at = positions(&def, j.length);
            out.push((def, j, at));
        }
    }
    for f in &p.fasteners {
        let def = p.def(&f.kind)?;
        let find = |id: u32| parts.iter().position(|q| q.id == id);
        let (Some(a), Some(b)) = (find(f.a), find(f.b)) else { continue };
        let Some(mut j) = crate::fasteners::joint(parts, a, b).or_else(|| crate::fasteners::joint(parts, b, a)) else { continue };
        if f.outside {
            j.inward = -j.inward;
        }
        let at = f.positions.clone().unwrap_or_else(|| positions(&def, j.length));
        out.push((def, j, at));
    }
    Ok(out)
}

/// Присадка: отверстия крепежа — в детали. Отверстие за пределами детали — ошибка с объяснением.
/// Затем отверстия моделей фурнитуры садятся в панели, к которым модель прижата.
fn drill(p: &Project, parts: &mut [Part]) -> Result<(), String> {
    for (def, j, at) in fastener_joints(p, parts)? {
        for s in at {
            for (i, h) in crate::fasteners::holes(&def, parts, &j, s)? {
                parts[i].panel.holes.push(h);
            }
        }
    }
    furniture_drill(p, parts);
    Ok(())
}

/// Фурнитура дерева в координатах проекта.
pub fn furniture(p: &Project) -> Vec<(Furniture, Frame)> {
    fn walk(list: &[Object], parent: &Frame, out: &mut Vec<(Furniture, Frame)>) {
        for o in list {
            match o {
                Object::Furniture(f) => out.push((f.clone(), parent.then(&f.frame))),
                Object::Block(b) => walk(&b.children, &parent.then(&b.frame), out),
                Object::Panel(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&p.objects, &Frame::IDENTITY, &mut out);
    out
}

/// Сажает отверстия фурнитуры в детали; возвращает число отверстий, не нашедших панель (по фурнитуре).
fn furniture_drill(p: &Project, parts: &mut [Part]) -> Vec<(String, usize)> {
    let mut panels: Vec<Panel> = parts.iter().map(|q| Panel { holes: Vec::new(), ..q.panel.clone() }).collect();
    let mut lost = Vec::new();
    for (f, fr) in furniture(p) {
        let world: Vec<crate::panel::WorldHole> = f
            .holes
            .iter()
            .map(|h| crate::panel::WorldHole { pos: fr.apply(h.pos), dir: fr.dir(h.dir), ..*h })
            .collect();
        let n = crate::panel::assign(&mut panels, &world);
        if n > 0 {
            lost.push((f.name.clone(), n));
        }
    }
    for (part, panel) in parts.iter_mut().zip(panels) {
        part.panel.holes.extend(panel.holes);
    }
    lost
}

/// Отверстия фурнитуры, не попавшие ни в одну панель: (фурнитура, число) — для анализа модели.
pub fn furniture_lost_holes(p: &Project) -> Vec<(String, usize)> {
    let Ok(mut parts) = build(p) else { return Vec::new() };
    parts.iter_mut().for_each(|q| q.panel.holes.clear());
    furniture_drill(p, &mut parts)
}

/// Крепёж для сметы: (название, штук).
pub fn hardware(p: &Project) -> Result<Vec<(String, usize)>, String> {
    let parts = build(p)?;
    let mut out: Vec<(String, usize)> = Vec::new();
    let fasteners = fastener_joints(p, &parts)?.into_iter().flat_map(|(def, _, at)| {
        let per: Vec<(String, usize)> = if def.items.is_empty() { vec![(def.name, 1)] } else { def.items };
        per.into_iter().map(move |(name, k)| (name, k * at.len()))
    });
    let items = fasteners.chain(furniture(p).into_iter().map(|(f, _)| (f.name, 1)));
    for (name, n) in items {
        match out.iter_mut().find(|(k, _)| *k == name) {
            Some(r) => r.1 += n,
            None => out.push((name, n)),
        }
    }
    out.sort();
    Ok(out)
}

/// Крепёж, чей стык пропал (панели раздвинули или удалили): номера экземпляров и причина.
pub fn hanging_fasteners(p: &Project) -> Vec<(u32, String)> {
    let Ok(parts) = build(p) else { return Vec::new() };
    p.fasteners
        .iter()
        .filter_map(|f| {
            let find = |id: u32| parts.iter().position(|q| q.id == id);
            match (find(f.a), find(f.b)) {
                (Some(a), Some(b)) if crate::fasteners::joint(&parts, a, b).or_else(|| crate::fasteners::joint(&parts, b, a)).is_some() => None,
                (Some(_), Some(_)) => Some((f.id, format!("«{}» {}: панели {} и {} больше не стыкуются", f.kind, f.id, f.a, f.b))),
                _ => Some((f.id, format!("«{}» {}: нет панели {} или {}", f.kind, f.id, f.a, f.b))),
            }
        })
        .collect()
}

/// Панели поддерева в координатах проекта.
fn objects(p: &Project, owner: u32, list: &[Object], parent: &Frame, out: &mut Vec<Part>) -> Result<(), String> {
    for o in list {
        match o {
            Object::Block(b) => {
                b.frame.check().map_err(|e| format!("блок {} «{}»: {e}", b.id, b.name))?;
                objects(p, owner, &b.children, &parent.then(&b.frame), out)?;
            }
            // Фурнитура деталей не даёт; её отверстия сажает `drill`.
            Object::Furniture(f) => f.frame.check().map_err(|e| format!("фурнитура {} «{}»: {e}", f.id, f.name))?,
            Object::Panel(fp) => {
                let err = |e: String| format!("деталь {} «{}»: {e}", fp.id, fp.name);
                let m = p.material(&fp.material).map_err(err)?;
                fp.frame.check().map_err(err)?;
                let (lo, hi) = crate::contour::bounds(&fp.segs).ok_or_else(|| err("пустой контур".into()))?;
                if lo[0].abs() > 1e-6 || lo[1].abs() > 1e-6 {
                    return Err(err(format!("габарит контура должен начинаться в (0, 0), а начинается в {lo:?}")));
                }
                let panel = Panel {
                    name: fp.name.clone(),
                    pos: fp.pos.clone(),
                    designation: fp.designation.clone(),
                    material: full(&m.name, &m.article),
                    thickness: m.thickness,
                    segs: fp.segs.clone(),
                    size: (hi[0], hi[1]),
                    texture: fp.texture,
                    edges: fp.edges.clone(),
                    frame: Some(parent.then(&fp.frame)),
                    holes: fp.holes.clone(),
                    grooves: fp.grooves.clone(),
                };
                out.push(Part { id: fp.id, ..Part::new(owner, fp.kind, panel)? });
            }
        }
    }
    Ok(())
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
    let mut add = |kind: Kind, panel: Panel, origin: [f64; 3], size: [f64; 3]| -> Result<(), String> {
        out.push(Part::placed(c.id, kind, panel, [o[0] + origin[0], o[1] + origin[1], o[2] + origin[2]], size)?);
        Ok(())
    };
    // Вертикальные детали: контур X — глубина (от задней кромки к лицевой), Y — высота, текстура вдоль высоты.
    let vertical = |name: &str, height: f64| rect(name, m, d, height, Texture::Vertical, &[RIGHT], edge);
    // Горизонтальные: X — длина, Y — глубина (лицевая кромка — верх контура), текстура вдоль длины.
    let horizontal = |name: &str, length: f64| rect(name, m, length, d, Texture::Horizontal, &[TOP], edge);

    add(Kind::Body, vertical("Боковина левая", h), [0.0, 0.0, 0.0], [t, h, d])?;
    add(Kind::Body, vertical("Боковина правая", h), [w - t, 0.0, 0.0], [t, h, d])?;
    add(Kind::Body, horizontal("Дно", inner_w), [t, c.plinth, 0.0], [inner_w, t, d])?;
    add(Kind::Body, horizontal("Крыша", inner_w), [t, h - t, 0.0], [inner_w, t, d])?;
    if c.plinth > 0.0 {
        add(Kind::Body, rect("Цоколь", m, inner_w, c.plinth, Texture::Horizontal, &[], None), [t, 0.0, d - t], [inner_w, c.plinth, t])?;
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
        add(Kind::Body, vertical("Перегородка", inner_h), [x, c.plinth + t, 0.0], [t, inner_h, d])?;
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
        add(Kind::Body, horizontal("Полка", sw), [sx, c.plinth + t + s.height, 0.0], [sw, t, d])?;
    }

    if let Some(name) = &c.back {
        let bm = p.material(name)?;
        let bh = h - c.plinth;
        add(Kind::Back, rect("Задняя стенка", bm, w, bh, Texture::Default, &[], None), [0.0, c.plinth, -bm.thickness], [w, bh, bm.thickness])?;
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
        add(Kind::Drawer, facade, [fx0 + g, front_y, d], [fw, hf, dm.thickness])?;
        let (bx, by, bz) = (dsx + RAIL_GAP, front_y + 20.0, d - bd);
        // Короб: боковины (X — глубина, Y — высота), стенки между боковинами, дно под коробом.
        let side = |name| rect(name, m, bd, bh, Texture::Horizontal, &[TOP], edge);
        add(Kind::Body, side("Боковина ящика"), [bx, by, bz], [t, bh, bd])?;
        add(Kind::Body, side("Боковина ящика"), [bx + bw - t, by, bz], [t, bh, bd])?;
        let wall = |name| rect(name, m, bw - 2.0 * t, bh, Texture::Horizontal, &[TOP], edge);
        add(Kind::Body, wall("Передняя стенка ящика"), [bx + t, by, d - t], [bw - 2.0 * t, bh, t])?;
        add(Kind::Body, wall("Задняя стенка ящика"), [bx + t, by, bz], [bw - 2.0 * t, bh, t])?;
        let bm = match &c.back {
            Some(n) => p.material(n)?,
            None => m,
        };
        add(Kind::Body, rect("Дно ящика", bm, bw, bd, Texture::Default, &[], None), [bx, by - bm.thickness, bz], [bw, bm.thickness, bd])?;
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
            add(Kind::Door, door, [x, front_y, d], [dw, dh, dm.thickness])?;
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
    let mut add = |kind: Kind, panel: Panel, origin: [f64; 3], size: [f64; 3]| -> Result<(), String> {
        out.push(Part::placed(c.id, kind, panel, [o[0] + origin[0], o[1] + origin[1], o[2] + origin[2]], size)?);
        Ok(())
    };
    // Г-образные горизонтали в плане (X, Z): лицевые кромки — внутренний угол (сегменты 3 и 4).
    // shortcut: на видах Г-деталь рисуется своим габаритом (квадратом) — для точного 3D нужны многогранники.
    let l_shape = |name| poly(name, m, &[[ax, 0.0], [0.0, 0.0], [0.0, bz], [d, bz], [d, d], [ax, d]], Texture::Horizontal, &[3, 4], edge);
    let side = |name| rect(name, m, d, h, Texture::Vertical, &[RIGHT], edge);
    add(Kind::Body, side("Боковина крыла X"), [ax, 0.0, 0.0], [t, h, d])?;
    add(Kind::Body, side("Боковина крыла Z"), [0.0, 0.0, bz], [d, h, t])?;
    add(Kind::Body, l_shape("Дно угловое"), [0.0, c.plinth, 0.0], [ax, t, bz])?;
    add(Kind::Body, l_shape("Крыша угловая"), [0.0, h - t, 0.0], [ax, t, bz])?;
    for &sh in &c.shelves {
        if sh < 0.0 || sh + t > inner_h {
            return Err(format!("полка на высоте {sh} мм не помещается (внутри {inner_h} мм)"));
        }
        add(Kind::Body, l_shape("Полка угловая"), [0.0, c.plinth + t + sh, 0.0], [ax, t, bz])?;
    }
    if c.plinth > 0.0 {
        add(Kind::Body, rect("Цоколь", m, ax - d + t, c.plinth, Texture::Horizontal, &[], None), [d - t, 0.0, d - t], [ax - d + t, c.plinth, t])?;
        add(Kind::Body, rect("Цоколь", m, bz - d, c.plinth, Texture::Horizontal, &[], None), [d - t, 0.0, d], [t, c.plinth, bz - d])?;
    }
    if let Some(name) = &c.back {
        let bm = p.material(name)?;
        let bh = h - c.plinth;
        add(Kind::Back, rect("Задняя стенка", bm, c.width_x, bh, Texture::Default, &[], None), [0.0, c.plinth, -bm.thickness], [c.width_x, bh, bm.thickness])?;
        add(Kind::Back, rect("Задняя стенка", bm, c.width_z, bh, Texture::Default, &[], None), [-bm.thickness, c.plinth, 0.0], [bm.thickness, bh, c.width_z])?;
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
        add(Kind::Door, door(dwx), [d + dm.thickness + g, c.plinth + g, d], [dwx, dh, dm.thickness])?;
        add(Kind::Door, door(dwz), [d, c.plinth + g, d + g], [dm.thickness, dh, dwz])?;
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
    out.push(Part::placed(ct.id, Kind::Body, panel, ct.position, [ct.length, m.thickness, ct.depth])?);
    Ok(())
}

#[cfg(test)]
mod tests;
