fn main() {
    println!("cargo:rerun-if-changed=src/copy_trace.c");
    if std::env::var_os("CARGO_FEATURE_COPY_TRACE").is_some()
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
        && std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("arm")
    {
        cc::Build::new()
            .file("src/copy_trace.c")
            .flag("-fno-builtin")
            .compile("balatro_copy_trace");
        println!("cargo:rustc-link-arg=-Wl,-wrap,memcpy");
        println!("cargo:rustc-link-arg=-Wl,-wrap,memmove");
    }
}
