// Copyright 2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::cmp::{max, min};
use std::io::{Result, Write};

use anstyle::Style;
use pulldown_cmark::{Alignment, CodeBlockKind, CowStr, HeadingLevel};
use syntect::highlighting::HighlightState;
use syntect::parsing::{ParseState, ScopeStack};
use textwrap::WordSeparator;
use textwrap::core::{Word, display_width};
use unicode_width::UnicodeWidthChar;

use super::super::Theme;
use super::super::references::UrlBase;
use super::super::terminal::TerminalSize;
use super::super::terminal::capabilities::{StyleCapability, TerminalCapabilities};
use super::super::terminal::osc::{clear_link, set_link_url};
use super::super::theme::CombineStyle;
use super::super::{Environment, Settings};
use super::data::{CurrentLine, CurrentTable, LinkReferenceDefinition, TableCell};
use super::highlighting::{highlighter, highlighter_for};
use super::state::*;

pub fn write_indent<W: Write>(writer: &mut W, level: u16) -> Result<()> {
    write!(writer, "{}", " ".repeat(level as usize))
}

pub fn write_styled<W: Write, S: AsRef<str>>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    style: &Style,
    text: S,
) -> Result<()> {
    match capabilities.style {
        None => write!(writer, "{}", text.as_ref()),
        Some(StyleCapability::Ansi) => write!(
            writer,
            "{}{}{}",
            style.render(),
            text.as_ref(),
            style.render_reset()
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn write_remaining_lines<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    style: &Style,
    indent: u16,
    mut buffer: String,
    next_lines: &[&[Word]],
    last_line: &[Word],
    line_prefix: &str,
) -> Result<CurrentLine> {
    // Finish the previous line
    writeln!(writer)?;
    write_indent(writer, indent)?;
    write!(writer, "{}", line_prefix)?;
    // Now write all lines up to the last
    for line in next_lines {
        match line.split_last() {
            None => {}
            Some((last, heads)) => {
                for word in heads {
                    buffer.push_str(word.word);
                    buffer.push_str(word.whitespace);
                }
                buffer.push_str(last.word);
                write_styled(writer, capabilities, style, &buffer)?;
                writeln!(writer)?;
                write_indent(writer, indent)?;
                write!(writer, "{}", line_prefix)?;
                buffer.clear();
            }
        };
    }

    match last_line.split_last() {
        None => Ok(CurrentLine::empty()),
        Some((last, heads)) => {
            for word in heads {
                buffer.push_str(word.word);
                buffer.push_str(word.whitespace);
            }
            buffer.push_str(last.word);
            write_styled(writer, capabilities, style, &buffer)?;
            Ok(CurrentLine {
                length: textwrap::core::display_width(&buffer) as u16,
                trailing_space: Some(last.whitespace.to_owned()),
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn write_styled_and_wrapped<W: Write, S: AsRef<str>>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    style: &Style,
    max_width: u16,
    indent: u16,
    current_line: CurrentLine,
    text: S,
    line_prefix: &str,
    prefix_cols: u16,
) -> Result<CurrentLine> {
    let max_width = max_width.saturating_sub(prefix_cols);
    let words = WordSeparator::UnicodeBreakProperties
        .find_words(text.as_ref())
        .collect::<Vec<_>>();
    match words.first() {
        None => Ok(current_line),
        Some(first_word) => {
            let current_width = current_line.length
                + indent
                + current_line
                    .trailing_space
                    .as_ref()
                    .map_or(0, |s| display_width(s.as_ref()) as u16);

            if 0 < current_line.length
                && max_width < current_width + display_width(first_word) as u16
            {
                writeln!(writer)?;
                write_indent(writer, indent)?;
                write!(writer, "{}", line_prefix)?;
                return write_styled_and_wrapped(
                    writer,
                    capabilities,
                    style,
                    max_width + prefix_cols,
                    indent,
                    CurrentLine::empty(),
                    text,
                    line_prefix,
                    prefix_cols,
                );
            }

            let widths = [
                (max_width - current_width.min(max_width)) as f64,
                max_width.saturating_sub(indent) as f64,
            ];
            let lines = textwrap::wrap_algorithms::wrap_first_fit(&words, &widths);
            match lines.split_first() {
                None => Ok(current_line),
                Some((first_line, tails)) => {
                    let mut buffer = String::with_capacity(max_width as usize);

                    let new_current_line = match first_line.split_last() {
                        None => current_line,
                        Some((last, heads)) => {
                            if let Some(s) = current_line.trailing_space {
                                buffer.push_str(&s);
                            }
                            for word in heads {
                                buffer.push_str(word.word);
                                buffer.push_str(word.whitespace);
                            }
                            buffer.push_str(last.word);
                            let length =
                                current_line.length + textwrap::core::display_width(&buffer) as u16;
                            write_styled(writer, capabilities, style, &buffer)?;
                            buffer.clear();
                            CurrentLine {
                                length,
                                trailing_space: Some(last.whitespace.to_owned()),
                            }
                        }
                    };

                    match tails.split_last() {
                        None => Ok(new_current_line),
                        Some((last_line, next_lines)) => write_remaining_lines(
                            writer,
                            capabilities,
                            style,
                            indent,
                            buffer,
                            next_lines,
                            last_line,
                            line_prefix,
                        ),
                    }
                }
            }
        }
    }
}

pub fn write_mark<W: Write>(_writer: &mut W, _capabilities: &TerminalCapabilities) -> Result<()> {
    Ok(())
}

pub fn write_rule<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    length: u16,
) -> std::io::Result<()> {
    let rule = "\u{2550}".repeat(length as usize);
    write_styled(
        writer,
        capabilities,
        &Style::new().fg_color(Some(theme.rule_color)),
        rule,
    )
}

pub fn write_code_block_border<W: Write>(
    writer: &mut W,
    _theme: &Theme,
    _capabilities: &TerminalCapabilities,
    _terminal_size: &TerminalSize,
) -> std::io::Result<()> {
    writeln!(writer)
}

pub fn write_link_refs<W: Write>(
    writer: &mut W,
    environment: &Environment,
    capabilities: &TerminalCapabilities,
    links: Vec<LinkReferenceDefinition>,
) -> Result<()> {
    if !links.is_empty() {
        writeln!(writer)?;
        for link in links {
            write_styled(
                writer,
                capabilities,
                &link.style,
                format!("[{}]: ", link.index),
            )?;

            if let Some(url) = environment.resolve_reference(&link.target) {
                match &capabilities.style {
                    Some(StyleCapability::Ansi) => {
                        set_link_url(writer, url, &environment.hostname)?;
                        write_styled(writer, capabilities, &link.style, link.target)?;
                        clear_link(writer)?;
                    }
                    None => write_styled(writer, capabilities, &link.style, link.target)?,
                };
            } else {
                write_styled(writer, capabilities, &link.style, link.target)?;
            }

            if !link.title.is_empty() {
                write_styled(
                    writer,
                    capabilities,
                    &link.style,
                    format!(" {}", link.title),
                )?;
            }
            writeln!(writer)?;
        }
    };
    Ok(())
}

pub fn write_start_code_block<W: Write>(
    writer: &mut W,
    settings: &Settings,
    indent: u16,
    style: Style,
    block_kind: CodeBlockKind<'_>,
) -> Result<StackedState> {
    writeln!(writer)?;
    write_indent(writer, indent)?;

    match (&settings.terminal_capabilities.style, block_kind) {
        (Some(StyleCapability::Ansi), CodeBlockKind::Fenced(name)) if !name.is_empty() => {
            match settings.syntax_set.find_syntax_by_token(&name) {
                None => Ok(LiteralBlockAttrs {
                    indent,
                    style: settings.theme.code_style.on_top_of(&style),
                }
                .into()),
                Some(syntax) => {
                    let highlight_state = match &settings.syntax_theme {
                        Some(t) => {
                            let hl = highlighter_for(t);
                            HighlightState::new(&hl, ScopeStack::new())
                        }
                        None => HighlightState::new(highlighter(), ScopeStack::new()),
                    };
                    let parse_state = ParseState::new(syntax);
                    Ok(HighlightBlockAttrs {
                        indent,
                        highlight_state,
                        parse_state,
                    }
                    .into())
                }
            }
        }
        (_, _) => Ok(LiteralBlockAttrs {
            indent,
            style: settings.theme.code_style.on_top_of(&style),
        }
        .into()),
    }
}

pub fn write_start_heading<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    theme: &Theme,
    context_style: Style,
    level: HeadingLevel,
) -> Result<StackedState> {
    let level_style = match level {
        HeadingLevel::H1 => {
            writeln!(writer)?;
            write_styled(writer, capabilities, &theme.h1_prefix_style, " ")?;
            theme.h1_text_style
        }
        HeadingLevel::H2 => {
            let s = theme.h2_style.on_top_of(&context_style);
            write_styled(writer, capabilities, &s, "━━ ")?;
            s
        }
        HeadingLevel::H3 => {
            let s = theme.h3_style.on_top_of(&context_style);
            write_styled(writer, capabilities, &s, "── ")?;
            s
        }
        HeadingLevel::H4 => {
            let s = theme.h4_style.on_top_of(&context_style);
            write_styled(writer, capabilities, &s, "┄ ")?;
            s
        }
        HeadingLevel::H5 => {
            let s = theme.h5_style.on_top_of(&context_style);
            write_styled(writer, capabilities, &s, "╌ ")?;
            s
        }
        HeadingLevel::H6 => {
            let s = theme.h6_style.on_top_of(&context_style);
            write_styled(writer, capabilities, &s, "· ")?;
            s
        }
    };

    Ok(StackedState::Inline(
        InlineState::InlineBlock,
        InlineAttrs {
            style: level_style,
            indent: 0,
            quote_depth: 0,
            border_style: None,
        },
    ))
}

/// Minimum column width before we accept overflow rather than shrink further.
const MIN_COL_WIDTH: usize = 8;

/// Compute per-column widths for a table, budgeting against `available` columns.
///
/// Natural widths are the longest cell per column. If they fit (plus the
/// one-space padding on each side of every column) within `available`, they are
/// returned unchanged. Otherwise each column gets a floor of `min(natural,
/// MIN_COL_WIDTH)`; the remaining width is distributed proportionally to how
/// much each column exceeds its floor. If even the floors do not fit, the floors
/// are returned and the table overflows (cells stay readable).
fn calculate_column_widths(table: &CurrentTable, available: u16) -> Option<Vec<usize>> {
    let first_row = table.head.as_ref().or(table.rows.first())?;
    let mut natural = vec![0usize; first_row.cells.len()];
    for row in table.head.iter().chain(table.rows.iter()) {
        for (i, cell) in row.cells.iter().enumerate() {
            if i < natural.len() {
                natural[i] = max(natural[i], cell.text_width());
            }
        }
    }
    let ncols = natural.len();
    if ncols == 0 {
        return Some(natural);
    }
    let content_avail = (available as usize).saturating_sub(2 * ncols);
    let total_natural: usize = natural.iter().sum();
    if total_natural <= content_avail {
        return Some(natural);
    }
    let floor: Vec<usize> = natural.iter().map(|&n| min(n, MIN_COL_WIDTH)).collect();
    let floor_sum: usize = floor.iter().sum();
    if floor_sum >= content_avail {
        return Some(floor);
    }
    let extra = content_avail - floor_sum;
    let weight: Vec<usize> = natural
        .iter()
        .zip(floor.iter())
        .map(|(&n, &f)| n.saturating_sub(f))
        .collect();
    let total_weight: usize = weight.iter().sum();
    let mut final_widths = floor;
    let mut distributed = 0usize;
    let mut heaviest = 0usize;
    for (i, &w) in weight.iter().enumerate() {
        let share = (extra * w).checked_div(total_weight).unwrap_or(0);
        final_widths[i] += share;
        distributed += share;
        if w > weight[heaviest] {
            heaviest = i;
        }
    }
    if total_weight > 0 {
        final_widths[heaviest] += extra.saturating_sub(distributed);
    }
    Some(final_widths)
}

fn write_table_prefix<W: Write>(writer: &mut W, indent: u16, line_prefix: &str) -> Result<()> {
    write_indent(writer, indent)?;
    write!(writer, "{line_prefix}")
}

fn write_table_rule<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    indent: u16,
    line_prefix: &str,
    length: u16,
) -> Result<()> {
    write_table_prefix(writer, indent, line_prefix)?;
    let rule = "\u{2500}".repeat(length.into());
    write_styled(writer, capabilities, &Style::new(), rule)?;
    writeln!(writer)
}

/// Wrap a table cell's styled fragments into lines of at most `width` display
/// columns, preserving per-fragment styling. Whitespace-delimited tokens wrap
/// greedily; a token longer than `width` is hard-broken across lines.
fn wrap_cell(fragments: &[(Style, CowStr)], width: usize) -> Vec<Vec<(Style, String)>> {
    if width == 0 {
        let line: Vec<(Style, String)> =
            fragments.iter().map(|(s, t)| (*s, t.to_string())).collect();
        return if line.is_empty() {
            vec![vec![]]
        } else {
            vec![line]
        };
    }
    let mut lines: Vec<Vec<(Style, String)>> = Vec::new();
    let mut current: Vec<(Style, String)> = Vec::new();
    let mut current_w = 0usize;
    for (style, text) in fragments {
        for piece in text.split(' ') {
            if piece.is_empty() {
                continue;
            }
            let pw = display_width(piece);
            if pw > width {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                    current_w = 0;
                }
                let mut chunk = String::new();
                let mut chunk_w = 0usize;
                for ch in piece.chars() {
                    let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if chunk_w + cw > width && !chunk.is_empty() {
                        lines.push(vec![(*style, std::mem::take(&mut chunk))]);
                        chunk_w = 0;
                    }
                    chunk.push(ch);
                    chunk_w += cw;
                }
                if !chunk.is_empty() {
                    current.push((*style, chunk));
                    current_w = chunk_w;
                }
                continue;
            }
            let sep = if current_w > 0 { 1 } else { 0 };
            if current_w + sep + pw > width {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                current.push((*style, piece.to_string()));
                current_w = pw;
            } else {
                current.push((*style, piece.to_string()));
                current_w += sep + pw;
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        vec![vec![]]
    } else {
        lines
    }
}

fn write_cell_line_pieces<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    line: &[(Style, String)],
) -> Result<()> {
    for (i, (style, text)) in line.iter().enumerate() {
        if i > 0 {
            write!(writer, " ")?;
        }
        write_styled(writer, capabilities, style, text)?;
    }
    Ok(())
}

fn write_cell_line<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    line: &[(Style, String)],
    width: usize,
    alignment: Alignment,
) -> Result<()> {
    let content_w: usize =
        line.iter().map(|(_, t)| display_width(t)).sum::<usize>() + line.len().saturating_sub(1);
    let padding = width.saturating_sub(content_w);
    match alignment {
        Alignment::Right => {
            write!(writer, " {:>padding$}", "")?;
            write_cell_line_pieces(writer, capabilities, line)?;
            write!(writer, " ")?;
        }
        Alignment::Center => {
            let left = padding / 2;
            let right = padding - left;
            write!(writer, " {:>left$}", "")?;
            write_cell_line_pieces(writer, capabilities, line)?;
            write!(writer, "{:>right$} ", "")?;
        }
        _ => {
            write!(writer, " ")?;
            write_cell_line_pieces(writer, capabilities, line)?;
            write!(writer, "{:>padding$} ", "")?;
        }
    }
    Ok(())
}

pub fn write_table<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    terminal_size: &TerminalSize,
    indent: u16,
    line_prefix: &str,
    prefix_cols: u16,
    table: CurrentTable,
) -> Result<()> {
    let available = terminal_size
        .columns
        .saturating_sub(indent)
        .saturating_sub(prefix_cols);
    if let Some(widths) = calculate_column_widths(&table, available) {
        let total_width: usize = widths.iter().sum();
        let rule_length = min(
            (total_width + 2 * widths.len())
                .try_into()
                .unwrap_or(u16::MAX),
            available,
        );
        write_table_rule(writer, capabilities, indent, line_prefix, rule_length)?;

        let alignments = table.alignments.clone();
        if let Some(head) = table.head {
            write_wrapped_row(
                writer,
                capabilities,
                indent,
                line_prefix,
                head.cells,
                &widths,
                &alignments,
            )?;
            write_table_rule(writer, capabilities, indent, line_prefix, rule_length)?;
        }
        for row in table.rows {
            write_wrapped_row(
                writer,
                capabilities,
                indent,
                line_prefix,
                row.cells,
                &widths,
                &alignments,
            )?;
        }
        write_table_rule(writer, capabilities, indent, line_prefix, rule_length)?;
    }
    Ok(())
}

/// Render a single table row, wrapping each cell to its column width and
/// emitting one visual line per wrapped line, padding shorter cells blank.
fn write_wrapped_row<W: Write>(
    writer: &mut W,
    capabilities: &TerminalCapabilities,
    indent: u16,
    line_prefix: &str,
    cells: Vec<TableCell>,
    widths: &[usize],
    alignments: &[Alignment],
) -> Result<()> {
    let wrapped: Vec<Vec<Vec<(Style, String)>>> = cells
        .iter()
        .zip(widths.iter())
        .map(|(cell, &w)| wrap_cell(&cell.fragments, w))
        .collect();
    let nlines = wrapped.iter().map(|l| l.len()).max().unwrap_or(1).max(1);
    for li in 0..nlines {
        write_table_prefix(writer, indent, line_prefix)?;
        for ((cell_lines, &width), &alignment) in
            wrapped.iter().zip(widths.iter()).zip(alignments.iter())
        {
            let line = cell_lines.get(li).map(|v| v.as_slice()).unwrap_or(&[]);
            write_cell_line(writer, capabilities, line, width, alignment)?;
        }
        writeln!(writer)?;
    }
    Ok(())
}
