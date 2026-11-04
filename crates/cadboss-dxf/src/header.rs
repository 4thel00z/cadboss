//! HEADER section variables and the document metadata derived from them.

use cadboss_core::{BBox, Point, Report, SkipKind, SkipReason};
use cadboss_model::{Format, Metadata, Text, Units};

use crate::record::Record;
use crate::tape::{Cursor, SectionName, Tape};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Header {
    pub version: Text,
    pub codepage: Option<Text>,
    pub insunits: u16,
    pub extmin: Option<Point>,
    pub extmax: Option<Point>,
    pub tdcreate: Option<f64>,
    pub tdupdate: Option<f64>,
    pub lastsavedby: Option<Text>,
}

impl Header {
    pub fn metadata(&self) -> Metadata {
        let mut metadata = Metadata::new(Format::Dxf, self.version.clone());
        metadata.codepage = self.codepage.clone();
        metadata.units = Units::from_insunits(self.insunits);
        metadata.author = self.lastsavedby.clone();
        metadata.created = self.tdcreate;
        metadata.updated = self.tdupdate;
        metadata.extents = self.extents();
        metadata
    }

    fn extents(&self) -> Option<BBox> {
        let extents = BBox {
            min: self.extmin?,
            max: self.extmax?,
        };
        Some(extents).filter(|extents| !extents.is_empty())
    }
}

pub fn read_header(tape: &Tape, bytes: &[u8], report: &mut Report) -> Header {
    let mut header = Header::default();
    let Some(section) = tape.section(SectionName::Header) else {
        return header;
    };
    let mut cursor = Cursor::new(tape, bytes, section.start..section.end);
    while let Some((code, name)) = cursor.next() {
        if code != 9 {
            continue;
        }
        let values = Record::until(&mut cursor, 9);
        if !apply(&mut header, name, &values) {
            report.note(SkipKind::Table, SkipReason::Malformed);
        }
    }
    header
}

fn apply(header: &mut Header, name: &[u8], values: &Record) -> bool {
    match name {
        b"$ACADVER" => {
            let Some(value) = values.first(1) else {
                return false;
            };
            header.version = Text::from_utf8_lossy_or_bytes(value);
        }
        b"$DWGCODEPAGE" => {
            let Some(value) = values.first(3) else {
                return false;
            };
            header.codepage = Some(Text::from_utf8_lossy_or_bytes(value));
        }
        b"$LASTSAVEDBY" => {
            let Some(value) = values.first(1) else {
                return false;
            };
            header.lastsavedby = Some(Text::from_utf8_lossy_or_bytes(value));
        }
        b"$INSUNITS" => {
            let Some(units) = values
                .first(70)
                .and_then(|value| values.i64(value))
                .and_then(|value| u16::try_from(value).ok())
            else {
                return false;
            };
            header.insunits = units;
        }
        b"$TDCREATE" => {
            let Some(day) = values.first(40).and_then(|value| values.f64(value)) else {
                return false;
            };
            header.tdcreate = Some(day);
        }
        b"$TDUPDATE" => {
            let Some(day) = values.first(40).and_then(|value| values.f64(value)) else {
                return false;
            };
            header.tdupdate = Some(day);
        }
        b"$EXTMIN" => {
            let Some(point) = point(values) else {
                return false;
            };
            header.extmin = Some(point);
        }
        b"$EXTMAX" => {
            let Some(point) = point(values) else {
                return false;
            };
            header.extmax = Some(point);
        }
        _ => {}
    }
    true
}

fn point(values: &Record) -> Option<Point> {
    let x = values.f64(values.first(10)?)?;
    let y = values.f64(values.first(20)?)?;
    let Some(z) = values.first(30) else {
        return Some(Point::new(x, y, 0.0));
    };
    Some(Point::new(x, y, values.f64(z)?))
}
