//! Mesh for the 3D view: a panel is the contour extruded by its thickness, in project coordinates.
//! Arcs are split with chord deviation no more than [`TOLERANCE`]; edges — the contour of both faces
//! and verticals at segment ends (like "No invisible" in Bazis: no arc-splitting edges).

use crate::contour::{self, Seg};
use crate::design::{Kind, Part};

/// Maximum chord deviation from the arc, mm.
pub const TOLERANCE: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    /// Texture coordinates, mm: on faces — the contour point, on edge faces — (along the edge, through thickness).
    pub uv: [f32; 2],
    /// Part index in the `parts` list.
    pub part: u32,
}

#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    /// Triangles, three indices each.
    pub indices: Vec<u32>,
    /// Edges: point pairs and part index.
    pub lines: Vec<([f32; 3], [f32; 3], u32)>,
    /// Part role, by part index.
    pub kinds: Vec<Kind>,
    /// Parts whose contour could not be built: (index, reason).
    pub errors: Vec<(usize, String)>,
}

/// Segment points from start to end (end excluded), arcs subdivided.
fn points(s: &Seg, out: &mut Vec<[f64; 2]>) {
    out.extend(contour::tessellate(s, TOLERANCE));
}

/// Closed contour as a polygon: points and segment starts (where vertical edges go).
#[derive(Debug, Clone)]
struct Ring {
    pts: Vec<[f64; 2]>,
    corners: Vec<usize>,
}

impl Ring {
    fn reverse(&mut self) {
        self.pts.reverse();
        // The same corner points in reverse traversal order.
        let n = self.pts.len();
        self.corners = self.corners.iter().map(|&c| n - 1 - c).collect();
    }
}

/// Panel contours: the first is outer (counter-clockwise), the rest are cutouts (clockwise).
fn rings(segs: &[Seg]) -> Result<Vec<Ring>, String> {
    let mut rings: Vec<Ring> = contour::loops(segs)?
        .into_iter()
        .map(|lp| {
            let mut r = Ring { pts: Vec::new(), corners: Vec::new() };
            for (i, rev) in lp {
                r.corners.push(r.pts.len());
                points(&if rev { contour::reversed(&segs[i]) } else { segs[i] }, &mut r.pts);
            }
            r
        })
        .filter(|r| r.pts.len() >= 3)
        .collect();
    let outer = (0..rings.len()).max_by(|&a, &b| area(&rings[a].pts).abs().total_cmp(&area(&rings[b].pts).abs())).ok_or("пустой контур")?;
    rings.swap(0, outer);
    for (i, r) in rings.iter_mut().enumerate() {
        if (area(&r.pts) > 0.0) != (i == 0) {
            r.reverse();
        }
    }
    Ok(rings)
}

/// Outer contour with cutouts as one polygon: each cutout is joined to the outer contour
/// by a bridge from a visible vertex (bridge vertices are duplicated).
fn merge(rings: &[Ring]) -> Vec<[f64; 2]> {
    let mut poly = rings[0].pts.clone();
    let mut holes: Vec<&Ring> = rings[1..].iter().collect();
    // Start from the rightmost cutout: bridges don't cross cutouts not yet inserted.
    holes.sort_by(|a, b| {
        let mx = |r: &Ring| r.pts.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        mx(b).total_cmp(&mx(a))
    });
    for (k, hole) in holes.iter().enumerate() {
        let hi = (0..hole.pts.len()).max_by(|&a, &b| hole.pts[a][0].total_cmp(&hole.pts[b][0])).unwrap_or(0);
        let h = hole.pts[hi];
        let crosses = |a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]| {
            let o = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
            let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
            d1 * d2 < -1e-12 && d3 * d4 < -1e-12
        };
        let edges = |pts: &[[f64; 2]]| (0..pts.len()).map(|i| (pts[i], pts[(i + 1) % pts.len()])).collect::<Vec<_>>();
        let mut blockers = edges(&poly);
        for other in &holes[k..] {
            blockers.extend(edges(&other.pts));
        }
        let visible = |j: usize| !blockers.iter().any(|&(c, d)| crosses(h, poly[j], c, d));
        let dist = |j: usize| (poly[j][0] - h[0]).hypot(poly[j][1] - h[1]);
        // Nearest visible vertex: usually the nearest one is visible, otherwise try by increasing distance.
        let nearest = (0..poly.len()).min_by(|&a, &b| dist(a).total_cmp(&dist(b))).unwrap_or(0);
        let j = if visible(nearest) {
            nearest
        } else {
            let mut order: Vec<usize> = (0..poly.len()).collect();
            order.sort_by(|&a, &b| dist(a).total_cmp(&dist(b)));
            order.into_iter().find(|&j| visible(j)).unwrap_or(0)
        };
        let mut next = poly[..=j].to_vec();
        next.extend(hole.pts[hi..].iter().chain(&hole.pts[..=hi]));
        next.extend_from_slice(&poly[j..]);
        poly = next;
    }
    poly
}

fn area(p: &[[f64; 2]]) -> f64 {
    (0..p.len()).map(|i| {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        a[0] * b[1] - b[0] * a[1]
    }).sum::<f64>() / 2.0
}

/// Triangulation of a simple polygon by ear clipping. Vertices counter-clockwise.
/// Only reflex vertices are tested against a candidate (only they can lie inside an ear),
/// traversal goes round in a loop — O(n·r), where r is the number of reflex vertices.
fn triangulate(p: &[[f64; 2]]) -> Vec<[usize; 3]> {
    let n = p.len();
    if n < 3 {
        return Vec::new();
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let convex = |prev: &[usize], next: &[usize], i: usize| cross(p[prev[i]], p[i], p[next[i]]) > 1e-12;
    let mut reflex: Vec<bool> = (0..n).map(|i| !convex(&prev, &next, i)).collect();
    let mut alive = vec![true; n];
    // Grid of cells with reflex vertices: only cells under the ear's bounding box are checked.
    // A reflex vertex can become convex but not vice versa — stale entries are filtered by `reflex[j]`.
    let (lo, hi) = p.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), q| ([lo[0].min(q[0]), lo[1].min(q[1])], [hi[0].max(q[0]), hi[1].max(q[1])]));
    let cells = ((n as f64).sqrt().ceil() as usize).clamp(1, 256);
    let cell = |v: f64, k: usize| (((v - lo[k]) / (hi[k] - lo[k]).max(1e-9) * cells as f64) as usize).min(cells - 1);
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); cells * cells];
    for j in (0..n).filter(|&j| reflex[j]) {
        grid[cell(p[j][1], 1) * cells + cell(p[j][0], 0)].push(j);
    }
    let mut out = Vec::with_capacity(n - 2);
    let (mut left, mut i, mut stall) = (n, 0, 0);
    while left > 3 {
        let (a, c) = (prev[i], next[i]);
        let ear = !reflex[i] && {
            let (pa, pb, pc) = (p[a], p[i], p[c]);
            let (x0, x1) = (cell(pa[0].min(pb[0]).min(pc[0]), 0), cell(pa[0].max(pb[0]).max(pc[0]), 0));
            let (y0, y1) = (cell(pa[1].min(pb[1]).min(pc[1]), 1), cell(pa[1].max(pb[1]).max(pc[1]), 1));
            !(y0..=y1).flat_map(|y| (x0..=x1).map(move |x| y * cells + x)).flat_map(|g| &grid[g]).any(|&j| {
                let q = p[j];
                alive[j] && reflex[j] && j != a && j != c && j != i && q != pa && q != pb && q != pc
                    && cross(pa, pb, q) >= 0.0 && cross(pb, pc, q) >= 0.0 && cross(pc, pa, q) >= 0.0
            })
        };
        // No ears in a full loop — the vertex is degenerate (collinear with neighbours): remove it.
        if ear || stall > left {
            if ear {
                out.push([a, i, c]);
            }
            alive[i] = false;
            next[a] = c;
            prev[c] = a;
            left -= 1;
            stall = 0;
            reflex[a] = !convex(&prev, &next, a);
            reflex[c] = !convex(&prev, &next, c);
            i = c;
        } else {
            stall += 1;
            i = next[i];
        }
    }
    if cross(p[prev[i]], p[i], p[next[i]]).abs() > 0.0 {
        out.push([prev[i], i, next[i]]);
    }
    out
}

/// Parts mesh. Parts with `skip` (e.g. doors when hidden) are skipped.
pub fn build(parts: &[Part], skip: impl Fn(&Part) -> bool) -> Mesh {
    let mut m = Mesh { kinds: parts.iter().map(|p| p.kind).collect(), ..Mesh::default() };
    for (pi, part) in parts.iter().enumerate() {
        let Some(f) = part.panel.frame else { continue };
        if skip(part) {
            continue;
        }
        let rings = match rings(&part.panel.segs) {
            Ok(r) => r,
            Err(e) => {
                m.errors.push((pi, e));
                continue;
            }
        };
        let pts = merge(&rings);
        let t = part.panel.thickness;
        let w = |p: [f64; 2], z: f64| f.apply([p[0], p[1], z]).map(|v| v as f32);
        let n = |v: [f64; 3]| f.dir(v).map(|v| v as f32);
        let pi = pi as u32;
        let tris = triangulate(&pts);
        // Faces: the front face (z = t) points along panel +Z, the back face opposite.
        for (z, normal, flip) in [(t, [0.0, 0.0, 1.0], false), (0.0, [0.0, 0.0, -1.0], true)] {
            let base = m.vertices.len() as u32;
            m.vertices.extend(pts.iter().map(|&p| Vertex { pos: w(p, z), normal: n(normal), uv: [p[0] as f32, p[1] as f32], part: pi }));
            for tri in &tris {
                let [a, b, c] = tri.map(|i| base + i as u32);
                m.indices.extend(if flip { [a, c, b] } else { [a, b, c] });
            }
        }
        // Edge faces: a quad per chord, normal outward — to the right of the traversal
        // (outer contour counter-clockwise, cutouts clockwise).
        for ring in &rings {
            let (pts, k) = (&ring.pts, ring.pts.len());
            for i in 0..k {
                let (a, b) = (pts[i], pts[(i + 1) % k]);
                let len = (b[0] - a[0]).hypot(b[1] - a[1]);
                if len < 1e-9 {
                    continue;
                }
                let normal = n([(b[1] - a[1]) / len, -(b[0] - a[0]) / len, 0.0]);
                let base = m.vertices.len() as u32;
                for (s, p, z) in [(0.0, a, 0.0), (len, b, 0.0), (len, b, t), (0.0, a, t)] {
                    m.vertices.push(Vertex { pos: w(p, z), normal, uv: [s as f32, z as f32], part: pi });
                }
                m.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            }
            for z in [0.0, t] {
                m.lines.extend((0..k).map(|i| (w(pts[i], z), w(pts[(i + 1) % k], z), pi)));
            }
            m.lines.extend(ring.corners.iter().map(|&c| (w(pts[c], 0.0), w(pts[c], t), pi)));
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Frame;
    use crate::panel::{Panel, Texture};

    fn part(segs: Vec<Seg>, size: (f64, f64)) -> Part {
        let panel = Panel {
            name: "т".into(),
            pos: String::new(),
            designation: String::new(),
            material: String::new(),
            thickness: 16.0,
            segs,
            size,
            texture: Texture::Default,
            edges: Vec::new(),
            frame: Some(Frame::new([100.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0])),
            holes: Vec::new(),
            grooves: Vec::new(),
        };
        Part { owner: 1, id: 0, kind: Kind::Body, panel, origin: [0.0; 3], size: [0.0; 3] }
    }

    fn tri_area(m: &Mesh, normal: [f32; 3]) -> f64 {
        m.indices
            .chunks(3)
            .filter(|t| m.vertices[t[0] as usize].normal == normal)
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| m.vertices[t[i] as usize].pos.map(f64::from));
                let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
                let x = crate::geom::cross(u, v);
                (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt() / 2.0
            })
            .sum()
    }

    #[test]
    fn l_shaped_panel_is_exact_solid() {
        // L-shaped horizontal panel (like the bottom of a corner unit), clockwise.
        let pts = [[984.0, 0.0], [0.0, 0.0], [0.0, 884.0], [560.0, 884.0], [560.0, 560.0], [984.0, 560.0]];
        let segs = (0..6).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % 6] }).collect();
        let m = build(&[part(segs, (984.0, 884.0))], |_| false);
        let expect = 984.0 * 560.0 + 560.0 * 324.0;
        // Horizontal panel: front face points down (−Y), back face up.
        assert!((tri_area(&m, [0.0, -1.0, 0.0]) - expect).abs() < 1e-3);
        assert!((tri_area(&m, [0.0, 1.0, 0.0]) - expect).abs() < 1e-3);
        // 6 edge faces; edges: 6 + 6 along the faces and 6 verticals.
        assert_eq!(m.lines.len(), 18);
        let ys: Vec<f32> = m.vertices.iter().map(|v| v.pos[1]).collect();
        assert!(ys.iter().all(|&y| y == 0.0 || y == -16.0));
        assert!(m.vertices.iter().all(|v| v.pos[0] >= 100.0 && v.pos[0] <= 1084.0));
    }

    #[test]
    fn arc_follows_tolerance() {
        // Semicircular edge of radius 300: chords no farther than 0.1 mm from the arc, vertical edges only at segment ends.
        let segs = vec![
            Seg::Line { a: [600.0, 0.0], b: [0.0, 0.0] },
            Seg::Line { a: [0.0, 0.0], b: [0.0, 300.0] },
            Seg::Arc { a: [0.0, 300.0], b: [600.0, 300.0], c: [300.0, 300.0], ccw: false },
            Seg::Line { a: [600.0, 300.0], b: [600.0, 0.0] },
        ];
        let m = build(&[part(segs, (600.0, 600.0))], |_| false);
        let r = &rings(&m_segs()).unwrap()[0];
        assert_eq!(r.corners.len(), 4);
        // Arc points (upper semicircle with ends): all on the circle, chords deviate ≤ tolerance.
        let on_arc = r.pts.iter().filter(|p| p[1] >= 300.0 - 1e-9 && ((p[0] - 300.0).hypot(p[1] - 300.0) - 300.0).abs() < 1e-9).count();
        let n = on_arc - 1;
        assert!(n > 10, "{n}");
        let chord = 2.0 * 300.0 * (std::f64::consts::PI / n as f64 / 2.0).sin();
        assert!(300.0 - (300.0f64.powi(2) - (chord / 2.0).powi(2)).sqrt() <= TOLERANCE);
        assert_eq!(m.lines.iter().filter(|l| l.0[1] != l.1[1]).count(), 4);
        let expect = 600.0 * 300.0 + std::f64::consts::PI * 300.0 * 300.0 / 2.0;
        // Chords inside the arc: area smaller by at most arc length × tolerance.
        let lost = expect - tri_area(&m, [0.0, 1.0, 0.0]);
        assert!((0.0..=std::f64::consts::PI * 300.0 * TOLERANCE).contains(&lost), "{lost}");

        fn m_segs() -> Vec<Seg> {
            vec![
                Seg::Line { a: [600.0, 0.0], b: [0.0, 0.0] },
                Seg::Line { a: [0.0, 0.0], b: [0.0, 300.0] },
                Seg::Arc { a: [0.0, 300.0], b: [600.0, 300.0], c: [300.0, 300.0], ccw: false },
                Seg::Line { a: [600.0, 300.0], b: [600.0, 0.0] },
            ]
        }
    }

    /// On all panels of the "Kitchen 24.04" reference the triangulation covers the whole contour: triangle area
    /// of one face equals the polygon area.
    #[test]
    fn kitchen_panels_triangulate_fully() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reference/pairs/kuhnya-24-04/_Кухня 24.04.b3d");
        let Ok(data) = std::fs::read(&path) else { return };
        let ex = crate::panel::panels(&crate::b3d::parse(&data).unwrap());
        let mut holes = 0;
        for p in &ex.panels {
            let rs = rings(&p.segs).unwrap_or_else(|e| panic!("{}: {e}", p.name));
            holes += rs.len() - 1;
            let pts = merge(&rs);
            let tris = triangulate(&pts);
            let want: f64 = rs.iter().map(|r| area(&r.pts)).sum();
            let sum: f64 = tris.iter().map(|t| area(&[pts[t[0]], pts[t[1]], pts[t[2]]])).sum();
            assert!((sum - want).abs() < 1e-6 * want.max(1.0), "{}: {sum} ≠ {want}", p.name);
            assert!(tris.iter().all(|t| area(&[pts[t[0]], pts[t[1]], pts[t[2]]]) >= -1e-9), "{}: вывернутый треугольник", p.name);
        }
        eprintln!("вырезов: {holes}");
    }

    #[test]
    fn hole_is_cut_out() {
        // 100×100 square with a round cutout r = 10 in the centre (the circle is a separate contour).
        let mut segs: Vec<Seg> = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]
            .windows(2)
            .map(|w| Seg::Line { a: w[0], b: w[1] })
            .collect();
        segs.push(Seg::Line { a: [0.0, 100.0], b: [0.0, 0.0] });
        segs.push(Seg::Circle { c: [50.0, 50.0], r: 10.0 });
        let rs = rings(&segs).unwrap();
        assert_eq!(rs.len(), 2);
        assert!(area(&rs[1].pts) < 0.0, "вырез обходится по часовой");
        let pts = merge(&rs);
        let tris = triangulate(&pts);
        let sum: f64 = tris.iter().map(|t| area(&[pts[t[0]], pts[t[1]], pts[t[2]]])).sum();
        let want = 10000.0 + area(&rs[1].pts);
        assert!((sum - want).abs() < 1e-6, "{sum} ≠ {want}");
        // No triangle lies inside the cutout.
        assert!(tris.iter().all(|t| {
            let c = [0, 1].map(|k| (pts[t[0]][k] + pts[t[1]][k] + pts[t[2]][k]) / 3.0);
            (c[0] - 50.0).hypot(c[1] - 50.0) > 9.9
        }));
        let m = build(&[part(segs, (100.0, 100.0))], |_| false);
        // Cutout edge faces point toward the cutout centre.
        let inner: Vec<&Vertex> = m.vertices.iter().filter(|v| (v.pos[0] - 150.0).hypot(v.pos[2] - 50.0) < 10.5).collect();
        assert!(!inner.is_empty());
        assert!(inner.iter().filter(|v| v.normal[1] == 0.0).all(|v| {
            let to_center = [150.0 - v.pos[0], 50.0 - v.pos[2]];
            to_center[0] * v.normal[0] + to_center[1] * v.normal[2] > 0.0
        }));
    }
}
