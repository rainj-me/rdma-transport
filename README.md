# rdma-transport


## Build

- update submodules

``` bash
git submodule init

git submodule update --recursive
```

- install clang/llvm
- install rust toolchain via [rustup](https://rustup.rs/)
- set the cuda comput cap environment (or/and set to rust-analyzer server extra env)

```bash
export CUDA_HOME=/usr/local/cuda
```

- compile by cargo

```bash
cargo build
```

## FAQ

- Install the dev packages when rdma-core build.sh failed with errors like:

```
--   No package 'libnl-3.0' found
--   No package 'libnl-route-3.0' found
```

