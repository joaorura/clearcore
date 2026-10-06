use std::path::PathBuf;

fn find_tensorrt_include() -> Option<PathBuf> {
    // 1. Environment variables
    for var in &["TENSORRT_INCLUDE_PATH", "CLEARCORE_TENSORRT_PATH", "TENSORRT_DIR"] {
        if let Ok(val) = std::env::var(var) {
            let p = PathBuf::from(val);
            if p.join("NvInfer.h").exists() {
                return Some(p);
            }
            if p.join("include").join("NvInfer.h").exists() {
                return Some(p.join("include"));
            }
        }
    }

    // 2. Dynamic discovery in $HOME/opt, /opt, /usr/local
    let mut search_dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        search_dirs.push(PathBuf::from(home).join("opt"));
    }
    search_dirs.push(PathBuf::from("/opt"));
    search_dirs.push(PathBuf::from("/usr/local"));

    for base in search_dirs {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with("tensorrt") {
                    let inc = entry.path().join("include");
                    if inc.join("NvInfer.h").exists() {
                        return Some(inc);
                    }
                }
            }
        }
    }

    // 3. System include directories
    for sys in &["/usr/local/include", "/usr/include"] {
        let p = PathBuf::from(sys);
        if p.join("NvInfer.h").exists() {
            return Some(p);
        }
    }

    None
}

fn find_cuda_include() -> Option<PathBuf> {
    // 1. Environment variables
    for var in &["CUDA_INCLUDE_PATH", "CUDA_PATH", "CUDA_HOME"] {
        if let Ok(val) = std::env::var(var) {
            let p = PathBuf::from(val);
            if p.join("cuda.h").exists() {
                return Some(p);
            }
            if p.join("include").join("cuda.h").exists() {
                return Some(p.join("include"));
            }
        }
    }

    // 2. Well-known directories
    for candidate in &[
        "/usr/local/cuda/include",
        "/usr/include",
        "/usr/local/include",
    ] {
        let p = PathBuf::from(candidate);
        if p.join("cuda.h").exists() {
            return Some(p);
        }
    }

    None
}

fn main() {
    println!("cargo:rerun-if-changed=csrc/trt_shim.cpp");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-check-cfg=cfg(has_trt_shim)");

    let trt_inc = find_tensorrt_include();
    let cuda_inc = find_cuda_include();

    if let (Some(trt_dir), Some(cuda_dir)) = (trt_inc, cuda_inc) {
        cc::Build::new()
            .cpp(true)
            .std("c++17")
            .flag_if_supported("-Wno-deprecated-declarations")
            .include(&trt_dir)
            .include(&cuda_dir)
            .file("csrc/trt_shim.cpp")
            .compile("trt_shim");

        println!("cargo:rustc-cfg=has_trt_shim");
    } else {
        println!("cargo:warning=TensorRT or CUDA headers not found; trt_shim will not be compiled");
    }
}
