/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
This module contains the builtin image related items.

When adding an item or a property, it needs to be kept in sync with different place.
Lookup the [`crate::items`] module documentation.
*/
use super::{Item, ItemConsts, ItemRc};
use crate::graphics::Rect;
use crate::input::{
    FocusEvent, InputEventFilterResult, InputEventResult, KeyEvent, KeyEventResult, MouseEvent,
};
use crate::item_rendering::CachedRenderingData;
use crate::item_rendering::ItemRenderer;
use crate::layout::{LayoutInfo, Orientation};
#[cfg(feature = "rtti")]
use crate::rtti::*;
use crate::window::ComponentWindow;
use crate::{Brush, Property};
use const_field_offset::FieldOffsets;
use core::pin::Pin;
use sixtyfps_corelib_macros::*;

#[derive(Copy, Clone, Debug, PartialEq, strum_macros::EnumString, strum_macros::Display)]
#[repr(C)]
#[allow(non_camel_case_types)]
pub enum ImageFit {
    fill,
    contain,
    cover,
}

impl Default for ImageFit {
    fn default() -> Self {
        ImageFit::fill
    }
}

#[repr(C)]
#[derive(FieldOffsets, Default, SixtyFPSElement)]
#[pin]
/// The implementation of the `Image` element
pub struct ImageItem {
    pub source: Property<crate::graphics::Image>,
    pub x: Property<f32>,
    pub y: Property<f32>,
    pub width: Property<f32>,
    pub height: Property<f32>,
    pub image_fit: Property<ImageFit>,
    pub cached_rendering_data: CachedRenderingData,
}

impl Item for ImageItem {
    fn init(self: Pin<&Self>, _window: &ComponentWindow) {}

    fn geometry(self: Pin<&Self>) -> Rect {
        euclid::rect(self.x(), self.y(), self.width(), self.height())
    }

    fn layouting_info(
        self: Pin<&Self>,
        orientation: Orientation,
        _window: &ComponentWindow,
    ) -> LayoutInfo {
        let natural_size = self.source().size();
        LayoutInfo {
            preferred: match orientation {
                _ if natural_size.width == 0. || natural_size.height == 0. => 0.,
                Orientation::Horizontal => natural_size.width,
                Orientation::Vertical => natural_size.height * self.width() / natural_size.width,
            },
            ..Default::default()
        }
    }

    fn input_event_filter_before_children(
        self: Pin<&Self>,
        _: MouseEvent,
        _window: &ComponentWindow,
        _self_rc: &ItemRc,
    ) -> InputEventFilterResult {
        InputEventFilterResult::ForwardAndIgnore
    }

    fn input_event(
        self: Pin<&Self>,
        _: MouseEvent,
        _window: &ComponentWindow,
        _self_rc: &ItemRc,
    ) -> InputEventResult {
        InputEventResult::EventIgnored
    }

    fn key_event(self: Pin<&Self>, _: &KeyEvent, _window: &ComponentWindow) -> KeyEventResult {
        KeyEventResult::EventIgnored
    }

    fn focus_event(self: Pin<&Self>, _: &FocusEvent, _window: &ComponentWindow) {}

    fn render(self: Pin<&Self>, backend: &mut &mut dyn ItemRenderer) {
        (*backend).draw_image(self)
    }
}

impl ItemConsts for ImageItem {
    const cached_rendering_data_offset: const_field_offset::FieldOffset<
        ImageItem,
        CachedRenderingData,
    > = ImageItem::FIELD_OFFSETS.cached_rendering_data.as_unpinned_projection();
}

#[repr(C)]
#[derive(FieldOffsets, Default, SixtyFPSElement)]
#[pin]
/// The implementation of the `ClippedImage` element
pub struct ClippedImage {
    pub source: Property<crate::graphics::Image>,
    pub x: Property<f32>,
    pub y: Property<f32>,
    pub width: Property<f32>,
    pub height: Property<f32>,
    pub image_fit: Property<ImageFit>,
    pub colorize: Property<Brush>,
    pub source_clip_x: Property<i32>,
    pub source_clip_y: Property<i32>,
    pub source_clip_width: Property<i32>,
    pub source_clip_height: Property<i32>,
    pub cached_rendering_data: CachedRenderingData,
}

impl Item for ClippedImage {
    fn init(self: Pin<&Self>, _window: &ComponentWindow) {}

    fn geometry(self: Pin<&Self>) -> Rect {
        euclid::rect(self.x(), self.y(), self.width(), self.height())
    }

    fn layouting_info(
        self: Pin<&Self>,
        orientation: Orientation,
        _window: &ComponentWindow,
    ) -> LayoutInfo {
        let natural_size = self.source().size();
        LayoutInfo {
            preferred: match orientation {
                _ if natural_size.width == 0. || natural_size.height == 0. => 0.,
                Orientation::Horizontal => natural_size.width,
                Orientation::Vertical => natural_size.height * self.width() / natural_size.width,
            },
            ..Default::default()
        }
    }

    fn input_event_filter_before_children(
        self: Pin<&Self>,
        _: MouseEvent,
        _window: &ComponentWindow,
        _self_rc: &ItemRc,
    ) -> InputEventFilterResult {
        InputEventFilterResult::ForwardAndIgnore
    }

    fn input_event(
        self: Pin<&Self>,
        _: MouseEvent,
        _window: &ComponentWindow,
        _self_rc: &ItemRc,
    ) -> InputEventResult {
        InputEventResult::EventIgnored
    }

    fn key_event(self: Pin<&Self>, _: &KeyEvent, _window: &ComponentWindow) -> KeyEventResult {
        KeyEventResult::EventIgnored
    }

    fn focus_event(self: Pin<&Self>, _: &FocusEvent, _window: &ComponentWindow) {}

    fn render(self: Pin<&Self>, backend: &mut &mut dyn ItemRenderer) {
        (*backend).draw_clipped_image(self)
    }
}

impl ItemConsts for ClippedImage {
    const cached_rendering_data_offset: const_field_offset::FieldOffset<
        ClippedImage,
        CachedRenderingData,
    > = ClippedImage::FIELD_OFFSETS.cached_rendering_data.as_unpinned_projection();
}
