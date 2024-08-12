from rdma_transport import VllmRdmaClient as RdmaClient, TensorBlocks, TensorBlock
import logging
import sys
import time
import torch
import argparse
import asyncio

FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
logging.basicConfig(stream=sys.stdout,format=FORMAT, level=logging.DEBUG)

def main():
    # Create the parser
    parser = argparse.ArgumentParser(description="A simple script to greet the user.")
    
    # Add arguments
    parser.add_argument("--server_addr", type=str, help="")
    parser.add_argument("--msg", type=str, help="")
    parser.add_argument("--gpu_ordinal", type=int, help="")
    
    # Parse the arguments
    args = parser.parse_args()

    server_addr = args.server_addr
    msg = args.msg
    gpu_ordinal = args.gpu_ordinal
    
    print(f"{server_addr}, {msg}!")

    torch.cuda.init()

    size = 1024 * 1024
    tensors = torch.empty(size, dtype=torch.int8)

    local_buffers = TensorBlocks()
    local_tensor_block = TensorBlock(tensors.data_ptr(), 0, size)
    local_buffers.add(local_tensor_block)

    dt = RdmaClient(gpu_ordinal, local_buffers)

    remote_tensor_blocks = dt.connect(server_addr)
    remote_tb_base_ptr = remote_tensor_blocks.get_base_ptrs()[0];
    remote_tensor_block = TensorBlock(remote_tb_base_ptr, 0, size)

    for i in range(10):
        # dt.send(local_tensor_block, remote_tensor_block)
        dt.recv(local_tensor_block, remote_tensor_block)
        time.sleep(1)

    # time.sleep(1)
    dt.shutdown()
    time.sleep(5)


if __name__ == "__main__":
    main()

