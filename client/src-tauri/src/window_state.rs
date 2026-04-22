use std::{fs, path::{Path, PathBuf}};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, WindowEvent};

const WINDOW_STATE_FILE_NAME: &str = "window-state.json";
const MAIN_WINDOW_LABEL: &str = "main";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
struct WindowBounds {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayArea {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl DisplayArea {
    fn from_monitor(monitor: &Monitor) -> Self {
        let position = monitor.position();
        let size = monitor.size();
        Self {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    }

    fn fully_contains(&self, bounds: WindowBounds) -> bool {
        let bounds_right = i64::from(bounds.x) + i64::from(bounds.width);
        let bounds_bottom = i64::from(bounds.y) + i64::from(bounds.height);
        let display_right = i64::from(self.x) + i64::from(self.width);
        let display_bottom = i64::from(self.y) + i64::from(self.height);

        i64::from(bounds.x) >= i64::from(self.x)
            && i64::from(bounds.y) >= i64::from(self.y)
            && bounds_right <= display_right
            && bounds_bottom <= display_bottom
    }
}

pub fn configure_main_window<R: Runtime>(app: AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };

    apply_startup_window_bounds(&app, &window);
    register_window_state_persistence(app, &window);

    if let Err(error) = window.show() {
        eprintln!("Cadence failed to show the main window after geometry bootstrap: {error}");
    }
}

fn apply_startup_window_bounds<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let display_areas = match display_areas_for_window(window) {
        Ok(display_areas) => display_areas,
        Err(error) => {
            eprintln!("Cadence could not inspect available displays for window restore: {error}");
            Vec::new()
        }
    };

    let saved_bounds = match load_window_bounds(app) {
        Ok(saved_bounds) => saved_bounds,
        Err(error) => {
            eprintln!("Cadence could not load saved window bounds; using first-launch fullscreen defaults instead: {error}");
            None
        }
    };

    let startup_bounds = match saved_bounds {
        Some(bounds) if saved_bounds_fit_display_areas(bounds, &display_areas) => Some(bounds),
        Some(bounds) => {
            eprintln!(
                "Cadence ignored saved window bounds outside the active display layout: x={}, y={}, width={}, height={}",
                bounds.x, bounds.y, bounds.width, bounds.height
            );
            default_window_bounds(window)
        }
        None => default_window_bounds(window),
    };

    if let Some(bounds) = startup_bounds {
        if let Err(error) = apply_window_bounds(window, bounds) {
            eprintln!("Cadence failed to apply startup window bounds: {error}");
        }
    }
}

fn register_window_state_persistence<R: Runtime>(app: AppHandle<R>, window: &WebviewWindow<R>) {
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { .. } = event {
            let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
                return;
            };

            if let Err(error) = save_current_window_bounds(&app, &window) {
                eprintln!("Cadence failed to persist main window bounds during shutdown: {error}");
            }
        }
    });
}

fn default_window_bounds<R: Runtime>(window: &WebviewWindow<R>) -> Option<WindowBounds> {
    current_or_primary_monitor(window).map(|monitor| {
        let position = monitor.position();
        let size = monitor.size();
        WindowBounds {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    })
}

fn current_or_primary_monitor<R: Runtime>(window: &WebviewWindow<R>) -> Option<Monitor> {
    window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .or_else(|| {
            window
                .available_monitors()
                .ok()
                .and_then(|monitors| monitors.into_iter().next())
        })
}

fn display_areas_for_window<R: Runtime>(window: &WebviewWindow<R>) -> Result<Vec<DisplayArea>, String> {
    window
        .available_monitors()
        .map(|monitors| monitors.iter().map(DisplayArea::from_monitor).collect())
        .map_err(|error| format!("available monitor lookup failed: {error}"))
}

fn saved_bounds_fit_display_areas(bounds: WindowBounds, display_areas: &[DisplayArea]) -> bool {
    display_areas.is_empty()
        || display_areas
            .iter()
            .any(|display_area| display_area.fully_contains(bounds))
}

fn apply_window_bounds<R: Runtime>(window: &WebviewWindow<R>, bounds: WindowBounds) -> Result<(), String> {
    window
        .set_size(PhysicalSize::new(bounds.width, bounds.height))
        .map_err(|error| format!("window resize failed: {error}"))?;
    window
        .set_position(PhysicalPosition::new(bounds.x, bounds.y))
        .map_err(|error| format!("window move failed: {error}"))?;
    Ok(())
}

fn save_current_window_bounds<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) -> Result<(), String> {
    let position = window
        .outer_position()
        .map_err(|error| format!("window position lookup failed: {error}"))?;
    let size = window
        .inner_size()
        .map_err(|error| format!("window size lookup failed: {error}"))?;

    save_window_bounds(
        app,
        WindowBounds {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        },
    )
}

fn save_window_bounds<R: Runtime>(app: &AppHandle<R>, bounds: WindowBounds) -> Result<(), String> {
    let path = resolve_window_state_path(app)?;
    save_window_bounds_to_path(&path, bounds)
}

fn load_window_bounds<R: Runtime>(app: &AppHandle<R>) -> Result<Option<WindowBounds>, String> {
    let path = resolve_window_state_path(app)?;
    load_window_bounds_from_path(&path)
}

fn resolve_window_state_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("app data directory lookup failed: {error}"))?;

    fs::create_dir_all(&app_data_dir)
        .map_err(|error| format!("app data directory creation failed: {error}"))?;

    Ok(app_data_dir.join(WINDOW_STATE_FILE_NAME))
}

fn save_window_bounds_to_path(path: &Path, bounds: WindowBounds) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("window state directory creation failed: {error}"))?;
    }

    let json = serde_json::to_string(&bounds)
        .map_err(|error| format!("window state serialization failed: {error}"))?;

    fs::write(path, json).map_err(|error| format!("window state write failed: {error}"))
}

fn load_window_bounds_from_path(path: &Path) -> Result<Option<WindowBounds>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let json = fs::read_to_string(path)
        .map_err(|error| format!("window state read failed: {error}"))?;

    let bounds: WindowBounds = serde_json::from_str(&json)
        .map_err(|error| format!("window state parse failed: {error}"))?;

    Ok(Some(bounds))
}

#[cfg(test)]
mod tests {
    use super::{
        load_window_bounds_from_path, save_window_bounds_to_path, saved_bounds_fit_display_areas,
        DisplayArea, WindowBounds,
    };

    #[test]
    fn load_window_bounds_returns_none_when_state_file_is_missing() {
        let root = tempfile::tempdir().expect("temp dir");
        let state_path = root.path().join("window-state.json");

        let loaded = load_window_bounds_from_path(&state_path).expect("missing state should not fail");

        assert_eq!(loaded, None);
    }

    #[test]
    fn save_and_load_window_bounds_round_trip() {
        let root = tempfile::tempdir().expect("temp dir");
        let state_path = root.path().join("window-state.json");
        let expected = WindowBounds {
            x: 128,
            y: 64,
            width: 1512,
            height: 982,
        };

        save_window_bounds_to_path(&state_path, expected).expect("window bounds should save");
        let loaded = load_window_bounds_from_path(&state_path)
            .expect("window bounds should load")
            .expect("saved state should exist");

        assert_eq!(loaded, expected);
    }

    #[test]
    fn saved_bounds_must_fit_entirely_within_a_display_area() {
        let display_areas = [DisplayArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }];

        assert!(saved_bounds_fit_display_areas(
            WindowBounds {
                x: 100,
                y: 100,
                width: 1600,
                height: 900,
            },
            &display_areas,
        ));
        assert!(!saved_bounds_fit_display_areas(
            WindowBounds {
                x: 1200,
                y: 100,
                width: 900,
                height: 900,
            },
            &display_areas,
        ));
    }
}
