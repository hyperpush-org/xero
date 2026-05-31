use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    for path in swift_runtime_candidates() {
        if path.join("libswift_Concurrency.dylib").exists() {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", path.display());
            return;
        }
    }
}

fn swift_runtime_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(developer_dir) = env::var("DEVELOPER_DIR") {
        push_developer_dir_candidates(Path::new(&developer_dir), &mut candidates);
    }
    if let Ok(output) = Command::new("xcode-select").arg("-p").output() {
        if output.status.success() {
            if let Ok(developer_dir) = String::from_utf8(output.stdout) {
                push_developer_dir_candidates(Path::new(developer_dir.trim()), &mut candidates);
            }
        }
    }
    push_developer_dir_candidates(
        Path::new("/Applications/Xcode.app/Contents/Developer"),
        &mut candidates,
    );
    push_developer_dir_candidates(
        Path::new("/Library/Developer/CommandLineTools"),
        &mut candidates,
    );
    candidates
}

fn push_developer_dir_candidates(developer_dir: &Path, candidates: &mut Vec<PathBuf>) {
    for swift_version in ["swift-5.5", "swift-6.2", "swift-5.0"] {
        candidates.push(
            developer_dir
                .join("Toolchains/XcodeDefault.xctoolchain/usr/lib")
                .join(swift_version)
                .join("macosx"),
        );
        candidates.push(
            developer_dir
                .join("usr/lib")
                .join(swift_version)
                .join("macosx"),
        );
    }
}
