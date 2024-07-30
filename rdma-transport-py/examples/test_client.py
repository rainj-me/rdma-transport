from rdma_transport import RdmaClient
import logging
import sys
import time
import torch
import argparse
import asyncio

FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
logging.basicConfig(stream=sys.stdout,format=FORMAT, level=logging.DEBUG)

async def main():
    # Create the parser
    parser = argparse.ArgumentParser(description="A simple script to greet the user.")
    
    # Add arguments
    parser.add_argument("--local_addr", type=str, help="")
    parser.add_argument("--server_addr", type=str, help="")
    parser.add_argument("--msg", type=str, help="")
    parser.add_argument("--gpu_ordinal", type=int, help="")
    
    # Parse the arguments
    args = parser.parse_args()
    
    local_addr = args.local_addr
    server_addr = args.server_addr
    msg = args.msg
    gpu_ordinal = args.gpu_ordinal
    
    print(f"{local_addr}, {server_addr}, {msg}!")

    torch.cuda.init()

    dt = RdmaClient(local_addr, gpu_ordinal)

    dt.connect(server_addr)

    buffer = dt.get_buffer()
    print(f"buffer: {buffer}")
    dt.fill_data(msg)

    for i in range(10):
        await dt.send(i, len(msg), b"abcdefg")
        time.sleep(1)

    # time.sleep(10)
    dt.shutdown()
    time.sleep(1)


if __name__ == "__main__":
    asyncio.run(main())

