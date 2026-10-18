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
    ("material.remove", "{name} — удалить материал; отказ, если он используется"),
    ("edge.add", "{name, thickness, article?, width?}"),
    ("edge.remove", "{name} — удалить кромку; отказ, если она стоит на деталях"),
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
    ("panel.fill", "{material, plane, at: [x, y] точка в проёме на виде спереди, gaps?: [4 отступа], name?, kind?} → {id}. Панель во весь проём между ближайшими деталями (двери не в счёт), как «~» в Базисе. horizontal — нижней пластью на высоту y, gaps [слева, справа, сзади, спереди]; vertical — левой пластью на x, gaps [сзади, спереди, снизу, сверху]; front — вровень с передом проёма, gaps [слева, справа, снизу, сверху]"),
    ("panel.edge", "{id, segments: [номера торцов], edge: имя кромки | null, clip?: true — подрезать заготовку на толщину кромки, allowance?: припуск на прифуговку, мм} — кромка на торцах панели; null — снять"),
    ("panel.box", "{id, lo: [x,y,z], hi: [x,y,z]} — прямоугольная панель вдоль осей занимает габарит (маркеры размера); по толщине габарит = толщине"),
    ("panel.resize", "{id, size: [a, b]} — новые размеры прямоугольной панели (кромка сохраняется)"),
    ("panel.corner", "{id, at: [x, y] вершина контура, radius? | chamfer?} — скругление или фаска угла между двумя отрезками"),
    ("panel.cutout", "{id, center: [x, y], radius? | size?: [w, h]} — круглый или прямоугольный вырез"),
    ("panel.contour", "{id, segs: [{Line:{a,b}} | {Arc:{a,b,c,ccw}} | {Circle:{c,r}}]} — контур целиком (в осях панели); проверяется замкнутость и самопересечения"),
    ("object.move", "{ids: [номера], d: [dx, dy, dz]} — сдвиг корпусов, модулей, столешниц, панелей, блоков"),
    ("object.rotate", "{ids, axis: x|y|z, angle: градусы, center?: [x,y,z]} — поворот панелей и блоков (шаблоны — после «Разрушить»)"),
    ("object.mirror", "{ids, axis: x|y|z, at: координата плоскости, copy?: true — зеркальная копия} → {ids}"),
    ("object.copy", "{ids, d: [dx, dy, dz], count?: n} → {ids} — копии со сдвигом d, 2d, … (массив)"),
    ("object.remove", "{ids} — удалить объекты"),
    ("block.create", "{ids, name?} → {id} — собрать панели и блоки в блок"),
    ("block.explode", "{id} — разрушить блок: содержимое выходит в корень проекта"),
    ("fastener.add", "{kind: название крепежа, a, b: номера панелей со стыком (торец одной на пласти другой), positions?: [мм вдоль стыка], outside?: true — чашка эксцентрика снаружи} → {id}. Отверстия с координатами появятся на обеих панелях; справочник по умолчанию: «Конфирмат 7х50», «Эксцентрик Minifix 15», «Шкант 8х30», «Полкодержатель 5»"),
    ("fastener.auto", "{kind, ids?: [панели], outside?: true} → {ids} — крепёж на все стыки панелей (как «Крепёж на стыки» в Базисе). У корпусов и угловых модулей — параметр fastener"),
    ("fastener.set", "{id, outside?: true — чашка эксцентрика снаружи корпуса (по умолчанию внутри)}"),
    ("fastener.remove", "{id}"),
    ("door.add", "{at: [x, y] точка в проёме на виде спереди, material, mount?: overlay|half|inset (накладная по умолчанию), double?: true, gap?: 2, edge?, hinges?: left|right|none} → {id блока}. Дверь с петлями: чашка ⌀35 в двери, планка — в стойке; число петель по высоте"),
    ("drawers.add", "{at, material, heights: [фасады снизу вверх], mount?: overlay|half|inset, gap?: 2, depth?: длина направляющих, bottom?: материал дна, edge?} → {id блока}. Ящики в проём: короба, фасады, направляющие с присадкой в стойках"),
    ("furniture.add", "{path: файл .fr3d, position?: [x,y,z] мин. угол габарита, name?} → {id}. Модель фурнитуры: её отверстия садятся в панели, к которым она прижата; двигать — object.move/rotate"),
    ("fastener.def", "{name, holes: [{panel: a|b, at, at_t?, at_tb?, dir, diameter, depth, depth_t?}], offset, step, min, max} — свой крепёж в справочник (координаты в системе стыка: X внутрь панели b, Y по нормали b, Z вдоль стыка)"),
    ("panel.set", "{id, name?, material?, frame?: {origin, x, y, z}, ...поля} — панель дерева объектов"),
    ("panel.remove", "{id}"),
    ("block.set", "{id, name?, frame?} — блок дерева объектов"),
    ("block.remove", "{id} — блок со всем содержимым"),
    ("cabinet.explode", "{id} — разрушить корпус: детали станут панелями блока с тем же номером (так же corner.explode, countertop.explode)"),
    ("parts", "список деталей: имя, материал, длина, ширина, толщина, корпус"),
    ("check", "анализ модели: пересечения панелей, крепёж и фурнитура без панели, деталь больше листа → [{kind, message, parts}]"),
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

    /// Заменить проект целиком (итог скрипта): одна запись в истории отмены; проект должен строиться.
    pub fn apply_project(&mut self, next: Project) -> Result<(), String> {
        build(&next)?;
        self.undo.push(std::mem::replace(&mut self.project, next));
        self.redo.clear();
        Ok(())
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
            "check" => return serde_json::to_value(super::analysis::analyze(&self.project)?).map_err(|e| e.to_string()),
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
        "material.remove" => {
            let name = s(p, "name")?;
            let i = pr.materials.iter().position(|m| m.name == name).ok_or_else(|| format!("нет материала «{name}»"))?;
            pr.materials.remove(i);
            // Детали без своего материала не строятся — значит, он где-то используется.
            if build(pr).is_err() {
                return Err(format!("материал «{name}» используется в деталях — сначала замените его там"));
            }
        }
        "edge.remove" => {
            let name = s(p, "name")?;
            let i = pr.edges.iter().position(|e| e.name == name).ok_or_else(|| format!("нет кромки «{name}»"))?;
            let templates = pr.cabinets.iter().map(|c| &c.edge).chain(pr.corners.iter().map(|c| &c.edge)).chain(pr.countertops.iter().map(|c| &c.edge));
            let mut used = templates.flatten().any(|e| e == name);
            used |= pr.panels().iter().flat_map(|p| &p.edges).any(|e| e.material.split('\r').next() == Some(name));
            if used {
                return Err(format!("кромка «{name}» стоит на деталях — сначала снимите или замените её там"));
            }
            pr.edges.remove(i);
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
        "panel.fill" => return panel_fill(pr, p),
        "panel.edge" => return panel_edge(pr, p),
        "door.add" => return fronts::door(pr, p),
        "drawers.add" => return fronts::drawers(pr, p),
        "furniture.add" => {
            let path = s(p, "path")?;
            let m = crate::library::read(std::path::Path::new(path))?;
            let position: [f64; 3] = match p.get("position") {
                Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("position: {e}"))?,
                None => [0.0; 3],
            };
            let id = pr.next_id();
            let name = p.get("name").and_then(Value::as_str).map_or(m.name.clone(), str::to_string);
            // Мин. угол габарита модели встаёт в `position`.
            let origin = std::array::from_fn(|k| position[k] - m.lo[k]);
            pr.objects.push(Object::Furniture(Furniture { id, name, kind: m.kind, source: path.to_string(), frame: Frame { origin, ..Frame::IDENTITY }, holes: m.holes, lo: m.lo, hi: m.hi }));
            return Ok(json!({ "id": id }));
        }
        "fastener.add" | "fastener.remove" | "fastener.set" | "fastener.auto" | "fastener.def" => return fastener_cmd(pr, cmd, p),
        "panel.contour" | "panel.corner" | "panel.cutout" | "panel.resize" | "panel.box" => return shape::command(pr, cmd, p),
        "object.move" | "object.rotate" | "object.mirror" | "object.copy" | "object.remove" | "block.create" | "block.explode" => return actions::command(pr, cmd, p),
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
            grooves: part.panel.grooves,
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

/// Крепёж: `fastener.add` (между двумя панелями), `fastener.auto` (на все стыки панелей),
/// `fastener.remove`, `fastener.def` (добавить или заменить крепёж справочника).
fn fastener_cmd(pr: &mut Project, cmd: &str, p: &Value) -> Result<Value, String> {
    match cmd {
        "fastener.def" => {
            let d: crate::fasteners::Def = serde_json::from_value(p.clone()).map_err(|e| format!("крепёж: {e}"))?;
            if d.holes.is_empty() || !(d.step > 0.0 && d.offset >= 0.0 && d.min >= 1 && d.max >= d.min) {
                return Err("крепёж: нужны отверстия, шаг > 0, отступ ≥ 0, 1 ≤ min ≤ max".into());
            }
            let mut defs = pr.defs();
            defs.retain(|x| x.name != d.name);
            defs.push(d);
            pr.fastener_defs = defs;
            Ok(Value::Null)
        }
        "fastener.set" => {
            let id = idx(p, "id")? as u32;
            let f = pr.fasteners.iter_mut().find(|f| f.id == id).ok_or_else(|| format!("нет крепежа {id}"))?;
            if let Some(o) = p.get("outside") {
                f.outside = o.as_bool().ok_or("outside: true или false")?;
            }
            Ok(Value::Null)
        }
        "fastener.remove" => {
            let id = idx(p, "id")? as u32;
            let n = pr.fasteners.len();
            pr.fasteners.retain(|f| f.id != id);
            if n == pr.fasteners.len() { Err(format!("нет крепежа {id}")) } else { Ok(Value::Null) }
        }
        "fastener.add" => {
            let kind = s(p, "kind")?.to_string();
            pr.def(&kind)?;
            let (a, b) = (idx(p, "a")? as u32, idx(p, "b")? as u32);
            let parts = build(pr)?;
            let find = |id: u32| parts.iter().position(|q| q.id == id).ok_or_else(|| format!("нет панели {id}"));
            let (ia, ib) = (find(a)?, find(b)?);
            if crate::fasteners::joint(&parts, ia, ib).or_else(|| crate::fasteners::joint(&parts, ib, ia)).is_none() {
                return Err(format!("панели {a} и {b} не стыкуются: торец одной должен касаться пласти другой"));
            }
            let positions = match p.get("positions") {
                Some(v) => Some(serde_json::from_value(v.clone()).map_err(|e| format!("positions: {e}"))?),
                None => None,
            };
            let id = pr.next_id();
            let outside = p.get("outside").and_then(Value::as_bool).unwrap_or(false);
            pr.fasteners.push(FastenerInst { id, kind, a, b, positions, outside });
            Ok(json!({ "id": id }))
        }
        _ => {
            // fastener.auto: на все стыки среди панелей `ids` (или всех панелей дерева).
            let kind = s(p, "kind")?.to_string();
            let outside = p.get("outside").and_then(Value::as_bool).unwrap_or(false);
            pr.def(&kind)?;
            let parts = build(pr)?;
            let wanted: Option<Vec<u32>> = match p.get("ids") {
                Some(v) => Some(serde_json::from_value(v.clone()).map_err(|e| format!("ids: {e}"))?),
                None => None,
            };
            let ids: Vec<usize> = (0..parts.len()).filter(|&i| parts[i].id != 0 && wanted.as_ref().is_none_or(|w| w.contains(&parts[i].id))).collect();
            let mut added = Vec::new();
            for j in crate::fasteners::joints(&parts, &ids) {
                let (a, b) = (parts[j.a].id, parts[j.b].id);
                if pr.fasteners.iter().any(|f| (f.a, f.b) == (a, b) || (f.a, f.b) == (b, a)) {
                    continue;
                }
                let id = pr.next_id();
                pr.fasteners.push(FastenerInst { id, kind: kind.clone(), a, b, positions: None, outside });
                added.push(id);
            }
            Ok(json!({ "ids": added }))
        }
    }
}

/// `panel.edge`: кромка на торцах (сегментах контура) панели дерева; `edge: null` — снять.
fn panel_edge(pr: &mut Project, p: &Value) -> Result<Value, String> {
    let id = idx(p, "id")? as u32;
    let segments: Vec<usize> = serde_json::from_value(field(p, "segments")?.clone()).map_err(|e| format!("segments: {e}"))?;
    let edge = match field(p, "edge")? {
        Value::Null => None,
        v => {
            let name = v.as_str().ok_or("«edge» — имя кромки или null")?;
            Some(pr.edges.iter().find(|e| e.name == name).ok_or_else(|| format!("нет кромки «{name}»"))?.clone())
        }
    };
    let clip = p.get("clip").and_then(Value::as_bool).unwrap_or(true);
    let allowance = p.get("allowance").and_then(Value::as_f64).unwrap_or(0.0);
    if !(0.0..=50.0).contains(&allowance) {
        return Err(format!("припуск {allowance} мм вне 0…50"));
    }
    let Some(Object::Panel(fp)) = find_mut(&mut pr.objects, id) else {
        return Err(format!("нет панели {id}: кромку деталей корпуса меняют параметром «edge» корпуса или после «Разрушить»"));
    };
    if let Some(&bad) = segments.iter().find(|&&i| i >= fp.segs.len()) {
        return Err(format!("у панели {} торцов, торца {bad} нет", fp.segs.len()));
    }
    fp.edges.retain(|e| !segments.contains(&e.segment));
    if let Some(e) = edge {
        fp.edges.extend(segments.iter().map(|&segment| Edge {
            material: full(&e.name, &e.article),
            thickness: e.thickness,
            width: e.width,
            allowance,
            clip,
            segment,
        }));
        fp.edges.sort_by_key(|e| e.segment);
    }
    Ok(Value::Null)
}

/// `panel.fill`: панель в проёме у точки вида спереди (см. [`fill::fill`]).
fn panel_fill(pr: &mut Project, p: &Value) -> Result<Value, String> {
    let plane: Plane = serde_json::from_value(field(p, "plane")?.clone()).map_err(|e| format!("plane: {e}"))?;
    let at: [f64; 2] = serde_json::from_value(field(p, "at")?.clone()).map_err(|e| format!("at: {e}"))?;
    let gaps: [f64; 4] = match p.get("gaps") {
        Some(g) => serde_json::from_value(g.clone()).map_err(|e| format!("gaps: {e}"))?,
        None => [0.0; 4],
    };
    let t = pr.material(s(p, "material")?)?.thickness;
    let f = fill::fill(&build(pr)?, plane, at, t, gaps)?;
    let mut args = p.clone();
    let obj = args.as_object_mut().ok_or("параметры должны быть объектом")?;
    obj.insert("size".into(), json!(f.size));
    obj.insert("position".into(), json!(f.position));
    panel_add(pr, &args)
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
        grooves: Vec::new(),
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
