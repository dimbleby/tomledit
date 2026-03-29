use pyo3::exceptions::PyAttributeError;
use pyo3::prelude::*;

use crate::item_proxy::{ItemProxy, resolve_proxy};

/// A scalar TOML value (string, integer, float, boolean, datetime, date, or time).
#[pyclass(name = "ScalarItem", module = "tomledit", extends = ItemProxy)]
pub(crate) struct ScalarProxy;

/// Invoke a binary operator from Python's `operator` module (e.g. "add", "sub").
fn py_binop(
    py: Python<'_>,
    op: &str,
    lhs: &Bound<'_, PyAny>,
    rhs: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    py.import("operator")?
        .getattr(op)?
        .call1((lhs, rhs))
        .map(Bound::unbind)
}

/// Invoke a unary operator from Python's `operator` module (e.g. "neg", "pos").
fn py_unop(py: Python<'_>, op: &str, operand: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    py.import("operator")?
        .getattr(op)?
        .call1((operand,))
        .map(Bound::unbind)
}

impl ScalarProxy {
    /// Resolve the underlying Python value from the TOML document.
    fn resolve(self_: &PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        self_.as_super().value(self_.py())
    }
}

#[pymethods]
impl ScalarProxy {
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        crate::item_proxy::parse_as::<ScalarProxy>(py, text, "ScalarItem", "scalar")
    }

    // ---- attribute forwarding ----

    /// Forward attribute access to the underlying Python value.
    ///
    /// This makes scalar items feel like their native Python types:
    /// a string item supports `.upper()`, `.startswith()`, etc.; an int item
    /// supports `.bit_length()`; a datetime supports `.isoformat()`.
    ///
    /// Only triggered as a fallback — Item-level attributes like `.value`,
    /// `.comment`, and `.inline_comment` are resolved through normal lookup
    /// first and are never forwarded.
    fn __getattr__(self_: PyRef<'_, Self>, name: &str) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let py_value = Self::resolve(&self_)?;
        let bound = py_value.bind(py);
        bound.getattr(name).map(|a| a.unbind()).map_err(|_| {
            let type_name = bound
                .get_type()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "unknown".to_owned());
            PyAttributeError::new_err(format!(
                "'ScalarItem' wrapping {type_name} has no attribute '{name}'"
            ))
        })
    }

    // ---- containment ----

    fn __contains__(self_: PyRef<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = self_.py();
        let resolved = Self::resolve(&self_)?;
        let resolved_value = resolve_proxy(value)?;
        let value = resolved_value.as_ref().map_or(value, |v| v.bind(py));
        py_binop(py, "contains", resolved.bind(py), value)?.extract::<bool>(py)
    }

    // ---- comparison ----

    fn __eq__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        self_.as_super().__eq__(other)
    }

    fn __lt__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::resolve(&self_)?.bind(self_.py()).lt(other)
    }

    fn __le__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::resolve(&self_)?.bind(self_.py()).le(other)
    }

    fn __gt__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::resolve(&self_)?.bind(self_.py()).gt(other)
    }

    fn __ge__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        Self::resolve(&self_)?.bind(self_.py()).ge(other)
    }

    // ---- type conversion ----

    fn __int__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py.import("builtins")?
            .getattr("int")?
            .call1((val.bind(py),))
            .map(Bound::unbind)
    }

    fn __float__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py.import("builtins")?
            .getattr("float")?
            .call1((val.bind(py),))
            .map(Bound::unbind)
    }

    fn __index__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "index", val.bind(py))
    }

    fn __hash__(self_: PyRef<'_, Self>) -> PyResult<isize> {
        let py = self_.py();
        Self::resolve(&self_)?.bind(py).hash()
    }

    // ---- binary arithmetic ----

    fn __add__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "add", val.bind(py), other)
    }

    fn __radd__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "add", other, val.bind(py))
    }

    fn __sub__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "sub", val.bind(py), other)
    }

    fn __rsub__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "sub", other, val.bind(py))
    }

    fn __mul__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "mul", val.bind(py), other)
    }

    fn __rmul__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "mul", other, val.bind(py))
    }

    fn __truediv__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "truediv", val.bind(py), other)
    }

    fn __rtruediv__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "truediv", other, val.bind(py))
    }

    fn __floordiv__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "floordiv", val.bind(py), other)
    }

    fn __rfloordiv__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "floordiv", other, val.bind(py))
    }

    fn __mod__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "mod", val.bind(py), other)
    }

    fn __rmod__(self_: PyRef<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_binop(py, "mod", other, val.bind(py))
    }

    fn __pow__(
        self_: PyRef<'_, Self>,
        exp: &Bound<'_, PyAny>,
        modulo: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        let pow_fn = py.import("builtins")?.getattr("pow")?;
        match modulo {
            Some(m) => {
                let resolved_m = resolve_proxy(m)?;
                let m = resolved_m.as_ref().map_or(m, |v| v.bind(py));
                pow_fn.call1((val.bind(py), exp, m))
            }
            None => pow_fn.call1((val.bind(py), exp)),
        }
        .map(Bound::unbind)
    }

    fn __rpow__(
        self_: PyRef<'_, Self>,
        base: &Bound<'_, PyAny>,
        modulo: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        let pow_fn = py.import("builtins")?.getattr("pow")?;
        match modulo {
            Some(m) => {
                let resolved_m = resolve_proxy(m)?;
                let m = resolved_m.as_ref().map_or(m, |v| v.bind(py));
                pow_fn.call1((base, val.bind(py), m))
            }
            None => pow_fn.call1((base, val.bind(py))),
        }
        .map(Bound::unbind)
    }

    // ---- unary operators ----

    fn __neg__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "neg", val.bind(py))
    }

    fn __pos__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "pos", val.bind(py))
    }

    fn __abs__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "abs", val.bind(py))
    }

    fn __invert__(self_: PyRef<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        py_unop(py, "invert", val.bind(py))
    }

    // ---- formatting ----

    fn __format__(self_: PyRef<'_, Self>, spec: &str) -> PyResult<Py<PyAny>> {
        let py = self_.py();
        let val = Self::resolve(&self_)?;
        val.bind(py)
            .call_method1("__format__", (spec,))
            .map(|a| a.unbind())
    }
}
