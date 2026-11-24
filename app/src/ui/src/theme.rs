//! Dark / light / system, and the colours of the screens.
//!
//! The look is the component library's default theme: its neutral greys, its
//! primary colour on what is strong or selected, its shapes (controls 6 px,
//! cards 8 px) and the system's UI font. Only its text size is set here: a
//! 15 px body.
//!
//! - [`Palette`] is what the screens draw themselves with (`cx.palette()`):
//!   the same colours, so they match the library's controls.
//! - The bitpool chart keeps two hues of its own (`chart_1`, `chart_2`).
//! - Status colours: good #34D399, warning #FBBF24, error #F87171 (in light
//!   mode text uses darker shades of the same hues for WCAG AA contrast).
//! - Type: 13 / 15 / 17 / 21 / 30 px, tabular numerals for numbers that change.
//! - 4 px spacing grid.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Global, Hsla, Pixels, Rgba, WindowAppearance, px, rgb, rgba};

/// Text sizes: hints, the body (the component library's controls take it
/// too), card titles, section titles, the page's title
pub const FS_SMALL: Pixels = px(13.);
pub const FS_BODY: Pixels = px(15.);
pub const FS_LARGE: Pixels = px(17.);
pub const FS_TITLE: Pixels = px(21.);
pub const FS_PAGE: Pixels = px(30.);

/// Spacing (the 4 px grid: --s1 .. --s8)
pub const S1: Pixels = px(4.);
pub const S2: Pixels = px(8.);
pub const S3: Pixels = px(12.);
pub const S4: Pixels = px(16.);
pub const S5: Pixels = px(20.);
pub const S6: Pixels = px(24.);
pub const S8: Pixels = px(32.);

/// Corner radius of controls and of cards (the component library's)
pub const RADIUS: Pixels = px(6.);
pub const RADIUS_CARD: Pixels = px(8.);

/// The system's UI font, as the component library uses (Segoe UI; Yu Gothic
/// UI on a Japanese Windows)
pub const FONT: &str = ".SystemUIFont";
pub const FONT_DISPLAY: &str = ".SystemUIFont";
pub const FONT_MONO: &str = "Cascadia Mono";

/// The colours of the screens: the component library's default greys
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub bg: Hsla,
    pub bg_nav: Hsla,
    pub surface: Hsla,
    pub surface_2: Hsla,
    pub surface_3: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub text_2: Hsla,
    pub text_3: Hsla,
    pub strong_bg: Hsla,
    pub strong_fg: Hsla,
    /// The library's primary colour for fills, and the text on it
    pub accent_fill: Hsla,
    pub on_accent: Hsla,
    pub disabled_text: Hsla,
    pub good: Hsla,
    pub warn: Hsla,
    pub bad: Hsla,
    /// The status colours as text (darker on light)
    pub good_text: Hsla,
    pub warn_text: Hsla,
    pub bad_text: Hsla,
    /// The status colours as borders
    pub good_line: Hsla,
    pub warn_line: Hsla,
    pub bad_line: Hsla,
    pub focus: Hsla,
    pub overlay: Hsla,
    pub chart_1: Hsla,
    pub chart_2: Hsla,
}

impl Global for Palette {}

fn hex(v: u32) -> Hsla {
    rgb(v).into()
}

fn alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a, ..c }
}

impl Palette {
    pub fn new(dark: bool) -> Palette {
        let good = hex(0x34d399);
        let warn = hex(0xfbbf24);
        let bad = hex(0xf87171);
        if dark {
            Palette {
                dark,
                bg: hex(0x0a0a0a),
                bg_nav: hex(0x0a0a0a),
                surface: hex(0x171717),
                surface_2: hex(0x262626),
                surface_3: hex(0x333333),
                border: hex(0x262626),
                border_strong: hex(0x404040),
                text: hex(0xfafafa),
                text_2: hex(0xa3a3a3),
                text_3: hex(0x8a8a8a),
                strong_bg: hex(0xfafafa),
                strong_fg: hex(0x171717),
                accent_fill: hex(0xfafafa),
                on_accent: hex(0x171717),
                disabled_text: rgba(0xffffff5c).into(),
                good,
                warn,
                bad,
                good_text: good,
                warn_text: warn,
                bad_text: bad,
                // A soft tint on dark, the text shade on light (#86)
                good_line: alpha(good, 0.55),
                warn_line: alpha(warn, 0.55),
                bad_line: alpha(bad, 0.55),
                focus: hex(0xa3a3a3),
                overlay: rgba(0x0000008c).into(),
                chart_1: hex(0x4f8cff),
                chart_2: hex(0x8b5cf6),
            }
        } else {
            let (good_text, warn_text, bad_text) = (hex(0x047857), hex(0x92400e), hex(0xb91c1c));
            Palette {
                dark,
                bg: hex(0xffffff),
                bg_nav: hex(0xfafafa),
                surface: hex(0xffffff),
                surface_2: hex(0xf5f5f5),
                surface_3: hex(0xe5e5e5),
                border: hex(0xe5e5e5),
                border_strong: hex(0xd4d4d4),
                text: hex(0x0a0a0a),
                text_2: hex(0x525252),
                text_3: hex(0x737373),
                strong_bg: hex(0x171717),
                strong_fg: hex(0xfafafa),
                accent_fill: hex(0x171717),
                on_accent: hex(0xfafafa),
                disabled_text: rgba(0x0000005c).into(),
                good,
                warn,
                bad,
                good_text,
                warn_text,
                bad_text,
                good_line: good_text,
                warn_line: warn_text,
                bad_line: bad_text,
                focus: hex(0x737373),
                overlay: Rgba { r: 20. / 255., g: 22. / 255., b: 28. / 255., a: 0.35 }.into(),
                chart_1: hex(0x4f8cff),
                chart_2: hex(0x8b5cf6),
            }
        }
    }
}

/// `cx.palette()`
pub trait ActivePalette {
    fn palette(&self) -> &Palette;
}

impl ActivePalette for App {
    fn palette(&self) -> &Palette {
        self.global::<Palette>()
    }
}

/// The theme setting as saved: anything but "light" and "system" is dark,
/// as v1 read it (v1 files may say e.g. "white")
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSetting {
    Dark,
    Light,
    System,
}

impl ThemeSetting {
    pub fn parse(s: &str) -> ThemeSetting {
        match s {
            "light" => ThemeSetting::Light,
            "system" => ThemeSetting::System,
            _ => ThemeSetting::Dark,
        }
    }
}

/// The setting in force, kept so that a change of Windows' own theme
/// applies again
struct Current(ThemeSetting);
impl Global for Current {}

/// Applies the theme setting
pub fn apply(setting: ThemeSetting, cx: &mut App) {
    cx.set_global(Current(setting));
    refresh(cx);
}

/// Reads Windows' theme again: when the window gets the focus,
/// so a change in Settings shows, and when Windows says its theme changed
pub fn refresh(cx: &mut App) {
    let setting = cx.try_global::<Current>().map_or(ThemeSetting::System, |c| c.0);
    let dark = match setting {
        ThemeSetting::Dark => true,
        ThemeSetting::Light => false,
        ThemeSetting::System => {
            matches!(cx.window_appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark)
        }
    };
    let palette = Palette::new(dark);
    if cx.try_global::<Palette>() == Some(&palette) {
        return;
    }
    cx.set_global(palette);
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    // The library's controls in the screens' body size
    Theme::update(cx, |t| t.font_size = FS_BODY);
    crate::windows::apply_theme(cx);
}
