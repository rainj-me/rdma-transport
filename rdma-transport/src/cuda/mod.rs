use std::{ops::DerefMut, ptr};

use cuda::{cuda_call, CuCtx, CuEvent, CuStream};
use cuda_sys::{
    cuCtxCreate_v2, cuCtxSetCurrent, cuDeviceGet, cuDevicePrimaryCtxRelease_v2, cuDevicePrimaryCtxRetain, cuEventCreate, cuEventQuery, cuInit, cuMemAlloc_v2, cuMemFree_v2, cuMemcpyDtoH_v2_ptds as cuMemcpyDtoH_v2, cuMemcpyHtoD_v2_ptds as cuMemcpyHtoD_v2, cuStreamCreate, cuStreamWaitEvent_ptsz, CU_CTX_MAP_HOST, CU_EVENT_DISABLE_TIMING, CU_EVENT_WAIT_DEFAULT, CU_STREAM_NON_BLOCKING
};

use crate::{GPUMemBuffer, Result};

pub fn cuda_init_ctx(gpu_ordinal: i32) -> Result<CuCtx> {
    let mut cu_dev = 0;
    let mut cu_ctx = ptr::null_mut();

    cuda_call!(cuInit, cuInit(0))?;
    cuda_call!(cuDeviceGet, cuDeviceGet(&mut cu_dev, gpu_ordinal))?;
    cuda_call!(
        cuCtxCreate_v2,
        cuCtxCreate_v2(&mut cu_ctx, CU_CTX_MAP_HOST, cu_dev)
    )?;
    Ok(CuCtx::new(cu_ctx))
}

pub fn cuda_device_primary_ctx_retain(gpu_ordinal: i32) -> Result<CuCtx> {
    let mut cu_dev = 0;
    let mut cu_ctx = ptr::null_mut();
    cuda_call!(cuDeviceGet, cuDeviceGet(&mut cu_dev, gpu_ordinal))?;
    cuda_call!(
        cuDevicePrimaryCtxRetain,
        cuDevicePrimaryCtxRetain(&mut cu_ctx, cu_dev)
    )?;
    Ok(CuCtx::new(cu_ctx))
}

pub fn cuda_device_primary_ctx_release(gpu_ordinal: i32) -> Result<()> {
    let mut cu_dev = 0;
    cuda_call!(cuDeviceGet, cuDeviceGet(&mut cu_dev, gpu_ordinal))?;
    cuda_call!(
        cuDevicePrimaryCtxRelease_v2,
        cuDevicePrimaryCtxRelease_v2(cu_dev)
    )?;
    Ok(())
}

pub fn cuda_set_current_ctx(cu_ctx: &mut CuCtx) -> Result<()> {
    let cu_ctx: *mut cuda_sys::CUctx_st = cu_ctx.deref_mut();
    cuda_call!(cuCtxSetCurrent, cuCtxSetCurrent(cu_ctx)).map_err(|e| e.into())
}

pub fn cuda_mem_alloc(size: usize) -> Result<GPUMemBuffer> {
    let mut cu_mem_ptr: u64 = 0;
    cuda_call!(cuMemAlloc_v2, cuMemAlloc_v2(&mut cu_mem_ptr, size))?;
    Ok(GPUMemBuffer::new(cu_mem_ptr, size))
}

pub fn cuda_mem_free(buffer: &GPUMemBuffer) -> Result<()> {
    let ptr = buffer.get_base_ptr();
    if ptr as *mut u8 == ptr::null_mut() {
        return Ok(());
    }

    cuda_call!(cuMemFree_v2, cuMemFree_v2(ptr)).map_err(|e| e.into())
}

pub fn cuda_host_to_device(host_buffer: &[u8], device_buffer: &GPUMemBuffer) -> Result<()> {
    let size = if host_buffer.len() > device_buffer.get_size() {
        device_buffer.get_size()
    } else {
        host_buffer.len()
    };

    cuda_call!(
        cuMemcpyHtoD_v2,
        cuMemcpyHtoD_v2(
            device_buffer.get_base_ptr(),
            host_buffer.as_ptr() as *const std::ffi::c_void,
            size,
        )
    )
    .map_err(|e| e.into())
}

pub fn cuda_device_to_host(device_buffer: &GPUMemBuffer, host_buffer: &mut [u8], size: Option<usize>) -> Result<()> {
    let size = match size {
        Some(size) => size.min(host_buffer.len()).min(device_buffer.get_size()),
        None => host_buffer.len().min(device_buffer.get_size())
    };

    cuda_call!(
        cuMemcpyDtoH_v2,
        cuMemcpyDtoH_v2(
            host_buffer.as_mut_ptr() as *mut std::ffi::c_void,
            device_buffer.get_base_ptr(),
            size,
        )
    )
    .map_err(|e| e.into())
}

pub fn cuda_create_stream() -> Result<CuStream> {
    let mut cu_stream = ptr::null_mut();

    cuda_call!(
        cuStreamCreate,
        cuStreamCreate(&mut cu_stream, CU_STREAM_NON_BLOCKING)
    )?;
    Ok(CuStream::new(cu_stream))
}

pub fn cuda_create_event() -> Result<CuEvent> {
    let mut cu_event = ptr::null_mut();
    cuda_call!(
        cuEventCreate,
        cuEventCreate(&mut cu_event, CU_EVENT_DISABLE_TIMING)
    )?;
    Ok(CuEvent::new(cu_event))
}

pub fn cuda_query_event(event: &mut CuEvent) -> Result<bool> {
    let ret = unsafe {
        cuEventQuery(event.deref_mut())
    };
    if ret == cuda_sys::CUDA_SUCCESS {
        Ok(true)
    } else if ret == cuda_sys::CUDA_ERROR_NOT_READY {
        Ok(false)
    } else {
        Err(crate::TransportErrors::OpsFailed("cuEventQuery".to_string(), ret.to_string()))
    }
}

pub fn cuda_wait_evnet(event: &mut CuEvent, stream: &mut CuStream) -> Result<()> {
    cuda_call!(
        cuStreamWaitEvent,
        cuStreamWaitEvent_ptsz(stream.deref_mut(), event.deref_mut(), CU_EVENT_WAIT_DEFAULT)
    )?;
    Ok(())
}

// pub fn cuda_create_event(event: u64)  -> Result<()> {
//     cuda_call!(
//         cuEventCreate,
//         cuEventCreate(
//             host_buffer.as_mut_ptr() as *mut std::ffi::c_void,
//             device_buffer.get_ptr(),
//             size,
//         )
//     )
//     .map_err(|e| e.into())
// }

// pub fn cuda_wait_event(event: u64) -> Result<()> {
//     cuda_call!(
//         cuMemcpyDtoH_v2,
//         cuMemcpyDtoH_v2(
//             host_buffer.as_mut_ptr() as *mut std::ffi::c_void,
//             device_buffer.get_ptr(),
//             size,
//         )
//     )
//     .map_err(|e| e.into())
// }
