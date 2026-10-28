use gpui::{Hsla, rgb, rgba};

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

    #[test]
    fn light_matches_the_fyne_overrides() {
        let theme = Theme::light();
        assert_eq!(theme.success_soft, rgb(0xe8f5ec).into());
        assert_eq!(theme.canvas, rgb(0xfafaf8).into());
        assert_eq!(theme.surface, rgb(0xffffff).into());
        assert_eq!(theme.button, rgb(0xf7f8f6).into());
        assert_eq!(theme.accent, rgb(0x337aff).into());
        assert_eq!(theme.accent_soft, rgb(0xe9f0ff).into());
        assert_eq!(theme.text, rgb(0x1f2937).into());
        assert_eq!(theme.border, rgba(0x0f172a1c).into());
    }

    #[test]
    fn dark_matches_the_fyne_overrides() {
        let theme = Theme::dark();
        assert_eq!(theme.canvas, rgb(0x1b1f27).into());
        assert_eq!(theme.surface, rgb(0x0f1217).into());
        assert_eq!(theme.button, rgb(0x1a1a21).into());
        assert_eq!(theme.accent, rgb(0x699bff).into());
        assert_eq!(theme.accent_soft, rgb(0x263752).into());
        assert_eq!(theme.text, rgb(0xf5f5f5).into());
        assert_eq!(theme.border, rgba(0xffffff19).into());
    }

    #[test]
    fn sizes_match_the_fyne_theme() {
        assert_eq!(TEXT_SIZE, 13.0);
        assert_eq!(PADDING, 4.0);
        assert_eq!(INNER_PADDING, 6.0);
        assert_eq!(ICON_SIZE, 16.0);
        assert_eq!(INPUT_RADIUS, 6.0);
        assert_eq!(CONTROL_RADIUS, 8.0);
        assert_eq!(CONTROL_MIN, 32.0);
        assert_eq!(HAIRLINE, 0.5);
    }
}

/// Design tokens ported from the Fyne implementation (internal/ui/theme.go
/// and third_party/fyne/theme/color.go), so both front ends share one visual
/// language.
///
/// Both variants keep the same contrast structure so widgets never hardcode a
/// color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Fyne ColorNameBackground（应用覆盖值）。
    pub canvas: Hsla,
    /// Fyne ColorNameInputBackground。
    pub surface: Hsla,
    /// Fyne ColorNameButton（framed control 的底色）。
    pub button: Hsla,
    /// 外壳侧栏与画布同色，仅以边框分隔。
    pub sidebar: Hsla,
    /// Fyne ColorNameInputBorder：浅色 #0F172A@0x1C，深色 #FFFFFF@0x19。
    pub border: Hsla,
    /// Fyne ColorNameForeground（应用覆盖值）。
    pub text: Hsla,
    /// Fyne ColorNameDisabled / 导航图标填充 #6B7280。
    pub muted: Hsla,
    /// Fyne ColorNamePrimary（应用覆盖值）。
    pub accent: Hsla,
    /// Fyne ColorNameSelection（应用覆盖值）。
    pub accent_soft: Hsla,
    /// Fyne ColorNameHover。
    pub hover: Hsla,
    /// Fyne ColorNameError。
    pub danger: Hsla,
    /// Fyne ColorNameSuccess。
    pub success: Hsla,
    /// 浅绿选中底（QA 记录的「浅绿选中行」）。
    pub success_soft: Hsla,
    pub placeholder: Hsla,
    pub warning_bg: Hsla,
    pub warning_border: Hsla,
    pub warning_text: Hsla,
}

/// Fyne SizeNameText。
pub const TEXT_SIZE: f32 = 13.0;
/// Fyne h4 缩放（1.2 × 正文），用于面板标题。
pub const HEADING_SIZE: f32 = 15.6;
/// Fyne SizeNamePadding。
pub const PADDING: f32 = 4.0;
/// Fyne SizeNameInnerPadding。
pub const INNER_PADDING: f32 = 6.0;
/// Fyne SizeNameInlineIcon。
pub const ICON_SIZE: f32 = 16.0;
/// Fyne SizeNameInputRadius / SelectionRadius。
pub const INPUT_RADIUS: f32 = 6.0;
/// controlFrame 的圆角。
pub const CONTROL_RADIUS: f32 = 8.0;
/// controlFrame 的最小控件边长。
pub const CONTROL_MIN: f32 = 32.0;
/// Fyne SizeNameSeparatorThickness / InputBorder。
pub const HAIRLINE: f32 = 0.5;

impl Theme {
    pub fn light() -> Self {
        Self {
            canvas: rgb(0xfafaf8).into(),
            surface: rgb(0xffffff).into(),
            button: rgb(0xf7f8f6).into(),
            sidebar: rgb(0xfafaf8).into(),
            border: rgba(0x0f172a1c).into(),
            text: rgb(0x1f2937).into(),
            muted: rgb(0x6b7280).into(),
            accent: rgb(0x337aff).into(),
            accent_soft: rgb(0xe9f0ff).into(),
            hover: rgba(0x0000000f).into(),
            danger: rgb(0xf44336).into(),
            success: rgb(0x43f436).into(),
            success_soft: rgb(0xe8f5ec).into(),
            placeholder: rgb(0x888888).into(),
            warning_bg: rgb(0xfff1d6).into(),
            warning_border: rgb(0xe0a94a).into(),
            warning_text: rgb(0x7a4a00).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            canvas: rgb(0x1b1f27).into(),
            surface: rgb(0x0f1217).into(),
            button: rgb(0x1a1a21).into(),
            sidebar: rgb(0x1b1f27).into(),
            border: rgba(0xffffff19).into(),
            text: rgb(0xf5f5f5).into(),
            muted: rgb(0x9ca3af).into(),
            accent: rgb(0x699bff).into(),
            accent_soft: rgb(0x263752).into(),
            hover: rgba(0xffffff0f).into(),
            danger: rgb(0xf44336).into(),
            success: rgb(0x43f436).into(),
            success_soft: rgb(0x1f3327).into(),
            placeholder: rgb(0xb2b2b2).into(),
            warning_bg: rgb(0x3a2e1a).into(),
            warning_border: rgb(0x5c4a2a).into(),
            warning_text: rgb(0xe8c07a).into(),
        }
    }
}
