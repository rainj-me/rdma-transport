use std::{
    ffi::CString,
    ops::{Deref, DerefMut},
    ptr::null_mut,
};

use crate::{
    ibverbs::{IbvPd, IbvQpInitAttr},
    rdma::{RdmaAddrInfo, RdmaCmId, RdmaConnParam},
    rdma_call, RdmaErrors, Result,
};

pub fn rdma_getaddrinfo(node: &str, service: &str, hints: &RdmaAddrInfo) -> Result<RdmaAddrInfo> {
    let mut addr_info = null_mut();
    let node = CString::new(node).map_err(|_| RdmaErrors::InvalidAddress(node.to_string()))?;
    let service =
        CString::new(service).map_err(|_| RdmaErrors::InvalidAddress(service.to_string()))?;

    rdma_call!(
        rdma_getaddrinfo,
        rdma_core_sys::rdma_getaddrinfo(
            node.as_ptr(),
            service.as_ptr(),
            hints.deref(),
            &mut addr_info
        ),
        addr_info.into()
    )
}

pub fn rdma_create_ep(
    addr_info: &mut RdmaAddrInfo,
    pd: Option<&mut IbvPd>,
    qp_init_attr: Option<&mut IbvQpInitAttr>,
) -> Result<RdmaCmId> {
    let mut listen_id = null_mut();
    let pd = pd.map(|v| v.deref_mut() as *mut _).unwrap_or(null_mut());
    let qp_init_attr = qp_init_attr
        .map(|v| v.deref_mut() as *mut _)
        .unwrap_or(null_mut());

    rdma_call!(
        rdma_create_ep,
        rdma_core_sys::rdma_create_ep(&mut listen_id, addr_info.deref_mut(), pd, qp_init_attr),
        listen_id.into()
    )
}

pub fn rdma_listen(id: &mut RdmaCmId, backlog: i32) -> Result<()> {
    rdma_call!(
        rdma_listen,
        rdma_core_sys::rdma_listen(id.deref_mut(), backlog)
    )
}

pub fn rdma_get_request(listen: &mut RdmaCmId) -> Result<RdmaCmId> {
    let mut id = null_mut();
    rdma_call!(
        rdma_get_request,
        rdma_core_sys::rdma_get_request(listen.deref_mut(), &mut id),
        id.into()
    )
}

pub fn rdma_accept(id: &mut RdmaCmId, conn_param: Option<&mut RdmaConnParam>) -> Result<()> {
    let conn_param = conn_param
        .map(|v| v.deref_mut() as *mut _)
        .unwrap_or(null_mut());

    rdma_call!(
        rdma_accept,
        rdma_core_sys::rdma_accept(id.deref_mut(), conn_param)
    )
}

pub fn rdma_connect(id: &mut RdmaCmId, conn_param: Option<&mut RdmaConnParam>) -> Result<()> {
    let conn_param = conn_param
        .map(|v| v.deref_mut() as *mut _)
        .unwrap_or(null_mut());

    rdma_call!(
        rdma_connect,
        rdma_core_sys::rdma_connect(id.deref_mut(), conn_param)
    )
}

pub fn rdma_disconnect(id: &mut RdmaCmId) -> Result<()> {
    rdma_call!(
        rdma_disconnect,
        rdma_core_sys::rdma_disconnect(id.deref_mut())
    )
}
