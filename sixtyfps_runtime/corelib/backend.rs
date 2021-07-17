/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
The backend is the abstraction for crates that need to do the actual drawing and event loop
*/

use std::path::Path;

use crate::graphics::{Image, Size};
use crate::window::ComponentWindow;

/// Behavior describing how the event loop should terminate.
pub enum EventLoopQuitBehavior {
    /// Terminate the event loop when the last window was closed.
    QuitOnLastWindowClosed,
    /// Keep the event loop running until [`Backend::quit_event_loop()`] is called.
    QuitOnlyExplicitly,
}

/// Interface implemented by backends
pub trait Backend: Send + Sync {
    /// Instantiate a window for a component.
    /// FIXME: should return a Box<dyn PlatformWindow>
    fn create_window(&'static self) -> ComponentWindow;

    /// Spins an event loop and renders the visible windows.
    fn run_event_loop(&'static self, behavior: EventLoopQuitBehavior);

    /// Exits the event loop.
    fn quit_event_loop(&'static self);

    /// This function can be used to register a custom TrueType font with SixtyFPS,
    /// for use with the `font-family` property. The provided slice must be a valid TrueType
    /// font.
    fn register_font_from_memory(
        &'static self,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// This function can be used to register a custom TrueType font with SixtyFPS,
    /// for use with the `font-family` property. The provided path must refer to a valid TrueType
    /// font.
    fn register_font_from_path(
        &'static self,
        path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>>;

    fn set_clipboard_text(&'static self, text: String);
    fn clipboard_text(&'static self) -> Option<String>;

    /// Send an user event to from another thread that should be run in the GUI event loop
    fn post_event(&'static self, event: Box<dyn FnOnce() + Send>);

    fn image_size(&'static self, image: &Image) -> Size;
}

static PRIVATE_BACKEND_INSTANCE: once_cell::sync::OnceCell<Box<dyn Backend + 'static>> =
    once_cell::sync::OnceCell::new();

pub fn instance() -> Option<&'static dyn Backend> {
    use std::ops::Deref;
    PRIVATE_BACKEND_INSTANCE.get().map(|backend_box| backend_box.deref())
}

pub fn instance_or_init(
    factory_fn: impl FnOnce() -> Box<dyn Backend + 'static>,
) -> &'static dyn Backend {
    use std::ops::Deref;
    PRIVATE_BACKEND_INSTANCE.get_or_init(factory_fn).deref()
}
