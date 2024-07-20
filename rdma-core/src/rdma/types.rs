use crate::rdma_type;

rdma_type!(RdmaAddrInfo, rdma_core_sys::rdma_addrinfo);
rdma_type!(RdmaCmId, rdma_core_sys::rdma_cm_id);
rdma_type!(RdmaConnParam, rdma_core_sys::rdma_conn_param);
