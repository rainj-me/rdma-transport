from rdma_transport import RdmaServer
import logging
import sys
import time
import torch
import argparse

FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
logging.basicConfig(stream=sys.stdout,format=FORMAT, level=logging.DEBUG)

def main():
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

    FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
    logging.basicConfig(stream=sys.stdout,format=FORMAT, level=logging.DEBUG)
    dt = RdmaServer(server_addr, gpu_ordinal)
    dt.listen()
    time.sleep(180)

    dt.shutdown()
    time.sleep(3)

if __name__ == "__main__":
    main()

