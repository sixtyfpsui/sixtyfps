/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#![warn(missing_docs)]
//! Exposed Window API

use crate::graphics::Point;
use crate::input::{KeyEvent, MouseEvent, MouseInputState, TextCursorBlinker};
use crate::items::{ItemRc, ItemRef, ItemWeak};
use crate::properties::PropertyTracker;
use crate::slice::Slice;
use crate::{
    component::{ComponentRc, ComponentWeak},
    SharedString,
};
use core::cell::Cell;
use core::pin::Pin;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// This trait represents the interface that the generated code and the run-time
/// require in order to implement functionality such as device-independent pixels,
/// window resizing and other typically windowing system related tasks.
pub trait PlatformWindow {
    /// Registers the window with the windowing system.
    fn show(self: Rc<Self>);
    /// De-registers the window from the windowing system.
    fn hide(self: Rc<Self>);
    /// Issue a request to the windowing system to re-render the contents of the window. This is typically an asynchronous
    /// request.
    fn request_redraw(&self);
    /// Returns the scale factor set on the window, as provided by the windowing system.
    fn scale_factor(&self) -> f32;
    /// Sets an overriding scale factor for the window. This is typically only used for testing.
    fn set_scale_factor(&self, factor: f32);

    /// This function is called by the generated code when a component and therefore its tree of items are destroyed. The
    /// implementation typically uses this to free the underlying graphics resources cached via [`crate::graphics::RenderingCache`].
    fn free_graphics_resources<'a>(self: Rc<Self>, items: &Slice<'a, Pin<ItemRef<'a>>>);

    /// Show a popup at the given position
    fn show_popup(&self, popup: &ComponentRc, position: Point);
    /// Close the active popup if any
    fn close_popup(&self);

    /// Request for the event loop to wake up and call [`Window::update_window_properties()`].
    fn request_window_properties_update(&self);
    /// Request for the given title string to be set to the windowing system for use as window title.
    fn apply_window_properties(&self, window_item: Pin<&crate::items::Window>);

    /// Return a font metrics trait object for the given font request. This is typically provided by the backend and
    /// requested by text related items in order to measure text metrics with the item's chosen font.
    /// Note that if the FontRequest's pixel_size is 0, it is interpreted as the undefined size and that the
    /// system default font size should be used for the returned font.
    /// With some back ends this may return none unless the window is mapped.
    fn font_metrics(
        &self,
        item_graphics_cache: &crate::item_rendering::CachedRenderingData,
        unresolved_font_request_getter: &dyn Fn() -> crate::graphics::FontRequest,
        reference_text: Pin<&crate::properties::Property<SharedString>>,
    ) -> Box<dyn crate::graphics::FontMetrics>;

    /// Return self as any so the backend can upcast
    fn as_any(&self) -> &dyn core::any::Any;
}

struct WindowPropertiesTracker {
    window_weak: Weak<Window>,
}

impl crate::properties::PropertyChangeHandler for WindowPropertiesTracker {
    fn notify(&self) {
        if let Some(platform_window) =
            self.window_weak.upgrade().and_then(|window| window.platform_window.get().cloned())
        {
            platform_window.request_window_properties_update();
        };
    }
}

struct WindowRedrawTracker {
    window_weak: Weak<Window>,
}

impl crate::properties::PropertyChangeHandler for WindowRedrawTracker {
    fn notify(&self) {
        if let Some(platform_window) =
            self.window_weak.upgrade().and_then(|window| window.platform_window.get().cloned())
        {
            platform_window.request_redraw();
        };
    }
}

/// Structure that represent a Window in the runtime
pub struct Window {
    /// FIXME! use Box instead;
    platform_window: once_cell::unsync::OnceCell<Rc<dyn PlatformWindow>>,
    component: RefCell<ComponentWeak>,
    mouse_input_state: Cell<MouseInputState>,
    redraw_tracker: once_cell::unsync::OnceCell<Pin<Box<PropertyTracker<WindowRedrawTracker>>>>,
    window_properties_tracker:
        once_cell::unsync::OnceCell<Pin<Box<PropertyTracker<WindowPropertiesTracker>>>>,
    /// Gets dirty when the layout restrictions, or some other property of the windows change
    /// FIXME: should only be exposed to the backend
    pub meta_properties_tracker: Pin<Rc<PropertyTracker>>,

    focus_item: RefCell<ItemWeak>,
    cursor_blinker: RefCell<pin_weak::rc::PinWeak<crate::input::TextCursorBlinker>>,
}

impl Drop for Window {
    fn drop(&mut self) {
        if let Some(existing_blinker) = self.cursor_blinker.borrow().upgrade() {
            existing_blinker.stop();
        }
    }
}

impl Window {
    /// Create a new instance of the window, given the platform_window factory fn
    pub fn new(
        platform_window_fn: impl FnOnce(&Weak<Window>) -> Rc<dyn PlatformWindow>,
    ) -> Rc<Self> {
        let window = Rc::new(Self {
            platform_window: Default::default(),
            component: Default::default(),
            mouse_input_state: Default::default(),
            redraw_tracker: Default::default(),
            window_properties_tracker: Default::default(),
            meta_properties_tracker: Rc::pin(Default::default()),
            focus_item: Default::default(),
            cursor_blinker: Default::default(),
        });
        let window_weak = Rc::downgrade(&window);
        window.platform_window.set(platform_window_fn(&window_weak)).ok().unwrap();

        window
            .window_properties_tracker
            .set(Box::pin(PropertyTracker::new_with_change_handler(WindowPropertiesTracker {
                window_weak: window_weak.clone(),
            })))
            .ok()
            .unwrap();
        window
            .redraw_tracker
            .set(Box::pin(PropertyTracker::new_with_change_handler(WindowRedrawTracker {
                window_weak,
            })))
            .ok()
            .unwrap();

        window
    }

    /// Associates this window with the specified component. Further event handling and rendering, etc. will be
    /// done with that component.
    pub fn set_component(&self, component: &ComponentRc) {
        if let Some(w) = self.platform_window.get() {
            w.close_popup(); // ensure the popup is closed
        }
        self.focus_item.replace(Default::default());
        self.mouse_input_state.replace(Default::default());
        self.component.replace(ComponentRc::downgrade(component));
        self.meta_properties_tracker.set_dirty(); // component changed, layout constraints for sure must be re-calculated
        self.request_window_properties_update();
        self.request_redraw();
    }

    /// return the component.
    /// Panics if it wasn't set.
    pub fn component(&self) -> ComponentRc {
        self.component.borrow().upgrade().unwrap()
    }

    /// returns the component or None if it isn't set.
    pub fn try_component(&self) -> Option<ComponentRc> {
        self.component.borrow().upgrade()
    }

    /// Receive a mouse event and pass it to the items of the component to
    /// change their state.
    ///
    /// Arguments:
    /// * `pos`: The position of the mouse event in window physical coordinates.
    /// * `what`: The type of mouse event.
    /// * `component`: The SixtyFPS compiled component that provides the tree of items.
    pub fn process_mouse_input(self: Rc<Self>, event: MouseEvent) {
        crate::animations::update_animations();
        let component = self.component.borrow().upgrade().unwrap();
        self.mouse_input_state.set(crate::input::process_mouse_input(
            component,
            event,
            &ComponentWindow::new(self.clone()),
            self.mouse_input_state.take(),
        ));
    }
    /// Receive a key event and pass it to the items of the component to
    /// change their state.
    ///
    /// Arguments:
    /// * `event`: The key event received by the windowing system.
    /// * `component`: The SixtyFPS compiled component that provides the tree of items.
    pub fn process_key_input(self: Rc<Self>, event: &KeyEvent) {
        let mut item = self.focus_item.borrow().clone();
        while let Some(focus_item) = item.upgrade() {
            let window = &ComponentWindow::new(self.clone());
            if focus_item.borrow().as_ref().key_event(event, window)
                == crate::input::KeyEventResult::EventAccepted
            {
                return;
            }
            item = focus_item.parent_item();
        }
    }

    /// Installs a binding on the specified property that's toggled whenever the text cursor is supposed to be visible or not.
    pub fn set_cursor_blink_binding(&self, prop: &crate::Property<bool>) {
        let existing_blinker = self.cursor_blinker.borrow().clone();

        let blinker = existing_blinker.upgrade().unwrap_or_else(|| {
            let new_blinker = TextCursorBlinker::new();
            *self.cursor_blinker.borrow_mut() =
                pin_weak::rc::PinWeak::downgrade(new_blinker.clone());
            new_blinker
        });

        TextCursorBlinker::set_binding(blinker, prop);
    }

    /// Sets the focus to the item pointed to by item_ptr. This will remove the focus from any
    /// currently focused item.
    pub fn set_focus_item(self: Rc<Self>, focus_item: &ItemRc) {
        let window = ComponentWindow::new(self.clone());

        if let Some(old_focus_item) = self.as_ref().focus_item.borrow().upgrade() {
            old_focus_item
                .borrow()
                .as_ref()
                .focus_event(&crate::input::FocusEvent::FocusOut, &window);
        }

        *self.as_ref().focus_item.borrow_mut() = focus_item.downgrade();

        focus_item.borrow().as_ref().focus_event(&crate::input::FocusEvent::FocusIn, &window);
    }

    /// Sets the focus on the window to true or false, depending on the have_focus argument.
    /// This results in WindowFocusReceived and WindowFocusLost events.
    pub fn set_focus(self: Rc<Self>, have_focus: bool) {
        let window = ComponentWindow::new(self.clone());
        let event = if have_focus {
            crate::input::FocusEvent::WindowReceivedFocus
        } else {
            crate::input::FocusEvent::WindowLostFocus
        };

        if let Some(focus_item) = self.as_ref().focus_item.borrow().upgrade() {
            focus_item.borrow().as_ref().focus_event(&event, &window);
        }
    }

    /// If the component's root item is a Window element, then this function synchronizes its properties, such as the title
    /// for example, with the properties known to the windowing system.
    pub fn update_window_properties(self: Rc<Self>) {
        if let Some(window_properties_tracker) = self.window_properties_tracker.get() {
            // No `if !dirty { return; }` check here because the backend window may be newly mapped and not up-to-date, so force
            // an evaluation.
            window_properties_tracker.as_ref().evaluate_as_dependency_root(|| {
                let component = self.component();
                let component = ComponentRc::borrow_pin(&component);
                let root_item = component.as_ref().get_item_ref(0);

                if let Some(window_item) = ItemRef::downcast_pin::<crate::items::Window>(root_item)
                {
                    self.platform_window.get().unwrap().apply_window_properties(window_item);
                }
            });
        }
    }

    /// Calls draw_fn using a [`crate::properties::PropertyTracker`], which is set up to issue a call to [`PlatformWindow::request_redraw`]
    /// when any properties accessed during drawing change.
    pub fn draw_tracked<R>(self: Rc<Self>, draw_fn: impl FnOnce() -> R) -> R {
        if let Some(redraw_tracker) = self.redraw_tracker.get() {
            redraw_tracker.as_ref().evaluate_as_dependency_root(|| draw_fn())
        } else {
            draw_fn()
        }
    }
}

impl core::ops::Deref for Window {
    type Target = dyn PlatformWindow;

    fn deref(&self) -> &Self::Target {
        self.platform_window.get().unwrap().as_ref()
    }
}

/// The ComponentWindow is the (rust) facing public type that can render the items
/// of components to the screen.
#[repr(C)]
#[derive(Clone)]
pub struct ComponentWindow(pub std::rc::Rc<Window>);

impl ComponentWindow {
    /// Creates a new instance of a ComponentWindow based on the given window implementation. Only used
    /// internally.
    pub fn new(window_impl: std::rc::Rc<Window>) -> Self {
        Self(window_impl)
    }

    /// Registers the window with the windowing system, in order to render the component's items and react
    /// to input events once the event loop spins.
    pub fn show(&self) {
        self.0.platform_window.get().unwrap().clone().show();
        self.0.clone().update_window_properties();
    }

    /// De-registers the window with the windowing system.
    pub fn hide(&self) {
        self.0.platform_window.get().unwrap().clone().hide();
    }

    /// Returns the scale factor set on the window.
    pub fn scale_factor(&self) -> f32 {
        self.0.scale_factor()
    }

    /// Sets an overriding scale factor for the window. This is typically only used for testing.
    pub fn set_scale_factor(&self, factor: f32) {
        self.0.set_scale_factor(factor)
    }

    /// This function is called by the generated code when a component and therefore its tree of items are destroyed. The
    /// implementation typically uses this to free the underlying graphics resources cached via [RenderingCache][`crate::graphics::RenderingCache`].
    pub fn free_graphics_resources<'a>(&self, items: &Slice<'a, Pin<ItemRef<'a>>>) {
        self.0.platform_window.get().unwrap().clone().free_graphics_resources(items);
    }

    /// Installs a binding on the specified property that's toggled whenever the text cursor is supposed to be visible or not.
    pub(crate) fn set_cursor_blink_binding(&self, prop: &crate::properties::Property<bool>) {
        self.0.clone().set_cursor_blink_binding(prop)
    }

    pub(crate) fn process_key_input(&self, event: &KeyEvent) {
        self.0.clone().process_key_input(event)
    }

    /// Clears the focus on any previously focused item and makes the provided
    /// item the focus item, in order to receive future key events.
    pub fn set_focus_item(&self, focus_item: &ItemRc) {
        self.0.clone().set_focus_item(focus_item)
    }

    /// Associates this window with the specified component, for future event handling, etc.
    pub fn set_component(&self, component: &ComponentRc) {
        self.0.set_component(component)
    }

    /// Show a popup at the given position
    pub fn show_popup(&self, popup: &ComponentRc, position: Point) {
        self.0.platform_window.get().unwrap().clone().show_popup(popup, position)
    }
    /// Close the active popup if any
    pub fn close_popup(&self) {
        self.0.platform_window.get().unwrap().clone().close_popup()
    }
}

/// This module contains the functions needed to interface with the event loop and window traits
/// from outside the Rust language.
#[cfg(feature = "ffi")]
pub mod ffi {
    #![allow(unsafe_code)]

    use super::*;

    #[allow(non_camel_case_types)]
    type c_void = ();

    /// Same layout as ComponentWindow
    #[repr(C)]
    pub struct ComponentWindowOpaque(*const c_void);

    /// Releases the reference to the component window held by handle.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_drop(handle: *mut ComponentWindowOpaque) {
        assert_eq!(
            core::mem::size_of::<ComponentWindow>(),
            core::mem::size_of::<ComponentWindowOpaque>()
        );
        core::ptr::read(handle as *mut ComponentWindow);
    }

    /// Releases the reference to the component window held by handle.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_clone(
        source: *const ComponentWindowOpaque,
        target: *mut ComponentWindowOpaque,
    ) {
        assert_eq!(
            core::mem::size_of::<ComponentWindow>(),
            core::mem::size_of::<ComponentWindowOpaque>()
        );
        let window = &*(source as *const ComponentWindow);
        core::ptr::write(target as *mut ComponentWindow, window.clone());
    }

    /// Spins an event loop and renders the items of the provided component in this window.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_show(handle: *const ComponentWindowOpaque) {
        let window = &*(handle as *const ComponentWindow);
        window.show();
    }

    /// Spins an event loop and renders the items of the provided component in this window.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_hide(handle: *const ComponentWindowOpaque) {
        let window = &*(handle as *const ComponentWindow);
        window.hide();
    }

    /// Returns the window scale factor.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_get_scale_factor(
        handle: *const ComponentWindowOpaque,
    ) -> f32 {
        assert_eq!(
            core::mem::size_of::<ComponentWindow>(),
            core::mem::size_of::<ComponentWindowOpaque>()
        );
        let window = &*(handle as *const ComponentWindow);
        window.scale_factor()
    }

    /// Sets the window scale factor, merely for testing purposes.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_set_scale_factor(
        handle: *const ComponentWindowOpaque,
        value: f32,
    ) {
        let window = &*(handle as *const ComponentWindow);
        window.set_scale_factor(value)
    }

    /// Sets the window scale factor, merely for testing purposes.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_free_graphics_resources<'a>(
        handle: *const ComponentWindowOpaque,
        items: &Slice<'a, Pin<ItemRef<'a>>>,
    ) {
        let window = &*(handle as *const ComponentWindow);
        window.free_graphics_resources(items)
    }

    /// Sets the focus item.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_set_focus_item(
        handle: *const ComponentWindowOpaque,
        focus_item: &ItemRc,
    ) {
        let window = &*(handle as *const ComponentWindow);
        window.set_focus_item(focus_item)
    }

    /// Associates the window with the given component.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_set_component(
        handle: *const ComponentWindowOpaque,
        component: &ComponentRc,
    ) {
        let window = &*(handle as *const ComponentWindow);
        window.set_component(component)
    }

    /// Show a popup.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_window_show_popup(
        handle: *const ComponentWindowOpaque,
        popup: &ComponentRc,
        position: crate::graphics::Point,
    ) {
        let window = &*(handle as *const ComponentWindow);
        window.show_popup(popup, position);
    }
    /// Close the current popup
    pub unsafe extern "C" fn sixtyfps_component_window_close_popup(
        handle: *const ComponentWindowOpaque,
    ) {
        let window = &*(handle as *const ComponentWindow);
        window.close_popup();
    }
}
