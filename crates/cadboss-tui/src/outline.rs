//! The tree the outline pane shows and search runs over, with the index from a row to its source record.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;

use cadboss_core::Report;
use cadboss_dxf::{decode, Codepage, DxfDocument, EntityDecoder, SectionName};
use cadboss_model::{
    Block, Document, Entity, EntityKind, Layer, Layout, LayoutKind, Text, TextGroups, TextItem,
};
use cadboss_output::one_line;

/// Index of the file row.
pub const ROOT: usize = 0;
/// Entity rows a type group materializes per `Outline::load_page`.
pub const PAGE: usize = 500;
/// Characters of entity text an entity row label keeps, the ellipsis included.
const TEXT_CHARS: usize = 60;

type Tally = Vec<(String, u64)>;

/// The layout or block whose entities a type group counts, by position in the document's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Layout(usize),
    Block(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    TitleBlock,
    Annotations,
    Names,
    Attributes,
    References,
    Properties,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::TitleBlock,
        Group::Annotations,
        Group::Names,
        Group::Attributes,
        Group::References,
        Group::Properties,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Group::TitleBlock => "title block",
            Group::Annotations => "annotations",
            Group::Names => "names",
            Group::Attributes => "attributes",
            Group::References => "references",
            Group::Properties => "properties",
        }
    }

    /// The group's name as `cadboss text` prints it in its section headings.
    pub fn cli_name(self) -> &'static str {
        match self {
            Group::TitleBlock => "title-block",
            Group::Annotations => "annotations",
            Group::Names => "names",
            Group::Attributes => "attributes",
            Group::References => "references",
            Group::Properties => "properties",
        }
    }

    pub fn items(self, groups: &TextGroups) -> &[TextItem] {
        match self {
            Group::TitleBlock => &groups.title_block,
            Group::Annotations => &groups.annotations,
            Group::Names => &groups.names,
            Group::Attributes => &groups.attributes,
            Group::References => &groups.references,
            Group::Properties => &groups.properties,
        }
    }
}

/// What a row stands for; indices point into the outline's `document` lists, `entities` or `rows`.
#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    File,
    LayoutsFolder,
    Layout(usize),
    BlocksFolder,
    Block(usize),
    LayersFolder,
    Layer(usize),
    TextFolder,
    TextGroup(Group),
    TextItem(Group, usize),
    TypeGroup {
        owner: Owner,
        type_name: String,
        total: u64,
        shown: u64,
    },
    Entity {
        slot: usize,
    },
    More {
        group: usize,
    },
}

/// A materialized entity and the tokens of its record, children included.
#[derive(Clone, Debug)]
pub struct Located {
    pub tokens: Range<u32>,
    pub entity: Entity,
}

/// One line of the tree; `detail` is the right-aligned muted column.
#[derive(Clone, Debug)]
pub struct Row {
    pub kind: RowKind,
    pub label: String,
    pub detail: String,
    pub depth: u8,
    pub parent: usize,
    pub children: Vec<usize>,
}

impl Row {
    fn new(kind: RowKind, label: String, detail: String) -> Row {
        Row {
            kind,
            label,
            detail,
            depth: 0,
            parent: ROOT,
            children: Vec::new(),
        }
    }
}

/// Every row, the file row first; `haystacks[i]` is what search matches against for `rows[i]`.
#[derive(Clone, Debug)]
pub struct Outline {
    pub rows: Vec<Row>,
    pub haystacks: Vec<String>,
    pub document: Document,
    pub entities: Vec<Located>,
    pub handles: HashMap<u64, Range<u32>>,
    pub layer_records: HashMap<Vec<u8>, Range<u32>>,
    pub layer_counts: HashMap<Vec<u8>, u64>,
    pub loaded: bool,
}

impl Outline {
    /// The file row alone, over the header's metadata, while `Outline::build` runs.
    pub fn pending(doc: &DxfDocument, title: &str) -> Outline {
        let document = Document {
            metadata: doc.header().metadata(),
            layers: Vec::new(),
            blocks: Vec::new(),
            layouts: Vec::new(),
            text: TextGroups::default(),
            report: doc.report().clone(),
        };
        let mut outline = Outline {
            rows: Vec::new(),
            haystacks: Vec::new(),
            document,
            entities: Vec::new(),
            handles: HashMap::new(),
            layer_records: HashMap::new(),
            layer_counts: HashMap::new(),
            loaded: false,
        };
        Tree {
            rows: &mut outline.rows,
            haystacks: &mut outline.haystacks,
        }
        .add(ROOT, RowKind::File, title.to_owned(), String::new());
        outline
    }

    /// Every row except entity rows: one located pass per layout and block body counts entity types and layers and indexes handles.
    /// A block whose body holds a paper layout takes that layout's counts instead of being read again.
    pub fn build(doc: &DxfDocument, title: &str) -> Outline {
        let document = doc.document();
        let mut survey = Survey::default();
        let layout_tallies: Vec<Tally> = (0..document.layouts.len())
            .map(|position| survey.tally(decoder(doc, &document, Owner::Layout(position))))
            .collect();
        let bodies: HashMap<usize, usize> = document
            .layouts
            .iter()
            .enumerate()
            .filter_map(|(position, layout)| Some((body_block(doc, layout)?, position)))
            .collect();
        let block_tallies: Vec<Tally> = (0..document.blocks.len())
            .map(|index| {
                let body = bodies
                    .get(&index)
                    .and_then(|position| layout_tallies.get(*position));
                body.cloned()
                    .unwrap_or_else(|| survey.tally(decoder(doc, &document, Owner::Block(index))))
            })
            .collect();
        let mut outline = Outline {
            rows: Vec::new(),
            haystacks: Vec::new(),
            document,
            entities: Vec::new(),
            handles: survey.handles,
            layer_records: layer_records(doc),
            layer_counts: survey.layer_counts,
            loaded: true,
        };
        let mut tree = Tree {
            rows: &mut outline.rows,
            haystacks: &mut outline.haystacks,
        };
        tree.add(ROOT, RowKind::File, title.to_owned(), String::new());
        tree.layouts(&outline.document.layouts, &layout_tallies);
        tree.blocks(&outline.document.blocks, &block_tallies);
        tree.layers(&outline.document.layers, &outline.layer_counts);
        tree.text(&outline.document.text);
        outline
    }

    /// Decodes the owner's entities again and adds the next `PAGE` of the group's type as entity rows.
    /// The trailing `More` row becomes the first new entity row, and a new `More` row follows while entities remain.
    pub fn load_page(&mut self, doc: &DxfDocument, group: usize) {
        let Some(RowKind::TypeGroup {
            owner,
            type_name,
            total,
            shown,
        }) = self.rows.get(group).map(|row| row.kind.clone())
        else {
            return;
        };
        if shown >= total {
            return;
        }
        let Some(mut decoder) = decoder(doc, &self.document, owner) else {
            return;
        };
        let page: Vec<(Range<u32>, Entity)> = std::iter::from_fn(|| decoder.next_located())
            .filter(|(_, entity)| entity.display_type() == type_name)
            .skip(usize::try_from(shown).unwrap_or(usize::MAX))
            .take(PAGE)
            .collect();
        let shown = shown.saturating_add(page.len() as u64);
        let mut more = self
            .rows
            .get(group)
            .and_then(|row| row.children.last().copied())
            .filter(|last| {
                matches!(
                    self.rows.get(*last).map(|row| &row.kind),
                    Some(RowKind::More { .. })
                )
            });
        let mut tree = Tree {
            rows: &mut self.rows,
            haystacks: &mut self.haystacks,
        };
        for (tokens, entity) in page {
            let row = Row::new(
                RowKind::Entity {
                    slot: self.entities.len(),
                },
                entity_label(&entity),
                hex(entity.common.handle),
            );
            let haystack = haystack(
                &row,
                entity_text(&entity),
                Some(&entity.common.layer),
                entity.common.handle,
            );
            self.entities.push(Located { tokens, entity });
            if let Some(index) = more.take() {
                tree.replace(index, row, haystack);
                continue;
            }
            tree.attach(group, row, haystack);
        }
        if let Some(RowKind::TypeGroup { shown: count, .. }) =
            tree.rows.get_mut(group).map(|row| &mut row.kind)
        {
            *count = shown;
        }
        if shown >= total || more.is_some() {
            return;
        }
        tree.add(
            group,
            RowKind::More { group },
            format!("… {} more", total - shown),
            String::new(),
        );
    }

    /// Tokens of the row's source record: the HEADER section, a BLOCK header, a LAYER record, an entity row's own record or a text item's record by handle.
    pub fn record(&self, doc: &DxfDocument, row: usize) -> Option<Range<u32>> {
        match &self.rows.get(row)?.kind {
            RowKind::File => doc
                .tape()
                .section(SectionName::Header)
                .map(|section| section.start..section.end),
            RowKind::Layout(position) => {
                let layout = self.document.layouts.get(*position)?;
                Some(doc.block_def(layout.block_name.as_str()?)?.header.clone())
            }
            RowKind::Block(index) => {
                let block = self.document.blocks.get(*index)?;
                Some(doc.block_def(block.name.as_str()?)?.header.clone())
            }
            RowKind::Layer(index) => {
                let layer = self.document.layers.get(*index)?;
                self.layer_records.get(layer.name.as_bytes()).cloned()
            }
            RowKind::TextItem(group, index) => {
                let item = group.items(&self.document.text).get(*index)?;
                self.handles.get(&item.handle?).cloned()
            }
            RowKind::Entity { slot } => self
                .entities
                .get(*slot)
                .map(|located| located.tokens.clone()),
            RowKind::LayoutsFolder
            | RowKind::BlocksFolder
            | RowKind::LayersFolder
            | RowKind::TextFolder
            | RowKind::TextGroup(_)
            | RowKind::TypeGroup { .. }
            | RowKind::More { .. } => None,
        }
    }

    /// The row and every row above it, the file row last.
    pub fn ancestors(&self, row: usize) -> Vec<usize> {
        std::iter::successors(Some(row), |index| {
            self.rows
                .get(*index)
                .map(|above| above.parent)
                .filter(|parent| parent < index)
        })
        .collect()
    }

    /// Whether the row is its parent's last child; the file row counts as last.
    pub fn is_last_child(&self, row: usize) -> bool {
        if row == ROOT {
            return true;
        }
        let Some(parent) = self.rows.get(row).map(|below| below.parent) else {
            return true;
        };
        self.rows
            .get(parent)
            .and_then(|above| above.children.last())
            .is_none_or(|last| *last == row)
    }
}

/// What search matches against: the label, layer, hex handle and text that are present, newline-joined and lowercased.
pub fn haystack(
    row: &Row,
    text: Option<&Text>,
    layer: Option<&Text>,
    handle: Option<u64>,
) -> String {
    [
        Some(row.label.clone()),
        layer.map(Text::to_string),
        Some(hex(handle)),
        text.map(Text::to_string),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
    .to_lowercase()
}

/// Rows and haystacks borrowed together, so rows can be added while the document is read.
struct Tree<'o> {
    rows: &'o mut Vec<Row>,
    haystacks: &'o mut Vec<String>,
}

impl Tree<'_> {
    fn attach(&mut self, parent: usize, mut row: Row, haystack: String) -> usize {
        let index = self.rows.len();
        row.parent = parent;
        row.depth = self
            .rows
            .get(parent)
            .map_or(0, |above| above.depth.saturating_add(1));
        self.rows.push(row);
        self.haystacks.push(haystack);
        if index == ROOT {
            return index;
        }
        if let Some(above) = self.rows.get_mut(parent) {
            above.children.push(index);
        }
        index
    }

    fn add(&mut self, parent: usize, kind: RowKind, label: String, detail: String) -> usize {
        let row = Row::new(kind, label, detail);
        let haystack = haystack(&row, None, None, None);
        self.attach(parent, row, haystack)
    }

    /// Puts `row` at `index`, keeping that row's place in the tree.
    fn replace(&mut self, index: usize, row: Row, haystack: String) {
        let (Some(slot), Some(hay)) = (self.rows.get_mut(index), self.haystacks.get_mut(index))
        else {
            return;
        };
        *slot = Row {
            parent: slot.parent,
            depth: slot.depth,
            ..row
        };
        *hay = haystack;
    }

    fn folder(&mut self, kind: RowKind, label: &str, children: usize) -> usize {
        self.add(ROOT, kind, label.to_owned(), children.to_string())
    }

    fn type_groups(&mut self, parent: usize, owner: Owner, tally: &Tally) {
        for (type_name, total) in tally {
            let kind = RowKind::TypeGroup {
                owner,
                type_name: type_name.clone(),
                total: *total,
                shown: 0,
            };
            self.add(parent, kind, type_name.clone(), total.to_string());
        }
    }

    fn layouts(&mut self, layouts: &[Layout], tallies: &[Tally]) {
        let folder = self.folder(RowKind::LayoutsFolder, "Layouts", layouts.len());
        for (position, (layout, tally)) in layouts.iter().zip(tallies).enumerate() {
            let detail = entity_count(tally.iter().map(|(_, count)| count).sum());
            let row = self.add(
                folder,
                RowKind::Layout(position),
                label(&layout.name),
                detail,
            );
            self.type_groups(row, Owner::Layout(position), tally);
        }
    }

    fn blocks(&mut self, blocks: &[Block], tallies: &[Tally]) {
        let folder = self.folder(RowKind::BlocksFolder, "Blocks", blocks.len());
        for (index, (block, tally)) in blocks.iter().zip(tallies).enumerate() {
            let detail = block.xref_path.as_ref().map_or_else(
                || entity_count(tally.iter().map(|(_, count)| count).sum()),
                Text::to_string,
            );
            let row = self.add(folder, RowKind::Block(index), label(&block.name), detail);
            self.type_groups(row, Owner::Block(index), tally);
        }
    }

    fn layers(&mut self, layers: &[Layer], counts: &HashMap<Vec<u8>, u64>) {
        let folder = self.folder(RowKind::LayersFolder, "Layers", layers.len());
        for (index, layer) in layers.iter().enumerate() {
            let count = counts.get(layer.name.as_bytes()).copied().unwrap_or(0);
            let flags = [
                (layer.frozen, " · frozen"),
                (layer.off, " · off"),
                (layer.locked, " · locked"),
            ]
            .into_iter()
            .filter(|(set, _)| *set)
            .map(|(_, flag)| flag);
            let detail = std::iter::once(entity_count(count).as_str())
                .chain(flags)
                .collect();
            self.add(folder, RowKind::Layer(index), label(&layer.name), detail);
        }
    }

    fn text(&mut self, groups: &TextGroups) {
        let folder = self.folder(RowKind::TextFolder, "Text", Group::ALL.len());
        for group in Group::ALL {
            let items = group.items(groups);
            let parent = self.add(
                folder,
                RowKind::TextGroup(group),
                group.label().to_owned(),
                items.len().to_string(),
            );
            for (index, item) in items.iter().enumerate() {
                let row = Row::new(
                    RowKind::TextItem(group, index),
                    label(&item.text),
                    item.layout.to_string(),
                );
                let haystack = haystack(&row, Some(&item.text), Some(&item.layer), item.handle);
                self.attach(parent, row, haystack);
            }
        }
    }
}

/// Counts and handles gathered by the located pass over every layout and block body.
#[derive(Default)]
struct Survey {
    handles: HashMap<u64, Range<u32>>,
    layer_counts: HashMap<Vec<u8>, u64>,
}

impl Survey {
    /// Entity-type counts of one owner, by count descending then name.
    fn tally(&mut self, decoder: Option<EntityDecoder<'_>>) -> Tally {
        let Some(mut decoder) = decoder else {
            return Vec::new();
        };
        let mut counts: HashMap<String, u64> = HashMap::new();
        while let Some((tokens, entity)) = decoder.next_located() {
            increment(&mut counts, entity.display_type());
            increment(&mut self.layer_counts, entity.common.layer.as_bytes());
            let Some(handle) = entity.common.handle else {
                continue;
            };
            self.handles.entry(handle).or_insert(tokens);
        }
        let mut tally: Tally = counts.into_iter().collect();
        tally.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        tally
    }
}

fn increment<K, Q>(counts: &mut HashMap<K, u64>, key: &Q)
where
    K: Borrow<Q> + Eq + Hash,
    Q: ToOwned<Owned = K> + Eq + Hash + ?Sized,
{
    if let Some(total) = counts.get_mut(key) {
        *total += 1;
        return;
    }
    counts.insert(key.to_owned(), 1);
}

fn decoder<'d>(
    doc: &'d DxfDocument,
    document: &Document,
    owner: Owner,
) -> Option<EntityDecoder<'d>> {
    match owner {
        Owner::Layout(position) => {
            Some(doc.entities(document.layouts.get(position)?.name.as_str()?))
        }
        Owner::Block(index) => doc.block_entities(document.blocks.get(index)?.name.as_str()?),
    }
}

/// Position in `blocks()` of the block whose non-empty body `DxfDocument::entities` reads for a paper layout.
fn body_block(doc: &DxfDocument, layout: &Layout) -> Option<usize> {
    if layout.kind != LayoutKind::Paper {
        return None;
    }
    let name = layout.block_name.as_str()?;
    doc.block_def(name).filter(|def| !def.entities.is_empty())?;
    doc.block_index(name)
}

/// LAYER records of the TABLES section keyed by their name, decoded with the document's codepage as `Layer::name` is.
fn layer_records(doc: &DxfDocument) -> HashMap<Vec<u8>, Range<u32>> {
    let mut records = HashMap::new();
    let tape = doc.tape();
    let Some(tables) = tape.section(SectionName::Tables) else {
        return records;
    };
    let header = doc.header();
    let cp = Codepage::from_header(
        header.version.as_bytes(),
        header.codepage.as_ref().map(Text::as_bytes),
    );
    let mut open: Option<(u32, Option<Vec<u8>>)> = None;
    for (index, code) in tape.pairs(tables.start..tables.end) {
        if code == 0 {
            close(&mut records, open.take(), index);
            open = (tape.value(index, doc.bytes()) == b"LAYER").then_some((index, None));
            continue;
        }
        if code != 2 {
            continue;
        }
        let Some((_, name)) = open.as_mut() else {
            continue;
        };
        if name.is_some() {
            continue;
        }
        let value = tape.value(index, doc.bytes());
        *name = Some(
            decode(cp, value, &mut Report::default())
                .as_bytes()
                .to_vec(),
        );
    }
    close(&mut records, open, tables.end);
    records
}

fn close(
    records: &mut HashMap<Vec<u8>, Range<u32>>,
    open: Option<(u32, Option<Vec<u8>>)>,
    end: u32,
) {
    let Some((start, Some(name))) = open else {
        return;
    };
    records.entry(name).or_insert(start..end);
}

fn label(text: &Text) -> String {
    one_line(&text.to_string())
}

fn entity_count(count: u64) -> String {
    format!("{count} entities")
}

fn hex(handle: Option<u64>) -> String {
    handle
        .map(|handle| format!("{handle:X}"))
        .unwrap_or_default()
}

/// The text an entity row shows: TEXT, MTEXT, DIMENSION and LEADER text, the INSERT block name or the IMAGE path.
fn entity_text(entity: &Entity) -> Option<&Text> {
    match &entity.kind {
        EntityKind::Text { text, .. }
        | EntityKind::MText { text, .. }
        | EntityKind::Dimension { text, .. } => Some(text),
        EntityKind::Leader { text, .. } => text.as_ref(),
        EntityKind::Insert { block_name, .. } => Some(block_name),
        EntityKind::Image { path, .. } => path.as_ref(),
        _ => None,
    }
}

fn entity_label(entity: &Entity) -> String {
    let head = format!("{}  {}", entity.display_type(), entity.common.layer);
    let Some(text) = entity_text(entity).filter(|text| !text.is_empty()) else {
        return head;
    };
    let text = label(text);
    if text.chars().count() <= TEXT_CHARS {
        return format!("{head}  {text}");
    }
    let cut: String = text.chars().take(TEXT_CHARS - 1).collect();
    format!("{head}  {cut}…")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use cadboss_testkit::dxf::DxfBuilder;

    use super::*;

    fn demo() -> DxfDocument {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/demo.dxf");
        DxfDocument::open(Path::new(path)).unwrap()
    }

    fn labels(outline: &Outline, row: usize) -> Vec<&str> {
        outline.rows[row]
            .children
            .iter()
            .map(|child| outline.rows[*child].label.as_str())
            .collect()
    }

    fn details(outline: &Outline, row: usize) -> Vec<&str> {
        outline.rows[row]
            .children
            .iter()
            .map(|child| outline.rows[*child].detail.as_str())
            .collect()
    }

    fn child(outline: &Outline, row: usize, label: &str) -> usize {
        *outline.rows[row]
            .children
            .iter()
            .find(|child| outline.rows[**child].label == label)
            .unwrap()
    }

    fn model(outline: &Outline) -> usize {
        child(outline, child(outline, ROOT, "Layouts"), "Model")
    }

    fn page(outline: &Outline, group: usize) -> (usize, Vec<&str>) {
        let kinds = outline.rows[group]
            .children
            .iter()
            .map(|child| &outline.rows[*child]);
        let entities = kinds
            .clone()
            .filter(|row| matches!(row.kind, RowKind::Entity { .. }))
            .count();
        let more = kinds
            .filter(|row| matches!(row.kind, RowKind::More { .. }))
            .map(|row| row.label.as_str())
            .collect();
        (entities, more)
    }

    fn shown(outline: &Outline, group: usize) -> u64 {
        let shown = match outline.rows[group].kind {
            RowKind::TypeGroup { shown, .. } => Some(shown),
            _ => None,
        };
        shown.unwrap()
    }

    #[test]
    fn pending_outline_has_only_the_file_row() {
        let outline = Outline::pending(&demo(), "demo.dxf");
        assert_eq!(outline.rows.len(), 1);
        assert_eq!(outline.haystacks, ["demo.dxf"]);
        assert_eq!(outline.rows[ROOT].kind, RowKind::File);
        assert_eq!(outline.rows[ROOT].label, "demo.dxf");
        assert!(outline.rows[ROOT].children.is_empty());
        assert!(!outline.loaded);
        assert!(outline.entities.is_empty());
        assert_eq!(outline.document.metadata.version, Text::from("AC1032"));
    }

    #[test]
    fn build_lists_layouts_blocks_layers_and_text_groups_in_order() {
        let outline = Outline::build(&demo(), "demo.dxf");
        assert!(outline.loaded);
        assert!(outline.entities.is_empty());
        assert_eq!(outline.haystacks.len(), outline.rows.len());
        assert_eq!(
            labels(&outline, ROOT),
            ["Layouts", "Blocks", "Layers", "Text"]
        );
        assert_eq!(details(&outline, ROOT), ["3", "6", "7", "6"]);

        let layouts = child(&outline, ROOT, "Layouts");
        assert_eq!(labels(&outline, layouts), ["Model", "Layout1", "Sheet 1"]);
        assert_eq!(
            details(&outline, layouts),
            ["14 entities", "0 entities", "1 entities"]
        );
        let model = child(&outline, layouts, "Model");
        assert_eq!(outline.rows[model].kind, RowKind::Layout(0));
        assert_eq!(outline.rows[model].depth, 2);

        let blocks = child(&outline, ROOT, "Blocks");
        assert_eq!(
            labels(&outline, blocks),
            [
                "*Model_Space",
                "*Paper_Space",
                "*Paper_Space0",
                "PUMP",
                "TITLEBLOCK",
                "SITE-PLAN"
            ]
        );
        assert_eq!(
            details(&outline, blocks),
            [
                "0 entities",
                "0 entities",
                "1 entities",
                "3 entities",
                "8 entities",
                "../xrefs/site-plan.dxf"
            ]
        );
        let pump = child(&outline, blocks, "PUMP");
        assert_eq!(labels(&outline, pump), ["CIRCLE", "LINE", "TEXT"]);

        let layers = child(&outline, ROOT, "Layers");
        assert_eq!(
            labels(&outline, layers),
            ["0", "WALLS", "EQUIPMENT", "PIPING", "DIM", "NOTES", "TITLE"]
        );
        assert_eq!(
            details(&outline, layers),
            [
                "11 entities",
                "2 entities",
                "2 entities",
                "3 entities",
                "3 entities",
                "4 entities",
                "1 entities"
            ]
        );

        let text = child(&outline, ROOT, "Text");
        let groups: Vec<&str> = Group::ALL.iter().map(|group| group.label()).collect();
        assert_eq!(labels(&outline, text), groups);
        assert_eq!(details(&outline, text), ["4", "7", "10", "11", "3", "0"]);
        let attributes = child(&outline, text, "attributes");
        assert_eq!(
            outline.rows[attributes].kind,
            RowKind::TextGroup(Group::Attributes)
        );
        assert_eq!(outline.rows[attributes].children.len(), 11);
        let first = outline.rows[attributes].children[0];
        assert_eq!(
            outline.rows[first].kind,
            RowKind::TextItem(Group::Attributes, 0)
        );
        assert_eq!(outline.rows[first].label, "TAG=P-101");
        assert_eq!(outline.rows[first].detail, "Model");
        assert_eq!(outline.rows[first].depth, 3);

        for (index, row) in outline.rows.iter().enumerate() {
            assert!(row
                .children
                .iter()
                .all(|child| outline.rows[*child].parent == index));
        }
        assert_eq!(outline.ancestors(first), [first, attributes, text, ROOT]);
        assert!(outline.is_last_child(ROOT));
        assert!(outline.is_last_child(text));
        assert!(!outline.is_last_child(layouts));
    }

    #[test]
    fn type_groups_are_sorted_by_count_then_name() {
        let outline = Outline::build(&demo(), "demo.dxf");
        let model = model(&outline);
        assert_eq!(
            labels(&outline, model),
            [
                "DIMENSION",
                "INSERT",
                "LINE",
                "ARC",
                "IMAGE",
                "LWPOLYLINE",
                "MTEXT",
                "TEXT"
            ]
        );
        assert_eq!(
            details(&outline, model),
            ["3", "3", "3", "1", "1", "1", "1", "1"]
        );
        let first = outline.rows[model].children[0];
        assert_eq!(
            outline.rows[first].kind,
            RowKind::TypeGroup {
                owner: Owner::Layout(0),
                type_name: "DIMENSION".to_owned(),
                total: 3,
                shown: 0,
            }
        );
        assert!(outline.rows[first].children.is_empty());
    }

    #[test]
    fn type_group_materializes_one_page() {
        let bytes = (0..1203)
            .fold(DxfBuilder::new(), |builder, step| {
                let y = f64::from(step);
                builder.line((0.0, y), (1.0, y))
            })
            .build();
        let doc = DxfDocument::load(bytes).unwrap();
        let mut outline = Outline::build(&doc, "lines.dxf");
        let model = model(&outline);
        let group = child(&outline, model, "LINE");
        assert_eq!(outline.rows[group].detail, "1203");
        assert_eq!(page(&outline, group), (0, vec![]));

        outline.load_page(&doc, group);
        assert_eq!(page(&outline, group), (500, vec!["… 703 more"]));
        assert_eq!(shown(&outline, group), 500);
        assert_eq!(outline.entities.len(), 500);
        assert_eq!(outline.haystacks.len(), outline.rows.len());
        let more = *outline.rows[group].children.last().unwrap();
        assert_eq!(outline.rows[more].kind, RowKind::More { group });
        assert_eq!(outline.rows[more].depth, 4);

        outline.load_page(&doc, group);
        assert_eq!(page(&outline, group), (1000, vec!["… 203 more"]));
        assert_eq!(shown(&outline, group), 1000);
        assert_eq!(outline.entities.len(), 1000);
        assert_eq!(outline.haystacks.len(), outline.rows.len());
        assert_eq!(outline.rows[group].children[500], more);
        assert_eq!(outline.rows[more].kind, RowKind::Entity { slot: 500 });

        outline.load_page(&doc, group);
        assert_eq!(page(&outline, group), (1203, vec![]));
        assert_eq!(shown(&outline, group), 1203);
        assert_eq!(outline.entities.len(), 1203);
        assert_eq!(outline.haystacks.len(), outline.rows.len());

        let rows = outline.rows.len();
        outline.load_page(&doc, group);
        assert_eq!(outline.rows.len(), rows);
        let slots: Vec<usize> = outline.rows[group]
            .children
            .iter()
            .filter_map(|child| match outline.rows[*child].kind {
                RowKind::Entity { slot } => Some(slot),
                _ => None,
            })
            .collect();
        assert_eq!(slots, (0..1203).collect::<Vec<_>>());
    }

    #[test]
    fn entity_rows_show_type_layer_and_text_cut_at_sixty_characters() {
        let doc = demo();
        let mut outline = Outline::build(&doc, "demo.dxf");
        let model = model(&outline);
        let mtext = child(&outline, model, "MTEXT");
        outline.load_page(&doc, mtext);
        let row = &outline.rows[outline.rows[mtext].children[0]];
        assert_eq!(
            row.label,
            "MTEXT  NOTES  NOTES: 1. ALL DIMENSIONS IN MILLIMETRES. 2. PIPING TO PIPE …"
        );
        assert_eq!(row.detail, "144");
        let line = child(&outline, model, "LINE");
        outline.load_page(&doc, line);
        assert_eq!(
            labels(&outline, line),
            ["LINE  WALLS", "LINE  PIPING", "LINE  PIPING"]
        );
        assert_eq!(details(&outline, line), ["135", "13E", "140"]);
    }

    #[test]
    fn record_of_an_entity_row_is_its_own_tokens_without_a_handle() {
        let bytes: &[u8] = concat!(
            "  0\nSECTION\n  2\nENTITIES\n",
            "  0\nLINE\n  8\nWALLS\n 10\n0\n 20\n0\n 11\n1\n 21\n1\n",
            "  0\nLINE\n  8\nWALLS\n 10\n1\n 20\n1\n 11\n2\n 21\n2\n",
            "  0\nENDSEC\n  0\nEOF\n"
        )
        .as_bytes();
        let doc = DxfDocument::load(bytes).unwrap();
        let mut outline = Outline::build(&doc, "bare.dxf");
        assert!(outline.handles.is_empty());
        let line = child(&outline, model(&outline), "LINE");
        outline.load_page(&doc, line);
        assert_eq!(details(&outline, line), ["", ""]);
        let records: Vec<Range<u32>> = outline.rows[line]
            .children
            .iter()
            .map(|row| outline.record(&doc, *row).unwrap())
            .collect();
        let tokens: Vec<Range<u32>> = outline
            .entities
            .iter()
            .map(|located| located.tokens.clone())
            .collect();
        assert_eq!(records, tokens);
        assert_ne!(records[0], records[1]);
        assert!(records
            .iter()
            .all(|record| doc.tape().value(record.start, doc.bytes()) == b"LINE"));
    }

    #[test]
    fn record_of_a_text_item_is_its_entity_record() {
        let doc = demo();
        let outline = Outline::build(&doc, "demo.dxf");
        let text = child(&outline, ROOT, "Text");
        let attributes = child(&outline, text, "attributes");
        let item = child(&outline, attributes, "TAG=P-101");
        let tokens = outline.record(&doc, item).unwrap();
        assert_eq!(doc.tape().code(tokens.start), 0);
        assert_eq!(doc.tape().value(tokens.start, doc.bytes()), b"INSERT");
        assert_eq!(doc.tape().value(tokens.start + 1, doc.bytes()), b"136");
        let names = child(&outline, text, "names");
        let walls = child(&outline, names, "WALLS");
        assert_eq!(outline.record(&doc, walls), None);
    }

    #[test]
    fn record_of_an_entity_layout_or_block_row_is_its_record() {
        let doc = demo();
        let mut outline = Outline::build(&doc, "demo.dxf");
        let tape = doc.tape();
        let layouts = child(&outline, ROOT, "Layouts");
        let model = child(&outline, layouts, "Model");
        let insert = child(&outline, model, "INSERT");
        outline.load_page(&doc, insert);
        let first = outline.rows[insert].children[0];
        let tokens = outline.record(&doc, first).unwrap();
        assert_eq!(tokens, outline.entities[0].tokens);
        assert_eq!(tape.value(tokens.start, doc.bytes()), b"INSERT");

        let sheet = child(&outline, layouts, "Sheet 1");
        let blocks = child(&outline, ROOT, "Blocks");
        let paper = child(&outline, blocks, "*Paper_Space0");
        assert_eq!(
            outline.record(&doc, sheet),
            Some(doc.block_def("*Paper_Space0").unwrap().header.clone())
        );
        assert_eq!(outline.record(&doc, paper), outline.record(&doc, sheet));
        let pump = child(&outline, blocks, "PUMP");
        let header = outline.record(&doc, pump).unwrap();
        assert_eq!(tape.value(header.start, doc.bytes()), b"BLOCK");

        assert_eq!(outline.record(&doc, layouts), None);
        assert_eq!(outline.record(&doc, insert), None);
        assert_eq!(outline.record(&doc, outline.rows.len()), None);
    }

    #[test]
    fn record_of_a_layer_is_its_layer_table_record() {
        let doc = demo();
        let outline = Outline::build(&doc, "demo.dxf");
        let tape = doc.tape();
        let layers = child(&outline, ROOT, "Layers");
        let walls = child(&outline, layers, "WALLS");
        let tokens = outline.record(&doc, walls).unwrap();
        assert_eq!(tape.code(tokens.start), 0);
        assert_eq!(tape.value(tokens.start, doc.bytes()), b"LAYER");
        assert_eq!(tape.code(tokens.end), 0);
        assert!(tokens
            .clone()
            .any(|index| tape.code(index) == 2 && tape.value(index, doc.bytes()) == b"WALLS"));
        assert_eq!(outline.layer_records.len(), 7);
    }

    #[test]
    fn record_of_the_file_is_the_header_section() {
        let doc = demo();
        let section = doc.tape().section(SectionName::Header).unwrap();
        let header = section.start..section.end;
        assert_eq!(doc.tape().value(header.start, doc.bytes()), b"$ACADVER");
        let outline = Outline::build(&doc, "demo.dxf");
        assert_eq!(outline.record(&doc, ROOT), Some(header.clone()));
        let pending = Outline::pending(&doc, "demo.dxf");
        assert_eq!(pending.record(&doc, ROOT), Some(header));
    }

    #[test]
    fn haystacks_hold_label_layer_text_and_hex_handle_lowercased() {
        let doc = demo();
        let mut outline = Outline::build(&doc, "demo.dxf");
        let text = child(&outline, ROOT, "Text");
        let attributes = child(&outline, text, "attributes");
        let item = child(&outline, attributes, "TAG=P-101");
        assert_eq!(
            outline.haystacks[item],
            "tag=p-101\nequipment\n136\ntag=p-101"
        );

        let annotations = child(&outline, text, "annotations");
        let notes = outline.rows[annotations]
            .children
            .iter()
            .copied()
            .find(|row| outline.rows[*row].label.starts_with("NOTES:"))
            .unwrap();
        assert!(!outline.rows[notes].label.contains('\n'));
        assert_eq!(
            outline.haystacks[notes],
            concat!(
                "notes: 1. all dimensions in millimetres. 2. piping to pipe class 1a. ",
                "3. p-102 is the standby unit, see datasheet § 4.\nnotes\n144\nnotes:\n",
                "1. all dimensions in millimetres.\n2. piping to pipe class 1a.\n",
                "3. p-102 is the standby unit, see datasheet § 4."
            )
        );

        let model = model(&outline);
        assert_eq!(outline.haystacks[model], "model");
        let insert = child(&outline, model, "INSERT");
        outline.load_page(&doc, insert);
        let first = outline.rows[insert].children[0];
        assert_eq!(outline.rows[first].label, "INSERT  EQUIPMENT  PUMP");
        assert_eq!(
            outline.haystacks[first],
            "insert  equipment  pump\nequipment\n136\npump"
        );

        let row = Row {
            kind: RowKind::File,
            label: "Plan".to_owned(),
            detail: String::new(),
            depth: 0,
            parent: ROOT,
            children: Vec::new(),
        };
        let bytes = Text::Bytes(vec![0xff, 0x41]);
        let layer = Text::from("Walls");
        assert_eq!(
            haystack(&row, Some(&bytes), Some(&layer), Some(0x1F)),
            "plan\nwalls\n1f\n[non-decodable text, 2 bytes]"
        );
        assert_eq!(haystack(&row, None, None, None), "plan");
    }
}
