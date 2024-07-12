use rdma_core_sys::{ibv_dereg_mr, ibv_mr, ibv_send_flags, rdma_addrinfo, rdma_cm_id, rdma_destroy_ep, rdma_disconnect, rdma_freeaddrinfo, IBV_SEND_INLINE};

#[derive(Clone, Default)]
pub struct RdmaDev {
    pub cm_id: Option<*mut rdma_cm_id>,
    pub send_mr: Option<*mut ibv_mr>,
    pub recv_mr: Option<*mut ibv_mr>,
    pub addr_info: Option<*mut rdma_addrinfo>,
    pub listen_id: Option<*mut rdma_cm_id>,
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
// req_id/size/block_sequence_number...

impl Drop for RdmaDev {
    fn drop(&mut self) {
        if self.is_connected {
            if let Some(cm_id) = self.cm_id {
                unsafe { rdma_disconnect(cm_id) };
            }
        }

        if self.send_flags & IBV_SEND_INLINE == 0 {
            if let Some(send_mr) = self.send_mr {
                unsafe { ibv_dereg_mr(send_mr) };
            }
        }

        if let Some(cm_id) = self.cm_id {
            unsafe { rdma_destroy_ep(cm_id) };
        }

        if let Some(listen_id) = self.listen_id {
            unsafe { rdma_destroy_ep(listen_id) };
        }

        if let Some(addr_info) = self.addr_info {
            unsafe { rdma_freeaddrinfo(addr_info) };
        }
    }
}


