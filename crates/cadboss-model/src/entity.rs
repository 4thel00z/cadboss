//! Drawing entities and their bounding boxes.

use cadboss_core::{arc_bbox, bulge_arc, BBox, Point};
use serde::Serialize;

use crate::text::Text;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EntityCommon {
    pub handle: Option<u64>,
    pub layer: Text,
    pub layout: Text,
    pub color: i16,
    pub hyperlink: Option<Text>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Attribute {
    pub tag: Text,
    pub value: Text,
    pub position: Point,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entity {
    pub common: EntityCommon,
    pub kind: EntityKind,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum EntityKind {
    Line {
        start: Point,
        end: Point,
    },
    Circle {
        center: Point,
        radius: f64,
    },
    Arc {
        center: Point,
        radius: f64,
        start: f64,
        end: f64,
    },
    Ellipse {
        center: Point,
        major: Point,
        ratio: f64,
        start: f64,
        end: f64,
    },
    /// `lightweight` is set for an LWPOLYLINE record and clear for a POLYLINE with VERTEX records.
    Polyline {
        points: Vec<Point>,
        bulges: Vec<f64>,
        closed: bool,
        lightweight: bool,
    },
    Spline {
        control: Vec<Point>,
        fit: Vec<Point>,
        degree: u16,
        closed: bool,
    },
    Hatch {
        pattern: Text,
        solid: bool,
        loops: u32,
    },
    Text {
        text: Text,
        position: Point,
        height: f64,
        rotation: f64,
        style: Text,
    },
    MText {
        text: Text,
        position: Point,
        height: f64,
        width: f64,
        rotation: f64,
        style: Text,
    },
    Dimension {
        text: Text,
        measurement: Option<f64>,
        text_position: Option<Point>,
        dim_type: u16,
        block_name: Text,
        style: Text,
    },
    Leader {
        text: Option<Text>,
        points: Vec<Point>,
    },
    Insert {
        block_name: Text,
        position: Point,
        scale: Point,
        rotation: f64,
        attributes: Vec<Attribute>,
    },
    Point {
        position: Point,
    },
    Solid {
        points: Vec<Point>,
    },
    Image {
        path: Option<Text>,
        image_def: Option<u64>,
        position: Point,
        size: Point,
    },
    Viewport {
        center: Point,
        width: f64,
        height: f64,
        view_center: Point,
        view_height: f64,
        frozen_layers: Vec<Text>,
    },
    Unknown {
        type_name: Text,
    },
}

impl EntityKind {
    pub fn type_name(&self) -> &'static str {
        match self {
            EntityKind::Line { .. } => "LINE",
            EntityKind::Circle { .. } => "CIRCLE",
            EntityKind::Arc { .. } => "ARC",
            EntityKind::Ellipse { .. } => "ELLIPSE",
            EntityKind::Polyline {
                lightweight: true, ..
            } => "LWPOLYLINE",
            EntityKind::Polyline {
                lightweight: false, ..
            } => "POLYLINE",
            EntityKind::Spline { .. } => "SPLINE",
            EntityKind::Hatch { .. } => "HATCH",
            EntityKind::Text { .. } => "TEXT",
            EntityKind::MText { .. } => "MTEXT",
            EntityKind::Dimension { .. } => "DIMENSION",
            EntityKind::Leader { .. } => "LEADER",
            EntityKind::Insert { .. } => "INSERT",
            EntityKind::Point { .. } => "POINT",
            EntityKind::Solid { .. } => "SOLID",
            EntityKind::Image { .. } => "IMAGE",
            EntityKind::Viewport { .. } => "VIEWPORT",
            EntityKind::Unknown { .. } => "UNKNOWN",
        }
    }
}

fn points_bbox(points: &[Point]) -> Option<BBox> {
    let mut bbox = BBox::empty();
    for point in points {
        bbox.include(*point);
    }
    Some(bbox).filter(|bbox| !bbox.is_empty())
}

/// Segment `i` runs from point `i` to point `i + 1`, and a closed polyline adds the segment back to the first point.
fn polyline_bbox(points: &[Point], bulges: &[f64], closed: bool) -> Option<BBox> {
    let mut bbox = points_bbox(points)?;
    let ends = points
        .iter()
        .skip(1)
        .chain(points.first().filter(|_| closed));
    let bulges = bulges.iter().chain(std::iter::repeat(&0.0));
    for ((start, end), bulge) in points.iter().zip(ends).zip(bulges) {
        if *bulge == 0.0 {
            continue;
        }
        let Some((center, radius, start_deg, end_deg)) = bulge_arc(*start, *end, *bulge) else {
            continue;
        };
        bbox = bbox.union(&arc_bbox(center, radius, start_deg, end_deg));
    }
    Some(bbox)
}

impl Entity {
    pub fn display_type(&self) -> &str {
        match &self.kind {
            EntityKind::Unknown { type_name } => type_name.as_str().unwrap_or("UNKNOWN"),
            kind => kind.type_name(),
        }
    }

    pub fn bbox(&self) -> Option<BBox> {
        match &self.kind {
            EntityKind::Line { start, end } => points_bbox(&[*start, *end]),
            EntityKind::Circle { center, radius } => Some(arc_bbox(*center, *radius, 0.0, 0.0)),
            EntityKind::Arc {
                center,
                radius,
                start,
                end,
            } => Some(arc_bbox(*center, *radius, *start, *end)),
            EntityKind::Ellipse { center, major, .. } => {
                let reach = major.x.hypot(major.y);
                points_bbox(&[
                    Point::new(center.x - reach, center.y - reach, center.z),
                    Point::new(center.x + reach, center.y + reach, center.z),
                ])
            }
            EntityKind::Polyline {
                points,
                bulges,
                closed,
                ..
            } => polyline_bbox(points, bulges, *closed),
            EntityKind::Spline { control, .. } => points_bbox(control),
            EntityKind::Point { position } => points_bbox(&[*position]),
            EntityKind::Solid { points } => points_bbox(points),
            EntityKind::Image { position, size, .. } => points_bbox(&[
                *position,
                Point::new(position.x + size.x, position.y + size.y, position.z),
            ]),
            EntityKind::Viewport {
                center,
                width,
                height,
                ..
            } => points_bbox(&[
                Point::new(center.x - width / 2.0, center.y - height / 2.0, center.z),
                Point::new(center.x + width / 2.0, center.y + height / 2.0, center.z),
            ]),
            EntityKind::Hatch { .. }
            | EntityKind::Text { .. }
            | EntityKind::MText { .. }
            | EntityKind::Dimension { .. }
            | EntityKind::Leader { .. }
            | EntityKind::Insert { .. }
            | EntityKind::Unknown { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadboss_core::Point;

    fn entity(kind: EntityKind) -> Entity {
        Entity {
            common: EntityCommon {
                handle: None,
                layer: "0".into(),
                layout: "Model".into(),
                color: 256,
                hyperlink: None,
            },
            kind,
        }
    }

    #[test]
    fn line_bbox_spans_both_ends() {
        let bbox = entity(EntityKind::Line {
            start: Point::xy(3.0, -1.0),
            end: Point::xy(-2.0, 4.0),
        })
        .bbox()
        .unwrap();
        assert_eq!(bbox.min, Point::xy(-2.0, -1.0));
        assert_eq!(bbox.max, Point::xy(3.0, 4.0));
    }

    #[test]
    fn arc_bbox_covers_the_quarter_turn() {
        let bbox = entity(EntityKind::Arc {
            center: Point::xy(0.0, 0.0),
            radius: 2.0,
            start: 0.0,
            end: 90.0,
        })
        .bbox()
        .unwrap();
        assert!((bbox.max.x - 2.0).abs() < 1e-9 && (bbox.max.y - 2.0).abs() < 1e-9);
        assert!(bbox.min.x.abs() < 1e-9 && bbox.min.y.abs() < 1e-9);
    }

    #[test]
    fn bulged_polyline_bbox_includes_the_arc() {
        let bbox = entity(EntityKind::Polyline {
            points: vec![Point::xy(0.0, 0.0), Point::xy(2.0, 0.0)],
            bulges: vec![1.0, 0.0],
            closed: false,
            lightweight: true,
        })
        .bbox()
        .unwrap();
        assert!((bbox.min.y + 1.0).abs() < 1e-9);
        assert!(bbox.max.y.abs() < 1e-9);
    }

    #[test]
    fn text_like_kinds_have_no_bbox_and_unknown_keeps_its_name() {
        let unknown = entity(EntityKind::Unknown {
            type_name: "ACAD_PROXY".into(),
        });
        assert!(unknown.bbox().is_none());
        assert_eq!(unknown.kind.type_name(), "UNKNOWN");
        assert_eq!(unknown.display_type(), "ACAD_PROXY");
        let line = entity(EntityKind::Line {
            start: Point::default(),
            end: Point::default(),
        });
        assert_eq!(line.display_type(), "LINE");
    }

    #[test]
    fn polyline_type_names_follow_the_record_kind() {
        let polyline = |lightweight: bool| EntityKind::Polyline {
            points: vec![],
            bulges: vec![],
            closed: false,
            lightweight,
        };
        assert_eq!(polyline(true).type_name(), "LWPOLYLINE");
        assert_eq!(polyline(false).type_name(), "POLYLINE");
    }
}
