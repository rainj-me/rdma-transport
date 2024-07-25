use std::{ffi::c_void, ops::DerefMut, ptr::null_mut};

use rdma_core_sys::{
    ibv_cq, ibv_pd, ibv_qp, ibv_qp_attr, ibv_qp_init_attr, ibv_recv_wr, ibv_send_wr, ibv_wc,
};

use crate::{macros::rdma_call, RdmaErrors, Result};

use super::IbvMr;

pub fn ibv_poll_cq(cq: *mut ibv_cq, num_entries: i32, wc: &mut ibv_wc) -> Result<i32> {
    let entries = unsafe {
        let ops = (*(*cq).context).ops;
        let poll_cq = ops
            .poll_cq
            .ok_or(RdmaErrors::OpsNotFound("ibv_poll_cq".to_string()))?;
        loop {
            let ret = poll_cq(cq, num_entries, wc);
            if ret != 0 {
                break ret;
            }
        }
    };

    return if entries > 0 {
        Ok(entries)
    } else {
        Err(RdmaErrors::OpsFailed("ibv_poll_cq".to_string(), entries))
    };
}

pub fn ibv_post_recv(
    qp: *mut ibv_qp,
    wr: *mut ibv_recv_wr,
    bad: *mut *mut ibv_recv_wr,
) -> Result<()> {
    let post_recv = unsafe { (*(*qp).context).ops.post_recv }
        .ok_or(RdmaErrors::OpsNotFound("ibv_post_recv".to_string()))?;

    rdma_call!(ibv_post_recv, post_recv(qp, wr, bad))
}

pub fn ibv_post_send(
    qp: *mut ibv_qp,
    wr: *mut ibv_send_wr,
    bad: *mut *mut ibv_send_wr,
) -> Result<()> {
    let post_send = unsafe { (*(*qp).context).ops.post_send }
        .ok_or(RdmaErrors::OpsNotFound("ibv_post_send".to_string()))?;

    rdma_call!(ibv_post_send, post_send(qp, wr, bad))
}

pub fn ibv_query_qp(
    qp: *mut ibv_qp,
    attr: *mut ibv_qp_attr,
    attr_mask: i32,
    init_attr: Option<*mut ibv_qp_init_attr>,
) -> Result<()> {
    let init_attr = init_attr.unwrap_or(&mut ibv_qp_init_attr::default());

    rdma_call!(
        ibv_query_qp,
        rdma_core_sys::ibv_query_qp(qp, attr, attr_mask, init_attr)
    )
}

pub fn ibv_modify_qp(qp: *mut ibv_qp, attr: *mut ibv_qp_attr, attr_mask: i32) -> Result<()> {
    rdma_call!(
        ibv_modify_qp,
        rdma_core_sys::ibv_modify_qp(qp, attr, attr_mask)
    )
}

pub fn ibv_reg_mr(pd: *mut ibv_pd, buffer: &mut [u8], access: i32) -> Result<IbvMr> {
    let buffer_ptr = buffer.as_mut_ptr() as *mut c_void;
    let mr = unsafe { rdma_core_sys::ibv_reg_mr(pd, buffer_ptr, buffer.len(), access) };
    if mr != null_mut() {
        Ok(mr.into())
    } else {
        Err(RdmaErrors::OpsFailed("ibv_reg_mr".to_string(), -1))
    }
}

pub fn ibv_dereg_mr(mr: &mut IbvMr) -> Result<()> {
    let mr = mr.deref_mut() as *mut _;
    if mr == null_mut() {
        return Ok(());
    }
    rdma_call!(ibv_dereg_mr, rdma_core_sys::ibv_dereg_mr(mr))
}
