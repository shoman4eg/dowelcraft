//! Tool icons drawn in code on a 24×24 grid (as in cadcraft, MIT OR Apache-2.0):
//! light lines — what already exists, blue — what the tool creates, orange dots — picked points.

use craft_ui::buttons::{ButtonVisuals, IconButton, IconButtonStyle};
use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, pos2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    New,
    Open,
    Save,
    SaveAs,
    Undo,
    Redo,
    Export,
    Analyze,
    Script,
    Unlock,
    Cabinet,
    WallCabinet,
    Corner,
    Countertop,
    PanelH,
    PanelV,
    PanelF,
    PanelFree,
    Door,
    Drawers,
    Furniture,
    Fastener,
    Move,
    Copy,
    Rotate,
    Mirror,
    MirrorCopy,
    Erase,
    Block,
    Explode,
    ViewFront,
    ViewLeft,
    ViewTop,
    ViewIso,
    Fit,
    Quad,
    Doors,
    ModeSolid,
    ModeHidden,
    ModeWire,
    ModeXRay,
    ModeTextured,
    Grid,
    Gear,
    Table,
    Ruler,
    CutSheet,
    Eye,
    EyeOff,
}

/// Icon colors: regular lines, created (blue), points (orange).
const BASE: Color32 = Color32::from_rgb(0xcf, 0xd4, 0xdb);
const ACCENT: Color32 = Color32::from_rgb(0x4f, 0xa3, 0xf7);
const POINT: Color32 = Color32::from_rgb(0xf5, 0x9e, 0x3b);
const DANGER: Color32 = Color32::from_rgb(0xe5, 0x5b, 0x5b);

struct Pen<'a> {
    p: &'a Painter,
    r: Rect,
    w: f32,
    base: Color32,
    acc: Color32,
    pt: Color32,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        let s = self.r.width() / 24.0;
        pos2(self.r.min.x + x * s, self.r.min.y + y * s)
    }
    fn sc(&self) -> f32 {
        self.r.width() / 24.0
    }
    fn st(&self, c: Color32) -> Stroke {
        Stroke::new(self.w, c)
    }
    fn l(&self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.p.line_segment([self.at(x1, y1), self.at(x2, y2)], self.st(self.base));
    }
    fn la(&self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.p.line_segment([self.at(x1, y1), self.at(x2, y2)], self.st(self.acc));
    }
    fn poly(&self, pts: &[(f32, f32)], closed: bool, c: Color32) {
        let mut v: Vec<Pos2> = pts.iter().map(|(x, y)| self.at(*x, *y)).collect();
        if closed && let Some(f) = v.first().copied() {
            v.push(f);
        }
        self.p.line(v, self.st(c));
    }
    fn c(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_stroke(self.at(x, y), r * self.sc(), self.st(c));
    }
    fn fill_c(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_filled(self.at(x, y), r * self.sc(), c);
    }
    /// Arc in degrees, counter-clockwise on screen.
    fn arc(&self, x: f32, y: f32, r: f32, a0: f32, a1: f32, c: Color32) {
        let n = (((a1 - a0).abs() / 10.0).ceil() as usize).max(2);
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
                self.at(x + r * a.cos(), y - r * a.sin())
            })
            .collect();
        self.p.line(pts, self.st(c));
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32, c: Color32) {
        self.poly(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], true, c);
    }
    fn fill_rect(&self, x: f32, y: f32, w: f32, h: f32, c: Color32) {
        self.p.rect_filled(Rect::from_min_max(self.at(x, y), self.at(x + w, y + h)), 0.0, c);
    }
    fn fill_poly(&self, pts: &[(f32, f32)], c: Color32) {
        self.p.add(egui::Shape::convex_polygon(pts.iter().map(|(x, y)| self.at(*x, *y)).collect(), c, Stroke::NONE));
    }
    fn dot(&self, x: f32, y: f32) {
        self.fill_c(x, y, 1.6, self.pt);
    }
    fn tri(&self, pts: [(f32, f32); 3], c: Color32) {
        self.fill_poly(&pts, c);
    }
    fn arrow_tip(&self, x: f32, y: f32, dx: f32, dy: f32, c: Color32) {
        let l = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (ux, uy) = (dx / l, dy / l);
        let (nx, ny) = (-uy, ux);
        self.tri([(x, y), (x - ux * 3.0 + nx * 1.4, y - uy * 3.0 + ny * 1.4), (x - ux * 3.0 - nx * 1.4, y - uy * 3.0 - ny * 1.4)], c);
    }
    /// Cabinet from the front: side panels, top, bottom, plinth (`plinth`) — from `y0` to `y1`.
    fn carcass(&self, x0: f32, y0: f32, x1: f32, y1: f32, plinth: f32) {
        self.rect(x0, y0, x1 - x0, y1 - y0, self.base);
        if plinth > 0.0 {
            self.l(x0, y1 - plinth, x1, y1 - plinth);
        }
    }
    /// Bar in axonometry: front face `w × h` at (x, y), depth `d` up and to the right.
    fn boxy(&self, x: f32, y: f32, w: f32, h: f32, d: f32, c: Color32) {
        self.rect(x, y, w, h, c);
        self.poly(&[(x, y), (x + d, y - d), (x + w + d, y - d), (x + w, y)], false, c);
        self.poly(&[(x + w + d, y - d), (x + w + d, y + h - d), (x + w, y + h)], false, c);
    }
}

/// Draws `icon` in square `rect`; `dim` — disabled.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, dim: bool) {
    let alpha = |c: Color32| if dim { c.gamma_multiply(0.35) } else { c };
    let pen = Pen { p, r: rect, w: (rect.width() / 16.0).clamp(1.0, 1.6), base: alpha(BASE), acc: alpha(ACCENT), pt: alpha(POINT) };
    let (b, a) = (pen.base, pen.acc);
    use Icon::*;
    match icon {
        New => {
            pen.poly(&[(6.0, 3.0), (15.0, 3.0), (19.0, 7.0), (19.0, 21.0), (6.0, 21.0)], true, b);
            pen.poly(&[(15.0, 3.0), (15.0, 7.0), (19.0, 7.0)], false, b);
            pen.la(12.5, 11.0, 12.5, 17.0);
            pen.la(9.5, 14.0, 15.5, 14.0);
        }
        Open => {
            pen.poly(&[(3.0, 6.0), (9.0, 6.0), (11.0, 8.0), (20.0, 8.0), (20.0, 19.0), (3.0, 19.0)], true, b);
            pen.poly(&[(3.0, 19.0), (6.0, 11.0), (22.0, 11.0), (20.0, 19.0)], false, a);
        }
        Save | SaveAs => {
            pen.poly(&[(4.0, 4.0), (17.0, 4.0), (20.0, 7.0), (20.0, 20.0), (4.0, 20.0)], true, b);
            pen.rect(8.0, 4.0, 8.0, 5.0, b);
            pen.rect(7.0, 13.0, 10.0, 7.0, b);
            if icon == SaveAs {
                pen.la(15.0, 22.0, 22.0, 15.0);
                pen.fill_c(15.0, 22.0, 1.2, a);
            }
        }
        Undo => {
            pen.arc(12.0, 13.0, 7.0, 150.0, -40.0, b);
            pen.arrow_tip(5.0, 9.0, -1.0, 2.5, b);
        }
        Redo => {
            pen.arc(12.0, 13.0, 7.0, 30.0, 220.0, b);
            pen.arrow_tip(19.0, 9.0, 1.0, 2.5, b);
        }
        Export => {
            pen.poly(&[(5.0, 3.0), (14.0, 3.0), (18.0, 7.0), (18.0, 21.0), (5.0, 21.0)], true, b);
            pen.la(11.0, 13.0, 21.0, 13.0);
            pen.arrow_tip(23.0, 13.0, 1.0, 0.0, a);
        }
        Analyze => {
            pen.c(10.0, 10.0, 6.0, b);
            pen.l(14.5, 14.5, 20.0, 20.0);
            pen.poly(&[(7.0, 10.0), (9.5, 12.5), (13.5, 7.5)], false, a);
        }
        Script => {
            pen.rect(3.0, 4.0, 18.0, 16.0, b);
            pen.poly(&[(6.0, 9.0), (9.0, 12.0), (6.0, 15.0)], false, a);
            pen.la(11.0, 15.0, 17.0, 15.0);
        }
        Unlock => {
            pen.rect(5.0, 11.0, 14.0, 10.0, b);
            pen.arc(9.0, 11.0, 4.0, 0.0, 180.0, a);
            pen.fill_c(12.0, 16.0, 1.4, pen.pt);
        }
        Cabinet => {
            pen.carcass(5.0, 3.0, 19.0, 21.0, 2.5);
            pen.la(12.0, 3.0, 12.0, 18.5);
            pen.dot(10.5, 11.0);
            pen.dot(13.5, 11.0);
        }
        WallCabinet => {
            pen.l(3.0, 3.0, 21.0, 3.0);
            pen.carcass(5.0, 5.0, 19.0, 15.0, 0.0);
            pen.la(12.0, 5.0, 12.0, 15.0);
            pen.l(3.0, 21.0, 21.0, 21.0);
        }
        Corner => {
            pen.poly(&[(3.0, 21.0), (3.0, 5.0), (21.0, 5.0)], false, b);
            pen.poly(&[(3.0, 21.0), (10.0, 21.0), (10.0, 12.0), (21.0, 12.0), (21.0, 5.0)], false, a);
            pen.dot(3.0, 5.0);
        }
        Countertop => {
            pen.fill_poly(&[(2.0, 9.0), (6.0, 5.0), (22.0, 5.0), (18.0, 9.0)], a.gamma_multiply(0.45));
            pen.poly(&[(2.0, 9.0), (6.0, 5.0), (22.0, 5.0), (18.0, 9.0)], true, a);
            pen.rect(2.0, 9.0, 16.0, 2.5, a);
            pen.carcass(3.0, 11.5, 17.0, 21.0, 0.0);
        }
        PanelH | PanelV | PanelF => {
            pen.carcass(4.0, 3.0, 20.0, 21.0, 0.0);
            match icon {
                PanelH => pen.fill_rect(4.5, 11.0, 15.0, 2.0, a),
                PanelV => pen.fill_rect(11.0, 3.5, 2.0, 17.0, a),
                _ => pen.fill_rect(6.0, 5.0, 12.0, 14.0, a.gamma_multiply(0.55)),
            }
            pen.dot(16.0, 16.0);
        }
        PanelFree => {
            pen.boxy(4.0, 8.0, 13.0, 12.0, 3.0, a);
            pen.la(18.5, 2.5, 18.5, 7.5);
            pen.la(16.0, 5.0, 21.0, 5.0);
        }
        Door => {
            pen.carcass(4.0, 3.0, 20.0, 21.0, 0.0);
            pen.fill_rect(5.0, 4.0, 14.0, 16.0, a.gamma_multiply(0.45));
            pen.rect(5.0, 4.0, 14.0, 16.0, a);
            pen.la(16.5, 10.0, 16.5, 14.0);
        }
        Drawers => {
            pen.carcass(4.0, 3.0, 20.0, 21.0, 0.0);
            for (y, h) in [(4.0, 4.5), (9.5, 5.0), (15.5, 4.5)] {
                pen.rect(5.0, y, 14.0, h, a);
                pen.la(10.0, y + h / 2.0, 14.0, y + h / 2.0);
            }
        }
        Furniture => {
            // Basket: frame and wires.
            pen.poly(&[(3.0, 8.0), (21.0, 8.0), (18.0, 19.0), (6.0, 19.0)], true, b);
            for x in [8.0, 12.0, 16.0] {
                pen.l(x, 8.0, x - (x - 12.0) * 0.2, 19.0);
            }
            pen.l(4.5, 13.5, 19.5, 13.5);
            pen.poly(&[(8.0, 8.0), (10.0, 4.0), (14.0, 4.0), (16.0, 8.0)], false, a);
        }
        Fastener => {
            pen.rect(3.0, 4.0, 7.0, 16.0, b);
            pen.rect(10.0, 13.0, 11.0, 7.0, b);
            pen.la(3.0, 16.5, 16.0, 16.5);
            pen.dot(16.0, 16.5);
            pen.dot(6.5, 16.5);
        }
        Move => {
            pen.la(12.0, 3.0, 12.0, 21.0);
            pen.la(3.0, 12.0, 21.0, 12.0);
            pen.arrow_tip(12.0, 2.0, 0.0, -1.0, a);
            pen.arrow_tip(12.0, 22.0, 0.0, 1.0, a);
            pen.arrow_tip(2.0, 12.0, -1.0, 0.0, a);
            pen.arrow_tip(22.0, 12.0, 1.0, 0.0, a);
        }
        Copy => {
            pen.rect(3.0, 3.0, 11.0, 13.0, b);
            pen.rect(10.0, 8.0, 11.0, 13.0, a);
        }
        Rotate => {
            pen.arc(12.0, 12.0, 8.0, 100.0, 380.0, a);
            pen.arrow_tip(10.6, 4.1, -1.0, 0.1, a);
            pen.dot(12.0, 12.0);
        }
        Mirror | MirrorCopy => {
            if icon == Mirror {
                pen.poly(&[(3.0, 18.0), (9.0, 6.0), (9.0, 18.0)], false, b.gamma_multiply(0.4));
            } else {
                pen.poly(&[(3.0, 18.0), (9.0, 6.0), (9.0, 18.0)], true, b);
            }
            pen.poly(&[(21.0, 18.0), (15.0, 6.0), (15.0, 18.0)], true, a);
            for y in [3.0, 7.0, 11.0, 15.0, 19.0] {
                pen.l(12.0, y, 12.0, y + 2.0);
            }
        }
        Erase => {
            let red = if dim { DANGER.gamma_multiply(0.35) } else { DANGER };
            pen.poly(&[(4.0, 16.0), (13.0, 7.0), (19.0, 13.0), (12.0, 20.0), (8.0, 20.0)], true, b);
            pen.poly(&[(9.0, 11.0), (15.0, 17.0)], false, red);
            pen.l(8.0, 20.0, 20.0, 20.0);
        }
        Block => {
            pen.poly(&[(12.0, 3.0), (20.0, 7.0), (12.0, 11.0), (4.0, 7.0)], true, a);
            pen.poly(&[(4.0, 7.0), (4.0, 16.0), (12.0, 20.0), (20.0, 16.0), (20.0, 7.0)], false, b);
            pen.l(12.0, 11.0, 12.0, 20.0);
        }
        Explode => {
            pen.poly(&[(4.0, 9.0), (9.0, 4.0)], false, a);
            pen.poly(&[(15.0, 4.0), (20.0, 9.0)], false, a);
            pen.poly(&[(20.0, 15.0), (15.0, 20.0)], false, a);
            pen.poly(&[(9.0, 20.0), (4.0, 15.0)], false, a);
            pen.fill_c(12.0, 12.0, 2.0, pen.pt);
        }
        ViewFront | ViewLeft | ViewTop => {
            // Cube with a highlighted face.
            let front = [(4.0, 9.0), (15.0, 9.0), (15.0, 20.0), (4.0, 20.0)];
            let top = [(4.0, 9.0), (9.0, 4.0), (20.0, 4.0), (15.0, 9.0)];
            let side = [(15.0, 9.0), (20.0, 4.0), (20.0, 15.0), (15.0, 20.0)];
            let lit = match icon {
                ViewFront => &front,
                ViewTop => &top,
                _ => &side,
            };
            pen.fill_poly(lit, a.gamma_multiply(0.55));
            for f in [&front, &top, &side] {
                pen.poly(f, true, b);
            }
        }
        ViewIso => {
            pen.poly(&[(12.0, 3.0), (20.0, 7.5), (12.0, 12.0), (4.0, 7.5)], true, b);
            pen.poly(&[(4.0, 7.5), (4.0, 16.5), (12.0, 21.0), (20.0, 16.5), (20.0, 7.5)], false, b);
            pen.l(12.0, 12.0, 12.0, 21.0);
            pen.dot(12.0, 12.0);
        }
        Fit => {
            for (x, y, dx, dy) in [(3.0, 3.0, 1.0, 1.0), (21.0, 3.0, -1.0, 1.0), (3.0, 21.0, 1.0, -1.0), (21.0, 21.0, -1.0, -1.0)] {
                pen.l(x, y, x + 5.0 * dx, y);
                pen.l(x, y, x, y + 5.0 * dy);
            }
            pen.rect(8.0, 8.0, 8.0, 8.0, a);
        }
        Quad => {
            pen.rect(3.0, 3.0, 18.0, 18.0, b);
            pen.l(12.0, 3.0, 12.0, 21.0);
            pen.l(3.0, 12.0, 21.0, 12.0);
            pen.fill_rect(13.0, 13.0, 7.0, 7.0, a.gamma_multiply(0.55));
        }
        Doors => {
            pen.carcass(4.0, 3.0, 20.0, 21.0, 0.0);
            pen.poly(&[(4.0, 3.0), (10.0, 6.0), (10.0, 24.0 - 6.0), (4.0, 21.0)], true, a);
        }
        Eye | EyeOff => {
            let c = if icon == Eye { b } else { b.gamma_multiply(0.45) };
            pen.arc(12.0, 18.0, 10.0, 35.0, 145.0, c);
            pen.arc(12.0, 6.0, 10.0, 215.0, 325.0, c);
            pen.c(12.0, 12.0, 2.8, c);
            if icon == EyeOff {
                pen.poly(&[(4.0, 20.0), (20.0, 4.0)], false, DANGER.gamma_multiply(if dim { 0.35 } else { 1.0 }));
            }
        }
        Ruler => {
            pen.rect(2.0, 8.0, 20.0, 8.0, b);
            for (x, long) in [(6.0, false), (10.0, true), (14.0, false), (18.0, true)] {
                if long { pen.l(x, 8.0, x, 13.0) } else { pen.la(x, 8.0, x, 11.0) }
            }
        }
        Table => {
            pen.rect(3.0, 4.0, 18.0, 16.0, b);
            pen.fill_rect(3.0, 4.0, 18.0, 4.0, a.gamma_multiply(0.6));
            for y in [8.0, 12.0, 16.0] {
                pen.l(3.0, y, 21.0, y);
            }
            pen.l(9.0, 8.0, 9.0, 20.0);
            pen.l(15.0, 8.0, 15.0, 20.0);
        }
        CutSheet => {
            pen.rect(2.0, 5.0, 20.0, 14.0, b);
            pen.fill_rect(3.0, 6.0, 8.0, 7.0, a.gamma_multiply(0.55));
            pen.fill_rect(12.0, 6.0, 5.0, 12.0, a.gamma_multiply(0.55));
            pen.fill_rect(3.0, 14.0, 8.0, 4.0, a.gamma_multiply(0.55));
        }
        Grid => {
            for k in 0..4 {
                let v = 4.0 + 5.33 * k as f32;
                pen.l(v, 3.0, v, 21.0);
                pen.l(3.0, v, 21.0, v);
            }
        }
        Gear => {
            pen.c(12.0, 12.0, 3.0, b);
            for k in 0..8 {
                let a = (k as f32 * 45.0).to_radians();
                pen.l(12.0 + 5.5 * a.cos(), 12.0 + 5.5 * a.sin(), 12.0 + 8.5 * a.cos(), 12.0 + 8.5 * a.sin());
            }
            pen.c(12.0, 12.0, 6.0, b);
        }
        ModeSolid | ModeHidden | ModeWire | ModeXRay | ModeTextured => {
            let f = [(4.0, 9.0), (15.0, 9.0), (15.0, 20.0), (4.0, 20.0)];
            let t = [(4.0, 9.0), (9.0, 4.0), (20.0, 4.0), (15.0, 9.0)];
            let s = [(15.0, 9.0), (20.0, 4.0), (20.0, 15.0), (15.0, 20.0)];
            match icon {
                ModeSolid | ModeHidden => {
                    pen.fill_poly(&f, b.gamma_multiply(0.55));
                    pen.fill_poly(&t, b.gamma_multiply(0.8));
                    pen.fill_poly(&s, b.gamma_multiply(0.35));
                }
                ModeXRay => {
                    pen.fill_poly(&f, a.gamma_multiply(0.25));
                    pen.fill_poly(&t, a.gamma_multiply(0.25));
                    pen.fill_poly(&s, a.gamma_multiply(0.25));
                }
                ModeTextured => {
                    pen.fill_poly(&f, Color32::from_rgb(0xb8, 0x8a, 0x5a).gamma_multiply(if dim { 0.35 } else { 1.0 }));
                    for y in [12.0, 15.0, 18.0] {
                        pen.l(5.0, y, 14.0, y - 1.0);
                    }
                }
                _ => {}
            }
            for p in [&f, &t, &s] {
                pen.poly(p, true, b);
            }
            if matches!(icon, ModeHidden | ModeXRay) {
                // Hidden edges dashed.
                for k in 0..3 {
                    let y = 9.0 + 4.0 * k as f32;
                    pen.la(9.0, 4.0 + 4.0 * k as f32, 9.0, y - 2.0);
                }
                pen.la(4.0, 20.0, 9.0, 15.0);
                pen.la(9.0, 15.0, 20.0, 15.0);
            }
        }
    }
}

fn style() -> IconButtonStyle {
    let visuals = |fill| ButtonVisuals { fill, stroke: Stroke::NONE, icon: BASE };
    IconButtonStyle {
        normal: visuals(Color32::TRANSPARENT),
        hovered: visuals(Color32::from_rgb(0x5c, 0x67, 0x7c).gamma_multiply(0.6)),
        pressed: None,
        selected: visuals(Color32::from_rgb(0x2f, 0x5f, 0x99)),
        corner_radius: egui::CornerRadius::same(3),
        focus_stroke: Stroke::new(1.0, ACCENT),
    }
}

/// Short caption under the icon in the "Tools" palette.
pub fn caption(icon: Icon) -> &'static str {
    use Icon::*;
    match icon {
        Cabinet => "Cabinet",
        WallCabinet => "Wall",
        Corner => "Corner",
        Countertop => "Countertop",
        PanelFree => "Panel",
        PanelH => "Shelf",
        PanelV => "Partition",
        PanelF => "Back panel",
        Door => "Door",
        Drawers => "Drawers",
        Furniture => "Library",
        Fastener => "Fasteners",
        Move => "Move",
        Copy => "Duplicate",
        Rotate => "Rotate",
        Mirror => "Mirror",
        MirrorCopy => "Mirrored",
        Block => "Block",
        Explode => "Explode",
        Erase => "Delete",
        Analyze => "Analyze",
        Ruler => "Ruler",
        Script => "Script",
        _ => "",
    }
}

/// Palette tool: icon with a caption under it (like labeled palettes), tooltip on hover.
pub fn tool(ui: &mut egui::Ui, enabled: bool, icon: Icon, tooltip: &str, selected: bool) -> egui::Response {
    let enabled = enabled && ui.is_enabled();
    let tooltip = crate::i18n::t(tooltip);
    let text = crate::i18n::t(caption(icon));
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(74.0, 54.0), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let fill = if selected {
        Color32::from_rgb(0x2f, 0x5f, 0x99)
    } else if enabled && resp.hovered() {
        Color32::from_rgb(0x5c, 0x67, 0x7c).gamma_multiply(0.6)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    let icon_rect = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 19.0), egui::vec2(30.0, 30.0));
    paint(ui.painter(), icon_rect, icon, !enabled);
    let color = if enabled { Color32::from_rgb(0xd7, 0xdb, 0xe0) } else { Color32::from_rgb(0x87, 0x8d, 0x96).gamma_multiply(0.7) };
    let galley = ui.painter().layout(text, egui::FontId::proportional(11.0), color, rect.width());
    ui.painter().galley(egui::pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 37.0), galley, color);
    remember_hover(&resp, &if enabled { tooltip.clone() } else { crate::i18n::t(format!("{tooltip} (not available now)")) });
    resp.on_hover_text(tooltip)
}

/// Top-bar "icon + text" button; `selected` — the selected view.
pub fn labeled(ui: &mut egui::Ui, icon: Icon, text: &str, tooltip: &str, selected: bool) -> egui::Response {
    labeled_if(ui, true, icon, text, tooltip, selected)
}

/// Same, can be disabled (pale, not clickable).
pub fn labeled_if(ui: &mut egui::Ui, enabled: bool, icon: Icon, text: &str, tooltip: &str, selected: bool) -> egui::Response {
    let text = crate::i18n::t(text);
    let color = if enabled { Color32::from_rgb(0xd7, 0xdb, 0xe0) } else { Color32::from_rgb(0x80, 0x86, 0x90) };
    let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(13.0), color);
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0 + galley.size().x + 10.0, 26.0), sense);
    let fill = if selected {
        Color32::from_rgb(0x2f, 0x5f, 0x99)
    } else if resp.hovered() {
        Color32::from_rgb(0x5c, 0x67, 0x7c).gamma_multiply(0.6)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 3.0, fill);
    paint(ui.painter(), egui::Rect::from_min_size(rect.min + egui::vec2(4.0, 3.0), egui::vec2(20.0, 20.0)), icon, !enabled);
    ui.painter().galley(egui::pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0), galley, color);
    let tooltip = if enabled { crate::i18n::t(tooltip) } else { crate::i18n::t(format!("{tooltip} (not available now)")) };
    remember_hover(&resp, &tooltip);
    resp.on_hover_text(tooltip)
}

/// Icon button of size `size` with a tooltip; `selected` — pressed state (active tool, mode).
pub fn button(ui: &mut egui::Ui, icon: Icon, size: f32, tooltip: &str, selected: bool) -> egui::Response {
    let tooltip = &crate::i18n::t(tooltip);
    let style = style();
    let resp = IconButton::new(tooltip, egui::vec2(size, size), &style).selected(selected).show(ui, |ui, rect, _| {
        paint(ui.painter(), rect.shrink(size * 0.1), icon, !ui.is_enabled());
    });
    remember_hover(&resp, tooltip);
    resp.on_hover_text(tooltip)
}

fn hover_id() -> egui::Id {
    egui::Id::new("icon_hover_tip")
}

/// Description of the hovered icon — for the status bar (like command hints in cadcraft).
fn remember_hover(resp: &egui::Response, tooltip: &str) {
    if resp.hovered() {
        resp.ctx.data_mut(|d| d.insert_temp(hover_id(), tooltip.to_string()));
    }
}

/// Take the description of the icon the mouse was over on the previous frame.
pub fn take_hover(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|d| d.remove_temp::<String>(hover_id()))
}

#[cfg(test)]
mod tests {
    use eframe::egui;

    /// Hover the icon and keep still — the tooltip opens.
    #[test]
    fn icon_shows_tooltip_on_hover() {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let mut rect = egui::Rect::NOTHING;
        let mut open = false;
        let mut time = 0.0;
        let frame = |events: Vec<egui::Event>, time: f64, rect: &mut egui::Rect, open: &mut bool| {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))), events, time: Some(time), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let r = super::button(ui, super::Icon::Cabinet, 36.0, "Floor cabinet", false);
                    *rect = r.rect;
                    *open = r.is_tooltip_open();
                });
            });
            out.textures_delta.clear();
        };
        frame(vec![], time, &mut rect, &mut open);
        frame(vec![egui::Event::PointerMoved(rect.center())], time, &mut rect, &mut open);
        for _ in 0..20 {
            time += 0.1;
            frame(vec![], time, &mut rect, &mut open);
        }
        assert!(open, "tooltip did not open");
    }

    /// Same inside a craft-ui palette (stack with a header), as in the app window.
    #[test]
    fn icon_shows_tooltip_inside_dock() {
        use craft_ui::docking::{DockArea, DockStyle, Layout, Node, Permissions, StackEntry};
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let layout = Layout { root: Some(Node::Stack { entries: vec![StackEntry { panel: 1u8, open: true, height: None }] }), floating: Vec::new() };
        let mut rect = egui::Rect::NOTHING;
        let mut open = false;
        let mut time = 0.0;
        let frame = |events: Vec<egui::Event>, time: f64, rect: &mut egui::Rect, open: &mut bool| {
            let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))), events, time: Some(time), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let style = DockStyle::from_ui(ui);
                    let _ = DockArea::new(egui::Id::new("dock")).show(ui, &layout, &style, |_| "Tools".to_string(), |_| Permissions::default(), |ui, _| {
                        let r = super::button(ui, super::Icon::Cabinet, 36.0, "Floor cabinet", false);
                        *rect = r.rect;
                        *open = r.is_tooltip_open();
                    });
                });
            });
            out.textures_delta.clear();
        };
        frame(vec![], time, &mut rect, &mut open);
        frame(vec![], time, &mut rect, &mut open);
        frame(vec![egui::Event::PointerMoved(rect.center())], time, &mut rect, &mut open);
        for _ in 0..20 {
            time += 0.1;
            frame(vec![], time, &mut rect, &mut open);
        }
        assert!(open, "palette tooltip did not open, rect {rect:?}");
    }
}
