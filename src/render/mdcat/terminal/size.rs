// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Terminal size.

use std::cmp::Ordering;

/// The size of a terminal window in pixels.
///
/// This type is partially ordered; a value is smaller than another if all fields
/// are smaller, and greater if all fields are greater.
///
/// If either field is greater and the other smaller values aren't orderable.
#[derive(Debug, Copy, Clone)]
pub struct PixelSize {
    /// The width of the window, in pixels.
    pub x: u32,
    // The height of the window, in pixels.
    pub y: u32,
}

impl PixelSize {
    /// Create a pixel size for a `(x, y)` pair.
    pub fn from_xy((x, y): (u32, u32)) -> Self {
        Self { x, y }
    }
}

impl PartialEq for PixelSize {
    fn eq(&self, other: &Self) -> bool {
        matches!(self.partial_cmp(other), Some(Ordering::Equal))
    }
}

impl PartialOrd for PixelSize {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.x == other.x && self.y == other.y {
            Some(Ordering::Equal)
        } else if self.x < other.x && self.y < other.y {
            Some(Ordering::Less)
        } else if self.x > other.x && self.y > other.y {
            Some(Ordering::Greater)
        } else {
            None
        }
    }
}

/// The size of a terminal.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct TerminalSize {
    /// The width of the terminal, in characters aka columns.
    pub columns: u16,
    /// The height of the terminal, in lines.
    pub rows: u16,
    /// The size in pixels, if available.
    pub pixels: Option<PixelSize>,
    /// The size of one cell, if available.
    pub cell: Option<PixelSize>,
}

impl Default for TerminalSize {
    fn default() -> Self {
        TerminalSize {
            columns: 80,
            rows: 24,
            pixels: None,
            cell: None,
        }
    }
}

impl TerminalSize {
    /// Get terminal size from `$COLUMNS` and `$LINES`.
    ///
    /// Do not assume any knowledge about window size.
    pub fn from_env() -> Option<Self> {
        let columns = std::env::var("COLUMNS")
            .ok()
            .and_then(|value| value.parse::<u16>().ok());
        let rows = std::env::var("LINES")
            .ok()
            .and_then(|value| value.parse::<u16>().ok());

        match (columns, rows) {
            (Some(columns), Some(rows)) => Some(Self {
                columns,
                rows,
                pixels: None,
                cell: None,
            }),
            _ => None,
        }
    }

    /// Query the terminal size. Not supported in mate-rs.
    pub fn from_terminal() -> Option<Self> {
        None
    }

    /// Detect the terminal size.
    ///
    /// Get the terminal size from the underlying TTY, and fallback to
    /// `$COLUMNS` and `$LINES`.
    pub fn detect() -> Option<Self> {
        Self::from_terminal().or_else(Self::from_env)
    }

    /// Shrink the terminal size to the given amount of maximum columns.
    ///
    /// Also shrinks the pixel size accordingly.
    pub fn with_max_columns(&self, max_columns: u16) -> Self {
        let pixels = match (self.pixels, self.cell) {
            (Some(pixels), Some(cell)) => Some(PixelSize {
                x: cell.x * max_columns as u32,
                y: pixels.y,
            }),
            _ => None,
        };
        Self {
            columns: max_columns,
            rows: self.rows,
            pixels,
            cell: self.cell,
        }
    }
}
