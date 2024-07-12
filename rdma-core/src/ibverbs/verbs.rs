use rdma_core_sys::{ibv_cq, ibv_qp, ibv_recv_wr, ibv_send_wr, ibv_wc};

use crate::{RdmaErrors, Result};

pub fn ibv_poll_cq(cq: *mut ibv_cq, num_entries: i32, wc: &mut ibv_wc) -> Result<i32> {
    let ret = unsafe {
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
    return if ret > 0 {
        Ok(ret)
    } else {
        Err(RdmaErrors::OpsFailed("ibv_poll_cq".to_string(), ret))
    };
}


pub fn ibv_post_recv(qp: *mut ibv_qp, wr: *mut ibv_recv_wr, bad: *mut *mut ibv_recv_wr) -> Result<()> {
    let ret = unsafe {
        let ops = (*(*qp).context).ops;
        let post_recv = ops.post_recv.ok_or(RdmaErrors::OpsNotFound("ibv_post_recv".to_string()))?;
        post_recv(qp, wr, bad)
    };

    return if ret == 0 {
        Ok(())
    } else {
        Err(RdmaErrors::OpsFailed("ibv_post_recv".to_string(), ret))
    }
}

pub fn ibv_post_send(qp: *mut ibv_qp, wr: *mut ibv_send_wr, bad: *mut *mut ibv_send_wr) -> Result<()> {
    let ret = unsafe {
        let ops = (*(*qp).context).ops;
        let post_send = ops.post_send.ok_or(RdmaErrors::OpsNotFound("ibv_post_send".to_string()))?;
        post_send(qp, wr, bad)
    };

    return if ret == 0 {
        Ok(())
    } else {
        Err(RdmaErrors::OpsFailed("ibv_post_send".to_string(), ret))
    }
}
