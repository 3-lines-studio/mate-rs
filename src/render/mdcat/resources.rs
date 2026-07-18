// Copyright 2018-2020 Sebastian Wiesner <sebastian@swsnr.de>

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Access to resources referenced from markdown documents.

use std::fmt::Debug;
use std::io::{Error, ErrorKind, Result};

use url::Url;

/// Data of a resource with associated mime type.
#[derive(Debug, Clone)]
pub struct MimeData {
    /// The mime type if known.
    pub mime_type: Option<String>,
    /// The data.
    pub data: Vec<u8>,
}

impl MimeData {
    /// Get the essence of the mime type, if any.
    pub fn mime_type_essence(&self) -> Option<&str> {
        self.mime_type.as_deref()
    }
}

/// Handle resource URLs.
pub trait ResourceUrlHandler {
    /// Read a resource.
    ///
    /// Read data from the given `url`, and return the data and its associated mime type if known,
    /// or any IO error which occurred while reading from the resource.
    ///
    /// Alternatively, return an IO error with [`ErrorKind::Unsupported`] to indicate that the
    /// given `url` is not supported by this resource handler.  In this case a higher level
    /// resource handler may try a different handler.
    fn read_resource(&self, url: &Url) -> Result<MimeData>;
}

impl<R: ResourceUrlHandler + ?Sized> ResourceUrlHandler for &'_ R {
    fn read_resource(&self, url: &Url) -> Result<MimeData> {
        (*self).read_resource(url)
    }
}

/// A resource handler which doesn't read anything.
#[derive(Debug, Clone, Copy)]
pub struct NoopResourceHandler;

impl ResourceUrlHandler for NoopResourceHandler {
    /// Always return an [`ErrorKind::Unsupported`] error.
    fn read_resource(&self, url: &Url) -> Result<MimeData> {
        Err(Error::new(
            ErrorKind::Unsupported,
            format!("Reading from resource {url} is not supported"),
        ))
    }
}
