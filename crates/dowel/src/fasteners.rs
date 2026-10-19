//! Fasteners and drilling: fastener library, panel joints, placement and holes with coordinates.
//!
//! Joint — the edge of panel B touches the face of panel A ("face – edge" scheme, table 28.1 of the Bazis
//! manual). A joint has its own frame: origin on the contact plane, at mid-thickness of B;
//! X — into B, Y — along B's normal (toward its front face; for horizontal B — up), Z — along the joint. Fastener holes are given
//! in this frame and depend on thicknesses: coordinate = c + t·(A thickness coef.) + tb·(B thickness coef.).
//! Library sizes are checked against fasteners from Bazis models (`Document/FurnList`).

use serde::{Deserialize, Serialize};

use crate::design::Part;
use crate::panel::Hole;

/// Fastener hole in the joint frame.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HoleDef {
    /// Which panel: "a" — the face one, "b" — the edge one.
    pub panel: char,
    pub at: [f64; 3],
    #[serde(default)]
    pub at_t: [f64; 3],
    #[serde(default)]
    pub at_tb: [f64; 3],
    /// Drilling direction.
    pub dir: [f64; 3],
    pub diameter: f64,
    pub depth: f64,
    /// Depth added per thickness of A (through A — `depth: 0, depth_t: 1`).
    #[serde(default)]
    pub depth_t: f64,
    /// Y points into the carcass (cam lock cup): down at the top, up at the bottom. Otherwise Y as is
    /// (shelf support always under the shelf).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inward: bool,
}

/// Library fastener.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Def {
    /// Name `Name\rArticle` — as in the Bazis cost estimate.
    pub name: String,
    pub holes: Vec<HoleDef>,
    /// Offset of the outermost points from the joint ends, mm.
    pub offset: f64,
    /// Maximum spacing between points, mm.
    pub step: f64,
    /// Minimum and maximum number of points per joint.
    pub min: usize,
    pub max: usize,
    /// What is bought per point if the fastener is composite (cam lock + dowel); empty — the fastener itself, 1 pc.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<(String, usize)>,
    /// System pitch, mm (32): spacing between points is a multiple of it and the points are centred on the joint;
    /// 0 — evenly between the end offsets.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub grid: f64,
    /// Spare holes in panel A below and above each point, `grid` apart (a shelf can be moved); holes that would
    /// leave the part are skipped.
    #[serde(default, skip_serializing_if = "is_none2")]
    pub rows: [usize; 2],
    /// Price per piece for the estimate; 0 — not set.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub price: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_none2(v: &[usize; 2]) -> bool {
    *v == [0, 0]
}

/// The 32 mm system of drilling heads.
pub const GRID: f64 = 32.0;

fn h(panel: char, at: [f64; 3], dir: [f64; 3], diameter: f64, depth: f64) -> HoleDef {
    HoleDef { panel, at, at_t: [0.0; 3], at_tb: [0.0; 3], dir, diameter, depth, depth_t: 0.0, inward: false }
}

/// Default library.
pub fn library() -> Vec<Def> {
    let (x, nx, ny) = ([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]);
    vec![
        Def {
            name: "Confirmat 7x50".into(),
            holes: vec![
                // Through ⌀8 across face A from its outer side; ⌀5 into edge B: 50 − t + 3 mm.
                HoleDef { at_t: [-1.0, 0.0, 0.0], depth_t: 1.0, ..h('a', [0.0; 3], x, 8.0, 0.0) },
                HoleDef { depth_t: -1.0, ..h('b', [0.0; 3], x, 5.0, 53.0) },
            ],
            offset: 50.0,
            step: 500.0,
            min: 2,
            max: 20,
            items: Vec::new(),
            grid: GRID,
            rows: [0, 0],
            price: 0.0,
        },
        Def {
            name: "Minifix 15 cam".into(),
            holes: vec![
                h('b', [0.0; 3], x, 8.0, 34.0),
                // Cup ⌀15×13 in face B, 34 mm from the joint, on the inner side of the carcass.
                HoleDef { at_tb: [0.0, 0.5, 0.0], inward: true, ..h('b', [34.0, 0.0, 0.0], ny, 15.0, 13.0) },
                h('a', [0.0; 3], nx, 5.0, 13.0),
            ],
            offset: 50.0,
            step: 500.0,
            min: 2,
            max: 20,
            items: Vec::new(),
            grid: GRID,
            rows: [0, 0],
            price: 0.0,
        },
        Def {
            name: "Dowel 8x30".into(),
            holes: vec![h('b', [0.0; 3], x, 8.0, 20.0), h('a', [0.0; 3], nx, 8.0, 13.0)],
            offset: 32.0,
            step: 300.0,
            min: 2,
            max: 30,
            items: Vec::new(),
            grid: GRID,
            rows: [0, 0],
            price: 0.0,
        },
        Def {
            name: "Shelf support 5".into(),
            // Under the shelf: centre 2.5 mm below its bottom face, ⌀5×9 into face A.
            holes: vec![HoleDef { at_tb: [0.0, -0.5, 0.0], ..h('a', [0.0, -2.5, 0.0], nx, 5.0, 9.0) }],
            offset: 50.0,
            step: 10_000.0,
            min: 2,
            max: 2,
            items: Vec::new(),
            grid: GRID,
            rows: [2, 2],
            price: 0.0,
        },
        Def {
            name: "Minifix 15 + dowel 8x30".into(),
            // At each point — a cam lock, next to it 32 mm closer to the joint middle — a dowel.
            holes: vec![
                h('b', [0.0; 3], x, 8.0, 34.0),
                HoleDef { at_tb: [0.0, 0.5, 0.0], inward: true, ..h('b', [34.0, 0.0, 0.0], ny, 15.0, 13.0) },
                h('a', [0.0; 3], nx, 5.0, 13.0),
                h('b', [0.0, 0.0, 32.0], x, 8.0, 20.0),
                h('a', [0.0, 0.0, 32.0], nx, 8.0, 13.0),
            ],
            offset: 50.0,
            step: 500.0,
            min: 2,
            max: 20,
            items: vec![("Minifix 15 cam".into(), 1), ("Dowel 8x30".into(), 1)],
            grid: GRID,
            rows: [0, 0],
            price: 0.0,
        },
    ]
}

/// Joint: edge of panel `b` on the face of panel `a`.
#[derive(Clone, Debug, PartialEq)]
pub struct Joint {
    pub a: usize,
    pub b: usize,
    /// Joint origin (Z = 0 in the joint frame) and X, Y, Z axes in project coordinates.
    pub origin: [f64; 3],
    pub axes: [[f64; 3]; 3],
    pub length: f64,
    /// Sign of the "into the carcass" Y for `HoleDef::inward`: −1 if horizontal B is above the middle of A (top).
    pub inward: f64,
}

const EPS: f64 = 1e-6;

fn thick_axis(p: &Part) -> Option<usize> {
    let f = p.panel.frame?;
    (0..3).find(|&k| f.z[k].abs() > 1.0 - 1e-9)
}

/// Joint of two parts if the edge of `b` touches the face of `a`. Only for parts aligned with the project axes.
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
    // Edge B lies fully on face A across B's thickness and overlaps along the joint length.
    let (j0, j1) = (alo[j].max(blo[j]), ahi[j].min(bhi[j]));
    if j1 - j0 <= EPS || blo[m] < alo[m] - EPS || bhi[m] > ahi[m] + EPS {
        return None;
    }
    let mut origin = [0.0; 3];
    origin[k] = if sign > 0.0 { ahi[k] } else { alo[k] };
    origin[m] = (blo[m] + bhi[m]) / 2.0;
    origin[j] = j0;
    let unit = |i: usize, s: f64| { let mut v = [0.0; 3]; v[i] = s; v };
    // For horizontal B the Y axis is always up: shelf support under the shelf, cam lock cup on top,
    // however its front face is turned.
    let ny = if m == 1 { 1.0 } else { pb.panel.frame.map_or(1.0, |f| f.z[m].signum()) };
    // Z — along the joint toward increasing coordinate (points are measured from its start); the triple may
    // be left-handed — it doesn't matter for coordinate conversion.
    let (xa, ya, za) = (unit(k, sign), unit(m, ny), unit(j, 1.0));
    let inward = if m == 1 && blo[1] + bhi[1] > alo[1] + ahi[1] { -1.0 } else { 1.0 };
    Some(Joint { a, b, origin, axes: [xa, ya, za], length: j1 - j0, inward })
}

/// Fastener points along a joint of length `len`: offset from the ends, spacing no more than `step`.
pub fn positions(def: &Def, len: f64) -> Vec<f64> {
    let span = len - 2.0 * def.offset;
    if span <= 0.0 {
        return vec![len / 2.0];
    }
    let n = (((span / def.step).ceil() as usize) + 1).clamp(def.min.max(2), def.max.max(2));
    // On the system grid: the even spacing rounded down to the pitch, the whole set centred on the joint.
    let pitch = if def.grid > 0.0 { (span / (n - 1) as f64 / def.grid + 1e-9).floor() * def.grid } else { 0.0 };
    if pitch > 0.0 {
        let start = (len - pitch * (n - 1) as f64) / 2.0;
        return (0..n).map(|i| start + pitch * i as f64).collect();
    }
    (0..n).map(|i| def.offset + span * i as f64 / (n - 1) as f64).collect()
}

/// Holes of fastener `def` at joint point `s`: (part index, hole in part coordinates).
/// Error if a hole goes outside the part (deeper than thickness, off the contour).
pub fn holes(def: &Def, parts: &[Part], jt: &Joint, s: f64) -> Result<Vec<(usize, Hole)>, String> {
    let (t, tb) = (parts[jt.a].panel.thickness, parts[jt.b].panel.thickness);
    let [x, y, z] = jt.axes;
    let world = |l: [f64; 3]| -> [f64; 3] { std::array::from_fn(|k| jt.origin[k] + x[k] * l[0] + y[k] * l[1] + z[k] * (l[2] + s)) };
    let wdir = |d: [f64; 3]| -> [f64; 3] { std::array::from_fn(|k| x[k] * d[0] + y[k] * d[1] + z[k] * d[2]) };
    // Offset along the joint — toward its middle: mirrored for points in the second half.
    let toward = if s > jt.length / 2.0 { -1.0 } else { 1.0 };
    let mut out = Vec::new();
    for hd in &def.holes {
        let idx = if hd.panel == 'a' { jt.a } else { jt.b };
        let p = &parts[idx];
        let f = p.panel.frame.ok_or("a part without a position")?;
        let mut at: [f64; 3] = std::array::from_fn(|k| hd.at[k] + hd.at_t[k] * t + hd.at_tb[k] * tb);
        at[2] *= toward;
        let mut dir = hd.dir;
        if hd.inward {
            at[1] *= jt.inward;
            dir[1] *= jt.inward;
        }
        let w = world(at);
        let d = wdir(dir);
        let local = f.local(w).map(clean);
        let ld = [0, 1, 2].map(|i| clean(d[0] * [f.x, f.y, f.z][i][0] + d[1] * [f.x, f.y, f.z][i][1] + d[2] * [f.x, f.y, f.z][i][2]));
        let depth = hd.depth + hd.depth_t * t;
        let (sx, sy, th) = (p.panel.size.0, p.panel.size.1, p.panel.thickness);
        let inside = |q: [f64; 3]| q[0] >= -EPS && q[0] <= sx + EPS && q[1] >= -EPS && q[1] <= sy + EPS && q[2] >= -EPS && q[2] <= th + EPS;
        let end: [f64; 3] = std::array::from_fn(|k| local[k] + ld[k] * depth);
        if !inside(local) || !inside(end) {
            return Err(format!(
                "“{}”: hole ⌀{}×{} in “{}” goes outside the part (entry {:?}, end {:?})",
                def.name.split('\r').next().unwrap_or(&def.name), hd.diameter, depth, p.panel.name, local, end.map(clean)
            ));
        }
        out.push((idx, Hole { pos: local, dir: ld, diameter: hd.diameter, depth }));
        // Spare holes in A along Y (B's normal: up for a shelf), a pitch apart; those leaving the part are skipped.
        if hd.panel == 'a' && def.grid > 0.0 {
            let shifts = (1..=def.rows[0]).map(|k| -(k as f64)).chain((1..=def.rows[1]).map(|k| k as f64));
            for k in shifts {
                let mut sa = at;
                sa[1] += k * def.grid;
                let local = f.local(world(sa)).map(clean);
                let end: [f64; 3] = std::array::from_fn(|i| local[i] + ld[i] * depth);
                if inside(local) && inside(end) {
                    out.push((idx, Hole { pos: local, dir: ld, diameter: hd.diameter, depth }));
                }
            }
        }
    }
    Ok(out)
}

fn clean(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

/// All joints among parts `ids` (pairs where the edge of one is on the face of the other).
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
