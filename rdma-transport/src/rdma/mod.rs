use rdma_core::{ibverbs::IbvMr, rdma::{RdmaAddrInfo, RdmaCmId}};
use rdma_core_sys::ibv_send_flags;

mod server;
pub use server::serve;
mod client;
pub use client::connect;


#[derive(Clone, Default)]
pub struct RdmaDev {
    pub cm_id: Option<RdmaCmId>,
    pub send_mr: Option<IbvMr>,
    pub recv_mr: Option<IbvMr>,
    pub addr_info: Option<RdmaAddrInfo>,
    pub listen_id: Option<RdmaCmId>,
    pub is_connected: bool,
    pub send_flags: ibv_send_flags,
}

#[derive(Debug, Clone, Copy)]
pub struct Connection {
    pub addr: u64,
    pub rkey: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Notification {
    pub size: usize,
    pub done: usize,
}


