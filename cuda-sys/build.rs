use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = env::var("CUDA_HOME").unwrap_or("/usr/local/cuda".to_string());
    println!("cargo:include={manifest_dir}/include");
    println!("cargo:rustc-link-search=native={manifest_dir}/lib64/stubs");
    println!("cargo:rustc-link-lib=cuda");

    // generate bindings.rs
    let bindings: bindgen::Bindings = bindgen::Builder::default()
        .header(format!("{manifest_dir}/include/cuda.h"))
        .clang_arg(format!("-I{manifest_dir}/include/"))
        .allowlist_function(".*")
        .allowlist_type(".*")
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