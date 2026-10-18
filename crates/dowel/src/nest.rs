//! Guillotine sheet nesting: each part is cut off by a through cut,
//! the remainder splits into two rectangles. The greedy layout is tried with different part orders,
//! remainder choice and cut direction; the layout with the fewest sheets wins, and on a tie —
//! the one with the largest single offcut (fewer small offcuts).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Sheet size, mm: length (along sheet grain) × width.
    pub sheet: (f64, f64),
    pub kerf: f64,
    /// Edge trim of the sheet on each side.
    pub trim: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Item {
    /// Part index on the caller's side.
    pub id: usize,
    pub length: f64,
    pub width: f64,
    /// Has grain: the part length must run along the sheet length.
    pub grain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub id: usize,
    /// Bottom-left corner on the sheet.
    pub x: f64,
    pub y: f64,
    /// Size on the sheet (after possible rotation).
    pub w: f64,
    pub h: f64,
    pub rotated: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sheet {
    pub parts: Vec<Placed>,
    /// Free remainders after nesting (non-overlapping): offcuts and waste.
    pub free: Vec<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Sheet {
    /// Fraction of sheet area under parts.
    pub fn usage(&self, p: &Params) -> f64 {
        self.parts.iter().map(|d| d.w * d.h).sum::<f64>() / (p.sheet.0 * p.sheet.1)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub sheets: Vec<Sheet>,
    /// Parts that don't fit on a clean sheet.
    pub too_big: Vec<usize>,
}

pub fn nest(items: &[Item], p: &Params) -> Layout {
    let area = |it: &Item| it.length * it.width;
    let orders: [fn(&Item) -> f64; 4] = [|it| it.length * it.width, |it| it.length, |it| it.width, |it| it.length.max(it.width)];
    let mut best: Option<(Layout, (usize, f64, usize))> = None;
    for key in orders {
        let mut order: Vec<&Item> = items.iter().collect();
        order.sort_by(|a, b| key(b).total_cmp(&key(a)).then(area(b).total_cmp(&area(a))).then(a.id.cmp(&b.id)));
        for fit in 0..3 {
            for split in [false, true] {
                let lay = greedy(&order, p, fit, split);
                // Fewer sheets, then the largest offcut, then fewer offcuts.
                let biggest = lay.sheets.iter().flat_map(|s| &s.free).map(|r| r.w * r.h).fold(0.0, f64::max);
                let pieces = lay.sheets.iter().map(|s| s.free.len()).sum();
                let score = (lay.sheets.len(), -biggest, pieces);
                if best.as_ref().is_none_or(|(_, b)| score.0 < b.0 || (score.0 == b.0 && (score.1 < b.1 || (score.1 == b.1 && score.2 < b.2)))) {
                    best = Some((lay, score));
                }
            }
        }
    }
    best.map(|b| b.0).unwrap_or_default()
}

/// One greedy layout: `fit` — remainder choice (0 — best short side, 1 — least leftover
/// area, 2 — best long side); `split` — cut along the long remainder rather than the short one.
fn greedy(order: &[&Item], p: &Params, fit: u8, split: bool) -> Layout {
    let usable = Rect { x: p.trim, y: p.trim, w: p.sheet.0 - 2.0 * p.trim, h: p.sheet.1 - 2.0 * p.trim };
    let fits = |f: &Rect, w: f64, h: f64| w <= f.w + 1e-9 && h <= f.h + 1e-9;
    let orients = |it: &Item| {
        let mut o = vec![(it.length, it.width, false)];
        if !it.grain && it.length != it.width {
            o.push((it.width, it.length, true));
        }
        o
    };

    let mut out = Layout::default();
    let mut free: Vec<Vec<Rect>> = Vec::new();
    for &it in order {
        if !orients(it).iter().any(|&(w, h, _)| fits(&usable, w, h)) {
            out.too_big.push(it.id);
            continue;
        }
        // Best free rectangle on already opened sheets, otherwise a new sheet.
        // (score, sheet, free rectangle, (width, height, rotated))
        type Choice = (f64, usize, usize, (f64, f64, bool));
        let mut best: Option<Choice> = None;
        for (s, rects) in free.iter().enumerate() {
            for (r, f) in rects.iter().enumerate() {
                for o in orients(it) {
                    if fits(f, o.0, o.1) {
                        let score = match fit {
                            0 => (f.w - o.0).min(f.h - o.1),
                            1 => f.w * f.h - o.0 * o.1,
                            _ => (f.w - o.0).max(f.h - o.1),
                        };
                        if best.is_none_or(|b| score < b.0) {
                            best = Some((score, s, r, o));
                        }
                    }
                }
            }
        }
        let (s, r, (w, h, rotated)) = match best {
            Some((_, s, r, o)) => (s, r, o),
            None => {
                free.push(vec![usable]);
                out.sheets.push(Sheet::default());
                let o = orients(it).into_iter().find(|&(w, h, _)| fits(&usable, w, h)).unwrap();
                (free.len() - 1, 0, o)
            }
        };
        let f = free[s].swap_remove(r);
        out.sheets[s].parts.push(Placed { id: it.id, x: f.x, y: f.y, w, h, rotated });
        // Cut along the short remainder: the larger piece stays whole.
        let (rw, rh) = (f.w - w - p.kerf, f.h - h - p.kerf);
        let (right, top) = if (f.w - w < f.h - h) != split {
            (Rect { x: f.x + w + p.kerf, y: f.y, w: rw, h }, Rect { x: f.x, y: f.y + h + p.kerf, w: f.w, h: rh })
        } else {
            (Rect { x: f.x + w + p.kerf, y: f.y, w: rw, h: f.h }, Rect { x: f.x, y: f.y + h + p.kerf, w, h: rh })
        };
        free[s].extend([right, top].into_iter().filter(|r| r.w > 0.0 && r.h > 0.0));
    }
    for (s, rects) in free.into_iter().enumerate() {
        out.sheets[s].free = merge(rects, p.kerf);
    }
    out
}

/// Merges adjacent remainders of the same height (or width), separated by a kerf, into one offcut.
fn merge(mut rs: Vec<Rect>, kerf: f64) -> Vec<Rect> {
    let eq = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let touch = |a: f64, b: f64| b >= a - 1e-6 && b <= a + kerf + 1e-6;
    'again: loop {
        for i in 0..rs.len() {
            for j in 0..rs.len() {
                let (a, b) = (rs[i], rs[j]);
                if i == j {
                    continue;
                }
                let joined = if eq(a.y, b.y) && eq(a.h, b.h) && touch(a.x + a.w, b.x) {
                    Rect { w: b.x + b.w - a.x, ..a }
                } else if eq(a.x, b.x) && eq(a.w, b.w) && touch(a.y + a.h, b.y) {
                    Rect { h: b.y + b.h - a.y, ..a }
                } else {
                    continue;
                };
                rs[i] = joined;
                rs.swap_remove(j);
                continue 'again;
            }
        }
        return rs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Params = Params { sheet: (2800.0, 2070.0), kerf: 4.0, trim: 10.0 };

    fn item(id: usize, l: f64, w: f64, grain: bool) -> Item {
        Item { id, length: l, width: w, grain }
    }

    fn check(items: &[Item], lay: &Layout) {
        let placed: usize = lay.sheets.iter().map(|s| s.parts.len()).sum();
        assert_eq!(placed + lay.too_big.len(), items.len(), "каждая деталь либо уложена, либо в too_big");
        for s in &lay.sheets {
            for (i, a) in s.parts.iter().enumerate() {
                assert!(a.x >= P.trim - 1e-9 && a.y >= P.trim - 1e-9);
                assert!(a.x + a.w <= P.sheet.0 - P.trim + 1e-9 && a.y + a.h <= P.sheet.1 - P.trim + 1e-9);
                let it = items.iter().find(|it| it.id == a.id).unwrap();
                if it.grain {
                    assert!(!a.rotated, "деталь с текстурой повёрнута");
                }
                for b in &s.parts[i + 1..] {
                    let apart = a.x + a.w + P.kerf <= b.x + 1e-9
                        || b.x + b.w + P.kerf <= a.x + 1e-9
                        || a.y + a.h + P.kerf <= b.y + 1e-9
                        || b.y + b.h + P.kerf <= a.y + 1e-9;
                    assert!(apart, "детали {} и {} ближе пропила", a.id, b.id);
                }
            }
        }
    }

    #[test]
    fn wardrobe_parts() {
        let mut items = Vec::new();
        for (i, &(l, w, g)) in [
            (1868.0, 513.0, true),
            (1868.0, 513.0, true),
            (1868.0, 513.0, true),
            (1868.0, 513.0, true),
            (1075.0, 513.0, true),
            (1075.0, 513.0, true),
            (337.0, 513.0, false),
            (337.0, 513.0, false),
            (484.0, 153.0, false),
            (484.0, 153.0, false),
            (279.0, 153.0, false),
            (279.0, 153.0, false),
        ]
        .iter()
        .enumerate()
        {
            items.push(item(i, l, w, g));
        }
        let lay = nest(&items, &P);
        check(&items, &lay);
        assert!(lay.too_big.is_empty());
        // 4 × 513 + kerfs = 2064 > 2050 usable width: won't fit on one sheet.
        assert_eq!(lay.sheets.len(), 2);
    }

    /// Bathroom cabinet and vanity on one 2750×1830 sheet: parts are packed tightly, leaving a large
    /// single offcut rather than many small ones (previously the largest was 1058×948).
    #[test]
    fn leftover_stays_in_one_piece() {
        let p = Params { sheet: (2750.0, 1830.0), kerf: 4.0, trim: 10.0 };
        let sizes = [
            (1100.0, 298.0), (1100.0, 298.0), (564.0, 298.0), (564.0, 298.0), (564.0, 278.0), (564.0, 278.0), (564.0, 278.0),
            (808.0, 277.0), (808.0, 277.0), (798.0, 348.0), (798.0, 348.0), (564.0, 100.0), (564.0, 100.0), (564.0, 100.0),
            (716.0, 295.0), (716.0, 295.0),
        ];
        let items: Vec<_> = sizes.iter().enumerate().map(|(i, &(l, w))| item(i, l, w, true)).collect();
        let lay = nest(&items, &p);
        assert_eq!(lay.sheets.len(), 1);
        let biggest = lay.sheets[0].free.iter().map(|r| r.w * r.h).fold(0.0, f64::max);
        eprintln!("самый большой обрезок {biggest}, обрезков {}: {:?}", lay.sheets[0].free.len(), lay.sheets[0].free);
        assert!(biggest > 1058.0 * 948.0 * 1.2, "{biggest}");
    }

    #[test]
    fn oversize_is_reported_not_lost() {
        let items = [item(0, 3000.0, 100.0, true), item(1, 100.0, 2790.0, true), item(2, 2000.0, 100.0, false)];
        let lay = nest(&items, &P);
        check(&items, &lay);
        assert_eq!(lay.too_big, vec![0, 1]);
    }

    #[test]
    fn grain_blocks_rotation() {
        // Without rotation 2000×100 won't fit across the sheet width, and with grain rotation is not allowed.
        let items = [item(0, 100.0, 2000.0, true)];
        let lay = nest(&items, &P);
        check(&items, &lay);
        assert!(!lay.sheets[0].parts[0].rotated);
    }

    #[test]
    fn neighbouring_leftovers_merge() {
        let r = |x, y, w, h| Rect { x, y, w, h };
        let m = merge(vec![r(0.0, 0.0, 100.0, 50.0), r(104.0, 0.0, 96.0, 50.0), r(0.0, 60.0, 10.0, 10.0)], 4.0);
        assert_eq!(m.len(), 2);
        assert!(m.contains(&r(0.0, 0.0, 200.0, 50.0)));
    }

    #[test]
    fn many_parts_spill_to_new_sheets() {
        let items: Vec<_> = (0..40).map(|i| item(i, 700.0, 500.0, i % 2 == 0)).collect();
        let lay = nest(&items, &P);
        check(&items, &lay);
        assert!(lay.sheets.len() >= 3);
    }
}
