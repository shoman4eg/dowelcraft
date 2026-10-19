//! Wavefront OBJ export for visualisation (Blender, 3ds Max, SketchUp Pro, …): every part is a named object — its
//! contour extruded by the thickness, as in the 3D view — hardware models come with their own meshes, and the
//! materials (colour, texture) go to a `.mtl` file next to it. Units — mm.

use std::fmt::Write;

use crate::design::{self, Kind, Part, Project};
use crate::panel::material_name;

/// The two files: the OBJ itself and its material library.
pub struct Obj {
    pub obj: String,
    pub mtl: String,
}

/// Writes `path` (.obj) and its materials next to it (same name, .mtl).
pub fn save(path: &std::path::Path, parts: &[Part], project: Option<&Project>, title: &str) -> Result<(), String> {
    let mtl_path = path.with_extension("mtl");
    let mtl_file = mtl_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "materials.mtl".into());
    let out = export(parts, project, title, &mtl_file);
    std::fs::write(path, out.obj).map_err(|e| format!("{}: {e}", path.display()))?;
    std::fs::write(&mtl_path, out.mtl).map_err(|e| format!("{}: {e}", mtl_path.display()))
}

/// Material library name in `mtllib`, e.g. "kitchen.mtl"; parts of a Bazis model have no project (no textures, no hardware).
pub fn export(parts: &[Part], project: Option<&Project>, title: &str, mtl_file: &str) -> Obj {
    let mesh = crate::mesh::build(parts, |_| false);
    let mut obj = format!("# dowel-craft — {title}\n# units: mm\nmtllib {mtl_file}\n");
    let mut mtl = String::from("# dowel-craft materials\n");
    // Triangles by part.
    let mut tris: Vec<Vec<[u32; 3]>> = vec![Vec::new(); parts.len()];
    for &t in mesh.indices.as_chunks::<3>().0 {
        tris[mesh.vertices[t[0] as usize].part as usize].push(t);
    }
    // Material (as parts carry it) → its unique one-word name in the .mtl.
    let mut materials: Vec<(String, String)> = Vec::new();
    // Vertices and normals written so far (OBJ indices are global, from 1).
    let (mut nv, mut nn) = (0usize, 0usize);
    for (pi, part) in parts.iter().enumerate().filter(|(i, _)| !tris[*i].is_empty()) {
        let found = materials.iter().find(|(m, _)| *m == part.panel.material).map(|(_, k)| k.clone());
        let key = found.clone().unwrap_or_else(|| unique(key(material_name(&part.panel.material)), &materials));
        if found.is_none() {
            let texture = project.and_then(|p| p.materials.iter().find(|m| design::full(&m.name, &m.article) == part.panel.material));
            let _ = write!(mtl, "\nnewmtl {key}\n");
            let [r, g, b] = colour(part.kind);
            let _ = writeln!(mtl, "Kd {r:.3} {g:.3} {b:.3}");
            // A full path: other programs open the .mtl from its own folder, not from ours.
            if let Some(path) = texture.and_then(|m| m.texture.as_deref()) {
                let full = std::fs::canonicalize(path).map_or_else(|_| path.to_string(), |p| p.display().to_string());
                let _ = writeln!(mtl, "map_Kd {full}");
            }
            materials.push((part.panel.material.clone(), key.clone()));
        }
        let tile = project
            .and_then(|p| p.materials.iter().find(|m| design::full(&m.name, &m.article) == part.panel.material))
            .and_then(|m| m.texture_size)
            .unwrap_or([1000.0, 1000.0]);
        let name = format!("{} {}", part.panel.pos, part.panel.name);
        let _ = write!(obj, "\no {}\nusemtl {key}\n", name.trim());
        // Only this part's vertices, renumbered.
        let mut used: Vec<u32> = tris[pi].iter().flatten().copied().collect();
        used.sort_unstable();
        used.dedup();
        for &i in &used {
            let v = mesh.vertices[i as usize];
            let _ = writeln!(obj, "v {} {} {}", v.pos[0], v.pos[1], v.pos[2]);
        }
        for &i in &used {
            let v = mesh.vertices[i as usize];
            let _ = writeln!(obj, "vt {} {}", f64::from(v.uv[0]) / tile[0], f64::from(v.uv[1]) / tile[1]);
        }
        for &i in &used {
            let n = mesh.vertices[i as usize].normal;
            let _ = writeln!(obj, "vn {} {} {}", n[0], n[1], n[2]);
        }
        for t in &tris[pi] {
            let [a, b, c] = t.map(|i| nv + used.binary_search(&i).unwrap_or(0) + 1);
            let _ = writeln!(obj, "f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}");
        }
        nv += used.len();
        nn += used.len();
    }
    // Hardware models with a mesh in their .fr3d file; those without one are left out.
    if let Some(p) = project {
        let hardware = unique("hardware".into(), &materials);
        let mut grey = false;
        for (f, frame) in design::furniture(p) {
            let Ok(model) = crate::library::read(std::path::Path::new(&f.source)) else { continue };
            if model.mesh.is_empty() {
                continue;
            }
            if !grey {
                let _ = write!(mtl, "\nnewmtl {hardware}\nKd 0.620 0.640 0.680\n");
                grey = true;
            }
            let _ = write!(obj, "\no {}\nusemtl {hardware}\n", f.name);
            for t in &model.mesh {
                let w = t.map(|q| frame.apply(q.map(f64::from)));
                let (a, b) = (std::array::from_fn::<f64, 3, _>(|k| w[1][k] - w[0][k]), std::array::from_fn::<f64, 3, _>(|k| w[2][k] - w[0][k]));
                let n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if l < 1e-12 {
                    continue;
                }
                for q in w {
                    let _ = writeln!(obj, "v {} {} {}", q[0], q[1], q[2]);
                }
                let _ = writeln!(obj, "vn {} {} {}", n[0] / l, n[1] / l, n[2] / l);
                nn += 1;
                let _ = writeln!(obj, "f {}//{nn} {}//{nn} {}//{nn}", nv + 1, nv + 2, nv + 3);
                nv += 3;
            }
        }
    }
    Obj { obj, mtl }
}

/// `k`, or `k_2`, `k_3`… if another material already has it ("A B" and "A_B" both become "A_B").
fn unique(k: String, taken: &[(String, String)]) -> String {
    let used = |c: &str| taken.iter().any(|(_, t)| t == c);
    if !used(&k) {
        return k;
    }
    (2..).map(|i| format!("{k}_{i}")).find(|c| !used(c)).unwrap_or(k)
}

/// Material name usable in OBJ/MTL (a single word).
fn key(name: &str) -> String {
    let k: String = name.chars().map(|c| if c.is_whitespace() { '_' } else { c }).collect();
    if k.is_empty() { "material".into() } else { k }
}

/// The 3D view's colours for a material without a texture.
fn colour(kind: Kind) -> [f32; 3] {
    match kind {
        Kind::Door | Kind::Drawer => [0.67, 0.78, 0.90],
        Kind::Back => [0.85, 0.85, 0.85],
        Kind::Body => [0.92, 0.86, 0.75],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::Session;
    use serde_json::json;

    /// Every part is a named object with its triangles; vertex indices stay in range; materials go to the .mtl.
    #[test]
    fn cabinet_exports_to_obj() {
        let mut s = Session::new(Project::new("Obj"));
        s.execute("material.add", &json!({"name": "Chipboard 16", "thickness": 16})).unwrap();
        s.execute("cabinet.add", &json!({"width": 600, "height": 720, "depth": 500, "material": "Chipboard 16", "doors": 1})).unwrap();
        let parts = design::build(&s.project).unwrap();
        let out = export(&parts, Some(&s.project), "Obj", "obj.mtl");
        assert_eq!(out.obj.lines().filter(|l| l.starts_with("o ")).count(), parts.len());
        assert!(out.obj.contains("mtllib obj.mtl") && out.obj.contains("usemtl Chipboard_16"));
        assert!(out.mtl.contains("newmtl Chipboard_16"));
        let vertices = out.obj.lines().filter(|l| l.starts_with("v ")).count();
        for f in out.obj.lines().filter(|l| l.starts_with("f ")) {
            for i in f.split_whitespace().skip(1).map(|c| c.split('/').next().unwrap().parse::<usize>().unwrap()) {
                assert!(i >= 1 && i <= vertices, "{f}");
            }
        }
        // Normals and texture coordinates: one per vertex for parts, every index in range.
        let normals = out.obj.lines().filter(|l| l.starts_with("vn ")).count();
        assert_eq!(normals, vertices);
        assert_eq!(unique("A_B".into(), &[("A B".into(), "A_B".into())]), "A_B_2");
        // A box part: 2 faces × 2 triangles + 4 edge faces × 2 triangles = 12.
        let side = out.obj.split("\no ").find(|o| o.contains("Left side")).unwrap();
        assert_eq!(side.lines().filter(|l| l.starts_with("f ")).count(), 12);
    }
}
