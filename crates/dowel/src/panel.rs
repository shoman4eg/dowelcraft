//! Parts (panels, Type 4002) from the model tree and their parts list.

use crate::b3d::{File, Node, Value};
use crate::contour::{self, Seg};
use crate::geom::Frame;

/// Panel in the `Document/Model` tree.
pub const TYPE_PANEL: i64 = 4002;

/// Grain direction (`TexDir`): 1 — along contour X, 2 — along Y, none — default.
/// Checked against BAZIS-Raskroy reference projects: horizontal parts — angle 0, uprights — π/2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Texture {
    Default,
    Horizontal,
    Vertical,
}

/// Side of the bounding rectangle in contour axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RectSide {
    Bottom,
    Left,
    Top,
    Right,
}

/// Side in parts-list axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Sides along the length.
    L1,
    L2,
    /// Sides along the width.
    W1,
    W2,
    /// Curved or slanted segment.
    Other,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    /// Material in Bazis format `Name\rArticle`.
    pub material: String,
    pub thickness: f64,
    /// Banding width (`Width`), 0 — not set.
    pub width: f64,
    pub allowance: f64,
    /// `ClipPanel`: the panel is trimmed by the edge banding thickness.
    pub clip: bool,
    /// Contour segment index (`Elem`).
    pub segment: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub name: String,
    /// Position number (`ArtPos`).
    pub pos: String,
    /// Designation (`Des`).
    pub designation: String,
    pub material: String,
    pub thickness: f64,
    /// Contour shifted so the bounding box starts at (0, 0).
    pub segs: Vec<Seg>,
    /// Contour bounding box in X and Y, mm.
    pub size: (f64, f64),
    pub texture: Texture,
    pub edges: Vec<Edge>,
    /// Placement in model space: origin — contour bounding-box corner (0, 0), thickness along panel +Z.
    /// Model axes: X right, Y up, Z toward the viewer. None — part without placement.
    pub frame: Option<Frame>,
    /// Drilling: fastener holes that landed in this panel.
    pub holes: Vec<Hole>,
    pub grooves: Vec<Groove>,
}

/// Groove along a straight path (`Cuts/Cut` of a Bazis panel), in panel contour axes. Cross-section is a rectangle:
/// across the path from `across[0]` to `across[1]` (to the left of the path direction) and through panel thickness
/// from `z[0]` to `z[1]` (Z — from the back face). Grooves along a curved path are not parsed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Groove {
    pub name: String,
    pub path: [[f64; 2]; 2],
    pub across: [f64; 2],
    pub z: [f64; 2],
}

impl Groove {
    /// Point in panel axes lies in the groove (with tolerance `eps`).
    pub fn contains(&self, p: [f64; 3], eps: f64) -> bool {
        let [a, b] = self.path;
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
        if len < 1e-9 {
            return false;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (rx, ry) = (p[0] - a[0], p[1] - a[1]);
        let t = rx * ux + ry * uy;
        let s = -rx * uy + ry * ux;
        t >= -eps && t <= len + eps && s >= self.across[0] - eps && s <= self.across[1] + eps && p[2] >= self.z[0] - eps && p[2] <= self.z[1] + eps
    }
}

/// Hole in panel coordinates: X, Y — as for the contour (from 0), Z — from the back face (0) to the front face (thickness).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hole {
    pub pos: [f64; 3],
    /// Unit drilling direction.
    pub dir: [f64; 3],
    pub diameter: f64,
    pub depth: f64,
}

const EPS: f64 = 0.01;

impl Panel {
    /// Bounding box in model space ([min], [max]), mm.
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        Some(self.frame?.bounds([0.0; 3], [self.size.0, self.size.1, self.thickness]))
    }

    /// Bounding-box side the contour segment lies on.
    pub fn rect_side(&self, seg: usize) -> Option<RectSide> {
        let Seg::Line { a, b } = *self.segs.get(seg)? else { return None };
        let (sx, sy) = self.size;
        let on = |v: f64, at: f64| (v - at).abs() < EPS;
        if on(a[1], b[1]) && on(a[1], 0.0) {
            Some(RectSide::Bottom)
        } else if on(a[1], b[1]) && on(a[1], sy) {
            Some(RectSide::Top)
        } else if on(a[0], b[0]) && on(a[0], 0.0) {
            Some(RectSide::Left)
        } else if on(a[0], b[0]) && on(a[0], sx) {
            Some(RectSide::Right)
        } else {
            None
        }
    }

    /// Contour is a rectangle of four segments along the bounding box.
    pub fn is_rect(&self) -> bool {
        self.segs.len() == 4 && (0..4).all(|i| self.rect_side(i).is_some())
    }

    /// Parts-list length runs along contour Y: grain is vertical, or without grain Y is longer.
    fn along_y(&self) -> bool {
        match self.texture {
            Texture::Vertical => true,
            Texture::Horizontal => false,
            Texture::Default => self.size.1 > self.size.0,
        }
    }

    pub fn grain(&self) -> bool {
        self.texture != Texture::Default
    }

    pub fn side(&self, e: &Edge) -> Side {
        let r = self.rect_side(e.segment);
        match (r, self.along_y()) {
            (Some(RectSide::Bottom), false) | (Some(RectSide::Left), true) => Side::L1,
            (Some(RectSide::Top), false) | (Some(RectSide::Right), true) => Side::L2,
            (Some(RectSide::Left), false) | (Some(RectSide::Bottom), true) => Side::W1,
            (Some(RectSide::Right), false) | (Some(RectSide::Top), true) => Side::W2,
            (None, _) => Side::Other,
        }
    }

    fn trim(&self, side: RectSide) -> f64 {
        // A side may consist of several segments — take the thickest edge banding.
        self.edges
            .iter()
            .filter(|e| self.rect_side(e.segment) == Some(side))
            .map(|e| if e.clip { e.thickness } else { 0.0 } - e.allowance)
            // An allowance larger than the banding thickness makes the blank larger than the bounding box; no banding — 0.
            .reduce(f64::max)
            .unwrap_or(0.0)
    }

    /// Blank in contour axes: bounding box minus thickness of trimming (`ClipPanel`) edge banding plus allowance.
    /// shortcut: edge banding on curved segments is ignored — for shaped parts BAZIS-Raskroy gives a slightly smaller size.
    pub fn blank(&self) -> (f64, f64) {
        (
            self.size.0 - self.trim(RectSide::Left) - self.trim(RectSide::Right),
            self.size.1 - self.trim(RectSide::Bottom) - self.trim(RectSide::Top),
        )
    }

    /// Cut size in parts-list axes: (length, width).
    pub fn cut_size(&self) -> (f64, f64) {
        let (x, y) = self.blank();
        if self.along_y() { (y, x) } else { (x, y) }
    }
}

#[derive(Debug, Default)]
pub struct Extract {
    pub panels: Vec<Panel>,
    /// Panels whose contour could not be parsed (name).
    pub skipped: Vec<String>,
    /// Fastener holes that landed in no panel.
    pub holes_lost: usize,
    /// Model blocks (Type 1005) in traversal order.
    pub blocks: Vec<BlockInfo>,
    /// Block containing each panel, in `panels` order.
    pub panel_block: Vec<Option<usize>>,
}

/// Bazis model block: name and parent block (index into `Extract::blocks`).
#[derive(Debug, Clone, PartialEq)]
pub struct BlockInfo {
    pub name: String,
    pub parent: Option<usize>,
}

/// Block (assembly unit) in the `Document/Model` tree.
pub const TYPE_BLOCK: i64 = 1005;

/// Largest gap between hole start and panel, mm (thickness of a hinge plate, hanger).
const GAP: f64 = 5.0;

pub fn panels(file: &File) -> Extract {
    let mut out = Extract::default();
    let mut frames = Vec::new();
    if let Some(model) = file.section("Document").and_then(|d| d.get("Model")) {
        walk(model, &mut out, &mut frames, &IDENTITY, None);
        out.holes_lost = assign(&mut out.panels, &world_holes(file));
    }
    out
}

/// Hole in model coordinates: start, drilling direction, diameter, depth.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorldHole {
    pub pos: [f64; 3],
    pub dir: [f64; 3],
    pub diameter: f64,
    pub depth: f64,
}

/// Fastener holes of a Bazis model in its coordinates. A fastener definition (`Document/FurnList`, by `FastID`)
/// stores holes in its own coordinates; an instance (Type 3001) places them into the model. Standalone holes — Type 3000.
pub fn world_holes(file: &File) -> Vec<WorldHole> {
    let Some(model) = file.section("Document").and_then(|d| d.get("Model")) else { return Vec::new() };
    let defs: Vec<(i64, &Node)> = file
        .section("Document")
        .and_then(|d| d.get("FurnList"))
        .map(|l| l.children.iter().filter_map(|f| Some((f.int("FastID")?, f))).collect())
        .unwrap_or_default();
    let mut world = Vec::new();
    fn holes(n: &Node) -> impl Iterator<Item = ([f64; 3], [f64; 3], f64, f64)> + '_ {
        n.children.iter().filter(|h| h.name == "Hole").map(|h| {
            let g = |k: &str| h.f64(k).unwrap_or(0.0);
            ([g("X"), g("Y"), g("Z")], [g("DirX"), g("DirY"), g("DirZ")], 2.0 * g("Radius"), g("Depth"))
        })
    }
    fn collect(n: &Node, parent: &Xform, defs: &[(i64, &Node)], out: &mut Vec<WorldHole>) {
        let xf = if n.get("Type").is_some() { parent.then(n) } else { *parent };
        let mut put = |p: [f64; 3], d: [f64; 3], diameter: f64, depth: f64| {
            let r = &xf.r;
            out.push(WorldHole { pos: xf.apply(p), dir: std::array::from_fn(|i| r[i][0] * d[0] + r[i][1] * d[1] + r[i][2] * d[2]), diameter, depth });
        };
        match n.int("Type") {
            Some(3001) => {
                if let Some((_, def)) = n.int("FastID").and_then(|id| defs.iter().find(|(f, _)| *f == id))
                    && let Some(hs) = def.get("Holes")
                {
                    holes(hs).for_each(|(p, d, dia, depth)| put(p, d, dia, depth));
                }
            }
            Some(3000) => {
                let g = |k: &str| n.f64(k).unwrap_or(0.0);
                put([g("X"), g("Y"), g("Z")], [g("DirX"), g("DirY"), g("DirZ")], 2.0 * g("Radius"), g("Depth"));
            }
            _ => {}
        }
        for c in &n.children {
            collect(c, &xf, defs, out);
        }
    }
    collect(model, &IDENTITY, &defs, &mut world);
    world
}

/// Hardware mesh triangle in model coordinates, mm.
pub type Tri = [[f32; 3]; 3];

/// Mesh from a fastener definition's `TriData`: u32 group count; each group — material (u32 length + UTF-16),
/// 4×4 f64 matrix (row-major, translation in the last row), byte flag, u32 triangle count
/// and 112 bytes per triangle: 3 vertices, 4 normals and 6 texture coordinates (f32), u32 flags.
pub fn tri_data(b: &[u8]) -> Result<Vec<Tri>, String> {
    let mut o = 0usize;
    let mut take = |n: usize| -> Result<&[u8], String> {
        let s = b.get(o..o + n).ok_or_else(|| format!("TriData: ends at {o} (+{n} of {})", b.len()))?;
        o += n;
        Ok(s)
    };
    let u32_ = |s: &[u8]| u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
    let groups = u32_(take(4)?);
    let mut out = Vec::new();
    for _ in 0..groups {
        let name_len = u32_(take(4)?) as usize;
        take(2 * name_len)?;
        let m: Vec<f64> = take(128)?.as_chunks::<8>().0.iter().map(|c| f64::from_le_bytes(*c)).collect();
        take(1)?;
        let count = u32_(take(4)?) as usize;
        for _ in 0..count {
            let t = take(112)?;
            let f = |i: usize| f32::from_le_bytes([t[4 * i], t[4 * i + 1], t[4 * i + 2], t[4 * i + 3]]) as f64;
            out.push(std::array::from_fn(|v| {
                let p = [f(3 * v), f(3 * v + 1), f(3 * v + 2)];
                std::array::from_fn(|j| (p[0] * m[j] + p[1] * m[4 + j] + p[2] * m[8 + j] + m[12 + j]) as f32)
            }));
        }
    }
    Ok(out)
}

/// Mesh of the whole model: fastener instances (Type 3001) with the definition mesh (`FurnList`, `TriData`), in model coordinates.
pub fn world_mesh(file: &File) -> Vec<Tri> {
    let Some(doc) = file.section("Document") else { return Vec::new() };
    let Some(model) = doc.get("Model") else { return Vec::new() };
    let defs: Vec<(i64, Vec<Tri>)> = doc
        .get("FurnList")
        .map(|l| l.children.iter().filter_map(|f| Some((f.int("FastID")?, tri_data(f.blob("TriData")?).ok()?))).collect())
        .unwrap_or_default();
    fn collect(n: &Node, parent: &Xform, defs: &[(i64, Vec<Tri>)], out: &mut Vec<Tri>) {
        let xf = if n.get("Type").is_some() { parent.then(n) } else { *parent };
        if n.int("Type") == Some(3001)
            && let Some((_, tris)) = n.int("FastID").and_then(|id| defs.iter().find(|(f, _)| *f == id))
        {
            out.extend(tris.iter().map(|t| t.map(|p| xf.apply(p.map(f64::from)).map(|v| v as f32))));
        }
        for c in &n.children {
            collect(c, &xf, defs, out);
        }
    }
    let mut out = Vec::new();
    collect(model, &IDENTITY, &defs, &mut out);
    out
}

/// Holes → panels: a hole goes to the panel the drilling ray enters no farther than `GAP` from the start
/// (the screw passes through a hinge plate, hanger, etc.); the entry point becomes the hole start.
/// Returns the number of holes that landed in no panel.
/// shortcut: the "inside the panel" check uses the contour bounding box — shaped panels may get false hits.
pub fn assign(panels: &mut [Panel], holes: &[WorldHole]) -> usize {
    let mut lost = 0;
    'hole: for h in holes {
        let p = h.pos;
        for panel in panels.iter_mut() {
            let (Some((lo, hi)), Some(f)) = (panel.bounds(), panel.frame) else { continue };
            if (0..3).any(|k| p[k] < lo[k] - 1.0 || p[k] > hi[k] + 1.0) {
                continue;
            }
            let l = f.local(p);
            let ld = [f.x, f.y, f.z].map(|a| a[0] * h.dir[0] + a[1] * h.dir[1] + a[2] * h.dir[2]);
            let hi = [panel.size.0, panel.size.1, panel.thickness];
            // Ray entry into the panel bounding box (slab method): entry and exit parameter per axis.
            let (mut t0, mut t1) = (f64::MIN, f64::MAX);
            for k in 0..3 {
                if ld[k].abs() < 1e-9 {
                    if l[k] < -0.5 || l[k] > hi[k] + 0.5 {
                        t1 = f64::MIN;
                    }
                } else {
                    let (a, b) = ((0.0 - l[k]) / ld[k], (hi[k] - l[k]) / ld[k]);
                    t0 = t0.max(a.min(b));
                    t1 = t1.min(a.max(b));
                }
            }
            if (-0.5..=GAP).contains(&t0) && t1 - t0 > 0.5 {
                let pos = std::array::from_fn(|k| l[k] + ld[k] * t0.max(0.0));
                panel.holes.push(Hole { pos, dir: ld, diameter: h.diameter, depth: (h.depth - t0.max(0.0)).max(0.0) });
                continue 'hole;
            }
        }
        lost += 1;
    }
    lost
}

/// Local → world coordinate transform: rotation and offset.
#[derive(Clone, Copy)]
struct Xform {
    r: [[f64; 3]; 3],
    t: [f64; 3],
}

const IDENTITY: Xform = Xform { r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] };

impl Xform {
    fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let r = &self.r;
        std::array::from_fn(|i| r[i][0] * p[0] + r[i][1] * p[1] + r[i][2] * p[2] + self.t[i])
    }

    /// Object `Trans`: X, Y, Z and quaternion Rx, Ry, Rz, Rw relative to the parent.
    fn then(&self, n: &Node) -> Xform {
        let Some(tr) = n.get("Trans") else { return *self };
        let g = |k: &str, d: f64| tr.f64(k).unwrap_or(d);
        let (x, y, z, w) = (g("Rx", 0.0), g("Ry", 0.0), g("Rz", 0.0), g("Rw", 1.0));
        let q = [
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
            [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
            [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
        ];
        let r = std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| self.r[i][k] * q[k][j]).sum()));
        let local = [g("X", 0.0), g("Y", 0.0), g("Z", 0.0)];
        Xform { r, t: self.apply(local) }
    }
}

fn col(f: &Xform, j: usize) -> [f64; 3] {
    std::array::from_fn(|i| f.r[i][j])
}

fn walk(n: &Node, out: &mut Extract, frames: &mut Vec<Xform>, parent: &Xform, block: Option<usize>) {
    let xf = if n.get("Type").is_some() { parent.then(n) } else { *parent };
    let block = if n.int("Type") == Some(TYPE_BLOCK) {
        out.blocks.push(BlockInfo { name: n.str("Name").unwrap_or_default().to_string(), parent: block });
        Some(out.blocks.len() - 1)
    } else {
        block
    };
    // Zero-thickness panels are images inside hardware (caps, mounts); they don't go to nesting.
    if n.int("Type") == Some(TYPE_PANEL) && n.f64("Thick").is_some_and(|t| t > 0.0) {
        match panel(n, &xf) {
            Some((p, frame)) => {
                out.panels.push(p);
                frames.push(frame);
                out.panel_block.push(block);
            }
            None => out.skipped.push(n.str("Name").unwrap_or_default().to_string()),
        }
    }
    for c in &n.children {
        walk(c, out, frames, &xf, block);
    }
}

/// Panel and its coordinate frame (origin at the contour bounding-box corner, as for `segs`).
fn panel(n: &Node, xf: &Xform) -> Option<(Panel, Xform)> {
    let segs = contour::parse(n.blob("Contour")?)?;
    let (lo, hi) = contour::bounds(&segs)?;
    // The contour lies in the panel XY plane, thickness goes along +Z (checked on a cabinet: shelves sit between side panels).
    let sh = |p: [f64; 2]| [p[0] - lo[0], p[1] - lo[1]];
    let segs = segs
        .into_iter()
        .map(|s| match s {
            Seg::Line { a, b } => Seg::Line { a: sh(a), b: sh(b) },
            Seg::Arc { a, b, c, ccw } => Seg::Arc { a: sh(a), b: sh(b), c: sh(c), ccw },
            Seg::Circle { c, r } => Seg::Circle { c: sh(c), r },
        })
        .collect();
    let edges = n
        .get("Butts")
        .map(|b| &b.children[..])
        .unwrap_or_default()
        .iter()
        .filter_map(|b| {
            Some(Edge {
                material: b.str("Mat").unwrap_or_default().to_string(),
                thickness: b.f64("Thick").unwrap_or(0.0),
                width: b.f64("Width").unwrap_or(0.0),
                allowance: b.f64("Allowance").unwrap_or(0.0),
                clip: matches!(b.get("Clip").map(|c| &c.value), Some(Value::True)),
                segment: b.int("Elem")? as usize,
            })
        })
        .collect();
    let frame = Xform { r: xf.r, t: xf.apply([lo[0], lo[1], 0.0]) };
    let grooves = n
        .get("Cuts")
        .map(|c| &c.children[..])
        .unwrap_or_default()
        .iter()
        .filter_map(|c| {
            let path = contour::parse(c.blob("Trajectory")?)?;
            let profile = contour::parse(c.blob("Contour")?)?;
            let [Seg::Line { a, b }] = path[..] else { return None };
            let (plo, phi) = contour::bounds(&profile)?;
            let sh = |p: [f64; 2]| [p[0] - lo[0], p[1] - lo[1]];
            Some(Groove { name: c.str("Name").unwrap_or_default().to_string(), path: [sh(a), sh(b)], across: [plo[0], phi[0]], z: [plo[1], phi[1]] })
        })
        .collect();
    let panel = Panel {
        name: n.str("Name").unwrap_or_default().to_string(),
        pos: n.str("ArtPos").unwrap_or_default().to_string(),
        designation: n.str("Des").unwrap_or_default().to_string(),
        material: n.str("Mat").unwrap_or_default().to_string(),
        thickness: n.f64("Thick").unwrap_or(0.0),
        segs,
        size: (hi[0] - lo[0], hi[1] - lo[1]),
        texture: match n.int("TexDir") {
            Some(1) => Texture::Horizontal,
            Some(2) => Texture::Vertical,
            _ => Texture::Default,
        },
        edges,
        frame: Some(Frame { origin: frame.t, x: col(&frame, 0), y: col(&frame, 1), z: col(&frame, 2) }),
        holes: Vec::new(),
        grooves,
    };
    Some((panel, frame))
}

/// Parts-list row: identical parts merged into one position.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecRow {
    pub panel: Panel,
    pub count: usize,
}

/// Grouping by material, position number, name, sizes and edge banding.
pub fn spec(panels: &[Panel]) -> Vec<SpecRow> {
    let key = |p: &Panel| {
        let (l, w) = p.cut_size();
        let edges: Vec<_> = p.edges.iter().map(|e| (format!("{:?}", p.side(e)), e.material.clone(), mm(e.thickness))).collect();
        (p.material.clone(), mm(p.thickness), p.pos.clone(), p.name.clone(), mm(l), mm(w), p.grain(), edges)
    };
    let mut rows: Vec<SpecRow> = Vec::new();
    for p in panels {
        match rows.iter_mut().find(|r| key(&r.panel) == key(p)) {
            Some(r) => r.count += 1,
            None => rows.push(SpecRow { panel: p.clone(), count: 1 }),
        }
    }
    rows.sort_by(|a, b| {
        (&a.panel.material, pos_num(&a.panel.pos), &a.panel.name).cmp(&(&b.panel.material, pos_num(&b.panel.pos), &b.panel.name))
    });
    rows
}

/// Bazis model hardware: (name `Name\rArticle`, quantity), by name.
/// Counts fasteners (3001) and purchased items (4001, 4004) — a purchased item as a whole, without its contents.
/// Objects with the `BazisFlags & 2` flag and everything inside them are excluded from the estimate — checked against the estimate
/// `reference/pairs/sanuzly` (this excludes, e.g., hinge cups inside hinge blocks).
pub fn hardware(file: &File) -> Vec<(String, usize)> {
    fn walk(n: &Node, out: &mut Vec<(String, usize)>, excluded: bool) {
        let ty = n.int("Type");
        let excluded = excluded || (ty.is_some() && n.int("BazisFlags").is_some_and(|f| f & 2 != 0));
        if matches!(ty, Some(3001 | 4001 | 4004)) && !excluded {
            let name = n.str("Name").map(str::to_string).unwrap_or_else(|| crate::i18n::t("Unnamed"));
            match out.iter_mut().find(|(m, _)| *m == name) {
                Some(row) => row.1 += 1,
                None => out.push((name, 1)),
            }
            if ty != Some(3001) {
                return;
            }
        }
        for c in &n.children {
            walk(c, out, excluded);
        }
    }
    let mut out = Vec::new();
    if let Some(model) = file.section("Document").and_then(|d| d.get("Model")) {
        walk(model, &mut out, false);
    }
    out.sort();
    out
}

/// Material usage row, as in the Bazis cost estimate.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialRow {
    /// Material in `Name\rArticle` format.
    pub material: String,
    /// Unit as in Russian output: m² for panels, m for edge banding.
    pub unit: &'static str,
    pub coef: f64,
    pub qty: f64,
    /// Without the coefficient: m² of parts, m of edge banding, pieces.
    pub net: f64,
    /// Price per unit; 0 — not set.
    pub price: f64,
    /// Weight of the parts, kg (boards with a known weight per m²); 0 — not known.
    pub weight: f64,
}

/// Material usage: panels — sum of bounding boxes (m²), edge banding — length of banded segments (m);
/// multiplied by the material coefficient and rounded up: m² to 0.1, m to a whole number.
/// Checked against the Bazis estimate `reference/pairs/sanuzly`.
pub fn materials(panels: &[Panel], coef: impl Fn(&str) -> f64) -> Vec<MaterialRow> {
    let mut sums: Vec<(String, &'static str, f64)> = Vec::new();
    let mut add = |m: &str, unit: &'static str, v: f64| match sums.iter_mut().find(|(n, u, _)| n == m && *u == unit) {
        Some(row) => row.2 += v,
        None => sums.push((m.to_string(), unit, v)),
    };
    for p in panels {
        add(&p.material, "m²", p.size.0 * p.size.1 / 1e6);
        for e in p.edges.iter().filter(|e| e.thickness > 0.0) {
            add(&e.material, "m", p.segs.get(e.segment).map_or(0.0, contour::length) / 1e3);
        }
    }
    let mut rows: Vec<MaterialRow> = sums
        .into_iter()
        .map(|(material, unit, v)| {
            let k = coef(&material);
            // Drop float noise before rounding up: 2.0000000001 → 2.0.
            let up = |x: f64, step: f64| ((x / step - 1e-9).ceil() * step * 1e6).round() / 1e6;
            let qty = if unit == "m" { up(v * k, 1.0) } else { up(v * k, 0.1) };
            MaterialRow { material, unit, coef: k, qty, net: v, price: 0.0, weight: 0.0 }
        })
        .collect();
    rows.sort_by(|a, b| a.material.cmp(&b.material));
    rows
}

/// Size comparison to 0.01 mm (models have noise like 1868.0000000001).
fn mm(v: f64) -> i64 {
    (v * 100.0).round() as i64
}

fn pos_num(s: &str) -> (u64, String) {
    (s.trim().parse().unwrap_or(u64::MAX), s.to_string())
}

/// Material name without the article number.
pub fn material_name(m: &str) -> &str {
    m.split('\r').next().unwrap_or(m)
}

/// Node value — only for debug output of the tree.
pub fn describe(v: &Value) -> String {
    match v {
        Value::Obj => String::new(),
        Value::Blob(b) => format!("<{} bytes>", b.len()),
        Value::Str(s) => format!("{s:?}"),
        v => format!("{v:?}"),
    }
}
