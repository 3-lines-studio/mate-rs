use crate::render::theme::VESPER;
use once_cell::sync::Lazy;
use ratatui::style::Color;

fn rgb(hex: &str) -> Color {
    let h = hex.trim_start_matches('#');
    Color::Rgb(
        u8::from_str_radix(&h[0..2], 16).unwrap_or(0),
        u8::from_str_radix(&h[2..4], 16).unwrap_or(0),
        u8::from_str_radix(&h[4..6], 16).unwrap_or(0),
    )
}

pub struct Colors {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub muted: Color,
    pub placeholder: Color,
    pub error: Color,
    pub border: Color,
    pub selected: Color,
    pub green: Color,
}

pub static COLORS: Lazy<Colors> = Lazy::new(|| Colors {
    bg: rgb(VESPER.bg),
    fg: rgb(VESPER.fg),
    accent: rgb(VESPER.accent),
    muted: rgb(VESPER.muted),
    placeholder: rgb(VESPER.placeholder),
    error: rgb(VESPER.error),
    border: rgb(VESPER.border),
    selected: rgb(VESPER.selected),
    green: rgb(VESPER.string),
});

pub const THEME: &crate::render::theme::Theme = &VESPER;
