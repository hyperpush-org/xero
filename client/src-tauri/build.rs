use std::path::{Path, PathBuf};
use std::process::Command;

/// Pinned scrcpy-server release. Keep this in lockstep with the
/// `SCRCPY_VERSION` constant in src/commands/emulator/android/scrcpy.rs —
/// mismatched versions cause the server to reject our control messages.
const SCRCPY_VERSION: &str = "2.7";
const SCRCPY_SHA256: &str = "a23c5659f36c260f105c022d27bcb3eafffa26070e7baa9eda66d01377a1adba";

fn main() {
    tauri_build::build();
    build_cookie_importer();
    fetch_scrcpy_server();
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

    // OUT_DIR looks like .../target/<profile>/build/cadence-desktop-<hash>/out.
    // Walk up three levels to reach the parent of target/<profile>/, then the
    // runtime binary lives in target/<profile>/.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR should be inside target/<profile>/build/<pkg>/out")
        .to_path_buf();

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()));
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(&helper_manifest);
    if profile == "release" {
        cmd.arg("--release");
    }

    let status = cmd.status().expect("failed to spawn cargo for cookie-importer");
    if !status.success() {
        panic!("failed to build cookie-importer (exit {status:?})");
    }

    let helper_target = manifest_dir
        .join("crates/cookie-importer/target")
        .join(&profile)
        .join(binary_name());
    if !helper_target.exists() {
        panic!("cookie-importer build succeeded but binary not found at {helper_target:?}");
    }

    let destination = profile_dir.join(binary_name());
    std::fs::copy(&helper_target, &destination)
        .unwrap_or_else(|error| panic!("failed to copy cookie-importer to {destination:?}: {error}"));
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "cadence-cookie-importer.exe"
    } else {
        "cadence-cookie-importer"
    }
}

/// Ensure `resources/scrcpy-server-v<VERSION>.jar` exists. If the file is
/// already present *and* its SHA-256 matches the pinned value we skip the
/// fetch. Otherwise we try to download via `curl` and verify. Network
/// failures emit a `cargo:warning` rather than aborting the build — the
/// Android pipeline surfaces a typed `scrcpy_jar_missing` error at runtime
/// so a dev can still build-and-boot the app even without the jar.
fn fetch_scrcpy_server() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let resources_dir = manifest_dir.join("resources");
    let target = resources_dir.join(format!("scrcpy-server-v{SCRCPY_VERSION}.jar"));

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CADENCE_SKIP_SIDECAR_FETCH");
    println!("cargo:rerun-if-changed={}", target.display());

    if std::env::var_os("CADENCE_SKIP_SIDECAR_FETCH").is_some() {
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
    let fetch = Command::new("curl")
        .args(["-sSL", "-f", "-o"])
        .arg(&target)
        .arg(&url)
        .status();
    match fetch {
        Ok(status) if status.success() => {}
        Ok(status) => {
            println!(
                "cargo:warning=curl exited {status} fetching scrcpy-server from {url}. Android streaming will fail until the jar is in place."
            );
            let _ = std::fs::remove_file(&target);
            return;
        }
        Err(err) => {
            println!(
                "cargo:warning=failed to invoke curl for scrcpy-server: {err}. Install curl or drop the jar into {} manually.",
                resources_dir.display()
            );
            return;
        }
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
        let child = cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn();
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
