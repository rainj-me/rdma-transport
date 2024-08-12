from rdma_transport import VllmRdmaServer as RdmaServer, TensorBlocks, TensorBlock
import logging
import sys
import time
import torch
import asyncio
import argparse
import signal

FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
logging.basicConfig(stream=sys.stdout,format=FORMAT, level=logging.DEBUG)

async def main():
    # Create the parser
    parser = argparse.ArgumentParser(description="A simple script to greet the user.")
    
    # Add arguments
    parser.add_argument("--server_addr", type=str, help="")
    parser.add_argument("--gpu_ordinal", type=int, help="")

    # Parse the arguments
    args = parser.parse_args()

    server_addr = args.server_addr
    gpu_ordinal = args.gpu_ordinal

    torch.cuda.init()

    size = 1024 * 1024
    tensors = torch.empty(size, dtype=torch.int8)

    local_buffers = TensorBlocks()
    buffer = TensorBlock(tensors.data_ptr(), 0, size)
    local_buffers.add(buffer)

    dt = RdmaServer(server_addr, gpu_ordinal, local_buffers)
    dt.listen()

    def signal_handler(sig, frame):
        print('You pressed Ctrl-C! Performing cleanup...')
        # Perform any cleanup actions here
        dt.shutdown()
        print('Cleanup done. Exiting.')
        time.sleep(1)
        sys.exit(0)

    # Register the signal handler for SIGINT
    signal.signal(signal.SIGINT, signal_handler)

    while True:
        time.sleep(1)


if __name__ == "__main__":
    asyncio.run(main())

