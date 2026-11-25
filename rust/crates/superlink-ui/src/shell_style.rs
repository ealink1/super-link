//! Shell accent palette taken from the iShell Pro visual reference.
//!
//! Only the accent family stays Shell-specific (purple selection). All neutral
//! tokens (canvas, surface, sidebar, button, border, text, muted) come from the
//! shared theme so the workspace background matches SQL and Note.
use crate::theme::Theme;
use gpui::{rgb, rgba};

pub(crate) fn palette(mut theme: Theme) -> Theme {
    if theme.canvas.l > 0.5 {
        theme.accent = rgb(0x6254dc).into();
        theme.accent_soft = rgba(0x6254dc18).into();
        theme.success = rgb(0x4da331).into();
    }
    theme
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_shell_keeps_the_shared_neutrals() {
        let base = Theme::light();
        let shell = palette(base);
        assert_eq!(shell.canvas, base.canvas);
        assert_eq!(shell.surface, base.surface);
        assert_eq!(shell.sidebar, base.sidebar);
        assert_eq!(shell.button, base.button);
        assert_eq!(shell.border, base.border);
        assert_eq!(shell.text, base.text);
        assert_eq!(shell.muted, base.muted);
        assert_eq!(shell.accent, rgb(0x6254dc).into());
    }

    #[test]
    fn dark_shell_is_untouched() {
        let base = Theme::dark();
        assert_eq!(palette(base), base);
    }
}
