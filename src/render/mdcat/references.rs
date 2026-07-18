// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Provide utilities for references.

use url::Url;

use super::Environment;

/// A base to resolve references with.
pub trait UrlBase {
    /// Resolve the given `reference` against self.
    fn resolve_reference(&self, reference: &str) -> Option<Url>;
}

impl UrlBase for Url {
    /// Resolve a reference against this URL.
    fn resolve_reference(&self, reference: &str) -> Option<Url> {
        match Url::parse(reference).or_else(|_| self.join(reference)) {
            Ok(url) => {
                log::trace!("reference resolved: {self} -> {url}");
                Some(url)
            }
            Err(error) => {
                log::trace!("failed to resolve reference: {self} -> {reference}: {error}");
                None
            }
        }
    }
}

impl UrlBase for Environment {
    /// Resolve a reference against the `base_url` of this environment.
    fn resolve_reference(&self, reference: &str) -> Option<Url> {
        self.base_url.resolve_reference(reference)
    }
}
