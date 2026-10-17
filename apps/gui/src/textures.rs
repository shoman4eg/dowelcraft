//! Библиотека текстур (как «Фурнитура»): папки с картинками (.jpg, .png), дерево по папкам, поиск, превью.
//! Список файлов собирается в фоне, превью уменьшаются в фоновом потоке и только для видимых строк —
//! у Балтламината тысячи картинок по сотне килобайт.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use eframe::egui::{self, Color32, Vec2};

/// Папка дерева: подпапки и картинки.
#[derive(Default)]
struct Folder {
    dirs: BTreeMap<String, Folder>,
    files: Vec<PathBuf>,
}

impl Folder {
    fn count(&self) -> usize {
        self.files.len() + self.dirs.values().map(Folder::count).sum::<usize>()
    }
}

/// Для какого материала выбирают текстуру.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Форма «Новый материал».
    New,
    /// Существующий материал по имени.
    Material(String),
}

pub struct TextureLib {
    pub dirs: Vec<String>,
    files: Vec<PathBuf>,
    scanning: Option<Receiver<Vec<PathBuf>>>,
    search: String,
    thumbs: HashMap<PathBuf, egui::TextureHandle>,
    requested: HashSet<PathBuf>,
    jobs: Sender<PathBuf>,
    done: Receiver<(PathBuf, Option<egui::ColorImage>)>,
    /// Окно выбора открыто для этого материала.
    pub open_for: Option<Target>,
}

const THUMB: u32 = 72;

fn is_image(p: &Path) -> bool {
    p.extension().is_some_and(|e| ["jpg", "jpeg", "png"].iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn list(dirs: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = dirs.iter().map(PathBuf::from).collect();
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if is_image(&p) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Превью: картинка, уменьшенная до `THUMB` по большей стороне.
fn thumbnail(p: &Path) -> Option<egui::ColorImage> {
    let img = image::open(p).ok()?.thumbnail(THUMB, THUMB).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// Имя для списка: «d(1101-L)1.jpg» → «1101-L» (артикул Балтламината), иначе имя файла без расширения.
pub fn label(p: &Path) -> String {
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    stem.strip_prefix("d(").and_then(|s| s.rsplit_once(')')).map_or(stem.clone(), |(a, _)| a.to_string())
}

impl TextureLib {
    pub fn new(dirs: Vec<String>) -> Self {
        let (jobs, rx_jobs) = channel::<PathBuf>();
        let (tx_done, done) = channel();
        // Один фоновый поток уменьшает картинки по очереди.
        std::thread::spawn(move || {
            for p in rx_jobs {
                let img = thumbnail(&p);
                if tx_done.send((p, img)).is_err() {
                    break;
                }
            }
        });
        let mut lib = TextureLib { dirs, files: Vec::new(), scanning: None, search: String::new(), thumbs: HashMap::new(), requested: HashSet::new(), jobs, done, open_for: None };
        lib.rescan();
        lib
    }

    pub fn rescan(&mut self) {
        let dirs = self.dirs.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(list(&dirs));
        });
        self.scanning = Some(rx);
    }

    fn poll(&mut self, ctx: &egui::Context) -> bool {
        while let Ok((p, img)) = self.done.try_recv() {
            let img = img.unwrap_or_else(|| egui::ColorImage::new([1, 1], vec![Color32::from_gray(90)]));
            let tex = ctx.load_texture(p.display().to_string(), img, Default::default());
            self.thumbs.insert(p, tex);
        }
        if let Some(rx) = &self.scanning {
            match rx.try_recv() {
                Ok(files) => {
                    self.files = files;
                    self.scanning = None;
                }
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => self.scanning = None,
            }
        }
        !self.requested.iter().all(|p| self.thumbs.contains_key(p))
    }

    /// Превью или заглушка (и заявка на превью в фоне).
    fn thumb(&mut self, ui: &egui::Ui, p: &Path) -> Option<egui::TextureId> {
        if let Some(t) = self.thumbs.get(p) {
            return Some(t.id());
        }
        if self.requested.insert(p.to_path_buf()) {
            let _ = self.jobs.send(p.to_path_buf());
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
        None
    }

    fn row(&mut self, ui: &mut egui::Ui, p: &Path, current: Option<&str>) -> bool {
        let mut picked = false;
        ui.horizontal(|ui| {
            let size = Vec2::splat(48.0);
            match self.thumb(ui, p) {
                Some(id) => {
                    ui.add(egui::Image::new((id, size)));
                }
                None => {
                    let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    ui.painter().rect_filled(r, 2.0, Color32::from_gray(70));
                }
            }
            let chosen = current.is_some_and(|c| Path::new(c) == p);
            let name = label(p);
            picked = ui.selectable_label(chosen, name).on_hover_text(p.display().to_string()).clicked();
        });
        picked
    }

    /// Папка дерева: подпапки свёрнуты, картинки — строками с превью; возвращает выбранную.
    fn folder(&mut self, ui: &mut egui::Ui, id: &str, f: &Folder, current: Option<&str>) -> Option<PathBuf> {
        let mut picked = None;
        for (name, sub) in &f.dirs {
            let key = format!("{id}/{name}");
            egui::CollapsingHeader::new(format!("{name}  ({})", sub.count())).id_salt(&key).show(ui, |ui| {
                if let Some(p) = self.folder(ui, &key, sub, current) {
                    picked = Some(p);
                }
            });
        }
        for p in &f.files {
            if self.row(ui, p, current) {
                picked = Some(p.clone());
            }
        }
        picked
    }

    /// Окно выбора: возвращает (для какого материала, выбранный файл или `None` — «без текстуры»).
    pub fn window(&mut self, ctx: &egui::Context, current: Option<&str>) -> Option<(Target, Option<PathBuf>)> {
        let target = self.open_for.clone()?;
        let busy = self.poll(ctx);
        let mut open = true;
        let mut result = None;
        let mut dirs_changed = false;
        let title = match &target {
            Target::New => crate::i18n::t("Текстура нового материала"),
            Target::Material(m) => crate::i18n::t(format!("Текстура материала «{m}»")),
        };
        egui::Window::new(title).id(egui::Id::new("texture_picker")).open(&mut open).default_size([460.0, 560.0]).show(ctx, |ui| {
            egui::CollapsingHeader::new(crate::i18n::t(format!("Папки текстур ({})", self.dirs.len()))).default_open(self.dirs.is_empty()).show(ui, |ui| {
                let mut remove = None;
                for (i, d) in self.dirs.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("✕").on_hover_text(crate::i18n::t("Убрать папку из библиотеки (файлы не удаляются)")).clicked() {
                            remove = Some(i);
                        }
                        ui.add(egui::Label::new(d.as_str()).truncate()).on_hover_text(d.as_str());
                    });
                }
                if let Some(i) = remove {
                    self.dirs.remove(i);
                    dirs_changed = true;
                }
                if ui.button(crate::i18n::t("+ Добавить папку…")).clicked()
                    && let Some(d) = rfd::FileDialog::new().pick_folder()
                {
                    let d = d.display().to_string();
                    if !self.dirs.contains(&d) {
                        self.dirs.push(d);
                        dirs_changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("Поиск"));
                ui.text_edit_singleline(&mut self.search);
                if ui.button(crate::i18n::t("Без текстуры")).clicked() {
                    result = Some((target.clone(), None));
                }
            });
            ui.separator();
            if self.scanning.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(crate::i18n::t("Читаю папки текстур…"));
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            let q = self.search.trim().to_lowercase();
            let files = self.files.clone();
            egui::ScrollArea::vertical().id_salt("tex_list").auto_shrink([false, false]).show(ui, |ui| {
                if !q.is_empty() {
                    let all: Vec<&PathBuf> = files.iter().filter(|p| label(p).to_lowercase().contains(&q) || p.to_string_lossy().to_lowercase().contains(&q)).collect();
                    let found = &all[..all.len().min(300)];
                    ui.weak(if all.len() > found.len() { crate::i18n::t(format!("Найдено: {}, показаны первые {}", all.len(), found.len())) } else { crate::i18n::t(format!("Найдено: {}", all.len())) });
                    for &p in found {
                        if self.row(ui, p, current) {
                            result = Some((target.clone(), Some(p.clone())));
                        }
                    }
                    return;
                }
                // Дерево папок, как лежат на диске (папка библиотеки → подпапки).
                let mut tree = Folder::default();
                for p in &files {
                    let dir = p.parent().unwrap_or(Path::new(""));
                    let root = self.dirs.iter().map(Path::new).filter(|r| dir.starts_with(r)).max_by_key(|r| r.components().count());
                    let mut f = &mut tree;
                    let top = root.map(|r| r.file_name().map_or_else(|| r.display().to_string(), |n| n.to_string_lossy().into_owned()));
                    for d in top.into_iter().chain(dir.strip_prefix(root.unwrap_or(Path::new(""))).unwrap_or(dir).components().map(|c| c.as_os_str().to_string_lossy().into_owned())) {
                        f = f.dirs.entry(d).or_default();
                    }
                    f.files.push(p.clone());
                }
                if let Some(p) = self.folder(ui, "tex", &tree, current) {
                    result = Some((target.clone(), Some(p)));
                }
            });
        });
        if dirs_changed {
            self.rescan();
        }
        if !open || result.is_some() {
            self.open_for = None;
        }
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
        result
    }

    pub fn set_search(&mut self, q: &str) {
        self.search = q.to_string();
    }

    /// Папки поменялись (для сохранения в настройках).
    pub fn dirs(&self) -> &[String] {
        &self.dirs
    }
}
