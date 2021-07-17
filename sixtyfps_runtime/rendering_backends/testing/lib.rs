/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!

*NOTE*: This library is an internal crate for the [SixtyFPS project](https://sixtyfps.io).
This crate should not be used directly by application using SixtyFPS.
You should use the `sixtyfps` crate instead.

*/
#![doc(html_logo_url = "https://sixtyfps.io/resources/logo.drawio.svg")]

use image::GenericImageView;
use sixtyfps_corelib::component::ComponentRc;
use sixtyfps_corelib::graphics::{FontMetrics, Image, Size};
use sixtyfps_corelib::slice::Slice;
use sixtyfps_corelib::window::{ComponentWindow, PlatformWindow, Window};
use sixtyfps_corelib::{ImageInner, Property};
use std::path::Path;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Mutex;

#[derive(Default)]
pub struct TestingBackend {
    clipboard: Mutex<Option<String>>,
}

impl sixtyfps_corelib::backend::Backend for TestingBackend {
    fn create_window(&'static self) -> ComponentWindow {
        ComponentWindow::new(Window::new(|_| Rc::new(TestingWindow::default())))
    }

    fn run_event_loop(&'static self, _behavior: sixtyfps_corelib::backend::EventLoopQuitBehavior) {
        unimplemented!("running an event loop with the testing backend");
    }

    fn quit_event_loop(&'static self) {}

    fn register_font_from_memory(
        &'static self,
        _data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    fn register_font_from_path(
        &'static self,
        _path: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    fn set_clipboard_text(&'static self, text: String) {
        *self.clipboard.lock().unwrap() = Some(text);
    }

    fn clipboard_text(&'static self) -> Option<String> {
        self.clipboard.lock().unwrap().clone()
    }

    fn post_event(&'static self, _event: Box<dyn FnOnce() + Send>) {
        unimplemented!("event with the testing backend");
    }

    fn image_size(&'static self, image: &Image) -> Size {
        let inner: &ImageInner = image.into();
        match &inner {
            ImageInner::None => Default::default(),
            ImageInner::EmbeddedRgbaImage { width, height, .. } => {
                Size::new(*width as _, *height as _)
            }
            ImageInner::AbsoluteFilePath(path) => image::open(Path::new(path.as_str()))
                .map(|img| {
                    let dim = img.dimensions();
                    Size::new(dim.0 as _, dim.1 as _)
                })
                .unwrap_or_default(),
            ImageInner::EmbeddedData(data) => image::load_from_memory(data.as_slice())
                .map(|img| {
                    let dim = img.dimensions();
                    Size::new(dim.0 as _, dim.1 as _)
                })
                .unwrap_or_default(),
        }
    }
}

pub struct TestingWindow {
    scale_factor: Pin<Box<Property<f32>>>,
}

impl Default for TestingWindow {
    fn default() -> Self {
        Self { scale_factor: Box::pin(Property::new(1.)) }
    }
}
impl PlatformWindow for TestingWindow {
    fn show(self: Rc<Self>) {
        unimplemented!("showing a testing window")
    }

    fn hide(self: Rc<Self>) {}

    fn request_redraw(&self) {}

    fn scale_factor(&self) -> f32 {
        self.scale_factor.as_ref().get()
    }

    fn set_scale_factor(&self, factor: f32) {
        self.scale_factor.as_ref().set(factor);
    }

    fn free_graphics_resources<'a>(
        self: Rc<Self>,
        _items: &Slice<'a, Pin<sixtyfps_corelib::items::ItemRef<'a>>>,
    ) {
    }

    fn show_popup(&self, _popup: &ComponentRc, _position: sixtyfps_corelib::graphics::Point) {
        todo!()
    }

    fn close_popup(&self) {}

    fn request_window_properties_update(&self) {}

    fn apply_window_properties(&self, _window_item: Pin<&sixtyfps_corelib::items::Window>) {
        todo!()
    }

    fn font_metrics(
        &self,
        _item_graphics_cache: &sixtyfps_corelib::item_rendering::CachedRenderingData,
        _unresolved_font_request_getter: &dyn Fn() -> sixtyfps_corelib::graphics::FontRequest,
        _reference_text: std::pin::Pin<&sixtyfps_corelib::Property<sixtyfps_corelib::SharedString>>,
    ) -> Box<dyn sixtyfps_corelib::graphics::FontMetrics> {
        Box::new(TestingFontMetrics::default())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Default)]
struct TestingFontMetrics {}

impl FontMetrics for TestingFontMetrics {
    fn text_size(&self, text: &str, _max_width: Option<f32>) -> Size {
        Size::new(text.len() as f32 * 10., 10.)
    }

    fn line_height(&self) -> f32 {
        10.
    }

    fn text_offset_for_x_position(&self, _text: &str, _x: f32) -> usize {
        0
    }
}

/// Initialize the testing backend.
/// Must be called before any call that would otherwise initialize the rendering backend.
/// Calling it when the rendering backend is already initialized will have no effects
pub fn init() {
    sixtyfps_corelib::backend::instance_or_init(|| Box::new(TestingBackend::default()));
}
