/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#![warn(missing_docs)]
/*!
    Graphics Abstractions.

    This module contains the abstractions and convenience types used for rendering.

    The run-time library also makes use of [RenderingCache] to store the rendering primitives
    created by the backend in a type-erased manner.
*/
extern crate alloc;
#[cfg(feature = "rtti")]
use crate::rtti::*;
use crate::SharedString;
use auto_enums::auto_enum;

/// 2D Rectangle
pub type Rect = euclid::default::Rect<f32>;
/// 2D Rectangle with integer coordinates
pub type IntRect = euclid::default::Rect<i32>;
/// 2D Point
pub type Point = euclid::default::Point2D<f32>;
/// 2D Size
pub type Size = euclid::default::Size2D<f32>;
/// 2D Transform
pub type Transform = euclid::default::Transform2D<f32>;

pub(crate) mod color;
pub use color::*;

mod path;
pub use path::*;

mod brush;
pub use brush::*;

pub(crate) mod image;
pub use self::image::*;

/// CachedGraphicsData allows the graphics backend to store an arbitrary piece of data associated with
/// an item, which is typically computed by accessing properties. The dependency_tracker is used to allow
/// for a lazy computation. Typically back ends store either compute intensive data or handles that refer to
/// data that's stored in GPU memory.
pub struct CachedGraphicsData<T> {
    /// The backend specific data.
    pub data: T,
    /// The property tracker that should be used to evaluate whether the primitive needs to be re-created
    /// or not.
    pub dependency_tracker: core::pin::Pin<Box<crate::properties::PropertyTracker>>,
}

impl<T> CachedGraphicsData<T> {
    /// Creates a new TrackingRenderingPrimitive by evaluating the provided update_fn once, storing the returned
    /// rendering primitive and initializing the dependency tracker.
    pub fn new(update_fn: impl FnOnce() -> T) -> Self {
        let dependency_tracker = Box::pin(crate::properties::PropertyTracker::default());
        let data = dependency_tracker.as_ref().evaluate(update_fn);
        Self { data, dependency_tracker }
    }
}

/// The RenderingCache, in combination with CachedGraphicsData, allows back ends to store data that's either
/// intensive to compute or has bad CPU locality. Back ends typically keep a RenderingCache instance and use
/// the item's cached_rendering_data() integer as index in the vec_arena::Arena.
pub struct RenderingCache<T> {
    slab: slab::Slab<CachedGraphicsData<T>>,
    generation: usize,
}

impl<T> Default for RenderingCache<T> {
    fn default() -> Self {
        Self { slab: Default::default(), generation: 1 }
    }
}

impl<T> RenderingCache<T> {
    /// Returns the generation of the cache. The generation starts at 1 and is increased
    /// whenever the cache is cleared, for example when the GL context is lost.
    pub fn generation(&self) -> usize {
        self.generation
    }

    /// Retrieves a mutable reference to the cached graphics data at index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut CachedGraphicsData<T>> {
        self.slab.get_mut(index)
    }

    /// Inserts data into the cache and returns the index for retrieval later.
    pub fn insert(&mut self, data: CachedGraphicsData<T>) -> usize {
        self.slab.insert(data)
    }

    /// Retrieves an immutable reference to the cached graphics data at index.
    pub fn get(&self, index: usize) -> Option<&CachedGraphicsData<T>> {
        self.slab.get(index)
    }

    /// Removes the cached graphics data at the given index.
    pub fn remove(&mut self, index: usize) -> CachedGraphicsData<T> {
        self.slab.remove(index)
    }

    /// Removes all entries from the cache and increases the cache's generation count, so
    /// that stale index access can be avoided.
    pub fn clear(&mut self) {
        self.slab.clear();
        self.generation += 1;
    }
}
/// FontRequest collects all the developer-configurable properties for fonts, such as family, weight, etc.
/// It is submitted as a request to the platform font system (i.e. CoreText on macOS) and in exchange the
/// backend returns a Box<dyn Font>.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontRequest {
    /// The name of the font family to be used, such as "Helvetica". An empty family name means the system
    /// default font family should be used.
    pub family: Option<SharedString>,
    /// If the weight is None, the the system default font weight should be used.
    pub weight: Option<i32>,
    /// If the pixel size is None, the system default font size should be used.
    pub pixel_size: Option<f32>,
    /// The additional spacing (or shrinking if negative) between glyphs. This is usually not submitted to
    /// the font-subsystem but collected here for API convenience
    pub letter_spacing: Option<f32>,
}

impl FontRequest {
    /// Consumes the FontRequest, replaces any missing fields from the specified other request and
    /// returns the new request.
    pub fn merge(self, other: &FontRequest) -> Self {
        Self {
            family: self.family.or_else(|| other.family.clone()),
            weight: self.weight.or(other.weight),
            pixel_size: self.pixel_size.or(other.pixel_size),
            letter_spacing: self.letter_spacing.or(other.letter_spacing),
        }
    }
}

/// The FontMetrics trait is constructed from a FontRequest by the graphics backend and supplied to text related
/// items in order to measure text.
pub trait FontMetrics {
    /// Returns the size of the given string in logical pixels.
    /// When set, `max_width` means that one need to wrap the text so it does not go further than that
    fn text_size(&self, text: &str, max_width: Option<f32>) -> Size;
    /// Returns the height of a line of text.
    fn line_height(&self) -> f32;
    /// Returns the (UTF-8) byte offset in the given text that refers to the character that contributed to
    /// the glyph cluster that's visually nearest to the given x coordinate. This is used for hit-testing,
    /// for example when receiving a mouse click into a text field. Then this function returns the "cursor"
    /// position.
    fn text_offset_for_x_position<'a>(&self, text: &'a str, x: f32) -> usize;
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    #![allow(unsafe_code)]

    /// Expand Rect so that cbindgen can see it. ( is in fact euclid::default::Rect<f32>)
    #[cfg(cbindgen)]
    #[repr(C)]
    struct Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    }

    /// Expand IntRect so that cbindgen can see it. ( is in fact euclid::default::Rect<i32>)
    #[cfg(cbindgen)]
    #[repr(C)]
    struct IntRect {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    }

    /// Expand Point so that cbindgen can see it. ( is in fact euclid::default::Point2D<f32>)
    #[cfg(cbindgen)]
    #[repr(C)]
    struct Point {
        x: f32,
        y: f32,
    }

    /// Expand Size so that cbindgen can see it. ( is in fact euclid::default::Size2D<f32>)
    #[cfg(cbindgen)]
    #[repr(C)]
    struct Size {
        width: f32,
        height: f32,
    }

    pub use super::path::ffi::*;
}
