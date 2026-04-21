//! Module-level functions: `load`, `loads`, `dump`, `dumps`.

use pyo3::exceptions::{PyTypeError, PyUnicodeDecodeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use toml_edit::DocumentMut as DocumentRs;

use crate::document::Document;
use crate::value::Table;

/// Parse a TOML string into a `Document`, preserving formatting.
#[pyfunction]
pub(crate) fn loads(text: &str) -> PyResult<Document> {
    let document_rs = text
        .parse::<DocumentRs>()
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(Document::from_inner(document_rs))
}

/// Parse TOML from a file into a `Document`.
///
/// `fp` must be opened in binary mode (e.g. `open(path, "rb")`).
#[pyfunction]
pub(crate) fn load(py: Python<'_>, fp: &Bound<'_, PyAny>) -> PyResult<Document> {
    let data = fp.call_method0("read")?;
    let bytes = data
        .cast::<PyBytes>()
        .map_err(|_| PyTypeError::new_err("File must be opened in binary mode, e.g. use \"rb\""))?;
    let raw = bytes.as_bytes();
    let text = std::str::from_utf8(raw).map_err(|e| {
        PyUnicodeDecodeError::new_utf8(py, raw, e).map_or_else(PyErr::from, PyErr::from)
    })?;
    loads(text)
}

/// Serialise a `Document` or `Mapping` to a TOML string.
///
/// Passing a `Document` preserves its formatting; passing any other
/// mapping produces freshly-formatted output.
#[pyfunction]
pub(crate) fn dumps(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(doc) = obj.cast::<Document>() {
        return Ok(doc.get().as_toml(py));
    }
    let table: Table = obj.extract()?;
    Ok(DocumentRs::from(table.0).to_string())
}

/// Serialise a `Document` or `Mapping` to a file.
///
/// `fp` must be opened in binary mode (e.g. `open(path, "wb")`).
#[pyfunction]
pub(crate) fn dump(py: Python<'_>, obj: &Bound<'_, PyAny>, fp: &Bound<'_, PyAny>) -> PyResult<()> {
    let text = dumps(py, obj)?;
    let bytes = PyBytes::new(py, text.as_bytes());
    match fp.call_method1("write", (bytes,)) {
        Ok(_) => Ok(()),
        Err(e) if e.is_instance_of::<PyTypeError>(py) => Err(PyTypeError::new_err(
            "File must be opened in binary mode, e.g. use \"wb\"",
        )),
        Err(e) => Err(e),
    }
}
