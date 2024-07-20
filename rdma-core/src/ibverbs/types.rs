use crate::rdma_type;

rdma_type!(IbvPd, rdma_core_sys::ibv_pd);
rdma_type!(IbvMr, rdma_core_sys::ibv_mr);

rdma_type!(IbvQp, rdma_core_sys::ibv_qp);
rdma_type!(IbvQpAttr, rdma_core_sys::ibv_qp_attr);
rdma_type!(IbvQpInitAttr, rdma_core_sys::ibv_qp_init_attr);
