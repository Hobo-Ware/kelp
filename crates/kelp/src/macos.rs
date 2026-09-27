use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub const TRAFFIC_LIGHTS_W: f32 = 78.0;

/// Folds the title bar into the app: an empty compact toolbar makes AppKit
/// center the window buttons in a 38pt strip that the tab bar draws under.
#[cfg(target_os = "macos")]
pub fn unify_titlebar(frame: &eframe::Frame) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSTitlebarSeparatorStyle, NSToolbar, NSView, NSWindowTitleVisibility, NSWindowToolbarStyle,
    };

    let Ok(handle) = frame.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: winit hands out a live NSView for the lifetime of the window,
    // and we are on the main thread.
    let view: &NSView = unsafe { appkit.ns_view.cast().as_ref() };
    let Some(window) = view.window() else {
        return;
    };
    let toolbar = NSToolbar::new(mtm);
    window.setToolbar(Some(&toolbar));
    window.setToolbarStyle(NSWindowToolbarStyle::UnifiedCompact);
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    window.setTitlebarAppearsTransparent(true);
    window.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::None);
}

#[cfg(not(target_os = "macos"))]
pub fn unify_titlebar(_frame: &eframe::Frame) {}
