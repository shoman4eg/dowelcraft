//! Bazis-Mebelshchik model (`.b3d`) from our project: every top-level object (cabinet, corner
//! module, countertop, block, panel) becomes a Bazis block (Type 1005) with slabs (4002) and holes (3000).
//! Slab placement is exact (axes → quaternion `Trans`). The header is as in Bazis 23.15 (older-version files
//! open in newer ones). Verified by reading back with our own parser (tests); in real Bazis —
//! by the user: we do not write Bazis's 64-byte tail (probably a signature).

use crate::b3d::{self, File, Node, Value};
use crate::contour::Seg;
use crate::design::{self, Part, Project};
use crate::geom::Frame;
use crate::panel::Texture;

fn leaf(name: &str, value: Value) -> Node {
    Node { name: name.into(), value, children: Vec::new() }
}

fn obj(name: &str, children: Vec<Node>) -> Node {
    Node { name: name.into(), value: Value::Obj, children }
}

fn s(name: &str, v: &str) -> Node {
    leaf(name, Value::Str(v.into()))
}

fn f(name: &str, v: f64) -> Node {
    leaf(name, Value::F64(v))
}

/// Rotation quaternion with axes `x`, `y`, `z` (matrix columns): Rx, Ry, Rz, Rw.
pub fn quaternion(fr: &Frame) -> [f64; 4] {
    let m = |i: usize, j: usize| [fr.x, fr.y, fr.z][j][i];
    let trace = m(0, 0) + m(1, 1) + m(2, 2);
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [(m(2, 1) - m(1, 2)) / s, (m(0, 2) - m(2, 0)) / s, (m(1, 0) - m(0, 1)) / s, 0.25 * s]
    } else if m(0, 0) > m(1, 1) && m(0, 0) > m(2, 2) {
        let s = (1.0 + m(0, 0) - m(1, 1) - m(2, 2)).sqrt() * 2.0;
        [0.25 * s, (m(0, 1) + m(1, 0)) / s, (m(0, 2) + m(2, 0)) / s, (m(2, 1) - m(1, 2)) / s]
    } else if m(1, 1) > m(2, 2) {
        let s = (1.0 + m(1, 1) - m(0, 0) - m(2, 2)).sqrt() * 2.0;
        [(m(0, 1) + m(1, 0)) / s, 0.25 * s, (m(1, 2) + m(2, 1)) / s, (m(0, 2) - m(2, 0)) / s]
    } else {
        let s = (1.0 + m(2, 2) - m(0, 0) - m(1, 1)).sqrt() * 2.0;
        [(m(0, 2) + m(2, 0)) / s, (m(1, 2) + m(2, 1)) / s, 0.25 * s, (m(1, 0) - m(0, 1)) / s]
    };
    // The sign does not matter (q and −q are the same rotation); Bazis writes Rw ≥ 0.
    if q[3] < 0.0 { q.map(|v| -v) } else { q }
}

fn trans(fr: &Frame) -> Node {
    let q = quaternion(fr);
    obj("Trans", vec![f("X", fr.origin[0]), f("Y", fr.origin[1]), f("Z", fr.origin[2]), f("Rx", q[0]), f("Ry", q[1]), f("Rz", q[2]), f("Rw", q[3])])
}

/// Contour as a Bazis blob: u32 n, then segments.
pub fn contour_blob(segs: &[Seg]) -> Vec<u8> {
    let mut out = (segs.len() as u32).to_le_bytes().to_vec();
    let mut kinds = Vec::new();
    for sg in segs {
        match *sg {
            Seg::Line { a, b } => kinds.push((0x10u8, vec![a[0], a[1], b[0], b[1]], None)),
            Seg::Arc { a, b, c, ccw } => kinds.push((0x12, vec![c[0], c[1], a[0], a[1], b[0], b[1]], Some(ccw as u8))),
            Seg::Circle { c, r } => kinds.push((0x11, vec![c[0], c[1], r], None)),
        }
    }
    for (k, vals, tail) in kinds {
        out.push(k);
        vals.iter().for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
        out.extend(tail);
    }
    out
}

fn panel_node(p: &Part, id: i32) -> Node {
    let pl = &p.panel;
    let fr = pl.frame.unwrap_or(Frame::IDENTITY);
    let mut c = vec![
        leaf("Type", Value::I32(4002)),
        s("Name", &pl.name),
        s("ArtPos", &pl.pos),
        s("Des", &pl.designation),
        leaf("ID", Value::I32(id)),
        trans(&fr),
        obj("Params", vec![obj("ObjectNotes", Vec::new())]),
        s("Mat", &pl.material),
        f("Thick", pl.thickness),
    ];
    match pl.texture {
        Texture::Horizontal => c.push(leaf("TexDir", Value::U8(1))),
        Texture::Vertical => c.push(leaf("TexDir", Value::U8(2))),
        Texture::Default => {}
    }
    c.push(leaf("Contour", Value::Blob(contour_blob(&pl.segs))));
    let butts = pl
        .edges
        .iter()
        .map(|e| {
            let mut b = vec![s("Mat", &e.material), f("Thick", e.thickness)];
            if e.width > 0.0 {
                b.push(f("Width", e.width));
            }
            b.extend([
                leaf("Elem", if e.segment <= 255 { Value::U8(e.segment as u8) } else { Value::I32(e.segment as i32) }),
                s("Sign", crate::panel::material_name(&e.material)),
                f("Overhung", 0.0),
                f("Allowance", e.allowance),
                leaf("Clip", if e.clip { Value::True } else { Value::False }),
            ]);
            obj("Butt", b)
        })
        .collect();
    c.push(obj("Butts", butts));
    // Grooves as Bazis keeps them (`Cuts/Cut`): a straight trajectory and a rectangular profile, across × through the
    // thickness from the back face — the way `panel::parse` reads them back.
    if !pl.grooves.is_empty() {
        let cuts = pl
            .grooves
            .iter()
            .map(|g| {
                let ([a0, a1], [z0, z1]) = (g.across, g.z);
                let corners = [[a0, z0], [a1, z0], [a1, z1], [a0, z1]];
                let profile: Vec<Seg> = (0..4).map(|i| Seg::Line { a: corners[i], b: corners[(i + 1) % 4] }).collect();
                obj("Cut", vec![
                    s("Name", &g.name),
                    s("Sign", &format!("{}×{}", a1 - a0, z1 - z0)),
                    leaf("Elem", Value::I32(-1)),
                    f("Thickness", 0.0),
                    leaf("Contour", Value::Blob(contour_blob(&profile))),
                    leaf("Trajectory", Value::Blob(contour_blob(&[Seg::Line { a: g.path[0], b: g.path[1] }]))),
                    leaf("Front", Value::False),
                ])
            })
            .collect();
        c.push(obj("Cuts", cuts));
    }
    obj("Obj", c)
}

/// Hole (Type 3000) in project coordinates: start, direction, radius, depth.
fn hole_node(p: &Part, h: &crate::panel::Hole, id: i32) -> Node {
    let fr = p.panel.frame.unwrap_or(Frame::IDENTITY);
    let (o, d) = (fr.apply(h.pos), fr.dir(h.dir));
    obj("Obj", vec![
        leaf("Type", Value::I32(3000)),
        leaf("ID", Value::I32(id)),
        f("X", o[0]), f("Y", o[1]), f("Z", o[2]),
        f("DirX", d[0]), f("DirY", d[1]), f("DirZ", d[2]),
        f("Radius", h.diameter / 2.0),
        f("Depth", h.depth),
    ])
}

/// Top-level object name for the Bazis block.
fn owner_name(pr: &Project, owner: u32) -> String {
    pr.cabinets.iter().find(|c| c.id == owner).map(|c| c.name.clone())
        .or_else(|| pr.corners.iter().find(|c| c.id == owner).map(|c| c.name.clone()))
        .or_else(|| pr.countertops.iter().find(|c| c.id == owner).map(|c| c.name.clone()))
        .or_else(|| pr.objects.iter().find(|o| o.id() == owner).map(|o| match o {
            design::Object::Block(b) => b.name.clone(),
            design::Object::Panel(p) => p.name.clone(),
            design::Object::Furniture(f) => f.name.clone(),
        }))
        .unwrap_or_else(|| crate::i18n::t(format!("Block {owner}")))
}

/// White 256×256 PNG image — the preview in the file header.
/// shortcut: the preview has no model image; Bazis redraws it on save.
fn blank_png() -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc(&body).to_be_bytes());
    }
    use std::io::Write;
    let (w, h) = (256u32, 256u32);
    let mut raw = Vec::with_capacity((w * 3 + 1) as usize * h as usize);
    for _ in 0..h {
        raw.push(0);
        raw.extend(std::iter::repeat_n(255u8, (w * 3) as usize));
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&raw).expect("writing to memory");
    let idat = z.finish().expect("writing to memory");
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = w.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &idat);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Bazis file tree for a project.
pub fn file(pr: &Project) -> Result<File, String> {
    let parts = design::build(pr)?;
    let mut owners: Vec<u32> = Vec::new();
    for p in &parts {
        if !owners.contains(&p.owner) {
            owners.push(p.owner);
        }
    }
    let mut id = 1;
    let mut next = || {
        id += 1;
        id
    };
    let model: Vec<Node> = owners
        .iter()
        .map(|&owner| {
            let block_id = next();
            let mut objs = Vec::new();
            for p in parts.iter().filter(|p| p.owner == owner) {
                objs.push(panel_node(p, next()));
                for h in &p.panel.holes {
                    objs.push(hole_node(p, h, next()));
                }
            }
            obj("Obj", vec![
                leaf("Type", Value::I32(1005)),
                s("Name", &owner_name(pr, owner)),
                leaf("ID", Value::I32(block_id)),
                trans(&Frame::IDENTITY),
                obj("Params", vec![obj("ObjectNotes", Vec::new())]),
                f("JointLength", 0.0),
                obj("Objs", objs),
            ])
        })
        .collect();
    let header = obj("Header", vec![
        leaf("Version", Value::U8(15)),
        leaf("Thumbnail", Value::Blob(blank_png())),
        obj("Article", vec![
            s("Name", &pr.name),
            leaf("ID", Value::U8(5)),
            obj("Params", Vec::new()),
            f("LDistance", 0.0),
            f("LDistance.Min", 0.0),
            f("LDistance.Max", 0.0),
            leaf("Furniture", Value::True),
            leaf("FileType", Value::U8(1)),
            obj("ExtraInfo", Vec::new()),
            obj("RotationList", Vec::new()),
        ]),
    ]);
    let document = obj("Document", vec![
        leaf("VersionMajor", Value::U8(23)),
        leaf("VersionMinor", Value::U8(15)),
        obj("FurnList", Vec::new()),
        obj("Model", model),
        obj("ModelParams", vec![obj("Params", vec![obj("ObjectNotes", Vec::new())])]),
        obj("Camera", vec![f("AngleX", 20.0), f("AngleY", -20.0)]),
        s("AppVersion", "2024.11.14.0"),
    ]);
    Ok(File { sections: vec![header, document] })
}

/// `.b3d` file for a project.
pub fn b3d(pr: &Project) -> Result<Vec<u8>, String> {
    Ok(b3d::write(&file(pr)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::Session;
    use serde_json::json;

    #[test]
    fn quaternion_of_basis_horizontal_panel() {
        // Rotation −90° about X, like a horizontal slab in a Bazis cabinet: Rx = −0.7071, Rw = 0.7071.
        let fr = Frame::new([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]);
        let q = quaternion(&fr);
        assert!((q[0] + 0.5f64.sqrt()).abs() < 1e-12 && q[1] == 0.0 && q[2] == 0.0 && (q[3] - 0.5f64.sqrt()).abs() < 1e-12, "{q:?}");
    }

    /// Our project → .b3d → parse: same slabs, sizes, edge banding, placement and drilling.
    #[test]
    fn exported_model_reads_back() {
        let mut s = Session::new(Project::new("Export"));
        s.execute("material.add", &json!({"name": "Chipboard 16", "article": "W980", "thickness": 16})).unwrap();
        s.execute("edge.add", &json!({"name": "PVC 2", "thickness": 2, "width": 19})).unwrap();
        s.execute("material.add", &json!({"name": "HDF 3", "thickness": 3})).unwrap();
        s.execute("cabinet.add", &json!({"width": 800, "height": 2000, "depth": 600, "material": "Chipboard 16", "edge": "PVC 2", "plinth": 80, "doors": 2, "shelves": [{"section": 0, "height": 700}], "back": "HDF 3", "back_groove": {}})).unwrap();
        s.execute("corner.add", &json!({"width_x": 1000, "width_z": 900, "height": 820, "depth": 560, "material": "Chipboard 16", "position": [2000, 0, 0]})).unwrap();
        let id = s.execute("panel.add", &json!({"material": "Chipboard 16", "plane": "vertical", "size": [500, 700], "position": [4000, 0, 0]})).unwrap()["id"].clone();
        s.execute("panel.corner", &json!({"id": id, "at": [500, 700], "radius": 100})).unwrap();
        s.execute("object.rotate", &json!({"ids": [id], "axis": "y", "angle": 30, "center": [4000, 0, 0]})).unwrap();
        let parts = design::build(&s.project).unwrap();
        let data = b3d(&s.project).unwrap();
        let back = crate::b3d::parse(&data).unwrap();
        let ex = crate::panel::panels(&back);
        assert!(ex.skipped.is_empty());
        assert_eq!(ex.panels.len(), parts.len());
        for (a, b) in parts.iter().zip(&ex.panels) {
            assert_eq!((&a.panel.name, &a.panel.material, a.panel.thickness, &a.panel.edges, &a.panel.grooves), (&b.name, &b.material, b.thickness, &b.edges, &b.grooves));
            let (ca, cb) = (a.panel.cut_size(), b.cut_size());
            assert!((ca.0 - cb.0).abs() < 1e-9 && (ca.1 - cb.1).abs() < 1e-9, "{}: {ca:?} ≠ {cb:?}", a.panel.name);
            let (fa, fb) = (a.panel.frame.unwrap(), b.frame.unwrap());
            let close = |x: [f64; 3], y: [f64; 3]| (0..3).all(|k| (x[k] - y[k]).abs() < 1e-9);
            assert!(close(fa.origin, fb.origin) && close(fa.x, fb.x) && close(fa.y, fb.y) && close(fa.z, fb.z), "{}: {fa:?} ≠ {fb:?}", a.panel.name);
        }
        assert_eq!(ex.panels.iter().filter(|p| !p.grooves.is_empty()).count(), 4, "the back panel grooves survive");
        // Blocks: one per top-level object.
        assert_eq!(ex.blocks.len(), 3);
        assert_eq!(back.section("Header").and_then(|h| h.get("Article")).and_then(|a| a.str("Name")), Some("Export"));
    }
}
