use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("failed to get current directory");
    println!("cargo:include={manifest_dir}/vendor/rdma-core/build/include");
    println!("cargo:rustc-link-search=native={manifest_dir}/vendor/rdma-core/build/lib");
    println!("cargo:rustc-link-lib=ibverbs");
    println!("cargo:rustc-link-lib=rdmacm");

    // initialize and update submodules
    if Path::new(".git").is_dir() {
        Command::new("git")
            .args(["submodule", "update", "--init"])
            .status()
            .expect("Failed to update submodules.");
    } else {
        assert!(
            Path::new("vendor/rdma-core").is_dir(),
            "vendor source not included"
        );
    }

    // build vendor/rdma-core
    Command::new("bash")
        .current_dir("vendor/rdma-core")
        .args(["build.sh"])
        .status()
        .expect("Failed to build vendor/rdma-core using build.sh");

    // generate bindings.rs
    let bindings = bindgen::Builder::default()
        .header("vendor/rdma-core/libibverbs/verbs.h")
        .header("vendor/rdma-core/librdmacm/rdma_cma.h")
        .header("vendor/rdma-core/librdmacm/rdma_verbs.h")
        .clang_arg("-Ivendor/rdma-core/build/include/")
        .allowlist_function(".*")
        .allowlist_type(".*")
        .blocklist_type("sockaddr.*")
        .raw_line("use libc::{sockaddr, sockaddr_in, sockaddr_in6, sockaddr_storage};")
        .derive_default(true)
        .derive_debug(true)
        .prepend_enum_name(false)
        .size_t_is_usize(true)
        .generate_inline_functions(true)
        .generate()
        .expect("Unable to generate bindings");

    // write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Could not write bindings");
}