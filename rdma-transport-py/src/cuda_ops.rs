use pyo3::{exceptions::PyIOError, prelude::*};
use rdma_transport::cuda::{self, cuda_init_ctx, cuda_set_current_ctx, CudaMemBuffer};

/// Formats the sum of two numbers as string.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}

#[pyclass]
struct CudaMemBufferPy {
    ptr: u64,
    size: usize,
}

#[pymethods]
impl CudaMemBufferPy {
    fn __repr__(&self) -> String {
        format!("Cuda(addr: {}, size: {})", self.ptr, self.size)
    }

    fn __str__(&self) -> String {
        format!("Cuda({}, {})", self.ptr, self.size)
    }
}

impl From<cuda::CudaMemBuffer> for CudaMemBufferPy {
    fn from(value: cuda::CudaMemBuffer) -> Self {
        CudaMemBufferPy {
            ptr: value.get_ptr(),
            size: value.get_size(),
        }
    }
}

impl From<&CudaMemBufferPy> for CudaMemBuffer {
    fn from(value: &CudaMemBufferPy) -> Self {
        CudaMemBuffer::new(value.ptr, value.size)
    }
}

#[pyfunction]
fn cuda_mem_alloc(gpu_ordinal: i32, size: usize) -> PyResult<CudaMemBufferPy> {
    let mut cu_ctx = cuda_init_ctx(gpu_ordinal).map_err(|e| PyIOError::new_err(e.to_string()))?;
    cuda_set_current_ctx(&mut cu_ctx);
    let buffer = cuda::cuda_mem_alloc(size).map_err(|e| PyIOError::new_err(e.to_string()))?;
    Ok(buffer.into())
}

#[pyfunction]
fn cuda_host_to_device(host_buffer: &[u8], device_buffer: &CudaMemBufferPy) -> PyResult<()> {
    let device_buffer = CudaMemBuffer::new(device_buffer.ptr, device_buffer.size);
    cuda::cuda_host_to_device(&host_buffer, &device_buffer)
        .map_err(|e| PyIOError::new_err(e.to_string()))
}

#[pyfunction]
fn cuda_device_to_host(device_buffer: &CudaMemBufferPy, size: usize) -> PyResult<Vec<u8>> {
    let mut res: Vec<u8> = vec![0; size];
    let device_buffer = device_buffer.into();
    cuda::cuda_device_to_host(&device_buffer, &mut res)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    Ok(res)
}

#[pyfunction]
fn cuda_mem_free(device_buffer: &CudaMemBufferPy) -> PyResult<()> {
    let device_buffer = device_buffer.into();
    cuda::cuda_mem_free(&device_buffer).map_err(|e| PyIOError::new_err(e.to_string()))
}

/// A Python module implemented in Rust.
#[pymodule]
#[pyo3(name = "rdma_transport")]
fn rdma_transport_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    m.add_function(wrap_pyfunction!(cuda_mem_alloc, m)?)?;
    m.add_function(wrap_pyfunction!(cuda_host_to_device, m)?)?;
    m.add_function(wrap_pyfunction!(cuda_device_to_host, m)?)?;
    m.add_function(wrap_pyfunction!(cuda_mem_free, m)?)?;
    Ok(())
}
