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
use crate::record::Record;
use crate::tables::{read_block_records, read_layers, BlockRecord};
use crate::tape::{Cursor, SectionName, Tape};

type LayoutOf = Box<dyn Fn(Option<u64>, bool) -> Text + Send + Sync>;

#[derive(Debug, Clone)]
pub struct Options {
    pub budget: Budget,
    /// Reserved for internal parallelism; currently unused.
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
    placement: Arc<Placement>,
    layout_of: LayoutOf,
    block_layouts: Vec<LayoutOf>,
    buckets: Vec<Vec<u32>>,
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

/// The layout an ENTITIES record belongs to, from its owner handle and paper-space flag.
struct Placement {
    owners: HashMap<u64, Text>,
    model: Text,
    paper: Text,
}

impl Placement {
    fn new(
        block_records: &[BlockRecord],
        layouts: &[Layout],
        layout_names: &HashMap<String, Text>,
    ) -> Placement {
        let owners = block_records
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
        Placement {
            owners,
            model,
            paper,
        }
    }

    fn layout(&self, owner: Option<u64>, paper: bool) -> &Text {
        if let Some(name) = owner.and_then(|handle| self.owners.get(&handle)) {
            return name;
        }
        if paper {
            return &self.paper;
        }
        &self.model
    }
}

/// What one walk over a layout found besides the visits.
pub(crate) struct LayoutPass {
    pub(crate) entity_count: u64,
    pub(crate) extents: Option<BBox>,
    pub(crate) decoded: Report,
    pub(crate) walked: Report,
}

fn constant(text: Text) -> LayoutOf {
    Box::new(move |_, _| text.clone())
}

impl DxfDocument {
    fn assemble(source: Source, options: &Options) -> Result<DxfDocument> {
        let bytes = source.bytes();
        let (tape, mut report) = Tape::lex(bytes, &options.budget)?;
        let header = read_header(&tape, bytes, &mut report);
        let cp = Codepage::from_header(
            header.version.as_bytes(),
            header.codepage.as_ref().map(Text::as_bytes),
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
        let placement = Arc::new(Placement::new(&block_records, &layouts, &layout_names));
        let resolver = Arc::clone(&placement);
        let mut document = DxfDocument {
            layout_of: Box::new(move |owner, paper| resolver.layout(owner, paper).clone()),
            placement,
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
            buckets: Vec::new(),
            report,
        };
        let (buckets, notes) = document.bucket_entities();
        document.buckets = buckets;
        document.report.merge(notes);
        Ok(document)
    }

    fn reporting(document: DxfDocument) -> (DxfDocument, Report) {
        let report = document.report.clone();
        (document, report)
    }

    pub fn open(path: &Path) -> Result<DxfDocument> {
        DxfDocument::open_with_options(path, &Options::default())
    }

    pub fn open_with_options(path: &Path, options: &Options) -> Result<DxfDocument> {
        DxfDocument::assemble(Source::map(path, &options.budget)?, options)
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
        DxfDocument::assemble(Source::read(path, &options.budget)?, options)
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

    pub fn load(bytes: impl Into<Vec<u8>>) -> Result<DxfDocument> {
        DxfDocument::load_with_options(bytes, &Options::default())
    }

    pub fn load_with_options(bytes: impl Into<Vec<u8>>, options: &Options) -> Result<DxfDocument> {
        DxfDocument::assemble(Source::from_bytes(bytes), options)
    }

    pub fn load_reporting(bytes: impl Into<Vec<u8>>) -> Result<(DxfDocument, Report)> {
        DxfDocument::load_with_options_reporting(bytes, &Options::default())
    }

    pub fn load_with_options_reporting(
        bytes: impl Into<Vec<u8>>,
        options: &Options,
    ) -> Result<(DxfDocument, Report)> {
        DxfDocument::load_with_options(bytes, options).map(DxfDocument::reporting)
    }

    /// Notes from opening the document: lexing, tables, blocks, layouts and the structure of the ENTITIES section.
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

    pub(crate) fn block_decoder(&self, index: usize) -> Option<EntityDecoder<'_>> {
        let def = self.blocks.get(index)?;
        let layout_of = self.block_layouts.get(index)?;
        Some(self.decoder(def.entities.clone(), layout_of))
    }

    fn entities_section(&self) -> Range<u32> {
        self.tape
            .section(SectionName::Entities)
            .map_or(0..0, |section| section.start..section.end)
    }

    /// Walks the ENTITIES section once and lists, per position in `layouts()`, the token index of each record that resolves to that layout.
    /// Stray VERTEX, ATTRIB and SEQEND records are noted once in the returned report.
    pub(crate) fn bucket_entities(&self) -> (Vec<Vec<u32>>, Report) {
        let mut positions: HashMap<&Text, usize> = HashMap::new();
        for (position, layout) in self.layouts.iter().enumerate() {
            positions.entry(&layout.name).or_insert(position);
        }
        let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); self.layouts.len()];
        let notes = self.decoder(self.entities_section(), &self.layout_of).skim(
            &mut |start, owner, paper| {
                let layout = self.placement.layout(owner, paper);
                let Some(bucket) = positions
                    .get(layout)
                    .and_then(|position| buckets.get_mut(*position))
                else {
                    return;
                };
                bucket.push(start);
            },
        );
        (buckets, notes)
    }

    /// Block whose body holds the entities of the layout at `position`: the block of a paper layout when that body is not empty.
    pub(crate) fn body_block(&self, position: usize) -> Option<usize> {
        let layout = self.layouts.get(position)?;
        if layout.kind != LayoutKind::Paper {
            return None;
        }
        let index = self.block_index(layout.block_name.as_str()?)?;
        self.blocks
            .get(index)
            .filter(|def| !def.entities.is_empty())?;
        Some(index)
    }

    /// Entities of `layout`: a paper layout whose block has a non-empty body reads that body; otherwise the ENTITIES records that resolve to the layout by owner.
    /// An unknown layout yields nothing.
    pub fn entities(&self, layout: &str) -> EntityDecoder<'_> {
        let Some(position) = self
            .layouts
            .iter()
            .position(|candidate| candidate.name.as_str() == Some(layout))
        else {
            return self.decoder(0..0, &self.layout_of);
        };
        if let Some(decoder) = self
            .body_block(position)
            .and_then(|index| self.block_decoder(index))
        {
            return decoder;
        }
        let starts = self.buckets.get(position).map_or(&[][..], Vec::as_slice);
        self.decoder(self.entities_section(), &self.layout_of)
            .at(starts)
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
        self.pass(layout, &mut |_| {}, visitor).walked
    }

    /// Walks `layout` once. `top` sees every top-level entity, also those past a walk-wide entity stop, so the count and the decode notes cover the whole layout;
    /// `visitor` sees every visited entity, and the extents cover the visited ones.
    pub(crate) fn pass(
        &self,
        layout: &str,
        top: &mut dyn FnMut(&Entity),
        visitor: &mut dyn FnMut(&Entity, &Transform, &[Text]),
    ) -> LayoutPass {
        let mut extents = BBox::empty();
        let mut entity_count = 0u64;
        let mut decoder = self.entities(layout);
        let mut counted = |entity: &Entity| {
            entity_count += 1;
            top(entity);
        };
        let mut measured = |entity: &Entity, transform: &Transform, path: &[Text]| {
            extents = extents.union(&extent(entity, transform));
            visitor(entity, transform, path);
        };
        let mut walker = Walker {
            document: self,
            visitor: &mut measured,
            top: &mut counted,
            report: Report::default(),
            names: Vec::new(),
            path: self.layout_block(layout).into_iter().collect(),
            nesting_noted: false,
            visited: 0,
            stopped: false,
        };
        walker.run(&mut decoder, &Transform::identity(), 0);
        let walked = walker.report;
        for entity in decoder.by_ref() {
            counted(&entity);
        }
        LayoutPass {
            entity_count,
            extents: Some(extents).filter(|extents| !extents.is_empty()),
            decoded: decoder.into_report(),
            walked,
        }
    }

    pub fn text(&self) -> TextGroups {
        self.text_reporting().0
    }

    /// The returned report carries text-level notes: walk notes, image definitions and layouts whose name is not text.
    pub fn text_reporting(&self) -> (TextGroups, Report) {
        let survey = crate::textgroups::survey(self, &mut |_| {});
        (survey.groups, survey.report)
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
            let record = Record::until(&mut cursor, 0);
            let handle = record.first(5).and_then(|value| record.handle(value));
            let (Some(handle), Some(path)) = (handle, record.first(1)) else {
                report.note(SkipKind::Object, SkipReason::Malformed);
                continue;
            };
            defs.push((handle, decode(self.cp, path, report)));
        }
        defs
    }

    /// Bounding box of every entity visited by `walk`, with text and MTEXT counted at their insertion point; `None` for an empty or unknown layout.
    pub fn layout_extents(&self, layout: &str) -> Option<BBox> {
        self.layout_extents_reporting(layout).0
    }

    /// `layout_extents` with the walk-level notes of its walk.
    pub fn layout_extents_reporting(&self, layout: &str) -> (Option<BBox>, Report) {
        let pass = self.pass(layout, &mut |_| {}, &mut |_, _, _| {});
        (pass.extents, pass.walked)
    }
}

fn extent(entity: &Entity, transform: &Transform) -> BBox {
    if let Some(bbox) = entity.bbox() {
        return bbox.transform(transform);
    }
    let (EntityKind::Text { position, .. } | EntityKind::MText { position, .. }) = &entity.kind
    else {
        return BBox::empty();
    };
    let mut bbox = BBox::empty();
    bbox.include(transform.apply(*position));
    bbox
}

struct Walker<'d, 'v> {
    document: &'d DxfDocument,
    visitor: &'v mut dyn FnMut(&Entity, &Transform, &[Text]),
    top: &'v mut dyn FnMut(&Entity),
    report: Report,
    names: Vec<Text>,
    path: Vec<usize>,
    nesting_noted: bool,
    visited: u64,
    stopped: bool,
}

impl<'d> Walker<'d, '_> {
    fn run(&mut self, decoder: &mut EntityDecoder<'d>, transform: &Transform, depth: u32) {
        for entity in decoder {
            if depth == 0 {
                (self.top)(&entity);
            }
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
        let Some(mut decoder) = self.document.block_decoder(index) else {
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
        self.run(&mut decoder, &child, depth + 1);
        self.names.pop();
        self.path.pop();
    }
}
