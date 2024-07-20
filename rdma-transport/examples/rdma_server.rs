use std::net::SocketAddr;

use anyhow::{anyhow, Result};

use libc::AI_PASSIVE;

use rdma_core::ibverbs::IbvQpInitAttr;
use rdma_core::rdma::RdmaAddrInfo;
use rdma_core::{
    ibverbs::{ibv_modify_qp, ibv_poll_cq, ibv_query_qp, ibv_reg_mr},
    rdma::{
        rdma_accept, rdma_create_ep, rdma_disconnect, rdma_get_request, rdma_getaddrinfo,
        rdma_listen, rdma_post_recv, rdma_post_send,
    },
};
use rdma_core_sys::{
    ibv_qp_attr, ibv_wc, IBV_ACCESS_LOCAL_WRITE, IBV_ACCESS_REMOTE_READ, IBV_ACCESS_REMOTE_WRITE,
    IBV_QP_ACCESS_FLAGS, IBV_QP_CAP, IBV_SEND_INLINE, IBV_WC_SUCCESS, RDMA_PS_TCP,
};
use rdma_transport::cuda::{cuda_device_to_host, cuda_mem_alloc};
use rdma_transport::rdma::{Connection, Notification, RdmaDev};

const BUFFER_SIZE: usize = 16 * 1024 * 1024;

pub fn main() -> Result<()> {
    let bind_addr = "192.168.14.224:23456".parse::<SocketAddr>()?;
    let gpu_ordinal = 4;

    let mut buffer = cuda_mem_alloc(gpu_ordinal, BUFFER_SIZE)?;
    let mut rdma_dev = RdmaDev::default();
    let mut hints = RdmaAddrInfo::default();
    hints.ai_flags = AI_PASSIVE;
    hints.ai_port_space = RDMA_PS_TCP as i32;

    let mut addr_info = rdma_getaddrinfo(
        &bind_addr.ip().to_string(),
        &bind_addr.port().to_string(),
        &hints,
    )?;
    rdma_dev.addr_info = Some(addr_info.clone());

    let mut qp_init_attr = IbvQpInitAttr::default();
    qp_init_attr.cap.max_send_wr = 1;
    qp_init_attr.cap.max_recv_wr = 1;
    qp_init_attr.cap.max_send_sge = 1;
    qp_init_attr.cap.max_recv_sge = 1;
    qp_init_attr.sq_sig_all = 1;

    let mut listen_id = rdma_create_ep(&mut addr_info, None, Some(&mut qp_init_attr))?;
    rdma_dev.listen_id = Some(listen_id.clone());

    rdma_listen(&mut listen_id, 0)?;

    let mut cm_id = rdma_get_request(&mut listen_id)?;
    rdma_dev.cm_id = Some(cm_id.clone());

    let qp = cm_id.qp;
    ibv_query_qp(qp, &mut ibv_qp_attr::default(), IBV_QP_CAP as i32, None)?;

    let mut mod_attr = ibv_qp_attr::default();
    mod_attr.qp_access_flags = IBV_ACCESS_REMOTE_READ | IBV_ACCESS_REMOTE_WRITE;
    ibv_modify_qp(qp, &mut mod_attr, IBV_QP_ACCESS_FLAGS as i32)?;

    rdma_dev.send_flags = IBV_SEND_INLINE;

    let access = IBV_ACCESS_LOCAL_WRITE | IBV_ACCESS_REMOTE_WRITE | IBV_ACCESS_REMOTE_READ;
    let pd = cm_id.pd;
    let mr = ibv_reg_mr(pd, &mut buffer, access as i32)?;
    rdma_dev.recv_mr = Some(mr.clone());

    let mut client_conn = Connection {
        addr: 0 as u64,
        rkey: 0,
    };

    rdma_post_recv(
        &mut cm_id,
        None::<&mut u32>,
        &mut client_conn,
        std::mem::size_of::<Connection>(),
        rdma_dev.recv_mr.as_mut().unwrap(),
    )?;

    rdma_accept(&mut cm_id, None)?;

    let mut wc = ibv_wc::default();
    let recv_cq = cm_id.recv_cq;
    ibv_poll_cq(recv_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(anyhow!(
            "poll_send_comp failed with status: {:?}",
            wc.status
        ));
    }

    let mut conn = Connection {
        addr: buffer.as_ptr() as u64,
        rkey: mr.rkey,
    };

    rdma_post_send(
        &mut cm_id,
        None::<&mut u32>,
        &mut conn,
        std::mem::size_of::<Connection>(),
        rdma_dev.send_mr.as_mut(),
        rdma_dev.send_flags,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(anyhow!(
            "poll_send_comp failed with errorno: {:?}",
            wc.status
        ));
    }

    let mut notification = Notification { size: 0, done: 0 };

    loop {
        rdma_post_recv(
            &mut cm_id,
            None::<&mut u32>,
            &mut notification,
            std::mem::size_of::<Notification>(),
            rdma_dev.recv_mr.as_mut().unwrap(),
        )?;

        let mut wc = ibv_wc::default();
        let recv_cq = cm_id.recv_cq;
        ibv_poll_cq(recv_cq, 1, &mut wc)?;
        if wc.status != IBV_WC_SUCCESS {
            return Err(anyhow!(
                "poll_recv_comp failed with errorno: {:?}",
                wc.status
            ));
        }
        if notification.done > 0 {
            rdma_disconnect(&mut cm_id)?;
            break;
        }
    }

    let mut buffer_cpu: Vec<u8> = vec![0; 100];

    println!("before {:?}", &buffer_cpu);

    cuda_device_to_host(&buffer, &mut buffer_cpu)?;

    println!("after {:?}", String::from_utf8_lossy(&buffer_cpu));

    Ok(())
}
