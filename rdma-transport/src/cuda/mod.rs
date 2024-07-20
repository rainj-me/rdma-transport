use std::ptr;

use cuda::cuda_call;
use cuda_sys::{
    cuCtxCreate_v2, cuCtxSetCurrent, cuDeviceGet, cuInit, cuMemAlloc_v2, cuMemcpyDtoH_v2, cuMemcpyHtoD_v2, CU_CTX_MAP_HOST
};

mod buffer;

use crate::Result;
pub use buffer::CudaMemBuffer;

pub fn cuda_mem_alloc(gpu_ordinal: i32, size: usize) -> Result<CudaMemBuffer> {
    let mut cu_dev = 0;
    let mut cu_ctx = ptr::null_mut();
    let mut cu_mem_ptr: u64 = 0;

    cuda_call!(cuInit, cuInit(0))?;
    cuda_call!(cuDeviceGet, cuDeviceGet(&mut cu_dev, gpu_ordinal))?;
    cuda_call!(
        cuCtxCreate_v2,
        cuCtxCreate_v2(&mut cu_ctx, CU_CTX_MAP_HOST, cu_dev)
    )?;
    cuda_call!(cuCtxSetCurrent, cuCtxSetCurrent(cu_ctx))?;
    cuda_call!(cuMemAlloc_v2, cuMemAlloc_v2(&mut cu_mem_ptr, size))?;

    Ok(CudaMemBuffer::new(cu_mem_ptr, size))
}

pub fn cuda_host_to_device(host_buffer: &[u8], device_buffer: &CudaMemBuffer) -> Result<()> {
    let size = if host_buffer.len() > device_buffer.get_size() {
        device_buffer.get_size()
    } else {
        host_buffer.len()
    };

    cuda_call!(
        cuMemcpyHtoD_v2,
        cuMemcpyHtoD_v2(
            device_buffer.get_ptr(),
            host_buffer.as_ptr() as *const std::ffi::c_void,
            size,
        )
    ).map_err(|e|e.into())
}

pub fn cuda_device_to_host(device_buffer: &CudaMemBuffer, host_buffer: &mut [u8]) -> Result<()> {
    let size = if host_buffer.len() > device_buffer.get_size() {
        device_buffer.get_size()
    } else {
        host_buffer.len()
    };

    cuda_call!(
        cuMemcpyDtoH_v2,
        cuMemcpyDtoH_v2(
            host_buffer.as_mut_ptr() as *mut std::ffi::c_void,
            device_buffer.get_ptr(),
            size,
        )
    ).map_err(|e|e.into())
}
