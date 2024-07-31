mod server;
mod client;

use pyo3::{pyclass, pymethods};
use rdma_transport::rdma::Notification;
pub use server::RdmaServer;
pub use client::RdmaClient;


#[pyclass]
pub struct Message {
    buffer: (u64, u32, u32),
    data: Vec<u8>
}

#[pymethods]
impl Message {
    pub fn get_buffer(&self) -> (u64, u32, u32) {
        self.buffer
    }

    pub fn get_data(&self) -> &[u8] {
        &self.data
    }
}

impl From<Notification> for Message {
    fn from(value: Notification) -> Self {
        Message {
            buffer: value.buffer,
            data: value.data
        }
    }
}
