fn main() {
    println!("cargo:rerun-if-changed=src/neon_card.c");
    println!("cargo:rerun-if-changed=src/neon_card_kernel.h");
    println!("cargo:rerun-if-changed=src/neon_rgba.h");
    println!("cargo:rerun-if-changed=src/neon_shader_raster.c");
    println!("cargo:rerun-if-changed=src/neon_sprite_types.h");
    println!("cargo:rerun-if-changed=src/neon_sprite.c");
    println!("cargo:rerun-if-changed=src/neon_sprite_span.h");
    println!("cargo:rerun-if-changed=src/neon_copy.c");
    println!("cargo:rerun-if-changed=src/neon_flame.c");
    println!("cargo:rerun-if-env-changed=BALATRO_SLEEF_DIR");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("arm")
        && std::env::var_os("CARGO_FEATURE_ARM_NEON").is_some()
    {
        cc::Build::new()
            .file("src/neon_card.c")
            .file("src/neon_shader_raster.c")
            .file("src/neon_sprite.c")
            .file("src/neon_copy.c")
            .flag("-mcpu=cortex-a7")
            .flag("-mfpu=neon-vfpv4")
            .flag("-ffp-contract=off")
            .flag("-fno-math-errno")
            .compile("balatro_card_neon");
        if std::env::var_os("CARGO_FEATURE_FLAME_SIMD").is_some() {
            let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
            let library = std::env::var_os("BALATRO_SLEEF_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| root.join("../../target/sleef-armv7-musl"));
            assert!(
                library.join("libbalatro_sleef.a").is_file(),
                "Run sh scripts/build-sleef.sh before building flame-simd"
            );
            cc::Build::new()
                .file("src/neon_flame.c")
                .flag("-mcpu=cortex-a7")
                .flag("-mfpu=neon-vfpv4")
                .flag("-ffp-contract=off")
                .flag("-fno-math-errno")
                .compile("balatro_flame_neon");
            println!("cargo:rustc-link-search=native={}", library.display());
            println!("cargo:rustc-link-lib=static=balatro_sleef");
        }
    }
}
