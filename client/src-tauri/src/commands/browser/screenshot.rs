use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
#[cfg(not(target_os = "macos"))]
use image::{ImageFormat, RgbaImage};
use tauri::{Runtime, Webview};

use crate::commands::{CommandError, CommandResult};

#[cfg(target_os = "macos")]
use {
    block2::RcBlock,
    objc2::{runtime::AnyObject, MainThreadMarker},
    objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSImage, NSView,
    },
    objc2_foundation::{NSData, NSDictionary, NSError},
    objc2_web_kit::{WKSnapshotConfiguration, WKWebView},
    std::{ptr::NonNull, sync::mpsc, time::Duration},
};

#[cfg(target_os = "macos")]
const WEBKIT_SNAPSHOT_TIMEOUT: Duration = Duration::from_millis(600);
#[cfg(target_os = "macos")]
const VIEW_CACHE_TIMEOUT: Duration = Duration::from_millis(900);

#[cfg(target_os = "macos")]
pub fn capture_webview<R: Runtime>(webview: &Webview<R>) -> CommandResult<String> {
    match capture_webview_cached_view(webview) {
        Ok(snapshot) => Ok(snapshot),
        Err(cached_view_error) => {
            if cached_view_error.code == "browser_screenshot_zero_area" {
                return Err(cached_view_error);
            }
            capture_webview_snapshot(webview)
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn capture_webview<R: Runtime>(webview: &Webview<R>) -> CommandResult<String> {
    use std::io::Cursor;

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

#[cfg(target_os = "macos")]
fn capture_webview_snapshot<R: Runtime>(webview: &Webview<R>) -> CommandResult<String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    webview
        .with_webview(move |platform_webview| {
            let Some(mtm) = MainThreadMarker::new() else {
                let _ = sender.send(Err(CommandError::system_fault(
                    "browser_screenshot_snapshot_thread_failed",
                    "Xero could not access the browser webview on the macOS main thread.",
                )));
                return;
            };

            let raw_webview = platform_webview.inner();
            let wk_webview = unsafe { &*(raw_webview.cast::<WKWebView>()) };
            let ns_view = unsafe { &*(raw_webview.cast::<NSView>()) };
            let configuration = unsafe { WKSnapshotConfiguration::new(mtm) };
            let bounds = ns_view.bounds();
            if bounds.size.width.is_finite()
                && bounds.size.height.is_finite()
                && bounds.size.width > 0.0
                && bounds.size.height > 0.0
            {
                unsafe {
                    configuration.setRect(bounds);
                }
            }
            unsafe {
                configuration.setAfterScreenUpdates(false);
            }

            let completion = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
                let result = unsafe { nsimage_png_base64(image, error) };
                let _ = sender.send(result);
            });
            unsafe {
                wk_webview.takeSnapshotWithConfiguration_completionHandler(
                    Some(&configuration),
                    &completion,
                );
            }
        })
        .map_err(|error| {
            CommandError::system_fault(
                "browser_screenshot_snapshot_failed",
                format!("Xero could not request a browser webview snapshot: {error}"),
            )
        })?;

    receiver
        .recv_timeout(WEBKIT_SNAPSHOT_TIMEOUT)
        .map_err(|_| {
            CommandError::system_fault(
                "browser_screenshot_snapshot_timeout",
                "The browser webview did not finish rendering a snapshot.",
            )
        })?
}

#[cfg(target_os = "macos")]
fn capture_webview_cached_view<R: Runtime>(webview: &Webview<R>) -> CommandResult<String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    webview
        .with_webview(move |platform_webview| {
            let ns_view = unsafe { &*(platform_webview.inner().cast::<NSView>()) };
            let result = nsview_png_base64(ns_view);
            let _ = sender.send(result);
        })
        .map_err(|error| {
            CommandError::system_fault(
                "browser_screenshot_view_snapshot_failed",
                format!("Xero could not request a cached browser webview snapshot: {error}"),
            )
        })?;

    receiver.recv_timeout(VIEW_CACHE_TIMEOUT).map_err(|_| {
        CommandError::system_fault(
            "browser_screenshot_view_snapshot_timeout",
            "The browser webview did not respond while rendering a snapshot.",
        )
    })?
}

#[cfg(target_os = "macos")]
fn nsview_png_base64(view: &NSView) -> CommandResult<String> {
    let bounds = view.bounds();
    if !bounds.size.width.is_finite()
        || !bounds.size.height.is_finite()
        || bounds.size.width <= 0.0
        || bounds.size.height <= 0.0
    {
        return Err(CommandError::user_fixable(
            "browser_screenshot_zero_area",
            "The browser webview has no visible area to capture.",
        ));
    }

    let Some(bitmap) = view.bitmapImageRepForCachingDisplayInRect(bounds) else {
        return Err(CommandError::system_fault(
            "browser_screenshot_view_bitmap_failed",
            "Xero could not create a bitmap for the browser webview.",
        ));
    };
    view.cacheDisplayInRect_toBitmapImageRep(bounds, &bitmap);
    bitmap_png_base64(&bitmap)
}

#[cfg(target_os = "macos")]
unsafe fn nsimage_png_base64(image: *mut NSImage, error: *mut NSError) -> CommandResult<String> {
    if !error.is_null() {
        return Err(CommandError::system_fault(
            "browser_screenshot_snapshot_failed",
            "The browser webview could not produce a snapshot image.",
        ));
    }

    let Some(image) = image.as_ref() else {
        return Err(CommandError::system_fault(
            "browser_screenshot_snapshot_empty",
            "The browser webview snapshot did not include an image.",
        ));
    };
    let Some(tiff_data) = image.TIFFRepresentation() else {
        return Err(CommandError::system_fault(
            "browser_screenshot_snapshot_tiff_failed",
            "Xero could not read the browser webview snapshot image.",
        ));
    };
    let Some(bitmap) = NSBitmapImageRep::imageRepWithData(&tiff_data) else {
        return Err(CommandError::system_fault(
            "browser_screenshot_snapshot_bitmap_failed",
            "Xero could not convert the browser webview snapshot to a bitmap.",
        ));
    };
    bitmap_png_base64(&bitmap)
}

#[cfg(target_os = "macos")]
fn bitmap_png_base64(bitmap: &NSBitmapImageRep) -> CommandResult<String> {
    let properties = NSDictionary::<NSBitmapImageRepPropertyKey, AnyObject>::new();
    let Some(png_data) = (unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
    }) else {
        return Err(CommandError::system_fault(
            "browser_screenshot_snapshot_png_failed",
            "Xero could not encode the browser webview snapshot as PNG.",
        ));
    };

    Ok(BASE64_STANDARD.encode(nsdata_bytes(&png_data)))
}

#[cfg(target_os = "macos")]
fn nsdata_bytes(data: &NSData) -> Vec<u8> {
    let length = data.length();
    let mut bytes = vec![0_u8; length];
    if length > 0 {
        let pointer = NonNull::new(bytes.as_mut_ptr().cast()).expect("non-empty vec pointer");
        unsafe {
            data.getBytes_length(pointer, length);
        }
    }
    bytes
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy)]
enum MonitorField {
    X,
    Y,
    Width,
    Height,
}

#[cfg(not(target_os = "macos"))]
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

#[cfg(not(target_os = "macos"))]
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
