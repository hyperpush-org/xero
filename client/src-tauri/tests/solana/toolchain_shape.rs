//! Missing-toolchain state tests — we just verify the probe returns a
//! well-shaped struct so the frontend "missing SDK" panel always has
//! something to render. Whether individual tools are found depends on the
//! CI host.

use xero_desktop_lib::commands::solana::toolchain;

pub fn toolchain_probe_returns_well_shaped_struct_on_this_host() {
    let status = toolchain::probe();
    // Every field is populated (present flag is always either true or
    // false — never missing) regardless of host toolchain.
    let _ = status.solana_cli.present;
    let _ = status.anchor.present;
    let _ = status.cargo_build_sbf.present;
    let _ = status.rust.present;
    let _ = status.node.present;
    let _ = status.pnpm.present;
    let _ = status.surfpool.present;
    let _ = status.trident.present;
    let _ = status.codama.present;
    let _ = status.solana_verify.present;

    if cfg!(target_os = "windows") {
        assert!(status.wsl2.is_some(), "Windows hosts must probe wsl2");
    } else {
        assert!(
            status.wsl2.is_none(),
            "non-Windows hosts must not probe wsl2"
        );
    }
}

pub fn toolchain_probe_serializes_to_camel_case_json() {
    let status = toolchain::probe();
    let json = serde_json::to_string(&status).expect("toolchain status must be serializable");
    // Frontend expects camelCase so the SdkStatus-style panel can render.
    assert!(json.contains("\"solanaCli\""));
    assert!(json.contains("\"cargoBuildSbf\""));
    assert!(json.contains("\"solanaVerify\""));
    // Fields must never emit snake_case.
    assert!(!json.contains("\"solana_cli\""));
    assert!(!json.contains("\"cargo_build_sbf\""));
}
