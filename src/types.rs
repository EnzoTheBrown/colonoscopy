use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde::Serialize;

#[pyclass]
#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "UPPERCASE")]
pub enum StatusColor {
    Red,
    Orange,
    Green,
}

#[pyclass]
#[derive(Serialize, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub status: StatusColor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub subservices: Vec<ServiceStatus>,
}

#[pymethods]
impl ServiceStatus {
    #[new]
    #[pyo3(signature = (name, status, description=None, subservices=None))]
    fn new(
        name: String,
        status: StatusColor,
        description: Option<String>,
        subservices: Option<Vec<ServiceStatus>>,
    ) -> Self {
        Self {
            name,
            status,
            description,
            subservices: subservices.unwrap_or_default(),
        }
    }
}

pub fn py_status_to_rust(color: &str) -> StatusColor {
    match color {
        "GREEN" => StatusColor::Green,
        "ORANGE" => StatusColor::Orange,
        _ => StatusColor::Red,
    }
}

pub fn dict_to_status(dict: &PyDict) -> PyResult<ServiceStatus> {
    let name: String = dict
        .get_item("name")?
        .ok_or_else(|| PyKeyError::new_err("name"))?
        .extract()?;
    let status_str: String = dict
        .get_item("status")?
        .ok_or_else(|| PyKeyError::new_err("status"))?
        .extract()?;
    let description: Option<String> = dict
        .get_item("description")?
        .map(|d| d.extract())
        .transpose()?;

    Ok(ServiceStatus {
        name,
        status: py_status_to_rust(&status_str),
        description,
        subservices: Vec::new(),
    })
}

impl<'a> std::convert::TryFrom<&'a pyo3::PyAny> for ServiceStatus {
    type Error = PyErr;
    fn try_from(obj: &'a pyo3::PyAny) -> PyResult<Self> {
        if let Ok(s) = obj.extract::<ServiceStatus>() {
            return Ok(s);
        }
        let dict: &PyDict = obj.downcast()?;
        dict_to_status(dict)
    }
}
