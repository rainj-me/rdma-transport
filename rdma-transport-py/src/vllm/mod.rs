mod client;
mod server;

use std::ops::{Deref, DerefMut};

pub use client::VllmRdmaClient;
use pyo3::{pyclass, pymethods};
use rdma_transport::{
    rdma::{Connection, Notification},
    GPUMemBuffer,
};
pub use server::VllmRdmaServer;

#[pyclass]
#[derive(Debug, Clone, Default)]
pub struct TensorBlock {
    base_ptr: u64,
    offset: u64,
    size: u32,
}

#[pymethods]
impl TensorBlock {
    #[new]
    pub fn new(base_ptr: u64, offset: u64, size: u32) -> TensorBlock {
        TensorBlock {
            base_ptr,
            offset,
            size,
        }
    }

    pub fn set_base_ptr(&mut self, base_ptr: u64) {
        self.base_ptr = base_ptr
    }

    pub fn get_base_ptr(&self) -> u64 {
        self.base_ptr
    }

    pub fn set_offset(&mut self, offset: u64) {
        self.offset = offset
    }

    pub fn get_offset(&self) -> u64 {
        self.offset
    }

    pub fn set_size(&mut self, size: u32) {
        self.size = size
    }

    pub fn get_size(&self) -> u32 {
        self.size
    }
}

impl From<&Connection> for TensorBlock {
    fn from(conn: &Connection) -> Self {
        TensorBlock::new(conn.get_base_ptr(), 0, 0)
    }
}

impl From<&Notification> for TensorBlock {
    fn from(value: &Notification) -> Self {
        let (base_ptr, offset, size) = value.buffer;
        TensorBlock::new(base_ptr, offset, size)
    }
}

impl Into<GPUMemBuffer> for &TensorBlock {
    fn into(self) -> GPUMemBuffer {
        GPUMemBuffer::new(self.base_ptr, self.size as usize)
    }
}

#[pyclass]
#[derive(Debug, Clone, Default)]
pub struct TensorBlocks(Vec<TensorBlock>);

#[pymethods]
impl TensorBlocks {
    #[new]
    pub fn new() -> TensorBlocks {
        Default::default()
    }

    pub fn add(&mut self, tensor_block: TensorBlock) {
        self.0.push(tensor_block)
    }

    pub fn extends(&mut self, tensor_blocks: &mut TensorBlocks) {
        self.0.append(tensor_blocks.deref_mut())
    }

    pub fn get_base_ptrs(&self) -> Vec<u64> {
        self.0.deref().iter().map(|block| block.get_base_ptr()).collect()
    }
}

impl From<&Vec<TensorBlock>> for TensorBlocks {
    fn from(value: &Vec<TensorBlock>) -> Self {
        TensorBlocks(value.to_owned())
    }
}

impl From<Vec<TensorBlock>> for TensorBlocks {
    fn from(value: Vec<TensorBlock>) -> Self {
        TensorBlocks(value)
    }
}

impl Deref for TensorBlocks {
    type Target = Vec<TensorBlock>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TensorBlocks {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
