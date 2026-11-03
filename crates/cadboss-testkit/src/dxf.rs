//! Builders that write valid ASCII and binary DXF files for tests.

const SENTINEL: &[u8; 22] = b"AutoCAD Binary DXF\r\n\x1a\0";
const MODEL_SPACE: &str = "*Model_Space";
const PAPER_SPACE: &str = "*Paper_Space";
const ROOT_DICTIONARY: &str = "C";
const FIRST_HANDLE: u64 = 0x100;
const MTEXT_CHUNK: usize = 250;
const XREF_FLAGS: i64 = 36;

#[derive(Clone, Debug)]
enum Value {
    Str(String),
    Raw(Vec<u8>),
    F64(f64),
    I16(i16),
    I32(i32),
    I64(i64),
    Bool(bool),
}

type Record = (i16, Value);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Width {
    Str,
    F64,
    I16,
    I32,
    I64,
    Bool,
}

fn width(code: i16) -> Width {
    match code {
        10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059 => Width::F64,
        60..=79 | 170..=179 | 270..=289 | 370..=389 | 400..=409 | 1060..=1070 => Width::I16,
        90..=99 | 420..=429 | 440..=459 | 1071 => Width::I32,
        160..=169 => Width::I64,
        290..=299 => Width::Bool,
        _ => Width::Str,
    }
}

fn int_value(code: i16, n: i64) -> Value {
    match width(code) {
        Width::F64 => Value::F64(n as f64),
        Width::I16 => Value::I16(n as i16),
        Width::I32 => Value::I32(n as i32),
        Width::I64 => Value::I64(n),
        Width::Bool => Value::Bool(n != 0),
        Width::Str => Value::Str(n.to_string()),
    }
}

fn typed_value(code: i16, text: &str) -> Value {
    let text = text.trim();
    let parsed = match width(code) {
        Width::F64 => text.parse().map(Value::F64).ok(),
        Width::I16 => text.parse().map(Value::I16).ok(),
        Width::I32 => text.parse().map(Value::I32).ok(),
        Width::I64 => text.parse().map(Value::I64).ok(),
        Width::Bool => text.parse::<i64>().map(|n| Value::Bool(n != 0)).ok(),
        Width::Str => None,
    };
    parsed.unwrap_or_else(|| Value::Str(text.to_string()))
}

fn hex(handle: u64) -> String {
    format!("{handle:X}")
}

fn paper_block_name(index: usize) -> String {
    if index == 0 {
        return PAPER_SPACE.to_string();
    }
    format!("{PAPER_SPACE}{}", index - 1)
}

struct Out {
    records: Vec<Record>,
    next: u64,
    dimensions: u32,
    imagedefs: Vec<u64>,
}

impl Out {
    fn new() -> Out {
        Out {
            records: Vec::new(),
            next: FIRST_HANDLE,
            dimensions: 0,
            imagedefs: Vec::new(),
        }
    }

    fn alloc(&mut self) -> u64 {
        let handle = self.next;
        self.next += 1;
        handle
    }

    fn s(&mut self, code: i16, value: &str) {
        self.records.push((code, Value::Str(value.to_string())));
    }

    fn f(&mut self, code: i16, value: f64) {
        self.records.push((code, Value::F64(value)));
    }

    fn n(&mut self, code: i16, value: i64) {
        self.records.push((code, int_value(code, value)));
    }

    fn id(&mut self, code: i16, handle: u64) {
        self.s(code, &hex(handle));
    }

    fn point(&mut self, code: i16, x: f64, y: f64, z: f64) {
        self.f(code, x);
        self.f(code + 10, y);
        self.f(code + 20, z);
    }

    fn begin(&mut self, section: &str) {
        self.s(0, "SECTION");
        self.s(2, section);
    }

    fn end(&mut self) {
        self.s(0, "ENDSEC");
    }

    fn variable(&mut self, name: &str, code: i16, value: Value) {
        self.s(9, name);
        self.records.push((code, value));
    }

    fn extent(&mut self, name: &str, at: (f64, f64)) {
        self.s(9, name);
        self.point(10, at.0, at.1, 0.0);
    }
}

#[derive(Clone, Debug)]
enum Kind {
    Line {
        a: (f64, f64),
        b: (f64, f64),
    },
    Circle {
        center: (f64, f64),
        radius: f64,
    },
    Arc {
        center: (f64, f64),
        radius: f64,
        start: f64,
        end: f64,
    },
    Lwpolyline {
        points: Vec<(f64, f64, f64)>,
        closed: bool,
    },
    Text {
        value: Value,
        at: (f64, f64),
        height: f64,
    },
    Mtext {
        value: String,
        at: (f64, f64),
        height: f64,
    },
    Insert {
        block: String,
        at: (f64, f64),
        scale: (f64, f64, f64),
        rotation: f64,
        attribs: Vec<(String, String)>,
    },
    Dimension {
        text: String,
        measurement: Option<f64>,
        at: (f64, f64),
    },
    Image {
        definition: usize,
        at: (f64, f64),
        size: (f64, f64),
    },
    Raw(i16, Value),
}

impl Kind {
    fn points(&self) -> Vec<(f64, f64)> {
        match self {
            Kind::Line { a, b } => vec![*a, *b],
            Kind::Circle { center, radius } | Kind::Arc { center, radius, .. } => vec![
                (center.0 - radius, center.1 - radius),
                (center.0 + radius, center.1 + radius),
            ],
            Kind::Lwpolyline { points, .. } => points.iter().map(|p| (p.0, p.1)).collect(),
            Kind::Text { at, .. }
            | Kind::Mtext { at, .. }
            | Kind::Insert { at, .. }
            | Kind::Image { at, .. } => vec![*at],
            Kind::Dimension { .. } | Kind::Raw(..) => Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct Entity {
    layer: String,
    kind: Kind,
    link: Option<String>,
}

#[derive(Clone, Debug)]
struct Layer {
    name: String,
    color: i16,
}

#[derive(Clone, Debug)]
struct BlockDef {
    name: String,
    base: (f64, f64),
    xref: Option<String>,
    entities: Vec<Entity>,
}

#[derive(Clone, Debug)]
struct PaperLayout {
    name: String,
    entities: Vec<Entity>,
}

#[derive(Clone, Copy, Debug)]
enum Target {
    Model,
    Block(usize),
    Paper(usize),
}

/// Builds a DXF file in memory; entity methods apply to model space until a block or paper layout is opened.
#[derive(Clone, Debug)]
pub struct DxfBuilder {
    version: &'static str,
    codepage: &'static str,
    layers: Vec<Layer>,
    blocks: Vec<BlockDef>,
    entities: Vec<Entity>,
    layouts: Vec<PaperLayout>,
    imagedefs: Vec<String>,
    binary: bool,
    crlf: bool,
    target: Target,
    current_layer: String,
}

/// Same type as `DxfBuilder` with a block as the entity target; `end` returns to model space.
pub type BlockBuilder = DxfBuilder;

/// Same type as `DxfBuilder` with a paper layout as the entity target; `end` returns to model space.
pub type LayoutBuilder = DxfBuilder;

impl Default for DxfBuilder {
    fn default() -> Self {
        DxfBuilder::new()
    }
}

struct Plan {
    layer_table: u64,
    layers: Vec<u64>,
    record_table: u64,
    model_record: u64,
    paper_records: Vec<u64>,
    block_records: Vec<u64>,
    layout_dictionary: u64,
    model_layout: u64,
    layouts: Vec<u64>,
    imagedefs: Vec<u64>,
}

impl DxfBuilder {
    pub fn new() -> DxfBuilder {
        DxfBuilder {
            version: "AC1027",
            codepage: "ANSI_1252",
            layers: vec![Layer {
                name: "0".to_string(),
                color: 7,
            }],
            blocks: Vec::new(),
            entities: Vec::new(),
            layouts: vec![PaperLayout {
                name: "Layout1".to_string(),
                entities: Vec::new(),
            }],
            imagedefs: Vec::new(),
            binary: false,
            crlf: false,
            target: Target::Model,
            current_layer: "0".to_string(),
        }
    }

    pub fn version(mut self, version: &'static str) -> DxfBuilder {
        self.version = version;
        self
    }

    pub fn codepage(mut self, codepage: &'static str) -> DxfBuilder {
        self.codepage = codepage;
        self
    }

    pub fn binary(mut self) -> DxfBuilder {
        self.binary = true;
        self
    }

    pub fn crlf(mut self) -> DxfBuilder {
        self.crlf = true;
        self
    }

    pub fn layer(mut self, name: &str, color: i16) -> DxfBuilder {
        let Some(existing) = self.layers.iter_mut().find(|layer| layer.name == name) else {
            self.layers.push(Layer {
                name: name.to_string(),
                color,
            });
            return self;
        };
        existing.color = color;
        self
    }

    pub fn on_layer(mut self, name: &str) -> DxfBuilder {
        self.current_layer = name.to_string();
        self.ensure_layer(name);
        self
    }

    pub fn block(mut self, name: &str, base: (f64, f64)) -> BlockBuilder {
        self.blocks.push(BlockDef {
            name: name.to_string(),
            base,
            xref: None,
            entities: Vec::new(),
        });
        self.target = Target::Block(self.blocks.len() - 1);
        self
    }

    pub fn xref_block(mut self, name: &str, path: &str) -> DxfBuilder {
        self.blocks.push(BlockDef {
            name: name.to_string(),
            base: (0.0, 0.0),
            xref: Some(path.to_string()),
            entities: Vec::new(),
        });
        self
    }

    pub fn paper_layout(mut self, name: &str) -> LayoutBuilder {
        let existing = self.layouts.iter().position(|layout| layout.name == name);
        self.target = Target::Paper(existing.unwrap_or(self.layouts.len()));
        if existing.is_some() {
            return self;
        }
        self.layouts.push(PaperLayout {
            name: name.to_string(),
            entities: Vec::new(),
        });
        self
    }

    pub fn end(mut self) -> DxfBuilder {
        self.target = Target::Model;
        self
    }

    pub fn line(self, a: (f64, f64), b: (f64, f64)) -> DxfBuilder {
        self.push(Kind::Line { a, b })
    }

    pub fn circle(self, center: (f64, f64), radius: f64) -> DxfBuilder {
        self.push(Kind::Circle { center, radius })
    }

    pub fn arc(self, center: (f64, f64), radius: f64, start: f64, end: f64) -> DxfBuilder {
        self.push(Kind::Arc {
            center,
            radius,
            start,
            end,
        })
    }

    pub fn lwpolyline(self, points: &[(f64, f64, f64)], closed: bool) -> DxfBuilder {
        self.push(Kind::Lwpolyline {
            points: points.to_vec(),
            closed,
        })
    }

    pub fn text(self, value: &str, at: (f64, f64), height: f64) -> DxfBuilder {
        self.push(Kind::Text {
            value: Value::Str(value.to_string()),
            at,
            height,
        })
    }

    pub fn text_bytes(self, bytes: &[u8], at: (f64, f64), height: f64) -> DxfBuilder {
        self.push(Kind::Text {
            value: Value::Raw(bytes.to_vec()),
            at,
            height,
        })
    }

    pub fn mtext(self, value: &str, at: (f64, f64), height: f64) -> DxfBuilder {
        self.push(Kind::Mtext {
            value: value.to_string(),
            at,
            height,
        })
    }

    pub fn insert(self, block: &str, at: (f64, f64)) -> DxfBuilder {
        self.insert_transformed(block, at, (1.0, 1.0, 1.0), 0.0)
    }

    pub fn insert_transformed(
        self,
        block: &str,
        at: (f64, f64),
        scale: (f64, f64, f64),
        rotation: f64,
    ) -> DxfBuilder {
        self.push(Kind::Insert {
            block: block.to_string(),
            at,
            scale,
            rotation,
            attribs: Vec::new(),
        })
    }

    pub fn insert_with_attribs(
        self,
        block: &str,
        at: (f64, f64),
        attribs: &[(&str, &str)],
    ) -> DxfBuilder {
        self.push(Kind::Insert {
            block: block.to_string(),
            at,
            scale: (1.0, 1.0, 1.0),
            rotation: 0.0,
            attribs: attribs
                .iter()
                .map(|(tag, value)| (tag.to_string(), value.to_string()))
                .collect(),
        })
    }

    pub fn dimension(self, text_override: &str, measurement: f64) -> DxfBuilder {
        self.dimension_at(text_override, measurement, (0.0, 0.0))
    }

    pub fn dimension_at(self, text_override: &str, measurement: f64, at: (f64, f64)) -> DxfBuilder {
        self.push(Kind::Dimension {
            text: text_override.to_string(),
            measurement: Some(measurement),
            at,
        })
    }

    pub fn dimension_unmeasured(self, text_override: &str) -> DxfBuilder {
        self.push(Kind::Dimension {
            text: text_override.to_string(),
            measurement: None,
            at: (0.0, 0.0),
        })
    }

    pub fn hyperlink(mut self, url: &str) -> DxfBuilder {
        let Some(last) = self.target_entities().last_mut() else {
            return self;
        };
        last.link = Some(url.to_string());
        self
    }

    pub fn imagedef(mut self, path: &str) -> DxfBuilder {
        self.imagedefs.push(path.to_string());
        self
    }

    /// Adds an IMAGE entity whose definition is the IMAGEDEF for `path`, reusing an existing definition with the same path.
    pub fn image(mut self, path: &str, at: (f64, f64), size: (f64, f64)) -> DxfBuilder {
        let existing = self.imagedefs.iter().position(|known| known == path);
        let definition = existing.unwrap_or(self.imagedefs.len());
        if existing.is_none() {
            self.imagedefs.push(path.to_string());
        }
        self.push(Kind::Image {
            definition,
            at,
            size,
        })
    }

    pub fn raw(self, code: i16, value: &str) -> DxfBuilder {
        self.push(Kind::Raw(code, typed_value(code, value)))
    }

    pub fn build(self) -> Vec<u8> {
        let records = self.records();
        if self.binary {
            return write_binary(&records);
        }
        write_ascii(&records, self.crlf)
    }

    fn ensure_layer(&mut self, name: &str) {
        if self.layers.iter().any(|layer| layer.name == name) {
            return;
        }
        self.layers.push(Layer {
            name: name.to_string(),
            color: 7,
        });
    }

    fn target_entities(&mut self) -> &mut Vec<Entity> {
        match self.target {
            Target::Block(index) if index < self.blocks.len() => &mut self.blocks[index].entities,
            Target::Paper(index) if index < self.layouts.len() => &mut self.layouts[index].entities,
            _ => &mut self.entities,
        }
    }

    fn push(mut self, kind: Kind) -> DxfBuilder {
        let layer = self.current_layer.clone();
        self.ensure_layer(&layer);
        self.target_entities().push(Entity {
            layer,
            kind,
            link: None,
        });
        self
    }

    fn plan(&self, out: &mut Out) -> Plan {
        let layer_table = out.alloc();
        let layers = self.layers.iter().map(|_| out.alloc()).collect();
        let record_table = out.alloc();
        let model_record = out.alloc();
        let paper_records = self.layouts.iter().map(|_| out.alloc()).collect();
        let block_records = self.blocks.iter().map(|_| out.alloc()).collect();
        let layout_dictionary = out.alloc();
        let model_layout = out.alloc();
        let layouts = self.layouts.iter().map(|_| out.alloc()).collect();
        let imagedefs: Vec<u64> = self.imagedefs.iter().map(|_| out.alloc()).collect();
        out.imagedefs = imagedefs.clone();
        Plan {
            layer_table,
            layers,
            record_table,
            model_record,
            paper_records,
            block_records,
            layout_dictionary,
            model_layout,
            layouts,
            imagedefs,
        }
    }

    fn records(&self) -> Vec<Record> {
        let mut out = Out::new();
        let plan = self.plan(&mut out);
        self.write_header(&mut out);
        self.write_tables(&mut out, &plan);
        self.write_blocks(&mut out, &plan);
        self.write_entities(&mut out, &plan);
        self.write_objects(&mut out, &plan);
        out.s(0, "EOF");
        out.records
    }

    fn write_header(&self, out: &mut Out) {
        let points: Vec<(f64, f64)> = self
            .entities
            .iter()
            .flat_map(|entity| entity.kind.points())
            .collect();
        let low = points
            .iter()
            .copied()
            .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1)));
        let high = points
            .iter()
            .copied()
            .reduce(|a, b| (a.0.max(b.0), a.1.max(b.1)));
        out.begin("HEADER");
        out.variable("$ACADVER", 1, Value::Str(self.version.to_string()));
        out.variable("$DWGCODEPAGE", 3, Value::Str(self.codepage.to_string()));
        out.variable("$INSUNITS", 70, Value::I16(4));
        out.extent("$EXTMIN", low.unwrap_or((0.0, 0.0)));
        out.extent("$EXTMAX", high.unwrap_or((0.0, 0.0)));
        out.variable("$TDCREATE", 40, Value::F64(2460371.5));
        out.variable("$TDUPDATE", 40, Value::F64(2460372.5));
        out.variable("$LASTSAVEDBY", 1, Value::Str("cadboss-testkit".to_string()));
        out.end();
    }

    fn write_tables(&self, out: &mut Out, plan: &Plan) {
        out.begin("TABLES");
        out.s(0, "TABLE");
        out.s(2, "LAYER");
        out.id(5, plan.layer_table);
        out.n(330, 0);
        out.s(100, "AcDbSymbolTable");
        out.n(70, self.layers.len() as i64);
        for (layer, handle) in self.layers.iter().zip(&plan.layers) {
            out.s(0, "LAYER");
            out.id(5, *handle);
            out.id(330, plan.layer_table);
            out.s(100, "AcDbSymbolTableRecord");
            out.s(100, "AcDbLayerTableRecord");
            out.s(2, &layer.name);
            out.n(70, 0);
            out.n(62, i64::from(layer.color));
            out.s(6, "Continuous");
        }
        out.s(0, "ENDTAB");

        out.s(0, "TABLE");
        out.s(2, "BLOCK_RECORD");
        out.id(5, plan.record_table);
        out.n(330, 0);
        out.s(100, "AcDbSymbolTable");
        out.n(70, (1 + self.layouts.len() + self.blocks.len()) as i64);
        write_block_record(
            out,
            plan.record_table,
            plan.model_record,
            MODEL_SPACE,
            Some(plan.model_layout),
        );
        for (index, (handle, layout)) in plan.paper_records.iter().zip(&plan.layouts).enumerate() {
            let name = paper_block_name(index);
            write_block_record(out, plan.record_table, *handle, &name, Some(*layout));
        }
        for (block, handle) in self.blocks.iter().zip(&plan.block_records) {
            write_block_record(out, plan.record_table, *handle, &block.name, None);
        }
        out.s(0, "ENDTAB");
        out.end();
    }

    fn write_blocks(&self, out: &mut Out, plan: &Plan) {
        out.begin("BLOCKS");
        write_block(out, MODEL_SPACE, plan.model_record, (0.0, 0.0), None);
        write_block_end(out, plan.model_record);
        for (index, (layout, handle)) in self.layouts.iter().zip(&plan.paper_records).enumerate() {
            write_block(out, &paper_block_name(index), *handle, (0.0, 0.0), None);
            if index > 0 {
                for entity in &layout.entities {
                    write_entity(out, entity, *handle, true);
                }
            }
            write_block_end(out, *handle);
        }
        for (block, handle) in self.blocks.iter().zip(&plan.block_records) {
            write_block(out, &block.name, *handle, block.base, block.xref.as_deref());
            for entity in &block.entities {
                write_entity(out, entity, *handle, false);
            }
            write_block_end(out, *handle);
        }
        out.end();
    }

    fn write_entities(&self, out: &mut Out, plan: &Plan) {
        out.begin("ENTITIES");
        for entity in &self.entities {
            write_entity(out, entity, plan.model_record, false);
        }
        if let (Some(layout), Some(owner)) = (self.layouts.first(), plan.paper_records.first()) {
            for entity in &layout.entities {
                write_entity(out, entity, *owner, true);
            }
        }
        out.end();
    }

    fn write_objects(&self, out: &mut Out, plan: &Plan) {
        out.begin("OBJECTS");
        out.s(0, "DICTIONARY");
        out.s(5, ROOT_DICTIONARY);
        out.n(330, 0);
        out.s(100, "AcDbDictionary");
        out.n(281, 1);
        out.s(3, "ACAD_LAYOUT");
        out.id(350, plan.layout_dictionary);

        out.s(0, "DICTIONARY");
        out.id(5, plan.layout_dictionary);
        out.s(330, ROOT_DICTIONARY);
        out.s(100, "AcDbDictionary");
        out.n(281, 1);
        out.s(3, "Model");
        out.id(350, plan.model_layout);
        for (layout, handle) in self.layouts.iter().zip(&plan.layouts) {
            out.s(3, &layout.name);
            out.id(350, *handle);
        }

        write_layout(out, plan, "Model", plan.model_layout, 0, plan.model_record);
        let sheets = self
            .layouts
            .iter()
            .zip(&plan.layouts)
            .zip(&plan.paper_records);
        for (index, ((layout, handle), record)) in sheets.enumerate() {
            write_layout(out, plan, &layout.name, *handle, index + 1, *record);
        }

        for (path, handle) in self.imagedefs.iter().zip(&plan.imagedefs) {
            let handle = *handle;
            out.s(0, "IMAGEDEF");
            out.id(5, handle);
            out.n(330, 0);
            out.s(100, "AcDbRasterImageDef");
            out.n(90, 0);
            out.s(1, path);
            out.f(10, 100.0);
            out.f(20, 100.0);
            out.n(280, 1);
            out.n(281, 0);
        }
        out.end();
    }
}

fn write_block_record(out: &mut Out, table: u64, handle: u64, name: &str, layout: Option<u64>) {
    out.s(0, "BLOCK_RECORD");
    out.id(5, handle);
    out.id(330, table);
    out.s(100, "AcDbSymbolTableRecord");
    out.s(100, "AcDbBlockTableRecord");
    out.s(2, name);
    let Some(layout) = layout else {
        return;
    };
    out.id(340, layout);
}

fn write_block(out: &mut Out, name: &str, record: u64, base: (f64, f64), xref: Option<&str>) {
    let handle = out.alloc();
    out.s(0, "BLOCK");
    out.id(5, handle);
    out.id(330, record);
    out.s(100, "AcDbEntity");
    out.s(8, "0");
    out.s(100, "AcDbBlockBegin");
    out.s(2, name);
    out.n(70, if xref.is_some() { XREF_FLAGS } else { 0 });
    out.point(10, base.0, base.1, 0.0);
    out.s(3, name);
    out.s(1, xref.unwrap_or_default());
}

fn write_block_end(out: &mut Out, record: u64) {
    let handle = out.alloc();
    out.s(0, "ENDBLK");
    out.id(5, handle);
    out.id(330, record);
    out.s(100, "AcDbEntity");
    out.s(8, "0");
    out.s(100, "AcDbBlockEnd");
}

fn write_layout(out: &mut Out, plan: &Plan, name: &str, handle: u64, tab: usize, record: u64) {
    out.s(0, "LAYOUT");
    out.id(5, handle);
    out.id(330, plan.layout_dictionary);
    out.s(100, "AcDbPlotSettings");
    out.s(1, name);
    out.s(100, "AcDbLayout");
    out.s(1, name);
    out.n(70, 1);
    out.n(71, tab as i64);
    out.id(330, record);
}

fn mtext_chunks(value: &str) -> (Vec<&str>, &str) {
    let mut rest = value;
    let mut leading = Vec::new();
    while rest.len() > MTEXT_CHUNK {
        let mut cut = MTEXT_CHUNK;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let (head, tail) = rest.split_at(cut);
        leading.push(head);
        rest = tail;
    }
    (leading, rest)
}

fn write_entity(out: &mut Out, entity: &Entity, owner: u64, paper: bool) {
    let (name, subclass) = match &entity.kind {
        Kind::Raw(code, value) => {
            out.records.push((*code, value.clone()));
            return;
        }
        Kind::Line { .. } => ("LINE", "AcDbLine"),
        Kind::Circle { .. } => ("CIRCLE", "AcDbCircle"),
        Kind::Arc { .. } => ("ARC", "AcDbCircle"),
        Kind::Lwpolyline { .. } => ("LWPOLYLINE", "AcDbPolyline"),
        Kind::Text { .. } => ("TEXT", "AcDbText"),
        Kind::Mtext { .. } => ("MTEXT", "AcDbMText"),
        Kind::Insert { .. } => ("INSERT", "AcDbBlockReference"),
        Kind::Dimension { .. } => ("DIMENSION", "AcDbDimension"),
        Kind::Image { .. } => ("IMAGE", "AcDbRasterImage"),
    };
    let handle = out.alloc();
    out.s(0, name);
    out.id(5, handle);
    out.id(330, owner);
    out.s(100, "AcDbEntity");
    out.s(8, &entity.layer);
    if paper {
        out.n(67, 1);
    }
    out.s(100, subclass);
    let attribs = write_body(out, entity);
    if let Some(url) = &entity.link {
        out.s(1001, "PE_URL");
        out.s(1002, "{");
        out.s(1000, url);
        out.s(1002, "}");
    }
    if attribs.is_empty() {
        return;
    }
    for (tag, value) in attribs {
        write_attrib(out, &entity.layer, owner, paper, (&tag, &value));
    }
    let handle = out.alloc();
    out.s(0, "SEQEND");
    out.id(5, handle);
    out.id(330, owner);
}

fn write_body(out: &mut Out, entity: &Entity) -> Vec<(String, String)> {
    match &entity.kind {
        Kind::Line { a, b } => {
            out.point(10, a.0, a.1, 0.0);
            out.point(11, b.0, b.1, 0.0);
        }
        Kind::Circle { center, radius } => {
            out.point(10, center.0, center.1, 0.0);
            out.f(40, *radius);
        }
        Kind::Arc {
            center,
            radius,
            start,
            end,
        } => {
            out.point(10, center.0, center.1, 0.0);
            out.f(40, *radius);
            out.s(100, "AcDbArc");
            out.f(50, *start);
            out.f(51, *end);
        }
        Kind::Lwpolyline { points, closed } => {
            out.n(90, points.len() as i64);
            out.n(70, i64::from(*closed));
            for (x, y, bulge) in points {
                out.f(10, *x);
                out.f(20, *y);
                if *bulge != 0.0 {
                    out.f(42, *bulge);
                }
            }
        }
        Kind::Text { value, at, height } => {
            out.point(10, at.0, at.1, 0.0);
            out.f(40, *height);
            out.records.push((1, value.clone()));
        }
        Kind::Mtext { value, at, height } => {
            out.point(10, at.0, at.1, 0.0);
            out.f(40, *height);
            out.f(41, 0.0);
            let (leading, last) = mtext_chunks(value);
            for chunk in leading {
                out.s(3, chunk);
            }
            out.s(1, last);
        }
        Kind::Insert {
            block,
            at,
            scale,
            rotation,
            attribs,
        } => {
            if !attribs.is_empty() {
                out.n(66, 1);
            }
            out.s(2, block);
            out.point(10, at.0, at.1, 0.0);
            if *scale != (1.0, 1.0, 1.0) {
                out.f(41, scale.0);
                out.f(42, scale.1);
                out.f(43, scale.2);
            }
            if *rotation != 0.0 {
                out.f(50, *rotation);
            }
            return attribs.clone();
        }
        Kind::Dimension {
            text,
            measurement,
            at,
        } => {
            out.dimensions += 1;
            out.s(2, &format!("*D{}", out.dimensions));
            out.point(10, 0.0, 0.0, 0.0);
            out.point(11, at.0, at.1, 0.0);
            out.n(70, 0);
            out.s(1, text);
            out.s(3, "Standard");
            if let Some(measurement) = measurement {
                out.f(42, *measurement);
            }
            out.s(100, "AcDbAlignedDimension");
            out.point(13, 0.0, 0.0, 0.0);
            out.point(14, 0.0, 0.0, 0.0);
        }
        Kind::Image {
            definition,
            at,
            size,
        } => {
            out.point(10, at.0, at.1, 0.0);
            out.point(13, size.0, size.1, 0.0);
            if let Some(handle) = out.imagedefs.get(*definition).copied() {
                out.id(340, handle);
            }
        }
        Kind::Raw(..) => {}
    }
    Vec::new()
}

fn write_attrib(out: &mut Out, layer: &str, owner: u64, paper: bool, attrib: (&str, &str)) {
    let handle = out.alloc();
    out.s(0, "ATTRIB");
    out.id(5, handle);
    out.id(330, owner);
    out.s(100, "AcDbEntity");
    out.s(8, layer);
    if paper {
        out.n(67, 1);
    }
    out.s(100, "AcDbText");
    out.point(10, 0.0, 0.0, 0.0);
    out.f(40, 2.5);
    out.s(1, attrib.1);
    out.s(100, "AcDbAttribute");
    out.s(2, attrib.0);
    out.n(70, 0);
}

fn write_binary(records: &[Record]) -> Vec<u8> {
    let mut bytes = SENTINEL.to_vec();
    for (code, value) in records {
        bytes.extend_from_slice(&code.to_le_bytes());
        match value {
            Value::Str(text) => {
                bytes.extend_from_slice(text.as_bytes());
                bytes.push(0);
            }
            Value::Raw(raw) => {
                bytes.extend_from_slice(raw);
                bytes.push(0);
            }
            Value::F64(number) => bytes.extend_from_slice(&number.to_le_bytes()),
            Value::I16(number) => bytes.extend_from_slice(&number.to_le_bytes()),
            Value::I32(number) => bytes.extend_from_slice(&number.to_le_bytes()),
            Value::I64(number) => bytes.extend_from_slice(&number.to_le_bytes()),
            Value::Bool(flag) => bytes.push(u8::from(*flag)),
        }
    }
    bytes
}

fn write_ascii(records: &[Record], crlf: bool) -> Vec<u8> {
    let newline: &[u8] = if crlf { b"\r\n" } else { b"\n" };
    let mut bytes = Vec::new();
    for (code, value) in records {
        bytes.extend_from_slice(format!("{code:>3}").as_bytes());
        bytes.extend_from_slice(newline);
        match value {
            Value::Str(text) => bytes.extend_from_slice(text.as_bytes()),
            Value::Raw(raw) => bytes.extend_from_slice(raw),
            Value::F64(number) => bytes.extend_from_slice(format!("{number:?}").as_bytes()),
            Value::I16(number) => bytes.extend_from_slice(number.to_string().as_bytes()),
            Value::I32(number) => bytes.extend_from_slice(number.to_string().as_bytes()),
            Value::I64(number) => bytes.extend_from_slice(number.to_string().as_bytes()),
            Value::Bool(flag) => bytes.push(if *flag { b'1' } else { b'0' }),
        }
        bytes.extend_from_slice(newline);
    }
    bytes
}

pub fn simple_drawing() -> Vec<u8> {
    simple_builder().build()
}

pub fn simple_builder() -> DxfBuilder {
    DxfBuilder::new()
        .layer("DIM", 1)
        .block("TITLEBLOCK", (0.0, 0.0))
        .text("TITLE: PUMP P-101", (5.0, 5.0), 2.5)
        .line((0.0, 0.0), (100.0, 0.0))
        .end()
        .line((0.0, 0.0), (50.0, 25.0))
        .on_layer("DIM")
        .circle((60.0, 30.0), 5.0)
        .on_layer("0")
        .text("Hello", (10.0, 40.0), 2.5)
        .paper_layout("Layout1")
        .insert("TITLEBLOCK", (200.0, 10.0))
        .end()
}

pub fn large_drawing(entities: usize) -> Vec<u8> {
    let mut builder = DxfBuilder::new();
    for index in 0..entities {
        let layer = if index % 2 == 0 { "0" } else { "L1" };
        let x = index as f64;
        builder = builder.on_layer(layer).line((x, 0.0), (x + 1.0, 1.0));
    }
    builder.build()
}

pub fn nested_inserts(depth: usize) -> Vec<u8> {
    let mut builder = DxfBuilder::new();
    for level in 0..=depth {
        builder = builder
            .block(&format!("B{level}"), (0.0, 0.0))
            .line((0.0, 0.0), (10.0, 10.0));
        if level < depth {
            builder = builder.insert(&format!("B{}", level + 1), (1.0, 1.0));
        }
        builder = builder.end();
    }
    builder.insert("B0", (0.0, 0.0)).build()
}

pub fn multi_layout(layouts: usize) -> Vec<u8> {
    let mut builder = DxfBuilder::new();
    for sheet in 1..=layouts {
        builder = builder
            .paper_layout(&format!("Sheet{sheet}"))
            .text(&format!("Sheet {sheet}"), (10.0, 10.0), 2.5)
            .end();
    }
    builder.build()
}

#[cfg(test)]
mod tests {
    use cadboss_core::Budget;
    use cadboss_dxf::{Cursor, SectionName, Tape, SENTINEL as DXF_SENTINEL};

    use super::*;

    fn count_records(bytes: &[u8], section: SectionName, name: &[u8]) -> usize {
        let (tape, report) = Tape::lex(bytes, &Budget::default()).unwrap();
        assert!(report.is_empty());
        let range = tape.section(section).unwrap();
        Cursor::new(&tape, bytes, range.start..range.end)
            .filter(|(code, value)| *code == 0 && *value == name)
            .count()
    }

    #[test]
    fn builder_output_lexes_with_an_empty_report() {
        let bytes = simple_drawing();
        let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
        assert!(report.is_empty());
        assert!(!tape.is_binary());
        let bytes = DxfBuilder::new()
            .binary()
            .line((0.0, 0.0), (1.0, 1.0))
            .build();
        let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
        assert!(report.is_empty());
        assert!(tape.is_binary());
    }

    #[test]
    fn simple_drawing_has_every_section() {
        let bytes = simple_drawing();
        let (tape, _) = Tape::lex(&bytes, &Budget::default()).unwrap();
        for name in [
            SectionName::Header,
            SectionName::Tables,
            SectionName::Blocks,
            SectionName::Entities,
            SectionName::Objects,
        ] {
            assert!(tape.section(name).is_some(), "{name:?}");
        }
    }

    #[test]
    fn binary_builder_output_starts_with_the_sentinel() {
        let bytes = DxfBuilder::new().binary().build();
        assert!(bytes.starts_with(DXF_SENTINEL));
    }

    #[test]
    fn nested_inserts_declares_every_block() {
        let bytes = nested_inserts(3);
        assert_eq!(count_records(&bytes, SectionName::Blocks, b"BLOCK"), 2 + 4);
        assert_eq!(count_records(&bytes, SectionName::Blocks, b"INSERT"), 3);
        assert_eq!(count_records(&bytes, SectionName::Entities, b"INSERT"), 1);
    }

    #[test]
    fn multi_layout_writes_one_layout_object_per_sheet() {
        let bytes = multi_layout(5);
        assert_eq!(count_records(&bytes, SectionName::Objects, b"LAYOUT"), 7);
    }

    #[test]
    fn large_drawing_writes_n_lines() {
        let bytes = large_drawing(250);
        assert_eq!(count_records(&bytes, SectionName::Entities, b"LINE"), 250);
    }

    type Pairs = Vec<(i16, Vec<u8>)>;

    fn section_pairs(bytes: &[u8], section: SectionName) -> Pairs {
        let (tape, _) = Tape::lex(bytes, &Budget::default()).unwrap();
        let range = tape.section(section).unwrap();
        Cursor::new(&tape, bytes, range.start..range.end)
            .map(|(code, value)| (code, value.to_vec()))
            .collect()
    }

    fn records(pairs: &Pairs) -> Vec<Pairs> {
        let mut out: Vec<Pairs> = Vec::new();
        for (code, value) in pairs {
            if *code == 0 {
                out.push(Vec::new());
            }
            if let Some(last) = out.last_mut() {
                last.push((*code, value.clone()));
            }
        }
        out
    }

    fn first(record: &Pairs, code: i16) -> Vec<u8> {
        record
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, v)| v.clone())
            .unwrap()
    }

    fn text(value: &[u8]) -> String {
        String::from_utf8(value.to_vec()).unwrap()
    }

    fn entity_records(bytes: &[u8]) -> Vec<Pairs> {
        records(&section_pairs(bytes, SectionName::Entities))
    }

    #[test]
    fn mtext_splits_long_text_into_250_byte_chunks_and_a_final_chunk() {
        let bytes = DxfBuilder::new()
            .mtext(&"a".repeat(600), (0.0, 0.0), 2.5)
            .build();
        let mtext = &entity_records(&bytes)[0];
        let leading: Vec<usize> = mtext
            .iter()
            .filter(|(c, _)| *c == 3)
            .map(|(_, v)| v.len())
            .collect();
        assert_eq!(leading, vec![250, 250]);
        assert_eq!(first(mtext, 1).len(), 100);
    }

    #[test]
    fn mtext_never_splits_a_multibyte_character() {
        let original = "é".repeat(300);
        let bytes = DxfBuilder::new().mtext(&original, (0.0, 0.0), 2.5).build();
        let mtext = &entity_records(&bytes)[0];
        let joined: String = mtext
            .iter()
            .filter(|(c, _)| *c == 3 || *c == 1)
            .map(|(_, v)| text(v))
            .collect();
        assert_eq!(joined, original);
        assert!(mtext
            .iter()
            .filter(|(c, _)| *c == 3)
            .all(|(_, v)| v.len() <= 250));
    }

    #[test]
    fn insert_with_attribs_writes_flag_attribs_and_seqend() {
        let bytes = DxfBuilder::new()
            .insert_with_attribs("B", (1.0, 2.0), &[("TAG1", "v1"), ("TAG2", "v2")])
            .build();
        let all = entity_records(&bytes);
        let names: Vec<String> = all.iter().map(|r| text(&r[0].1)).collect();
        assert_eq!(names, ["INSERT", "ATTRIB", "ATTRIB", "SEQEND"]);
        assert!(all[0].contains(&(66, b"1".to_vec())));
        assert_eq!(first(&all[1], 2), b"TAG1");
        assert_eq!(first(&all[1], 1), b"v1");
        assert_eq!(first(&all[2], 2), b"TAG2");
        assert_eq!(first(&all[2], 1), b"v2");
    }

    #[test]
    fn dimension_writes_name_override_and_measurement() {
        let bytes = DxfBuilder::new().dimension("<>mm", 42.5).build();
        let dimension = &entity_records(&bytes)[0];
        assert_eq!(first(dimension, 2), b"*D1");
        assert_eq!(first(dimension, 1), b"<>mm");
        assert_eq!(first(dimension, 42), b"42.5");
    }

    #[test]
    fn hyperlink_writes_the_url_xdata_sequence() {
        let bytes = DxfBuilder::new()
            .line((0.0, 0.0), (1.0, 1.0))
            .hyperlink("http://x.test/a")
            .build();
        let line = &entity_records(&bytes)[0];
        let tail: Vec<(i16, Vec<u8>)> = line[line.len() - 4..].to_vec();
        assert_eq!(
            tail,
            vec![
                (1001, b"PE_URL".to_vec()),
                (1002, b"{".to_vec()),
                (1000, b"http://x.test/a".to_vec()),
                (1002, b"}".to_vec()),
            ]
        );
    }

    #[test]
    fn xref_block_writes_the_path_into_the_block_record() {
        let bytes = DxfBuilder::new()
            .xref_block("BASE", "C:/refs/base.dwg")
            .build();
        assert!(bytes.windows(18).any(|w| w == b"\nC:/refs/base.dwg\n"));
        assert_eq!(count_records(&bytes, SectionName::Blocks, b"BLOCK"), 3);
    }

    #[test]
    fn imagedef_writes_one_object_per_path() {
        let bytes = DxfBuilder::new()
            .imagedef("a.png")
            .imagedef("b.png")
            .build();
        assert_eq!(count_records(&bytes, SectionName::Objects, b"IMAGEDEF"), 2);
    }

    #[test]
    fn image_points_its_entity_at_a_shared_imagedef() {
        let bytes = DxfBuilder::new()
            .image("a.png", (0.0, 0.0), (4.0, 3.0))
            .image("a.png", (9.0, 0.0), (4.0, 3.0))
            .build();
        assert_eq!(count_records(&bytes, SectionName::Objects, b"IMAGEDEF"), 1);
        let all = entity_records(&bytes);
        assert_eq!(all.len(), 2);
        assert_eq!(first(&all[0], 340), first(&all[1], 340));
        assert_eq!(first(&all[0], 13), b"4.0");
    }

    #[test]
    fn text_bytes_reach_ascii_verbatim_and_binary_nul_terminated() {
        let ascii = DxfBuilder::new()
            .text_bytes(b"caf\xE9", (0.0, 0.0), 2.5)
            .build();
        assert!(ascii.windows(6).any(|w| w == b"\ncaf\xE9\n"));
        let binary = DxfBuilder::new()
            .binary()
            .text_bytes(b"caf\xE9", (0.0, 0.0), 2.5)
            .build();
        assert!(binary.windows(5).any(|w| w == b"caf\xE9\0"));
    }

    #[test]
    fn binary_values_round_trip_for_every_width_class() {
        let bytes = DxfBuilder::new()
            .binary()
            .line((1.5, 2.5), (3.0, 4.0))
            .lwpolyline(&[(0.0, 0.0, 0.0), (1.0, 1.0, 0.0)], true)
            .raw(160, "7")
            .raw(290, "1")
            .build();
        let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
        assert!(report.is_empty());
        let range = tape.section(SectionName::Entities).unwrap();
        let items: Vec<(i16, Vec<u8>)> = Cursor::new(&tape, &bytes, range.start..range.end)
            .map(|(c, v)| (c, v.to_vec()))
            .collect();
        let cursor = Cursor::new(&tape, &bytes, 0..0);
        let value = |code: i16| {
            items
                .iter()
                .find(|(c, _)| *c == code)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(cursor.f64(&value(10)), Some(1.5));
        assert_eq!(cursor.i64(&value(70)), Some(1));
        assert_eq!(cursor.i64(&value(90)), Some(2));
        assert_eq!(cursor.i64(&value(160)), Some(7));
        assert_eq!(cursor.i64(&value(290)), Some(1));
        assert_eq!(value(70).len(), 2);
        assert_eq!(value(90).len(), 4);
        assert_eq!(value(160).len(), 8);
        assert_eq!(value(290).len(), 1);
    }

    #[test]
    fn every_handle_in_simple_drawing_is_unique() {
        let bytes = simple_drawing();
        let (tape, _) = Tape::lex(&bytes, &Budget::default()).unwrap();
        let handles: Vec<Vec<u8>> = Cursor::new(&tape, &bytes, 0..tape.len())
            .filter(|(code, _)| *code == 5)
            .map(|(_, value)| value.to_vec())
            .collect();
        let unique: std::collections::HashSet<&Vec<u8>> = handles.iter().collect();
        assert!(handles.len() > 10);
        assert_eq!(unique.len(), handles.len());
    }

    #[test]
    fn layout_and_block_record_point_at_each_other() {
        let bytes = simple_drawing();
        let tables = records(&section_pairs(&bytes, SectionName::Tables));
        let paper = tables
            .iter()
            .find(|r| r.contains(&(2, b"*Paper_Space".to_vec())))
            .unwrap();
        let objects = records(&section_pairs(&bytes, SectionName::Objects));
        let layout = objects
            .iter()
            .find(|r| r[0].1 == b"LAYOUT" && r.contains(&(1, b"Layout1".to_vec())))
            .unwrap();
        let owner = layout.iter().rev().find(|(c, _)| *c == 330).unwrap();
        assert_eq!(owner.1, first(paper, 5));
        assert_eq!(first(paper, 340), first(layout, 5));
    }

    #[test]
    fn simple_drawing_extmax_is_65_by_40() {
        let bytes = simple_drawing();
        let header = section_pairs(&bytes, SectionName::Header);
        let at = header
            .iter()
            .position(|(c, v)| *c == 9 && v == b"$EXTMAX")
            .unwrap();
        assert_eq!(text(&header[at + 1].1), "65.0");
        assert_eq!(text(&header[at + 2].1), "40.0");
    }

    #[test]
    fn ascii_and_binary_builds_lex_to_matching_tapes() {
        let ascii = simple_builder().build();
        let binary = simple_builder().binary().build();
        let (ascii_tape, _) = Tape::lex(&ascii, &Budget::default()).unwrap();
        let (binary_tape, _) = Tape::lex(&binary, &Budget::default()).unwrap();
        assert_eq!(ascii_tape.len(), binary_tape.len());
        for name in [
            SectionName::Header,
            SectionName::Tables,
            SectionName::Blocks,
            SectionName::Entities,
            SectionName::Objects,
        ] {
            let a = ascii_tape.section(name).map(|s| (s.start, s.end));
            let b = binary_tape.section(name).map(|s| (s.start, s.end));
            assert_eq!(a, b, "{name:?}");
        }
    }
}
