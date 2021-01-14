/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2020 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2020 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*! Module handling mouse events
*/
#![warn(missing_docs)]

use crate::component::ComponentRc;
use crate::graphics::Point;
use crate::item_tree::ItemVisitorResult;
use crate::items::{ItemRc, ItemRef, ItemWeak};
use crate::Property;
use const_field_offset::FieldOffsets;
use euclid::default::Vector2D;
use sixtyfps_corelib_macros::*;
use std::convert::TryFrom;
use std::pin::Pin;
use std::rc::Rc;

/// The type of a MouseEvent
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseEventType {
    /// The mouse was pressed
    MousePressed,
    /// The mouse was relased
    MouseReleased,
    /// The mouse position has changed
    MouseMoved,
    /// The mouse exited the item or component
    MouseExit,
}

/// Structur representing a mouse event
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    /// The position of the cursor
    pub pos: Point,
    /// The action performed (pressed/released/moced)
    pub what: MouseEventType,
}

/// This value is returned by the input handler of a component
/// to notify the run-time about how the event was handled and
/// what the next steps are.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum InputEventResult {
    /// The event was accepted. This may result in additional events, for example
    /// accepting a mouse move will result in a MouseExit event later.
    EventAccepted,
    /// The event was ignored.
    EventIgnored,
    /* /// Same as grab, but continue forwarding the event to children.
    /// If a child grab the mouse, the grabber will be stored in the item itself.
    /// Only item that have grabbed storage can return this.
    /// The new_grabber is a reference to a usize to store thenext grabber
    TentativeGrab {
        new_grabber: &'a Cell<usize>,
    },
    /// While we have a TentaztiveGrab
    Forward {
        to: usize,
    },*/
    /// All further mouse event need to be sent to this item or component
    GrabMouse,
    /// One must send an MouseExit when the mouse leave this item
    ObserveHover,
}

impl Default for InputEventResult {
    fn default() -> Self {
        Self::EventIgnored
    }
}

/// A key code is a symbolic name for a key on a keyboard. Depending on the
/// key mappings, different keys may produce different key codes.
/// Key codes are typically produced when pressing or releasing a key.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, MappedKeyCode)]
#[allow(missing_docs)]
pub enum KeyCode {
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Snapshot,
    Scroll,
    Pause,
    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,
    Left,
    Up,
    Right,
    Down,
    Back,
    Return,
    Space,
    Compose,
    Caret,
    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    AbntC1,
    AbntC2,
    NumpadAdd,
    Apostrophe,
    Apps,
    Asterisk,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    NumpadDecimal,
    NumpadDivide,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    NumpadMultiply,
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    OEM102,
    Period,
    PlayPause,
    Plus,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    NumpadSubtract,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
    Copy,
    Paste,
    Cut,
}

impl TryFrom<char> for KeyCode {
    type Error = ();

    fn try_from(value: char) -> Result<Self, Self::Error> {
        Ok(match value {
            'a' => Self::A,
            'b' => Self::B,
            'c' => Self::C,
            'd' => Self::D,
            'e' => Self::E,
            'f' => Self::F,
            'g' => Self::G,
            'h' => Self::H,
            'i' => Self::I,
            'j' => Self::J,
            'k' => Self::K,
            'l' => Self::L,
            'm' => Self::M,
            'n' => Self::N,
            'o' => Self::O,
            'p' => Self::P,
            'q' => Self::Q,
            'r' => Self::R,
            's' => Self::S,
            't' => Self::T,
            'u' => Self::U,
            'v' => Self::V,
            'w' => Self::W,
            'x' => Self::X,
            'y' => Self::Y,
            'z' => Self::Z,
            '1' => Self::Key1,
            '2' => Self::Key2,
            '3' => Self::Key3,
            '4' => Self::Key4,
            '5' => Self::Key5,
            '6' => Self::Key6,
            '7' => Self::Key7,
            '8' => Self::Key8,
            '9' => Self::Key9,
            _ => return Err(()),
        })
    }
}

/// KeyboardModifiers wraps a u32 that reserves a single bit for each
/// possible modifier key on a keyboard, such as Shift, Control, etc.
///
/// On macOS, the command key is mapped to the logo modifier.
///
/// On Windows, the windows key is mapped to the logo modifier.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct KeyboardModifiers(u32);
/// KeyboardModifier wraps a u32 that has a single bit set to represent
/// a modifier key such as shift on a keyboard. Convenience constants such as
/// [`NO_MODIFIER`], [`SHIFT_MODIFIER`], [`CONTROL_MODIFIER`], [`ALT_MODIFIER`]
/// and [`LOGO_MODIFIER`] are provided.
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct KeyboardModifier(u32);
/// Convenience constant that indicates no modifier key being pressed on a keyboard.
pub const NO_MODIFIER: KeyboardModifier = KeyboardModifier(0);
/// Convenience constant that indicates the shift key being pressed on a keyboard.
pub const SHIFT_MODIFIER: KeyboardModifier =
    KeyboardModifier(winit::event::ModifiersState::SHIFT.bits());
/// Convenience constant that indicates the control key being pressed on a keyboard.
pub const CONTROL_MODIFIER: KeyboardModifier =
    KeyboardModifier(winit::event::ModifiersState::CTRL.bits());
/// Convenience constant that indicates the control key being pressed on a keyboard.
pub const ALT_MODIFIER: KeyboardModifier =
    KeyboardModifier(winit::event::ModifiersState::ALT.bits());
/// Convenience constant that on macOS indicates the command key and on Windows the
/// windows key being pressed on a keyboard.
pub const LOGO_MODIFIER: KeyboardModifier =
    KeyboardModifier(winit::event::ModifiersState::LOGO.bits());

/// Convenience constant that is used to detect copy & paste related shortcuts, where
/// on macOS the modifier is the command key (aka LOGO_MODIFIER) and on Linux and Windows
/// it is control.
pub const COPY_PASTE_MODIFIER: KeyboardModifier =
    if cfg!(target_os = "macos") { LOGO_MODIFIER } else { CONTROL_MODIFIER };

impl KeyboardModifiers {
    /// Returns true if this set of keyboard modifiers includes the given modifier; false otherwise.
    ///
    /// Arguments:
    /// * `modifier`: The keyboard modifier to test for, usually one of the provided convenience
    ///               constants such as [`SHIFT_MODIFIER`].
    pub fn test(&self, modifier: KeyboardModifier) -> bool {
        self.0 & modifier.0 != 0
    }

    /// Returns true if this set of keyboard modifiers consists of exactly the one specified
    /// modifier; false otherwise.
    ///
    /// Arguments:
    /// * `modifier`: The only modifier that is allowed to be in this modifier set, in order
    //                for this function to return true;
    pub fn test_exclusive(&self, modifier: KeyboardModifier) -> bool {
        self.0 == modifier.0
    }

    /// Returns true if the shift key is part of this set of keyboard modifiers.
    pub fn shift(&self) -> bool {
        self.test(SHIFT_MODIFIER)
    }

    /// Returns true if the control key is part of this set of keyboard modifiers.
    pub fn control(&self) -> bool {
        self.test(CONTROL_MODIFIER)
    }

    /// Returns true if the alt key is part of this set of keyboard modifiers.
    pub fn alt(&self) -> bool {
        self.test(ALT_MODIFIER)
    }

    /// Returns true if on macOS the command key and on Windows the Windows key is part of this
    /// set of keyboard modifiers.
    pub fn logo(&self) -> bool {
        self.test(LOGO_MODIFIER)
    }
}

impl Default for KeyboardModifiers {
    fn default() -> Self {
        Self(NO_MODIFIER.0)
    }
}

impl From<winit::event::ModifiersState> for KeyboardModifiers {
    fn from(state: winit::event::ModifiersState) -> Self {
        Self(state.bits())
    }
}

impl From<KeyboardModifier> for KeyboardModifiers {
    fn from(modifier: KeyboardModifier) -> Self {
        Self(modifier.0)
    }
}

impl core::ops::BitOrAssign<KeyboardModifier> for KeyboardModifiers {
    fn bitor_assign(&mut self, rhs: KeyboardModifier) {
        self.0 |= rhs.0;
    }
}

/// Represents a key event sent by the windowing system.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub enum KeyEvent {
    /// A key on a keyboard was pressed.
    KeyPressed {
        /// The key code of the pressed key.
        code: KeyCode,
        /// The keyboard modifiers active at the time of the key press event.
        modifiers: KeyboardModifiers,
    },
    /// A key on a keyboard was released.
    KeyReleased {
        /// The key code of the released key.
        code: KeyCode,
        /// The keyboard modifiers active at the time of the key release event.
        modifiers: KeyboardModifiers,
    },
    /// A key on a keyboard was released that results in
    /// a character that's suitable for text input.
    CharacterInput {
        /// The u32 is a unicode scalar value that is safe to convert to char.
        unicode_scalar: u32,
        /// The keyboard modifiers active at the time of the char input event.
        modifiers: KeyboardModifiers,
    },
}

impl TryFrom<(&winit::event::KeyboardInput, KeyboardModifiers)> for KeyEvent {
    type Error = ();

    fn try_from(
        input: (&winit::event::KeyboardInput, KeyboardModifiers),
    ) -> Result<Self, Self::Error> {
        let key_code = match input.0.virtual_keycode {
            Some(code) => code.into(),
            None => return Err(()),
        };
        Ok(match input.0.state {
            winit::event::ElementState::Pressed => {
                KeyEvent::KeyPressed { code: key_code, modifiers: input.1 }
            }
            winit::event::ElementState::Released => {
                KeyEvent::KeyReleased { code: key_code, modifiers: input.1 }
            }
        })
    }
}

/// Represents how an item's key_event handler dealt with a key event.
/// An accepted event results in no further event propagation.
#[repr(C)]
pub enum KeyEventResult {
    /// The event was handled.
    EventAccepted,
    /// The event was not handled and should be sent to other items.
    EventIgnored,
}

/// This event is sent to a component and items when they receive or loose
/// the keyboard focus.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub enum FocusEvent {
    /// This event is sent when an item receives the focus.
    FocusIn,
    /// This event is sent when an item looses the focus.
    FocusOut,
    /// This event is sent when the window receives the keyboard focus.
    WindowReceivedFocus,
    /// This event is sent when the window looses the keyboard focus.
    WindowLostFocus,
}

/// The state which a window should hold for the mouse input
#[derive(Default)]
pub struct MouseInputState {
    /// The stack of item which contain the mouse cursor (or grab)
    item_stack: Vec<ItemWeak>,
    /// true if the top item of the stack has the mouse grab
    grabbed: bool,
}

/// Process the `mouse_event` on the `component`, the `mouse_grabber_stack` is the prebious stack
/// of mouse grabber.
/// Returns a new mouse grabber stack.
pub fn process_mouse_input(
    component: ComponentRc,
    mouse_event: MouseEvent,
    window: &crate::window::ComponentWindow,
    mouse_input_state: MouseInputState,
) -> MouseInputState {
    'grab: loop {
        if !mouse_input_state.grabbed || mouse_input_state.item_stack.is_empty() {
            break 'grab;
        };
        let mut event = mouse_event.clone();
        for it in mouse_input_state.item_stack.iter() {
            let item = if let Some(item) = it.upgrade() { item } else { break 'grab };
            let g = item.borrow().as_ref().geometry();
            event.pos -= g.origin.to_vector();
        }
        let grabber = mouse_input_state.item_stack.last().unwrap().upgrade().unwrap();
        return match grabber.borrow().as_ref().input_event(event, window, &grabber) {
            InputEventResult::GrabMouse => mouse_input_state,
            _ => Default::default(),
        };
    }

    // Send the Exit event.
    // FIXME: we should send the exit event only if they no longer have the mouse
    let mut pos = mouse_event.pos;
    for it in mouse_input_state.item_stack.iter() {
        let item = if let Some(item) = it.upgrade() { item } else { break };
        let g = item.borrow().as_ref().geometry();
        pos -= g.origin.to_vector();
        item.borrow().as_ref().input_event(
            MouseEvent { pos, what: MouseEventType::MouseExit },
            window,
            &item,
        );
    }

    let mut result = MouseInputState::default();
    type State = (Vector2D<f32>, Vec<ItemWeak>);
    crate::item_tree::visit_items(
        &component,
        crate::item_tree::TraversalOrder::FrontToBack,
        |comp_rc: &ComponentRc,
         item: core::pin::Pin<ItemRef>,
         item_index: usize,
         (offset, mouse_grabber_stack): &State|
         -> ItemVisitorResult<State> {
            let item_rc = ItemRc::new(comp_rc.clone(), item_index);

            let geom = item.as_ref().geometry();
            let geom = geom.translate(*offset);

            if geom.contains(mouse_event.pos) {
                let mut event2 = mouse_event.clone();
                event2.pos -= geom.origin.to_vector();
                match item.as_ref().input_event(event2, window, &item_rc) {
                    InputEventResult::EventAccepted => {
                        result.item_stack = mouse_grabber_stack.clone();
                        result.item_stack.push(item_rc.downgrade());
                        result.grabbed = false;
                        return ItemVisitorResult::Abort;
                    }
                    InputEventResult::EventIgnored => (),
                    InputEventResult::GrabMouse => {
                        result.item_stack = mouse_grabber_stack.clone();
                        result.item_stack.push(item_rc.downgrade());
                        result.grabbed = true;
                        return ItemVisitorResult::Abort;
                    }
                    InputEventResult::ObserveHover => {
                        result.item_stack = mouse_grabber_stack.clone();
                        result.item_stack.push(item_rc.downgrade());
                        result.grabbed = false;
                    }
                };
            }

            let mut mouse_grabber_stack = mouse_grabber_stack.clone();
            mouse_grabber_stack.push(item_rc.downgrade());
            ItemVisitorResult::Continue((geom.origin.to_vector(), mouse_grabber_stack))
        },
        (Vector2D::new(0., 0.), Vec::new()),
    );
    result
}

/// The TextCursorBlinker takes care of providing a toggled boolean property
/// that can be used to animate a blinking cursor. It's typically stored in the
/// Window using a Weak and set_binding() can be used to set up a binding on a given
/// property that'll keep it up-to-date. That binding keeps a strong reference to the
/// blinker. If the underlying item that uses it goes away, the binding goes away and
/// so does the blinker.
#[derive(FieldOffsets)]
#[repr(C)]
#[pin]
pub struct TextCursorBlinker {
    cursor_visible: Property<bool>,
    cursor_blink_timer: crate::timers::Timer,
}

impl TextCursorBlinker {
    /// Creates a new instance, wrapped in a Pin<Rc<_>> because the boolean property
    /// the blinker properties uses the property system that requires pinning.
    pub fn new() -> Pin<Rc<Self>> {
        Rc::pin(Self {
            cursor_visible: Property::new(true),
            cursor_blink_timer: Default::default(),
        })
    }

    /// Sets a binding on the provided property that will ensure that the property value
    /// is true when the cursor should be shown and false if not.
    pub fn set_binding(instance: Pin<Rc<TextCursorBlinker>>, prop: &Property<bool>) {
        instance.as_ref().cursor_visible.set(true);
        // Re-start timer, in case.
        Self::start(&instance);
        prop.set_binding(move || {
            TextCursorBlinker::FIELD_OFFSETS.cursor_visible.apply_pin(instance.as_ref()).get()
        });
    }

    /// Starts the blinking cursor timer that will toggle the cursor and update all bindings that
    /// were installed on properties with set_binding call.
    pub fn start(self: &Pin<Rc<Self>>) {
        if self.cursor_blink_timer.running() {
            self.cursor_blink_timer.restart();
        } else {
            let toggle_cursor = {
                let weak_blinker = pin_weak::rc::PinWeak::downgrade(self.clone());
                move || {
                    if let Some(blinker) = weak_blinker.upgrade() {
                        let visible = TextCursorBlinker::FIELD_OFFSETS
                            .cursor_visible
                            .apply_pin(blinker.as_ref())
                            .get();
                        blinker.cursor_visible.set(!visible);
                    }
                }
            };
            self.cursor_blink_timer.start(
                crate::timers::TimerMode::Repeated,
                std::time::Duration::from_millis(500),
                toggle_cursor,
            );
        }
    }

    /// Stops the blinking cursor timer. This is usually used for example when the window that contains
    /// text editable elements looses the focus or is hidden.
    pub fn stop(&self) {
        self.cursor_blink_timer.stop()
    }
}
