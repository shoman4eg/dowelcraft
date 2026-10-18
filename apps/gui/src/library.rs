//! "Fittings" tab: .fr3d models as a folder tree, as laid out on disk (section → manufacturer → model).
//! With a search — a flat list of matches with the folder path.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use dowel::library::ModelInfo;
use eframe::egui::{self, Color32, Vec2};

/// Model previews: path → egui texture.
pub type Thumbs = HashMap<PathBuf, egui::TextureHandle>;

/// Tree folder: subfolders and indices of models in it.
#[derive(Default)]
struct Folder {
    dirs: BTreeMap<String, Folder>,
    models: Vec<usize>,
}

impl Folder {
    fn count(&self) -> usize {
        self.models.len() + self.dirs.values().map(Folder::count).sum::<usize>()
    }
}

/// Model folders: name of the library folder it lives in, and the subfolders inside it.
fn folders(roots: &[PathBuf], m: &ModelInfo) -> Vec<String> {
    let dir = m.path.parent().unwrap_or(Path::new(""));
    let Some(root) = roots.iter().filter(|r| dir.starts_with(r)).max_by_key(|r| r.components().count()) else {
        return vec![dir.display().to_string()];
    };
    let name = root.file_name().map_or_else(|| root.display().to_string(), |n| n.to_string_lossy().into_owned());
    std::iter::once(name).chain(dir.strip_prefix(root).unwrap_or(dir).components().map(|c| c.as_os_str().to_string_lossy().into_owned())).collect()
}

/// Model row: preview, name, kind and size, "Place". Returns true if "Place" was clicked.
fn row(ui: &mut egui::Ui, m: &ModelInfo, thumbs: &mut Thumbs, editable: bool, path: Option<&str>) -> bool {
    let mut place = false;
    ui.horizontal(|ui| {
        let tex = thumbs.entry(m.path.clone()).or_insert_with(|| {
            let img = m.thumbnail.as_deref().and_then(crate::decode_png).unwrap_or_else(|| egui::ColorImage::new([1, 1], vec![Color32::from_gray(230)]));
            ui.ctx().load_texture(m.path.display().to_string(), img, Default::default())
        });
        ui.add(egui::Image::new((tex.id(), Vec2::splat(56.0))));
        ui.vertical(|ui| {
            ui.label(&m.name);
            let size: Vec<String> = (0..3).map(|k| format!("{:.0}", m.hi[k] - m.lo[k])).collect();
            ui.weak(crate::i18n::t(format!("{} · {} mm · holes {}", if m.kind.is_empty() { "model" } else { &m.kind }, size.join("×"), m.holes.len())));
            if let Some(p) = path {
                ui.weak(crate::i18n::t(format!("folder: {p}")));
            }
            place = ui.add_enabled(editable, egui::Button::new(crate::i18n::t("Place"))).on_hover_text(crate::i18n::t("Placed to the right of the modules; then move it in “Properties”")).clicked();
        });
    });
    place
}

fn show_folder(ui: &mut egui::Ui, id: &str, f: &Folder, models: &[ModelInfo], thumbs: &mut Thumbs, editable: bool, place: &mut Option<PathBuf>) {
    for (name, sub) in &f.dirs {
        let key = format!("{id}/{name}");
        egui::CollapsingHeader::new(format!("{name}  ({})", sub.count())).id_salt(&key).show(ui, |ui| {
            show_folder(ui, &key, sub, models, thumbs, editable, place);
        });
    }
    for &i in &f.models {
        if row(ui, &models[i], thumbs, editable, None) {
            *place = Some(models[i].path.clone());
        }
    }
}

/// Tree (or matches for `query`); returns the file to place.
pub fn show(ui: &mut egui::Ui, roots: &[PathBuf], models: &[ModelInfo], thumbs: &mut Thumbs, editable: bool, query: &str) -> Option<PathBuf> {
    let q = query.trim().to_lowercase();
    let mut place = None;
    egui::ScrollArea::vertical().id_salt("lib").show(ui, |ui| {
        if !q.is_empty() {
            let found: Vec<&ModelInfo> = models.iter().filter(|m| m.name.to_lowercase().contains(&q) || m.kind.to_lowercase().contains(&q)).collect();
            ui.weak(crate::i18n::t(format!("Found: {}", found.len())));
            for m in found {
                let path = folders(roots, m).join(" / ");
                if row(ui, m, thumbs, editable, Some(&path)) {
                    place = Some(m.path.clone());
                }
                ui.separator();
            }
            return;
        }
        let mut tree = Folder::default();
        for (i, m) in models.iter().enumerate() {
            let mut f = &mut tree;
            for d in folders(roots, m) {
                f = f.dirs.entry(d).or_default();
            }
            f.models.push(i);
        }
        show_folder(ui, "lib", &tree, models, thumbs, editable, &mut place);
    });
    place
}
