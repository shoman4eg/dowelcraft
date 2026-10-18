//! Panel "into the opening" (like the "~" key in Bazis): from a point on the front view the nearest
//! opening boundaries are found, and the panel fills it entirely minus the gaps. Doors and fronts are not boundaries.

use super::{Kind, Part, Plane};

/// Opening boundaries around a point and the resulting panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    /// Contour size (as in `panel.add`).
    pub size: [f64; 2],
    /// Bounding box min corner.
    pub position: [f64; 3],
}

fn blocks(p: &Part) -> bool {
    !matches!(p.kind, Kind::Door | Kind::Drawer)
}

/// Nearest boundaries along axis `axis` (0 — X, 1 — Y) from `at` among parts whose projection on the other
/// front-view axis covers the point. Parts covering the point along `axis` are not boundaries (a shelf doesn't bound
/// the opening width, a back panel bounds nothing). Returns (nearest left/below, nearest right/above).
fn bounds(parts: &[Part], at: [f64; 2], axis: usize) -> (Option<&Part>, Option<&Part>) {
    let other = 1 - axis;
    let across = |p: &&Part| p.origin[other] <= at[other] && at[other] <= p.origin[other] + p.size[other];
    let cands: Vec<&Part> = parts.iter().filter(|p| blocks(p)).filter(across).collect();
    let lo = cands.iter().filter(|p| p.origin[axis] + p.size[axis] <= at[axis]).max_by(|a, b| (a.origin[axis] + a.size[axis]).total_cmp(&(b.origin[axis] + b.size[axis])));
    let hi = cands.iter().filter(|p| p.origin[axis] >= at[axis]).min_by(|a, b| a.origin[axis].total_cmp(&b.origin[axis]));
    (lo.copied(), hi.copied())
}

/// Common Z range of two boundaries — the opening depth.
fn depth(a: &Part, b: &Part) -> Result<(f64, f64), String> {
    let (z0, z1) = (a.origin[2].max(b.origin[2]), (a.origin[2] + a.size[2]).min(b.origin[2] + b.size[2]));
    if z1 > z0 { Ok((z0, z1)) } else { Err(format!("the bounds “{}” and “{}” do not overlap in depth", a.panel.name, b.panel.name)) }
}

fn need<'a>(b: Option<&'a Part>, side: &str, at: [f64; 2]) -> Result<&'a Part, String> {
    b.ok_or_else(|| format!("no opening bound {side} of point {at:?}"))
}

/// Panel in the opening at point `at` (X, Y on the front view, mm). `gaps` are offsets from the boundaries in contour axes:
/// horizontal — [left, right, back, front], vertical — [back, front, bottom, top],
/// front — [left, right, bottom, top]. A horizontal panel rests its bottom face at height `at.y`,
/// a vertical one its left face at `at.x`, a front one is flush with the opening front.
pub fn fill(parts: &[Part], plane: Plane, at: [f64; 2], thickness: f64, gaps: [f64; 4]) -> Result<Fill, String> {
    let [g0, g1, g2, g3] = gaps;
    let check = |size: [f64; 2]| {
        if size[0] > 0.0 && size[1] > 0.0 { Ok(size) } else { Err(format!("the opening minus the offsets is empty: {size:?}")) }
    };
    match plane {
        Plane::Horizontal => {
            let (l, r) = bounds(parts, at, 0);
            let (l, r) = (need(l, "on the left", at)?, need(r, "on the right", at)?);
            let (z0, z1) = depth(l, r)?;
            let x0 = l.origin[0] + l.size[0];
            let size = check([r.origin[0] - x0 - g0 - g1, z1 - z0 - g2 - g3])?;
            Ok(Fill { size, position: [x0 + g0, at[1], z0 + g2] })
        }
        Plane::Vertical => {
            let (b, t) = bounds(parts, at, 1);
            let (b, t) = (need(b, "below", at)?, need(t, "above", at)?);
            let (z0, z1) = depth(b, t)?;
            let y0 = b.origin[1] + b.size[1];
            let size = check([z1 - z0 - g0 - g1, t.origin[1] - y0 - g2 - g3])?;
            Ok(Fill { size, position: [at[0], y0 + g2, z0 + g0] })
        }
        Plane::Front => {
            let (l, r) = bounds(parts, at, 0);
            let (b, t) = bounds(parts, at, 1);
            let (l, r, b, t) = (need(l, "on the left", at)?, need(r, "on the right", at)?, need(b, "below", at)?, need(t, "above", at)?);
            let (_, z1) = depth(l, r)?;
            let (x0, y0) = (l.origin[0] + l.size[0], b.origin[1] + b.size[1]);
            let size = check([r.origin[0] - x0 - g0 - g1, t.origin[1] - y0 - g2 - g3])?;
            Ok(Fill { size, position: [x0 + g0, y0 + g2, z1 - thickness] })
        }
    }
}
