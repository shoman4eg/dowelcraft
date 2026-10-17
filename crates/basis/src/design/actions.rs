//! Действия с объектами (как гл. 14 руководства Базиса): сдвиг, поворот, зеркало, копирование массивом,
//! удаление; создание и разрушение блоков. Объект — шаблон (корпус, угловой модуль, столешница) или
//! объект дерева (панель, блок). Поворот и зеркало — только для объектов дерева: шаблон сначала «разрушают».

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Block, FreePanel, Object, Project, find_mut, remove};
use crate::contour::{self, Seg};
use crate::geom::Frame;

#[derive(Deserialize)]
struct Args {
    #[serde(default)]
    ids: Vec<u32>,
    #[serde(default)]
    d: [f64; 3],
    #[serde(default)]
    axis: Option<char>,
    #[serde(default)]
    angle: f64,
    #[serde(default)]
    center: [f64; 3],
    #[serde(default)]
    at: f64,
    #[serde(default)]
    copy: bool,
    #[serde(default = "one")]
    count: u32,
    #[serde(default)]
    name: String,
}

fn one() -> u32 {
    1
}

fn axis_index(a: Option<char>) -> Result<usize, String> {
    match a.map(|c| c.to_ascii_lowercase()) {
        Some('x') => Ok(0),
        Some('y') => Ok(1),
        Some('z') => Ok(2),
        _ => Err("axis: \"x\", \"y\" или \"z\"".into()),
    }
}

/// Убирает шум float: значения, отличающиеся от целого меньше чем на 1e-9, становятся целыми
/// (поворот на 90° даёт точные 0 и ±1, как у деталей вдоль осей).
fn clean(v: f64) -> f64 {
    let r = v.round();
    if (v - r).abs() < 1e-9 { r } else { v }
}

fn clean3(v: [f64; 3]) -> [f64; 3] {
    v.map(clean)
}

/// Сдвиг объекта на `d`.
fn shift(pr: &mut Project, id: u32, d: [f64; 3]) -> Result<(), String> {
    let add = |p: &mut [f64; 3]| (0..3).for_each(|k| p[k] = clean(p[k] + d[k]));
    if let Some(c) = pr.cabinets.iter_mut().find(|c| c.id == id) {
        add(&mut c.position);
    } else if let Some(c) = pr.corners.iter_mut().find(|c| c.id == id) {
        add(&mut c.position);
    } else if let Some(c) = pr.countertops.iter_mut().find(|c| c.id == id) {
        add(&mut c.position);
    } else {
        match find_mut(&mut pr.objects, id) {
            Some(Object::Panel(p)) => add(&mut p.frame.origin),
            Some(Object::Block(b)) => add(&mut b.frame.origin),
            Some(Object::Furniture(f)) => add(&mut f.frame.origin),
            None => return Err(format!("нет объекта {id}")),
        }
    }
    Ok(())
}

/// Поворот положения вокруг оси `axis` на `deg` градусов относительно точки `center`.
fn rotate_frame(f: &Frame, axis: usize, deg: f64, center: [f64; 3]) -> Frame {
    let (s, c) = deg.to_radians().sin_cos();
    let (s, c) = (clean(s), clean(c));
    let (i, j) = ((axis + 1) % 3, (axis + 2) % 3);
    let rot = |v: [f64; 3]| {
        let mut r = v;
        r[i] = c * v[i] - s * v[j];
        r[j] = s * v[i] + c * v[j];
        clean3(r)
    };
    let rel = std::array::from_fn(|k| f.origin[k] - center[k]);
    let o = rot(rel);
    Frame { origin: clean3(std::array::from_fn(|k| o[k] + center[k])), x: rot(f.x), y: rot(f.y), z: rot(f.z) }
}

fn tree_frame(pr: &mut Project, id: u32) -> Result<&mut Frame, String> {
    if pr.cabinets.iter().any(|c| c.id == id) || pr.corners.iter().any(|c| c.id == id) || pr.countertops.iter().any(|c| c.id == id) {
        return Err(format!("объект {id} — шаблон: поворот и зеркало доступны после «Разрушить»"));
    }
    match find_mut(&mut pr.objects, id) {
        Some(Object::Panel(p)) => Ok(&mut p.frame),
        Some(Object::Block(b)) => Ok(&mut b.frame),
        Some(Object::Furniture(f)) => Ok(&mut f.frame),
        None => Err(format!("нет объекта {id}")),
    }
}

/// Отражение панели плоскостью `axis = at` (в координатах родителя). Отражение меняет «руку» системы,
/// поэтому контур панели отражается по своей оси X: x → ширина − x, а ось X панели разворачивается.
fn mirror_panel(p: &mut FreePanel, axis: usize, at: f64) -> Result<(), String> {
    let w = contour::bounds(&p.segs).ok_or("пустой контур")?.1[0];
    let m = |v: [f64; 3]| { let mut r = v; r[axis] = -r[axis]; r };
    let f = p.frame;
    let (mx, my, mz) = (m(f.x), m(f.y), m(f.z));
    let mut origin = f.origin;
    origin[axis] = 2.0 * at - origin[axis];
    p.frame = Frame { origin: clean3(std::array::from_fn(|k| origin[k] + mx[k] * w)), x: clean3(mx.map(|v| -v)), y: clean3(my), z: clean3(mz) };
    let fx = |q: [f64; 2]| [clean(w - q[0]), q[1]];
    p.segs = p.segs.iter().map(|s| match *s {
        Seg::Line { a, b } => Seg::Line { a: fx(a), b: fx(b) },
        Seg::Arc { a, b, c, ccw } => Seg::Arc { a: fx(a), b: fx(b), c: fx(c), ccw: !ccw },
        Seg::Circle { c, r } => Seg::Circle { c: fx(c), r },
    }).collect();
    for h in &mut p.holes {
        h.pos[0] = clean(w - h.pos[0]);
        h.dir[0] = -h.dir[0];
    }
    // Паз: отражённая траектория в обратную сторону — чтобы сечение осталось с той же стороны.
    for g in &mut p.grooves {
        let [a, b] = g.path;
        g.path = [[clean(w - b[0]), b[1]], [clean(w - a[0]), a[1]]];
    }
    Ok(())
}

fn mirror_object(o: &mut Object, axis: usize, at: f64) -> Result<(), String> {
    match o {
        Object::Panel(p) => mirror_panel(p, axis, at),
        // Зеркальная фурнитура — другое изделие (левое/правое): её ставят из библиотеки.
        Object::Furniture(f) => Err(format!("«{}»: фурнитуру не отражают — поставьте зеркальное исполнение из библиотеки", f.name)),
        // Блок: отражаются его панели в координатах проекта, положение блока становится нулевым.
        Object::Block(b) => {
            let f = b.frame;
            b.frame = Frame::IDENTITY;
            for c in &mut b.children {
                compose(c, &f);
                mirror_object(c, axis, at)?;
            }
            Ok(())
        }
    }
}

/// Переносит объект из системы блока с положением `parent` в систему родителя блока.
fn compose(o: &mut Object, parent: &Frame) {
    match o {
        Object::Panel(p) => p.frame = parent.then(&p.frame),
        Object::Block(b) => b.frame = parent.then(&b.frame),
        Object::Furniture(f) => f.frame = parent.then(&f.frame),
    }
}

/// Копия объекта (шаблона или объекта дерева) с новыми номерами; возвращает номер копии.
fn duplicate(pr: &mut Project, id: u32) -> Result<u32, String> {
    let new = pr.next_id();
    if let Some(c) = pr.cabinets.iter().find(|c| c.id == id).cloned() {
        pr.cabinets.push(super::Cabinet { id: new, ..c });
    } else if let Some(c) = pr.corners.iter().find(|c| c.id == id).cloned() {
        pr.corners.push(super::Corner { id: new, ..c });
    } else if let Some(c) = pr.countertops.iter().find(|c| c.id == id).cloned() {
        pr.countertops.push(super::Countertop { id: new, ..c });
    } else {
        let mut o = find_mut(&mut pr.objects, id).ok_or_else(|| format!("нет объекта {id}"))?.clone();
        let mut next = new;
        renumber(&mut o, &mut next);
        pr.objects.push(o);
    }
    Ok(new)
}

fn renumber(o: &mut Object, next: &mut u32) {
    match o {
        Object::Panel(p) => {
            p.id = *next;
            *next += 1;
        }
        Object::Furniture(f) => {
            f.id = *next;
            *next += 1;
        }
        Object::Block(b) => {
            b.id = *next;
            *next += 1;
            b.children.iter_mut().for_each(|c| renumber(c, next));
        }
    }
}

fn remove_any(pr: &mut Project, id: u32) -> Result<(), String> {
    let before = (pr.cabinets.len(), pr.corners.len(), pr.countertops.len());
    pr.cabinets.retain(|c| c.id != id);
    pr.corners.retain(|c| c.id != id);
    pr.countertops.retain(|c| c.id != id);
    if before != (pr.cabinets.len(), pr.corners.len(), pr.countertops.len()) || remove(&mut pr.objects, id).is_some() {
        Ok(())
    } else {
        Err(format!("нет объекта {id}"))
    }
}

/// Команды `object.*` и `block.create` / `block.explode`.
pub fn command(pr: &mut Project, cmd: &str, p: &Value) -> Result<Value, String> {
    let a: Args = serde_json::from_value(p.clone()).map_err(|e| format!("{cmd}: {e}"))?;
    if a.ids.is_empty() && cmd != "block.explode" {
        return Err("ids: список номеров объектов пуст".into());
    }
    match cmd {
        "object.move" => {
            for &id in &a.ids {
                shift(pr, id, a.d)?;
            }
        }
        "object.rotate" => {
            let axis = axis_index(a.axis)?;
            for &id in &a.ids {
                let f = tree_frame(pr, id)?;
                *f = rotate_frame(f, axis, a.angle, a.center);
            }
        }
        "object.mirror" => {
            let axis = axis_index(a.axis)?;
            let mut out = Vec::new();
            for &id in &a.ids {
                tree_frame(pr, id)?;
                let target = if a.copy { duplicate(pr, id)? } else { id };
                let o = find_mut(&mut pr.objects, target).ok_or_else(|| format!("нет объекта {target}"))?;
                mirror_object(o, axis, a.at)?;
                out.push(target);
            }
            return Ok(json!({ "ids": out }));
        }
        "object.copy" => {
            if a.count == 0 || a.count > 500 {
                return Err(format!("count {} вне 1…500", a.count));
            }
            let mut out = Vec::new();
            for &id in &a.ids {
                for k in 1..=a.count {
                    let new = duplicate(pr, id)?;
                    shift(pr, new, a.d.map(|v| v * k as f64))?;
                    out.push(new);
                }
            }
            return Ok(json!({ "ids": out }));
        }
        "object.remove" => {
            for &id in &a.ids {
                remove_any(pr, id)?;
            }
        }
        "block.create" => {
            let mut children = Vec::new();
            for &id in &a.ids {
                if pr.cabinets.iter().any(|c| c.id == id) || pr.corners.iter().any(|c| c.id == id) || pr.countertops.iter().any(|c| c.id == id) {
                    return Err(format!("объект {id} — шаблон: в блок его кладут после «Разрушить»"));
                }
                // shortcut: объект из вложенного блока переносится в новый блок без учёта положения прежнего блока.
                children.push(remove(&mut pr.objects, id).ok_or_else(|| format!("нет объекта {id}"))?);
            }
            let id = pr.next_id();
            let name = if a.name.is_empty() { format!("Блок {id}") } else { a.name };
            pr.objects.push(Object::Block(Block { id, name, frame: Frame::IDENTITY, children }));
            return Ok(json!({ "id": id }));
        }
        "block.explode" => {
            let id = p.get("id").and_then(Value::as_u64).ok_or("нужен id блока")? as u32;
            if !matches!(find_mut(&mut pr.objects, id), Some(Object::Block(_))) {
                return Err(format!("нет блока {id}"));
            }
            let Some(Object::Block(b)) = remove(&mut pr.objects, id) else {
                return Err(format!("нет блока {id}"));
            };
            // shortcut: содержимое вложенного блока выносится в корень проекта, а не в родительский блок.
            for mut c in b.children {
                compose(&mut c, &b.frame);
                pr.objects.push(c);
            }
        }
        _ => return Err(format!("неизвестная команда «{cmd}»")),
    }
    Ok(Value::Null)
}
