# rdma-transport


## Build

- update submodules

``` bash
git submodule init

git submodule update --recursive
```

- install clang/llvm
- instlal rust toolchain via [rustup](https://rustup.rs/)
- compile by cargo

```bash
cargo build
```

## FAQ

- Instead the dev package when rdma-core build.sh failed with errors like:

```
--   No package 'libnl-3.0' found
--   No package 'libnl-route-3.0' found
```

