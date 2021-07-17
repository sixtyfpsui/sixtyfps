/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use crate::slice::Slice;
use crate::{SharedString, SharedVector};

/// A resource is a reference to binary data, for example images. They can be accessible on the file
/// system or embedded in the resulting binary. Or they might be URLs to a web server and a downloaded
/// is necessary before they can be used.
#[derive(Clone, PartialEq, Debug)]
#[repr(u8)]
pub enum ImageInner {
    /// A resource that does not represent any data.
    None,
    /// A resource that points to a file in the file system
    AbsoluteFilePath(SharedString),
    /// A resource that is embedded in the program and accessible via pointer
    /// The format is the same as in a file
    EmbeddedData(Slice<'static, u8>),
    /// Raw ARGB
    #[allow(missing_docs)]
    EmbeddedRgbaImage { width: u32, height: u32, data: SharedVector<u32> },
}

impl Default for ImageInner {
    fn default() -> Self {
        ImageInner::None
    }
}

impl<'a> From<&'a Image> for &'a ImageInner {
    fn from(other: &'a Image) -> Self {
        &other.0
    }
}

/// Error generated if an image cannot be loaded for any reasons.
#[derive(Default, Debug, PartialEq)]
pub struct LoadImageError(());

/// An image type that can be displayed by the Image element
#[repr(transparent)]
#[derive(Default, Clone, Debug, PartialEq, derive_more::From)]
pub struct Image(ImageInner);

impl Image {
    /// Load an Image from a path to a file containing an image
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, LoadImageError> {
        Ok(Image(ImageInner::AbsoluteFilePath(path.to_str().ok_or(LoadImageError(()))?.into())))
    }
    /*
    /// Create an image from argb pixels.
    ///
    /// Returns an error if the `data` does not have a size of `width * height`.
    pub fn load_from_argb(
        width: u32,
        height: u32,
        data: SharedVector<u32>,
    ) -> Result<Self, LoadImageError> {
        if data.len() != width as usize * height as usize {
            Err(LoadImageError(()))
        } else {
            Ok(Image(ImageReference::EmbeddedRgbaImage { width, height, data }))
        }
    }
    */

    /// Returns the size of the Image in pixels.
    pub fn size(&self) -> crate::graphics::Size {
        match crate::backend::instance() {
            Some(backend) => backend.image_size(self),
            None => panic!("sixtyfps::Image::size() called too early (before a graphics backend was chosen). You need to create a component first."),
        }
    }
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    #![allow(unsafe_code)]

    use super::super::Size;
    use super::*;

    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_image_size(image: &Image) -> Size {
        image.size()
    }
}
