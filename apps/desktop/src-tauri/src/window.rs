//! Expanded and compact window modes (spec §5.1).
//!
//! The window keeps its left edge fixed and changes width, so the sidebar
//! never moves. Below 760 px the frontend switches to the compact layout on
//! its own; this command only drives the animated resize for the
//! collapse/expand button.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{State, WebviewWindow};

pub const COMPACT_WIDTH: f64 = 380.0;
/// Widths below this use the compact layout.
pub const COMPACT_BREAKPOINT: f64 = 760.0;
/// Expanding never lands on a width narrower than this.
pub const EXPANDED_MIN_WIDTH: f64 = 1080.0;
pub const EXPANDED_DEFAULT_WIDTH: f64 = 1280.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WindowMode {
    Compact,
    Expanded,
}

/// The width to return to when expanding again.
#[derive(Default)]
pub struct LastExpandedWidth(Mutex<Option<f64>>);

#[tauri::command]
#[specta::specta]
pub fn set_window_mode(
    window: WebviewWindow,
    last: State<'_, LastExpandedWidth>,
    mode: WindowMode,
) -> Result<(), String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let size = window
        .inner_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    let mut remembered = last.0.lock().unwrap_or_else(|p| p.into_inner());

    let width = match mode {
        WindowMode::Compact => {
            if size.width >= COMPACT_BREAKPOINT {
                *remembered = Some(size.width);
            }
            COMPACT_WIDTH
        }
        WindowMode::Expanded => remembered
            .unwrap_or(EXPANDED_DEFAULT_WIDTH)
            .max(EXPANDED_MIN_WIDTH),
    };
    resize_width(&window, width, size.height).map_err(|e| e.to_string())
}

/// The window's backdrop material, for the UI to match.
#[derive(Default)]
pub struct Backdrop(Mutex<Option<String>>);

/// Windows 11 draws the sidebar over Mica (spec §8.3, phase 8). Older
/// Windows keeps the solid background: the page only turns translucent
/// once it's told Mica is on.
pub fn apply_backdrop(window: &WebviewWindow, backdrop: &Backdrop) {
    if jclean_core::platform::windows_build().is_none_or(|b| b < 22_000) {
        return;
    }
    let effects = tauri::window::EffectsBuilder::new()
        .effect(tauri::window::Effect::MicaDark)
        .build();
    if window.set_effects(effects).is_ok()
        && window
            .set_background_color(Some(tauri::window::Color(0, 0, 0, 0)))
            .is_ok()
    {
        *backdrop.0.lock().unwrap_or_else(|p| p.into_inner()) = Some("mica".to_string());
    }
}

/// `"mica"` when the window has a translucent backdrop, otherwise nothing.
#[tauri::command]
#[specta::specta]
pub fn window_backdrop(backdrop: State<'_, Backdrop>) -> Option<String> {
    backdrop.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

#[cfg(target_os = "macos")]
fn resize_width(window: &WebviewWindow, width: f64, _height: f64) -> tauri::Result<()> {
    let ptr = window.ns_window()? as usize;
    window.run_on_main_thread(move || macos::animate_width(ptr, width))
}

#[cfg(not(target_os = "macos"))]
fn resize_width(window: &WebviewWindow, width: f64, height: f64) -> tauri::Result<()> {
    window.set_size(tauri::LogicalSize::new(width, height))
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos {
    use objc2_app_kit::NSWindow;

    /// Animates the window to `width` points, keeping the top-left corner
    /// in place. Must run on the main thread.
    pub fn animate_width(ns_window: usize, width: f64) {
        if ns_window == 0 {
            return;
        }
        // SAFETY: `ns_window` comes from `WebviewWindow::ns_window`, which
        // returns this window's live NSWindow, and we're on the main thread
        // (`run_on_main_thread`), where AppKit requires window calls.
        let window: &NSWindow = unsafe { &*(ns_window as *const NSWindow) };
        let mut frame = window.frame();
        frame.size.width = width;
        window.setFrame_display_animate(frame, true, true);
    }
}
