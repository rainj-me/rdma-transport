use std::{
    ops::{Deref, DerefMut},
    ptr, slice,
};

use cuda::cuda_call;
use cuda_sys::cuMemFree_v2;

pub struct CudaMemBuffer {
    ptr: u64,
    size: usize,
}

impl CudaMemBuffer {
    pub fn new(ptr: u64, size: usize) -> CudaMemBuffer {
        CudaMemBuffer { ptr, size }
    }

    pub fn get_ptr(&self) -> u64 {
        self.ptr
    }

    pub fn get_size(&self) -> usize {
        self.size
    }
}

impl Deref for CudaMemBuffer {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.ptr as *mut u8, self.size) }
    }
}

impl DerefMut for CudaMemBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.ptr as *mut u8, self.size) }
    }
}

impl Drop for CudaMemBuffer {
    fn drop(&mut self) {
        if self.ptr as *mut u8 != ptr::null_mut() {
            let _ = cuda_call!(cuMemFree, cuMemFree_v2(self.ptr));
        }
    }
}
