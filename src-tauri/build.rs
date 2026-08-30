#[cfg(target_os = "macos")]
fn build_screen_capture_kit_bridge() {
    use std::{env, path::PathBuf, process::Command};

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("missing OUT_DIR"));
    let object = out_dir.join("screen_capture_kit.o");
    let library = out_dir.join("libscreen_capture_kit.a");
    let status = Command::new("xcrun")
        .args(["clang", "-fobjc-arc", "-fmodules", "-mmacosx-version-min=15.0", "-c"])
        .arg("macos/screen_capture_kit.m")
        .arg("-o")
        .arg(&object)
        .status()
        .expect("failed to run clang for ScreenCaptureKit bridge");
    assert!(status.success(), "failed to compile ScreenCaptureKit bridge");
    let status = Command::new("libtool")
        .args(["-static", "-o"])
        .arg(&library)
        .arg(&object)
        .status()
        .expect("failed to archive ScreenCaptureKit bridge");
    assert!(status.success(), "failed to archive ScreenCaptureKit bridge");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=screen_capture_kit");
    println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Vision");
    println!("cargo:rustc-link-lib=framework=ImageIO");
    println!("cargo:rerun-if-changed=macos/screen_capture_kit.m");
    println!("cargo:rerun-if-changed=macos/screen_capture_kit.h");
}

fn main() {
    #[cfg(target_os = "macos")]
    build_screen_capture_kit_bridge();
    tauri_build::build()
}
