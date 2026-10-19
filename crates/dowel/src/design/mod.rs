//! Native product model: parametric cabinets and commands to change them.
//!
//! The project stores parameters (dimensions, shelves, partitions, doors), and parts are built from them
//! by [`build`] as [`Panel`]. So the specification, sheet cutting, cutting maps, estimate and XML
//! for Raskroy work with native models the same way as with Bazis models.
//! All changes go through [`Session::execute`]: a command with JSON parameters.
//! The CLI, the window and MCP will call the same entry point.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::contour::Seg;
use crate::geom::Frame;
use crate::panel::{BlockInfo, Edge, Hole, Panel, Texture};

pub use crate::view::{Face, FrontRect, bounds, front_view, iso_view};

mod commands;
pub mod fill;
pub mod shape;
pub mod actions;
pub mod fronts;
pub mod analysis;
pub mod motion;
pub use commands::{COMMANDS, Session};

/// Native project file `.dcraft`.
pub fn is_project_file(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dcraft"))
}

/// Format tag in a `.dcraft` project file.
pub const FORMAT: &str = "dowel-craft/2";
/// The same format under the program's former name (basis-craft).
const FORMAT_OLD_NAME: &str = "basis-craft/2";
/// Former format: no object tree (free parts stored as bounding boxes). Readable if it has no free parts.
const FORMAT_V1: &str = "basis-craft/1";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    #[serde(default)]
    pub article: String,
    pub thickness: f64,
    /// Sheet size, mm.
    #[serde(default = "default_sheet")]
    pub sheet: (f64, f64),
    /// Material usage coefficient for the estimate.
    #[serde(default = "default_coef")]
    pub coef: f64,
    /// Sheet edge trim, mm; none — the global sheet cutting setting (usually 0 for countertops).
    #[serde(default)]
    pub trim: Option<f64>,
    /// Kerf, mm; none — the global one.
    #[serde(default)]
    pub kerf: Option<f64>,
    /// Texture image for 3D (file path).
    #[serde(default)]
    pub texture: Option<String>,
    /// Texture tile size on parts, mm (length along the grain × width); none — 1000 × 1000.
    #[serde(default)]
    pub texture_size: Option<[f64; 2]>,
    /// Price per m² for the estimate; 0 — not set.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub price: f64,
    /// Weight of 1 m², kg (a sheet's weight divided by its area); 0 — not set.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub weight: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EdgeMaterial {
    pub name: String,
    #[serde(default)]
    pub article: String,
    pub thickness: f64,
    /// Band width, mm.
    #[serde(default)]
    pub width: f64,
    /// Price per metre for the estimate; 0 — not set.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub price: f64,
}

fn default_sheet() -> (f64, f64) {
    (2800.0, 2070.0)
}
fn default_coef() -> f64 {
    1.2
}
fn default_gap() -> f64 {
    2.0
}

/// Shelf: which section (between partitions, left to right from 0) and at what height
/// its bottom face is above the top face of the bottom, mm.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Shelf {
    #[serde(default)]
    pub section: usize,
    pub height: f64,
}

/// Cabinet: side panels, bottom and top between the side panels, plinth, partitions, shelves,
/// overlay back panel and overlay doors. Axes: X — right, Y — up, Z — towards the front.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cabinet {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// Cabinet joint fasteners (name from the library); shelves sit on shelf supports. None — no drilling.
    #[serde(default)]
    pub fastener: Option<String>,
    /// Position of the bottom-left-back corner, mm.
    #[serde(default)]
    pub position: [f64; 3],
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    /// Cabinet material (name from `materials`).
    pub material: String,
    /// Edge banding for front edges and doors (name from `edges`).
    #[serde(default)]
    pub edge: Option<String>,
    /// Plinth height, mm; 0 — no plinth.
    #[serde(default)]
    pub plinth: f64,
    /// The plinth height is on adjustable legs: the side panels stand on them, no plinth board.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legs: bool,
    /// Back panel material; none — no back panel.
    #[serde(default)]
    pub back: Option<String>,
    /// The back panel sits in grooves of the side panels, bottom and top instead of being nailed on the back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_groove: Option<BackGroove>,
    /// Left faces of partitions from the cabinet's left edge, mm.
    #[serde(default)]
    pub partitions: Vec<f64>,
    #[serde(default)]
    pub shelves: Vec<Shelf>,
    /// Number of doors across the width; 0 — open cabinet.
    #[serde(default)]
    pub doors: u32,
    /// Door material; none — the cabinet material.
    #[serde(default)]
    pub door_material: Option<String>,
    /// Gap between doors and at the edges, mm.
    #[serde(default = "default_gap")]
    pub door_gap: f64,
    /// Drawers: front heights bottom to top, mm. Doors cover the height above them.
    #[serde(default)]
    pub drawers: Vec<f64>,
    /// Drawer runner length (box depth), mm; none — cabinet depth rounded down to 50 mm.
    #[serde(default)]
    pub drawer_depth: Option<f64>,
    /// Section with drawers (left to right from 0), if there are partitions.
    #[serde(default)]
    pub drawer_section: usize,
}

/// Drawer front heights where 0 means "Auto": what is left of `space` after the given fronts and the `gaps` is shared
/// equally in whole mm, the remainder going to the top auto front (as Bazis rounds them).
pub(crate) fn front_heights(heights: &[f64], space: f64, gaps: f64) -> Result<Vec<f64>, String> {
    if heights.iter().any(|&h| h < 0.0) {
        return Err("drawer front heights cannot be negative".into());
    }
    let auto = heights.iter().filter(|&&h| h == 0.0).count();
    if auto == 0 {
        return Ok(heights.to_vec());
    }
    let left = space - gaps - heights.iter().sum::<f64>();
    let each = (left / auto as f64).floor();
    if each < 100.0 {
        return Err(format!("auto drawer fronts: {left:.0} mm left for {auto} of them, at least 100 mm each is needed"));
    }
    let top = heights.iter().rposition(|&h| h == 0.0).unwrap_or(0);
    Ok(heights.iter().enumerate().map(|(i, &h)| if h > 0.0 { h } else if i == top { left - each * (auto - 1) as f64 } else { each }).collect())
}

/// Groove for a back panel.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BackGroove {
    /// From the back edge of the carcass to the groove, mm.
    #[serde(default = "default_groove_offset")]
    pub offset: f64,
    /// Into the board, mm; the back panel goes 1 mm less deep.
    #[serde(default = "default_groove_depth")]
    pub depth: f64,
    /// Across, mm; none — the back panel thickness plus 1 mm. The back panel sits in the middle of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
}

fn default_groove_offset() -> f64 {
    16.0
}

fn default_groove_depth() -> f64 {
    8.0
}

impl BackGroove {
    /// Play between the back panel and the groove: across (width) and at the groove bottom, mm.
    const PLAY: f64 = 1.0;
}

/// Countertop: lies on top of cabinets, positioned by its bottom-left-back corner.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Countertop {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub material: String,
    #[serde(default)]
    pub position: [f64; 3],
    pub length: f64,
    pub depth: f64,
    /// Front edge banding.
    #[serde(default)]
    pub edge: Option<String>,
}

/// L-shaped corner unit in a room corner: a wing along X (`width_x`) and a wing along Z (`width_z`)
/// from the shared back corner `position`. Walls are behind the wings, fronts face the inner corner.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Corner {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// Unit joint fasteners; shelves sit on shelf supports.
    #[serde(default)]
    pub fastener: Option<String>,
    #[serde(default)]
    pub position: [f64; 3],
    pub width_x: f64,
    pub width_z: f64,
    pub height: f64,
    pub depth: f64,
    pub material: String,
    #[serde(default)]
    pub edge: Option<String>,
    #[serde(default)]
    pub plinth: f64,
    #[serde(default)]
    pub back: Option<String>,
    /// Shelf heights above the top of the bottom, mm.
    #[serde(default)]
    pub shelves: Vec<f64>,
    /// Doors on both wings.
    #[serde(default)]
    pub doors: bool,
    #[serde(default)]
    pub door_material: Option<String>,
    #[serde(default = "default_gap")]
    pub door_gap: f64,
}

/// Free part: a board with an arbitrary contour — this is how parts from Bazis models get into the project.
/// A panel in the project tree: contour, material, edge banding, holes and exact position.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FreePanel {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    /// Item number (for imported ones — from the source model).
    #[serde(default)]
    pub pos: String,
    #[serde(default)]
    pub designation: String,
    pub material: String,
    /// Contour in panel axes, bounding box from (0, 0).
    pub segs: Vec<Seg>,
    pub texture: Texture,
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Holes in panel coordinates.
    #[serde(default)]
    pub holes: Vec<Hole>,
    /// Grooves (from the Bazis model).
    #[serde(default)]
    pub grooves: Vec<crate::panel::Groove>,
    /// Purpose — for coloring on views.
    #[serde(default)]
    pub kind: Kind,
    /// Position relative to the parent; thickness runs along the panel's +Z.
    pub frame: Frame,
}

/// Block (assembly unit): a named group of objects with its own position.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Block {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub frame: Frame,
    #[serde(default)]
    pub children: Vec<Object>,
}

/// Hardware model (from `.fr3d`): not included in sheet cutting, drills its holes into the panels it
/// touches, counted in the estimate by name. Holes and bounding box are stored in the project — the file may disappear.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Furniture {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub kind: String,
    /// File the model was placed from.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub frame: Frame,
    #[serde(default)]
    pub holes: Vec<crate::panel::WorldHole>,
    /// Bounding box in model axes.
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

/// Project tree object.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Object {
    Panel(FreePanel),
    Block(Block),
    Furniture(Furniture),
}

impl Object {
    pub fn id(&self) -> u32 {
        match self {
            Object::Panel(p) => p.id,
            Object::Block(b) => b.id,
            Object::Furniture(f) => f.id,
        }
    }
}

/// Thickness of a tree panel (from its material).
fn panels_thickness(p: &Project, id: u32) -> Result<f64, String> {
    let fp = p.panels().into_iter().find(|f| f.id == id).ok_or_else(|| format!("no panel {id}"))?;
    Ok(p.material(&fp.material)?.thickness)
}

/// All tree panels, depth-first.
fn walk_panels<'a>(objects: &'a [Object], out: &mut Vec<&'a FreePanel>) {
    for o in objects {
        match o {
            Object::Panel(p) => out.push(p),
            Object::Block(b) => walk_panels(&b.children, out),
            Object::Furniture(_) => {}
        }
    }
}

/// Object with id `id` anywhere in the tree.
fn find_mut(objects: &mut [Object], id: u32) -> Option<&mut Object> {
    for o in objects {
        if o.id() == id {
            return Some(o);
        }
        if let Object::Block(b) = o
            && let Some(found) = find_mut(&mut b.children, id)
        {
            return Some(found);
        }
    }
    None
}

fn remove(objects: &mut Vec<Object>, id: u32) -> Option<Object> {
    if let Some(i) = objects.iter().position(|o| o.id() == id) {
        return Some(objects.remove(i));
    }
    objects.iter_mut().find_map(|o| match o {
        Object::Block(b) => remove(&mut b.children, id),
        Object::Panel(_) | Object::Furniture(_) => None,
    })
}

fn max_id(objects: &[Object]) -> u32 {
    objects
        .iter()
        .map(|o| match o {
            Object::Panel(p) => p.id,
            Object::Furniture(f) => f.id,
            Object::Block(b) => b.id.max(max_id(&b.children)),
        })
        .max()
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Project {
    pub format: String,
    pub name: String,
    #[serde(default)]
    pub materials: Vec<Material>,
    #[serde(default)]
    pub edges: Vec<EdgeMaterial>,
    #[serde(default)]
    pub cabinets: Vec<Cabinet>,
    #[serde(default)]
    pub countertops: Vec<Countertop>,
    #[serde(default)]
    pub corners: Vec<Corner>,
    /// Tree of panels and blocks (free parts, import from Bazis).
    #[serde(default)]
    pub objects: Vec<Object>,
    /// Fasteners between tree panels.
    #[serde(default)]
    pub fasteners: Vec<FastenerInst>,
    /// Custom fastener library; empty — the default library ([`crate::fasteners::library`]).
    #[serde(default)]
    pub fastener_defs: Vec<crate::fasteners::Def>,
}

/// Fastener at a joint of two tree panels: edge of `b` on the face of `a` (order doesn't matter — the joint is searched both ways).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FastenerInst {
    pub id: u32,
    /// Fastener name from the library.
    pub kind: String,
    pub a: u32,
    pub b: u32,
    /// Points along the joint, mm from its start; none — by the fastener rule (offset and pitch).
    #[serde(default)]
    pub positions: Option<Vec<f64>>,
    /// Cam lock cup outside the cabinet (inside by default).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outside: bool,
}

impl Project {
    pub fn new(name: &str) -> Self {
        Project {
            format: FORMAT.into(),
            name: name.into(),
            materials: Vec::new(),
            edges: Vec::new(),
            cabinets: Vec::new(),
            countertops: Vec::new(),
            corners: Vec::new(),
            objects: Vec::new(),
            fasteners: Vec::new(),
            fastener_defs: Vec::new(),
        }
    }

    /// The project's fastener library.
    pub fn defs(&self) -> Vec<crate::fasteners::Def> {
        if self.fastener_defs.is_empty() { crate::fasteners::library() } else { self.fastener_defs.clone() }
    }

    /// Fastener by name (without article number or full).
    pub fn def(&self, name: &str) -> Result<crate::fasteners::Def, String> {
        self.defs()
            .into_iter()
            .find(|d| d.name == name || d.name.split('\r').next() == Some(name))
            .ok_or_else(|| format!("no fastener “{name}” in the catalogue"))
    }

    pub fn load(json: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| format!("project: {e}"))?;
        let format = v.get("format").and_then(Value::as_str).unwrap_or_default();
        if format == FORMAT_V1 && v.get("panels").and_then(Value::as_array).is_some_and(|a| !a.is_empty()) {
            return Err("an old-format project with parts from a Bazis model: import the model again".into());
        }
        if format != FORMAT && format != FORMAT_OLD_NAME && format != FORMAT_V1 {
            return Err(format!("project: unknown format “{format}”"));
        }
        let mut p: Project = serde_json::from_value(v).map_err(|e| format!("project: {e}"))?;
        p.format = FORMAT.into();
        p.rename_legacy_fasteners();
        build(&p)?;
        Ok(p)
    }

    /// Projects saved before the built-in catalogue was in English name its fasteners in Russian (`lang/bazis.tsv`).
    fn rename_legacy_fasteners(&mut self) {
        let legacy: std::collections::HashMap<&str, String> = crate::fasteners::library()
            .into_iter()
            .filter_map(|d| Some((Some(crate::i18n::bazis(&format!("fastener.{}", d.name))).filter(|o| !o.is_empty())?, d.name)))
            .collect();
        let rename = |k: &mut String| {
            if let Some(n) = legacy.get(k.as_str()) {
                *k = n.clone();
            }
        };
        self.cabinets.iter_mut().filter_map(|c| c.fastener.as_mut()).for_each(rename);
        self.corners.iter_mut().filter_map(|c| c.fastener.as_mut()).for_each(rename);
        self.fasteners.iter_mut().for_each(|f| rename(&mut f.kind));
        // A project that ever ran fastener.def keeps the whole catalogue, built-in names and parts of combined kinds included.
        for d in &mut self.fastener_defs {
            rename(&mut d.name);
            d.items.iter_mut().for_each(|(n, _)| rename(n));
        }
    }

    /// All panels of the object tree, depth-first.
    pub fn panels(&self) -> Vec<&FreePanel> {
        let mut out = Vec::new();
        walk_panels(&self.objects, &mut out);
        out
    }

    /// Free id: ids are shared by cabinets, corner units, countertops and tree objects.
    pub fn next_id(&self) -> u32 {
        let c = self.cabinets.iter().map(|c| c.id);
        let k = self.corners.iter().map(|c| c.id);
        let t = self.countertops.iter().map(|c| c.id);
        let f = self.fasteners.iter().map(|f| f.id);
        c.chain(k).chain(t).chain(f).chain([max_id(&self.objects)]).max().unwrap_or(0) + 1
    }

    pub fn save(&self) -> String {
        serde_json::to_string_pretty(self).expect("the project serialises")
    }

    pub fn material(&self, name: &str) -> Result<&Material, String> {
        self.materials.iter().find(|m| m.name == name).ok_or_else(|| format!("no material “{name}”"))
    }

    fn edge(&self, name: &Option<String>) -> Result<Option<&EdgeMaterial>, String> {
        name.as_ref()
            .map(|n| self.edges.iter().find(|e| &e.name == n).ok_or_else(|| format!("no edge banding “{n}”")))
            .transpose()
    }
}

/// Name and article number in Bazis format `Name\rArticle`.
/// Material as panels carry it: `Name\rArticle`, or just the name.
pub fn full(name: &str, article: &str) -> String {
    if article.is_empty() { name.into() } else { format!("{name}\r{article}") }
}

/// Purpose of a Bazis part by its name — only for coloring on views.
/// shortcut: guessed from the words for front, door and back panel; refine if the model has a marker.
pub fn guess_kind(p: &Panel) -> Kind {
    let name = p.name.to_lowercase();
    // Word stems from Bazis models (`lang/bazis.tsv`): any of the door ones; all of the back panel ones.
    let words = |key| crate::i18n::bazis(key).split(',');
    if words("guess.door").any(|w| name.contains(w)) {
        Kind::Door
    } else if words("guess.back").all(|w| name.contains(w)) && p.thickness <= 6.0 {
        Kind::Back
    } else {
        Kind::Body
    }
}

/// Project from Bazis model parts: boards become tree panels with their exact position, Bazis blocks become blocks.
/// Materials come from those encountered (default sheet size and coefficient). Boards without a position are skipped.
/// `blocks` and `panel_block` are the block structure from [`crate::panel::Extract`].
pub fn import(name: &str, panels: &[Panel], blocks: &[BlockInfo], panel_block: &[Option<usize>]) -> Project {
    let mut p = Project::new(name);
    // (Bazis material, thickness) → project material name. One material in different thicknesses becomes different materials.
    let mut seen: Vec<((&str, f64), String)> = Vec::new();
    // The tree is assembled in child lists: 0 — the root, then blocks in order of appearance.
    let mut kids: Vec<Vec<Object>> = vec![Vec::new()];
    let mut made: Vec<(usize, usize)> = Vec::new();
    let mut next_block = panels.len() as u32 + 1;
    let mut block_ids: Vec<(u32, String)> = vec![(0, String::new())];
    for (i, src) in panels.iter().enumerate() {
        let Some(frame) = src.frame else { continue };
        let (mat_name, article) = src.material.split_once('\r').unwrap_or((&src.material, ""));
        let key = match seen.iter().find(|(k, _)| *k == (src.material.as_str(), src.thickness)) {
            Some((_, name)) => name.clone(),
            None => {
                let taken = p.materials.iter().any(|m| m.name == mat_name);
                let key = if taken { format!("{mat_name} ({})", src.thickness) } else { mat_name.to_string() };
                p.materials.push(Material { name: key.clone(), article: article.into(), thickness: src.thickness, sheet: default_sheet(), price: 0.0, weight: 0.0, coef: default_coef(), trim: None, kerf: None, texture: None, texture_size: None });
                seen.push(((src.material.as_str(), src.thickness), key.clone()));
                key
            }
        };
        // Chain of blocks from the root to the board's block; missing blocks are created on first encounter,
        // so the part order when walking the tree matches the order in the model.
        let mut chain = Vec::new();
        let mut b = panel_block.get(i).copied().flatten();
        while let Some(bi) = b {
            chain.push(bi);
            b = blocks.get(bi).and_then(|x| x.parent);
        }
        let mut parent = 0;
        for &bi in chain.iter().rev() {
            parent = match made.iter().find(|(k, _)| *k == bi) {
                Some(&(_, slot)) => slot,
                None => {
                    let slot = kids.len();
                    kids.push(Vec::new());
                    block_ids.push((next_block, blocks.get(bi).map(|x| x.name.clone()).unwrap_or_default()));
                    next_block += 1;
                    kids[parent].push(Object::Block(Block { id: 0, name: String::new(), frame: Frame::IDENTITY, children: Vec::new() }));
                    made.push((bi, slot));
                    // Store the slot number in the placeholder id; the name and children are filled in during assembly.
                    if let Some(Object::Block(stub)) = kids[parent].last_mut() {
                        stub.id = slot as u32;
                    }
                    slot
                }
            };
        }
        kids[parent].push(Object::Panel(FreePanel {
            id: i as u32 + 1,
            name: src.name.clone(),
            pos: src.pos.clone(),
            designation: src.designation.clone(),
            material: key,
            segs: src.segs.clone(),
            texture: src.texture,
            edges: src.edges.clone(),
            holes: src.holes.clone(),
            grooves: src.grooves.clone(),
            kind: guess_kind(src),
            frame,
        }));
    }
    fn assemble(slot: usize, kids: &mut Vec<Vec<Object>>, ids: &[(u32, String)]) -> Vec<Object> {
        let list = std::mem::take(&mut kids[slot]);
        list.into_iter()
            .map(|o| match o {
                Object::Block(stub) => {
                    let s = stub.id as usize;
                    let (id, name) = ids[s].clone();
                    Object::Block(Block { id, name, frame: Frame::IDENTITY, children: assemble(s, kids, ids) })
                }
                panel => panel,
            })
            .collect()
    }
    p.objects = assemble(0, &mut kids, &block_ids);
    p
}

/// Part purpose — for display.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Body,
    Back,
    Door,
    /// Drawer front.
    Drawer,
}

/// Product part: a board for reports (positioned by `panel.frame`) and its bounding box in project coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// Top-level object id: cabinet, corner unit, countertop or tree object.
    pub owner: u32,
    /// Object tree panel id; 0 — a template part (cabinet etc.) or a Bazis model part.
    pub id: u32,
    pub kind: Kind,
    pub panel: Panel,
    /// Bounding box min corner, mm.
    pub origin: [f64; 3],
    /// Bounding box size along X, Y, Z, mm.
    pub size: [f64; 3],
}

impl Part {
    /// Positioned part: the bounding box is computed from the position, contour and thickness.
    fn new(owner: u32, kind: Kind, panel: Panel) -> Result<Part, String> {
        let (lo, hi) = panel.bounds().ok_or_else(|| format!("part “{}” has no position", panel.name))?;
        Ok(Part { owner, id: 0, kind, panel, origin: lo, size: std::array::from_fn(|k| hi[k] - lo[k]) })
    }

    /// Generator part given by a bounding box: the plane is determined by the axis along which the thickness runs.
    /// Front — contour X→X, Y→Y, thickness towards the viewer; horizontal — X→X, Y→+Z (front edge is the contour
    /// top), thickness down; vertical — X→+Z (front edge is the right one), Y→Y, thickness to the left.
    fn placed(owner: u32, kind: Kind, mut panel: Panel, origin: [f64; 3], size: [f64; 3]) -> Result<Part, String> {
        panel.frame = Some(place(&panel, origin, size)?);
        let part = Part::new(owner, kind, panel)?;
        let same = (0..3).all(|k| (part.origin[k] - origin[k]).abs() < 1e-6 && (part.size[k] - size[k]).abs() < 1e-6);
        if !same {
            return Err(format!("part “{}”: the position does not match the box {origin:?} {size:?}", part.panel.name));
        }
        Ok(part)
    }
}

/// Panel position from its bounding box in the project (see [`Part::placed`]).
fn place(panel: &Panel, o: [f64; 3], size: [f64; 3]) -> Result<Frame, String> {
    let eq = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let (t, (sx, sy)) = (panel.thickness, panel.size);
    let (x, y, z) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    if eq(size[2], t) && eq(size[0], sx) && eq(size[1], sy) {
        Ok(Frame::new(o, x, y))
    } else if eq(size[1], t) && eq(size[0], sx) && eq(size[2], sy) {
        Ok(Frame::new([o[0], o[1] + t, o[2]], x, z))
    } else if eq(size[0], t) && eq(size[2], sx) && eq(size[1], sy) {
        Ok(Frame::new([o[0] + t, o[1], o[2]], z, y))
    } else {
        Err(format!("part “{}” {sx}×{sy}×{t} does not fit the box {size:?}", panel.name))
    }
}

/// Plane of a new panel.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Plane {
    /// Front: in the XY plane (width × height).
    Front,
    /// Horizontal: in the XZ plane (width × depth).
    Horizontal,
    /// Vertical (profile): in the ZY plane (depth × height).
    Vertical,
}

/// Sides of a rectangular contour: bottom, left, top, right — in segment order, as in Bazis.
const BOTTOM: usize = 0;
const LEFT: usize = 1;
const TOP: usize = 2;
const RIGHT: usize = 3;

fn rect(name: &str, mat: &Material, sx: f64, sy: f64, texture: Texture, edged: &[usize], edge: Option<&EdgeMaterial>) -> Panel {
    poly(name, mat, &[[sx, 0.0], [0.0, 0.0], [0.0, sy], [sx, sy]], texture, edged, edge)
}

/// Board with a polygonal contour: segment i runs from vertex i to i+1. Bounding box from (0, 0).
fn poly(name: &str, mat: &Material, pts: &[[f64; 2]], texture: Texture, edged: &[usize], edge: Option<&EdgeMaterial>) -> Panel {
    let segs: Vec<Seg> = (0..pts.len()).map(|i| Seg::Line { a: pts[i], b: pts[(i + 1) % pts.len()] }).collect();
    let size = pts.iter().fold((0.0, 0.0), |s: (f64, f64), p| (s.0.max(p[0]), s.1.max(p[1])));
    let edges = edge
        .map(|e| {
            edged
                .iter()
                .map(|&segment| Edge {
                    material: full(&e.name, &e.article),
                    thickness: e.thickness,
                    width: e.width,
                    allowance: 0.0,
                    // Part size in the model is the finished size; edge banding trims the blank.
                    clip: true,
                    segment,
                })
                .collect()
        })
        .unwrap_or_default();
    Panel {
        // Template part names are made in the current language (they show in lists, maps and drawings).
        name: crate::i18n::t(name),
        pos: String::new(),
        designation: String::new(),
        material: full(&mat.name, &mat.article),
        thickness: mat.thickness,
        segs,
        size,
        texture,
        edges,
        frame: None,
        holes: Vec::new(),
        grooves: Vec::new(),
    }
}

/// Builds the parts of all cabinets. Error if the parameters don't yield a valid product.
pub fn build(p: &Project) -> Result<Vec<Part>, String> {
    let mut parts = Vec::new();
    for c in &p.cabinets {
        cabinet(p, c, &mut parts).map_err(|e| format!("cabinet {} “{}”: {e}", c.id, c.name))?;
    }
    for c in &p.corners {
        corner(p, c, &mut parts).map_err(|e| format!("corner unit {} “{}”: {e}", c.id, c.name))?;
    }
    for o in &p.objects {
        objects(p, o.id(), std::slice::from_ref(o), &Frame::IDENTITY, &mut parts)?;
    }
    for ct in &p.countertops {
        countertop(p, ct, &mut parts).map_err(|e| format!("countertop {} “{}”: {e}", ct.id, ct.name))?;
    }
    drill(p, &mut parts)?;
    // Item numbers follow the part order in the project; imported parts keep their own.
    for (i, part) in parts.iter_mut().enumerate() {
        if part.panel.pos.is_empty() {
            part.panel.pos = (i + 1).to_string();
        }
    }
    Ok(parts)
}

/// Fastener points and joints: (fastener name, joint, points). For templates — by their fasteners parameter,
/// for tree panels — by the `fasteners` instances. A joint that no longer exists is skipped (analysis will report it).
pub fn fastener_joints(p: &Project, parts: &[Part]) -> Result<Vec<(crate::fasteners::Def, crate::fasteners::Joint, Vec<f64>)>, String> {
    use crate::fasteners::{joints, positions};
    let mut out = Vec::new();
    let templates = p.cabinets.iter().map(|c| (c.id, &c.fastener)).chain(p.corners.iter().map(|c| (c.id, &c.fastener)));
    for (owner, fastener) in templates {
        let Some(name) = fastener else { continue };
        let main = p.def(name)?;
        let shelf = p.def("Shelf support 5").ok();
        // Drawer boxes, back panels and fronts get no cabinet fasteners.
        let ids: Vec<usize> = (0..parts.len())
            .filter(|&i| parts[i].owner == owner && parts[i].kind == Kind::Body && !parts[i].panel.name.to_lowercase().contains(&crate::i18n::t("drawer")))
            .collect();
        for j in joints(parts, &ids) {
            let def = match (&shelf, parts[j.b].panel.name.to_lowercase().contains(&crate::i18n::t("Shelf").to_lowercase())) {
                (Some(s), true) => s.clone(),
                _ => main.clone(),
            };
            let at = positions(&def, j.length);
            out.push((def, j, at));
        }
    }
    for f in &p.fasteners {
        let def = p.def(&f.kind)?;
        let find = |id: u32| parts.iter().position(|q| q.id == id);
        let (Some(a), Some(b)) = (find(f.a), find(f.b)) else { continue };
        let Some(mut j) = crate::fasteners::joint(parts, a, b).or_else(|| crate::fasteners::joint(parts, b, a)) else { continue };
        if f.outside {
            j.inward = -j.inward;
        }
        let at = f.positions.clone().unwrap_or_else(|| positions(&def, j.length));
        out.push((def, j, at));
    }
    Ok(out)
}

/// Drilling: fastener holes go into the parts. A hole outside the part is an error with an explanation.
/// Then holes of hardware models are drilled into the panels the model touches.
fn drill(p: &Project, parts: &mut [Part]) -> Result<(), String> {
    for (def, j, at) in fastener_joints(p, parts)? {
        for s in at {
            for (i, h) in crate::fasteners::holes(&def, parts, &j, s)? {
                // A spare shelf-support hole can fall on one already drilled (shelves a multiple of 32 mm apart).
                let same = |o: &crate::panel::Hole| (0..3).all(|k| (o.pos[k] - h.pos[k]).abs() < 1e-6 && (o.dir[k] - h.dir[k]).abs() < 1e-9) && o.diameter == h.diameter;
                match parts[i].panel.holes.iter_mut().find(|o| same(o)) {
                    Some(o) => o.depth = o.depth.max(h.depth),
                    None => parts[i].panel.holes.push(h),
                }
            }
        }
    }
    furniture_drill(p, parts);
    Ok(())
}

/// Tree hardware in project coordinates.
pub fn furniture(p: &Project) -> Vec<(Furniture, Frame)> {
    fn walk(list: &[Object], parent: &Frame, out: &mut Vec<(Furniture, Frame)>) {
        for o in list {
            match o {
                Object::Furniture(f) => out.push((f.clone(), parent.then(&f.frame))),
                Object::Block(b) => walk(&b.children, &parent.then(&b.frame), out),
                Object::Panel(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&p.objects, &Frame::IDENTITY, &mut out);
    out.extend(p.cabinets.iter().filter(|c| c.legs && c.plinth > 0.0).flat_map(legs));
    out
}

/// Adjustable legs under a cabinet's bottom, 50 mm in from its edges (a quarter of a small cabinet): 4, or 6 on
/// cabinets wider than 1000 mm. They carry the cabinet's id: picking one picks the cabinet, moving it moves the cabinet.
fn legs(c: &Cabinet) -> Vec<(Furniture, Frame)> {
    const R: f64 = 25.0;
    let (ix, iz) = ((c.width / 4.0).min(50.0), (c.depth / 4.0).min(50.0));
    let xs = if c.width > 1000.0 { vec![ix, c.width / 2.0, c.width - ix] } else { vec![ix, c.width - ix] };
    let o = c.position;
    xs.iter()
        .flat_map(|&x| [iz, c.depth - iz].map(|z| (x, z)))
        .map(|(x, z)| {
            let leg = Furniture {
                id: c.id,
                name: crate::i18n::t(format!("Adjustable leg {:.0} mm", c.plinth)),
                kind: String::new(),
                source: String::new(),
                frame: Frame::IDENTITY,
                holes: Vec::new(),
                lo: [-R, 0.0, -R],
                hi: [R, c.plinth, R],
            };
            (leg, Frame { origin: [o[0] + x, o[1], o[2] + z], ..Frame::IDENTITY })
        })
        .collect()
}

/// Drills hardware holes into parts; returns the number of holes that found no panel (per hardware item).
fn furniture_drill(p: &Project, parts: &mut [Part]) -> Vec<(String, usize)> {
    let mut panels: Vec<Panel> = parts.iter().map(|q| Panel { holes: Vec::new(), ..q.panel.clone() }).collect();
    let mut lost = Vec::new();
    for (f, fr) in furniture(p) {
        let world: Vec<crate::panel::WorldHole> = f
            .holes
            .iter()
            .map(|h| crate::panel::WorldHole { pos: fr.apply(h.pos), dir: fr.dir(h.dir), ..*h })
            .collect();
        let n = crate::panel::assign(&mut panels, &world);
        if n > 0 {
            lost.push((f.name.clone(), n));
        }
    }
    for (part, panel) in parts.iter_mut().zip(panels) {
        part.panel.holes.extend(panel.holes);
    }
    lost
}

/// Hardware holes that hit no panel: (hardware, count) — for model analysis.
pub fn furniture_lost_holes(p: &Project) -> Vec<(String, usize)> {
    let Ok(mut parts) = build(p) else { return Vec::new() };
    parts.iter_mut().for_each(|q| q.panel.holes.clear());
    furniture_drill(p, &mut parts)
}

/// Fasteners for the estimate: (name, count).
pub fn hardware(p: &Project) -> Result<Vec<(String, usize)>, String> {
    let parts = build(p)?;
    let mut out: Vec<(String, usize)> = Vec::new();
    let fasteners = fastener_joints(p, &parts)?.into_iter().flat_map(|(def, _, at)| {
        let per: Vec<(String, usize)> = if def.items.is_empty() { vec![(def.name, 1)] } else { def.items };
        per.into_iter().map(move |(name, k)| (name, k * at.len()))
    });
    let items = fasteners.chain(furniture(p).into_iter().map(|(f, _)| (f.name, 1)));
    for (name, n) in items {
        match out.iter_mut().find(|(k, _)| *k == name) {
            Some(r) => r.1 += n,
            None => out.push((name, n)),
        }
    }
    out.sort();
    Ok(out)
}

/// Fasteners whose joint is gone (panels moved apart or deleted): instance ids and the reason.
pub fn hanging_fasteners(p: &Project) -> Vec<(u32, String)> {
    let Ok(parts) = build(p) else { return Vec::new() };
    p.fasteners
        .iter()
        .filter_map(|f| {
            let find = |id: u32| parts.iter().position(|q| q.id == id);
            match (find(f.a), find(f.b)) {
                (Some(a), Some(b)) if crate::fasteners::joint(&parts, a, b).or_else(|| crate::fasteners::joint(&parts, b, a)).is_some() => None,
                (Some(_), Some(_)) => Some((f.id, format!("“{}” {}: panels {} and {} no longer join", f.kind, f.id, f.a, f.b))),
                _ => Some((f.id, format!("“{}” {}: no panel {} or {}", f.kind, f.id, f.a, f.b))),
            }
        })
        .collect()
}

/// Subtree panels in project coordinates.
fn objects(p: &Project, owner: u32, list: &[Object], parent: &Frame, out: &mut Vec<Part>) -> Result<(), String> {
    for o in list {
        match o {
            Object::Block(b) => {
                b.frame.check().map_err(|e| format!("block {} “{}”: {e}", b.id, b.name))?;
                objects(p, owner, &b.children, &parent.then(&b.frame), out)?;
            }
            // Hardware yields no parts; its holes are drilled by `drill`.
            Object::Furniture(f) => f.frame.check().map_err(|e| format!("hardware {} “{}”: {e}", f.id, f.name))?,
            Object::Panel(fp) => {
                let err = |e: String| format!("part {} “{}”: {e}", fp.id, fp.name);
                let m = p.material(&fp.material).map_err(err)?;
                fp.frame.check().map_err(err)?;
                let (lo, hi) = crate::contour::bounds(&fp.segs).ok_or_else(|| err("empty contour".into()))?;
                if lo[0].abs() > 1e-6 || lo[1].abs() > 1e-6 {
                    return Err(err(format!("the contour box must start at (0, 0), it starts at {lo:?}")));
                }
                let panel = Panel {
                    name: fp.name.clone(),
                    pos: fp.pos.clone(),
                    designation: fp.designation.clone(),
                    material: full(&m.name, &m.article),
                    thickness: m.thickness,
                    segs: fp.segs.clone(),
                    size: (hi[0], hi[1]),
                    texture: fp.texture,
                    edges: fp.edges.clone(),
                    frame: Some(parent.then(&fp.frame)),
                    holes: fp.holes.clone(),
                    grooves: fp.grooves.clone(),
                };
                out.push(Part { id: fp.id, ..Part::new(owner, fp.kind, panel)? });
            }
        }
    }
    Ok(())
}

fn cabinet(p: &Project, c: &Cabinet, out: &mut Vec<Part>) -> Result<(), String> {
    let m = p.material(&c.material)?;
    let edge = p.edge(&c.edge)?;
    let t = m.thickness;
    let (w, h, d) = (c.width, c.height, c.depth);
    let inner_w = w - 2.0 * t;
    let inner_h = h - c.plinth - 2.0 * t;
    if !(t > 0.0 && d > 0.0 && inner_w > 0.0 && inner_h > 0.0 && c.plinth >= 0.0) {
        return Err(format!("size {w}×{h}×{d} with {t} mm boards and a {} mm plinth leaves no inner space", c.plinth));
    }
    let o = c.position;
    let mut add = |kind: Kind, panel: Panel, origin: [f64; 3], size: [f64; 3]| -> Result<(), String> {
        out.push(Part::placed(c.id, kind, panel, [o[0] + origin[0], o[1] + origin[1], o[2] + origin[2]], size)?);
        Ok(())
    };
    // Vertical parts: contour X — depth (from the back edge to the front edge), Y — height, grain along the height.
    let vertical = |name: &str, height: f64| rect(name, m, d, height, Texture::Vertical, &[RIGHT], edge);
    // Horizontal: X — length, Y — depth (front edge is the contour top), grain along the length.
    let horizontal = |name: &str, length: f64| rect(name, m, length, d, Texture::Horizontal, &[TOP], edge);

    // A back panel in grooves: where the grooves are and where the inner space (shelves, partitions, drawers) begins.
    let back_material = c.back.as_ref().map(|n| p.material(n)).transpose()?;
    let groove = match (c.back_groove, back_material) {
        (Some(g), Some(bm)) => {
            let width = g.width.unwrap_or(bm.thickness + BackGroove::PLAY);
            if width < bm.thickness {
                return Err(format!("back panel groove: {width} mm is narrower than the {} mm back panel", bm.thickness));
            }
            if !(g.offset >= 0.0 && g.depth > BackGroove::PLAY && g.depth < t && g.offset + width < d) {
                return Err(format!("back panel groove: offset {} mm and depth {} mm do not fit {t} mm boards {d} mm deep", g.offset, g.depth));
            }
            Some((g, width))
        }
        _ => None,
    };
    let back_z = groove.map_or(0.0, |(g, width)| g.offset + width);
    // Groove along a part: `along_y` — up a vertical part's contour, otherwise along a horizontal one; `inner_back`
    // — the inner face is the part's back face (Z from 0), otherwise the front one.
    let grooved = |mut panel: Panel, along_y: bool, inner_back: bool| {
        if let Some((g, width)) = groove {
            let (sx, sy) = panel.size;
            let path = if along_y { [[g.offset + width, 0.0], [g.offset + width, sy]] } else { [[0.0, g.offset], [sx, g.offset]] };
            let z = if inner_back { [0.0, g.depth] } else { [t - g.depth, t] };
            panel.grooves.push(crate::panel::Groove { name: crate::i18n::t("Back panel groove"), path, across: [0.0, width], z });
        }
        panel
    };

    // Sides: the frame puts a vertical part's back face (Z = 0) towards +X — inside for the left one.
    let (sy, sh) = if c.legs { (c.plinth, h - c.plinth) } else { (0.0, h) };
    add(Kind::Body, grooved(vertical("Left side", sh), true, true), [0.0, sy, 0.0], [t, sh, d])?;
    add(Kind::Body, grooved(vertical("Right side", sh), true, false), [w - t, sy, 0.0], [t, sh, d])?;
    // Horizontal parts have their back face up — inside for the bottom.
    add(Kind::Body, grooved(horizontal("Bottom", inner_w), false, true), [t, c.plinth, 0.0], [inner_w, t, d])?;
    add(Kind::Body, grooved(horizontal("Top panel", inner_w), false, false), [t, h - t, 0.0], [inner_w, t, d])?;
    // Partitions and shelves stop at a grooved back panel.
    let inner_d = d - back_z;
    let vertical = |name: &str, height: f64| rect(name, m, inner_d, height, Texture::Vertical, &[RIGHT], edge);
    let horizontal = |name: &str, length: f64| rect(name, m, length, inner_d, Texture::Horizontal, &[TOP], edge);
    if c.plinth > 0.0 && !c.legs {
        add(Kind::Body, rect("Plinth", m, inner_w, c.plinth, Texture::Horizontal, &[], None), [t, 0.0, d - t], [inner_w, c.plinth, t])?;
    }

    // Partitions and the sections between them.
    let mut xs = c.partitions.clone();
    xs.sort_by(f64::total_cmp);
    let mut sections = Vec::new();
    let mut left = t;
    for &x in &xs {
        if x < left || x + t > w - t {
            return Err(format!("a partition at {x} mm does not fit (free from {left} to {} mm)", w - 2.0 * t));
        }
        sections.push((left, x - left));
        add(Kind::Body, vertical("Partition", inner_h), [x, c.plinth + t, back_z], [t, inner_h, inner_d])?;
        left = x + t;
    }
    sections.push((left, w - t - left));
    if let Some(&(_, sw)) = sections.iter().find(|s| s.1 <= 0.0) {
        return Err(format!("the partitions leave a section {sw} mm wide"));
    }

    let &(dsx, dsw) = sections
        .get(c.drawer_section)
        .ok_or_else(|| format!("drawers: no section {} (there are {})", c.drawer_section, sections.len()))?;
    // Drawer runner length is a multiple of 50 mm; the drawer zone top is above the top front, measured from the top of the bottom.
    let drawer_depth = c.drawer_depth.unwrap_or(((inner_d - 10.0) / 50.0).floor() * 50.0);
    // Fronts with height 0 ("Auto") share what is left of the cabinet front; with doors there is no fixed space for them.
    let drawers = if c.drawers.contains(&0.0) {
        if c.doors > 0 {
            return Err("auto drawer fronts (height 0) fill a cabinet without doors; with doors give the heights".into());
        }
        front_heights(&c.drawers, h - c.plinth - 2.0 * c.door_gap, c.door_gap * (c.drawers.len() - 1) as f64)?
    } else {
        c.drawers.clone()
    };
    let drawer_zone = if drawers.is_empty() {
        0.0
    } else {
        c.door_gap + drawers.iter().map(|hf| hf + c.door_gap).sum::<f64>() - t
    };
    for s in &c.shelves {
        let &(sx, sw) = sections
            .get(s.section)
            .ok_or_else(|| format!("no section {} (there are {})", s.section, sections.len()))?;
        if s.height < 0.0 || s.height + t > inner_h {
            return Err(format!("a shelf at {} mm does not fit (inside is {inner_h} mm)", s.height));
        }
        if s.section == c.drawer_section && s.height < drawer_zone {
            return Err(format!("a shelf at {} mm falls into the drawer zone (up to {drawer_zone} mm)", s.height));
        }
        add(Kind::Body, horizontal("Shelf", sw), [sx, c.plinth + t + s.height, back_z], [sw, t, inner_d])?;
    }

    match (back_material, groove) {
        // In the grooves: between the sides and between the bottom and the top, going into each groove 1 mm less than its depth.
        (Some(bm), Some((g, width))) => {
            let into = g.depth - BackGroove::PLAY;
            let (bw, bh) = (inner_w + 2.0 * into, inner_h + 2.0 * into);
            let at = [t - into, c.plinth + t - into, g.offset + (width - bm.thickness) / 2.0];
            add(Kind::Back, rect("Back panel", bm, bw, bh, Texture::Default, &[], None), at, [bw, bh, bm.thickness])?;
        }
        (Some(bm), None) => {
            let bh = h - c.plinth;
            add(Kind::Back, rect("Back panel", bm, w, bh, Texture::Default, &[], None), [0.0, c.plinth, -bm.thickness], [w, bh, bm.thickness])?;
        }
        (None, _) => {}
    }

    let dm = match &c.door_material {
        Some(n) => p.material(n)?,
        None => m,
    };
    let g = c.door_gap;
    // Drawers: fronts bottom to top with gap g, box for ball-bearing drawer runners (13 mm per side).
    let mut front_y = c.plinth + g;
    // A drawer front covers the section up to the middle of adjacent partitions (for end sections — to the cabinet edge).
    let fx0 = if c.drawer_section == 0 { 0.0 } else { dsx - t / 2.0 };
    let fx1 = if c.drawer_section + 1 == sections.len() { w } else { dsx + dsw + t / 2.0 };
    for (i, &hf) in drawers.iter().enumerate() {
        let fw = fx1 - fx0 - 2.0 * g;
        let bw = dsw - 2.0 * RAIL_GAP;
        let bh = hf - 40.0;
        let bd = drawer_depth;
        if !(hf > 0.0 && fw > 0.0 && bw > 2.0 * t && bh >= 60.0 && bd >= 250.0 && bd <= inner_d) {
            return Err(format!("drawer {} (front {hf} mm, box {bw:.0}×{bh:.0}×{bd:.0}) cannot be built: a front ≥ 100 mm and depth ≥ 250 mm are required", i + 1));
        }
        let facade = rect("Drawer front", dm, fw, hf, Texture::Horizontal, &[BOTTOM, LEFT, TOP, RIGHT], edge);
        add(Kind::Drawer, facade, [fx0 + g, front_y, d], [fw, hf, dm.thickness])?;
        let (bx, by, bz) = (dsx + RAIL_GAP, front_y + 20.0, d - bd);
        // Box: side panels (X — depth, Y — height), walls between the side panels, bottom under the box.
        let side = |name| rect(name, m, bd, bh, Texture::Horizontal, &[TOP], edge);
        add(Kind::Body, side("Drawer side"), [bx, by, bz], [t, bh, bd])?;
        add(Kind::Body, side("Drawer side"), [bx + bw - t, by, bz], [t, bh, bd])?;
        let wall = |name| rect(name, m, bw - 2.0 * t, bh, Texture::Horizontal, &[TOP], edge);
        add(Kind::Body, wall("Drawer front wall"), [bx + t, by, d - t], [bw - 2.0 * t, bh, t])?;
        add(Kind::Body, wall("Drawer back wall"), [bx + t, by, bz], [bw - 2.0 * t, bh, t])?;
        let bm = match &c.back {
            Some(n) => p.material(n)?,
            None => m,
        };
        add(Kind::Body, rect("Drawer bottom", bm, bw, bd, Texture::Default, &[], None), [bx, by - bm.thickness, bz], [bw, bm.thickness, bd])?;
        front_y += hf + g;
    }

    if c.doors > 0 {
        let n = c.doors as f64;
        let (dw, dh) = ((w - (n + 1.0) * g) / n, h - front_y - g);
        if !(dw > 0.0 && dh > 0.0 && g >= 0.0) {
            return Err(format!("{} doors with a {g} mm gap do not fit above the drawers", c.doors));
        }
        for i in 0..c.doors {
            let x = g + i as f64 * (dw + g);
            let door = rect("Door", dm, dw, dh, Texture::Vertical, &[BOTTOM, LEFT, TOP, RIGHT], edge);
            add(Kind::Door, door, [x, front_y, d], [dw, dh, dm.thickness])?;
        }
    } else if front_y > h + 1e-6 {
        return Err(format!("drawers are higher than the cabinet: fronts up to {front_y} mm with a height of {h} mm"));
    }
    Ok(())
}

fn corner(p: &Project, c: &Corner, out: &mut Vec<Part>) -> Result<(), String> {
    let m = p.material(&c.material)?;
    let edge = p.edge(&c.edge)?;
    let t = m.thickness;
    let (h, d, g) = (c.height, c.depth, c.door_gap);
    let (ax, bz) = (c.width_x - t, c.width_z - t);
    let inner_h = h - c.plinth - 2.0 * t;
    if !(ax >= d + 100.0 && bz >= d + 100.0 && inner_h > 0.0 && c.plinth >= 0.0) {
        return Err(format!("the wings {}×{} must be at least 100 mm more than depth {d} plus the board; inner height {inner_h} mm", c.width_x, c.width_z));
    }
    let o = c.position;
    let mut add = |kind: Kind, panel: Panel, origin: [f64; 3], size: [f64; 3]| -> Result<(), String> {
        out.push(Part::placed(c.id, kind, panel, [o[0] + origin[0], o[1] + origin[1], o[2] + origin[2]], size)?);
        Ok(())
    };
    // L-shaped horizontals in plan (X, Z): front edges form the inner corner (segments 3 and 4).
    // shortcut: on views an L-part is drawn as its bounding box (a square); exact 3D needs polyhedra.
    let l_shape = |name| poly(name, m, &[[ax, 0.0], [0.0, 0.0], [0.0, bz], [d, bz], [d, d], [ax, d]], Texture::Horizontal, &[3, 4], edge);
    let side = |name| rect(name, m, d, h, Texture::Vertical, &[RIGHT], edge);
    add(Kind::Body, side("Wing X side"), [ax, 0.0, 0.0], [t, h, d])?;
    add(Kind::Body, side("Wing Z side"), [0.0, 0.0, bz], [d, h, t])?;
    add(Kind::Body, l_shape("Corner bottom"), [0.0, c.plinth, 0.0], [ax, t, bz])?;
    add(Kind::Body, l_shape("Corner top"), [0.0, h - t, 0.0], [ax, t, bz])?;
    for &sh in &c.shelves {
        if sh < 0.0 || sh + t > inner_h {
            return Err(format!("a shelf at {sh} mm does not fit (inside is {inner_h} mm)"));
        }
        add(Kind::Body, l_shape("Corner shelf"), [0.0, c.plinth + t + sh, 0.0], [ax, t, bz])?;
    }
    if c.plinth > 0.0 {
        add(Kind::Body, rect("Plinth", m, ax - d + t, c.plinth, Texture::Horizontal, &[], None), [d - t, 0.0, d - t], [ax - d + t, c.plinth, t])?;
        add(Kind::Body, rect("Plinth", m, bz - d, c.plinth, Texture::Horizontal, &[], None), [d - t, 0.0, d], [t, c.plinth, bz - d])?;
    }
    if let Some(name) = &c.back {
        let bm = p.material(name)?;
        let bh = h - c.plinth;
        add(Kind::Back, rect("Back panel", bm, c.width_x, bh, Texture::Default, &[], None), [0.0, c.plinth, -bm.thickness], [c.width_x, bh, bm.thickness])?;
        add(Kind::Back, rect("Back panel", bm, c.width_z, bh, Texture::Default, &[], None), [-bm.thickness, c.plinth, 0.0], [bm.thickness, bh, c.width_z])?;
    }
    if c.doors {
        let dm = match &c.door_material {
            Some(n) => p.material(n)?,
            None => m,
        };
        let dh = h - c.plinth - 2.0 * g;
        // The X-wing door starts past the Z-wing door so they don't collide in the inner corner.
        let (dwx, dwz) = (c.width_x - d - dm.thickness - 2.0 * g, c.width_z - d - 2.0 * g);
        if !(dwx > 0.0 && dwz > 0.0 && dh > 0.0) {
            return Err("the doors do not fit".into());
        }
        let door = |w| rect("Door", dm, w, dh, Texture::Vertical, &[BOTTOM, LEFT, TOP, RIGHT], edge);
        add(Kind::Door, door(dwx), [d + dm.thickness + g, c.plinth + g, d], [dwx, dh, dm.thickness])?;
        add(Kind::Door, door(dwz), [d, c.plinth + g, d + g], [dm.thickness, dh, dwz])?;
    }
    Ok(())
}

/// Clearance for a ball-bearing drawer runner on each side of the box, mm.
const RAIL_GAP: f64 = 13.0;

fn countertop(p: &Project, ct: &Countertop, out: &mut Vec<Part>) -> Result<(), String> {
    let m = p.material(&ct.material)?;
    if !(ct.length > 0.0 && ct.depth > 0.0) {
        return Err(format!("size {}×{} must be greater than zero", ct.length, ct.depth));
    }
    let panel = rect("Countertop", m, ct.length, ct.depth, Texture::Horizontal, &[TOP], p.edge(&ct.edge)?);
    out.push(Part::placed(ct.id, Kind::Body, panel, ct.position, [ct.length, m.thickness, ct.depth])?);
    Ok(())
}

#[cfg(test)]
mod tests;
