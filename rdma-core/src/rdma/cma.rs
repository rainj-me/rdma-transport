use rdma_core_sys::{ibv_pd, ibv_qp_init_attr, rdma_addrinfo, rdma_cm_id, rdma_conn_param};
use std::{ffi::CString, ptr::null_mut};

use crate::{RdmaErrors, Result, macros::rdma_call};

pub fn rdma_getaddrinfo(
    node: &str,
    service: &str,
    hints: &rdma_addrinfo,
) -> Result<*mut rdma_addrinfo> {
    let mut addr_info: *mut rdma_addrinfo = null_mut();
    let node = CString::new(node).map_err(|_| RdmaErrors::InvalidAddress(node.to_string()))?;
    let service =
        CString::new(service).map_err(|_| RdmaErrors::InvalidAddress(service.to_string()))?;

    rdma_call!(rdma_getaddrinfo, rdma_core_sys::rdma_getaddrinfo(node.as_ptr(), service.as_ptr(), hints, &mut addr_info), addr_info)
}

pub fn rdma_create_ep(
    addr_info: *mut rdma_addrinfo,
    pd: Option<*mut ibv_pd>,
    qp_init_attr: Option<*mut ibv_qp_init_attr>,
) -> Result<*mut rdma_cm_id> {
    let mut listen_id: *mut rdma_cm_id = null_mut();
    let pd = pd.unwrap_or(null_mut());
    let qp_init_attr = qp_init_attr.unwrap_or(null_mut());

    rdma_call!(rdma_create_ep, rdma_core_sys::rdma_create_ep(&mut listen_id, addr_info, pd, qp_init_attr), listen_id)
}

pub fn rdma_listen(id: *mut rdma_cm_id, backlog: i32) -> Result<()> {
    rdma_call!(rdma_listen, rdma_core_sys::rdma_listen(id, backlog))
}

pub fn rdma_get_request(listen: *mut rdma_cm_id) -> Result<*mut rdma_cm_id> {
    let mut id: *mut rdma_cm_id = null_mut();
    rdma_call!(rdma_get_request, rdma_core_sys::rdma_get_request(listen, &mut id), id)
}

pub fn rdma_accept(id: *mut rdma_cm_id, conn_param: Option<*mut rdma_conn_param>) -> Result<()> {
    let conn_param = conn_param.unwrap_or(null_mut());

    rdma_call!(rdma_accept, rdma_core_sys::rdma_accept(id, conn_param))
}

pub fn rdma_connect(id: *mut rdma_cm_id, conn_param: Option<*mut rdma_conn_param>) -> Result<()> {
    let conn_param = conn_param.unwrap_or(null_mut());
    rdma_call!(rdma_connect, rdma_core_sys::rdma_connect(id, conn_param))
}