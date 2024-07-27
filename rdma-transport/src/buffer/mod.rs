use std::{
    ops::{Deref, DerefMut},
    slice,
};

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
    buffer: Box<[u8; 4096]>,
}

impl MemBuffer {
    pub fn new() -> MemBuffer {
        MemBuffer {
            buffer: Box::new([0; 4096]),
        }
    }

    pub fn get_ptr(&mut self) -> u64 {
        self.buffer.as_mut_ptr() as u64
    }

    pub fn get_capacity(&self) -> usize {
        self.buffer.len()
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
