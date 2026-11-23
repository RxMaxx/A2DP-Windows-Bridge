//! The main window (960×640, 760×640 for the setup), and the rule that ends
//! the UI process: a closed window is gone but the process stays, so that
//! opening it again from the tray is instant, and the process exits after
//! no window was open for a few minutes (the core keeps streaming without
//! it). With "Minimize to System Tray" off, closing the main window exits
//! BlueCaster, core included, as the tray's Exit does (v1).
//!
//! Ported from app/src/ui/src-tauri/src/windows.rs (the flyout: flyout.rs).

use std::time::{Duration, Instant};

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Bounds, Global, Pixels, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, px, size,
};
use ipc::Method;
use raw_window_handle::RawWindowHandle;

use crate::app::{self, Route};
use crate::shell::MainWindow;
use crate::theme::ActivePalette as _;

/// Exit after no window has been open this long
const HIDDEN_EXIT_AFTER: Duration = Duration::from_secs(180);
/// The main window's size during the first-run setup (logical pixels)
const SETUP_WIDTH: f32 = 760.;
const SETUP_HEIGHT: f32 = 640.;
const MAIN_WIDTH: f32 = 960.;
const MAIN_HEIGHT: f32 = 640.;
/// The main window's minimum: Home keeps two columns of devices (issue #86)
const MAIN_MIN_WIDTH: f32 = 960.;
const MAIN_MIN_HEIGHT: f32 = 520.;

/// The icon resource build.rs embeds app.ico as
const ICON_RESOURCE: u16 = 101;

#[derive(Default)]
struct Windows {
    main: Option<AnyWindowHandle>,
    main_hwnd: isize,
    /// Where the main window was when it closed: it opens there again
    main_bounds: Option<WindowBounds>,
    /// The main window's bounds while the setup has it
    before_setup: Option<WindowBounds>,
}

impl Global for Windows {}

fn windows(cx: &mut App) -> &mut Windows {
    cx.default_global::<Windows>()
}

/// Mica needs Windows 11 (build 22000). On Windows 10 a transparent window
/// would show the desktop through it, so there the windows stay opaque.
pub fn mica_supported() -> bool {
    windows_version::OsVersion::current().build >= 22000
}

/// A window's Win32 handle (0 when it has none)
pub fn hwnd(window: &Window) -> isize {
    match raw_window_handle::HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    }
}

/// The main window's Win32 handle, for a system dialog to stand over (0
/// when it is not open)
pub fn main_hwnd(cx: &App) -> isize {
    cx.try_global::<Windows>().map_or(0, |w| if w.main.is_some() { w.main_hwnd } else { 0 })
}

/// No window of ours shows: none is open, or every one is minimized (the
/// flyout is removed when it hides). A window that cannot be asked (it is
/// being updated right now) counts as shown.
pub fn none_shown(cx: &mut App) -> bool {
    cx.windows()
        .into_iter()
        .all(|handle| handle.update(cx, |_, window, _| win_sys::window::is_minimized(hwnd(window))).unwrap_or(false))
}

/// The title bars follow the app's theme, not Windows' own
pub fn apply_theme(cx: &mut App) {
    let dark = cx.palette().dark;
    for handle in cx.windows() {
        let _ = handle.update(cx, |_, window, _| {
            win_sys::window::set_dark_title_bar(hwnd(window), dark);
        });
    }
}

/// Shows the main window; `route` opens that screen
pub fn show_main(route: Option<Route>, focus: bool, cx: &mut App) {
    if let Some(route) = route {
        app::navigate(route, cx);
    }
    crate::flyout_window::hide(cx);
    if let Some(main) = windows(cx).main {
        let shown = main.update(cx, |_, window, _| window.activate_window()).is_ok();
        if shown {
            return;
        }
        windows(cx).main = None;
    }
    let setup = app::state(cx).read(cx).setup_active;
    let (width, height) =
        crate::demo::size().unwrap_or(if setup { (SETUP_WIDTH, SETUP_HEIGHT) } else { (MAIN_WIDTH, MAIN_HEIGHT) });
    let bounds = windows(cx)
        .main_bounds
        .filter(|_| !setup)
        .unwrap_or_else(|| WindowBounds::Windowed(Bounds::centered(None, size(px(width), px(height)), cx)));
    let options = WindowOptions {
        window_bounds: Some(bounds),
        titlebar: Some(TitlebarOptions { title: Some("BlueCaster".into()), ..Default::default() }),
        window_min_size: Some(min_size(setup)),
        window_background: WindowBackgroundAppearance::Opaque,
        focus,
        ..Default::default()
    };
    let mut main_hwnd = 0;
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        let dark = cx.palette().dark;
        main_hwnd = hwnd(window);
        win_sys::window::set_dark_title_bar(main_hwnd, dark);
        // The title bar and taskbar icons at their own sizes from the .ico
        // (issue #86: one image shrunk is a blue square at 100 % scaling)
        if !win_sys::app_icon::set_window_icons(main_hwnd, ICON_RESOURCE) {
            tracing::warn!("[main] window icons not set");
        }
        window.on_window_should_close(cx, |window, cx| {
            main_closing(window, cx);
            true
        });
        cx.new(|cx| MainWindow::new(window, cx))
    });
    match opened {
        Ok((handle, _)) => {
            windows(cx).main = Some(handle);
            windows(cx).main_hwnd = main_hwnd;
            tracing::info!("[main] shown");
        }
        Err(e) => tracing::error!("[main] cannot open the window: {e}"),
    }
}

fn min_size(setup: bool) -> gpui_kit::Size<Pixels> {
    size(px(if setup { SETUP_WIDTH } else { MAIN_MIN_WIDTH }), px(MAIN_MIN_HEIGHT))
}

/// The first-run setup has a window size of its own; leaving it restores
/// the one before
pub fn setup_window(active: bool, window: &mut Window, cx: &mut App) {
    let before = windows(cx).before_setup;
    if active {
        if before.is_some() {
            return;
        }
        windows(cx).before_setup = Some(window.window_bounds());
        window.resize(size(px(SETUP_WIDTH), px(SETUP_HEIGHT)));
    } else if let Some(before) = before {
        windows(cx).before_setup = None;
        window.resize(before.get_bounds().size);
    }
}

/// The main window closes. With "Minimize to System Tray" the core shows
/// its one-time "still running in the tray" balloon; without it the core is
/// asked to exit, and its Bye ends this process.
fn main_closing(window: &mut Window, cx: &mut App) {
    let w = windows(cx);
    w.main = None;
    w.main_bounds = Some(w.before_setup.take().unwrap_or_else(|| window.window_bounds()));
    tracing::info!("[main] closed");
    // Not connected: the setting is not known, the window is only gone
    // (and the process ends when idle)
    let Some(to_tray) = app::core(cx).read(cx).snap().map(|s| s.settings.minimize_to_tray) else { return };
    if to_tray {
        app::request::<serde_json::Value>(Method::AppHiddenToTray, cx).detach();
        return;
    }
    let quit = app::request::<serde_json::Value>(Method::AppQuit, cx);
    cx.spawn(async move |cx: &mut AsyncApp| {
        // A core that runs as a service stays: app.close ends the user's
        // session of the app instead (UI and tray)
        if let Err(e) = quit.await
            && e.code == ipc::access::NOT_IN_SERVICE
        {
            cx.update(|cx| app::request::<serde_json::Value>(Method::AppClose, cx).detach());
        }
    })
    .detach();
}

/// Hides the main window to the tray ("Minimize to System Tray" on
/// minimizing) and lets the core show its one-time balloon
pub fn hide_to_tray(window: &mut Window, cx: &mut App) {
    let w = windows(cx);
    w.main = None;
    w.main_bounds = Some(w.before_setup.take().unwrap_or_else(|| window.window_bounds()));
    window.remove_window();
    app::request::<serde_json::Value>(Method::AppHiddenToTray, cx).detach();
}

/// Ends the process once no window has been open for HIDDEN_EXIT_AFTER
pub fn start_idle_exit(cx: &mut App) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        let mut hidden_since: Option<Instant> = None;
        loop {
            cx.background_executor().timer(Duration::from_secs(5)).await;
            let busy = cx.update(|cx| {
                // A downloaded update waits in this process for the stream to end
                !cx.windows().is_empty() || crate::host::backend(cx).is_some_and(|h| h.update_busy())
            });
            if busy {
                hidden_since = None;
                continue;
            }
            let since = *hidden_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= HIDDEN_EXIT_AFTER {
                tracing::info!("no window for a while; exiting");
                cx.update(|cx| cx.quit());
                return;
            }
        }
    })
    .detach();
}
