//! Python bindings for cadboss.

use pyo3::prelude::*;

#[pymodule]
fn _cadboss(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))
}
