//! The V4L2 ioctl numbers are nested macros, so clang's macro fallback
//! evaluates them.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // V4L2 headers exist only on Linux, and src/lib.rs refuses other targets.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // clang_macro_fallback only works with a header that is a file on disk.
    let header = out.join("v4l2.h");

    std::fs::write(&header, "#include <linux/videodev2.h>\n").unwrap();

    bindgen::Builder::default()
        .header(header.to_str().unwrap())
        .clang_macro_fallback()
        .clang_macro_fallback_build_dir(&out)
        .allowlist_item("(v4l2|V4L2|VIDIOC)_.*")
        .generate()
        .expect("the V4L2 headers are there")
        .write_to_file(out.join("v4l2.rs"))
        .unwrap();
}
