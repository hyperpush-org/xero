use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;
use tauri::{AppHandle, Runtime};

use crate::commands::{CommandError, CommandResult};

const MAX_DOCK_ICON_DATA_URL_BYTES: usize = 1_000_000;
const PNG_DATA_URL_PREFIX: &str = "data:image/png;base64,";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetThemeDockIconRequest {
    png_data_url: String,
}

#[tauri::command]
pub fn set_theme_dock_icon<R: Runtime>(
    app: AppHandle<R>,
    request: SetThemeDockIconRequest,
) -> CommandResult<()> {
    let png_bytes = decode_png_data_url(&request.png_data_url)?;
    validate_png_bytes(&png_bytes)?;
    set_platform_icon(app, png_bytes)
}

fn decode_png_data_url(data_url: &str) -> CommandResult<Vec<u8>> {
    if data_url.len() > MAX_DOCK_ICON_DATA_URL_BYTES {
        return Err(CommandError::user_fixable(
            "dock_icon_too_large",
            "The generated Dock icon image is too large.",
        ));
    }

    let encoded = data_url
        .trim()
        .strip_prefix(PNG_DATA_URL_PREFIX)
        .ok_or_else(|| CommandError::invalid_request("request.pngDataUrl"))?;

    STANDARD.decode(encoded).map_err(|error| {
        CommandError::user_fixable(
            "dock_icon_decode_failed",
            format!("The generated Dock icon image could not be decoded: {error}"),
        )
    })
}

fn validate_png_bytes(bytes: &[u8]) -> CommandResult<()> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map(|_| ())
        .map_err(|error| {
            CommandError::user_fixable(
                "dock_icon_invalid_png",
                format!("The generated Dock icon image was not a valid PNG: {error}"),
            )
        })
}

#[cfg(target_os = "macos")]
fn set_platform_icon<R: Runtime>(app: AppHandle<R>, png_bytes: Vec<u8>) -> CommandResult<()> {
    app.run_on_main_thread(move || {
        if let Err(error) = macos::set_application_icon(&png_bytes) {
            eprintln!("[dock-icon] failed to apply themed Dock icon: {error}");
        }
    })
    .map_err(|error| {
        CommandError::system_fault(
            "dock_icon_schedule_failed",
            format!("Xero could not schedule the Dock icon update: {error}"),
        )
    })
}

#[cfg(not(target_os = "macos"))]
fn set_platform_icon<R: Runtime>(app: AppHandle<R>, png_bytes: Vec<u8>) -> CommandResult<()> {
    use tauri::Manager;

    let icon = tauri::image::Image::from_bytes(&png_bytes).map_err(|error| {
        CommandError::user_fixable(
            "app_icon_decode_failed",
            format!("The generated application icon image could not be decoded: {error}"),
        )
    })?;

    if let Some(window) = app.get_webview_window("main") {
        window.set_icon(icon).map_err(|error| {
            CommandError::system_fault(
                "app_icon_update_failed",
                format!("Xero could not update the application icon: {error}"),
            )
        })?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::{rc::autoreleasepool, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    pub(super) fn set_application_icon(png_bytes: &[u8]) -> Result<(), String> {
        autoreleasepool(|_| {
            let mtm =
                MainThreadMarker::new().ok_or("Dock icon update did not run on the main thread")?;
            let data = NSData::with_bytes(png_bytes);
            let image = NSImage::initWithData(mtm.alloc(), &data)
                .ok_or("AppKit could not create an image from the generated PNG")?;
            let app = NSApplication::sharedApplication(mtm);

            unsafe {
                app.setApplicationIconImage(Some(&image));
            }

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_png_data_url, MAX_DOCK_ICON_DATA_URL_BYTES};

    #[test]
    fn decode_png_data_url_accepts_png_data_urls() {
        let decoded = decode_png_data_url("data:image/png;base64,AQIDBA==").expect("decoded");

        assert_eq!(decoded, vec![1, 2, 3, 4]);
    }

    #[test]
    fn decode_png_data_url_rejects_missing_png_prefix() {
        let error = decode_png_data_url("AQIDBA==").expect_err("prefix required");

        assert_eq!(error.code, "invalid_request");
    }

    #[test]
    fn decode_png_data_url_rejects_oversized_payloads() {
        let payload = format!(
            "data:image/png;base64,{}",
            "A".repeat(MAX_DOCK_ICON_DATA_URL_BYTES)
        );
        let error = decode_png_data_url(&payload).expect_err("payload too large");

        assert_eq!(error.code, "dock_icon_too_large");
    }
}
