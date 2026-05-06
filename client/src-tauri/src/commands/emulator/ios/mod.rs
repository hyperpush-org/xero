//! iOS Simulator pipeline. Fully implemented only on macOS — on other hosts
//! every entry point is cfg-gated away and the titlebar button is hidden.

pub mod sdk;

#[cfg(target_os = "macos")]
pub mod cg_input;
#[cfg(target_os = "macos")]
pub mod helper;
#[cfg(target_os = "macos")]
pub mod helper_client;
#[cfg(target_os = "macos")]
pub mod idb_client;
#[cfg(target_os = "macos")]
pub mod idb_companion;
#[cfg(target_os = "macos")]
pub mod input;
#[cfg(target_os = "macos")]
pub mod session;
#[cfg(target_os = "macos")]
pub mod xcrun;

#[cfg(target_os = "macos")]
pub use session::{list_devices, spawn, IosSession, SimulatorDescriptor, SpawnArgs};
