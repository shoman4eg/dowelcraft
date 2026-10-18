//! Panel contour editing: contour replacement, corner fillet and chamfer, cut-outs, resizing.
//! After each edit the contour is validated ([`contour::validate`]) and shifted so that the bounding box
//! starts at (0, 0) while the part itself doesn't move in space.

use serde::Deserialize;
use serde_json::Value;

use super::{FreePanel, Object, Project, find_mut};
use crate::contour::{self, Seg};

const EPS: f64 = 1e-6;

fn near(p: [f64; 2], q: [f64; 2]) -> bool {
    (p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3
}

fn shift(s: Seg, d: [f64; 2]) -> Seg {
    let m = |p: [f64; 2]| [p[0] - d[0], p[1] - d[1]];
    match s {
        Seg::Line { a, b } => Seg::Line { a: m(a), b: m(b) },
        Seg::Arc { a, b, c, ccw } => Seg::Arc { a: m(a), b: m(b), c: m(c), ccw },
        Seg::Circle { c, r } => Seg::Circle { c: m(c), r },
    }
}

/// New panel contour: validate, shift to (0, 0) moving the axes origin, drop edge banding from edges that no longer exist.
pub fn set_contour(fp: &mut FreePanel, segs: Vec<Seg>) -> Result<(), String> {
    contour::validate(&segs)?;
    let (lo, _) = contour::bounds(&segs).ok_or("пустой контур")?;
    fp.frame.origin = fp.frame.apply([lo[0], lo[1], 0.0]);
    fp.segs = segs.into_iter().map(|s| shift(s, lo)).collect();
    let n = fp.segs.len();
    fp.edges.retain(|e| e.segment < n);
    Ok(())
}

/// Fillet (`radius`) or chamfer (`chamfer` — leg length) of the corner at vertex `at` between two line segments.
/// The new edge gets edge banding if it is the same on both neighbours.
pub fn corner(fp: &FreePanel, at: [f64; 2], radius: Option<f64>, chamfer: Option<f64>) -> Result<(Vec<Seg>, Vec<crate::panel::Edge>), String> {
    let size = radius.or(chamfer).ok_or("нужен radius или chamfer")?;
    if !(size > 0.0 && size.is_finite()) {
        return Err(format!("размер скругления/фаски {size} должен быть больше нуля"));
    }
    // Traversal order: the first edge ends at the vertex, the second starts there.
    let lp = contour::loops(&fp.segs)?.into_iter().find(|lp| lp.iter().any(|&(i, _)| {
        let (a, b) = contour::ends(&fp.segs[i]);
        near(a, at) || near(b, at)
    })).ok_or_else(|| format!("в точке {at:?} нет вершины контура"))?;
    let end = |&(i, rev): &(usize, bool)| { let (a, b) = contour::ends(&fp.segs[i]); if rev { a } else { b } };
    let k = lp.iter().position(|x| near(end(x), at)).ok_or_else(|| format!("в точке {at:?} нет вершины контура"))?;
    let (first, second) = (lp[k], lp[(k + 1) % lp.len()]);
    let line = |(i, rev): (usize, bool)| match fp.segs[i] {
        Seg::Line { a, b } => Ok(if rev { (b, a) } else { (a, b) }),
        _ => Err("скруглить или срезать можно только угол между двумя отрезками".to_string()),
    };
    let ((a, v), (_, b)) = (line(first)?, line(second)?);
    let unit = |p: [f64; 2]| { let l = (p[0] - v[0]).hypot(p[1] - v[1]); ([(p[0] - v[0]) / l, (p[1] - v[1]) / l], l) };
    let ((u1, l1), (u2, l2)) = (unit(a), unit(b));
    let cos = (u1[0] * u2[0] + u1[1] * u2[1]).clamp(-1.0, 1.0);
    let theta = cos.acos();
    if theta < 1e-6 || (std::f64::consts::PI - theta) < 1e-6 {
        return Err("торцы в этой вершине лежат на одной прямой".into());
    }
    let d = if radius.is_some() { size / (theta / 2.0).tan() } else { size };
    if d >= l1 - EPS || d >= l2 - EPS {
        return Err(format!("{size} мм не помещается: торцы у вершины {l1:.1} и {l2:.1} мм"));
    }
    let (p1, p2) = ([v[0] + u1[0] * d, v[1] + u1[1] * d], [v[0] + u2[0] * d, v[1] + u2[1] * d]);
    let mut segs = fp.segs.clone();
    // The edge end lying at the vertex moves to the tangent point.
    let mv = |s: &mut Seg, to: [f64; 2]| if let Seg::Line { a, b } = s { if near(*a, v) { *a = to } else { *b = to } };
    mv(&mut segs[first.0], p1);
    mv(&mut segs[second.0], p2);
    segs.push(match radius {
        Some(r) => {
            let bis = [u1[0] + u2[0], u1[1] + u2[1]];
            let bl = bis[0].hypot(bis[1]);
            let dist = r / (theta / 2.0).sin();
            let c = [v[0] + bis[0] / bl * dist, v[1] + bis[1] / bl * dist];
            let cross = (p1[0] - c[0]) * (p2[1] - c[1]) - (p1[1] - c[1]) * (p2[0] - c[0]);
            Seg::Arc { a: p1, b: p2, c, ccw: cross > 0.0 }
        }
        None => Seg::Line { a: p1, b: p2 },
    });
    let mut edges = fp.edges.clone();
    let on = |i: usize| fp.edges.iter().find(|e| e.segment == i);
    if let (Some(e1), Some(e2)) = (on(first.0), on(second.0))
        && e1.material == e2.material
    {
        edges.push(crate::panel::Edge { segment: segs.len() - 1, ..e1.clone() });
    }
    Ok((segs, edges))
}

/// Cut-out: a circle of radius `radius` or a rectangle `size` centered at `center`.
pub fn cutout(fp: &FreePanel, center: [f64; 2], radius: Option<f64>, size: Option<[f64; 2]>) -> Result<Vec<Seg>, String> {
    let mut segs = fp.segs.clone();
    match (radius, size) {
        (Some(r), _) if r > 0.0 => segs.push(Seg::Circle { c: center, r }),
        (None, Some([w, h])) if w > 0.0 && h > 0.0 => {
            let (x0, y0, x1, y1) = (center[0] - w / 2.0, center[1] - h / 2.0, center[0] + w / 2.0, center[1] + h / 2.0);
            let p = [[x0, y0], [x0, y1], [x1, y1], [x1, y0]];
            segs.extend((0..4).map(|i| Seg::Line { a: p[i], b: p[(i + 1) % 4] }));
        }
        _ => return Err("вырез: нужен radius > 0 или size: [ширина, высота] > 0".into()),
    }
    Ok(segs)
}

/// New size of a rectangular panel: vertices on the right and top sides are moved,
/// edge order and edge banding are preserved.
pub fn resize(fp: &FreePanel, size: [f64; 2]) -> Result<Vec<Seg>, String> {
    let (lo, hi) = contour::bounds(&fp.segs).ok_or("пустой контур")?;
    let rect = fp.segs.len() == 4
        && fp.segs.iter().all(|s| matches!(s, Seg::Line { a, b } if (a[0] - b[0]).abs() < EPS || (a[1] - b[1]).abs() < EPS))
        && lo == [0.0, 0.0];
    if !rect {
        return Err("размеры меняются только у прямоугольной панели; фигурную правьте контуром".into());
    }
    if !(size[0] > 0.0 && size[1] > 0.0) {
        return Err(format!("размеры {size:?} должны быть больше нуля"));
    }
    let m = |p: [f64; 2]| [if (p[0] - hi[0]).abs() < EPS { size[0] } else { p[0] }, if (p[1] - hi[1]).abs() < EPS { size[1] } else { p[1] }];
    Ok(fp.segs.iter().map(|s| match *s {
        Seg::Line { a, b } => Seg::Line { a: m(a), b: m(b) },
        other => other,
    }).collect())
}

/// A rectangular axis-aligned panel occupies the bounding box [lo, hi] (dimension markers): along contour axes —
/// new size; along the thickness the box must match the thickness; the axes origin shifts so that
/// the bounding box lands exactly on [lo, hi].
pub fn fit_box(fp: &mut FreePanel, thickness: f64, lo: [f64; 3], hi: [f64; 3]) -> Result<(), String> {
    let f = fp.frame;
    let axis = |v: [f64; 3]| (0..3).find(|&k| (v[k].abs() - 1.0).abs() < 1e-9).ok_or("панель не вдоль осей проекта");
    let (ax, ay, az) = (axis(f.x)?, axis(f.y)?, axis(f.z)?);
    let size = [hi[ax] - lo[ax], hi[ay] - lo[ay]];
    if (hi[az] - lo[az] - thickness).abs() > 1e-6 {
        return Err(format!("по толщине габарит {} ≠ толщине {thickness}", hi[az] - lo[az]));
    }
    let segs = resize(fp, size)?;
    // Bounding box: along each axis — from min to max of the projections of contour [0,size] and thickness [0,t].
    let mut origin = [0.0; 3];
    for (k, (ext, sign)) in [(ax, (size[0], f.x[ax])), (ay, (size[1], f.y[ay])), (az, (thickness, f.z[az]))] {
        // The origin is at the lower bound if the axis points positive, otherwise at the upper one.
        origin[k] = if sign > 0.0 { lo[k] } else { lo[k] + ext };
    }
    fp.segs = segs;
    fp.frame.origin = origin;
    Ok(())
}

/// Commands `panel.contour`, `panel.corner`, `panel.cutout`, `panel.resize`, `panel.box`.
pub fn command(pr: &mut Project, cmd: &str, p: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct Args {
        id: u32,
        segs: Option<Vec<Seg>>,
        at: Option<[f64; 2]>,
        radius: Option<f64>,
        chamfer: Option<f64>,
        center: Option<[f64; 2]>,
        size: Option<[f64; 2]>,
        lo: Option<[f64; 3]>,
        hi: Option<[f64; 3]>,
    }
    let a: Args = serde_json::from_value(p.clone()).map_err(|e| format!("{cmd}: {e}"))?;
    let pr_thickness = super::panels_thickness(pr, a.id)?;
    let Some(Object::Panel(fp)) = find_mut(&mut pr.objects, a.id) else {
        return Err(format!("нет панели {}: контур деталей корпуса правят после «Разрушить»", a.id));
    };
    let mut next = fp.clone();
    match cmd {
        "panel.contour" => set_contour(&mut next, a.segs.ok_or("нужен segs")?)?,
        "panel.corner" => {
            let (segs, edges) = corner(&next, a.at.ok_or("нужна вершина at: [x, y]")?, a.radius, a.chamfer)?;
            next.edges = edges;
            set_contour(&mut next, segs)?;
        }
        "panel.cutout" => {
            let segs = cutout(&next, a.center.ok_or("нужен center: [x, y]")?, a.radius, a.size)?;
            set_contour(&mut next, segs)?;
        }
        "panel.box" => {
            let t = pr_thickness;
            fit_box(&mut next, t, a.lo.ok_or("нужен lo: [x,y,z]")?, a.hi.ok_or("нужен hi: [x,y,z]")?)?;
        }
        "panel.resize" => {
            let segs = resize(&next, a.size.ok_or("нужен size: [a, b]")?)?;
            set_contour(&mut next, segs)?;
        }
        _ => return Err(format!("неизвестная команда «{cmd}»")),
    }
    *fp = next;
    Ok(Value::Null)
}
