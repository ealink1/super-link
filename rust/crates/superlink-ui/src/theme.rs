use gpui::{Hsla, rgb};

/// Design tokens. Both variants keep the same contrast structure so widgets
/// never hardcode a color.
#[derive(Debug, Clone, Copy)]
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
        }
    }
}
