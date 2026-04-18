use pyo3::exceptions::PyAttributeError;
use pyo3::prelude::*;
use pyo3::types::{PyFloat, PyInt};

use crate::item_proxy::{ItemProxy, resolve_proxy};

/// A scalar TOML value (string, integer, float, boolean, datetime, date, or time).
#[pyclass(frozen, name = "ScalarItem", module = "tomledit", extends = ItemProxy)]
pub(crate) struct ScalarProxy;

/// Resolve the underlying Python value from the TOML document.
fn resolve<'py>(slf: &Bound<'py, ScalarProxy>) -> PyResult<Bound<'py, PyAny>> {
    let py = slf.py();
    slf.as_super().get().value(py).map(|v| v.into_bound(py))
}

/// Resolve a `__pow__` modulo argument, unwrapping a proxy if present.
/// `None` becomes Python's `None` (which `PyAnyMethods::pow` treats as
/// "no modulo").
fn resolve_modulo<'py>(
    py: Python<'py>,
    modulo: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    match modulo {
        None => Ok(py.None().into_bound(py)),
        Some(m) => Ok(resolve_proxy(m)?.map_or_else(|| m.clone(), |v| v.into_bound(py))),
    }
}

#[pymethods]
impl ScalarProxy {
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        crate::item_proxy::parse_as::<ScalarProxy>(py, text, "ScalarItem", "scalar")
    }

    // ---- attribute forwarding ----

    fn __getattr__<'py>(slf: &Bound<'py, Self>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        let resolved = resolve(slf)?;
        resolved.getattr(name).map_err(|_| {
            let type_name = resolved
                .get_type()
                .name()
                .map_or_else(|_| "unknown".to_owned(), |n| n.to_string());
            PyAttributeError::new_err(format!(
                "'ScalarItem' wrapping {type_name} has no attribute '{name}'"
            ))
        })
    }

    // ---- containment ----

    fn __contains__(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let resolved = resolve(slf)?;
        let resolved_value = resolve_proxy(value)?;
        let value = resolved_value.as_ref().map_or(value, |v| v.bind(slf.py()));
        resolved.contains(value)
    }

    // ---- comparison ----

    fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        slf.as_super().get().__eq__(other)
    }

    fn __lt__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        resolve(slf)?.lt(other)
    }

    fn __le__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        resolve(slf)?.le(other)
    }

    fn __gt__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        resolve(slf)?.gt(other)
    }

    fn __ge__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        resolve(slf)?.ge(other)
    }

    // ---- type conversion ----

    fn __int__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        py.get_type::<PyInt>().call1((resolve(slf)?,))
    }

    fn __float__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        py.get_type::<PyFloat>().call1((resolve(slf)?,))
    }

    fn __index__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.call_method0("__index__")
    }

    fn __hash__(slf: &Bound<'_, Self>) -> PyResult<isize> {
        resolve(slf)?.hash()
    }

    // ---- binary arithmetic ----

    fn __add__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.add(other)
    }

    fn __radd__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        other.add(resolve(slf)?)
    }

    fn __sub__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.sub(other)
    }

    fn __rsub__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        other.sub(resolve(slf)?)
    }

    fn __mul__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.mul(other)
    }

    fn __rmul__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        other.mul(resolve(slf)?)
    }

    fn __truediv__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.div(other)
    }

    fn __rtruediv__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        other.div(resolve(slf)?)
    }

    fn __floordiv__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.floor_div(other)
    }

    fn __rfloordiv__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        other.floor_div(resolve(slf)?)
    }

    fn __mod__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.rem(other)
    }

    fn __rmod__<'py>(
        slf: &Bound<'py, Self>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        other.rem(resolve(slf)?)
    }

    fn __pow__<'py>(
        slf: &Bound<'py, Self>,
        exp: &Bound<'py, PyAny>,
        modulo: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let modulo = resolve_modulo(slf.py(), modulo)?;
        resolve(slf)?.pow(exp, &modulo)
    }

    fn __rpow__<'py>(
        slf: &Bound<'py, Self>,
        base: &Bound<'py, PyAny>,
        modulo: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let modulo = resolve_modulo(slf.py(), modulo)?;
        base.pow(resolve(slf)?, &modulo)
    }

    // ---- unary operators ----

    fn __neg__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.neg()
    }

    fn __pos__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.pos()
    }

    fn __abs__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.abs()
    }

    fn __invert__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.bitnot()
    }

    // ---- formatting ----

    fn __format__<'py>(slf: &Bound<'py, Self>, spec: &str) -> PyResult<Bound<'py, PyAny>> {
        resolve(slf)?.call_method1("__format__", (spec,))
    }
}
