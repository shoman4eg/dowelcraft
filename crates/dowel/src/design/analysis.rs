//! "Model analysis" (ch. 3.4 of the Bazis manual): panel intersections, fasteners and hardware without a panel,
//! part larger than the sheet. Each issue lists the parts it concerns (they can be selected).

use serde::Serialize;

use super::{Part, Project, build, furniture_lost_holes, hanging_fasteners};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Problem {
    /// Kind (Russian label): intersection, fasteners, hardware or size.
    pub kind: &'static str,
    pub message: String,
    /// Part indices in the `build` list.
    pub parts: Vec<usize>,
}

/// Volume of the intersection of two parts' bounding boxes, mm³ (0 if they touch or don't intersect).
fn overlap(a: &Part, b: &Part) -> Option<([f64; 3], [f64; 3])> {
    let lo: [f64; 3] = std::array::from_fn(|k| a.origin[k].max(b.origin[k]));
    let hi: [f64; 3] = std::array::from_fn(|k| (a.origin[k] + a.size[k]).min(b.origin[k] + b.size[k]));
    // 0.01 mm tolerance: touching faces is not an intersection.
    (0..3).all(|k| hi[k] - lo[k] > 0.01).then_some((lo, hi))
}

/// Whether a project point is inside the part body (by its contour and thickness).
fn inside(p: &Part, w: [f64; 3]) -> bool {
    let Some(f) = p.panel.frame else { return false };
    let l = f.local(w);
    if l[2] < -1e-6 || l[2] > p.panel.thickness + 1e-6 {
        return false;
    }
    // Point in the contour polygon (arcs approximated by chords).
    let Ok(loops) = crate::contour::loops(&p.panel.segs) else { return false };
    let mut crossings = 0;
    for lp in loops {
        let pts: Vec<[f64; 2]> = lp
            .into_iter()
            .flat_map(|(i, rev)| crate::contour::tessellate(&if rev { crate::contour::reversed(&p.panel.segs[i]) } else { p.panel.segs[i] }, 0.1))
            .collect();
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a[1] > l[1]) != (b[1] > l[1]) && l[0] < a[0] + (l[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                crossings += 1;
            }
        }
    }
    crossings % 2 == 1
}

pub fn analyze(p: &Project) -> Result<Vec<Problem>, String> {
    let parts = build(p)?;
    let mut out = Vec::new();
    for i in 0..parts.len() {
        for j in i + 1..parts.len() {
            let Some((lo, hi)) = overlap(&parts[i], &parts[j]) else { continue };
            // A part in another's groove (back panel in a side panel groove) is not an intersection: the whole overlap is in the groove.
            let corners: Vec<[f64; 3]> = (0..8).map(|c| std::array::from_fn(|k| if c >> k & 1 == 0 { lo[k] } else { hi[k] })).collect();
            let in_groove = |p: &Part| p.panel.frame.is_some_and(|f| p.panel.grooves.iter().any(|g| corners.iter().all(|&c| g.contains(f.local(c), 0.01))));
            if in_groove(&parts[i]) || in_groove(&parts[j]) {
                continue;
            }
            // Shaped parts have a bounding box larger than the body: intersection only if the overlap center is inside both.
            let c: [f64; 3] = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
            let figured = !parts[i].panel.is_rect() || !parts[j].panel.is_rect();
            if figured && !(inside(&parts[i], c) && inside(&parts[j], c)) {
                continue;
            }
            let d: Vec<String> = (0..3).map(|k| format!("{:.1}", hi[k] - lo[k])).collect();
            out.push(Problem {
                kind: "Overlap",
                message: format!("“{} {}” and “{} {}” overlap by {} mm", parts[i].panel.pos, parts[i].panel.name, parts[j].panel.pos, parts[j].panel.name, d.join(" × ")),
                parts: vec![i, j],
            });
        }
    }
    for (_, msg) in hanging_fasteners(p) {
        out.push(Problem { kind: "Fasteners", message: msg, parts: Vec::new() });
    }
    for (name, n) in furniture_lost_holes(p) {
        out.push(Problem { kind: "Hardware", message: format!("“{name}”: {n} holes hit no panel"), parts: Vec::new() });
    }
    for (i, part) in parts.iter().enumerate() {
        let name = crate::panel::material_name(&part.panel.material);
        let Some(m) = p.materials.iter().find(|m| m.name == name && m.thickness == part.panel.thickness) else { continue };
        let trim = m.trim.unwrap_or(10.0);
        let (sl, sw) = (m.sheet.0 - 2.0 * trim, m.sheet.1 - 2.0 * trim);
        let (l, w) = part.panel.cut_size();
        // Grained parts are not rotated: length runs along the sheet length.
        let fits = if part.panel.grain() { l <= sl && w <= sw } else { (l <= sl && w <= sw) || (w <= sl && l <= sw) };
        if !fits {
            out.push(Problem {
                kind: "Oversize",
                message: format!("“{} {}” {l:.1}×{w:.1} does not fit a {}×{} sheet (trim {trim} mm)", part.panel.pos, part.panel.name, m.sheet.0, m.sheet.1),
                parts: vec![i],
            });
        }
    }
    Ok(out)
}
