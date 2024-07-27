use crate::macros::cuda_type;

cuda_type!(CuCtx, cuda_sys::CUctx_st);
cuda_type!(CuEvent, cuda_sys::CUevent_st);
cuda_type!(CuStream, cuda_sys::CUstream_st);