# Solana toolchain resources

Cadence resolves Solana workbench tools in this order:

1. Files bundled here inside the Tauri resource directory.
2. Files installed on first use into the app data directory under `solana-toolchain/`.
3. Host PATH and common developer install directories.

The first-run installer currently provisions:

- Agave `v3.1.13` via `agave-install-init`, installed with `--no-modify-path`.
- Anchor CLI `v1.0.0` as a managed executable.

For release builds that need fully offline startup, place a compatible tree here
before packaging. The resolver checks these layouts:

```text
solana-toolchain/
├── bin/
│   └── anchor
└── agave/install/active_release/bin/
    ├── solana
    ├── solana-test-validator
    ├── cargo-build-sbf
    └── spl-token
```

Large binaries are intentionally not committed. Use the managed installer path
for development builds and cache this directory only in release packaging jobs
that are explicitly allowed to redistribute the upstream artifacts.
