//! Opening doors and drawers for the 3D view: how each front moves, its parts' positions at a given opening and
//! which of them hit something when fully open. Only for showing — the project is not changed.

use super::{Kind, Part};

/// How a part moves when the fronts open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// A door turns about a vertical axis through (`x`, `z`): `-1` — hinges on the left, `1` — on the right.
    Swing { x: f64, z: f64, side: f64 },
    /// A drawer front and its box slide forward (+Z) by `d` mm.
    Slide { d: f64 },
}

/// A door opens this far, degrees.
pub const DOOR_ANGLE: f64 = 95.0;
/// A drawer comes out by this share of its box depth.
const DRAWER_OUT: f64 = 0.8;
const EPS: f64 = 0.5;

/// Motion of every part; `hinges` — x of hinge models (from hardware), to tell a door's hinge side.
///
/// Doors facing the front (thickness along Z) turn about their hinge edge: the side with hinge models, otherwise
/// the one away from a neighbouring door leaf, otherwise the left. A drawer front takes along the parts behind it
/// that lie within its outline (the box, its bottom).
pub fn motions(parts: &[Part], hinges: &[[f64; 3]]) -> Vec<Option<Motion>> {
    let mut out = vec![None; parts.len()];
    let hi = |p: &Part, k: usize| p.origin[k] + p.size[k];
    let front_facing = |p: &Part| p.size[2] <= p.size[0].min(p.size[1]) && (p.size[2] - p.panel.thickness).abs() < EPS;
    for (i, p) in parts.iter().enumerate().filter(|(_, p)| front_facing(p)) {
        match p.kind {
            Kind::Door => {
                let (x0, x1) = (p.origin[0], hi(p, 0));
                // Hinge models at each edge of the door (a neighbour's hinges sit just past the other edge: counted apart).
                let at_edge = |x: f64| hinges.iter().filter(|h| (h[0] - x).abs() < 40.0 && h[1] >= p.origin[1] - EPS && h[1] <= hi(p, 1) + EPS).count();
                let (on_left, on_right) = (at_edge(x0 + 20.0), at_edge(x1 - 20.0));
                // A neighbouring leaf in the same plane, edge to edge (a double door): hinges go on the far side.
                let leaf = |left: bool| {
                    parts.iter().any(|q| {
                        q.kind == Kind::Door
                            && (q.origin[2] - p.origin[2]).abs() < EPS
                            && q.origin[1] < hi(p, 1)
                            && hi(q, 1) > p.origin[1]
                            && if left { (hi(q, 0) - x0).abs() < 10.0 } else { (q.origin[0] - x1).abs() < 10.0 }
                    })
                };
                // The edge with more hinge models; otherwise the right edge only for the right leaf of a pair, else the left.
                let left = if on_left != on_right { on_left > on_right } else { !(leaf(true) && !leaf(false)) };
                // About the front edge: the door never goes behind its own front plane (an inset door or one flush
                // with the sides would otherwise swing into them).
                out[i] = Some(Motion::Swing { x: if left { x0 } else { x1 }, z: hi(p, 2), side: if left { -1.0 } else { 1.0 } });
            }
            Kind::Drawer => {
                let inside = |q: &Part| {
                    // Only its own cabinet or block: a shelf of another unit behind the front stays put.
                    q.owner == p.owner
                        && q.kind != Kind::Door
                        && q.kind != Kind::Drawer
                        && q.origin[0] >= p.origin[0] - EPS
                        && hi(q, 0) <= hi(p, 0) + EPS
                        && q.origin[1] >= p.origin[1] - EPS
                        && hi(q, 1) <= hi(p, 1) + EPS
                        && hi(q, 2) <= p.origin[2] + EPS
                };
                let group: Vec<usize> = parts.iter().enumerate().filter(|(_, q)| inside(q)).map(|(j, _)| j).collect();
                let depth = group.iter().map(|&j| p.origin[2] - parts[j].origin[2]).fold(0.0, f64::max);
                let d = DRAWER_OUT * if depth > 0.0 { depth } else { 300.0 };
                for j in std::iter::once(i).chain(group) {
                    out[j] = Some(Motion::Slide { d });
                }
            }
            _ => {}
        }
    }
    out
}

/// A point of a moving part at opening `f` (0 — closed, 1 — fully open).
pub fn moved(m: Motion, f: f64, p: [f64; 3]) -> [f64; 3] {
    match m {
        Motion::Slide { d } => [p[0], p[1], p[2] + d * f],
        Motion::Swing { x, z, side } => {
            // Outward (towards +Z) about the hinge axis.
            let a = (side * DOOR_ANGLE * f).to_radians();
            let (dx, dz) = (p[0] - x, p[2] - z);
            [x + dx * a.cos() + dz * a.sin(), p[1], z - dx * a.sin() + dz * a.cos()]
        }
    }
}

/// A direction (normal) of a moving part at opening `f`.
pub fn turned(m: Motion, f: f64, n: [f64; 3]) -> [f64; 3] {
    match m {
        Motion::Slide { .. } => n,
        Motion::Swing { side, .. } => {
            let a = (side * DOOR_ANGLE * f).to_radians();
            [n[0] * a.cos() + n[2] * a.sin(), n[1], -n[0] * a.sin() + n[2] * a.cos()]
        }
    }
}

/// Bounding box of a part at opening `f`.
fn bbox(p: &Part, m: Option<Motion>, f: f64) -> ([f64; 3], [f64; 3]) {
    let Some(m) = m else { return (p.origin, std::array::from_fn(|k| p.origin[k] + p.size[k])) };
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for c in 0..8 {
        let corner: [f64; 3] = std::array::from_fn(|k| p.origin[k] + if c >> k & 1 == 1 { p.size[k] } else { 0.0 });
        let q = moved(m, f, corner);
        for k in 0..3 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    (lo, hi)
}

/// Moving parts that hit another part when fully open (along their path: checked at several openings), and that
/// other part. Pairs that already touch when closed (a door and its own cabinet) don't count.
pub fn clashes(parts: &[Part], motions: &[Option<Motion>]) -> Vec<usize> {
    let overlap = |a: &([f64; 3], [f64; 3]), b: &([f64; 3], [f64; 3])| (0..3).all(|k| a.0[k] < b.1[k] - EPS && b.0[k] < a.1[k] - EPS);
    let closed: Vec<_> = parts.iter().enumerate().map(|(i, p)| bbox(p, motions[i], 0.0)).collect();
    let mut hit = vec![false; parts.len()];
    for step in [0.35, 0.7, 1.0] {
        let now: Vec<_> = parts.iter().enumerate().map(|(i, p)| bbox(p, motions[i], step)).collect();
        for i in (0..parts.len()).filter(|&i| motions[i].is_some()) {
            for j in (0..parts.len()).filter(|&j| j != i) {
                // Parts moving together (a drawer and its box) never hit each other.
                if motions[i] == motions[j] && matches!(motions[i], Some(Motion::Slide { .. })) {
                    continue;
                }
                if overlap(&now[i], &now[j]) && !overlap(&closed[i], &closed[j]) {
                    hit[i] = true;
                    hit[j] = true;
                }
            }
        }
    }
    (0..parts.len()).filter(|&i| hit[i]).collect()
}
