//! Flat part contour (blob `Contour`): u32 n, n segments, then
//! optionally n × u32 segment attributes (meaning not established).

use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Seg {
    /// 0x10
    Line { a: [f64; 2], b: [f64; 2] },
    /// 0x12: center, start, end (6 × f64), direction byte: 1 — counterclockwise.
    Arc { a: [f64; 2], b: [f64; 2], c: [f64; 2], ccw: bool },
    /// 0x11 and 0x14: circle (center, radius); 0x14 apparently differs in traversal direction.
    Circle { c: [f64; 2], r: f64 },
}

pub fn parse(v: &[u8]) -> Option<Vec<Seg>> {
    let f = |p: usize| Some(f64::from_le_bytes(v.get(p..p + 8)?.try_into().ok()?));
    let n = u32::from_le_bytes(v.get(..4)?.try_into().ok()?) as usize;
    let mut p = 4;
    let mut out = Vec::with_capacity(n.min(v.len() / 25));
    for _ in 0..n {
        let k = *v.get(p)?;
        let q = p + 1;
        out.push(match k {
            0x10 => Seg::Line { a: [f(q)?, f(q + 8)?], b: [f(q + 16)?, f(q + 24)?] },
            0x12 => Seg::Arc {
                c: [f(q)?, f(q + 8)?],
                a: [f(q + 16)?, f(q + 24)?],
                b: [f(q + 32)?, f(q + 40)?],
                ccw: *v.get(q + 48)? != 0,
            },
            0x11 | 0x14 => Seg::Circle { c: [f(q)?, f(q + 8)?], r: f(q + 16)? },
            _ => return None,
        });
        p += match k {
            0x10 => 33,
            0x12 => 50,
            _ => 25,
        };
    }
    let rest = v.len() - p;
    (rest == 0 || rest == 4 * n).then_some(out)
}

/// Segment length, mm.
pub fn length(s: &Seg) -> f64 {
    match *s {
        Seg::Line { a, b } => (b[0] - a[0]).hypot(b[1] - a[1]),
        Seg::Arc { a, b, c, ccw } => {
            let ang = |p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
            let sweep = if ccw { ang(b) - ang(a) } else { ang(a) - ang(b) }.rem_euclid(TAU);
            (a[0] - c[0]).hypot(a[1] - c[1]) * if sweep < 1e-12 { TAU } else { sweep }
        }
        Seg::Circle { r, .. } => TAU * r,
    }
}

/// Segment start and end (for a circle — the same point on it).
pub fn ends(s: &Seg) -> ([f64; 2], [f64; 2]) {
    match *s {
        Seg::Line { a, b } | Seg::Arc { a, b, .. } => (a, b),
        Seg::Circle { c, r } => ([c[0] + r, c[1]], [c[0] + r, c[1]]),
    }
}

/// The same segment traversed in reverse.
pub fn reversed(s: &Seg) -> Seg {
    match *s {
        Seg::Line { a, b } => Seg::Line { a: b, b: a },
        Seg::Arc { a, b, c, ccw } => Seg::Arc { a: b, b: a, c, ccw: !ccw },
        circle => circle,
    }
}

/// Closed contour: segment indices in traversal order and a "traversed in reverse" flag.
pub type Loop = Vec<(usize, bool)>;

/// Chains segments into closed contours by matching endpoints: Bazis stores segments
/// out of traversal order, some reversed. Error if a contour does not close.
pub fn loops(segs: &[Seg]) -> Result<Vec<Loop>, String> {
    const EPS: f64 = 1e-3;
    let near = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).abs() < EPS && (p[1] - q[1]).abs() < EPS;
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    while let Some(first) = used.iter().position(|u| !u) {
        used[first] = true;
        let mut lp = vec![(first, false)];
        let (start, mut end) = ends(&segs[first]);
        while !near(start, end) {
            let next = (0..segs.len()).filter(|&i| !used[i]).find_map(|i| {
                let (a, b) = ends(&segs[i]);
                if near(a, end) {
                    Some((i, false))
                } else if near(b, end) {
                    Some((i, true))
                } else {
                    None
                }
            });
            let Some((i, rev)) = next else {
                return Err(format!("the contour is not closed: nothing continues from point {end:?}"));
            };
            used[i] = true;
            lp.push((i, rev));
            let (a, b) = ends(&segs[i]);
            end = if rev { a } else { b };
        }
        out.push(lp);
    }
    Ok(out)
}

/// Segment points from start to end (end excluded); arcs as chords with deviation at most `tol`.
pub fn tessellate(s: &Seg, tol: f64) -> Vec<[f64; 2]> {
    let steps = |r: f64, sweep: f64| -> usize {
        if r <= tol {
            return 1;
        }
        ((sweep / (2.0 * (1.0 - tol / r).acos())).ceil() as usize).clamp(1, 512)
    };
    match *s {
        Seg::Line { a, .. } => vec![a],
        Seg::Arc { a, b, c, ccw } => {
            let r = (a[0] - c[0]).hypot(a[1] - c[1]);
            let ang = |p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
            let a0 = ang(a);
            let sweep = if ccw { ang(b) - a0 } else { a0 - ang(b) }.rem_euclid(TAU);
            let sweep = if sweep < 1e-12 { TAU } else { sweep };
            let n = steps(r, sweep);
            let sign = if ccw { 1.0 } else { -1.0 };
            (0..n)
                .map(|i| {
                    let t = a0 + sign * sweep * i as f64 / n as f64;
                    if i == 0 { a } else { [c[0] + r * t.cos(), c[1] + r * t.sin()] }
                })
                .collect()
        }
        Seg::Circle { c, r } => {
            let n = steps(r, TAU);
            (0..n).map(|i| {
                let t = TAU * i as f64 / n as f64;
                [c[0] + r * t.cos(), c[1] + r * t.sin()]
            }).collect()
        }
    }
}

/// Contour check, as on "Finish editing" in Bazis: all contours are closed, none
/// intersects itself or another, cutouts lie inside the outer contour.
pub fn validate(segs: &[Seg]) -> Result<(), String> {
    if segs.is_empty() {
        return Err("the contour is empty".into());
    }
    let rings: Vec<Vec<[f64; 2]>> = loops(segs)?
        .into_iter()
        .map(|lp| lp.into_iter().flat_map(|(i, rev)| tessellate(&if rev { reversed(&segs[i]) } else { segs[i] }, 0.05)).collect())
        .collect();
    let area = |p: &[[f64; 2]]| (0..p.len()).map(|i| { let (a, b) = (p[i], p[(i + 1) % p.len()]); a[0] * b[1] - b[0] * a[1] }).sum::<f64>() / 2.0;
    if rings.iter().any(|r| r.len() < 3 || area(r).abs() < 1e-6) {
        return Err("the contour is degenerate: zero area".into());
    }
    // Edges of all contours: (contour, index, start, end).
    let edges: Vec<(usize, usize, [f64; 2], [f64; 2])> = rings.iter().enumerate().flat_map(|(r, pts)| (0..pts.len()).map(move |i| (r, i, pts[i], pts[(i + 1) % pts.len()]))).collect();
    let o = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]);
    for (x, &(ra, ia, a0, a1)) in edges.iter().enumerate() {
        for &(rb, ib, b0, b1) in &edges[x + 1..] {
            let n = rings[ra].len();
            // Adjacent edges of one contour touch at endpoints — that is not an intersection.
            if ra == rb && (ia + 1) % n == ib || ra == rb && (ib + 1) % n == ia {
                continue;
            }
            let (d1, d2, d3, d4) = (o(b0, b1, a0), o(b0, b1, a1), o(a0, a1, b0), o(a0, a1, b1));
            if d1 * d2 < -1e-9 && d3 * d4 < -1e-9 {
                return Err(format!("the contour crosses itself near ({:.1}, {:.1})", a0[0], a0[1]));
            }
        }
    }
    let outer = (0..rings.len()).max_by(|&a, &b| area(&rings[a]).abs().total_cmp(&area(&rings[b]).abs())).unwrap_or(0);
    let inside = |p: [f64; 2], poly: &[[f64; 2]]| {
        let mut c = false;
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                c = !c;
            }
        }
        c
    };
    for (i, r) in rings.iter().enumerate() {
        if i != outer && !inside(r[0], &rings[outer]) {
            return Err(format!("the cut-out at ({:.1}, {:.1}) lies outside the panel", r[0][0], r[0][1]));
        }
    }
    Ok(())
}

/// Bounding box [min, max] of the contour, accounting for arc bulge.
pub fn bounds(segs: &[Seg]) -> Option<([f64; 2], [f64; 2])> {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let mut add = |p: [f64; 2]| {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    };
    for s in segs {
        match *s {
            Seg::Line { a, b } => {
                add(a);
                add(b);
            }
            Seg::Arc { a, b, c, ccw } => {
                add(a);
                add(b);
                let r = (a[0] - c[0]).hypot(a[1] - c[1]);
                let (s, e) = if ccw { (a, b) } else { (b, a) };
                let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
                let sweep = ((e[1] - c[1]).atan2(e[0] - c[0]) - a0).rem_euclid(TAU);
                for q in 0..4 {
                    let ang = q as f64 * TAU / 4.0;
                    if (ang - a0).rem_euclid(TAU) <= sweep {
                        add([c[0] + r * ang.cos(), c[1] + r * ang.sin()]);
                    }
                }
            }
            Seg::Circle { c, r } => {
                add([c[0] - r, c[1] - r]);
                add([c[0] + r, c[1] + r]);
            }
        }
    }
    lo[0].is_finite().then_some((lo, hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_bounds_follow_direction() {
        // Half circle r=1 from (1,0) to (-1,0): counterclockwise goes over the top.
        let up = Seg::Arc { a: [1.0, 0.0], b: [-1.0, 0.0], c: [0.0, 0.0], ccw: true };
        let down = Seg::Arc { a: [1.0, 0.0], b: [-1.0, 0.0], c: [0.0, 0.0], ccw: false };
        let (lo, hi) = bounds(&[up]).unwrap();
        assert!((hi[1] - 1.0).abs() < 1e-9 && lo[1].abs() < 1e-9);
        let (lo, hi) = bounds(&[down]).unwrap();
        assert!((lo[1] + 1.0).abs() < 1e-9 && hi[1].abs() < 1e-9);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse(&[1, 0, 0, 0, 0x99]), None);
        assert_eq!(parse(&[1, 0, 0]), None);
    }

    #[test]
    fn loops_join_unordered_and_reversed_segments() {
        // Rectangle 10×5: segments shuffled, two reversed; plus a separate circular cutout.
        let segs = [
            Seg::Line { a: [0.0, 0.0], b: [10.0, 0.0] },
            Seg::Line { a: [0.0, 5.0], b: [0.0, 0.0] },
            Seg::Line { a: [10.0, 5.0], b: [10.0, 0.0] },
            Seg::Circle { c: [5.0, 2.5], r: 1.0 },
            Seg::Arc { a: [0.0, 5.0], b: [10.0, 5.0], c: [5.0, 5.0], ccw: true },
        ];
        let l = loops(&segs).unwrap();
        assert_eq!(l, vec![vec![(0, false), (2, true), (4, true), (1, false)], vec![(3, false)]]);
        assert_eq!(reversed(&segs[4]), Seg::Arc { a: [10.0, 5.0], b: [0.0, 5.0], c: [5.0, 5.0], ccw: false });
        assert!(loops(&segs[..2]).is_err());
    }

    #[test]
    fn validate_catches_bad_contours() {
        let rect = |w: f64, h: f64, x: f64, y: f64| -> Vec<Seg> {
            let p = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
            (0..4).map(|i| Seg::Line { a: p[i], b: p[(i + 1) % 4] }).collect()
        };
        validate(&rect(100.0, 50.0, 0.0, 0.0)).unwrap();
        let mut with_hole = rect(100.0, 50.0, 0.0, 0.0);
        with_hole.push(Seg::Circle { c: [50.0, 25.0], r: 10.0 });
        validate(&with_hole).unwrap();
        let mut outside = rect(100.0, 50.0, 0.0, 0.0);
        outside.push(Seg::Circle { c: [500.0, 25.0], r: 10.0 });
        assert!(validate(&outside).unwrap_err().contains("outside"));
        // "Bow tie": self-intersection.
        let bow = [[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 6.0]];
        let bow: Vec<Seg> = (0..4).map(|i| Seg::Line { a: bow[i], b: bow[(i + 1) % 4] }).collect();
        assert!(validate(&bow).unwrap_err().contains("crosses itself"));
        assert!(validate(&rect(100.0, 50.0, 0.0, 0.0)[..3]).is_err(), "not closed");
    }
}
