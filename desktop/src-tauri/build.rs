fn main() {
    // The downloaded ONNX Runtime binary bundles CoreML support on macOS
    // (not something we opted into -- it's just what `ort`'s download
    // strategy ships there), and its CoreML object files reference
    // ___isPlatformVersionAtLeast from clang's compiler-rt. rustc links
    // with -nodefaultlibs, so that normally-automatic runtime support
    // library has to be linked in explicitly here, or the final binary
    // fails to link with an "undefined symbol" error.
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("clang").arg("--print-file-name=libclang_rt.osx.a").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Some(dir) = std::path::Path::new(&path).parent() {
                    println!("cargo:rustc-link-search=native={}", dir.display());
                }
                println!("cargo:rustc-link-lib=static=clang_rt.osx");
            }
        }
    }

    tauri_build::build()
}
