//! Small visual vocabulary shared by every screen.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{App, Hsla, Rgba, rgb};

/// Project colours: calm, readable on light and dark, and no red so nothing
/// ever looks like a warning.
pub const PROJECT_COLORS: &[&str] = &[
    "#5B8DEF", // blue
    "#4AA3A2", // teal
    "#6BA368", // green
    "#C9A227", // mustard
    "#E08A4F", // orange
    "#D46A8C", // rose
    "#9B7BD4", // purple
    "#7C8A99", // slate
];

/// Colours rotate so new projects look different without asking.
pub fn next_project_color(existing: usize) -> &'static str {
    PROJECT_COLORS[existing % PROJECT_COLORS.len()]
}

pub fn hex(color: &str) -> Hsla {
    let value = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0x7C8A99);
    let rgba: Rgba = rgb(value);
    rgba.into()
}

/// The soft background behind the selected row.
pub fn selection(cx: &App) -> Hsla {
    if cx.theme().is_dark() {
        cx.theme().accent.opacity(0.9)
    } else {
        cx.theme().foreground.opacity(0.07)
    }
}

pub fn subtle(cx: &App) -> Hsla {
    cx.theme().muted_foreground
}
