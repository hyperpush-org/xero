use std::path::{Path, PathBuf};
use std::process::Command;

/// Pinned scrcpy-server release. Keep this in lockstep with the
/// `SCRCPY_VERSION` constant in src/commands/emulator/android/scrcpy.rs —
/// mismatched versions cause the server to reject our control messages.
const SCRCPY_VERSION: &str = "2.7";
const SCRCPY_SHA256: &str = "a23c5659f36c260f105c022d27bcb3eafffa26070e7baa9eda66d01377a1adba";

/// Pinned `idb_companion` universal tarball. The archive extracts to a
/// sibling `idb-companion.universal/` directory which holds both the binary
/// at `bin/idb_companion` and its `@executable_path/../Frameworks` dylibs —
/// the whole tree has to ship together or the binary fails to load.
const IDB_COMPANION_VERSION: &str = "1.1.8";
const IDB_COMPANION_SHA256: &str =
    "3b72cc6a9a5b1a22a188205a84090d3a294347a846180efd755cf1a3c848e3e7";
const IDB_COMPANION_DIR: &str = "idb-companion.universal";
const BUILD_COOKIE_IMPORTER_ENV: &str = "XERO_BUILD_COOKIE_IMPORTER";
const SKIP_COOKIE_IMPORTER_ENV: &str = "XERO_SKIP_COOKIE_IMPORTER";

fn main() {
    configure_custom_cfgs();
    tauri_build::build();
    compile_dictation_shim();
    compile_ios_helper();
    build_cookie_importer();
    fetch_scrcpy_server();
    fetch_idb_companion();
    compile_idb_proto();
}

fn configure_custom_cfgs() {
    println!("cargo:rustc-check-cfg=cfg(xero_dictation_native_shim)");
    println!("cargo:rustc-check-cfg=cfg(xero_dictation_modern_sdk)");
}

fn compile_dictation_shim() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=native/dictation/CapabilityStatus.swift");
    println!("cargo:rerun-if-changed=native/dictation/SessionLifecycle.swift");
    println!("cargo:rerun-if-changed=native/dictation/LegacyEngine.swift");
    println!("cargo:rerun-if-changed=native/dictation/ModernAvailable.swift");
    println!("cargo:rerun-if-changed=native/dictation/ModernStub.swift");
    println!("cargo:rerun-if-env-changed=XERO_SKIP_DICTATION_SHIM");

    println!("cargo:rustc-env=XERO_MACOS_SDK_VERSION=");
    println!("cargo:rustc-env=XERO_DICTATION_MODERN_COMPILED=0");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    println!("cargo:rustc-link-lib=framework=Speech");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=Security");

    if std::env::var_os("XERO_SKIP_DICTATION_SHIM").is_some() {
        println!(
            "cargo:warning=XERO_SKIP_DICTATION_SHIM is set; dictation status will report the native shim unavailable."
        );
        return;
    }

    let Some(swiftc) = xcrun_find("swiftc") else {
        println!(
            "cargo:warning=swiftc was not found via xcrun; dictation status will report the native shim unavailable."
        );
        return;
    };
    let Some(sdk_path) = xcrun_output(&["--sdk", "macosx", "--show-sdk-path"]) else {
        println!(
            "cargo:warning=macOS SDK path was not found via xcrun; dictation status will report the native shim unavailable."
        );
        return;
    };
    let sdk_version = xcrun_output(&["--sdk", "macosx", "--show-sdk-version"]).unwrap_or_default();
    let modern_sdk = macos_sdk_supports_modern_dictation(&sdk_version);

    println!("cargo:rustc-env=XERO_MACOS_SDK_VERSION={sdk_version}");
    println!(
        "cargo:rustc-env=XERO_DICTATION_MODERN_COMPILED={}",
        if modern_sdk { "1" } else { "0" }
    );
    if modern_sdk {
        println!("cargo:rustc-cfg=xero_dictation_modern_sdk");
    }

    // macOS 26+ splits Foundation into new Swift overlay dylibs
    // (swift_DarwinFoundation{1,2,3}, swiftSynchronization, etc.)
    // that live under the SDK's usr/lib/swift. Without this search
    // path the linker can't resolve auto-linked libs from the shim.
    let sdk_swift_lib = PathBuf::from(&sdk_path).join("usr/lib/swift");
    if sdk_swift_lib.is_dir() {
        println!("cargo:rustc-link-search=native={}", sdk_swift_lib.display());
        println!(
            "cargo:rustc-link-search=framework={}",
            sdk_swift_lib.display()
        );
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let shim_dir = manifest_dir.join("native/dictation");
    let modern_source = if modern_sdk {
        shim_dir.join("ModernAvailable.swift")
    } else {
        shim_dir.join("ModernStub.swift")
    };
    let output = out_dir.join("libXeroDictationShim.a");

    let status = Command::new(&swiftc)
        .arg("-emit-library")
        .arg("-static")
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("XeroDictationShim")
        .arg("-sdk")
        .arg(&sdk_path)
        .arg("-o")
        .arg(&output)
        .arg(shim_dir.join("CapabilityStatus.swift"))
        .arg(shim_dir.join("SessionLifecycle.swift"))
        .arg(shim_dir.join("LegacyEngine.swift"))
        .arg(modern_source)
        .status()
        .expect("failed to spawn swiftc for dictation shim");

    if !status.success() {
        panic!("failed to compile Xero dictation Swift shim (exit {status:?})");
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    for runtime_path in swift_runtime_library_paths(&swiftc) {
        println!("cargo:rustc-link-search=native={runtime_path}");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{runtime_path}");
    }
    if modern_sdk {
        // The modern dictation shim compiled with Swift 6.2 / Xcode 26
        // references symbols across Speech, Foundation overlays, and the
        // Swift concurrency runtime. Compiling to a static .a loses
        // Swift's auto-link metadata, so the Rust linker never discovers
        // those implicit dependencies. Rather than manually enumerating
        // every Swift overlay dylib (which changes across beta releases),
        // use -undefined dynamic_lookup to defer them to runtime. On
        // macOS 26 all these symbols exist in /usr/lib/swift and the
        // Speech framework; the @available guards in the Swift code
        // ensure this path is never called on older OS versions.
        println!("cargo:rustc-link-arg=-Wl,-undefined,dynamic_lookup");
    }
    println!("cargo:rustc-link-lib=static=XeroDictationShim");
    println!("cargo:rustc-cfg=xero_dictation_native_shim");
}

/// Compile the Swift helper binary (`xero-ios-helper`) that uses
/// ScreenCaptureKit for frame capture and IndigoHID for input injection.
/// The binary is a standalone executable (not a static library) that
/// communicates with the Tauri Rust backend over a Unix domain socket.
///
/// Unlike `compile_dictation_shim()` which produces a `.a` linked into the
/// main binary, this produces an independent executable copied next to the
/// Tauri output binary.
#[cfg(target_os = "macos")]
fn compile_ios_helper() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let helper_dir = manifest_dir.join("native/ios-helper");

    println!("cargo:rerun-if-changed=native/ios-helper/Main.swift");
    println!("cargo:rerun-if-changed=native/ios-helper/Connection.swift");
    println!("cargo:rerun-if-changed=native/ios-helper/FrameCapture.swift");
    println!("cargo:rerun-if-changed=native/ios-helper/HidBridge.swift");
    println!("cargo:rerun-if-changed=native/ios-helper/JpegEncoder.swift");
    println!("cargo:rerun-if-changed=native/ios-helper/AccessibilityBridge.swift");
    println!("cargo:rerun-if-env-changed=XERO_SKIP_IOS_HELPER");

    if std::env::var_os("XERO_SKIP_IOS_HELPER").is_some() {
        println!(
            "cargo:warning=XERO_SKIP_IOS_HELPER is set; iOS helper binary will not be compiled."
        );
        return;
    }

    let Some(swiftc) = xcrun_find("swiftc") else {
        println!(
            "cargo:warning=swiftc not found via xcrun; iOS helper binary will not be compiled."
        );
        return;
    };
    let Some(sdk_path) = xcrun_output(&["--sdk", "macosx", "--show-sdk-path"]) else {
        println!(
            "cargo:warning=macOS SDK path not found; iOS helper binary will not be compiled."
        );
        return;
    };

    // Check that ScreenCaptureKit is available (macOS 12.3+ SDK).
    let sdk_version = xcrun_output(&["--sdk", "macosx", "--show-sdk-version"]).unwrap_or_default();
    let major: u32 = sdk_version
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if major < 12 {
        println!(
            "cargo:warning=macOS SDK {sdk_version} < 12.3; ScreenCaptureKit unavailable. \
             iOS helper binary will not be compiled."
        );
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let output = out_dir.join("xero-ios-helper");

    let sources = [
        helper_dir.join("Main.swift"),
        helper_dir.join("Connection.swift"),
        helper_dir.join("FrameCapture.swift"),
        helper_dir.join("HidBridge.swift"),
        helper_dir.join("JpegEncoder.swift"),
        helper_dir.join("AccessibilityBridge.swift"),
    ];

    let status = Command::new(&swiftc)
        .arg("-parse-as-library")
        .arg("-module-name")
        .arg("XeroIosHelper")
        .arg("-sdk")
        .arg(&sdk_path)
        .arg("-O")
        .arg("-framework")
        .arg("ScreenCaptureKit")
        .arg("-framework")
        .arg("CoreGraphics")
        .arg("-framework")
        .arg("ImageIO")
        .arg("-framework")
        .arg("Foundation")
        .arg("-framework")
        .arg("CoreMedia")
        .arg("-framework")
        .arg("ApplicationServices")
        .arg("-framework")
        .arg("AppKit")
        .arg("-o")
        .arg(&output)
        .args(sources.iter())
        .status()
        .expect("failed to spawn swiftc for iOS helper");

    if !status.success() {
        // Non-fatal: the helper is optional. The session falls back to
        // idb_companion / screenshot polling when the binary is absent.
        println!(
            "cargo:warning=failed to compile xero-ios-helper (exit {status:?}); \
             iOS Simulator will use fallback paths."
        );
        return;
    }

    // Copy the binary next to the Tauri output executable so it can be
    // discovered by `helper::resolve_helper_binary()` during development.
    // In bundled builds the binary is included via tauri.conf.json resources.
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR should be inside target/<profile>/build/<pkg>/out")
        .to_path_buf();
    let destination = profile_dir.join("xero-ios-helper");
    if let Err(e) = std::fs::copy(&output, &destination) {
        println!(
            "cargo:warning=failed to copy xero-ios-helper to {}: {e}",
            destination.display()
        );
    }

    // Also copy to resources/ for Tauri bundling.
    let resources_dir = manifest_dir.join("resources");
    let res_destination = resources_dir.join("xero-ios-helper");
    if let Err(e) = std::fs::copy(&output, &res_destination) {
        println!(
            "cargo:warning=failed to copy xero-ios-helper to resources/: {e}"
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn compile_ios_helper() {
    // No-op on non-macOS hosts.
}

fn xcrun_find(tool: &str) -> Option<PathBuf> {
    xcrun_output(&["--find", tool]).map(PathBuf::from)
}

fn xcrun_output(args: &[&str]) -> Option<String> {
    let output = Command::new("xcrun").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn macos_sdk_supports_modern_dictation(version: &str) -> bool {
    version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        .is_some_and(|major| major >= 26)
}

fn swift_runtime_library_paths(swiftc: &Path) -> Vec<String> {
    let output = Command::new(swiftc).arg("-print-target-info").output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let Some(key_index) = text.find("\"runtimeLibraryPaths\"") else {
        return Vec::new();
    };
    let Some(start_offset) = text[key_index..].find('[') else {
        return Vec::new();
    };
    let start = key_index + start_offset + 1;
    let Some(end_offset) = text[start..].find(']') else {
        return Vec::new();
    };
    let array = &text[start..start + end_offset];

    array
        .split('"')
        .skip(1)
        .step_by(2)
        .filter_map(|entry| {
            let trimmed = entry.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect()
}

/// Compile `proto/idb.proto` into a tonic gRPC client. Only runs when the
/// `ios-grpc` feature is enabled — the default build leaves the stub client
/// in `ios/idb_client.rs` alone.
#[cfg(feature = "ios-grpc")]
fn compile_idb_proto() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let proto = manifest_dir.join("proto/idb.proto");

    println!("cargo:rerun-if-changed=proto/idb.proto");

    if !proto.exists() {
        println!(
            "cargo:warning=proto/idb.proto missing — ios-grpc feature was enabled but the vendored proto is not present."
        );
        return;
    }

    // Point tonic-build at the vendored `protoc` so developers don't need
    // `brew install protobuf` just to build Xero.
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().unwrap());

    if let Err(err) = tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&[proto], &[manifest_dir.join("proto")])
    {
        panic!("failed to compile idb.proto: {err}");
    }
}

#[cfg(not(feature = "ios-grpc"))]
fn compile_idb_proto() {
    // No-op. The stub client in src/commands/emulator/ios/idb_client.rs
    // keeps the module compilable without pulling tonic + prost into
    // every build.
}

// Build the sibling cookie-importer crate and copy its binary next to the main
// app binary so it can be spawned at runtime. The cookie-importer has to live
// in a separate cargo workspace because it links rookie → rusqlite 0.31 which
// conflicts with the desktop app's rusqlite 0.37 on the `links = "sqlite3"`
// metadata.
fn build_cookie_importer() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let helper_manifest = manifest_dir.join("crates/cookie-importer/Cargo.toml");
    println!("cargo:rerun-if-changed=crates/cookie-importer/Cargo.toml");
    println!("cargo:rerun-if-changed=crates/cookie-importer/src/main.rs");
    println!("cargo:rerun-if-env-changed={BUILD_COOKIE_IMPORTER_ENV}");
    println!("cargo:rerun-if-env-changed={SKIP_COOKIE_IMPORTER_ENV}");

    // OUT_DIR looks like .../target/<profile>/build/xero-desktop-<hash>/out.
    // Walk up three levels to reach the parent of target/<profile>/, then the
    // runtime binary lives in target/<profile>/.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR should be inside target/<profile>/build/<pkg>/out")
        .to_path_buf();

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let helper_target = manifest_dir
        .join("crates/cookie-importer/target")
        .join(&profile)
        .join(binary_name());
    let destination = profile_dir.join(binary_name());

    if std::env::var_os(SKIP_COOKIE_IMPORTER_ENV).is_some() {
        return;
    }

    if std::env::var_os(BUILD_COOKIE_IMPORTER_ENV).is_none() {
        if helper_target.exists() {
            copy_cookie_importer(&helper_target, &destination);
        }
        return;
    }

    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()));
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(&helper_manifest);
    if profile == "release" {
        cmd.arg("--release");
    }

    let status = cmd
        .status()
        .expect("failed to spawn cargo for cookie-importer");
    if !status.success() {
        panic!("failed to build cookie-importer (exit {status:?})");
    }

    if !helper_target.exists() {
        panic!("cookie-importer build succeeded but binary not found at {helper_target:?}");
    }

    copy_cookie_importer(&helper_target, &destination);
}

fn copy_cookie_importer(helper_target: &Path, destination: &Path) {
    std::fs::copy(helper_target, destination).unwrap_or_else(|error| {
        panic!("failed to copy cookie-importer to {destination:?}: {error}")
    });
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "xero-cookie-importer.exe"
    } else {
        "xero-cookie-importer"
    }
}

/// Ensure `resources/scrcpy-server-v<VERSION>.jar` exists. If the file is
/// already present *and* its SHA-256 matches the pinned value we skip the
/// fetch. Otherwise we download with reqwest and verify. Network
/// failures emit a `cargo:warning` rather than aborting the build — the
/// Android pipeline surfaces a typed `scrcpy_jar_missing` error at runtime
/// so a dev can still build-and-boot the app even without the jar.
fn fetch_scrcpy_server() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let resources_dir = manifest_dir.join("resources");
    let target = resources_dir.join(format!("scrcpy-server-v{SCRCPY_VERSION}.jar"));

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=XERO_SKIP_SIDECAR_FETCH");
    println!("cargo:rerun-if-changed={}", target.display());

    if std::env::var_os("XERO_SKIP_SIDECAR_FETCH").is_some() {
        return;
    }

    if target.exists() {
        match sha256_of(&target) {
            Ok(digest) if digest == SCRCPY_SHA256 => return,
            Ok(other) => {
                println!(
                    "cargo:warning=scrcpy-server-v{SCRCPY_VERSION}.jar present but SHA mismatch (got {other}, want {SCRCPY_SHA256}). Re-fetching."
                );
                let _ = std::fs::remove_file(&target);
            }
            Err(err) => {
                println!("cargo:warning=failed to hash existing scrcpy jar: {err}");
            }
        }
    }

    if std::fs::create_dir_all(&resources_dir).is_err() {
        println!(
            "cargo:warning=could not create {} — drop scrcpy-server-v{SCRCPY_VERSION}.jar there manually",
            resources_dir.display()
        );
        return;
    }

    let url = format!(
        "https://github.com/Genymobile/scrcpy/releases/download/v{SCRCPY_VERSION}/scrcpy-server-v{SCRCPY_VERSION}"
    );
    if let Err(err) = download_to_path(&url, &target) {
        println!(
            "cargo:warning=failed to fetch scrcpy-server from {url}: {err}. Android streaming will fail until the jar is in place."
        );
        let _ = std::fs::remove_file(&target);
        return;
    }

    match sha256_of(&target) {
        Ok(digest) if digest == SCRCPY_SHA256 => {}
        Ok(other) => {
            println!(
                "cargo:warning=fetched scrcpy-server SHA {other} does not match pinned {SCRCPY_SHA256}. Discarding."
            );
            let _ = std::fs::remove_file(&target);
        }
        Err(err) => {
            println!("cargo:warning=could not hash fetched scrcpy-server: {err}");
            let _ = std::fs::remove_file(&target);
        }
    }
}

/// Ensure `resources/idb-companion.universal/bin/idb_companion` exists for
/// macOS builds. On non-macOS hosts this is a no-op — the iOS pipeline is
/// compiled out entirely by `#[cfg(target_os = "macos")]` guards, so the
/// bundled resource would never be loaded.
///
/// The fetch is skipped when `XERO_SKIP_SIDECAR_FETCH` is set (CI caches
/// the extraction itself) or when the pinned version marker is already
/// present. Failures downgrade to a `cargo:warning`; the runtime probe
/// falls back to Homebrew / `PATH` so `tauri dev` without a prior fetch
/// still works.
#[cfg(target_os = "macos")]
fn fetch_idb_companion() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let resources_dir = manifest_dir.join("resources");
    let extracted = resources_dir.join(IDB_COMPANION_DIR);
    let sentinel = extracted.join(".xero-version");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=XERO_SKIP_SIDECAR_FETCH");
    println!("cargo:rerun-if-changed={}", sentinel.display());

    if std::env::var_os("XERO_SKIP_SIDECAR_FETCH").is_some() {
        return;
    }

    if let Ok(existing) = std::fs::read_to_string(&sentinel) {
        if existing.trim() == IDB_COMPANION_VERSION {
            return;
        }
    }

    if let Err(err) = std::fs::create_dir_all(&resources_dir) {
        println!(
            "cargo:warning=could not create {}: {err}. Drop idb_companion into resources/ manually.",
            resources_dir.display()
        );
        return;
    }

    // Drop any previous (stale) extraction before writing new contents so
    // version mismatches can't leave a mixed tree behind.
    if extracted.exists() {
        if let Err(err) = std::fs::remove_dir_all(&extracted) {
            println!(
                "cargo:warning=failed to prune stale idb-companion tree at {}: {err}",
                extracted.display()
            );
            return;
        }
    }

    let tarball = resources_dir.join(format!(
        "idb-companion.universal-v{IDB_COMPANION_VERSION}.tar.gz"
    ));
    let url = format!(
        "https://github.com/facebook/idb/releases/download/v{IDB_COMPANION_VERSION}/idb-companion.universal.tar.gz"
    );

    if let Err(err) = download_to_path(&url, &tarball) {
        println!(
            "cargo:warning=failed to fetch idb_companion from {url}: {err}. iOS streaming will fall back to Homebrew / PATH."
        );
        let _ = std::fs::remove_file(&tarball);
        return;
    }

    match sha256_of(&tarball) {
        Ok(digest) if digest == IDB_COMPANION_SHA256 => {}
        Ok(other) => {
            println!(
                "cargo:warning=fetched idb_companion SHA {other} does not match pinned {IDB_COMPANION_SHA256}. Discarding."
            );
            let _ = std::fs::remove_file(&tarball);
            return;
        }
        Err(err) => {
            println!("cargo:warning=could not hash fetched idb_companion: {err}");
            let _ = std::fs::remove_file(&tarball);
            return;
        }
    }

    if let Err(err) = extract_tar_gz_into(&tarball, &resources_dir) {
        println!(
            "cargo:warning=failed to extract {}: {err}. iOS streaming will fall back to Homebrew / PATH.",
            tarball.display()
        );
        let _ = std::fs::remove_file(&tarball);
        let _ = std::fs::remove_dir_all(&extracted);
        return;
    }

    let _ = std::fs::remove_file(&tarball);

    let binary = extracted.join("bin").join("idb_companion");
    if !binary.is_file() {
        println!(
            "cargo:warning=idb_companion binary missing from extracted tree at {}",
            binary.display()
        );
        let _ = std::fs::remove_dir_all(&extracted);
        return;
    }

    // tar usually preserves the execute bit, but certain archivers (and
    // some CI Docker layer caches) strip it. Force it back on so Tauri
    // doesn't ship a non-executable sidecar.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&binary) {
            let mut perms = metadata.permissions();
            let mode = perms.mode();
            if mode & 0o111 == 0 {
                perms.set_mode(mode | 0o755);
                let _ = std::fs::set_permissions(&binary, perms);
            }
        }
    }

    if let Err(err) = std::fs::write(&sentinel, IDB_COMPANION_VERSION) {
        println!("cargo:warning=could not write idb_companion version marker: {err}");
    }
}

#[cfg(not(target_os = "macos"))]
fn fetch_idb_companion() {
    // idb_companion only runs on macOS; non-macOS builds have no iOS
    // Simulator to point it at.
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize_hex())
}

fn download_to_path(url: &str, target: &Path) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .build()
        .map_err(|error| format!("could not build HTTP client: {error}"))?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|error| format!("GET failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("GET returned {}", response.status()));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    let mut file = std::fs::File::create(target)
        .map_err(|error| format!("could not create {}: {error}", target.display()))?;
    std::io::copy(&mut response, &mut file)
        .map_err(|error| format!("could not write {}: {error}", target.display()))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn extract_tar_gz_into(tarball: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(tarball)
        .map_err(|error| format!("could not open {}: {error}", tarball.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_ownerships(false);
    archive
        .unpack(target)
        .map_err(|error| format!("could not unpack into {}: {error}", target.display()))
}

/// Tiny SHA-256 wrapper to avoid pulling the `sha2` crate in build-deps.
/// Defers to whatever `shasum` / `sha256sum` exists on the host — both
/// macOS and Linux ship one. On Windows we use `certutil`.
struct Sha256Hasher {
    data: Vec<u8>,
}

impl Sha256Hasher {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn update(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    fn finalize_hex(self) -> String {
        use std::io::Write;

        // Prefer native shasum/sha256sum; fall back to a pure-Rust impl
        // that's trivial to inline (FIPS 180-4). We take the CLI path first
        // because it's ~10x faster on large inputs.
        let tool = if cfg!(target_os = "macos") {
            "shasum"
        } else if cfg!(target_os = "windows") {
            "certutil"
        } else {
            "sha256sum"
        };

        if tool == "certutil" {
            // certutil -hashfile <path> SHA256 — we don't have a path; use
            // a temp file. Skip certutil and fall back to the inline impl.
            return inline_sha256(&self.data);
        }

        let mut cmd = Command::new(tool);
        if tool == "shasum" {
            cmd.args(["-a", "256"]);
        }
        let child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn();
        if let Ok(mut child) = child {
            if let Some(mut stdin) = child.stdin.take() {
                if stdin.write_all(&self.data).is_err() {
                    return inline_sha256(&self.data);
                }
            }
            if let Ok(output) = child.wait_with_output() {
                if output.status.success() {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if let Some(digest) = text.split_whitespace().next() {
                        return digest.to_string();
                    }
                }
            }
        }
        inline_sha256(&self.data)
    }
}

/// Minimal FIPS 180-4 SHA-256 implementation for build.rs use. Keeps the
/// build-dependency list empty while giving us a reliable fallback when no
/// CLI hasher is on PATH.
fn inline_sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|w| format!("{:08x}", w)).collect::<String>()
}
