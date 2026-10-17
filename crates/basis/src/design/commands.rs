//! Команды проекта: `Session::execute(команда, JSON)` с отменой и возвратом.

use super::*;

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
    ("panel.add", "{material, plane: front|horizontal|vertical, size: [a, b], position?: [x,y,z] мин. угол габарита, name?, texture?: Horizontal|Vertical|Default, kind?: body|back|door|drawer, block?} → {id}. Размеры контура: front — ширина×высота, horizontal — ширина×глубина, vertical — глубина×высота; толщина — из материала"),
    ("panel.set", "{id, name?, material?, frame?: {origin, x, y, z}, ...поля} — панель дерева объектов"),
    ("panel.remove", "{id}"),
    ("block.set", "{id, name?, frame?} — блок дерева объектов"),
    ("block.remove", "{id} — блок со всем содержимым"),
    ("cabinet.explode", "{id} — разрушить корпус: детали станут панелями блока с тем же номером (так же corner.explode, countertop.explode)"),
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
                                "pos": part.panel.pos, "name": part.panel.name, "owner": part.owner,
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
impl Item for Corner {
    fn id(&self) -> u32 {
        self.id
    }
    fn name(&mut self) -> &mut String {
        &mut self.name
    }
}

/// Команды `*.add` (→ {id}), `*.set` {id, ...поля}, `*.remove` {id} над списком элементов.
fn item<T: Item>(list: &mut Vec<T>, next_id: u32, cmd: &str, p: &Value, what: &str) -> Result<Value, String> {
    if cmd.ends_with(".add") {
        let id = next_id;
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
            let id = pr.next_id();
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
        "countertop.add" | "countertop.set" | "countertop.remove" => {
            let id = pr.next_id();
            return item(&mut pr.countertops, id, cmd, p, "Столешница");
        }
        "corner.add" | "corner.set" | "corner.remove" => {
            let id = pr.next_id();
            return item(&mut pr.corners, id, cmd, p, "Угловой модуль");
        }
        "panel.add" => return panel_add(pr, p),
        "cabinet.explode" | "corner.explode" | "countertop.explode" => return explode(pr, idx(p, "id")? as u32),
        "panel.set" | "block.set" => {
            let id = idx(p, "id")? as u32;
            match find_mut(&mut pr.objects, id) {
                Some(Object::Panel(x)) if cmd == "panel.set" => *x = patch(&*x, p, "id")?,
                Some(Object::Block(x)) if cmd == "block.set" => {
                    let children = std::mem::take(&mut x.children);
                    *x = patch(&*x, p, "id")?;
                    x.children = children;
                }
                _ => return Err(format!("нет {} {id}", if cmd == "panel.set" { "детали" } else { "блока" })),
            }
        }
        "panel.remove" | "block.remove" => {
            let id = idx(p, "id")? as u32;
            let want_block = cmd == "block.remove";
            match find_mut(&mut pr.objects, id) {
                Some(Object::Block(_)) if want_block => {}
                Some(Object::Panel(_)) if !want_block => {}
                _ => return Err(format!("нет {} {id}", if want_block { "блока" } else { "детали" })),
            }
            remove(&mut pr.objects, id);
        }
        _ => return Err(format!("неизвестная команда «{cmd}»")),
    }
    Ok(Value::Null)
}

/// Разрушение шаблона (корпуса, углового модуля, столешницы): его детали становятся панелями блока
/// с тем же номером и именем. Дальше их правят по одной, параметры шаблона больше не действуют.
fn explode(pr: &mut Project, id: u32) -> Result<Value, String> {
    let name = (pr.cabinets.iter().find(|c| c.id == id).map(|c| c.name.clone()))
        .or_else(|| pr.corners.iter().find(|c| c.id == id).map(|c| c.name.clone()))
        .or_else(|| pr.countertops.iter().find(|c| c.id == id).map(|c| c.name.clone()))
        .ok_or_else(|| format!("нет корпуса, углового модуля или столешницы {id}"))?;
    let parts: Vec<Part> = build(pr)?.into_iter().filter(|p| p.owner == id).collect();
    let mut children = Vec::new();
    for (next, part) in (pr.next_id()..).zip(parts) {
        let m = pr
            .materials
            .iter()
            .find(|m| full(&m.name, &m.article) == part.panel.material && m.thickness == part.panel.thickness)
            .ok_or_else(|| format!("нет материала «{}»", part.panel.material))?;
        children.push(Object::Panel(FreePanel {
            id: next,
            name: part.panel.name,
            pos: String::new(),
            designation: part.panel.designation,
            material: m.name.clone(),
            segs: part.panel.segs,
            texture: part.panel.texture,
            edges: part.panel.edges,
            holes: part.panel.holes,
            kind: part.kind,
            frame: part.panel.frame.ok_or("деталь без положения")?,
        }));
    }
    pr.cabinets.retain(|c| c.id != id);
    pr.corners.retain(|c| c.id != id);
    pr.countertops.retain(|c| c.id != id);
    pr.objects.push(Object::Block(Block { id, name, frame: Frame::IDENTITY, children }));
    Ok(json!({ "id": id }))
}

/// `panel.add`: прямоугольная панель в одной из трёх плоскостей, `position` — минимальный угол габарита.
fn panel_add(pr: &mut Project, p: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Args {
        material: String,
        plane: Plane,
        /// Размеры контура: фронтальная — ширина × высота, горизонтальная — ширина × глубина,
        /// вертикальная — глубина × высота.
        size: [f64; 2],
        #[serde(default)]
        position: [f64; 3],
        #[serde(default)]
        name: String,
        #[serde(default)]
        texture: Option<Texture>,
        #[serde(default)]
        kind: Kind,
        /// Блок, в который положить панель; нет — в корень.
        #[serde(default)]
        block: Option<u32>,
    }
    let a: Args = serde_json::from_value(p.clone()).map_err(|e| format!("панель: {e}"))?;
    let [sx, sy] = a.size;
    if !(sx > 0.0 && sy > 0.0 && sx.is_finite() && sy.is_finite()) {
        return Err(format!("размеры панели {sx}×{sy} должны быть больше нуля"));
    }
    let m = pr.material(&a.material)?.clone();
    let t = m.thickness;
    let (box_size, texture) = match a.plane {
        Plane::Front => ([sx, sy, t], Texture::Horizontal),
        Plane::Horizontal => ([sx, t, sy], Texture::Horizontal),
        Plane::Vertical => ([t, sy, sx], Texture::Vertical),
    };
    let texture = a.texture.unwrap_or(texture);
    let panel = rect(&a.name, &m, sx, sy, texture, &[], None);
    let frame = place(&panel, a.position, box_size)?;
    let id = pr.next_id();
    let name = if a.name.is_empty() {
        match a.plane {
            Plane::Front => format!("Фронтальная {id}"),
            Plane::Horizontal => format!("Горизонтальная {id}"),
            Plane::Vertical => format!("Вертикальная {id}"),
        }
    } else {
        a.name
    };
    let fp = FreePanel {
        id,
        name,
        pos: String::new(),
        designation: String::new(),
        material: a.material,
        segs: panel.segs,
        texture,
        edges: Vec::new(),
        holes: Vec::new(),
        kind: a.kind,
        frame,
    };
    match a.block {
        None => pr.objects.push(Object::Panel(fp)),
        Some(b) => match find_mut(&mut pr.objects, b) {
            // shortcut: положение задаётся в координатах блока; блоки импорта стоят в начале координат.
            Some(Object::Block(block)) => block.children.push(Object::Panel(fp)),
            _ => return Err(format!("нет блока {b}")),
        },
    }
    Ok(json!({ "id": id }))
}
