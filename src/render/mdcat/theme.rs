// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Provide a colour theme for mdcat.

use anstyle::{Color, Effects, RgbColor, Style};

/// A colour theme for mdcat.
///
/// All fields are public so themes can be fully customised.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Theme {
    /// Style for HTML blocks.
    pub html_block_style: Style,
    /// Style for inline HTML.
    pub inline_html_style: Style,
    /// Style for code, unless the code is syntax-highlighted.
    pub code_style: Style,
    /// Style for links.
    pub link_style: Style,
    /// Color for image links (unless the image is rendered inline)
    pub image_link_style: Style,
    /// Color for rulers.
    pub rule_color: Color,
    /// Style for block quote borders (`│`).
    pub quote_border_style: Style,
    /// Style for H2 headings.
    pub h2_style: Style,
    /// Style for H3 headings.
    pub h3_style: Style,
    /// Style for H4 headings.
    pub h4_style: Style,
    /// Style for H5 headings.
    pub h5_style: Style,
    /// Style for H6 headings.
    pub h6_style: Style,
    /// Style for footnote references and definitions.
    pub footnote_style: Style,
    /// Style for math expressions.
    pub math_style: Style,
    /// Style for `[!NOTE]` alerts.
    pub alert_note_style: Style,
    /// Style for `[!TIP]` alerts.
    pub alert_tip_style: Style,
    /// Style for `[!IMPORTANT]` alerts.
    pub alert_important_style: Style,
    /// Style for `[!WARNING]` alerts.
    pub alert_warning_style: Style,
    /// Style for `[!CAUTION]` alerts.
    pub alert_caution_style: Style,
    /// Background-colored padding space written before H1 text.
    pub h1_prefix_style: Style,
    /// Style for H1 heading text (fg + bg color).
    pub h1_text_style: Style,
}

impl Theme {
    /// Set the H1 style, keeping `h1_prefix_style`'s background in sync with `text_style`'s.
    pub fn with_h1(mut self, text_style: Style) -> Self {
        let bg = text_style.get_bg_color();
        self.h1_prefix_style = Style::new().bg_color(bg).fg_color(bg);
        self.h1_text_style = text_style;
        self
    }
}

/// Combine styles.
pub trait CombineStyle {
    /// Put this style on top of the other style.
    ///
    /// Return a new style which falls back to the `other` style for all style attributes not
    /// specified in this style.
    fn on_top_of(self, other: &Self) -> Self;
}

impl CombineStyle for Style {
    /// Put this style on top of the `other` style.
    fn on_top_of(self, other: &Style) -> Style {
        Style::new()
            .fg_color(self.get_fg_color().or(other.get_fg_color()))
            .bg_color(self.get_bg_color().or(other.get_bg_color()))
            .effects(other.get_effects() | self.get_effects())
            .underline_color(self.get_underline_color().or(other.get_underline_color()))
    }
}

fn vesp(color: &str) -> Color {
    let (r, g, b) = crate::render::block::hex_to_rgb(color);
    RgbColor(r, g, b).into()
}

/// Vesper theme for mate.
pub fn vesper() -> Theme {
    let v = crate::render::theme::VESPER;
    let fg = vesp(v.fg);
    let string = vesp(v.string);
    let accent = vesp(v.accent);
    let muted = vesp(v.muted);
    let border = vesp(v.border);
    let bg = vesp(v.bg);
    let typ = vesp(v.typ);
    let warning = vesp(v.warning);
    let error = vesp(v.error);

    let (h1_prefix_style, h1_text_style) = (
        Style::new().bg_color(Some(bg)).fg_color(Some(bg)),
        Style::new().bg_color(Some(bg)).fg_color(Some(typ)).bold(),
    );

    Theme {
        html_block_style: Style::new().fg_color(Some(fg)),
        inline_html_style: Style::new().fg_color(Some(fg)),
        code_style: Style::new().fg_color(Some(string)),
        link_style: Style::new()
            .fg_color(Some(accent))
            .effects(Effects::UNDERLINE),
        image_link_style: Style::new()
            .fg_color(Some(accent))
            .effects(Effects::UNDERLINE),
        rule_color: border,
        quote_border_style: Style::new().fg_color(Some(muted)),
        h2_style: Style::new().fg_color(Some(typ)).bold(),
        h3_style: Style::new().fg_color(Some(typ)).bold(),
        h4_style: Style::new().fg_color(Some(typ)).bold(),
        h5_style: Style::new().fg_color(Some(typ)).bold(),
        h6_style: Style::new().fg_color(Some(typ)).bold(),
        footnote_style: Style::new().fg_color(Some(muted)),
        math_style: Style::new().fg_color(Some(string)),
        alert_note_style: Style::new().fg_color(Some(accent)).bold(),
        alert_tip_style: Style::new().fg_color(Some(string)).bold(),
        alert_important_style: Style::new().fg_color(Some(accent)).bold(),
        alert_warning_style: Style::new().fg_color(Some(warning)).bold(),
        alert_caution_style: Style::new().fg_color(Some(error)).bold(),
        h1_prefix_style,
        h1_text_style,
    }
}

impl Default for Theme {
    fn default() -> Self {
        vesper()
    }
}
