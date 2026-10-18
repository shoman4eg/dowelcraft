//! "Model structure": the tree as nested in the project (blocks inside blocks), cabinets grouped by part
//! role; an eye on every row and group — hide/show in 3D and in the front view; grouping the selection.

use std::collections::HashSet;

use dowel::design::{Kind, Object, Part, Project};
use eframe::egui;
use serde_json::json;

use crate::icons::{self, Icon};
use crate::{App, MenuAct, Sel};

enum Row {
    Part(usize),
    Furniture(u32, String),
    /// Group: title, key for the expanded state, block number (if it's a project block), contents.
    Group { title: String, key: String, block: Option<u32>, rows: Vec<Row> },
}

impl Row {
    /// Parts inside (for the group's eye and selection).
    fn parts(&self, out: &mut Vec<usize>) {
        match self {
            Row::Part(i) => out.push(*i),
            Row::Furniture(..) => {}
            Row::Group { rows, .. } => rows.iter().for_each(|r| r.parts(out)),
        }
    }
}

/// What was done in the tree this frame.
#[derive(Default)]
struct Acts {
    select: Option<Vec<usize>>,
    add: bool,
    menu_hit: Option<usize>,
    menu: Option<MenuAct>,
    toggle: Option<Vec<usize>>,
    furniture: Option<u32>,
    ungroup: Option<u32>,
    rename: Option<(u32, String)>,
}

fn kind_title(k: Kind) -> &'static str {
    match k {
        Kind::Body => "Frame",
        Kind::Back => "Back panels",
        Kind::Door => "Doors",
        Kind::Drawer => "Drawers",
    }
}

/// Template parts (cabinet, corner module, countertop) by role; a single group — without subgroups.
fn template_rows(parts: &[Part], owner: u32) -> Vec<Row> {
    let mine: Vec<usize> = (0..parts.len()).filter(|&i| parts[i].owner == owner && parts[i].id == 0).collect();
    let kinds: Vec<Kind> = [Kind::Body, Kind::Back, Kind::Door, Kind::Drawer].into_iter().filter(|k| mine.iter().any(|&i| parts[i].kind == *k)).collect();
    if kinds.len() <= 1 {
        return mine.into_iter().map(Row::Part).collect();
    }
    kinds
        .into_iter()
        .map(|k| Row::Group { title: kind_title(k).into(), key: format!("t{owner}{k:?}"), block: None, rows: mine.iter().filter(|&&i| parts[i].kind == k).map(|&i| Row::Part(i)).collect() })
        .collect()
}

/// Project object tree (blocks as nested groups).
fn object_rows(parts: &[Part], objects: &[Object]) -> Vec<Row> {
    objects
        .iter()
        .map(|o| match o {
            Object::Block(b) => Row::Group { title: format!("Block: {}", b.name), key: format!("b{}", b.id), block: Some(b.id), rows: object_rows(parts, &b.children) },
            Object::Panel(p) => {
                let mine: Vec<Row> = (0..parts.len()).filter(|&i| parts[i].id == p.id).map(Row::Part).collect();
                if mine.len() == 1 { mine.into_iter().next().unwrap_or(Row::Group { title: String::new(), key: String::new(), block: None, rows: Vec::new() }) } else { Row::Group { title: format!("Panel: {}", p.name), key: format!("p{}", p.id), block: None, rows: mine } }
            }
            Object::Furniture(f) => Row::Furniture(f.id, f.name.clone()),
        })
        .collect()
}

fn tree(parts: &[Part], project: Option<&Project>) -> Vec<Row> {
    let Some(pr) = project else {
        // A Bazis model without its own project: one group per top-level object.
        let mut owners: Vec<u32> = Vec::new();
        for p in parts {
            if !owners.contains(&p.owner) {
                owners.push(p.owner);
            }
        }
        return owners.into_iter().map(|o| Row::Group { title: "Bazis model".into(), key: format!("o{o}"), block: None, rows: (0..parts.len()).filter(|&i| parts[i].owner == o).map(Row::Part).collect() }).collect();
    };
    let mut rows = Vec::new();
    for c in &pr.cabinets {
        rows.push(Row::Group { title: format!("Cabinet: {}", c.name), key: format!("c{}", c.id), block: None, rows: template_rows(parts, c.id) });
    }
    for c in &pr.corners {
        rows.push(Row::Group { title: format!("Corner unit: {}", c.name), key: format!("c{}", c.id), block: None, rows: template_rows(parts, c.id) });
    }
    for c in &pr.countertops {
        rows.push(Row::Group { title: format!("Countertop: {}", c.name), key: format!("c{}", c.id), block: None, rows: template_rows(parts, c.id) });
    }
    rows.extend(object_rows(parts, &pr.objects));
    rows
}

impl App {
    fn eye(&self, ui: &mut egui::Ui, idx: &[usize], acts: &mut Acts) {
        let parts = self.parts.as_ref().map(Vec::as_slice).unwrap_or_default();
        let hidden = !idx.is_empty() && idx.iter().all(|&i| self.hidden.contains(&Sel::of(parts, i)));
        let tip = if hidden { "Show" } else { "Hide in 3D and the front view (stays in parts and cutting)" };
        if icons::button(ui, if hidden { Icon::EyeOff } else { Icon::Eye }, 18.0, tip, false).clicked() {
            acts.toggle = Some(idx.to_vec());
        }
    }

    fn rows(&self, ui: &mut egui::Ui, rows: &[Row], acts: &mut Acts) {
        let parts = self.parts.as_ref().map(Vec::as_slice).unwrap_or_default();
        let all = self.selected_all();
        for row in rows {
            match row {
                Row::Part(i) => {
                    let p = &parts[*i].panel;
                    ui.horizontal(|ui| {
                        self.eye(ui, &[*i], acts);
                        let r = ui.selectable_label(all.contains(i), format!("{} {}", p.pos, p.name).trim().to_string());
                        if r.clicked() {
                            acts.select = Some(vec![*i]);
                            acts.add = ui.input(|i| i.modifiers.command);
                        }
                        if r.secondary_clicked() {
                            acts.menu_hit = Some(*i);
                        }
                        r.context_menu(|ui| acts.menu = self.menu_items(ui).or(acts.menu));
                    });
                }
                Row::Furniture(id, name) => {
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        if ui.selectable_label(self.furniture_sel == Some(*id), crate::i18n::t(format!("Hardware: {name}"))).clicked() {
                            acts.furniture = Some(*id);
                        }
                    });
                }
                Row::Group { title, key, block, rows } => {
                    let mut idx = Vec::new();
                    row.parts(&mut idx);
                    let id = ui.make_persistent_id(("structure", key));
                    let open_sel = idx.iter().any(|i| all.contains(i));
                    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
                    if open_sel && all.len() == 1 {
                        state.set_open(true);
                    }
                    state
                        .show_header(ui, |ui| {
                            self.eye(ui, &idx, acts);
                            let chosen = !idx.is_empty() && idx.iter().all(|i| all.contains(i));
                            let r = ui.selectable_label(chosen, crate::i18n::t(format!("{title} ({})", idx.len())));
                            if r.clicked() {
                                acts.select = Some(idx.clone());
                                acts.add = ui.input(|i| i.modifiers.command);
                            }
                            if let Some(b) = block {
                                r.context_menu(|ui| {
                                    let name = title.strip_prefix("Block: ").unwrap_or(title);
                                    ui.label(crate::i18n::t("Group name"));
                                    if let Some(n) = crate::editors::text_field(ui, ("block_name", *b), name) {
                                        acts.rename = Some((*b, n));
                                    }
                                    if ui.button(crate::i18n::t("Ungroup")).clicked() {
                                        acts.ungroup = Some(*b);
                                        ui.close();
                                    }
                                });
                            }
                        })
                        .body(|ui| self.rows(ui, rows, acts));
                }
            }
        }
    }

    pub(crate) fn structure_panel(&mut self, ui: &mut egui::Ui) {
        let Ok(parts) = &self.parts else { return };
        let project = self.session.as_ref().map(|s| &s.project);
        let rows = tree(parts, project);
        let editable = self.session.is_some();
        let selected_block = self.selected_block();
        let mut acts = Acts::default();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(editable && !self.targets().is_empty(), egui::Button::new(crate::i18n::t("Group"))).on_hover_text(crate::i18n::t("Group the selection into a block: it moves and copies together")).clicked() {
                acts.menu = Some(MenuAct::Block);
            }
            if ui.add_enabled(editable && selected_block.is_some(), egui::Button::new(crate::i18n::t("Ungroup"))).on_hover_text(crate::i18n::t("Select a group (click its name) — its contents move one level up")).clicked() {
                acts.ungroup = selected_block;
            }
            if ui.add_enabled(!self.hidden.is_empty(), egui::Button::new(crate::i18n::t("Show hidden"))).clicked() {
                acts.toggle = Some(Vec::new());
            }
        });
        ui.separator();
        self.rows(ui, &rows, &mut acts);
        self.apply_structure(acts);
    }

    /// Selected block group: the selected parts are exactly the contents of one block.
    fn selected_block(&self) -> Option<u32> {
        let pr = &self.session.as_ref()?.project;
        let parts = self.parts.as_ref().ok()?;
        let mut sel: Vec<usize> = self.selected_all();
        sel.sort_unstable();
        fn find(parts: &[Part], objects: &[Object], sel: &[usize]) -> Option<u32> {
            for o in objects {
                if let Object::Block(b) = o {
                    let mut idx = Vec::new();
                    Row::Group { title: String::new(), key: String::new(), block: None, rows: object_rows(parts, &b.children) }.parts(&mut idx);
                    idx.sort_unstable();
                    if !idx.is_empty() && idx == sel {
                        return Some(b.id);
                    }
                    if let Some(id) = find(parts, &b.children, sel) {
                        return Some(id);
                    }
                }
            }
            None
        }
        find(parts, &pr.objects, &sel)
    }

    fn apply_structure(&mut self, a: Acts) {
        if let Some(idx) = a.select {
            let Ok(parts) = &self.parts else { return };
            let sels: Vec<Sel> = idx.iter().map(|&i| Sel::of(parts, i)).collect();
            if a.add {
                for s in sels {
                    if let Some(k) = self.selection.iter().position(|x| *x == s) {
                        self.selection.remove(k);
                    } else {
                        self.selection.push(s);
                    }
                }
            } else {
                self.selection = sels;
            }
            self.furniture_sel = None;
        }
        if a.menu_hit.is_some() {
            self.menu_pick(a.menu_hit);
            self.furniture_sel = None;
        }
        if let Some(m) = a.menu {
            self.menu_apply(m);
        }
        if let Some(id) = a.furniture {
            self.furniture_sel = Some(id);
            self.selection.clear();
        }
        if let Some(idx) = a.toggle {
            let Ok(parts) = &self.parts else { return };
            if idx.is_empty() {
                self.hidden.clear();
            } else {
                let sels: Vec<Sel> = idx.iter().map(|&i| Sel::of(parts, i)).collect();
                let all_hidden = sels.iter().all(|s| self.hidden.contains(s));
                for s in sels {
                    if all_hidden {
                        self.hidden.remove(&s);
                    } else {
                        self.hidden.insert(s);
                    }
                }
            }
            self.revision += 1;
        }
        if let Some(id) = a.ungroup {
            self.exec("block.explode", json!({ "id": id }));
            self.selection.clear();
        }
        if let Some((id, name)) = a.rename {
            self.exec("block.set", json!({ "id": id, "name": name }));
        }
    }

    /// Parts hidden by the eye: their indices in the current list.
    pub(crate) fn hidden_parts(&self) -> HashSet<usize> {
        let Ok(parts) = &self.parts else { return HashSet::new() };
        self.hidden.iter().filter_map(|s| s.find(parts)).collect()
    }
}
