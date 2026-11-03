//! Python bindings for cadboss.

#![deny(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cadboss_core::{Budget, Skipped};
use cadboss_dxf::{DxfDocument, Options};
use cadboss_model::{Document, Text, TextItem, TextSource};
use cadboss_output::{to_json, to_json_compact, to_markdown};
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyString};

pyo3::create_exception!(
    cadboss,
    CadError,
    PyException,
    "A CAD file could not be read."
);

type Corner = (f64, f64, f64);

const GROUPS: [&str; 7] = [
    "title-block",
    "annotations",
    "names",
    "attributes",
    "references",
    "properties",
    "all",
];

#[derive(Clone, PartialEq)]
struct PyText(Text);

impl<'py> IntoPyObject<'py> for PyText {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = std::convert::Infallible;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        match self.0 {
            Text::Str(text) => Ok(PyString::new(py, &text).into_any()),
            Text::Bytes(bytes) => Ok(PyBytes::new(py, &bytes).into_any()),
        }
    }
}

fn text_label(text: &Text) -> String {
    text.to_string()
}

fn source_label(source: TextSource) -> &'static str {
    match source {
        TextSource::TitleBlock => "title-block",
        TextSource::Annotation => "annotation",
        TextSource::Dimension => "dimension",
        TextSource::Leader => "leader",
        TextSource::BlockName => "block-name",
        TextSource::LayerName => "layer-name",
        TextSource::Attribute => "attribute",
        TextSource::Xref => "xref",
        TextSource::ImagePath => "image-path",
        TextSource::Hyperlink => "hyperlink",
        TextSource::Property => "property",
    }
}

fn apply_budget(budget: &mut Budget, key: &str, value: u64) -> Result<(), String> {
    let narrow = |value: u64| {
        u32::try_from(value).map_err(|_| format!("budget value too large for {key}: {value}"))
    };
    match key {
        "max_bytes" => budget.max_bytes = value,
        "max_entities" => budget.max_entities = value,
        "max_tokens" => budget.max_tokens = value,
        "max_layouts" => budget.max_layouts = narrow(value)?,
        "max_nesting" => budget.max_nesting = narrow(value)?,
        "max_string_bytes" => budget.max_string_bytes = narrow(value)?,
        other => return Err(format!("unknown budget key: {other}")),
    }
    Ok(())
}

fn options(budget: Option<&Bound<'_, PyDict>>) -> PyResult<Options> {
    let mut options = Options::default();
    let Some(budget) = budget else {
        return Ok(options);
    };
    for (key, value) in budget.iter() {
        let key: String = key
            .extract()
            .map_err(|_| PyValueError::new_err("budget keys must be strings"))?;
        if value.is_instance_of::<PyBool>() {
            return Err(PyValueError::new_err(format!(
                "budget value for {key} must be an integer, not a bool"
            )));
        }
        let value: u64 = value.extract().map_err(|_| {
            PyValueError::new_err(format!(
                "budget value for {key} must be a non-negative integer"
            ))
        })?;
        apply_budget(&mut options.budget, &key, value).map_err(PyValueError::new_err)?;
    }
    Ok(options)
}

fn select_text(
    document: &Document,
    group: &str,
    layout: Option<&str>,
) -> Result<Vec<PyTextItem>, String> {
    if !GROUPS.contains(&group) {
        return Err(format!(
            "unknown text group: {group} (expected {})",
            GROUPS.join(", ")
        ));
    }
    let groups = &document.text;
    let entries: [(&str, &[TextItem], bool); 6] = [
        ("title-block", &groups.title_block, true),
        ("annotations", &groups.annotations, true),
        ("names", &groups.names, false),
        ("attributes", &groups.attributes, true),
        ("references", &groups.references, true),
        ("properties", &groups.properties, false),
    ];
    let layout = layout.map(Text::from);
    let mut items = Vec::new();
    for (label, group_items, scoped) in entries {
        if group != "all" && group != label {
            continue;
        }
        if layout.is_some() && !scoped {
            continue;
        }
        items.extend(
            group_items
                .iter()
                .filter(|item| layout.as_ref().is_none_or(|name| item.layout == *name))
                .map(PyTextItem::from_item),
        );
    }
    Ok(items)
}

#[pyclass(frozen, get_all, eq, name = "Layer", module = "cadboss")]
#[derive(Clone, PartialEq)]
struct PyLayer {
    name: PyText,
    color: i16,
    frozen: bool,
    off: bool,
    locked: bool,
}

#[pymethods]
impl PyLayer {
    fn __repr__(&self) -> String {
        format!(
            "Layer(name={:?}, color={}, frozen={}, off={}, locked={})",
            text_label(&self.name.0),
            self.color,
            flag(self.frozen),
            flag(self.off),
            flag(self.locked)
        )
    }
}

#[pyclass(frozen, get_all, eq, name = "Layout", module = "cadboss")]
#[derive(Clone, PartialEq)]
struct PyLayout {
    name: PyText,
    kind: String,
    entity_count: u64,
}

#[pymethods]
impl PyLayout {
    fn __repr__(&self) -> String {
        format!(
            "Layout(name={:?}, kind={:?}, entity_count={})",
            text_label(&self.name.0),
            self.kind,
            self.entity_count
        )
    }
}

#[pyclass(frozen, get_all, eq, name = "TextItem", module = "cadboss")]
#[derive(Clone, PartialEq)]
struct PyTextItem {
    text: PyText,
    source: String,
    layer: PyText,
    layout: PyText,
    position: Option<Corner>,
    height: Option<f64>,
}

impl PyTextItem {
    fn from_item(item: &TextItem) -> PyTextItem {
        PyTextItem {
            text: PyText(item.text.clone()),
            source: source_label(item.source).to_owned(),
            layer: PyText(item.layer.clone()),
            layout: PyText(item.layout.clone()),
            position: item.position.map(|point| (point.x, point.y, point.z)),
            height: item.height,
        }
    }
}

#[pymethods]
impl PyTextItem {
    fn __repr__(&self) -> String {
        format!(
            "TextItem(text={:?}, source={:?}, layer={:?}, layout={:?})",
            text_label(&self.text.0),
            self.source,
            text_label(&self.layer.0),
            text_label(&self.layout.0)
        )
    }
}

#[pyclass(frozen, get_all, eq, name = "Skipped", module = "cadboss")]
#[derive(Clone, PartialEq)]
struct PySkipped {
    kind: String,
    reason: String,
    count: u64,
}

impl PySkipped {
    fn from_skipped(skipped: &Skipped) -> PySkipped {
        PySkipped {
            kind: skipped.kind.label().to_owned(),
            reason: skipped.reason.label().to_owned(),
            count: skipped.count,
        }
    }
}

#[pymethods]
impl PySkipped {
    fn __repr__(&self) -> String {
        format!(
            "Skipped(kind={:?}, reason={:?}, count={})",
            self.kind, self.reason, self.count
        )
    }
}

fn flag(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}

#[pyclass(frozen, name = "Document", module = "cadboss")]
struct PyDocument {
    document: Arc<Document>,
    title: String,
}

fn cad_error(error: cadboss_core::Error) -> PyErr {
    CadError::new_err(error.to_string())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[pymethods]
impl PyDocument {
    #[staticmethod]
    #[pyo3(signature = (path, *, budget=None))]
    fn open(
        py: Python<'_>,
        path: PathBuf,
        budget: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyDocument> {
        let options = options(budget)?;
        let document = py
            .allow_threads(|| {
                DxfDocument::read_with_options(&path, &options).map(|doc| doc.document())
            })
            .map_err(cad_error)?;
        Ok(PyDocument {
            document: Arc::new(document),
            title: stem(&path),
        })
    }

    #[staticmethod]
    #[pyo3(signature = (data, *, budget=None))]
    fn load(
        py: Python<'_>,
        data: &Bound<'_, PyAny>,
        budget: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyDocument> {
        let options = options(budget)?;
        let buffer = PyBuffer::<u8>::get(data)
            .map_err(|_| PyTypeError::new_err("data must be bytes, bytearray or memoryview"))?;
        options
            .budget
            .check_bytes(buffer.len_bytes() as u64)
            .map_err(cad_error)?;
        let bytes = buffer.to_vec(py)?;
        let document = py
            .allow_threads(|| {
                DxfDocument::load_with_options(bytes, &options).map(|doc| doc.document())
            })
            .map_err(cad_error)?;
        Ok(PyDocument {
            document: Arc::new(document),
            title: "document".to_owned(),
        })
    }

    #[getter]
    fn format(&self) -> &'static str {
        self.document.metadata.format.label()
    }

    #[getter]
    fn version(&self) -> String {
        text_label(&self.document.metadata.version)
    }

    #[getter]
    fn units(&self) -> String {
        self.document.metadata.units.label()
    }

    #[getter]
    fn layers(&self) -> Vec<PyLayer> {
        self.document
            .layers
            .iter()
            .map(|layer| PyLayer {
                name: PyText(layer.name.clone()),
                color: layer.color,
                frozen: layer.frozen,
                off: layer.off,
                locked: layer.locked,
            })
            .collect()
    }

    #[getter]
    fn layouts(&self) -> Vec<PyLayout> {
        self.document
            .layouts
            .iter()
            .map(|layout| PyLayout {
                name: PyText(layout.name.clone()),
                kind: layout.kind.label().to_owned(),
                entity_count: layout.entity_count,
            })
            .collect()
    }

    #[getter]
    fn blocks(&self) -> Vec<String> {
        self.document
            .blocks
            .iter()
            .filter(|block| !block.anonymous)
            .map(|block| text_label(&block.name))
            .collect()
    }

    #[getter]
    fn entity_counts(&self) -> Vec<(String, u64)> {
        self.document.metadata.entity_counts.clone()
    }

    #[getter]
    fn extents(&self) -> Option<(Corner, Corner)> {
        self.document.metadata.extents.map(|bbox| {
            (
                (bbox.min.x, bbox.min.y, bbox.min.z),
                (bbox.max.x, bbox.max.y, bbox.max.z),
            )
        })
    }

    #[pyo3(signature = (group="all", layout=None))]
    fn text(&self, py: Python<'_>, group: &str, layout: Option<&str>) -> PyResult<Vec<PyTextItem>> {
        py.allow_threads(|| select_text(&self.document, group, layout))
            .map_err(PyValueError::new_err)
    }

    #[pyo3(signature = (title=None))]
    fn markdown(&self, py: Python<'_>, title: Option<&str>) -> String {
        let title = title.unwrap_or(&self.title);
        py.allow_threads(|| to_markdown(&self.document, title))
    }

    #[pyo3(signature = (compact=false))]
    fn json(&self, py: Python<'_>, compact: bool) -> String {
        py.allow_threads(|| {
            if compact {
                to_json_compact(&self.document)
            } else {
                to_json(&self.document)
            }
        })
    }

    fn report(&self) -> Vec<PySkipped> {
        self.document
            .report
            .skipped
            .iter()
            .map(PySkipped::from_skipped)
            .collect()
    }
}

#[pymodule]
fn _cadboss(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("CadError", m.py().get_type::<CadError>())?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyLayer>()?;
    m.add_class::<PyLayout>()?;
    m.add_class::<PyTextItem>()?;
    m.add_class::<PySkipped>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadboss_model::document::TextGroups;
    use cadboss_model::{Format, Metadata};

    fn item(text: &str, layout: &str) -> TextItem {
        TextItem {
            text: Text::from(text),
            source: TextSource::Annotation,
            layer: Text::from("0"),
            layout: Text::from(layout),
            position: None,
            height: None,
        }
    }

    fn document() -> Document {
        Document {
            metadata: Metadata::new(Format::Dxf, Text::from("AC1027")),
            layers: Vec::new(),
            blocks: Vec::new(),
            layouts: Vec::new(),
            text: TextGroups {
                annotations: vec![item("a", "Model"), item("b", "Sheet")],
                names: vec![item("n", "Model")],
                ..TextGroups::default()
            },
            report: cadboss_core::Report::default(),
        }
    }

    #[test]
    fn budget_keys_set_their_fields() {
        let mut budget = Budget::default();
        apply_budget(&mut budget, "max_bytes", 5).ok();
        apply_budget(&mut budget, "max_nesting", 3).ok();
        assert_eq!(budget.max_bytes, 5);
        assert_eq!(budget.max_nesting, 3);
    }

    #[test]
    fn unknown_budget_key_is_rejected() {
        let mut budget = Budget::default();
        let error = apply_budget(&mut budget, "max_nothing", 1);
        assert_eq!(error, Err("unknown budget key: max_nothing".to_owned()));
    }

    #[test]
    fn oversized_narrow_budget_value_is_rejected() {
        let mut budget = Budget::default();
        assert!(apply_budget(&mut budget, "max_layouts", u64::MAX).is_err());
    }

    #[test]
    fn all_group_collects_every_group_in_order() {
        let items = select_text(&document(), "all", None).unwrap_or_default();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn layout_scope_drops_names_and_other_layouts() {
        let items = select_text(&document(), "all", Some("Sheet")).unwrap_or_default();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn unknown_group_is_rejected() {
        assert!(select_text(&document(), "bogus", None).is_err());
    }
}
