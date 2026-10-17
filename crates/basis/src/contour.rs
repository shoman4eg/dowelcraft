//! Плоский контур детали (blob `Contour`): u32 n, n сегментов, затем
//! необязательно n × u32 атрибутов сегментов (смысл не установлен).

use std::f64::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    /// 0x10
    Line { a: [f64; 2], b: [f64; 2] },
    /// 0x12: центр, начало, конец (6 × f64), байт направления: 1 — против часовой стрелки.
    Arc { a: [f64; 2], b: [f64; 2], c: [f64; 2], ccw: bool },
    /// 0x11 и 0x14: окружность (центр, радиус); 0x14, видимо, отличается направлением обхода.
    Circle { c: [f64; 2], r: f64 },
}

pub fn parse(v: &[u8]) -> Option<Vec<Seg>> {
    let f = |p: usize| Some(f64::from_le_bytes(v.get(p..p + 8)?.try_into().ok()?));
    let n = u32::from_le_bytes(v.get(..4)?.try_into().ok()?) as usize;
    let mut p = 4;
    let mut out = Vec::with_capacity(n.min(v.len() / 25));
    for _ in 0..n {
        let k = *v.get(p)?;
        let q = p + 1;
        out.push(match k {
            0x10 => Seg::Line { a: [f(q)?, f(q + 8)?], b: [f(q + 16)?, f(q + 24)?] },
            0x12 => Seg::Arc {
                c: [f(q)?, f(q + 8)?],
                a: [f(q + 16)?, f(q + 24)?],
                b: [f(q + 32)?, f(q + 40)?],
                ccw: *v.get(q + 48)? != 0,
            },
            0x11 | 0x14 => Seg::Circle { c: [f(q)?, f(q + 8)?], r: f(q + 16)? },
            _ => return None,
        });
        p += match k {
            0x10 => 33,
            0x12 => 50,
            _ => 25,
        };
    }
    let rest = v.len() - p;
    (rest == 0 || rest == 4 * n).then_some(out)
}

/// Длина сегмента, мм.
pub fn length(s: &Seg) -> f64 {
    match *s {
        Seg::Line { a, b } => (b[0] - a[0]).hypot(b[1] - a[1]),
        Seg::Arc { a, b, c, ccw } => {
            let ang = |p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]);
            let sweep = if ccw { ang(b) - ang(a) } else { ang(a) - ang(b) }.rem_euclid(TAU);
            (a[0] - c[0]).hypot(a[1] - c[1]) * if sweep < 1e-12 { TAU } else { sweep }
        }
        Seg::Circle { r, .. } => TAU * r,
    }
}

/// Габарит [min, max] контура с учётом выпуклости дуг.
pub fn bounds(segs: &[Seg]) -> Option<([f64; 2], [f64; 2])> {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let mut add = |p: [f64; 2]| {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    };
    for s in segs {
        match *s {
            Seg::Line { a, b } => {
                add(a);
                add(b);
            }
            Seg::Arc { a, b, c, ccw } => {
                add(a);
                add(b);
                let r = (a[0] - c[0]).hypot(a[1] - c[1]);
                let (s, e) = if ccw { (a, b) } else { (b, a) };
                let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
                let sweep = ((e[1] - c[1]).atan2(e[0] - c[0]) - a0).rem_euclid(TAU);
                for q in 0..4 {
                    let ang = q as f64 * TAU / 4.0;
                    if (ang - a0).rem_euclid(TAU) <= sweep {
                        add([c[0] + r * ang.cos(), c[1] + r * ang.sin()]);
                    }
                }
            }
            Seg::Circle { c, r } => {
                add([c[0] - r, c[1] - r]);
                add([c[0] + r, c[1] + r]);
            }
        }
    }
    lo[0].is_finite().then_some((lo, hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_bounds_follow_direction() {
        // Полуокружность r=1 от (1,0) до (-1,0): против часовой идёт через верх.
        let up = Seg::Arc { a: [1.0, 0.0], b: [-1.0, 0.0], c: [0.0, 0.0], ccw: true };
        let down = Seg::Arc { a: [1.0, 0.0], b: [-1.0, 0.0], c: [0.0, 0.0], ccw: false };
        let (lo, hi) = bounds(&[up]).unwrap();
        assert!((hi[1] - 1.0).abs() < 1e-9 && lo[1].abs() < 1e-9);
        let (lo, hi) = bounds(&[down]).unwrap();
        assert!((lo[1] + 1.0).abs() < 1e-9 && hi[1].abs() < 1e-9);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse(&[1, 0, 0, 0, 0x99]), None);
        assert_eq!(parse(&[1, 0, 0]), None);
    }
}
