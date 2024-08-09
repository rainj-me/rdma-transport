use rdma_core_sys::{htonl, ibv_recv_wr, ibv_send_wr, ibv_sge, IBV_WR_RDMA_READ, IBV_WR_RDMA_WRITE, IBV_WR_RDMA_WRITE_WITH_IMM, IBV_WR_SEND};
use std::ptr::{self, null_mut};

use crate::ibverbs::{ibv_post_recv, ibv_post_send, IbvMr};
use crate::{rdma::RdmaCmId, Result};

pub fn rdma_post_send<Context, Addr>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    addr: &mut Addr,
    length: usize,
    mr: Option<&mut IbvMr>,
    flags: u32,
) -> Result<()> {
    let mut sge = ibv_sge::default();
    sge.addr = addr as *mut _ as u64;
    sge.length = length as u32;
    sge.lkey = mr.map(|mr| mr.lkey).unwrap_or(0);

    rdma_post_sendv(id, context, &mut sge, 1, flags)
}

pub fn rdma_post_sendv<Context>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    sgl: *mut ibv_sge,
    nsge: i32,
    flags: u32,
) -> Result<()> {
    let mut wr = ibv_send_wr::default();
    wr.wr_id = context.map(|v| v as *mut _).unwrap_or(null_mut()) as u64;
    wr.next = ptr::null_mut();
    wr.sg_list = sgl;
    wr.num_sge = nsge;
    wr.opcode = IBV_WR_SEND;
    wr.send_flags = flags as u32;

    let mut bad: *mut ibv_send_wr = &mut ibv_send_wr::default();

    ibv_post_send(id.qp, &mut wr, &mut bad)
}

pub fn rdma_post_recv<Context>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    addr: u64,
    length: usize,
    mr: &mut IbvMr,
) -> Result<()> {
    let mut sge = ibv_sge::default();
    sge.addr = addr;
    sge.length = length as u32;
    sge.lkey = mr.lkey;

    rdma_post_recvv(id, context, &mut sge, 1)
}

pub fn rdma_post_recvv<Context>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    sgl: *mut ibv_sge,
    nsge: i32,
) -> Result<()> {
    let mut wr = ibv_recv_wr::default();
    wr.wr_id = context.map(|v| v as *mut _).unwrap_or(null_mut()) as u64;
    wr.next = ptr::null_mut();
    wr.sg_list = sgl;
    wr.num_sge = nsge;

    let mut bad: *mut ibv_recv_wr = &mut ibv_recv_wr::default();

    let qp = id.qp;
    ibv_post_recv(qp, &mut wr, &mut bad)
}

pub fn rdma_post_write<Context>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    addr: u64,
    length: usize,
    mr: Option<&mut IbvMr>,
    flags: u32,
    remote_addr: u64,
    rkey: u32,
) -> Result<()> {
    let mut sge = ibv_sge::default();
    sge.addr = addr as u64;
    sge.length = length as u32;
    sge.lkey = mr.map(|mr| mr.lkey).unwrap_or(0);

    rdma_post_writev(id, context, &mut sge, 1, flags, remote_addr, rkey)
}

pub fn rdma_post_writev<Context>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    sgl: *mut ibv_sge,
    nsge: i32,
    flags: u32,
    remote_addr: u64,
    rkey: u32,
) -> Result<()> {
    let mut wr = ibv_send_wr::default();
    wr.wr_id = context.map(|v| v as *mut _).unwrap_or(null_mut()) as u64;
    wr.next = ptr::null_mut();
    wr.sg_list = sgl;
    wr.num_sge = nsge;
    wr.opcode = IBV_WR_RDMA_WRITE;
    wr.send_flags = flags;
    wr.wr.rdma.remote_addr = remote_addr;
    wr.wr.rdma.rkey = rkey;

    let mut bad: *mut ibv_send_wr = &mut ibv_send_wr::default();

    ibv_post_send(id.qp, &mut wr, &mut bad)
}


pub fn rdma_post_write_with_opcode<Context>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    addr: u64,
    length: usize,
    mr: Option<&mut IbvMr>,
    flags: u32,
    remote_addr: u64,
    rkey: u32,
    opcode: u32,
    imm_data: u32,
) -> Result<()> {
    let mut sge = ibv_sge::default();
    sge.addr = addr;
    sge.length = length as u32;
    sge.lkey = mr.map(|mr| mr.lkey).unwrap_or(0);

    rdma_post_writev_with_opcode(id, context, &mut sge, 1, flags, remote_addr, rkey, opcode, imm_data)
}

pub fn rdma_post_writev_with_opcode<Context>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    sgl: *mut ibv_sge,
    nsge: i32,
    flags: u32,
    remote_addr: u64,
    rkey: u32,
    opcode: u32,
    imm_data: u32,
) -> Result<()> {
    let mut wr = ibv_send_wr::default();
    wr.wr_id = context.map(|v| v as *mut _).unwrap_or(null_mut()) as u64;
    wr.next = ptr::null_mut();
    wr.sg_list = sgl;
    wr.num_sge = nsge;
    wr.opcode = opcode;
    wr.send_flags = flags;
    wr.wr.rdma.remote_addr = remote_addr;
    wr.wr.rdma.rkey = rkey;

    if opcode == IBV_WR_RDMA_WRITE_WITH_IMM {
        wr.__bindgen_anon_1.imm_data = unsafe {htonl(imm_data) };
    }

    let mut bad: *mut ibv_send_wr = &mut ibv_send_wr::default();

    ibv_post_send(id.qp, &mut wr, &mut bad)
}

pub fn rdma_post_read<Context>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    addr: u64,
    length: usize,
    mr: Option<&mut IbvMr>,
    flags: u32,
    remote_addr: u64,
    rkey: u32,
) -> Result<()> {
    let mut sge = ibv_sge::default();
    sge.addr = addr as u64;
    sge.length = length as u32;
    sge.lkey = mr.map(|mr| mr.lkey).unwrap_or(0);

    rdma_post_readv(id, context, &mut sge, 1, flags, remote_addr, rkey)
}

pub fn rdma_post_readv<Context>(
    id: &mut RdmaCmId,
    context: Option<&mut Context>,
    sgl: *mut ibv_sge,
    nsge: i32,
    flags: u32,
    remote_addr: u64,
    rkey: u32,
) -> Result<()> {
    let mut wr = ibv_send_wr::default();
    wr.wr_id = context.map(|v| v as *mut _).unwrap_or(null_mut()) as u64;
    wr.next = ptr::null_mut();
    wr.sg_list = sgl;
    wr.num_sge = nsge;
    wr.opcode = IBV_WR_RDMA_READ;
    wr.send_flags = flags;
    wr.wr.rdma.remote_addr = remote_addr;
    wr.wr.rdma.rkey = rkey;

    let mut bad: *mut ibv_send_wr = &mut ibv_send_wr::default();

    ibv_post_send(id.qp, &mut wr, &mut bad)
}
