//! Points, bounding boxes, affine transforms and arc geometry in drawing units.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Point {
    pub fn new(x: f64, y: f64, z: f64) -> Point {
        Point { x, y, z }
    }

    pub fn xy(x: f64, y: f64) -> Point {
        Point { x, y, z: 0.0 }
    }
}

impl Default for Point {
    fn default() -> Point {
        Point::xy(0.0, 0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BBox {
    pub min: Point,
    pub max: Point,
}

impl BBox {
    pub fn empty() -> BBox {
        BBox {
            min: Point::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            max: Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }

    pub fn include(&mut self, p: Point) {
        self.min = Point::new(
            self.min.x.min(p.x),
            self.min.y.min(p.y),
            self.min.z.min(p.z),
        );
        self.max = Point::new(
            self.max.x.max(p.x),
            self.max.y.max(p.y),
            self.max.z.max(p.z),
        );
    }

    pub fn union(&self, other: &BBox) -> BBox {
        if other.is_empty() {
            return *self;
        }
        let mut merged = *self;
        merged.include(other.min);
        merged.include(other.max);
        merged
    }

    pub fn width(&self) -> f64 {
        if self.is_empty() {
            return 0.0;
        }
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        if self.is_empty() {
            return 0.0;
        }
        self.max.y - self.min.y
    }

    /// Bounding box of the eight transformed corners.
    pub fn transform(&self, t: &Transform) -> BBox {
        if self.is_empty() {
            return *self;
        }
        let mut out = BBox::empty();
        for x in [self.min.x, self.max.x] {
            for y in [self.min.y, self.max.y] {
                for z in [self.min.z, self.max.z] {
                    out.include(t.apply(Point::new(x, y, z)));
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Transform {
    pub m: [[f64; 3]; 3],
    pub t: Point,
}

impl Transform {
    pub fn identity() -> Transform {
        Transform {
            m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            t: Point::default(),
        }
    }

    pub fn translate(offset: Point) -> Transform {
        Transform {
            t: offset,
            ..Transform::identity()
        }
    }

    /// Scales per axis, rotates counter-clockwise about Z by `rotation_deg`, then translates by `base`.
    pub fn insert(base: Point, scale: Point, rotation_deg: f64) -> Transform {
        let (sin, cos) = rotation_deg.to_radians().sin_cos();
        Transform {
            m: [
                [cos * scale.x, -sin * scale.y, 0.0],
                [sin * scale.x, cos * scale.y, 0.0],
                [0.0, 0.0, scale.z],
            ],
            t: base,
        }
    }

    /// Length of the transformed unit y vector, the factor by which text heights grow.
    pub fn scale_factor(&self) -> f64 {
        self.m[0][1].hypot(self.m[1][1])
    }

    pub fn apply(&self, p: Point) -> Point {
        let row = |r: &[f64; 3]| r[0] * p.x + r[1] * p.y + r[2] * p.z;
        Point::new(
            row(&self.m[0]) + self.t.x,
            row(&self.m[1]) + self.t.y,
            row(&self.m[2]) + self.t.z,
        )
    }

    /// Composes so that `a.then(b).apply(p) == b.apply(a.apply(p))`.
    pub fn then(&self, next: &Transform) -> Transform {
        let mut m = [[0.0; 3]; 3];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| next.m[i][k] * self.m[k][j]).sum();
            }
        }
        Transform {
            m,
            t: next.apply(self.t),
        }
    }
}

fn normalize_degrees(deg: f64) -> f64 {
    deg.rem_euclid(360.0)
}

fn on_circle(center: Point, radius: f64, deg: f64) -> Point {
    let (sin, cos) = deg.to_radians().sin_cos();
    Point::new(center.x + radius * cos, center.y + radius * sin, center.z)
}

/// Bounding box of the counter-clockwise arc from `start_deg` to `end_deg`; equal angles give a full circle.
pub fn arc_bbox(center: Point, radius: f64, start_deg: f64, end_deg: f64) -> BBox {
    let start = normalize_degrees(start_deg);
    let mut end = normalize_degrees(end_deg);
    if end <= start {
        end += 360.0;
    }
    let mut bbox = BBox::empty();
    bbox.include(on_circle(center, radius, start));
    bbox.include(on_circle(center, radius, end));
    for quarter in 0..8 {
        let extreme = f64::from(quarter) * 90.0;
        if extreme > start && extreme < end {
            bbox.include(on_circle(center, radius, extreme));
        }
    }
    bbox
}

/// Returns `(center, radius, start_deg, end_deg)` where `start_deg -> end_deg` is the counter-clockwise sweep of the drawn arc.
/// A positive bulge is the counter-clockwise arc from `p0` to `p1`; a negative bulge is the clockwise one, so the endpoints swap.
pub fn bulge_arc(p0: Point, p1: Point, bulge: f64) -> Option<(Point, f64, f64, f64)> {
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let chord = dx.hypot(dy);
    if bulge == 0.0 || chord == 0.0 {
        return None;
    }
    let half_angle = 2.0 * bulge.atan();
    let signed_radius = chord / (2.0 * half_angle.sin());
    let offset = signed_radius * half_angle.cos();
    let center = Point::new(
        (p0.x + p1.x) / 2.0 - dy / chord * offset,
        (p0.y + p1.y) / 2.0 + dx / chord * offset,
        p0.z,
    );
    let from_p0 = normalize_degrees((p0.y - center.y).atan2(p0.x - center.x).to_degrees());
    let from_p1 = normalize_degrees((p1.y - center.y).atan2(p1.x - center.x).to_degrees());
    if bulge > 0.0 {
        return Some((center, signed_radius.abs(), from_p0, from_p1));
    }
    Some((center, signed_radius.abs(), from_p1, from_p0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn scale_factor_ignores_rotation_and_follows_the_y_scale() {
        let scaled = Transform::insert(Point::default(), Point::new(2.0, 2.0, 2.0), 0.0);
        assert!(close(scaled.scale_factor(), 2.0));
        let turned = Transform::insert(Point::default(), Point::new(1.0, 1.0, 1.0), 90.0);
        assert!(close(turned.scale_factor(), 1.0));
    }

    fn angle_close(a: f64, b: f64) -> bool {
        let diff = (a - b).rem_euclid(360.0);
        diff < 1e-9 || 360.0 - diff < 1e-9
    }

    #[test]
    fn bbox_empty_is_empty_and_include_grows_it() {
        let mut bbox = BBox::empty();
        assert!(bbox.is_empty());
        bbox.include(Point::xy(1.0, 2.0));
        bbox.include(Point::xy(-3.0, 5.0));
        assert!(!bbox.is_empty());
        assert!(close(bbox.width(), 4.0));
        assert!(close(bbox.height(), 3.0));
    }

    #[test]
    fn union_with_an_empty_box_is_the_other_box() {
        let mut bbox = BBox::empty();
        bbox.include(Point::xy(1.0, 1.0));
        assert_eq!(BBox::empty().union(&bbox), bbox);
        assert_eq!(bbox.union(&BBox::empty()), bbox);
    }

    #[test]
    fn arc_bbox_of_a_quarter_arc_from_0_to_90_is_the_first_quadrant_box() {
        let bbox = arc_bbox(Point::default(), 1.0, 0.0, 90.0);
        assert!(close(bbox.min.x, 0.0) && close(bbox.min.y, 0.0));
        assert!(close(bbox.max.x, 1.0) && close(bbox.max.y, 1.0));
    }

    #[test]
    fn arc_bbox_crossing_zero_degrees_includes_the_rightmost_point() {
        let bbox = arc_bbox(Point::default(), 1.0, 350.0, 10.0);
        assert!(close(bbox.max.x, 1.0));
        assert!(bbox.min.x > 0.9);
    }

    #[test]
    fn arc_bbox_with_equal_angles_is_a_full_circle() {
        let bbox = arc_bbox(Point::default(), 2.0, 30.0, 30.0);
        assert!(close(bbox.min.x, -2.0) && close(bbox.max.y, 2.0));
    }

    #[test]
    fn insert_transform_rotates_90_degrees_then_translates() {
        let t = Transform::insert(Point::xy(10.0, 10.0), Point::new(1.0, 1.0, 1.0), 90.0);
        let p = t.apply(Point::xy(1.0, 0.0));
        assert!(close(p.x, 10.0) && close(p.y, 11.0));
    }

    #[test]
    fn translate_moves_a_point_by_the_offset() {
        let moved =
            Transform::translate(Point::new(1.0, -2.0, 3.0)).apply(Point::new(4.0, 5.0, 6.0));
        assert!(close(moved.x, 5.0) && close(moved.y, 3.0) && close(moved.z, 9.0));
    }

    #[test]
    fn then_applies_the_first_transform_before_the_second() {
        let a = Transform::insert(Point::xy(1.0, 2.0), Point::new(2.0, 3.0, 1.0), 30.0);
        let b = Transform::insert(Point::xy(-4.0, 5.0), Point::new(1.0, 2.0, 1.0), 75.0);
        let p = Point::new(0.7, -1.3, 2.0);
        let composed = a.then(&b).apply(p);
        let stepwise = b.apply(a.apply(p));
        assert!(close(composed.x, stepwise.x));
        assert!(close(composed.y, stepwise.y));
        assert!(close(composed.z, stepwise.z));
    }

    #[test]
    fn transformed_bbox_of_a_rotated_unit_square_is_its_hull() {
        let mut square = BBox::empty();
        square.include(Point::xy(0.0, 0.0));
        square.include(Point::xy(1.0, 1.0));
        let t = Transform::insert(Point::default(), Point::new(1.0, 1.0, 1.0), 90.0);
        let hull = square.transform(&t);
        assert!(close(hull.min.x, -1.0) && close(hull.max.x, 0.0));
        assert!(close(hull.min.y, 0.0) && close(hull.max.y, 1.0));
    }

    #[test]
    fn bulge_of_one_is_a_counter_clockwise_semicircle_from_p0_to_p1() {
        let (center, radius, start, end) =
            bulge_arc(Point::xy(0.0, 0.0), Point::xy(2.0, 0.0), 1.0).unwrap();
        assert!(close(center.x, 1.0) && close(center.y, 0.0));
        assert!(close(radius, 1.0));
        assert!(angle_close(start, 180.0));
        assert!(angle_close(end, 0.0));
    }

    #[test]
    fn bulge_of_minus_one_is_the_clockwise_semicircle_swapped_to_a_counter_clockwise_sweep() {
        let (center, radius, start, end) =
            bulge_arc(Point::xy(0.0, 0.0), Point::xy(2.0, 0.0), -1.0).unwrap();
        assert!(close(center.x, 1.0) && close(center.y, 0.0));
        assert!(close(radius, 1.0));
        assert!(angle_close(start, 0.0));
        assert!(angle_close(end, 180.0));
    }

    #[test]
    fn zero_bulge_or_zero_chord_has_no_arc() {
        assert!(bulge_arc(Point::xy(0.0, 0.0), Point::xy(1.0, 0.0), 0.0).is_none());
        assert!(bulge_arc(Point::xy(1.0, 1.0), Point::xy(1.0, 1.0), 1.0).is_none());
    }
}
