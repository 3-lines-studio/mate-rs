// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Capabilities of terminal emulators.

/// The capability of basic styling.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum StyleCapability {
    /// The terminal supports ANSI styles including OSC 8.
    Ansi,
}

/// A placeholder for image capabilities (not supported in mate-rs).
#[derive(Debug, Copy, Clone)]
pub enum ImageCapability {
    /// Placeholder variant to satisfy compilation.
    NoImage,
}

/// The capabilities of a terminal.
///
/// See [`crate::render::mdcat::TerminalProgram`] for a way to detect a terminal and derive known capabilities.
#[derive(Debug)]
pub struct TerminalCapabilities {
    /// Whether the terminal supports basic ANSI styling.
    pub style: Option<StyleCapability>,
    /// How the terminal supports images.
    pub image: Option<ImageCapability>,
}

impl Default for TerminalCapabilities {
    /// A terminal which supports nothing.
    fn default() -> Self {
        TerminalCapabilities {
            style: None,
            image: None,
        }
    }
}
