use rdma_core_sys::{
    ibv_mr, ibv_recv_wr, ibv_send_wr, ibv_sge, rdma_cm_id, IBV_WR_RDMA_WRITE, IBV_WR_SEND,
};
use std::ptr;

use crate::ibverbs::{ibv_post_recv, ibv_post_send};
use crate::Result;

pub fn rdma_post_send<Context, Addr>(
    id: *mut rdma_cm_id,
    context: *mut Context,
    addr: *mut Addr,
    length: usize,
    mr: Option<*mut ibv_mr>,
    flags: u32,
) -> Result<()> {
    let mut sge = ibv_sge::default();
    sge.addr = addr as u64;
    sge.length = length as u32;
    sge.lkey = mr.map(|mr| unsafe { (*mr).lkey }).unwrap_or(0);

    let mut wr = ibv_send_wr::default();
    wr.wr_id = context as u64;
    wr.next = ptr::null_mut();
    wr.sg_list = &mut sge;
    wr.num_sge = 1;
    wr.opcode = IBV_WR_SEND;
    wr.send_flags = flags as u32;

    let mut bad: *mut ibv_send_wr = &mut ibv_send_wr::default();

    let qp = unsafe { (*id).qp };
    ibv_post_send(qp, &mut wr, &mut bad)
}

pub fn rdma_post_recv<Context, Addr>(
    id: *mut rdma_cm_id,
    context: *mut Context,
    addr: *mut Addr,
    length: usize,
    mr: *mut ibv_mr,
) -> Result<()> {
    let mut sge = ibv_sge::default();
    sge.addr = addr as u64;
    sge.length = length as u32;
    sge.lkey = unsafe { (*mr).lkey };

    let mut wr = ibv_recv_wr::default();
    wr.wr_id = context as u64;
    wr.next = ptr::null_mut();
    wr.sg_list = &mut sge;
    wr.num_sge = 1;

    let mut bad: *mut ibv_recv_wr = &mut ibv_recv_wr::default();

    let qp = unsafe { (*id).qp };
    ibv_post_recv(qp, &mut wr, &mut bad)
}

pub fn rdma_post_write<Context, Addr>(
    id: *mut rdma_cm_id,
    context: *mut Context,
    addr: *mut Addr,
    length: usize,
    mr: Option<*mut ibv_mr>,
    flags: u32,
    remote_addr: u64,
    rkey: u32,
) -> Result<()> {
    let mut sge = ibv_sge::default();
    sge.addr = addr as u64;
    sge.length = length as u32;
    sge.lkey = mr.map(|mr| unsafe { (*mr).lkey }).unwrap_or(0);

    let mut wr = ibv_send_wr::default();
    wr.wr_id = context as u64;
    wr.next = ptr::null_mut();
    wr.sg_list = &mut sge;
    wr.num_sge = 1;
    wr.opcode = IBV_WR_RDMA_WRITE;
    wr.send_flags = flags;
    wr.wr.rdma.remote_addr = remote_addr;
    wr.wr.rdma.rkey = rkey;

    let mut bad: *mut ibv_send_wr = &mut ibv_send_wr::default();

    let qp = unsafe { (*id).qp };
    ibv_post_send(qp, &mut wr, &mut bad)
}
