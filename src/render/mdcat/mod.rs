// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(dead_code, unused_variables, unused_imports, missing_docs)]
#![forbid(unsafe_code)]

use std::io::{Result, Write};

use pulldown_cmark::{Event, Options};
use syntect::highlighting::Theme as SyntectTheme;
use syntect::parsing::SyntaxSet;
use url::Url;

pub use crate::render::mdcat::resources::ResourceUrlHandler;
pub use crate::render::mdcat::terminal::capabilities::TerminalCapabilities;
pub use crate::render::mdcat::terminal::{TerminalProgram, TerminalSize};
pub use crate::render::mdcat::theme::Theme;

mod references;
pub mod resources;
pub mod terminal;
mod theme;

mod render;

/// Settings for markdown rendering.
#[derive(Debug)]
pub struct Settings<'a> {
    /// Capabilities of the terminal mdcat writes to.
    pub terminal_capabilities: TerminalCapabilities,
    /// The size of the terminal mdcat writes to.
    pub terminal_size: TerminalSize,
    /// Syntax set for syntax highlighting of code blocks.
    pub syntax_set: &'a SyntaxSet,
    /// Colour theme for mdcat
    pub theme: Theme,
    /// Syntect theme for syntax-highlighted code blocks.
    ///
    /// When set, code blocks are rendered with 24-bit RGB colors from this theme.
    /// When absent, falls back to the built-in Solarized Dark → ANSI color mapping.
    pub syntax_theme: Option<SyntectTheme>,
}

/// The environment to render markdown in.
#[derive(Debug, Clone)]
pub struct Environment {
    /// The base URL to resolve relative URLs with.
    pub base_url: Url,
    /// The local host name.
    pub hostname: String,
}

/// Return the pulldown-cmark options mdcat uses for Markdown parsing.
pub fn markdown_options() -> Options {
    Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
        | Options::ENABLE_DEFINITION_LIST
}

/// Strip YAML frontmatter from the beginning of a Markdown document.
pub fn strip_frontmatter(input: &str) -> &str {
    let after_open = match input
        .strip_prefix("---\n")
        .or_else(|| input.strip_prefix("---\r\n"))
    {
        Some(s) => s,
        None => return input,
    };

    let mut start = 0;
    while start < after_open.len() {
        let end = after_open[start..]
            .find('\n')
            .map_or(after_open.len(), |i| start + i);
        let line = after_open[start..end].trim_end_matches('\r');
        let next = (end + 1).min(after_open.len());
        if line == "---" || line == "..." {
            return &after_open[next..];
        }
        start = end + 1;
    }

    input
}

/// Write markdown to a TTY.
///
/// Iterate over Markdown AST `events`, format each event for TTY output and
/// write the result to a `writer`, using the given `settings` and `environment`
/// for rendering and resource access.
///
/// `push_tty` tries to limit output to the given number of TTY `columns` but
/// does not guarantee that output stays within the column limit.
pub fn push_tty<'a, 'e, W, I>(
    settings: &Settings,
    environment: &Environment,
    resource_handler: &dyn ResourceUrlHandler,
    writer: &'a mut W,
    mut events: I,
) -> Result<()>
where
    I: Iterator<Item = Event<'e>>,
    W: Write,
{
    use render::*;
    let StateAndData(final_state, final_data) = events.try_fold(
        StateAndData(State::default(), StateData::default()),
        |StateAndData(state, data), event| {
            write_event(
                writer,
                settings,
                environment,
                &resource_handler,
                state,
                data,
                event,
            )
        },
    )?;
    finish(writer, settings, environment, final_state, final_data)
}
