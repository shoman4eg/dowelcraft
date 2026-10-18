//! Library of hardware models (`.fr3d`, also `.b3d`): name, kind, preview and the holes
//! the model drills in the panels it is attached to. The user fills the folder.

use std::path::{Path, PathBuf};

use crate::panel::{self, Tri, WorldHole};

#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    pub path: PathBuf,
    /// `Article/Name` from the file, otherwise the file name.
    pub name: String,
    /// `Article/FurnType` (e.g. drawer, hinge), if present.
    pub kind: String,
    /// PNG preview from the header.
    pub thumbnail: Option<Vec<u8>>,
    /// Holes in model coordinates.
    pub holes: Vec<WorldHole>,
    /// Model bounding box (from slabs, mesh and holes), mm.
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    /// Model mesh (baskets, runners, hinges) for 3D, mm.
    pub mesh: Vec<Tri>,
}

pub fn read(path: &Path) -> Result<ModelInfo, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let file = crate::b3d::parse(&data).map_err(|e| format!("{}: {e}", path.display()))?;
    let article = file.section("Header").and_then(|h| h.get("Article"));
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = article.and_then(|a| a.str("Name")).filter(|n| !n.trim().is_empty()).map_or(stem, str::to_string);
    let kind = article.and_then(|a| a.str("FurnType")).unwrap_or_default().to_string();
    let thumbnail = file.section("Header").and_then(|h| h.blob("Thumbnail")).map(<[u8]>::to_vec);
    let holes = panel::world_holes(&file);
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    let mut add = |p: [f64; 3]| (0..3).for_each(|k| { lo[k] = lo[k].min(p[k]); hi[k] = hi[k].max(p[k]); });
    for p in &panel::panels(&file).panels {
        if let Some((a, b)) = p.bounds() {
            add(a);
            add(b);
        }
    }
    let mesh = panel::world_mesh(&file);
    for p in mesh.iter().flatten() {
        add(p.map(f64::from));
    }
    for h in &holes {
        add(h.pos);
        add(std::array::from_fn(|k| h.pos[k] + h.dir[k] * h.depth));
    }
    if lo[0] > hi[0] {
        (lo, hi) = ([0.0; 3], [0.0; 3]);
    }
    Ok(ModelInfo { path: path.to_path_buf(), name, kind, thumbnail, holes, lo, hi, mesh })
}

/// Texture by material article number in the library (as in Bazis: `d(<article>)1.jpg`, e.g. Baltlaminat).
pub fn find_texture(dir: &Path, article: &str) -> Option<PathBuf> {
    let article = article.trim().to_lowercase();
    if article.is_empty() {
        return None;
    }
    let wanted = [format!("d({article})1"), format!("d({article})"), article.clone()];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            if matches!(ext.as_str(), "jpg" | "jpeg" | "png") && wanted.contains(&stem) {
                return Some(p);
            }
        }
    }
    None
}

/// Models in the folder and its subfolders (`.fr3d`); unreadable files are skipped with a reason.
pub fn scan(dir: &Path) -> (Vec<ModelInfo>, Vec<String>) {
    let (mut ok, mut bad) = (Vec::new(), Vec::new());
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fr3d")) {
                match read(&p) {
                    Ok(m) => ok.push(m),
                    Err(e) => bad.push(e),
                }
            }
        }
    }
    ok.sort_by(|a: &ModelInfo, b| a.name.cmp(&b.name));
    (ok, bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fr3d_from_examples() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/telegram");
        if !dir.exists() {
            return;
        }
        let (models, bad) = scan(&dir);
        assert!(bad.is_empty(), "{bad:?}");
        let tandem = models.iter().find(|m| m.name.contains("TANDEM")).expect("Tandem in the examples");
        assert_eq!(tandem.kind, crate::i18n::bazis("library.drawer"));
        assert!(tandem.thumbnail.as_ref().is_some_and(|t| t.starts_with(b"\x89PNG")));
        assert!(!tandem.holes.is_empty());
    }

    #[test]
    fn baltlaminat_models_have_mesh_and_volume() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example");
        // The Baltlaminat "Anthracite" basket 600 by its article; the folder name is Russian.
        let is_basket = |p: &Path| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("WG-KOSZ60-60 "));
        let has_basket = |d: &Path| std::fs::read_dir(d).into_iter().flatten().flatten().any(|e| is_basket(&e.path()));
        if !std::fs::read_dir(&dir).into_iter().flatten().flatten().any(|e| e.path().is_dir() && has_basket(&e.path())) {
            return;
        }
        let (models, bad) = scan(&dir);
        assert!(bad.is_empty(), "{bad:?}");
        // The mesh is parsed for every model with TriData, without gaps.
        for path in models.iter().map(|m| &m.path) {
            let file = crate::b3d::parse(&std::fs::read(path).unwrap()).unwrap();
            let defs = file.section("Document").and_then(|d| d.get("FurnList")).map(|l| l.children.clone()).unwrap_or_default();
            for d in defs.iter().filter_map(|d| d.blob("TriData")) {
                panel::tri_data(d).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            }
        }
        let basket = models.iter().find(|m| is_basket(&m.path)).expect("basket 600");
        assert!(basket.mesh.len() > 1000, "{}", basket.mesh.len());
        // Volumetric: the bounding box used to be computed from holes and the height was 0.
        assert!((0..3).all(|k| basket.hi[k] - basket.lo[k] > 100.0), "{:?} {:?}", basket.lo, basket.hi);
        let flat = models.iter().filter(|m| (0..3).any(|k| m.hi[k] - m.lo[k] < 1.0)).map(|m| m.name.as_str()).collect::<Vec<_>>();
        println!("flat: {flat:?}");
    }

    /// Baltlaminat fragments (locks, magnets, lighting, sockets): all are read, each has a mesh and volume.
    #[test]
    fn baltlaminat_fragments() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/fragments");
        if !dir.exists() {
            return;
        }
        let t = std::time::Instant::now();
        let (models, bad) = scan(&dir);
        println!("models {}, errors {}, {:?}", models.len(), bad.len(), t.elapsed());
        assert!(bad.is_empty(), "{bad:?}");
        let flat: Vec<&str> = models.iter().filter(|m| (0..3).any(|k| m.hi[k] - m.lo[k] < 1.0)).map(|m| m.name.as_str()).collect();
        let no_mesh: Vec<&str> = models.iter().filter(|m| m.mesh.is_empty()).map(|m| m.name.as_str()).collect();
        let holes: usize = models.iter().map(|m| m.holes.len()).sum();
        println!("flat {}: {flat:?}\nwithout mesh {}: {no_mesh:?}\nholes in total {holes}", flat.len(), no_mesh.len());
        assert!(flat.is_empty() && no_mesh.is_empty());
    }
}

