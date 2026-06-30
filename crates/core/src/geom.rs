//! Geometry primitives. pic works in a Cartesian plane with y pointing up;
//! internal units are pic "inches" (scaled to device units by each backend).

use std::ops::{Add, Div, Mul, Sub};

/// A point / 2-vector in pic coordinate space (inches, y-up).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    /// Euclidean distance to another point.
    pub fn dist(self, other: Point) -> f64 {
        (self - other).len()
    }

    /// Length (magnitude) of this point treated as a vector.
    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }

    /// Linear interpolation: `self` at t=0, `other` at t=1.
    pub fn lerp(self, other: Point, t: f64) -> Point {
        self + (other - self) * t
    }

    /// Rotate around the origin by `angle` radians (counter-clockwise).
    pub fn rotate(self, angle: f64) -> Point {
        let (s, c) = angle.sin_cos();
        Point::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Point {
    type Output = Point;
    fn sub(self, o: Point) -> Point {
        Point::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Point {
    type Output = Point;
    fn mul(self, k: f64) -> Point {
        Point::new(self.x * k, self.y * k)
    }
}
impl Div<f64> for Point {
    type Output = Point;
    fn div(self, k: f64) -> Point {
        Point::new(self.x / k, self.y / k)
    }
}

/// An axis-aligned bounding box. Empty until the first point is added.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bbox {
    pub min: Point,
    pub max: Point,
    empty: bool,
}

impl Default for Bbox {
    fn default() -> Self {
        Bbox::new()
    }
}

impl Bbox {
    pub fn new() -> Self {
        Bbox {
            min: Point::ZERO,
            max: Point::ZERO,
            empty: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// Extend the box to include a point.
    pub fn add(&mut self, p: Point) {
        if !p.x.is_finite() || !p.y.is_finite() {
            return;
        }
        if self.empty {
            self.min = p;
            self.max = p;
            self.empty = false;
        } else {
            self.min.x = self.min.x.min(p.x);
            self.min.y = self.min.y.min(p.y);
            self.max.x = self.max.x.max(p.x);
            self.max.y = self.max.y.max(p.y);
        }
    }

    /// Merge another bounding box into this one.
    pub fn union(&mut self, other: &Bbox) {
        if !other.empty {
            self.add(other.min);
            self.add(other.max);
        }
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_ops() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(3.0, 4.0);
        assert_eq!(a + b, Point::new(4.0, 6.0));
        assert_eq!(b - a, Point::new(2.0, 2.0));
        assert_eq!(a * 2.0, Point::new(2.0, 4.0));
        assert_eq!(a.lerp(b, 0.5), Point::new(2.0, 3.0));
        assert!((Point::new(3.0, 4.0).len() - 5.0).abs() < 1e-12);
    }

    #[test]
    fn bbox_grows() {
        let mut bb = Bbox::new();
        assert!(bb.is_empty());
        bb.add(Point::new(1.0, 1.0));
        bb.add(Point::new(-1.0, 3.0));
        assert_eq!(bb.min, Point::new(-1.0, 1.0));
        assert_eq!(bb.max, Point::new(1.0, 3.0));
        assert_eq!(bb.width(), 2.0);
        assert_eq!(bb.height(), 2.0);
    }
}
