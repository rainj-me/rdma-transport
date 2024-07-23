use std::net::SocketAddr;

use cuda::CuCtx;
use libc::AI_PASSIVE;

use rdma_core::ibverbs::{IbvMr, IbvQpInitAttr};
use rdma_core::rdma::{rdma_disconnect, RdmaAddrInfo, RdmaCmId};
use rdma_core::{
    ibverbs::{ibv_modify_qp, ibv_poll_cq, ibv_query_qp, ibv_reg_mr},
    rdma::{
        rdma_accept, rdma_create_ep, rdma_get_request, rdma_getaddrinfo, rdma_listen,
        rdma_post_recv, rdma_post_send,
    },
};
use rdma_core_sys::{
    ibv_qp_attr, ibv_wc, IBV_ACCESS_LOCAL_WRITE, IBV_ACCESS_REMOTE_READ, IBV_ACCESS_REMOTE_WRITE,
    IBV_QP_ACCESS_FLAGS, IBV_QP_CAP, IBV_SEND_INLINE, IBV_WC_SUCCESS, RDMA_PS_TCP,
};

use crate::cuda::{cuda_mem_alloc, cuda_mem_free, cuda_set_current_ctx, CudaMemBuffer};
use crate::{Result, TransportErrors};

use super::{Connection, Notification};

const BUFFER_SIZE: usize = 16 * 1024 * 1024;

pub async fn serve(bind_addr: SocketAddr, cu_ctx: CuCtx) -> Result<()> {
    let mut hints = RdmaAddrInfo::default();
    hints.ai_flags = AI_PASSIVE;
    hints.ai_port_space = RDMA_PS_TCP as i32;

    let mut addr_info = rdma_getaddrinfo(
        &bind_addr.ip().to_string(),
        &bind_addr.port().to_string(),
        &hints,
    )?;

    let mut qp_init_attr = IbvQpInitAttr::default();
    qp_init_attr.cap.max_send_wr = 1;
    qp_init_attr.cap.max_recv_wr = 1;
    qp_init_attr.cap.max_send_sge = 1;
    qp_init_attr.cap.max_recv_sge = 1;
    qp_init_attr.sq_sig_all = 1;

    let mut listen_id = rdma_create_ep(&mut addr_info, None, Some(&mut qp_init_attr))?;

    rdma_listen(&mut listen_id, 0)?;

    loop {
        let cm_id = rdma_get_request(&mut listen_id)?;
        let mut cu_ctx = cu_ctx.clone();
        tokio::spawn(async move {
            cuda_set_current_ctx(&mut cu_ctx).unwrap();
            let buffer: CudaMemBuffer = cuda_mem_alloc(BUFFER_SIZE).unwrap();
            let mut cm_id = cm_id;
            let mut buffer = buffer;
            let (mut mr, mut conn) = accept(&mut cm_id, &mut buffer).await.unwrap();
            handle_request(&mut cm_id, &mut mr, &mut conn)
                .await
                .unwrap();
            print_msg(&buffer).await.unwrap();
            cuda_mem_free(&buffer).unwrap();
        });
    }
}

async fn accept(cm_id: &mut RdmaCmId, buffer: &mut CudaMemBuffer) -> Result<(IbvMr, Connection)> {
    let qp = cm_id.qp;
    ibv_query_qp(qp, &mut ibv_qp_attr::default(), IBV_QP_CAP as i32, None)?;

    let mut mod_attr = ibv_qp_attr::default();
    mod_attr.qp_access_flags = IBV_ACCESS_REMOTE_READ | IBV_ACCESS_REMOTE_WRITE;
    ibv_modify_qp(qp, &mut mod_attr, IBV_QP_ACCESS_FLAGS as i32)?;

    let access = IBV_ACCESS_LOCAL_WRITE | IBV_ACCESS_REMOTE_WRITE | IBV_ACCESS_REMOTE_READ;
    let pd = cm_id.pd;
    let mut mr = ibv_reg_mr(pd, buffer, access as i32)?;

    let mut conn_client_info = Connection {
        addr: 0 as u64,
        rkey: 0,
    };

    rdma_post_recv(
        cm_id,
        None::<&mut u32>,
        &mut conn_client_info,
        std::mem::size_of::<Connection>(),
        &mut mr,
    )?;

    rdma_accept(cm_id, None)?;

    let mut wc = ibv_wc::default();
    let recv_cq = cm_id.recv_cq;
    ibv_poll_cq(recv_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "accept".to_string(),
            format!("poll_recv_comp failed with status: {:?}", wc.status),
        ));
    }

    let mut conn_server_info = Connection {
        addr: buffer.as_ptr() as u64,
        rkey: mr.rkey,
    };

    rdma_post_send(
        cm_id,
        None::<&mut u32>,
        &mut conn_server_info,
        std::mem::size_of::<Connection>(),
        None,
        IBV_SEND_INLINE,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "accept".to_string(),
            format!("poll_send_comp failed with status: {:?}", wc.status),
        ));
    }

    Ok((mr, conn_client_info))
}

pub async fn handle_request(
    cm_id: &mut RdmaCmId,
    mr: &mut IbvMr,
    _conn: &mut Connection,
) -> Result<()> {
    let mut notification = Notification { size: 0, done: 0 };

    loop {
        rdma_post_recv(
            cm_id,
            None::<&mut u32>,
            &mut notification,
            std::mem::size_of::<Notification>(),
            // rdma_dev.recv_mr.as_mut().unwrap(),
            mr,
        )?;

        let mut wc = ibv_wc::default();
        let recv_cq = cm_id.recv_cq;
        ibv_poll_cq(recv_cq, 1, &mut wc)?;
        if wc.status != IBV_WC_SUCCESS {
            return Err(TransportErrors::OpsFailed(
                "handle_request".to_string(),
                format!("poll_recv_comp failed with status: {:?}", wc.status),
            ));
        }
        if notification.done > 0 {
            rdma_disconnect(cm_id)?;
            break;
        }
    }

    Ok(())
}

pub async fn print_msg(buffer: &CudaMemBuffer) -> Result<()> {

    let mut buffer_cpu: Vec<u8> = vec![0; 100];

    println!("before {:?}", &buffer_cpu);

    crate::cuda::cuda_device_to_host(buffer, &mut buffer_cpu)?;

    println!("after {:?}", String::from_utf8_lossy(&buffer_cpu));

    Ok(())
}