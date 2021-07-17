/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
    Support for timers.

    Timers are just a bunch of callbacks sorted by expiry date.
*/

#![warn(missing_docs)]
use std::cell::{Cell, RefCell};

type TimerCallback = Box<dyn Fn()>;
type SingleShotTimerCallback = Box<dyn FnOnce()>;

/// The TimerMode specifies what should happen after the timer fired.
///
/// Used by the [`Timer::start`] function.
#[derive(Copy, Clone)]
pub enum TimerMode {
    /// A SingleShot timer is fired only once.
    SingleShot,
    /// A Repeated timer is fired repeatedly until it is stopped.
    Repeated,
}

/// Timer is a handle to the timer system that allows triggering a callback to be called
/// after a specified period of time.
#[derive(Default)]
pub struct Timer {
    id: Cell<Option<usize>>,
}

impl Timer {
    /// Starts the timer with the given mode and duration, in order for the callback to called when the
    /// timer fires. If the timer has been started previously and not fired yet, then it will be restarted.
    ///
    /// Arguments:
    /// * `mode`: The timer mode to apply, i.e. whether to repeatedly fire the timer or just once.
    /// * `duration`: The duration from now until when the timer should fire.
    /// * `callback`: The function to call when the time has been reached or exceeded.
    pub fn start(
        &self,
        mode: TimerMode,
        duration: std::time::Duration,
        callback: impl Fn() + 'static,
    ) {
        CURRENT_TIMERS.with(|timers| {
            let mut timers = timers.borrow_mut();
            let id = timers.start_or_restart_timer(
                self.id.get(),
                mode,
                duration,
                CallbackVariant::MultiFire(Box::new(callback)),
            );
            self.id.set(Some(id));
        })
    }

    /// Starts the timer with the duration, in order for the callback to called when the
    /// timer fires. It is fired only once and then deleted.
    ///
    /// Arguments:
    /// * `duration`: The duration from now until when the timer should fire.
    /// * `callback`: The function to call when the time has been reached or exceeded.
    pub fn single_shot(duration: std::time::Duration, callback: impl FnOnce() + 'static) {
        CURRENT_TIMERS.with(|timers| {
            let mut timers = timers.borrow_mut();
            let id = timers.start_or_restart_timer(
                None,
                TimerMode::SingleShot,
                duration,
                CallbackVariant::SingleShot(Box::new(callback)),
            );
            timers.timers[id].removed = true;
        })
    }

    /// Stops the previously started timer. Does nothing if the timer has never been started. A stopped
    /// timer cannot be restarted with restart() -- instead you need to call start().
    pub fn stop(&self) {
        if let Some(id) = self.id.take() {
            CURRENT_TIMERS.with(|timers| {
                timers.borrow_mut().remove_timer(id);
            });
        }
    }

    /// Restarts the timer, if it was previously started.
    pub fn restart(&self) {
        if let Some(id) = self.id.get() {
            CURRENT_TIMERS.with(|timers| {
                timers.borrow_mut().deactivate_timer(id);
                timers.borrow_mut().activate_timer(id);
            });
        }
    }

    /// Returns true if the timer is running; false otherwise.
    pub fn running(&self) -> bool {
        self.id
            .get()
            .map(|timer_id| CURRENT_TIMERS.with(|timers| timers.borrow().timers[timer_id].running))
            .unwrap_or(false)
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Some(id) = self.id.get() {
            CURRENT_TIMERS.with(|timers| {
                timers.borrow_mut().remove_timer(id);
            })
        }
    }
}

enum CallbackVariant {
    Empty,
    MultiFire(TimerCallback),
    SingleShot(SingleShotTimerCallback),
}

impl CallbackVariant {
    fn invoke(self) -> Self {
        match self {
            CallbackVariant::Empty => CallbackVariant::Empty,
            CallbackVariant::MultiFire(cb) => {
                cb();
                CallbackVariant::MultiFire(cb)
            }
            CallbackVariant::SingleShot(cb) => {
                cb();
                CallbackVariant::Empty
            }
        }
    }
}

struct TimerData {
    duration: std::time::Duration,
    mode: TimerMode,
    running: bool,
    /// Set to true when it is removed when the callback is still running
    removed: bool,
    callback: CallbackVariant,
}

#[derive(Clone, Copy)]
struct ActiveTimer {
    id: usize,
    timeout: instant::Instant,
}

/// TimerList provides the interface to the event loop for activating times and
/// determining the nearest timeout.
#[derive(Default)]
pub struct TimerList {
    timers: slab::Slab<TimerData>,
    active_timers: Vec<ActiveTimer>,
    /// If a callback is currently running, this is the id of the currently running callback
    callback_active: Option<usize>,
}

impl TimerList {
    /// Returns the timeout of the timer that should fire the soonest, or None if there
    /// is no timer active.
    pub fn next_timeout() -> Option<instant::Instant> {
        CURRENT_TIMERS.with(|timers| {
            timers
                .borrow()
                .active_timers
                .first()
                .map(|first_active_timer| first_active_timer.timeout)
        })
    }

    /// Activates any expired timers by calling their callback function. Returns true if any timers were
    /// activated; false otherwise.
    pub fn maybe_activate_timers() -> bool {
        let now = instant::Instant::now();
        // Shortcut: Is there any timer worth activating?
        if TimerList::next_timeout().map(|timeout| now < timeout).unwrap_or(false) {
            return false;
        }

        CURRENT_TIMERS.with(|timers| {
            assert!(timers.borrow().callback_active.is_none(), "Recursion in timer code");

            let mut any_activated = false;

            // The active timer list is cleared here and not-yet-fired ones are inserted below, in order to allow
            // timer callbacks to register their own timers.
            let timers_to_process = std::mem::take(&mut timers.borrow_mut().active_timers);
            for active_timer in timers_to_process.into_iter() {
                if active_timer.timeout <= now {
                    any_activated = true;

                    timers.borrow_mut().callback_active = Some(active_timer.id);
                    let callback = std::mem::replace(
                        &mut timers.borrow_mut().timers[active_timer.id].callback,
                        CallbackVariant::Empty,
                    );
                    let callback = callback.invoke();
                    let mut timers = timers.borrow_mut();
                    timers.timers[active_timer.id].callback = callback;
                    timers.callback_active = None;

                    if timers.timers[active_timer.id].removed {
                        timers.timers.remove(active_timer.id);
                    } else if matches!(timers.timers[active_timer.id].mode, TimerMode::Repeated) {
                        timers.activate_timer(active_timer.id);
                    }
                } else {
                    timers.borrow_mut().register_active_timer(active_timer);
                }
            }

            any_activated
        })
    }

    fn start_or_restart_timer(
        &mut self,
        id: Option<usize>,
        mode: TimerMode,
        duration: std::time::Duration,
        callback: CallbackVariant,
    ) -> usize {
        let timer_data = TimerData { duration, mode, running: false, removed: false, callback };
        let inactive_timer_id = if let Some(id) = id {
            self.deactivate_timer(id);
            self.timers[id] = timer_data;
            id
        } else {
            self.timers.insert(timer_data)
        };
        self.activate_timer(inactive_timer_id);
        inactive_timer_id
    }

    fn deactivate_timer(&mut self, id: usize) {
        let mut i = 0;
        while i < self.active_timers.len() {
            if self.active_timers[i].id == id {
                self.active_timers.remove(i);
                self.timers[id].running = false;
                break;
            } else {
                i += 1;
            }
        }
    }

    fn activate_timer(&mut self, timer_id: usize) {
        self.register_active_timer(ActiveTimer {
            id: timer_id,
            timeout: instant::Instant::now() + self.timers[timer_id].duration,
        });
    }

    fn register_active_timer(&mut self, new_active_timer: ActiveTimer) {
        let insertion_index = lower_bound(&self.active_timers, |existing_timer| {
            existing_timer.timeout < new_active_timer.timeout
        });

        self.active_timers.insert(insertion_index, new_active_timer);
        self.timers[new_active_timer.id].running = true;
    }

    fn remove_timer(&mut self, timer_id: usize) {
        self.deactivate_timer(timer_id);
        if self.callback_active == Some(timer_id) {
            self.timers[timer_id].removed = true;
        } else {
            self.timers.remove(timer_id);
        }
    }
}

thread_local!(static CURRENT_TIMERS : RefCell<TimerList> = RefCell::default());

fn lower_bound<T>(vec: &Vec<T>, mut less_than: impl FnMut(&T) -> bool) -> usize {
    let mut left = 0;
    let mut right = vec.len();

    while left != right {
        let mid = left + (right - left) / 2;
        let value = &vec[mid];
        if less_than(value) {
            left = mid + 1;
        } else {
            right = mid;
        }
    }

    left
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    #![allow(unsafe_code)]

    use super::*;
    #[allow(non_camel_case_types)]
    type c_void = ();

    struct WrapFn {
        callback: extern "C" fn(*mut c_void),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
    }

    impl Drop for WrapFn {
        fn drop(&mut self) {
            if let Some(x) = self.drop_user_data {
                x(self.user_data)
            }
        }
    }

    /// Start a timer with the given duration in millisecond.
    /// Returns the timer id.
    /// The timer MUST be stopped with sixtyfps_timer_stop
    #[no_mangle]
    pub extern "C" fn sixtyfps_timer_start(
        duration: u64,
        callback: extern "C" fn(*mut c_void),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
    ) -> i64 {
        let wrap = WrapFn { callback, user_data, drop_user_data };
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, core::time::Duration::from_millis(duration), move || {
            (wrap.callback)(wrap.user_data)
        });
        timer.id.take().map(|x| x as i64).unwrap_or(-1)
    }

    /// Execute a callback with a delay in millisecond
    #[no_mangle]
    pub extern "C" fn sixtyfps_timer_singleshot(
        delay: u64,
        callback: extern "C" fn(*mut c_void),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
    ) {
        let wrap = WrapFn { callback, user_data, drop_user_data };
        Timer::single_shot(core::time::Duration::from_millis(delay), move || {
            (wrap.callback)(wrap.user_data)
        });
    }

    /// Stop a timer and free its raw data
    #[no_mangle]
    pub extern "C" fn sixtyfps_timer_stop(id: i64) {
        if id == -1 {
            return;
        }
        let timer = Timer { id: Cell::new(Some(id as _)) };
        timer.stop()
    }
}
