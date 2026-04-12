use pyo3::exceptions::PyAttributeError;
use pyo3::prelude::*;

use crate::item_proxy::{ItemProxy, resolve_proxy};

/// A scalar TOML value (string, integer, float, boolean, datetime, date, or time).
#[pyclass(frozen, name = "ScalarItem", module = "tomledit", extends = ItemProxy)]
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

/// Resolve the underlying Python value from the TOML document.
fn resolve(slf: &Bound<'_, ScalarProxy>, py: Python<'_>) -> PyResult<Py<PyAny>> {
    slf.as_super().get().value(py)
}

#[pymethods]
impl ScalarProxy {
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        crate::item_proxy::parse_as::<ScalarProxy>(py, text, "ScalarItem", "scalar")
    }

    // ---- attribute forwarding ----

    fn __getattr__(slf: &Bound<'_, Self>, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        let py_value = resolve(slf, py)?;
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

    fn __contains__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let resolved = resolve(slf, py)?;
        let resolved_value = resolve_proxy(value)?;
        let value = resolved_value.as_ref().map_or(value, |v| v.bind(py));
        py_binop(py, "contains", resolved.bind(py), value)?.extract::<bool>(py)
    }

    // ---- comparison ----

    fn __eq__(slf: &Bound<'_, Self>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        slf.as_super().get().__eq__(other)
    }

    fn __lt__(slf: &Bound<'_, Self>, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        resolve(slf, py)?.bind(py).lt(other)
    }

    fn __le__(slf: &Bound<'_, Self>, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        resolve(slf, py)?.bind(py).le(other)
    }

    fn __gt__(slf: &Bound<'_, Self>, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        resolve(slf, py)?.bind(py).gt(other)
    }

    fn __ge__(slf: &Bound<'_, Self>, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        resolve(slf, py)?.bind(py).ge(other)
    }

    // ---- type conversion ----

    fn __int__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let val = resolve(slf, py)?;
        py.import("builtins")?
            .getattr("int")?
            .call1((val.bind(py),))
            .map(Bound::unbind)
    }

    fn __float__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let val = resolve(slf, py)?;
        py.import("builtins")?
            .getattr("float")?
            .call1((val.bind(py),))
            .map(Bound::unbind)
    }

    fn __index__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let val = resolve(slf, py)?;
        py_unop(py, "index", val.bind(py))
    }

    fn __hash__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<isize> {
        resolve(slf, py)?.bind(py).hash()
    }

    // ---- binary arithmetic ----

    fn __add__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "add", resolve(slf, py)?.bind(py), other)
    }

    fn __radd__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "add", other, resolve(slf, py)?.bind(py))
    }

    fn __sub__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "sub", resolve(slf, py)?.bind(py), other)
    }

    fn __rsub__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "sub", other, resolve(slf, py)?.bind(py))
    }

    fn __mul__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "mul", resolve(slf, py)?.bind(py), other)
    }

    fn __rmul__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "mul", other, resolve(slf, py)?.bind(py))
    }

    fn __truediv__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "truediv", resolve(slf, py)?.bind(py), other)
    }

    fn __rtruediv__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "truediv", other, resolve(slf, py)?.bind(py))
    }

    fn __floordiv__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "floordiv", resolve(slf, py)?.bind(py), other)
    }

    fn __rfloordiv__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "floordiv", other, resolve(slf, py)?.bind(py))
    }

    fn __mod__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "mod", resolve(slf, py)?.bind(py), other)
    }

    fn __rmod__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        py_binop(py, "mod", other, resolve(slf, py)?.bind(py))
    }

    fn __pow__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        exp: &Bound<'_, PyAny>,
        modulo: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let val = resolve(slf, py)?;
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
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        base: &Bound<'_, PyAny>,
        modulo: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let val = resolve(slf, py)?;
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

    fn __neg__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        py_unop(py, "neg", resolve(slf, py)?.bind(py))
    }

    fn __pos__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        py_unop(py, "pos", resolve(slf, py)?.bind(py))
    }

    fn __abs__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        py_unop(py, "abs", resolve(slf, py)?.bind(py))
    }

    fn __invert__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        py_unop(py, "invert", resolve(slf, py)?.bind(py))
    }

    // ---- formatting ----

    fn __format__(slf: &Bound<'_, Self>, py: Python<'_>, spec: &str) -> PyResult<Py<PyAny>> {
        let val = resolve(slf, py)?;
        val.bind(py)
            .call_method1("__format__", (spec,))
            .map(|a| a.unbind())
    }
}
