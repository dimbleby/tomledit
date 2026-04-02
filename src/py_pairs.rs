use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

pub(crate) fn extract_pair<'py>(
    pair: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>)> {
    let mut iter = pair.try_iter()?;
    let key = iter
        .next()
        .transpose()?
        .ok_or_else(|| PyValueError::new_err("expected a length-2 iterable pair"))?;
    let value = iter
        .next()
        .transpose()?
        .ok_or_else(|| PyValueError::new_err("expected a length-2 iterable pair"))?;
    if iter.next().transpose()?.is_some() {
        return Err(PyValueError::new_err("expected a length-2 iterable pair"));
    }
    Ok((key, value))
}
