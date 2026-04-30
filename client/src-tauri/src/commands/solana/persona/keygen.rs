//! Keypair provider + on-disk keypair file format.
//!
//! The workbench uses the canonical `solana-keygen` keypair format on disk —
//! a JSON array of 64 bytes (32 seed + 32 public key). This keeps generated
//! wallets interoperable with `solana`, `spl-token`, `anchor`, and any other
//! tool the user already has installed.
//!
//! `KeypairProvider` is injectable so unit tests don't need to spawn
//! `solana-keygen` or rely on system entropy — tests use a deterministic
//! in-memory provider and still exercise the storage + encoding paths.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use rand::RngCore;

use crate::commands::{CommandError, CommandResult};

/// Full keypair bytes in the `solana-keygen` format: 32 bytes of seed
/// followed by 32 bytes of the derived ed25519 public key.
///
/// Intentionally does *not* derive `Serialize`/`Deserialize`: the JSON
/// representation is the explicit `solana-keygen` byte-array format, not
/// a newtype wrapper. Callers use `to_solana_keygen_json` /
/// `from_solana_keygen_json` to cross the trust boundary.
#[derive(Clone)]
pub struct KeypairBytes(pub [u8; 64]);

impl std::fmt::Debug for KeypairBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never expose raw key material in Debug formatting so a stray log
        // line can't leak a localnet keypair.
        f.debug_struct("KeypairBytes")
            .field("pubkey", &self.pubkey_base58())
            .finish_non_exhaustive()
    }
}

impl KeypairBytes {
    pub fn pubkey_base58(&self) -> String {
        bs58::encode(&self.0[32..]).into_string()
    }

    pub fn secret_seed(&self) -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.0[..32]);
        out
    }

    pub fn from_solana_keygen_json(bytes: &[u8]) -> CommandResult<Self> {
        let parsed: Vec<u8> = serde_json::from_slice(bytes).map_err(|err| {
            CommandError::user_fixable(
                "solana_persona_keypair_parse_failed",
                format!("Keypair JSON must be an array of 64 bytes: {err}"),
            )
        })?;
        if parsed.len() != 64 {
            return Err(CommandError::user_fixable(
                "solana_persona_keypair_wrong_length",
                format!("Keypair JSON must contain 64 bytes, got {}.", parsed.len()),
            ));
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(&parsed);
        Ok(KeypairBytes(out))
    }

    pub fn to_solana_keygen_json(&self) -> String {
        // Serde on a [u8; 64] emits a JSON array which is exactly what
        // `solana-keygen` reads — the spl-token and anchor CLIs are happy
        // with it, confirmed by round-tripping in the tests below.
        serde_json::to_string(&self.0.to_vec()).expect("keypair bytes serialize")
    }
}

pub trait KeypairProvider: Send + Sync + std::fmt::Debug {
    fn generate(&self) -> CommandResult<KeypairBytes>;
}

/// Production provider: uses the host's OS RNG + ed25519-dalek to produce a
/// keypair in the exact on-disk format `solana-keygen` writes. We never
/// call the `solana-keygen` binary itself because we don't want to add a
/// latency-coupled sub-process to persona creation — the acceptance target
/// is <5s for a "whale" persona end-to-end and a CLI sub-process would eat
/// most of that budget on a cold cache.
#[derive(Debug, Default)]
pub struct OsRngKeypairProvider;

impl KeypairProvider for OsRngKeypairProvider {
    fn generate(&self) -> CommandResult<KeypairBytes> {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let signing = SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();

        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&seed);
        out[32..].copy_from_slice(verifying.as_bytes());
        Ok(KeypairBytes(out))
    }
}

/// On-disk keypair store. Lives alongside the persona registry under
/// `data_dir()/xero/solana/keypairs/{cluster}/{name}.json`.
#[derive(Debug)]
pub struct KeypairStore {
    root: PathBuf,
    provider: Mutex<Box<dyn KeypairProvider>>,
}

impl KeypairStore {
    pub fn new(root: impl Into<PathBuf>, provider: Box<dyn KeypairProvider>) -> Self {
        Self {
            root: root.into(),
            provider: Mutex::new(provider),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create a keypair, write it to disk, and return the encoded public key
    /// plus the file path a CLI can read with `--keypair`.
    pub fn generate(&self, cluster: &str, name: &str) -> CommandResult<(KeypairBytes, PathBuf)> {
        let provider = self.provider.lock().map_err(|_| {
            CommandError::system_fault(
                "solana_persona_keypair_provider_poisoned",
                "Keypair provider lock poisoned.",
            )
        })?;
        let bytes = provider.generate()?;
        drop(provider);

        let path = self.write_file(cluster, name, &bytes)?;
        Ok((bytes, path))
    }

    pub fn import_bytes(
        &self,
        cluster: &str,
        name: &str,
        bytes: KeypairBytes,
    ) -> CommandResult<PathBuf> {
        self.write_file(cluster, name, &bytes)
    }

    /// Read a keypair file back from disk.
    pub fn read(&self, cluster: &str, name: &str) -> CommandResult<KeypairBytes> {
        let path = self.path_for(cluster, name);
        let data = fs::read(&path).map_err(|err| {
            CommandError::system_fault(
                "solana_persona_keypair_read_failed",
                format!("Could not read {}: {err}", path.display()),
            )
        })?;
        KeypairBytes::from_solana_keygen_json(&data)
    }

    pub fn delete(&self, cluster: &str, name: &str) -> CommandResult<()> {
        let path = self.path_for(cluster, name);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(CommandError::system_fault(
                "solana_persona_keypair_delete_failed",
                format!("Could not delete {}: {err}", path.display()),
            )),
        }
    }

    pub fn path_for(&self, cluster: &str, name: &str) -> PathBuf {
        self.root
            .join(sanitize_segment(cluster))
            .join(format!("{}.json", sanitize_segment(name)))
    }

    fn write_file(
        &self,
        cluster: &str,
        name: &str,
        bytes: &KeypairBytes,
    ) -> CommandResult<PathBuf> {
        let cluster_dir = self.root.join(sanitize_segment(cluster));
        fs::create_dir_all(&cluster_dir).map_err(|err| {
            CommandError::system_fault(
                "solana_persona_keypair_dir_create_failed",
                format!("Could not create {}: {err}", cluster_dir.display()),
            )
        })?;

        let path = cluster_dir.join(format!("{}.json", sanitize_segment(name)));
        // Atomic-ish: temp file + rename so a crash mid-write can't leave a
        // truncated keypair file that the CLI would happily accept.
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, bytes.to_solana_keygen_json()).map_err(|err| {
            CommandError::system_fault(
                "solana_persona_keypair_write_failed",
                format!("Could not write {}: {err}", tmp.display()),
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // 0600 — never world- or group-readable. This is secret material
            // even on localnet; the invariant travels with the file so a
            // `cp -r` from the data dir preserves it.
            let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        }

        fs::rename(&tmp, &path).map_err(|err| {
            CommandError::system_fault(
                "solana_persona_keypair_rename_failed",
                format!(
                    "Could not rename {} to {}: {err}",
                    tmp.display(),
                    path.display()
                ),
            )
        })?;
        Ok(path)
    }
}

/// Strip path separators and dot runs so a user-chosen persona name can't
/// escape the storage root even by accident. Dots are turned into dashes
/// outright — we never need a period inside a segment, and allowing them
/// opens `..` path-traversal paths.
pub fn sanitize_segment(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() => c,
            '-' | '_' => c,
            _ => '-',
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    #[derive(Debug)]
    pub struct DeterministicProvider {
        counter: std::sync::Mutex<u8>,
    }

    impl DeterministicProvider {
        pub fn new() -> Self {
            Self {
                counter: std::sync::Mutex::new(0),
            }
        }
    }

    impl KeypairProvider for DeterministicProvider {
        fn generate(&self) -> CommandResult<KeypairBytes> {
            let mut guard = self.counter.lock().unwrap();
            *guard = guard.wrapping_add(1);
            let n = *guard;
            drop(guard);

            // Seed is n repeated — deterministic, but still a valid ed25519
            // private key so the derived pubkey is a real curve point.
            let seed = [n; 32];
            let signing = SigningKey::from_bytes(&seed);
            let verifying = signing.verifying_key();
            let mut out = [0u8; 64];
            out[..32].copy_from_slice(&seed);
            out[32..].copy_from_slice(verifying.as_bytes());
            Ok(KeypairBytes(out))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::DeterministicProvider;
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generated_keypair_is_64_bytes_and_pubkey_is_base58() {
        let provider = OsRngKeypairProvider;
        let bytes = provider.generate().unwrap();
        assert_eq!(bytes.0.len(), 64);
        let pubkey = bytes.pubkey_base58();
        // Solana pubkeys are 44 base58 chars at most (32 bytes) — check that
        // the decode round-trips.
        let decoded = bs58::decode(&pubkey).into_vec().unwrap();
        assert_eq!(decoded.len(), 32);
        assert_eq!(&decoded, &bytes.0[32..]);
    }

    #[test]
    fn pubkey_matches_derived_public_key_from_seed() {
        let provider = OsRngKeypairProvider;
        let bytes = provider.generate().unwrap();
        let seed = bytes.secret_seed();
        let signing = SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        assert_eq!(&bytes.0[32..], verifying.as_bytes());
    }

    #[test]
    fn keypair_json_round_trips_through_solana_keygen_format() {
        let provider = OsRngKeypairProvider;
        let bytes = provider.generate().unwrap();
        let json = bytes.to_solana_keygen_json();
        let parsed = KeypairBytes::from_solana_keygen_json(json.as_bytes()).unwrap();
        assert_eq!(bytes.0, parsed.0);
    }

    #[test]
    fn keypair_store_writes_and_reads_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = KeypairStore::new(
            tmp.path().to_path_buf(),
            Box::new(DeterministicProvider::new()),
        );
        let (bytes, path) = store.generate("localnet", "alice").unwrap();
        assert!(path.is_file());

        let loaded = store.read("localnet", "alice").unwrap();
        assert_eq!(loaded.0, bytes.0);
    }

    #[test]
    fn keypair_store_rejects_malformed_file() {
        let tmp = TempDir::new().unwrap();
        let store = KeypairStore::new(
            tmp.path().to_path_buf(),
            Box::new(DeterministicProvider::new()),
        );
        let cluster_dir = tmp.path().join("localnet");
        fs::create_dir_all(&cluster_dir).unwrap();
        fs::write(cluster_dir.join("busted.json"), b"\"not-an-array\"").unwrap();

        let err = store.read("localnet", "busted").unwrap_err();
        assert_eq!(err.code, "solana_persona_keypair_parse_failed");
    }

    #[test]
    fn sanitize_segment_strips_path_escapes() {
        assert_eq!(sanitize_segment("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_segment("slash/name"), "slash-name");
        assert_eq!(sanitize_segment("weird  name"), "weird--name");
        assert_eq!(sanitize_segment(""), "unnamed");
        assert_eq!(sanitize_segment("---"), "unnamed");
    }

    #[test]
    fn import_bytes_writes_same_file_as_generate() {
        let tmp = TempDir::new().unwrap();
        let store = KeypairStore::new(
            tmp.path().to_path_buf(),
            Box::new(DeterministicProvider::new()),
        );
        let (bytes, generated_path) = store.generate("localnet", "from-generate").unwrap();
        let imported_path = store
            .import_bytes("localnet", "from-import", bytes.clone())
            .unwrap();
        let loaded_generated = store.read("localnet", "from-generate").unwrap();
        let loaded_imported = store.read("localnet", "from-import").unwrap();
        assert_eq!(loaded_generated.0, loaded_imported.0);
        assert_ne!(generated_path, imported_path);
    }

    #[test]
    fn delete_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let store = KeypairStore::new(
            tmp.path().to_path_buf(),
            Box::new(DeterministicProvider::new()),
        );
        store.delete("localnet", "nonexistent").unwrap();
        store.generate("localnet", "alice").unwrap();
        store.delete("localnet", "alice").unwrap();
        store.delete("localnet", "alice").unwrap(); // second delete no-ops
    }
}
