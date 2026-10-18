//! Object placement: origin and axes of the local coordinate system in parent coordinates.
//! Axes rather than a quaternion: for parts aligned with project axes all numbers are exactly 0 and ±1, no float noise.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub origin: [f64; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub z: [f64; 3],
}

impl Default for Frame {
    fn default() -> Self {
        Frame::IDENTITY
    }
}

pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}

impl Frame {
    pub const IDENTITY: Frame = Frame { origin: [0.0; 3], x: [1.0, 0.0, 0.0], y: [0.0, 1.0, 0.0], z: [0.0, 0.0, 1.0] };

    /// System with axes `x`, `y` and `z = x × y`.
    pub fn new(origin: [f64; 3], x: [f64; 3], y: [f64; 3]) -> Self {
        Frame { origin, x, y, z: cross(x, y) }
    }

    /// Direction from local to parent coordinates.
    pub fn dir(&self, v: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| self.x[i] * v[0] + self.y[i] * v[1] + self.z[i] * v[2])
    }

    /// Point from local to parent coordinates.
    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let d = self.dir(p);
        std::array::from_fn(|i| d[i] + self.origin[i])
    }

    /// Point from parent to local coordinates.
    pub fn local(&self, p: [f64; 3]) -> [f64; 3] {
        let d = std::array::from_fn(|i| p[i] - self.origin[i]);
        [dot(d, self.x), dot(d, self.y), dot(d, self.z)]
    }

    /// System `child`, given in this one, expressed in this one's parent coordinates.
    pub fn then(&self, child: &Frame) -> Frame {
        Frame { origin: self.apply(child.origin), x: self.dir(child.x), y: self.dir(child.y), z: self.dir(child.z) }
    }

    /// Axes are unit, mutually perpendicular and right-handed (no mirroring).
    pub fn check(&self) -> Result<(), String> {
        let ok = |v: f64, want: f64| (v - want).abs() < 1e-9;
        let (x, y, z) = (self.x, self.y, self.z);
        let unit = ok(dot(x, x), 1.0) && ok(dot(y, y), 1.0) && ok(dot(z, z), 1.0);
        let ortho = ok(dot(x, y), 0.0) && ok(dot(y, z), 0.0) && ok(dot(x, z), 0.0);
        let right = ok(dot(cross(x, y), z), 1.0);
        let finite = self.origin.iter().chain(&x).chain(&y).chain(&z).all(|v| v.is_finite());
        if unit && ortho && right && finite { Ok(()) } else { Err(format!("the position axes are not a right-handed orthonormal frame: {self:?}")) }
    }

    /// Bounding box [lo, hi] of a local box in parent coordinates.
    pub fn bounds(&self, lo: [f64; 3], hi: [f64; 3]) -> ([f64; 3], [f64; 3]) {
        (0..8)
            .map(|i| self.apply(std::array::from_fn(|k| if i >> k & 1 == 0 { lo[k] } else { hi[k] })))
            .fold(([f64::MAX; 3], [f64::MIN; 3]), |(a, b), p| (std::array::from_fn(|k| a[k].min(p[k])), std::array::from_fn(|k| b[k].max(p[k]))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_and_invert() {
        // Horizontal panel: X → X, Y → +Z, thickness (Z) → −Y.
        let f = Frame::new([10.0, 20.0, 30.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
        assert_eq!(f.z, [0.0, -1.0, 0.0]);
        f.check().unwrap();
        let p = [1.0, 2.0, 3.0];
        assert_eq!(f.local(f.apply(p)), p);
        let g = Frame::new([5.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]);
        let gf = g.then(&f);
        assert_eq!(gf.apply(p), g.apply(f.apply(p)));
        assert_eq!(f.bounds([0.0; 3], [100.0, 50.0, 16.0]), ([10.0, 4.0, 30.0], [110.0, 20.0, 80.0]));
        assert!(Frame { x: [2.0, 0.0, 0.0], ..Frame::IDENTITY }.check().is_err());
        assert!(Frame { z: [0.0, 0.0, -1.0], ..Frame::IDENTITY }.check().is_err());
    }
}
