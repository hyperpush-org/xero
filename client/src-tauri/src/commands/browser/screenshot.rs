use std::io::Cursor;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use image::{ImageFormat, RgbaImage};
use tauri::{Runtime, Webview};

use crate::commands::{CommandError, CommandResult};

pub fn capture_webview<R: Runtime>(webview: &Webview<R>) -> CommandResult<String> {
    let webview_pos = webview.position().map_err(|error| {
        CommandError::system_fault(
            "browser_screenshot_position_failed",
            format!("Xero could not read the browser webview position: {error}"),
        )
    })?;
    let webview_size = webview.size().map_err(|error| {
        CommandError::system_fault(
            "browser_screenshot_size_failed",
            format!("Xero could not read the browser webview size: {error}"),
        )
    })?;

    if webview_size.width == 0 || webview_size.height == 0 {
        return Err(CommandError::user_fixable(
            "browser_screenshot_zero_area",
            "The browser webview has no visible area to capture.",
        ));
    }

    let parent = webview.window();
    let inner_pos = parent.inner_position().map_err(|error| {
        CommandError::system_fault(
            "browser_screenshot_window_pos_failed",
            format!("Xero could not read the host window position: {error}"),
        )
    })?;

    let screen_x = inner_pos.x.saturating_add(webview_pos.x);
    let screen_y = inner_pos.y.saturating_add(webview_pos.y);

    let monitors = xcap::Monitor::all().map_err(|error| {
        CommandError::system_fault(
            "browser_screenshot_monitors_failed",
            format!("Xero could not enumerate displays for screenshot: {error}"),
        )
    })?;

    let monitor = monitors
        .into_iter()
        .find(|monitor| monitor_contains(monitor, screen_x, screen_y))
        .ok_or_else(|| {
            CommandError::system_fault(
                "browser_screenshot_no_monitor",
                "Xero could not find a display containing the browser webview.",
            )
        })?;

    let monitor_x = monitor_geometry_field(&monitor, MonitorField::X)?;
    let monitor_y = monitor_geometry_field(&monitor, MonitorField::Y)?;

    let captured: RgbaImage = monitor.capture_image().map_err(|error| {
        CommandError::system_fault(
            "browser_screenshot_capture_failed",
            format!("Xero could not capture the display contents: {error}"),
        )
    })?;

    let local_x = (screen_x - monitor_x).max(0) as u32;
    let local_y = (screen_y - monitor_y).max(0) as u32;
    let crop_w = webview_size
        .width
        .min(captured.width().saturating_sub(local_x));
    let crop_h = webview_size
        .height
        .min(captured.height().saturating_sub(local_y));

    if crop_w == 0 || crop_h == 0 {
        return Err(CommandError::system_fault(
            "browser_screenshot_out_of_bounds",
            "The browser webview is outside the captured display area.",
        ));
    }

    let cropped = image::imageops::crop_imm(&captured, local_x, local_y, crop_w, crop_h).to_image();

    let mut buffer = Cursor::new(Vec::with_capacity((crop_w * crop_h * 4) as usize));
    cropped
        .write_to(&mut buffer, ImageFormat::Png)
        .map_err(|error| {
            CommandError::system_fault(
                "browser_screenshot_encode_failed",
                format!("Xero could not encode the screenshot as PNG: {error}"),
            )
        })?;

    Ok(BASE64_STANDARD.encode(buffer.into_inner()))
}

#[derive(Clone, Copy)]
enum MonitorField {
    X,
    Y,
    Width,
    Height,
}

fn monitor_contains(monitor: &xcap::Monitor, x: i32, y: i32) -> bool {
    let Ok(mx) = monitor_geometry_field(monitor, MonitorField::X) else {
        return false;
    };
    let Ok(my) = monitor_geometry_field(monitor, MonitorField::Y) else {
        return false;
    };
    let Ok(mw) = monitor_geometry_field(monitor, MonitorField::Width) else {
        return false;
    };
    let Ok(mh) = monitor_geometry_field(monitor, MonitorField::Height) else {
        return false;
    };

    x >= mx && y >= my && x < mx.saturating_add(mw) && y < my.saturating_add(mh)
}

fn monitor_geometry_field(monitor: &xcap::Monitor, field: MonitorField) -> CommandResult<i32> {
    let value: i64 = match field {
        MonitorField::X => monitor.x().map(i64::from),
        MonitorField::Y => monitor.y().map(i64::from),
        MonitorField::Width => monitor.width().map(i64::from),
        MonitorField::Height => monitor.height().map(i64::from),
    }
    .map_err(|error| {
        CommandError::system_fault(
            "browser_screenshot_monitor_field_failed",
            format!("Xero could not read display geometry: {error}"),
        )
    })?;

    i32::try_from(value).map_err(|_| {
        CommandError::system_fault(
            "browser_screenshot_monitor_field_overflow",
            "Display geometry value did not fit in an i32.",
        )
    })
}
