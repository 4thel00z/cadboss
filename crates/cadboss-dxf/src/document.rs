//! The opened DXF document: tables, blocks, layouts and entity access.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use cadboss_core::{BBox, Budget, Point, Report, Result, SkipKind, SkipReason, Source, Transform};
use cadboss_model::{Block, Entity, EntityKind, Layer, Layout, LayoutKind, Text, TextGroups};

use crate::blocks::{read_blocks, BlockDef};
use crate::codepage::{decode, Codepage};
use crate::entity::EntityDecoder;
use crate::header::{read_header, Header};
use crate::layouts::read_layouts;
use crate::record::{first, pairs_until, without_groups};
use crate::tables::{read_block_records, read_layers, BlockRecord};
use crate::tape::{Cursor, SectionName, Tape};

type LayoutOf = Box<dyn Fn(Option<u64>, bool) -> Text + Send + Sync>;

#[derive(Debug, Clone)]
pub struct Options {
    pub budget: Budget,
    pub threads: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            budget: Budget::default(),
            threads: 1,
        }
    }
}

pub struct DxfDocument {
    source: Source,
    tape: Tape,
    header: Header,
    cp: Codepage,
    budget: Budget,
    layers: Vec<Layer>,
    blocks: Vec<BlockDef>,
    block_lookup: HashMap<String, usize>,
    layouts: Vec<Layout>,
    layout_of: LayoutOf,
    block_layouts: Vec<LayoutOf>,
    report: Report,
}

impl std::fmt::Debug for DxfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DxfDocument")
            .field("source", &self.source)
            .field("layers", &self.layers.len())
            .field("blocks", &self.blocks.len())
            .field("layouts", &self.layouts.len())
            .finish()
    }
}

fn text_bytes(text: &Text) -> &[u8] {
    match text {
        Text::Str(text) => text.as_bytes(),
        Text::Bytes(bytes) => bytes,
    }
}

fn constant(text: Text) -> LayoutOf {
    Box::new(move |_, _| text.clone())
}

fn layout_resolver(
    block_records: &[BlockRecord],
    layouts: &[Layout],
    layout_names: &HashMap<String, Text>,
) -> LayoutOf {
    let owners: HashMap<u64, Text> = block_records
        .iter()
        .filter(|record| record.handle != 0)
        .filter_map(|record| {
            let name = layout_names.get(&record.name.as_str()?.to_ascii_lowercase())?;
            Some((record.handle, name.clone()))
        })
        .collect();
    let model = layouts
        .iter()
        .find(|layout| layout.kind == LayoutKind::Model)
        .map_or_else(|| Text::from("Model"), |layout| layout.name.clone());
    let paper = layouts
        .iter()
        .find(|layout| {
            layout
                .block_name
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case("*Paper_Space"))
        })
        .or_else(|| {
            layouts
                .iter()
                .find(|layout| layout.kind == LayoutKind::Paper)
        })
        .map_or_else(|| Text::from("Layout1"), |layout| layout.name.clone());
    Box::new(move |owner, paper_flag| {
        if let Some(name) = owner.and_then(|handle| owners.get(&handle)) {
            return name.clone();
        }
        if paper_flag {
            return paper.clone();
        }
        model.clone()
    })
}

impl DxfDocument {
    fn assemble(source: Source, options: &Options) -> Result<DxfDocument> {
        let bytes = source.bytes();
        let (tape, mut report) = Tape::lex(bytes, &options.budget)?;
        let header = read_header(&tape, bytes, &mut report);
        let cp = Codepage::from_header(
            text_bytes(&header.version),
            header.codepage.as_ref().map(text_bytes),
        );
        let layers = read_layers(&tape, bytes, cp, &mut report);
        let block_records = read_block_records(&tape, bytes, cp, &mut report);
        let blocks = read_blocks(&tape, bytes, cp, &mut report);
        let layouts = read_layouts(
            &tape,
            bytes,
            cp,
            &block_records,
            &options.budget,
            &mut report,
        );
        let mut block_lookup = HashMap::new();
        for (index, def) in blocks.iter().enumerate() {
            let Some(name) = def.block.name.as_str() else {
                continue;
            };
            block_lookup
                .entry(name.to_ascii_lowercase())
                .or_insert(index);
        }
        let mut layout_names = HashMap::new();
        for layout in &layouts {
            let Some(block) = layout.block_name.as_str() else {
                continue;
            };
            layout_names
                .entry(block.to_ascii_lowercase())
                .or_insert_with(|| layout.name.clone());
        }
        let block_layouts = blocks
            .iter()
            .map(|def| {
                let owner = def
                    .block
                    .name
                    .as_str()
                    .and_then(|name| layout_names.get(&name.to_ascii_lowercase()));
                constant(owner.map_or_else(|| def.block.name.clone(), Text::clone))
            })
            .collect();
        Ok(DxfDocument {
            layout_of: layout_resolver(&block_records, &layouts, &layout_names),
            source,
            tape,
            header,
            cp,
            budget: options.budget,
            layers,
            blocks,
            block_lookup,
            layouts,
            block_layouts,
            report,
        })
    }

    fn reporting(document: DxfDocument) -> (DxfDocument, Report) {
        let report = document.report.clone();
        (document, report)
    }

    pub fn open(path: &Path) -> Result<DxfDocument> {
        DxfDocument::open_with_options(path, &Options::default())
    }

    pub fn open_with_options(path: &Path, options: &Options) -> Result<DxfDocument> {
        DxfDocument::assemble(Source::map(path)?, options)
    }

    pub fn open_reporting(path: &Path) -> Result<(DxfDocument, Report)> {
        DxfDocument::open_with_options_reporting(path, &Options::default())
    }

    pub fn open_with_options_reporting(
        path: &Path,
        options: &Options,
    ) -> Result<(DxfDocument, Report)> {
        DxfDocument::open_with_options(path, options).map(DxfDocument::reporting)
    }

    pub fn read(path: &Path) -> Result<DxfDocument> {
        DxfDocument::read_with_options(path, &Options::default())
    }

    pub fn read_with_options(path: &Path, options: &Options) -> Result<DxfDocument> {
        DxfDocument::assemble(Source::read(path)?, options)
    }

    pub fn read_reporting(path: &Path) -> Result<(DxfDocument, Report)> {
        DxfDocument::read_with_options_reporting(path, &Options::default())
    }

    pub fn read_with_options_reporting(
        path: &Path,
        options: &Options,
    ) -> Result<(DxfDocument, Report)> {
        DxfDocument::read_with_options(path, options).map(DxfDocument::reporting)
    }

    pub fn load(bytes: impl Into<Arc<[u8]>>) -> Result<DxfDocument> {
        DxfDocument::load_with_options(bytes, &Options::default())
    }

    pub fn load_with_options(
        bytes: impl Into<Arc<[u8]>>,
        options: &Options,
    ) -> Result<DxfDocument> {
        DxfDocument::assemble(Source::from_bytes(bytes), options)
    }

    pub fn load_reporting(bytes: impl Into<Arc<[u8]>>) -> Result<(DxfDocument, Report)> {
        DxfDocument::load_with_options_reporting(bytes, &Options::default())
    }

    pub fn load_with_options_reporting(
        bytes: impl Into<Arc<[u8]>>,
        options: &Options,
    ) -> Result<(DxfDocument, Report)> {
        DxfDocument::load_with_options(bytes, options).map(DxfDocument::reporting)
    }

    pub fn report(&self) -> &Report {
        &self.report
    }

    pub fn tape(&self) -> &Tape {
        &self.tape
    }

    pub fn bytes(&self) -> &[u8] {
        self.source.bytes()
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.blocks.iter().map(|def| &def.block)
    }

    pub fn layouts(&self) -> &[Layout] {
        &self.layouts
    }

    /// Case-insensitive position of the block named `name` among `blocks()`.
    pub fn block_index(&self, name: &str) -> Option<usize> {
        self.block_lookup.get(&name.to_ascii_lowercase()).copied()
    }

    fn decoder<'a>(&'a self, range: Range<u32>, layout_of: &'a LayoutOf) -> EntityDecoder<'a> {
        let cursor = Cursor::new(&self.tape, self.source.bytes(), range);
        EntityDecoder::new(cursor, self.cp, &self.budget, layout_of.as_ref())
    }

    fn block_decoder(&self, index: usize) -> Option<EntityDecoder<'_>> {
        let def = self.blocks.get(index)?;
        let layout_of = self.block_layouts.get(index)?;
        Some(self.decoder(def.entities.clone(), layout_of))
    }

    /// Entities of `layout`: a paper layout whose block has a non-empty body reads that body; otherwise the ENTITIES section filtered by owner.
    /// An unknown layout yields nothing.
    pub fn entities(&self, layout: &str) -> EntityDecoder<'_> {
        let Some(found) = self
            .layouts
            .iter()
            .find(|candidate| candidate.name.as_str() == Some(layout))
        else {
            return self.decoder(0..0, &self.layout_of);
        };
        let own_block = found
            .block_name
            .as_str()
            .and_then(|name| self.block_index(name))
            .filter(|index| {
                found.kind == LayoutKind::Paper
                    && self
                        .blocks
                        .get(*index)
                        .is_some_and(|def| !def.entities.is_empty())
            });
        if let Some(decoder) = own_block.and_then(|index| self.block_decoder(index)) {
            return decoder;
        }
        let range = self
            .tape
            .section(SectionName::Entities)
            .map_or(0..0, |section| section.start..section.end);
        self.decoder(range, &self.layout_of)
            .only_layout(found.name.clone())
    }

    /// Entities of one block body; `common.layout` is the layout name when the block belongs to a layout, otherwise the block name.
    pub fn block_entities(&self, block: &str) -> Option<EntityDecoder<'_>> {
        self.block_decoder(self.block_index(block)?)
    }

    fn layout_block(&self, layout: &str) -> Option<usize> {
        let found = self
            .layouts
            .iter()
            .find(|candidate| candidate.name.as_str() == Some(layout))?;
        self.block_index(found.block_name.as_str()?)
    }

    /// The returned report carries walk-level notes only; entity decode notes come from `entities` and `block_entities`.
    pub fn walk(&self, layout: &str, visitor: &mut dyn FnMut(&Entity, &Transform, u32)) -> Report {
        self.walk_with_path(layout, &mut |entity, transform, path| {
            visitor(entity, transform, path.len() as u32);
        })
    }

    /// Like `walk`, but the visitor receives the names of the blocks entered to reach the entity, outermost first, instead of the depth.
    pub fn walk_with_path(
        &self,
        layout: &str,
        visitor: &mut dyn FnMut(&Entity, &Transform, &[Text]),
    ) -> Report {
        let mut walker = Walker {
            document: self,
            visitor,
            report: Report::default(),
            names: Vec::new(),
            path: self.layout_block(layout).into_iter().collect(),
            nesting_noted: false,
            visited: 0,
            stopped: false,
        };
        walker.run(self.entities(layout), &Transform::identity(), 0);
        walker.report
    }

    pub fn text(&self) -> TextGroups {
        self.text_reporting().0
    }

    pub fn text_reporting(&self) -> (TextGroups, Report) {
        crate::textgroups::collect(self)
    }

    pub(crate) fn image_defs(&self, report: &mut Report) -> Vec<(u64, Text)> {
        let Some(section) = self.tape.section(SectionName::Objects) else {
            return Vec::new();
        };
        let mut cursor = Cursor::new(&self.tape, self.source.bytes(), section.start..section.end);
        let mut defs = Vec::new();
        while let Some((code, value)) = cursor.next() {
            if code != 0 || value != b"IMAGEDEF" {
                continue;
            }
            let pairs = without_groups(pairs_until(&mut cursor, 0));
            let handle = first(&pairs, 5).and_then(|value| cursor.handle(value));
            let (Some(handle), Some(path)) = (handle, first(&pairs, 1)) else {
                report.note(SkipKind::Object, SkipReason::Malformed);
                continue;
            };
            defs.push((handle, decode(self.cp, path, report)));
        }
        defs
    }

    pub fn layout_extents(&self, layout: &str) -> Option<BBox> {
        let mut extents = BBox::empty();
        self.walk(layout, &mut |entity, transform, _| {
            let bbox = match (&entity.kind, entity.bbox()) {
                (_, Some(bbox)) => bbox.transform(transform),
                (EntityKind::Text { position, .. } | EntityKind::MText { position, .. }, None) => {
                    point_bbox(transform.apply(*position))
                }
                _ => return,
            };
            extents = extents.union(&bbox);
        });
        Some(extents).filter(|extents| !extents.is_empty())
    }
}

fn point_bbox(point: Point) -> BBox {
    let mut bbox = BBox::empty();
    bbox.include(point);
    bbox
}

struct Walker<'d, 'v> {
    document: &'d DxfDocument,
    visitor: &'v mut dyn FnMut(&Entity, &Transform, &[Text]),
    report: Report,
    names: Vec<Text>,
    path: Vec<usize>,
    nesting_noted: bool,
    visited: u64,
    stopped: bool,
}

impl<'d> Walker<'d, '_> {
    fn run(&mut self, decoder: EntityDecoder<'d>, transform: &Transform, depth: u32) {
        for entity in decoder {
            if !self.stopped && self.visited >= self.document.budget.max_entities {
                self.report.note(SkipKind::Entity, SkipReason::EntityLimit);
                self.stopped = true;
            }
            if self.stopped {
                break;
            }
            self.visited += 1;
            (self.visitor)(&entity, transform, &self.names);
            self.expand(&entity, transform, depth);
        }
    }

    fn expand(&mut self, entity: &Entity, transform: &Transform, depth: u32) {
        let EntityKind::Insert {
            block_name,
            position,
            scale,
            rotation,
            ..
        } = &entity.kind
        else {
            return;
        };
        if self.stopped {
            return;
        }
        let Some(index) = block_name
            .as_str()
            .and_then(|name| self.document.block_index(name))
        else {
            self.report.note(SkipKind::Entity, SkipReason::Unknown);
            return;
        };
        if self.path.contains(&index) {
            self.report.note(SkipKind::Entity, SkipReason::Malformed);
            return;
        }
        if depth + 1 >= self.document.budget.max_nesting {
            if !self.nesting_noted {
                self.report.note(SkipKind::Entity, SkipReason::NestingLimit);
                self.nesting_noted = true;
            }
            return;
        }
        let Some(decoder) = self.document.block_decoder(index) else {
            return;
        };
        let base = self
            .document
            .blocks
            .get(index)
            .map_or_else(Point::default, |def| def.block.base);
        let child = Transform::translate(Point::new(-base.x, -base.y, -base.z))
            .then(&Transform::insert(*position, *scale, *rotation))
            .then(transform);
        self.path.push(index);
        self.names.push(block_name.clone());
        self.run(decoder, &child, depth + 1);
        self.names.pop();
        self.path.pop();
    }
}
