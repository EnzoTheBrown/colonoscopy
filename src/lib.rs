mod server;
mod types;

use pyo3::prelude::*;
use server::set_probe;
use types::{ServiceStatus, StatusColor};

#[pymodule]
fn colonoscopy(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(set_probe, m)?)?;
    m.add_class::<StatusColor>()?;
    m.add_class::<ServiceStatus>()?;
    Ok(())
}
