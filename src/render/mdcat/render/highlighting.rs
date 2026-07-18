// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Tools for syntax highlighting.

use anstyle::{AnsiColor, Color, Effects, RgbColor};
use std::{
    io::{Result, Write},
    sync::OnceLock,
};
use syntect::highlighting::{FontStyle, Highlighter, Style, Theme};

static THEME: OnceLock<Theme> = OnceLock::new();
static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();

fn theme() -> &'static Theme {
    THEME.get_or_init(|| {
        syntect::highlighting::ThemeSet::load_defaults()
            .themes
            .get("Solarized (dark)")
            .cloned()
            .unwrap_or_else(syntect::highlighting::Theme::default)
    })
}

pub fn highlighter() -> &'static Highlighter<'static> {
    HIGHLIGHTER.get_or_init(|| Highlighter::new(theme()))
}

/// Write regions as ANSI 8-bit coloured text.
fn solarized_to_ansi(style: Style) -> anstyle::Style {
    let rgb = {
        let fg = style.foreground;
        (fg.r, fg.g, fg.b)
    };
    let color: Option<Color> = match rgb {
        (0x00, 0x2b, 0x36)
        | (0x07, 0x36, 0x42)
        | (0x58, 0x6e, 0x75)
        | (0x65, 0x7b, 0x83)
        | (0x83, 0x94, 0x96)
        | (0x93, 0xa1, 0xa1)
        | (0xee, 0xe8, 0xd5)
        | (0xfd, 0xf6, 0xe3) => None,
        (0xb5, 0x89, 0x00) => Some(AnsiColor::Yellow.into()),
        (0xcb, 0x4b, 0x16) => Some(AnsiColor::BrightRed.into()),
        (0xdc, 0x32, 0x2f) => Some(AnsiColor::Red.into()),
        (0xd3, 0x36, 0x82) => Some(AnsiColor::Magenta.into()),
        (0x6c, 0x71, 0xc4) => Some(AnsiColor::BrightMagenta.into()),
        (0x26, 0x8b, 0xd2) => Some(AnsiColor::Blue.into()),
        (0x2a, 0xa1, 0x98) => Some(AnsiColor::Cyan.into()),
        (0x85, 0x99, 0x00) => Some(AnsiColor::Green.into()),
        (r, g, b) => panic!("Unexpected RGB colour: #{r:2>0x}{g:2>0x}{b:2>0x}"),
    };
    let font = style.font_style;
    let effects = Effects::new()
        .set(Effects::BOLD, font.contains(FontStyle::BOLD))
        .set(Effects::ITALIC, font.contains(FontStyle::ITALIC))
        .set(Effects::UNDERLINE, font.contains(FontStyle::UNDERLINE));
    anstyle::Style::new().fg_color(color).effects(effects)
}

pub fn write_as_ansi<'a, W: Write, I: Iterator<Item = (Style, &'a str)>>(
    writer: &mut W,
    regions: I,
) -> Result<()> {
    for (style, text) in regions {
        let style = solarized_to_ansi(style);
        write!(writer, "{}{}{}", style.render(), text, style.render_reset())?;
    }
    Ok(())
}

/// Write highlighted regions using 24-bit RGB colors from the syntect theme.
pub fn write_as_rgb<'a, W: Write, I: Iterator<Item = (Style, &'a str)>>(
    writer: &mut W,
    regions: I,
) -> Result<()> {
    for (style, text) in regions {
        let fg = style.foreground;
        let font = style.font_style;
        let effects = Effects::new()
            .set(Effects::BOLD, font.contains(FontStyle::BOLD))
            .set(Effects::ITALIC, font.contains(FontStyle::ITALIC))
            .set(Effects::UNDERLINE, font.contains(FontStyle::UNDERLINE));
        let ansi = anstyle::Style::new()
            .fg_color(Some(RgbColor(fg.r, fg.g, fg.b).into()))
            .effects(effects);
        write!(writer, "{}{}{}", ansi.render(), text, ansi.render_reset())?;
    }
    Ok(())
}

/// Create a highlighter for a given syntect theme.
pub fn highlighter_for(theme: &Theme) -> Highlighter<'_> {
    Highlighter::new(theme)
}
