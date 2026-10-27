use gpui::{Hsla, rgb};

/// How the palette is chosen. Persisted by key so a restart keeps the choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn resolve(self, appearance_is_dark: bool) -> Theme {
        match self {
            ThemeMode::Light => Theme::light(),
            ThemeMode::Dark => Theme::dark(),
            ThemeMode::System if appearance_is_dark => Theme::dark(),
            ThemeMode::System => Theme::light(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::System => "自动",
            ThemeMode::Light => "浅色",
            ThemeMode::Dark => "深色",
        }
    }

    pub fn next(self) -> Self {
        match self {
            ThemeMode::System => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::System,
        }
    }

    pub fn as_key(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    pub fn from_key(value: &str) -> Self {
        match value {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_through_all_modes() {
        assert_eq!(ThemeMode::System.next(), ThemeMode::Light);
        assert_eq!(ThemeMode::Light.next(), ThemeMode::Dark);
        assert_eq!(ThemeMode::Dark.next(), ThemeMode::System);
    }

    #[test]
    fn keys_round_trip() {
        for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
            assert_eq!(ThemeMode::from_key(mode.as_key()), mode);
        }
        assert_eq!(ThemeMode::from_key("unknown"), ThemeMode::System);
    }

    #[test]
    fn system_follows_the_window() {
        assert_eq!(ThemeMode::System.resolve(true), Theme::dark());
        assert_eq!(ThemeMode::System.resolve(false), Theme::light());
        assert_eq!(ThemeMode::Light.resolve(true), Theme::light());
    }
}

/// Design tokens. Both variants keep the same contrast structure so widgets
/// never hardcode a color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub canvas: Hsla,
    pub surface: Hsla,
    pub sidebar: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub accent_soft: Hsla,
    pub success: Hsla,
    pub warning_bg: Hsla,
    pub warning_border: Hsla,
    pub warning_text: Hsla,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            canvas: rgb(0xf7f8fa).into(),
            surface: rgb(0xffffff).into(),
            sidebar: rgb(0xf2f4f7).into(),
            border: rgb(0xe2e6ec).into(),
            text: rgb(0x1c2128).into(),
            muted: rgb(0x6b7280).into(),
            accent: rgb(0x16803d).into(),
            accent_soft: rgb(0xe8f5ec).into(),
            success: rgb(0x16803d).into(),
            warning_bg: rgb(0xfff4e0).into(),
            warning_border: rgb(0xf0c36b).into(),
            warning_text: rgb(0x8a5a00).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            canvas: rgb(0x16181d).into(),
            surface: rgb(0x1d2026).into(),
            sidebar: rgb(0x191c21).into(),
            border: rgb(0x2b3038).into(),
            text: rgb(0xe6e8ec).into(),
            muted: rgb(0x9aa1ad).into(),
            accent: rgb(0x4ade80).into(),
            accent_soft: rgb(0x1f3327).into(),
            success: rgb(0x4ade80).into(),
            warning_bg: rgb(0x3a2e1a).into(),
            warning_border: rgb(0x5c4a2a).into(),
            warning_text: rgb(0xe8c07a).into(),
        }
    }
}
