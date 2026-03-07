use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use toml_edit::TomlError as TomlErrorRs;

create_exception!(tomledit, TomlError, PyException, "TomlError");

pub(crate) struct TomlErrorWrapper(TomlErrorRs);

impl From<TomlErrorRs> for TomlErrorWrapper {
    fn from(error: TomlErrorRs) -> Self {
        Self(error)
    }
}

impl From<TomlErrorWrapper> for PyErr {
    fn from(error: TomlErrorWrapper) -> Self {
        TomlError::new_err(error.0.message().to_owned())
    }
}
