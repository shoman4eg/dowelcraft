//! Doors and drawers in an opening (like "Door installation" and "Drawer installation" in Bazis). The opening
//! is found by a front-view point ([`super::fill`]). A door with hinges and drawers with runners are blocks; hinges and
//! drawer runners are hardware, their holes are drilled into doors, uprights and drawer boxes.
//! Hinge dimensions follow the overlay soft-close hinge from the Bazis models (`FurnList`).

use serde::Deserialize;
use serde_json::{Value, json};

use super::{Block, FreePanel, Furniture, Kind, Object, Part, Project, build, place, rect};
use crate::geom::Frame;
use crate::panel::{Texture, WorldHole};

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
enum Mount {
    /// Overlay: covers the upright edges.
    Overlay,
    /// Half-overlay: up to the middle of the upright thickness.
    Half,
    /// Inset: inside the opening, flush with the front.
    Inset,
}

/// Opening boundaries at a point: left, right, bottom, top parts.
fn opening<'a>(parts: &'a [Part], at: [f64; 2]) -> Result<[&'a Part; 4], String> {
    let blocks = |p: &&Part| !matches!(p.kind, Kind::Door | Kind::Drawer);
    let side = |axis: usize, lo: bool| -> Option<&'a Part> {
        let other = 1 - axis;
        let c = parts.iter().filter(blocks).filter(|p| p.origin[other] <= at[other] && at[other] <= p.origin[other] + p.size[other]);
        if lo {
            c.filter(|p| p.origin[axis] + p.size[axis] <= at[axis]).max_by(|a, b| (a.origin[axis] + a.size[axis]).total_cmp(&(b.origin[axis] + b.size[axis])))
        } else {
            c.filter(|p| p.origin[axis] >= at[axis]).min_by(|a, b| a.origin[axis].total_cmp(&b.origin[axis]))
        }
    };
    let need = |p: Option<&'a Part>, s: &str| p.ok_or_else(|| format!("no opening bound {s} of point {at:?}"));
    Ok([need(side(0, true), "on the left")?, need(side(0, false), "on the right")?, need(side(1, true), "below")?, need(side(1, false), "above")?])
}

/// Front edges along an axis between boundaries `lo` and `hi`: overlay — at outer faces, half-overlay — at middles,
/// inset — at inner faces; minus the gap.
fn span(lo: &Part, hi: &Part, axis: usize, mount: Mount, gap: f64) -> (f64, f64) {
    let (lmin, lmax, hmin, hmax) = (lo.origin[axis], lo.origin[axis] + lo.size[axis], hi.origin[axis], hi.origin[axis] + hi.size[axis]);
    let (a, b) = match mount {
        Mount::Overlay => (lmin, hmax),
        Mount::Half => ((lmin + lmax) / 2.0, (hmin + hmax) / 2.0),
        Mount::Inset => (lmax, hmin),
    };
    (a + gap, b - gap)
}

/// The opening is already closed: doors or drawers whose front on the front view overlaps the new `[x0, x1] × [y0, y1]`.
fn occupied(parts: &[Part], x: (f64, f64), y: (f64, f64)) -> Result<(), String> {
    let hit = parts.iter().filter(|p| matches!(p.kind, Kind::Door | Kind::Drawer)).find(|p| {
        p.origin[0] < x.1 - 1.0 && x.0 + 1.0 < p.origin[0] + p.size[0] && p.origin[1] < y.1 - 1.0 && y.0 + 1.0 < p.origin[1] + p.size[1]
    });
    match hit {
        Some(p) => Err(format!("the opening already has “{}” — delete it or choose another opening", p.panel.name)),
        None => Ok(()),
    }
}

/// Hinge: cup ⌀35×11.5 in the door's back face, center 22 mm from the edge; mounting plate — 2 holes ⌀2×12 in the upright
/// 37 mm from its front edge, 32 mm apart. Placed at the door edge (`edge_x`), at hinge height,
/// on the back face (`door_back`); `inward` is +1 if the door extends right from the edge (hinges on the left).
fn hinge(edge_x: f64, inward: f64, y: f64, door_back: f64, stand_face: f64, stand_front: f64) -> Furniture {
    let hole = |pos: [f64; 3], dir: [f64; 3], diameter: f64, depth: f64| WorldHole { pos, dir, diameter, depth };
    let (px, pz) = (stand_face - edge_x, stand_front - 37.0 - door_back);
    let x1 = 50.0 * inward;
    Furniture {
        id: 0,
        name: crate::i18n::t("Overlay hinge with soft close"),
        kind: crate::i18n::bazis("library.hinge").into(),
        source: String::new(),
        frame: Frame { origin: [edge_x, y, door_back], ..Frame::IDENTITY },
        holes: vec![
            hole([22.0 * inward, 0.0, 0.0], [0.0, 0.0, 1.0], 35.0, 11.5),
            hole([px, 16.0, pz], [-inward, 0.0, 0.0], 2.0, 12.0),
            hole([px, -16.0, pz], [-inward, 0.0, 0.0], 2.0, 12.0),
        ],
        lo: [x1.min(0.0), -30.0, -40.0],
        hi: [x1.max(0.0), 30.0, 0.0],
    }
}

/// Number of hinges by door height (manufacturer practice: up to 900 — 2, up to 1600 — 3, up to 2000 — 4, above — 5).
fn hinge_count(h: f64) -> usize {
    if h <= 900.0 { 2 } else if h <= 1600.0 { 3 } else if h <= 2000.0 { 4 } else { 5 }
}

/// `door.add {at, material, mount?, double?, gap?, edge?, hinges?: "left"|"right"|"none"}` → {block id}.
pub fn door(pr: &mut Project, p: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct A {
        at: [f64; 2],
        material: String,
        #[serde(default = "overlay")]
        mount: Mount,
        #[serde(default)]
        double: bool,
        #[serde(default = "two")]
        gap: f64,
        #[serde(default)]
        edge: Option<String>,
        #[serde(default = "left")]
        hinges: String,
    }
    fn overlay() -> Mount { Mount::Overlay }
    fn two() -> f64 { 2.0 }
    fn left() -> String { "left".into() }
    let a: A = serde_json::from_value(p.clone()).map_err(|e| format!("door: {e}"))?;
    let parts = build(pr)?;
    let [l, r, b, t] = opening(&parts, a.at)?;
    let m = pr.material(&a.material)?.clone();
    let edge = pr.edge(&a.edge)?.cloned();
    let (x0, x1) = span(l, r, 0, a.mount, a.gap);
    let (y0, y1) = span(b, t, 1, a.mount, a.gap);
    occupied(&parts, (x0, x1), (y0, y1))?;
    let front = (l.origin[2] + l.size[2]).min(r.origin[2] + r.size[2]);
    let z = if a.mount == Mount::Inset { front - m.thickness } else { front };
    let leaves = if a.double { 2 } else { 1 };
    let w = (x1 - x0 - a.gap * (leaves - 1) as f64) / leaves as f64;
    let h = y1 - y0;
    if !(w > 50.0 && h > 50.0) {
        return Err(format!("door {w:.1}×{h:.1} mm is too small"));
    }
    let mut next = pr.next_id();
    let mut children = Vec::new();
    for i in 0..leaves {
        let x = x0 + i as f64 * (w + a.gap);
        let panel = rect("Door", &m, w, h, Texture::Vertical, &[0, 1, 2, 3], edge.as_ref());
        let frame = place(&panel, [x, y0, z], [w, h, m.thickness])?;
        let id = next;
        next += 1;
        children.push(Object::Panel(FreePanel {
            id, name: panel.name.clone(), pos: String::new(), designation: String::new(), material: m.name.clone(),
            segs: panel.segs, texture: Texture::Vertical, edges: panel.edges, holes: Vec::new(), grooves: Vec::new(), kind: Kind::Door, frame,
        }));
        // Hinges: on a double door each leaf on its own side, otherwise as chosen.
        let side_left = match (leaves, i, a.hinges.as_str()) {
            (2, 0, _) => Some(true),
            (2, _, _) => Some(false),
            (_, _, "none") => None,
            (_, _, "right") => Some(false),
            _ => Some(true),
        };
        if let Some(left_side) = side_left {
            let (edge_x, inward, stand) = if left_side { (x, 1.0, l) } else { (x + w, -1.0, r) };
            // Mounting plate goes on the upright's inner face.
            let face = if left_side { stand.origin[0] + stand.size[0] } else { stand.origin[0] };
            let stand_front = stand.origin[2] + stand.size[2];
            let n = hinge_count(h);
            for k in 0..n {
                let y = y0 + 100.0 + (h - 200.0) * k as f64 / (n - 1) as f64;
                let mut f = hinge(edge_x, inward, y, z, face, stand_front);
                f.id = next;
                next += 1;
                children.push(Object::Furniture(f));
            }
        }
    }
    let id = next;
    pr.objects.push(Object::Block(Block { id, name: crate::i18n::t(if a.double { "Double door" } else { "Door" }), frame: Frame::IDENTITY, children }));
    Ok(json!({ "id": id }))
}

/// `drawers.add {at, material, heights: [front heights bottom to top], mount?, gap?, depth?, bottom?, edge?}` → {block id}.
/// Box for ball-bearing drawer runners (13 mm per side), as in cabinet drawers.
pub fn drawers(pr: &mut Project, p: &Value) -> Result<Value, String> {
    #[derive(Deserialize)]
    struct A {
        at: [f64; 2],
        material: String,
        heights: Vec<f64>,
        #[serde(default = "overlay")]
        mount: Mount,
        #[serde(default = "two")]
        gap: f64,
        #[serde(default)]
        depth: Option<f64>,
        #[serde(default)]
        bottom: Option<String>,
        #[serde(default)]
        edge: Option<String>,
    }
    fn overlay() -> Mount { Mount::Overlay }
    fn two() -> f64 { 2.0 }
    let a: A = serde_json::from_value(p.clone()).map_err(|e| format!("drawers: {e}"))?;
    if a.heights.is_empty() {
        return Err("heights: at least one front height is required".into());
    }
    let parts = build(pr)?;
    let [l, r, b, t] = opening(&parts, a.at)?;
    let m = pr.material(&a.material)?.clone();
    let bm = match &a.bottom { Some(n) => pr.material(n)?.clone(), None => m.clone() };
    let edge = pr.edge(&a.edge)?.cloned();
    let tt = m.thickness;
    let (inner0, inner1) = (l.origin[0] + l.size[0], r.origin[0]);
    let (z0, z1) = (l.origin[2].max(r.origin[2]), (l.origin[2] + l.size[2]).min(r.origin[2] + r.size[2]));
    let depth = a.depth.unwrap_or(((z1 - z0 - 10.0) / 50.0).floor() * 50.0);
    let (fx0, fx1) = span(l, r, 0, a.mount, a.gap);
    let (fy0, fy1) = span(b, t, 1, a.mount, a.gap);
    let total: f64 = a.heights.iter().sum::<f64>() + a.gap * (a.heights.len() - 1) as f64;
    if total > fy1 - fy0 + 1e-6 {
        return Err(format!("fronts of {total} mm do not fit the {:.1} mm opening", fy1 - fy0));
    }
    occupied(&parts, (fx0, fx1), (fy0, fy0 + total))?;
    let bw = inner1 - inner0 - 2.0 * super::RAIL_GAP;
    if !(depth >= 250.0 && depth <= z1 - z0 && bw > 2.0 * tt) {
        return Err(format!("box {bw:.0}×{depth:.0} cannot be built: depth ≥ 250 mm is required"));
    }
    let front_z = if a.mount == Mount::Inset { z1 - m.thickness } else { z1 };
    let mut next = pr.next_id();
    let mut children = Vec::new();
    let mut add = |name: &str, kind: Kind, mat: &super::Material, sx: f64, sy: f64, tex: Texture, edged: &[usize], origin: [f64; 3], size: [f64; 3]| -> Result<(), String> {
        let panel = rect(name, mat, sx, sy, tex, edged, edge.as_ref());
        let frame = place(&panel, origin, size)?;
        children.push(Object::Panel(FreePanel {
            id: next, name: panel.name.clone(), pos: String::new(), designation: String::new(), material: mat.name.clone(),
            segs: panel.segs, texture: tex, edges: panel.edges, holes: Vec::new(), grooves: Vec::new(), kind, frame,
        }));
        next += 1;
        Ok(())
    };
    let mut y = fy0;
    let mut runners = Vec::new();
    for &hf in &a.heights {
        let fw = fx1 - fx0;
        add("Drawer front", Kind::Drawer, &m, fw, hf, Texture::Horizontal, &[0, 1, 2, 3], [fx0, y, front_z], [fw, hf, m.thickness])?;
        let bh = hf - 40.0;
        if bh < 60.0 {
            return Err(format!("a {hf} mm front is too small for a box (≥ 100 mm is required)"));
        }
        let (bx, by, bz) = (inner0 + super::RAIL_GAP, y + 20.0, front_z - depth);
        add("Drawer side", Kind::Body, &m, depth, bh, Texture::Horizontal, &[2], [bx, by, bz], [tt, bh, depth])?;
        add("Drawer side", Kind::Body, &m, depth, bh, Texture::Horizontal, &[2], [bx + bw - tt, by, bz], [tt, bh, depth])?;
        add("Drawer front wall", Kind::Body, &m, bw - 2.0 * tt, bh, Texture::Horizontal, &[2], [bx + tt, by, front_z - tt], [bw - 2.0 * tt, bh, tt])?;
        add("Drawer back wall", Kind::Body, &m, bw - 2.0 * tt, bh, Texture::Horizontal, &[2], [bx + tt, by, bz], [bw - 2.0 * tt, bh, tt])?;
        add("Drawer bottom", Kind::Body, &bm, bw, depth, Texture::Default, &[], [bx, by - bm.thickness, bz], [bw, bm.thickness, depth])?;
        // Drawer runners (pair): ⌀4×12 screws into the uprights at 37 and 261 mm from the front edge, 10 mm above the box bottom.
        for (stand_face, dir) in [(inner0, -1.0), (inner1, 1.0)] {
            let holes = [37.0, 261.0f64.min(depth - 20.0)]
                .map(|d| WorldHole { pos: [stand_face, by - bm.thickness + 10.0, front_z - d], dir: [dir, 0.0, 0.0], diameter: 4.0, depth: 12.0 })
                .to_vec();
            runners.push(holes);
        }
        y += hf + a.gap;
    }
    for holes in runners {
        children.push(Object::Furniture(Furniture {
            id: next, name: crate::i18n::t(format!("Ball-bearing runner {depth:.0} mm (half a pair)")), kind: crate::i18n::bazis("library.drawer").into(), source: String::new(),
            frame: Frame::IDENTITY, holes, lo: [0.0; 3], hi: [0.0; 3],
        }));
        next += 1;
    }
    let id = next;
    pr.objects.push(Object::Block(Block { id, name: crate::i18n::t("Drawers"), frame: Frame::IDENTITY, children }));
    Ok(json!({ "id": id }))
}
