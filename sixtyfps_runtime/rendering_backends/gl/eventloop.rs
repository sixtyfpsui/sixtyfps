/* LICENSE BEGIN
    This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#![warn(missing_docs)]
/*!
    This module contains the event loop implementation using winit, as well as the
    [PlatformWindow] trait used by the generated code and the run-time to change
    aspects of windows on the screen.
*/
use sixtyfps_corelib as corelib;

use corelib::graphics::Point;
use corelib::input::{InternalKeyCode, KeyEvent, KeyEventType, KeyboardModifiers, MouseEvent};
use corelib::window::*;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

#[cfg(not(target_arch = "wasm32"))]
use winit::platform::run_return::EventLoopExtRunReturn;

struct NotRunningEventLoop {
    instance: winit::event_loop::EventLoop<CustomEvent>,
    event_loop_proxy: winit::event_loop::EventLoopProxy<CustomEvent>,
}

impl NotRunningEventLoop {
    fn new() -> Self {
        let instance = winit::event_loop::EventLoop::with_user_event();
        let event_loop_proxy = instance.create_proxy();
        Self { instance, event_loop_proxy }
    }
}

struct RunningEventLoop<'a> {
    event_loop_target: &'a winit::event_loop::EventLoopWindowTarget<CustomEvent>,
    event_loop_proxy: &'a winit::event_loop::EventLoopProxy<CustomEvent>,
}

pub(crate) trait EventLoopInterface {
    fn event_loop_target(&self) -> &winit::event_loop::EventLoopWindowTarget<CustomEvent>;
    fn event_loop_proxy(&self) -> &winit::event_loop::EventLoopProxy<CustomEvent>;
}

impl EventLoopInterface for NotRunningEventLoop {
    fn event_loop_target(&self) -> &winit::event_loop::EventLoopWindowTarget<CustomEvent> {
        &*self.instance
    }

    fn event_loop_proxy(&self) -> &winit::event_loop::EventLoopProxy<CustomEvent> {
        &self.event_loop_proxy
    }
}

impl<'a> EventLoopInterface for RunningEventLoop<'a> {
    fn event_loop_target(&self) -> &winit::event_loop::EventLoopWindowTarget<CustomEvent> {
        self.event_loop_target
    }

    fn event_loop_proxy(&self) -> &winit::event_loop::EventLoopProxy<CustomEvent> {
        &self.event_loop_proxy
    }
}

thread_local! {
    static ALL_WINDOWS: RefCell<std::collections::HashMap<winit::window::WindowId, Weak<crate::graphics_window::GraphicsWindow>>> = RefCell::new(std::collections::HashMap::new());
    static MAYBE_LOOP_INSTANCE: RefCell<Option<NotRunningEventLoop>> = RefCell::new(Some(NotRunningEventLoop::new()));
}

scoped_tls_hkt::scoped_thread_local!(static CURRENT_WINDOW_TARGET : for<'a> &'a RunningEventLoop<'a>);

#[cfg(not(target_arch = "wasm32"))]
pub(crate) enum GlobalEventLoopProxyOrEventQueue {
    Proxy(winit::event_loop::EventLoopProxy<CustomEvent>),
    Queue(Vec<CustomEvent>),
}

#[cfg(not(target_arch = "wasm32"))]
impl GlobalEventLoopProxyOrEventQueue {
    pub(crate) fn send_event(&mut self, event: CustomEvent) {
        match self {
            GlobalEventLoopProxyOrEventQueue::Proxy(proxy) => proxy.send_event(event).ok().unwrap(),
            GlobalEventLoopProxyOrEventQueue::Queue(queue) => {
                queue.push(event);
            }
        };
    }

    fn set_proxy(&mut self, proxy: winit::event_loop::EventLoopProxy<CustomEvent>) {
        match self {
            GlobalEventLoopProxyOrEventQueue::Proxy(_) => {}
            GlobalEventLoopProxyOrEventQueue::Queue(queue) => {
                std::mem::take(queue)
                    .into_iter()
                    .for_each(|event| proxy.send_event(event).ok().unwrap());
                *self = GlobalEventLoopProxyOrEventQueue::Proxy(proxy);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for GlobalEventLoopProxyOrEventQueue {
    fn default() -> Self {
        Self::Queue(Vec::new())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) static GLOBAL_PROXY: once_cell::sync::OnceCell<
    std::sync::Mutex<GlobalEventLoopProxyOrEventQueue>,
> = once_cell::sync::OnceCell::new();

pub(crate) fn with_window_target<T>(callback: impl FnOnce(&dyn EventLoopInterface) -> T) -> T {
    if CURRENT_WINDOW_TARGET.is_set() {
        CURRENT_WINDOW_TARGET.with(|current_target| callback(current_target))
    } else {
        MAYBE_LOOP_INSTANCE.with(|loop_instance| {
            if loop_instance.borrow().is_none() {
                *loop_instance.borrow_mut() = Some(NotRunningEventLoop::new());
            }
            callback(loop_instance.borrow().as_ref().unwrap())
        })
    }
}

pub fn register_window(
    id: winit::window::WindowId,
    window: Rc<crate::graphics_window::GraphicsWindow>,
) {
    ALL_WINDOWS.with(|windows| {
        windows.borrow_mut().insert(id, Rc::downgrade(&window));
    })
}

pub fn unregister_window(id: winit::window::WindowId) {
    ALL_WINDOWS.with(|windows| {
        windows.borrow_mut().remove(&id);
    })
}

/// This enum captures run-time specific events that can be dispatched to the event loop in
/// addition to the winit events.
pub enum CustomEvent {
    /// Request for the event loop to wake up and redraw all windows. This is used on the
    /// web for example to request an animation frame.
    #[cfg(target_arch = "wasm32")]
    RedrawAllWindows,
    UpdateWindowProperties(winit::window::WindowId),
    UserEvent(Box<dyn FnOnce() + Send>),
    Exit,
}

impl std::fmt::Debug for CustomEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(target_arch = "wasm32")]
            Self::RedrawAllWindows => write!(f, "RedrawAllWindows"),
            Self::UpdateWindowProperties(e) => write!(f, "UpdateWindowProperties({:?})", e),
            Self::UserEvent(_) => write!(f, "UserEvent"),
            Self::Exit => write!(f, "Exit"),
        }
    }
}

/// Runs the event loop and renders the items in the provided `component` in its
/// own window.
#[allow(unused_mut)] // mut need changes for wasm
pub fn run(quit_behavior: sixtyfps_corelib::backend::EventLoopQuitBehavior) {
    use winit::event::Event;
    use winit::event_loop::{ControlFlow, EventLoopWindowTarget};

    let not_running_loop_instance = MAYBE_LOOP_INSTANCE.with(|loop_instance| {
        loop_instance.borrow_mut().take().unwrap_or_else(|| NotRunningEventLoop::new())
    });

    let event_loop_proxy = not_running_loop_instance.event_loop_proxy;
    #[cfg(not(target_arch = "wasm32"))]
    {
        GLOBAL_PROXY
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .set_proxy(event_loop_proxy.clone());
    }

    let mut winit_loop = not_running_loop_instance.instance;

    // last seen cursor position, (physical coordinate)
    let mut cursor_pos = Point::default();
    let mut pressed = false;
    let mut run_fn = move |event: Event<CustomEvent>,
                           event_loop_target: &EventLoopWindowTarget<CustomEvent>,
                           control_flow: &mut ControlFlow| {
        let running_instance =
            RunningEventLoop { event_loop_target, event_loop_proxy: &event_loop_proxy };
        CURRENT_WINDOW_TARGET.set(&running_instance, || {
            *control_flow = ControlFlow::Wait;

            match event {
                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::CloseRequested,
                    window_id,
                } => {
                    if let Some(window_rc) = ALL_WINDOWS.with(|windows| {
                        windows.borrow().get(&window_id).and_then(|weakref| weakref.upgrade())
                    }) {
                        window_rc.hide();
                    }
                    match quit_behavior {
                        corelib::backend::EventLoopQuitBehavior::QuitOnLastWindowClosed => {
                            let window_count = ALL_WINDOWS.with(|windows| windows.borrow().len());
                            if window_count == 0 {
                                *control_flow = winit::event_loop::ControlFlow::Exit;
                            }
                        }
                        corelib::backend::EventLoopQuitBehavior::QuitOnlyExplicitly => {}
                    }
                }
                winit::event::Event::RedrawRequested(id) => {
                    corelib::animations::update_animations();
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&id).map(|weakref| weakref.upgrade())
                        {
                            window.draw();
                        }
                    });
                }
                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::Resized(size),
                    window_id,
                } => {
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            window.refresh_window_scale_factor();
                            let size = size.to_logical(window.scale_factor() as f64);
                            window.set_geometry(size.width, size.height);
                        }
                    });
                }
                winit::event::Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::ScaleFactorChanged {
                            scale_factor,
                            new_inner_size: size,
                        },
                    window_id,
                } => {
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            let size = size.to_logical(scale_factor);
                            window.set_geometry(size.width, size.height);
                            window.set_scale_factor(scale_factor as f32);
                        }
                    });
                }

                winit::event::Event::WindowEvent {
                    ref window_id,
                    event: winit::event::WindowEvent::MouseInput { state, .. },
                    ..
                } => {
                    corelib::animations::update_animations();
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            let ev = match state {
                                winit::event::ElementState::Pressed => {
                                    pressed = true;
                                    MouseEvent::MousePressed { pos: cursor_pos }
                                }
                                winit::event::ElementState::Released => {
                                    pressed = false;
                                    MouseEvent::MouseReleased { pos: cursor_pos }
                                }
                            };
                            window.clone().process_mouse_input(ev);
                        }
                    });
                }
                winit::event::Event::WindowEvent {
                    ref window_id,
                    event: winit::event::WindowEvent::Touch(touch),
                    ..
                } => {
                    corelib::animations::update_animations();
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            let location = touch.location.to_logical(window.scale_factor() as f64);
                            let pos = euclid::point2(location.x, location.y);
                            let ev = match touch.phase {
                                winit::event::TouchPhase::Started => {
                                    pressed = true;
                                    MouseEvent::MousePressed { pos }
                                }
                                winit::event::TouchPhase::Ended
                                | winit::event::TouchPhase::Cancelled => {
                                    pressed = false;
                                    MouseEvent::MouseReleased { pos }
                                }
                                winit::event::TouchPhase::Moved => MouseEvent::MouseMoved { pos },
                            };
                            window.clone().process_mouse_input(ev);
                        }
                    });
                }
                winit::event::Event::WindowEvent {
                    window_id,
                    event: winit::event::WindowEvent::CursorMoved { position, .. },
                    ..
                } => {
                    corelib::animations::update_animations();
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            let position = position.to_logical(window.scale_factor() as f64);
                            cursor_pos = euclid::point2(position.x, position.y);
                            window
                                .clone()
                                .process_mouse_input(MouseEvent::MouseMoved { pos: cursor_pos });
                        }
                    });
                }
                // On the html canvas, we don't get the mouse move or release event when outside the canvas. So we have no choice but canceling the event
                #[cfg(target_arch = "wasm32")]
                winit::event::Event::WindowEvent {
                    ref window_id,
                    event: winit::event::WindowEvent::CursorLeft { .. },
                    ..
                } => {
                    if pressed {
                        corelib::animations::update_animations();
                        ALL_WINDOWS.with(|windows| {
                            if let Some(Some(window)) =
                                windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                            {
                                pressed = false;
                                window.clone().process_mouse_input(MouseEvent::MouseExit);
                            }
                        });
                    }
                }
                winit::event::Event::WindowEvent {
                    ref window_id,
                    event: winit::event::WindowEvent::MouseWheel { delta, .. },
                    ..
                } => {
                    corelib::animations::update_animations();
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            let delta = match delta {
                                winit::event::MouseScrollDelta::LineDelta(lx, ly) => {
                                    euclid::point2(lx * 60., ly * 60.)
                                }
                                winit::event::MouseScrollDelta::PixelDelta(d) => {
                                    let d = d.to_logical(window.scale_factor() as f64);
                                    euclid::point2(d.x, d.y)
                                }
                            };
                            window.clone().process_mouse_input(MouseEvent::MouseWheel {
                                pos: cursor_pos,
                                delta,
                            });
                        }
                    });
                }
                winit::event::Event::WindowEvent {
                    ref window_id,
                    event: winit::event::WindowEvent::KeyboardInput { ref input, .. },
                } => {
                    corelib::animations::update_animations();
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            if let Some(key_code) =
                                input.virtual_keycode.and_then(|virtual_keycode| {
                                    match virtual_keycode {
                                        winit::event::VirtualKeyCode::Left => {
                                            Some(InternalKeyCode::Left)
                                        }
                                        winit::event::VirtualKeyCode::Right => {
                                            Some(InternalKeyCode::Right)
                                        }
                                        winit::event::VirtualKeyCode::Home => {
                                            Some(InternalKeyCode::Home)
                                        }
                                        winit::event::VirtualKeyCode::End => {
                                            Some(InternalKeyCode::End)
                                        }
                                        winit::event::VirtualKeyCode::Back => {
                                            Some(InternalKeyCode::Back)
                                        }
                                        winit::event::VirtualKeyCode::Delete => {
                                            Some(InternalKeyCode::Delete)
                                        }
                                        winit::event::VirtualKeyCode::Return => {
                                            Some(InternalKeyCode::Return)
                                        }
                                        winit::event::VirtualKeyCode::Escape => {
                                            Some(InternalKeyCode::Escape)
                                        }
                                        _ => None,
                                    }
                                })
                            {
                                let text = key_code.encode_to_string();
                                let event = KeyEvent {
                                    event_type: match input.state {
                                        winit::event::ElementState::Pressed => {
                                            KeyEventType::KeyPressed
                                        }
                                        winit::event::ElementState::Released => {
                                            KeyEventType::KeyReleased
                                        }
                                    },
                                    text,
                                    modifiers: window.current_keyboard_modifiers(),
                                };
                                window.self_weak.upgrade().unwrap().process_key_input(&event);
                            };
                        }
                    });
                }
                winit::event::Event::WindowEvent {
                    ref window_id,
                    event: winit::event::WindowEvent::ReceivedCharacter(ch),
                } => {
                    if !ch.is_control() {
                        corelib::animations::update_animations();
                        ALL_WINDOWS.with(|windows| {
                            if let Some(Some(window)) =
                                windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                            {
                                let modifiers = window.current_keyboard_modifiers();

                                let mut event = KeyEvent {
                                    event_type: KeyEventType::KeyPressed,
                                    text: ch.to_string().into(),
                                    modifiers,
                                };

                                window.self_weak.upgrade().unwrap().process_key_input(&event);

                                event.event_type = KeyEventType::KeyReleased;
                                window.self_weak.upgrade().unwrap().process_key_input(&event);
                            }
                        });
                    }
                }
                winit::event::Event::WindowEvent {
                    ref window_id,
                    event: winit::event::WindowEvent::ModifiersChanged(state),
                } => {
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            // To provide an easier cross-platform behavior, we map the command key to control
                            // on macOS, and control to meta.
                            #[cfg(target_os = "macos")]
                            let (control, meta) = (state.logo(), state.ctrl());
                            #[cfg(not(target_os = "macos"))]
                            let (control, meta) = (state.ctrl(), state.logo());
                            let modifiers = KeyboardModifiers {
                                shift: state.shift(),
                                alt: state.alt(),
                                control,
                                meta,
                            };
                            window.set_current_keyboard_modifiers(modifiers);
                        }
                    });
                }

                winit::event::Event::WindowEvent {
                    ref window_id,
                    event: winit::event::WindowEvent::Focused(have_focus),
                } => {
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            window.self_weak.upgrade().unwrap().set_focus(have_focus);
                        }
                    });
                }

                winit::event::Event::UserEvent(CustomEvent::UpdateWindowProperties(window_id)) => {
                    ALL_WINDOWS.with(|windows| {
                        if let Some(Some(window)) =
                            windows.borrow().get(&window_id).map(|weakref| weakref.upgrade())
                        {
                            window.self_weak.upgrade().unwrap().update_window_properties();
                        }
                    });
                }

                winit::event::Event::UserEvent(CustomEvent::Exit) => {
                    *control_flow = winit::event_loop::ControlFlow::Exit;
                }

                winit::event::Event::UserEvent(CustomEvent::UserEvent(user)) => {
                    user();
                }

                #[cfg(target_arch = "wasm32")]
                winit::event::Event::UserEvent(CustomEvent::RedrawAllWindows) => {
                    ALL_WINDOWS.with(|windows| {
                        windows.borrow().values().for_each(|window| {
                            if let Some(window) = window.upgrade() {
                                window.request_redraw();
                            }
                        })
                    })
                }

                _ => (),
            }

            if *control_flow != winit::event_loop::ControlFlow::Exit {
                corelib::animations::CURRENT_ANIMATION_DRIVER.with(|driver| {
                    if !driver.has_active_animations() {
                        return;
                    }
                    *control_flow = ControlFlow::Poll;
                    ALL_WINDOWS.with(|windows| {
                        windows.borrow().values().for_each(|window| {
                            if let Some(window) = window.upgrade() {
                                window.request_redraw();
                            }
                        })
                    })
                })
            }

            corelib::timers::TimerList::maybe_activate_timers();

            if *control_flow == winit::event_loop::ControlFlow::Wait {
                if let Some(next_timer) = corelib::timers::TimerList::next_timeout() {
                    *control_flow = winit::event_loop::ControlFlow::WaitUntil(next_timer);
                }
            }
        })
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        winit_loop.run_return(run_fn);

        *GLOBAL_PROXY.get_or_init(Default::default).lock().unwrap() = Default::default();
    }

    #[cfg(target_arch = "wasm32")]
    {
        winit_loop.run(run_fn)
    }
}
