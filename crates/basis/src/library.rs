//! Библиотека моделей фурнитуры (`.fr3d`, а также `.b3d`): название, вид, превью и отверстия,
//! которые модель сверлит в панелях, к которым её ставят. Папку пополняет пользователь.

use std::path::{Path, PathBuf};

use crate::panel::{self, Tri, WorldHole};

#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    pub path: PathBuf,
    /// `Article/Name` из файла, иначе имя файла.
    pub name: String,
    /// `Article/FurnType` («Ящик», «Петля»…), если есть.
    pub kind: String,
    /// Превью PNG из заголовка.
    pub thumbnail: Option<Vec<u8>>,
    /// Отверстия в координатах модели.
    pub holes: Vec<WorldHole>,
    /// Габарит модели (по плитам, сетке и отверстиям), мм.
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    /// Сетка модели (корзины, направляющие, петли) для 3D, мм.
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

/// Текстура по артикулу материала в библиотеке (как у Базиса: `d(<артикул>)1.jpg`, напр. Балтламинат).
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

/// Модели в папке и её подпапках (`.fr3d`); нечитаемые файлы пропускаются с причиной.
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
        let tandem = models.iter().find(|m| m.name.contains("TANDEM")).expect("Tandem в примерах");
        assert_eq!(tandem.kind, "Ящик");
        assert!(tandem.thumbnail.as_ref().is_some_and(|t| t.starts_with(b"\x89PNG")));
        assert!(!tandem.holes.is_empty());
    }

    #[test]
    fn baltlaminat_models_have_mesh_and_volume() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example");
        if !dir.join("Антрацит").exists() {
            return;
        }
        let (models, bad) = scan(&dir);
        assert!(bad.is_empty(), "{bad:?}");
        // Сетка разобрана у каждой модели с TriData и без обрывов.
        for path in models.iter().map(|m| &m.path) {
            let file = crate::b3d::parse(&std::fs::read(path).unwrap()).unwrap();
            let defs = file.section("Document").and_then(|d| d.get("FurnList")).map(|l| l.children.clone()).unwrap_or_default();
            for d in defs.iter().filter_map(|d| d.blob("TriData")) {
                panel::tri_data(d).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            }
        }
        let basket = models.iter().find(|m| m.name == "GTV Ажурная корзина ELITE, Антрацит, 600").expect("корзина 600");
        assert!(basket.mesh.len() > 1000, "{}", basket.mesh.len());
        // Объёмная: раньше габарит считался по отверстиям и высота была 0.
        assert!((0..3).all(|k| basket.hi[k] - basket.lo[k] > 100.0), "{:?} {:?}", basket.lo, basket.hi);
        let flat = models.iter().filter(|m| (0..3).any(|k| m.hi[k] - m.lo[k] < 1.0)).map(|m| m.name.as_str()).collect::<Vec<_>>();
        println!("плоских: {flat:?}");
    }

    /// Фрагменты Балтламината (замки, магниты, освещение, розетки): читаются все, у каждой сетка и объём.
    #[test]
    fn baltlaminat_fragments() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/fragments");
        if !dir.exists() {
            return;
        }
        let t = std::time::Instant::now();
        let (models, bad) = scan(&dir);
        println!("моделей {}, ошибок {}, {:?}", models.len(), bad.len(), t.elapsed());
        assert!(bad.is_empty(), "{bad:?}");
        let flat: Vec<&str> = models.iter().filter(|m| (0..3).any(|k| m.hi[k] - m.lo[k] < 1.0)).map(|m| m.name.as_str()).collect();
        let no_mesh: Vec<&str> = models.iter().filter(|m| m.mesh.is_empty()).map(|m| m.name.as_str()).collect();
        let holes: usize = models.iter().map(|m| m.holes.len()).sum();
        println!("плоских {}: {flat:?}\nбез сетки {}: {no_mesh:?}\nотверстий всего {holes}", flat.len(), no_mesh.len());
        assert!(flat.is_empty() && no_mesh.is_empty());
    }
}

