use std::{
    ops::{Deref, DerefMut},
    slice,
};

pub const OFFSET_SLOTS: usize = 64;
pub const CPU_BUFFER_BASE_SIZE: usize = 4096; // 4KB
pub const CPU_BUFFER_SIZE: usize = CPU_BUFFER_BASE_SIZE * OFFSET_SLOTS;
pub const GPU_BUFFER_BASE_SIZE: usize = 1024 * 1024; // 1MB
pub const GPU_BUFFER_SIZE: usize = GPU_BUFFER_BASE_SIZE * OFFSET_SLOTS;

#[derive(Debug, Clone, Copy)]
pub struct GPUMemBuffer {
    ptr: u64,
    capacity: usize,
}

impl GPUMemBuffer {
    pub fn new(ptr: u64, size: usize) -> GPUMemBuffer {
        GPUMemBuffer {
            ptr,
            capacity: size,
        }
    }

    pub fn get_ptr(&self) -> u64 {
        self.ptr
    }

    pub fn get_capacity(&self) -> usize {
        self.capacity
    }
}

impl Deref for GPUMemBuffer {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        unsafe { slice::from_raw_parts(self.ptr as *mut u8, self.capacity) }
    }
}

impl DerefMut for GPUMemBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { slice::from_raw_parts_mut(self.ptr as *mut u8, self.capacity) }
    }
}

#[derive(Debug, Clone)]
pub struct MemBuffer {
    buffer: Box<[u8; CPU_BUFFER_SIZE]>,
}

impl MemBuffer {
    pub fn new() -> MemBuffer {
        MemBuffer {
            buffer: Box::new([0; CPU_BUFFER_SIZE]),
        }
    }

    pub fn get_ptr(&mut self) -> u64 {
        self.buffer.as_mut_ptr() as u64
    }

    pub fn get_capacity(&self) -> usize {
        self.buffer.len()
    }

    pub fn range(&self, start: usize, end: usize) -> Vec<u8> {
        self.buffer[start .. end].to_vec()
    }
}

impl Deref for MemBuffer {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.buffer.as_ref()
    }
}

impl DerefMut for MemBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer.as_mut()
    }
}

impl Default for MemBuffer {
    fn default() -> Self {
        MemBuffer::new()
    }
}
