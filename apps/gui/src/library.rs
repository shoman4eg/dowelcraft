//! Вкладка «Фурнитура»: модели .fr3d деревом папок, как они лежат на диске (раздел → производитель → модель).
//! С поиском — плоский список найденного с путём папки.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use basis::library::ModelInfo;
use eframe::egui::{self, Color32, Vec2};

/// Превью моделей: путь → текстура egui.
pub type Thumbs = HashMap<PathBuf, egui::TextureHandle>;

/// Папка дерева: подпапки и номера моделей в ней.
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

/// Папки модели: имя папки библиотеки, в которой она лежит, и подпапки внутри неё.
fn folders(roots: &[PathBuf], m: &ModelInfo) -> Vec<String> {
    let dir = m.path.parent().unwrap_or(Path::new(""));
    let Some(root) = roots.iter().filter(|r| dir.starts_with(r)).max_by_key(|r| r.components().count()) else {
        return vec![dir.display().to_string()];
    };
    let name = root.file_name().map_or_else(|| root.display().to_string(), |n| n.to_string_lossy().into_owned());
    std::iter::once(name).chain(dir.strip_prefix(root).unwrap_or(dir).components().map(|c| c.as_os_str().to_string_lossy().into_owned())).collect()
}

/// Строка модели: превью, название, вид и размер, «Поставить». Возвращает true, если нажали «Поставить».
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
            ui.weak(format!("{} · {} мм · отверстий {}", if m.kind.is_empty() { "модель" } else { &m.kind }, size.join("×"), m.holes.len()));
            if let Some(p) = path {
                ui.weak(format!("папка: {p}"));
            }
            place = ui.add_enabled(editable, egui::Button::new("Поставить")).on_hover_text("Встанет справа от модулей; дальше — сдвиг в «Свойствах»").clicked();
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

/// Дерево (или найденное по `query`); возвращает файл, который надо поставить.
pub fn show(ui: &mut egui::Ui, roots: &[PathBuf], models: &[ModelInfo], thumbs: &mut Thumbs, editable: bool, query: &str) -> Option<PathBuf> {
    let q = query.trim().to_lowercase();
    let mut place = None;
    egui::ScrollArea::vertical().id_salt("lib").show(ui, |ui| {
        if !q.is_empty() {
            let found: Vec<&ModelInfo> = models.iter().filter(|m| m.name.to_lowercase().contains(&q) || m.kind.to_lowercase().contains(&q)).collect();
            ui.weak(format!("Найдено: {}", found.len()));
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
