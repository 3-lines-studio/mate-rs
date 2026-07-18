// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Terminal utilities.

pub(crate) mod osc;
mod size;

pub mod capabilities;

pub use self::size::PixelSize;
pub use self::size::TerminalSize;

/// A terminal application.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TerminalProgram {
    /// A dumb terminal which does not support any formatting.
    Dumb,
    /// A plain ANSI terminal which supports only standard ANSI formatting.
    Ansi,
}

impl TerminalProgram {
    /// Get the capabilities of this terminal emulator.
    pub fn capabilities(self) -> capabilities::TerminalCapabilities {
        let ansi = capabilities::TerminalCapabilities {
            style: Some(capabilities::StyleCapability::Ansi),
            image: None,
        };
        match self {
            TerminalProgram::Dumb => capabilities::TerminalCapabilities::default(),
            TerminalProgram::Ansi => ansi,
        }
    }
}
