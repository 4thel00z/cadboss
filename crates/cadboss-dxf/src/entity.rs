//! On-demand decoding of entity records into model entities.

use std::f64::consts::TAU;

use cadboss_core::{Budget, Point, Report, SkipKind, SkipReason};
use cadboss_model::{Attribute, Entity, EntityCommon, EntityKind, Text};

use crate::codepage::{decode, decode_unescaped, Codepage};
use crate::mtext::strip_mtext;
use crate::record::{first, pairs_until, without_groups, Pair};
use crate::tape::Cursor;

const BY_LAYER: i16 = 256;
const CLOSED: i64 = 1;
const DIMENSION_TYPE_MASK: i64 = 0x0F;
const POLYFACE_FACE: i64 = 128;
const POLYFACE_VERTEX: i64 = 64;
const DEFAULT_SPLINE_DEGREE: u16 = 3;

type LayoutOf<'t> = &'t dyn Fn(Option<u64>, bool) -> Text;

struct Child<'t> {
    name: &'t [u8],
    pairs: Vec<Pair<'t>>,
}

/// Yields one entity per code-0 record of the cursor range, consuming the VERTEX or ATTRIB records and SEQEND that belong to it.
/// Strings are decoded first and then cut to `Budget::max_string_bytes` on a character boundary.
pub struct EntityDecoder<'t> {
    cursor: Cursor<'t>,
    cp: Codepage,
    budget: &'t Budget,
    layout_of: LayoutOf<'t>,
    decoded: u64,
    report: Report,
    done: bool,
    layout_filter: Option<Text>,
}

impl<'t> EntityDecoder<'t> {
    pub fn new(
        cursor: Cursor<'t>,
        cp: Codepage,
        budget: &'t Budget,
        layout_of: LayoutOf<'t>,
    ) -> EntityDecoder<'t> {
        EntityDecoder {
            cursor,
            cp,
            budget,
            layout_of,
            decoded: 0,
            report: Report::default(),
            done: false,
            layout_filter: None,
        }
    }

    /// Skips entities whose resolved layout differs from `name`; skipped entities do not count toward the budget.
    pub fn only_layout(mut self, name: Text) -> EntityDecoder<'t> {
        self.layout_filter = Some(name);
        self
    }

    pub fn report(&self) -> &Report {
        &self.report
    }

    pub fn into_report(self) -> Report {
        self.report
    }

    fn decode_record(&mut self, name: &'t [u8]) -> Option<Entity> {
        let head = without_groups(pairs_until(&mut self.cursor, 0));
        let children = self.children_of(name, &head);
        let mut fields = Fields {
            cursor: &self.cursor,
            pairs: &head,
            cp: self.cp,
            max_string: usize::try_from(self.budget.max_string_bytes).unwrap_or(usize::MAX),
            report: &mut self.report,
        };
        if let Some(filter) = &self.layout_filter {
            let layout = (self.layout_of)(fields.handle(330), fields.int(67) == Some(1));
            if layout != *filter {
                return None;
            }
        }
        let Some(kind) = build(&mut fields, &children, name) else {
            fields.report.note(SkipKind::Entity, SkipReason::Malformed);
            return None;
        };
        let common = fields.common(self.layout_of);
        if matches!(kind, EntityKind::Unknown { .. }) {
            fields.report.note(SkipKind::Entity, SkipReason::Unknown);
        }
        Some(Entity { common, kind })
    }

    fn children_of(&mut self, name: &[u8], head: &[Pair<'t>]) -> Vec<Child<'t>> {
        if !self.has_children(name, head) {
            return Vec::new();
        }
        self.children()
    }

    fn has_children(&self, name: &[u8], head: &[Pair<'t>]) -> bool {
        if name == b"POLYLINE" {
            return true;
        }
        if name != b"INSERT" {
            return false;
        }
        let announced = first(head, 66).and_then(|value| self.cursor.i64(value)) == Some(1);
        announced || matches!(self.cursor.peek(), Some((0, b"ATTRIB")))
    }

    fn children(&mut self) -> Vec<Child<'t>> {
        let mut children = Vec::new();
        while let Some((0, name)) = self.cursor.peek() {
            if !matches!(name, b"VERTEX" | b"ATTRIB" | b"SEQEND") {
                break;
            }
            self.cursor.next();
            let pairs = without_groups(pairs_until(&mut self.cursor, 0));
            if name == b"SEQEND" {
                break;
            }
            children.push(Child { name, pairs });
        }
        children
    }
}

impl<'t> Iterator for EntityDecoder<'t> {
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        while !self.done {
            self.cursor.skip_to_code_zero();
            let (_, name) = self.cursor.next()?;
            if matches!(name, b"VERTEX" | b"ATTRIB" | b"SEQEND") {
                self.report.note(SkipKind::Entity, SkipReason::Malformed);
                continue;
            }
            if self.decoded >= self.budget.max_entities {
                self.report.note(SkipKind::Entity, SkipReason::EntityLimit);
                self.done = true;
                return None;
            }
            let Some(entity) = self.decode_record(name) else {
                continue;
            };
            self.decoded += 1;
            return Some(entity);
        }
        None
    }
}

struct Fields<'a, 't> {
    cursor: &'a Cursor<'t>,
    pairs: &'a [Pair<'t>],
    cp: Codepage,
    max_string: usize,
    report: &'a mut Report,
}

impl<'a, 't> Fields<'a, 't> {
    fn over<'b>(&'b mut self, pairs: &'b [Pair<'t>]) -> Fields<'b, 't> {
        Fields {
            cursor: self.cursor,
            pairs,
            cp: self.cp,
            max_string: self.max_string,
            report: &mut *self.report,
        }
    }

    fn raw(&self, code: i16) -> Option<&'t [u8]> {
        first(self.pairs, code)
    }

    fn float(&self, code: i16) -> Option<f64> {
        self.cursor.f64(self.raw(code)?)
    }

    fn int(&self, code: i16) -> Option<i64> {
        self.cursor.i64(self.raw(code)?)
    }

    fn closed(&self) -> bool {
        self.int(70).is_some_and(|flags| flags & CLOSED != 0)
    }

    fn handle(&self, code: i16) -> Option<u64> {
        self.cursor.handle(self.raw(code)?)
    }

    fn point(&self, code: i16) -> Option<Point> {
        let x = self.float(code)?;
        let y = self.float(code + 10)?;
        Some(Point::new(x, y, self.float(code + 20).unwrap_or(0.0)))
    }

    fn points(&mut self, code: i16) -> Vec<Point> {
        let mut points: Vec<Point> = Vec::new();
        let mut skipping = false;
        for &(candidate, value) in self.pairs {
            let Some(offset) = candidate
                .checked_sub(code)
                .filter(|offset| matches!(offset, 0 | 10 | 20))
            else {
                continue;
            };
            match (offset, self.cursor.f64(value)) {
                (0, Some(x)) => {
                    skipping = false;
                    points.push(Point::new(x, 0.0, 0.0));
                }
                (0, None) => {
                    skipping = true;
                    self.report.note(SkipKind::Entity, SkipReason::Malformed);
                }
                (_, _) if skipping => {}
                (10, Some(y)) => {
                    if let Some(last) = points.last_mut() {
                        last.y = y;
                    }
                }
                (_, Some(z)) => {
                    if let Some(last) = points.last_mut() {
                        last.z = z;
                    }
                }
                (_, None) => self.report.note(SkipKind::Entity, SkipReason::Malformed),
            }
        }
        points
    }

    fn cut_len(&self, raw: &[u8]) -> usize {
        if raw.len() <= self.max_string || self.cp != Codepage::Utf8 {
            return raw.len().min(self.max_string);
        }
        (0..=self.max_string)
            .rev()
            .find(|at| raw.get(*at).is_none_or(|byte| byte & 0xC0 != 0x80))
            .unwrap_or(0)
    }

    fn finish(&mut self, text: Text, cut: bool) -> Text {
        let (text, cut) = match text {
            Text::Str(mut text) => {
                let excess = text.len() > self.max_string;
                if excess {
                    let at = (0..=self.max_string)
                        .rev()
                        .find(|at| text.is_char_boundary(*at))
                        .unwrap_or(0);
                    text.truncate(at);
                }
                text.shrink_to_fit();
                (Text::Str(text), cut || excess)
            }
            Text::Bytes(mut bytes) => {
                let excess = bytes.len() > self.max_string;
                bytes.truncate(self.max_string);
                bytes.shrink_to_fit();
                (Text::Bytes(bytes), cut || excess)
            }
        };
        if cut {
            self.report.note(SkipKind::Text, SkipReason::StringLimit);
        }
        text
    }

    fn string(&mut self, raw: &[u8], unescape: bool) -> Text {
        let keep = self.cut_len(raw);
        let cut = keep < raw.len();
        let raw = raw.get(..keep).unwrap_or(raw);
        let text = if unescape {
            decode_unescaped(self.cp, raw, self.report)
        } else {
            decode(self.cp, raw, self.report)
        };
        self.finish(text, cut)
    }

    fn text(&mut self, code: i16) -> Option<Text> {
        let raw = self.raw(code)?;
        Some(self.string(raw, false))
    }

    fn text_unescaped(&mut self, code: i16) -> Option<Text> {
        let raw = self.raw(code)?;
        Some(self.string(raw, true))
    }

    fn mtext(&mut self) -> Text {
        let mut joined: Vec<u8> = Vec::new();
        let mut cut = false;
        let chunks = self
            .pairs
            .iter()
            .filter(|(code, _)| *code == 3)
            .chain(self.pairs.iter().filter(|(code, _)| *code == 1));
        for (_, value) in chunks {
            let room = self.max_string.saturating_sub(joined.len());
            cut |= value.len() > room;
            joined.extend_from_slice(value.get(..room.min(value.len())).unwrap_or(&[]));
        }
        if cut && self.cp == Codepage::Utf8 {
            if let Err(error) = std::str::from_utf8(&joined) {
                if error.error_len().is_none() {
                    joined.truncate(error.valid_up_to());
                }
            }
        }
        let text = match decode(self.cp, &joined, self.report) {
            Text::Str(text) => Text::Str(strip_mtext(&text)),
            other => other,
        };
        self.finish(text, cut)
    }

    fn common(&mut self, layout_of: LayoutOf) -> EntityCommon {
        let owner = self.handle(330);
        let paper = self.int(67) == Some(1);
        EntityCommon {
            handle: self.handle(5),
            layer: self.text(8).unwrap_or_else(|| Text::from("0")),
            layout: layout_of(owner, paper),
            color: self
                .int(62)
                .and_then(|color| i16::try_from(color).ok())
                .unwrap_or(BY_LAYER),
        }
    }
}

fn build<'t>(
    fields: &mut Fields<'_, 't>,
    children: &[Child<'t>],
    name: &[u8],
) -> Option<EntityKind> {
    match name {
        b"LINE" => line(fields),
        b"CIRCLE" => circle(fields),
        b"ARC" => arc(fields),
        b"ELLIPSE" => ellipse(fields),
        b"POINT" => point(fields),
        b"SOLID" => solid(fields),
        b"LEADER" => Some(leader(fields)),
        b"LWPOLYLINE" => Some(lwpolyline(fields)),
        b"POLYLINE" => Some(polyline(fields, children)),
        b"SPLINE" => Some(spline(fields)),
        b"HATCH" => Some(hatch(fields)),
        b"TEXT" => text(fields),
        b"MTEXT" => mtext(fields),
        b"DIMENSION" => Some(dimension(fields)),
        b"INSERT" => insert(fields, children),
        b"IMAGE" => image(fields),
        b"VIEWPORT" => viewport(fields),
        other => Some(unknown(fields, other)),
    }
}

fn unknown(fields: &mut Fields, name: &[u8]) -> EntityKind {
    EntityKind::Unknown {
        type_name: fields.string(name, false),
    }
}

fn line(fields: &Fields) -> Option<EntityKind> {
    Some(EntityKind::Line {
        start: fields.point(10)?,
        end: fields.point(11)?,
    })
}

fn circle(fields: &Fields) -> Option<EntityKind> {
    Some(EntityKind::Circle {
        center: fields.point(10)?,
        radius: fields.float(40)?,
    })
}

fn arc(fields: &Fields) -> Option<EntityKind> {
    Some(EntityKind::Arc {
        center: fields.point(10)?,
        radius: fields.float(40)?,
        start: fields.float(50)?,
        end: fields.float(51)?,
    })
}

fn ellipse(fields: &Fields) -> Option<EntityKind> {
    Some(EntityKind::Ellipse {
        center: fields.point(10)?,
        major: fields.point(11)?,
        ratio: fields.float(40)?,
        start: fields.float(41).unwrap_or(0.0),
        end: fields.float(42).unwrap_or(TAU),
    })
}

fn point(fields: &Fields) -> Option<EntityKind> {
    Some(EntityKind::Point {
        position: fields.point(10)?,
    })
}

fn solid(fields: &Fields) -> Option<EntityKind> {
    let corners = [10, 11, 12, 13].map(|code| fields.point(code));
    let [Some(a), Some(b), Some(c), fourth] = corners else {
        return None;
    };
    let points = [Some(a), Some(b), Some(c), fourth]
        .into_iter()
        .flatten()
        .collect();
    Some(EntityKind::Solid { points })
}

fn leader(fields: &mut Fields) -> EntityKind {
    EntityKind::Leader {
        text: None,
        points: fields.points(10),
    }
}

fn lwpolyline(fields: &mut Fields) -> EntityKind {
    let mut points: Vec<Point> = Vec::new();
    let mut bulges: Vec<f64> = Vec::new();
    let mut skipping = false;
    for &(code, value) in fields.pairs {
        match (code, fields.cursor.f64(value)) {
            (10, Some(x)) => {
                skipping = false;
                points.push(Point::new(x, 0.0, 0.0));
                bulges.push(0.0);
            }
            (10, None) => {
                skipping = true;
                fields.report.note(SkipKind::Entity, SkipReason::Malformed);
            }
            (20 | 42, _) if skipping => {}
            (20, Some(y)) => {
                if let Some(last) = points.last_mut() {
                    last.y = y;
                }
            }
            (42, Some(bulge)) => {
                if let Some(last) = bulges.last_mut() {
                    *last = bulge;
                }
            }
            (20 | 42, None) => fields.report.note(SkipKind::Entity, SkipReason::Malformed),
            _ => {}
        }
    }
    EntityKind::Polyline {
        points,
        bulges,
        closed: fields.closed(),
    }
}

fn polyline<'t>(fields: &mut Fields<'_, 't>, children: &[Child<'t>]) -> EntityKind {
    let mut points = Vec::new();
    let mut bulges = Vec::new();
    for child in children.iter().filter(|child| child.name == b"VERTEX") {
        let vertex = fields.over(&child.pairs);
        let flags = vertex.int(70).unwrap_or(0);
        if flags & POLYFACE_FACE != 0 && flags & POLYFACE_VERTEX == 0 {
            continue;
        }
        let Some(point) = vertex.point(10) else {
            continue;
        };
        points.push(point);
        bulges.push(vertex.float(42).unwrap_or(0.0));
    }
    EntityKind::Polyline {
        points,
        bulges,
        closed: fields.closed(),
    }
}

fn spline(fields: &mut Fields) -> EntityKind {
    EntityKind::Spline {
        control: fields.points(10),
        fit: fields.points(11),
        degree: fields
            .int(71)
            .and_then(|degree| u16::try_from(degree).ok())
            .unwrap_or(DEFAULT_SPLINE_DEGREE),
        closed: fields.closed(),
    }
}

fn hatch(fields: &mut Fields) -> EntityKind {
    EntityKind::Hatch {
        pattern: fields.text(2).unwrap_or_default(),
        solid: fields.int(70) == Some(1),
        loops: fields
            .int(91)
            .and_then(|loops| u32::try_from(loops).ok())
            .unwrap_or(0),
    }
}

fn text(fields: &mut Fields) -> Option<EntityKind> {
    let position = fields.point(10)?;
    Some(EntityKind::Text {
        text: fields.text_unescaped(1).unwrap_or_default(),
        position,
        height: fields.float(40).unwrap_or(0.0),
        rotation: fields.float(50).unwrap_or(0.0),
        style: fields.text(7).unwrap_or_else(|| Text::from("Standard")),
    })
}

fn mtext(fields: &mut Fields) -> Option<EntityKind> {
    let position = fields.point(10)?;
    Some(EntityKind::MText {
        text: fields.mtext(),
        position,
        height: fields.float(40).unwrap_or(0.0),
        width: fields.float(41).unwrap_or(0.0),
        rotation: fields.float(50).unwrap_or(0.0),
        style: fields.text(7).unwrap_or_else(|| Text::from("Standard")),
    })
}

fn dimension(fields: &mut Fields) -> EntityKind {
    EntityKind::Dimension {
        text: fields.text_unescaped(1).unwrap_or_default(),
        measurement: fields.float(42),
        dim_type: fields
            .int(70)
            .and_then(|kind| u16::try_from(kind & DIMENSION_TYPE_MASK).ok())
            .unwrap_or(0),
        block_name: fields.text(2).unwrap_or_default(),
        style: fields.text(3).unwrap_or_default(),
    }
}

fn insert<'t>(fields: &mut Fields<'_, 't>, children: &[Child<'t>]) -> Option<EntityKind> {
    let block_name = fields.text(2)?;
    let position = fields.point(10)?;
    let mut attributes = Vec::new();
    for child in children.iter().filter(|child| child.name == b"ATTRIB") {
        let mut attrib = fields.over(&child.pairs);
        let Some(tag) = attrib.text(2) else {
            attrib.report.note(SkipKind::Entity, SkipReason::Malformed);
            continue;
        };
        attributes.push(Attribute {
            tag,
            value: attrib.text_unescaped(1).unwrap_or_default(),
            position: attrib.point(10).unwrap_or_default(),
            height: attrib.float(40).unwrap_or(0.0),
        });
    }
    Some(EntityKind::Insert {
        block_name,
        position,
        scale: Point::new(
            fields.float(41).unwrap_or(1.0),
            fields.float(42).unwrap_or(1.0),
            fields.float(43).unwrap_or(1.0),
        ),
        rotation: fields.float(50).unwrap_or(0.0),
        attributes,
    })
}

fn image(fields: &Fields) -> Option<EntityKind> {
    let size = fields.point(13)?;
    Some(EntityKind::Image {
        path: None,
        image_def: fields.handle(340),
        position: fields.point(10)?,
        size: Point::new(size.x, size.y, 0.0),
    })
}

fn viewport(fields: &Fields) -> Option<EntityKind> {
    let frozen_layers = fields
        .pairs
        .iter()
        .filter(|(code, _)| *code == 331)
        .filter_map(|(_, value)| fields.cursor.handle(value))
        .map(|handle| Text::from(format!("{handle:X}")))
        .collect();
    Some(EntityKind::Viewport {
        center: fields.point(10)?,
        width: fields.float(40)?,
        height: fields.float(41)?,
        view_center: fields.point(12).unwrap_or_default(),
        view_height: fields.float(45).unwrap_or(0.0),
        frozen_layers,
    })
}
