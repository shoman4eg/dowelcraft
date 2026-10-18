//! Shared flow for CLI and window: open a model (Bazis `.b3d` or our `.dcraft`),
//! nest by material and build documents.

use std::collections::BTreeMap;
use std::path::Path;

use crate::design::{self, Part, Project};
use crate::nest::{self, Item, Layout, Params};
use crate::panel::{self, Panel, Side, material_name};
use crate::report::{self, Card, Label};

/// Opened model.
pub struct Job {
    /// Order name (from the file name).
    pub name: String,
    pub panels: Vec<Panel>,
    /// Bazis panels with an unparsed contour.
    pub skipped: Vec<String>,
    /// Hardware from the Bazis model: (name, quantity).
    pub hardware: Vec<(String, usize)>,
    /// Fastener holes of the Bazis model that landed in no panel.
    pub holes_lost: usize,
    /// Our own project, if a `.dcraft` is open: sheet sizes and coefficients come from it.
    pub project: Option<Project>,
    /// Bazis model blocks and each panel's block (in `panels` order) — for import into our project.
    pub blocks: Vec<panel::BlockInfo>,
    pub panel_block: Vec<Option<usize>>,
}

impl Job {
    pub fn open(path: &Path) -> Result<Job, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if crate::design::is_project_file(path) {
            let text = String::from_utf8(data).map_err(|_| format!("{}: не UTF-8", path.display()))?;
            Ok(Job::from_project(Project::load(&text)?))
        } else {
            let file = crate::b3d::parse(&data).map_err(|e| format!("{}: {e}", path.display()))?;
            let ex = panel::panels(&file);
            Ok(Job {
                name,
                panels: ex.panels,
                skipped: ex.skipped,
                hardware: panel::hardware(&file),
                holes_lost: ex.holes_lost,
                project: None,
                blocks: ex.blocks,
                panel_block: ex.panel_block,
            })
        }
    }

    pub fn from_project(p: Project) -> Job {
        let panels = design::build(&p).map(|parts| parts.into_iter().map(|x| x.panel).collect()).unwrap_or_default();
        let hardware = design::hardware(&p).unwrap_or_default();
        Job { name: p.name.clone(), panels, skipped: Vec::new(), hardware, holes_lost: 0, project: Some(p), blocks: Vec::new(), panel_block: Vec::new() }
    }

    /// Bazis model as our project: panels with exact placement, Bazis blocks as blocks.
    pub fn import(&self) -> Project {
        design::import(&self.name, &self.panels, &self.blocks, &self.panel_block)
    }

    fn material(&self, full: &str) -> Option<&design::Material> {
        let name = material_name(full);
        self.project.as_ref()?.materials.iter().find(|m| m.name == name)
    }

    /// Sheet size: from the project material, otherwise `default`.
    pub fn sheet(&self, full: &str, default: (f64, f64)) -> (f64, f64) {
        self.material(full).map_or(default, |m| m.sheet)
    }

    /// Material nesting parameters: size, trim and kerf from the project, the rest from `default`.
    pub fn params(&self, full: &str, default: Params) -> Params {
        match self.material(full) {
            Some(m) => Params { sheet: m.sheet, kerf: m.kerf.unwrap_or(default.kerf), trim: m.trim.unwrap_or(default.trim) },
            None => default,
        }
    }

    /// Usage coefficient: from the project material, otherwise `default`.
    pub fn coef(&self, full: &str, default: f64) -> f64 {
        self.material(full).map_or(default, |m| m.coef)
    }

    /// Cost estimate: panels and edge banding with coefficients (`default` if the project has none), then hardware in pieces.
    pub fn estimate(&self, default: f64) -> Vec<panel::MaterialRow> {
        let mut rows = panel::materials(&self.panels, |m| self.coef(m, default));
        rows.extend(self.hardware.iter().map(|(n, q)| panel::MaterialRow { material: n.clone(), unit: "шт", coef: 1.0, qty: *q as f64 }));
        rows
    }

    /// Part number: position from the model, or the sequence number if there is none.
    pub fn number(&self, i: usize) -> String {
        let pos = self.panels[i].pos.trim();
        if pos.is_empty() { (i + 1).to_string() } else { pos.to_string() }
    }
}

/// Parts placed in space — for the front view.
/// Our own project is rebuilt; for a Bazis model the panel bounding boxes are used.
pub fn parts(job: &Job) -> Result<Vec<Part>, String> {
    if let Some(p) = &job.project {
        return design::build(p);
    }
    Ok(job
        .panels
        .iter()
        .filter_map(|p| {
            let (lo, hi) = p.bounds()?;
            Some(Part { owner: 0, id: 0, kind: design::guess_kind(p), panel: p.clone(), origin: lo, size: std::array::from_fn(|i| hi[i] - lo[i]) })
        })
        .collect())
}

/// Nesting of one material.
pub struct MaterialCut {
    pub material: String,
    pub params: Params,
    pub layout: Layout,
}

/// Nesting separately per material and thickness. `params` — parameters for a material.
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

/// Cutting maps for all materials (SVG pages).
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

/// Document for printing: maps and “Panels for production”.
pub fn document(job: &Job, cuts: &[MaterialCut]) -> String {
    let rows = panel::spec(&job.panels);
    let prod = report::production_html(&job.name, &rows, |r| {
        job.panels.iter().position(|p| p == &r.panel).map(|i| job.number(i)).unwrap_or_default()
    });
    report::document_html(&job.name, &cards(job, cuts), &prod)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn material_cut_params_override_defaults() {
        let mut s = design::Session::new(Project::new("т"));
        s.execute("material.add", &json!({"name": "Столешница", "thickness": 38, "sheet": [3050, 600], "trim": 0})).unwrap();
        s.execute("material.add", &json!({"name": "ЛДСП", "thickness": 16})).unwrap();
        s.execute("countertop.add", &json!({"material": "Столешница", "length": 1400, "depth": 600})).unwrap();
        let job = Job::from_project(s.project);
        let d = Params { sheet: (2800.0, 2070.0), kerf: 4.0, trim: 10.0 };
        assert_eq!(job.params("Столешница", d), Params { sheet: (3050.0, 600.0), kerf: 4.0, trim: 0.0 });
        assert_eq!(job.params("ЛДСП", d), d);
        // A countertop spanning the full sheet width fits, since there is no trim.
        let cuts = cut(&job, |m| job.params(m, d));
        assert!(cuts[0].layout.too_big.is_empty());
    }
}
