//! Python bindings for cadboss.

#![deny(unsafe_code)]

use pyo3::prelude::*;

#[pymodule]
fn _cadboss(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))
}
