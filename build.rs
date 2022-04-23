use cbindgen::Config;
use std::env;
fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_config(Config::from_file("cbindgen.toml").unwrap())
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("nice.h");

    println!("cargo:rerun-if-changed=src/lib.rs");

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .out_dir("src")
        .compile(&["hello.proto"], &["proto"])
        .unwrap();

    println!("cargo:rerun-if-changed=proto/hello.proto");
}
