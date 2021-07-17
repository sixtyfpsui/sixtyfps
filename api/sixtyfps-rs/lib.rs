/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
# SixtyFPS

This crate is the main entry point for embedding user interfaces designed with
[SixtyFPS UI](https://sixtyfps.io/) in Rust programs.

Included in this documentation is also the [language reference](docs::langref),
documentation of [builtin elements](docs::builtin_elements), [widgets](docs::widgets) and [layouting](docs::layouting).

If you are new to SixtyFPS, you might also consider going through our [Walk-through tutorial](https://sixtyfps.io/docs/tutorial/rust).

## How to use:

The user interfaces are described in the `.60` design markup language. There are two ways
of including the design in Rust:

 - The `.60` code is inline in a macro.
 - The `.60` code in external files compiled with `build.rs`

 This markup code is translated to Rust code and each component is turned into a Rust
 struct with functions to instantiated, show or access properties. This documentation
 includes an [example](docs::generated_code::SampleComponent) of how the API looks
 like.

### The .60 code in a macro

This method combines your Rust code with the `.60` design markup in one file, using a macro:

```rust
sixtyfps::sixtyfps!{
    HelloWorld := Window {
        Text {
            text: "hello world";
            color: green;
        }
    }
}
fn main() {
#   return; // Don't run a window in an example
    HelloWorld::new().run();
}
```

### The .60 file in external files compiled with `build.rs`

This method allows you to a separate `.60` file on the file system, which works well if
your design becomes bigger and you split it up across multiple files. You need to use a
so-called [build script](https://doc.rust-lang.org/cargo/reference/build-scripts.html)
to trigger the compilation of the `.60` file.

In your Cargo.toml:

```toml
[package]
...
build = "build.rs"
resolver = "2" # avoid dependency conflicts on some platforms
edition = "2018"

[dependencies]
sixtyfps = "0.1.0"
...

[build-dependencies]
sixtyfps-build = "0.1.0"
```

In the `build.rs` file:

```ignore
fn main() {
    sixtyfps_build::compile("ui/hello.60").unwrap();
}
```

Then in your main file

```ignore
sixtyfps::include_modules!();
fn main() {
    HelloWorld::new().run();
}
```

## Generated components

As of now, only the last component of a .60 source is generated. It is planned to generate all exported components.

The component is generated and re-exported at the location of the [`include_modules!`]  or [`sixtyfps!`] macro.
it consist of a struct of the same name of the component.
For example, if you have `export MyComponent := Window { /*...*/ }` in the .60 file, it will create a `struct MyComponent{ /*...*/ }`.
This documentation contains a documented generated component: [`docs::generated_code::SampleComponent`].

The component is created using the [`fn new() -> Self`](docs::generated_code::SampleComponent::new) function. In addition
the following convenience functions are available through the [`ComponentHandle`] implementation:

  - [`fn clone_strong(&self) -> Self`](docs::generated_code::SampleComponent::clone_strong): to create a strongly referenced clone.
  - [`fn as_weak(&self) -> Weak`](docs::generated_code::SampleComponent::as_weak): to create a [weak](Weak) reference.
  - [`fn show(&self)`](docs::generated_code::SampleComponent::show): to show the window of the component.
  - [`fn hide(&self)`](docs::generated_code::SampleComponent::hide): to hide the window of the component.
  - [`fn run(&self)`](docs::generated_code::SampleComponent::run): a convenience function that first calls `show()`,
    followed by spinning the event loop, and `hide()` when returning from the event loop.

For each top-level property
  - A setter [`fn set_<property_name>(&self, value: <PropertyType>)`](docs::generated_code::SampleComponent::set_counter)
  - A getter [`fn get_<property_name>(&self) -> <PropertyType>`](docs::generated_code::SampleComponent::get_counter)

For each top-level callback
  - [`fn invoke_<callback_name>(&self)`](docs::generated_code::SampleComponent::invoke_hello): to invoke the callback
  - [`fn on_<callback_name>(&self, callback: impl Fn(<CallbackArgs>) + 'static)`](docs::generated_code::SampleComponent::on_hello): to set the callback handler.

After instantiating the component you can call just [`ComponentHandle::run()`] on it, in order to show it and spin the event loop to
render and react to input events. If you want to show multiple components simultaneously, then you can also call just
[`ComponentHandle::show()`] first. When you're ready to enter the event loop, just call [`run_event_loop()`].

The generated component struct act as a handle holding a strong reference (similar to a `Rc`). It does not implement
`Clone` because we want to make explicit if we are cloning a strong reference (with [`ComponentHandle::clone_strong`]),
or a weak reference (with  [`ComponentHandle::as_weak`]). A strong reference should not be captured by the closures
given to a callback, as this would produce a reference loop and leak the component. Instead, the callback function
should capture a weak component.

## Threading and Event-loop

For platform-specific reasons, the event loop must run in the main thread, in most backends, and all the components
must be created in the same thread as the thread the event loop is running or is going to run.

Ideally, you should perform the minimum amount of work in the main thread and delegate the actual logic to another
thread. To communicate from your worker thread to the UI thread, the [`invoke_from_event_loop`] function can be used.

To run a function with a delay or with an interval you can use a [`Timer`].

## Type Mappings

The types used for properties in `.60` design markup each translate to specific types in Rust.
The follow table summarizes the entire mapping:

| `.60` Type | Rust Type | Note |
| --- | --- | --- |
| `int` | `i32` | |
| `float` | `f32` | |
| `string` | [`SharedString`] | A reference-counted string type that can be easily converted to a str reference. |
| `color` | [`Color`] | |
| `brush` | [`Brush`] | |
| `image` | [`Image`] | |
| `physical_length` | `f32` | The unit are physical pixels. |
| `length` | `f32` | At run-time, logical lengths are automatically translated to physical pixels using the device pixel ratio. |
| `duration` | `i64` | At run-time, durations are always represented as signed 64-bit integers with millisecond precision. |
| structure | `struct` of the same name | |
| array | [`ModelHandle`] |  |

For user defined structures in the .60, an extra struct is generated.
For example, if the `.60` contains
```60
export struct MyStruct := {
    foo: int,
    bar: string,
}
```

The following struct would be generated:

```rust
#[derive(Default, Clone, Debug, PartialEq)]
struct MyStruct {
    foo : i32,
    bar: sixtyfps::SharedString,
}
```

*/

#![warn(missing_docs)]
#![deny(unsafe_code)]
#![doc(html_logo_url = "https://sixtyfps.io/resources/logo.drawio.svg")]

pub use sixtyfps_macros::sixtyfps;

pub use sixtyfps_corelib::graphics::{Brush, Color, Image, LoadImageError, RgbaColor};
pub use sixtyfps_corelib::model::{
    Model, ModelHandle, ModelNotify, ModelPeer, StandardListViewItem, VecModel,
};
pub use sixtyfps_corelib::sharedvector::SharedVector;
pub use sixtyfps_corelib::string::SharedString;
pub use sixtyfps_corelib::timers::{Timer, TimerMode};

/// This function can be used to register a custom TrueType font with SixtyFPS,
/// for use with the `font-family` property. The provided slice must be a valid TrueType
/// font.
#[doc(hidden)]
pub fn register_font_from_memory(data: &'static [u8]) -> Result<(), Box<dyn std::error::Error>> {
    sixtyfps_rendering_backend_default::backend().register_font_from_memory(data)
}

/// This function can be used to register a custom TrueType font with SixtyFPS,
/// for use with the `font-family` property. The provided path must refer to a valid TrueType
/// font.
#[doc(hidden)]
pub fn register_font_from_path<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<(), Box<dyn std::error::Error>> {
    sixtyfps_rendering_backend_default::backend().register_font_from_path(path.as_ref())
}

/// internal re_exports used by the macro generated
#[doc(hidden)]
pub mod re_exports {
    pub use const_field_offset::{self, FieldOffsets, PinnedDrop};
    pub use core::iter::FromIterator;
    pub use once_cell::sync::Lazy;
    pub use once_cell::unsync::OnceCell;
    pub use pin_weak::rc::PinWeak;
    pub use sixtyfps_corelib::animations::EasingCurve;
    pub use sixtyfps_corelib::callbacks::Callback;
    pub use sixtyfps_corelib::component::{
        init_component_items, Component, ComponentRefPin, ComponentVTable,
    };
    pub use sixtyfps_corelib::graphics::{
        Brush, GradientStop, Image, ImageInner, LinearGradientBrush, PathArcTo, PathCubicTo,
        PathData, PathElement, PathEvent, PathLineTo, PathMoveTo, PathQuadraticTo, Point, Rect,
        Size,
    };
    pub use sixtyfps_corelib::input::{
        FocusEvent, InputEventResult, KeyEvent, KeyEventResult, KeyboardModifiers, MouseEvent,
    };
    pub use sixtyfps_corelib::item_tree::{
        visit_item_tree, ItemTreeNode, ItemVisitorRefMut, ItemVisitorVTable, TraversalOrder,
        VisitChildrenResult,
    };
    pub use sixtyfps_corelib::items::*;
    pub use sixtyfps_corelib::layout::*;
    pub use sixtyfps_corelib::model::*;
    pub use sixtyfps_corelib::properties::{
        set_state_binding, Property, PropertyTracker, StateInfo,
    };
    pub use sixtyfps_corelib::slice::Slice;
    pub use sixtyfps_corelib::window::ComponentWindow;
    pub use sixtyfps_corelib::Color;
    pub use sixtyfps_corelib::ComponentVTable_static;
    pub use sixtyfps_corelib::SharedString;
    pub use sixtyfps_corelib::SharedVector;
    pub use sixtyfps_rendering_backend_default::native_widgets::*;
    pub use vtable::{self, *};
}

/// Creates a new window to render components in.
#[doc(hidden)]
pub fn create_window() -> re_exports::ComponentWindow {
    sixtyfps_rendering_backend_default::backend().create_window()
}

/// Enters the main event loop. This is necessary in order to receive
/// events from the windowing system in order to render to the screen
/// and react to user input.
pub fn run_event_loop() {
    sixtyfps_rendering_backend_default::backend()
        .run_event_loop(sixtyfps_corelib::backend::EventLoopQuitBehavior::QuitOnLastWindowClosed);
}

/// Schedules the main event loop for termination. This function is meant
/// to be called from callbacks triggered by the UI. After calling the function,
/// it will return immediately and once control is passed back to the event loop,
/// the initial call to [`run_event_loop()`] will return.
pub fn quit_event_loop() {
    sixtyfps_rendering_backend_default::backend().quit_event_loop();
}

/// Adds the specified function to an internal queue, notifies the event loop to wake up.
/// Once woken up, any queued up functors will be invoked.
///
/// This function is thread-safe and can be called from any thread, including the one
/// running the event loop. The provided functors will only be invoked from the thread
/// that started the event loop.
///
/// You can use this to set properties or use any other SixtyFPS APIs from other threads,
/// by collecting the code in a functor and queuing it up for invocation within the event loop.
///
/// # Example
/// ```rust
/// sixtyfps::sixtyfps! { MyApp := Window { property <int> foo; /* ... */ } }
/// let handle = MyApp::new();
/// let handle_weak = handle.as_weak();
/// let thread = std::thread::spawn(move || {
///     // ... Do some computation in the thread
///     let foo = 42;
///      // now forward the data to the main thread using invoke_from_event_loop
///     let handle_copy = handle_weak.clone();
///     sixtyfps::invoke_from_event_loop(move || handle_copy.unwrap().set_foo(foo));
/// });
/// # thread.join(); return; // don't run the event loop in examples
/// handle.run();
/// ```
pub fn invoke_from_event_loop(func: impl FnOnce() + Send + 'static) {
    sixtyfps_rendering_backend_default::backend().post_event(Box::new(func))
}

/// This trait describes the common public API of a strongly referenced SixtyFPS component.
/// It allows creating strongly-referenced clones, a conversion into/ a weak pointer as well
/// as other convenience functions.
///
/// This trait is implemented by the [generated component](mod@crate#generated-components)
pub trait ComponentHandle {
    /// The type of the generated component.
    #[doc(hidden)]
    type Inner;
    /// Returns a new weak pointer.
    fn as_weak(&self) -> Weak<Self>
    where
        Self: Sized;

    /// Returns a clone of this handle that's a strong reference.
    fn clone_strong(&self) -> Self;

    /// Internal function used when upgrading a weak reference to a strong one.
    #[doc(hidden)]
    fn from_inner(_: vtable::VRc<re_exports::ComponentVTable, Self::Inner>) -> Self;

    /// Marks the window of this component to be shown on the screen. This registers
    /// the window with the windowing system. In order to react to events from the windowing system,
    /// such as draw requests or mouse/touch input, it is still necessary to spin the event loop,
    /// using [`crate::run_event_loop`].
    fn show(&self);

    /// Marks the window of this component to be hidden on the screen. This de-registers
    /// the window from the windowing system and it will not receive any further events.
    fn hide(&self);

    /// This is a convenience function that first calls [`Self::show`], followed by [`crate::run_event_loop()`]
    /// and [`Self::hide`].
    fn run(&self);
}

mod weak_handle {

    use super::*;

    /// Struct that's used to hold weak references of [SixtyFPS component](mod@crate#generated-components)
    ///
    /// In order to create a Weak, you should use [`ComponentHandle::as_weak`].
    ///
    /// Strong references should not be captured by the functions given to a lambda,
    /// as this would produce a reference loop and leak the component.
    /// Instead, the callback function should capture a weak component.
    ///
    /// The Weak component also implement `Send` and can be send to another thread.
    /// but the upgrade function will only return a valid component from the same thread
    /// as the one it has been created from.
    /// This is useful to use with [`invoke_from_event_loop()`].
    pub struct Weak<T: ComponentHandle> {
        inner: vtable::VWeak<re_exports::ComponentVTable, T::Inner>,
        thread: std::thread::ThreadId,
    }

    impl<T: ComponentHandle> Clone for Weak<T> {
        fn clone(&self) -> Self {
            Self { inner: self.inner.clone(), thread: self.thread.clone() }
        }
    }

    impl<T: ComponentHandle> Weak<T> {
        #[doc(hidden)]
        pub fn new(rc: &vtable::VRc<re_exports::ComponentVTable, T::Inner>) -> Self {
            Self { inner: vtable::VRc::downgrade(&rc), thread: std::thread::current().id() }
        }

        /// Returns a new strongly referenced component if some other instance still
        /// holds a strong reference. Otherwise, returns None.
        ///
        /// This also returns None if the current thread is not the thread that created
        /// the component
        pub fn upgrade(&self) -> Option<T>
        where
            T: ComponentHandle,
        {
            if std::thread::current().id() != self.thread {
                return None;
            }
            self.inner.upgrade().map(|inner| T::from_inner(inner))
        }

        /// Convenience function that returns a new strongly referenced component if
        /// some other instance still holds a strong reference and the current thread
        /// is the thread that created this component.
        /// Otherwise, this function panics.
        pub fn unwrap(&self) -> T {
            self.upgrade().unwrap()
        }
    }

    // Safety: we make sure in upgrade that the thread is the proper one,
    // and the VWeak only use atomic pointer so it is safe to clone and drop in another thread
    #[allow(unsafe_code)]
    unsafe impl<T: ComponentHandle> Send for Weak<T> {}
}

pub use weak_handle::*;

/// This module contains functions useful for unit tests
pub mod testing {
    use core::cell::Cell;
    thread_local!(static KEYBOARD_MODIFIERS : Cell<crate::re_exports::KeyboardModifiers> = Default::default());

    use super::ComponentHandle;

    /// This trait gives access to the underlying Window of a component for the
    /// purposes of testing.
    pub trait HasWindow {
        /// Returns a reference to the component's window.
        fn component_window(&self) -> &super::re_exports::ComponentWindow;
    }

    pub use sixtyfps_corelib::tests::sixtyfps_mock_elapsed_time as mock_elapsed_time;

    /// Simulate a mouse click
    pub fn send_mouse_click<
        X: vtable::HasStaticVTable<sixtyfps_corelib::component::ComponentVTable> + HasWindow + 'static,
        Component: Into<vtable::VRc<sixtyfps_corelib::component::ComponentVTable, X>> + ComponentHandle,
    >(
        component: &Component,
        x: f32,
        y: f32,
    ) {
        let rc = component.clone_strong().into();
        let dyn_rc = vtable::VRc::into_dyn(rc.clone());
        sixtyfps_corelib::tests::sixtyfps_send_mouse_click(&dyn_rc, x, y, rc.component_window());
    }

    /// Simulate a change in keyboard modifiers being pressed
    pub fn set_current_keyboard_modifiers<
        X: vtable::HasStaticVTable<sixtyfps_corelib::component::ComponentVTable> + HasWindow,
        Component: Into<vtable::VRc<sixtyfps_corelib::component::ComponentVTable, X>> + ComponentHandle,
    >(
        _component: &Component,
        modifiers: crate::re_exports::KeyboardModifiers,
    ) {
        KEYBOARD_MODIFIERS.with(|x| x.set(modifiers))
    }

    /// Simulate entering a sequence of ascii characters key by key.
    pub fn send_keyboard_string_sequence<
        X: vtable::HasStaticVTable<sixtyfps_corelib::component::ComponentVTable> + HasWindow,
        Component: Into<vtable::VRc<sixtyfps_corelib::component::ComponentVTable, X>> + ComponentHandle,
    >(
        component: &Component,
        sequence: &str,
    ) {
        let component = component.clone_strong().into();
        sixtyfps_corelib::tests::send_keyboard_string_sequence(
            &super::SharedString::from(sequence),
            KEYBOARD_MODIFIERS.with(|x| x.get()),
            component.component_window(),
        )
    }

    /// Applies the specified scale factor to the window that's associated with the given component.
    /// This overrides the value provided by the windowing system.
    pub fn set_window_scale_factor<
        X: vtable::HasStaticVTable<sixtyfps_corelib::component::ComponentVTable> + HasWindow,
        Component: Into<vtable::VRc<sixtyfps_corelib::component::ComponentVTable, X>> + ComponentHandle,
    >(
        component: &Component,
        factor: f32,
    ) {
        let component = component.clone_strong().into();
        component.component_window().set_scale_factor(factor)
    }
}

/// Include the code generated with the sixtyfps-build crate from the build script. After calling `sixtyfps_build::compile`
/// in your `build.rs` build script, the use of this macro includes the generated Rust code and makes the exported types
/// available for you to instantiate.
///
/// Check the documentation of the `sixtyfps-build` crate for more information.
#[macro_export]
macro_rules! include_modules {
    () => {
        include!(env!("SIXTYFPS_INCLUDE_GENERATED"));
    };
}

/// Helper type that helps checking that the generated code is generated for the right version
#[doc(hidden)]
#[allow(non_camel_case_types)]
pub struct VersionCheck_0_1_0;

#[cfg(doctest)]
mod compile_fail_tests;

#[cfg(all(doc, nightly))]
pub mod docs;
