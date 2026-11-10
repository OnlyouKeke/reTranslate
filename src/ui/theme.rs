use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReadingMode {
    #[default]
    InlineMuted,
    InlineDotted,
    SideBySide,
    TargetOnly,
    OriginalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReadingWidth {
    Narrow,   // 680px
    #[default]
    Standard, // 850px
    Wide,     // 1100px
    Full,     // 100%
}

impl ReadingWidth {
    pub fn max_width_px(&self) -> Option<f32> {
        match self {
            ReadingWidth::Narrow => Some(680.0),
            ReadingWidth::Standard => Some(850.0),
            ReadingWidth::Wide => Some(1100.0),
            ReadingWidth::Full => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutSettings {
    pub reading_width: ReadingWidth,
    pub page_padding: f32,
    pub line_height: f32,
    pub paragraph_spacing: f32,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            reading_width: ReadingWidth::Standard,
            page_padding: 24.0,
            line_height: 1.6,
            paragraph_spacing: 16.0,
        }
    }
}

pub mod colors {
    // Catppuccin Mocha inspired dark theme colors
    pub const DARK_BG_BASE: u32 = 0x1e1e2e;
    pub const DARK_BG_SURFACE: u32 = 0x313244;
    pub const DARK_TEXT_MAIN: u32 = 0xcdd6f4;
    pub const DARK_TEXT_MUTED: u32 = 0xa6adc8;
    pub const DARK_ACCENT_BLUE: u32 = 0x89b4fa;
    pub const DARK_ERROR_RED: u32 = 0xf38ba8;
    pub const DARK_BORDER: u32 = 0x45475a;

    // Light theme colors
    pub const LIGHT_BG_BASE: u32 = 0xfafafa;
    pub const LIGHT_BG_SURFACE: u32 = 0xffffff;
    pub const LIGHT_TEXT_MAIN: u32 = 0x18181b;
    pub const LIGHT_TEXT_MUTED: u32 = 0x71717a;
    pub const LIGHT_ACCENT_BLUE: u32 = 0x2563eb;
    pub const LIGHT_ERROR_RED: u32 = 0xdc2626;
    pub const LIGHT_BORDER: u32 = 0xe4e4e7;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reading_width_pixels() {
        assert_eq!(ReadingWidth::Narrow.max_width_px(), Some(680.0));
        assert_eq!(ReadingWidth::Standard.max_width_px(), Some(850.0));
        assert_eq!(ReadingWidth::Wide.max_width_px(), Some(1100.0));
        assert_eq!(ReadingWidth::Full.max_width_px(), None);
    }
}
