//! Общий сценарий для CLI и окна: открыть модель (`.b3d` Базиса или свой `.bcraft`),
//! раскроить по материалам и собрать документы.

use std::collections::BTreeMap;
use std::path::Path;

use crate::design::{self, Project};
use crate::nest::{self, Item, Layout, Params};
use crate::panel::{self, Panel, Side, material_name};
use crate::report::{self, Card, Label};

/// Открытая модель.
pub struct Job {
    /// Имя заказа (по имени файла).
    pub name: String,
    pub panels: Vec<Panel>,
    /// Плиты Базиса с неразобранным контуром.
    pub skipped: Vec<String>,
    /// Свой проект, если открыт `.bcraft`: из него берутся форматы листов и коэффициенты.
    pub project: Option<Project>,
}

impl Job {
    pub fn open(path: &Path) -> Result<Job, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bcraft")) {
            let text = String::from_utf8(data).map_err(|_| format!("{}: не UTF-8", path.display()))?;
            Ok(Job::from_project(Project::load(&text)?))
        } else {
            let file = crate::b3d::parse(&data).map_err(|e| format!("{}: {e}", path.display()))?;
            let ex = panel::panels(&file);
            Ok(Job { name, panels: ex.panels, skipped: ex.skipped, project: None })
        }
    }

    pub fn from_project(p: Project) -> Job {
        let panels = design::build(&p).map(|parts| parts.into_iter().map(|x| x.panel).collect()).unwrap_or_default();
        Job { name: p.name.clone(), panels, skipped: Vec::new(), project: Some(p) }
    }

    fn material(&self, full: &str) -> Option<&design::Material> {
        let name = material_name(full);
        self.project.as_ref()?.materials.iter().find(|m| m.name == name)
    }

    /// Формат листа: из материала проекта, иначе `default`.
    pub fn sheet(&self, full: &str, default: (f64, f64)) -> (f64, f64) {
        self.material(full).map_or(default, |m| m.sheet)
    }

    /// Коэффициент расхода: из материала проекта, иначе `default`.
    pub fn coef(&self, full: &str, default: f64) -> f64 {
        self.material(full).map_or(default, |m| m.coef)
    }

    /// Номер детали: позиция из модели, а если её нет — порядковый номер.
    pub fn number(&self, i: usize) -> String {
        let pos = self.panels[i].pos.trim();
        if pos.is_empty() { (i + 1).to_string() } else { pos.to_string() }
    }
}

/// Раскрой одного материала.
pub struct MaterialCut {
    pub material: String,
    pub params: Params,
    pub layout: Layout,
}

/// Раскрой отдельно по каждому материалу и толщине. `params` — параметры для материала.
pub fn cut(job: &Job, params: impl Fn(&str) -> Params) -> Vec<MaterialCut> {
    let mut groups: BTreeMap<(String, i64), Vec<usize>> = BTreeMap::new();
    for (i, p) in job.panels.iter().enumerate() {
        groups.entry((p.material.clone(), (p.thickness * 100.0).round() as i64)).or_default().push(i);
    }
    groups
        .into_iter()
        .map(|((material, _), ids)| {
            let items: Vec<Item> = ids
                .iter()
                .map(|&i| {
                    let p = &job.panels[i];
                    let (length, width) = p.cut_size();
                    Item { id: i, length, width, grain: p.grain() }
                })
                .collect();
            let params = params(&material);
            let layout = nest::nest(&items, &params);
            MaterialCut { material, params, layout }
        })
        .collect()
}

pub fn label(job: &Job, id: usize) -> Label {
    let p = &job.panels[id];
    let has = |sd| p.edges.iter().any(|e| p.side(e) == sd && e.thickness > 0.0);
    Label { num: job.number(id), size: p.cut_size(), edges: [has(Side::L1), has(Side::L2), has(Side::W1), has(Side::W2)] }
}

/// Карты раскроя по всем материалам (SVG-страницы).
pub fn cards(job: &Job, cuts: &[MaterialCut]) -> Vec<String> {
    let mut out = Vec::new();
    for c in cuts {
        let title = material_name(&c.material);
        for (k, s) in c.layout.sheets.iter().enumerate() {
            let card = Card { order: &job.name, material: title, index: k + 1, total: c.layout.sheets.len() };
            out.push(report::card_svg(&card, s, &c.params, |id| label(job, id)));
        }
    }
    out
}

/// Документ для печати: карты и «Панели в производство».
pub fn document(job: &Job, cuts: &[MaterialCut]) -> String {
    let rows = panel::spec(&job.panels);
    let prod = report::production_html(&job.name, &rows, |r| {
        job.panels.iter().position(|p| p == &r.panel).map(|i| job.number(i)).unwrap_or_default()
    });
    report::document_html(&job.name, &cards(job, cuts), &prod)
}
