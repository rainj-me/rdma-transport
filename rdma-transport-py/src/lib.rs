use pyo3::prelude::*;
use threading::{RdmaClient, RdmaServer};

mod threading;

/// Formats the sum of two numbers as string.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}

/// A Python module implemented in Rust.
#[pymodule]
#[pyo3(name = "rdma_transport")]
fn rdma_transport_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();

    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    m.add_class::<RdmaServer>()?;
    m.add_class::<RdmaClient>()?;
    Ok(())
}
