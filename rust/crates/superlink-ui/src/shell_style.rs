//! Shell-only palette, measured from the user's iShell Pro visual reference.
use crate::theme::Theme;
use gpui::{rgb, rgba};

pub(crate) fn palette(mut theme: Theme) -> Theme {
    if theme.canvas.l > 0.5 {
        theme.canvas = rgb(0xeff1f5).into();
        theme.surface = rgb(0xeaecf0).into();
        theme.sidebar = rgb(0xe6e8eb).into();
        theme.button = rgb(0xdee0e4).into();
        theme.border = rgb(0xd3d5d8).into();
        theme.text = rgb(0x4d4f69).into();
        theme.muted = rgb(0x7c7f83).into();
        theme.accent = rgb(0x6254dc).into();
        theme.accent_soft = rgba(0x6254dc18).into();
        theme.success = rgb(0x4da331).into();
    }
    theme
}
