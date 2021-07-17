/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*!
    Property binding engine.

    The current implementation uses lots of heap allocation but that can be optimized later using
    thin dst container, and intrusive linked list
*/

#![allow(unsafe_code)]
#![warn(missing_docs)]

mod single_linked_list_pin {
    #![allow(unsafe_code)]

    ///! A singled linked list whose nodes are pinned
    use core::pin::Pin;
    type NodePtr<T> = Option<Pin<Box<SingleLinkedListPinNode<T>>>>;
    struct SingleLinkedListPinNode<T> {
        next: NodePtr<T>,
        value: T,
    }

    pub struct SingleLinkedListPinHead<T>(NodePtr<T>);
    impl<T> Default for SingleLinkedListPinHead<T> {
        fn default() -> Self {
            Self(None)
        }
    }

    impl<T> Drop for SingleLinkedListPinHead<T> {
        fn drop(&mut self) {
            // Use a loop instead of relying on the Drop of NodePtr to avoid recursion
            while let Some(mut x) = core::mem::take(&mut self.0) {
                // Safety: we don't touch the `x.value` which is the one protected by the Pin
                self.0 = std::mem::take(unsafe { &mut Pin::get_unchecked_mut(x.as_mut()).next });
            }
        }
    }

    impl<T> SingleLinkedListPinHead<T> {
        pub fn push_front(&mut self, value: T) -> Pin<&T> {
            self.0 = Some(Box::pin(SingleLinkedListPinNode { next: self.0.take(), value }));
            // Safety: we can project from SingleLinkedListPinNode
            unsafe { Pin::new_unchecked(&self.0.as_ref().unwrap().value) }
        }

        #[allow(unused)]
        pub fn iter<'a>(&'a self) -> impl Iterator<Item = Pin<&T>> + 'a {
            struct I<'a, T>(&'a NodePtr<T>);

            impl<'a, T> Iterator for I<'a, T> {
                type Item = Pin<&'a T>;
                fn next(&mut self) -> Option<Self::Item> {
                    if let Some(x) = &self.0 {
                        let r = unsafe { Pin::new_unchecked(&x.value) };
                        self.0 = &x.next;
                        Some(r)
                    } else {
                        None
                    }
                }
            }
            I(&self.0)
        }
    }

    #[test]
    fn test_list() {
        let mut head = SingleLinkedListPinHead::default();
        head.push_front(1);
        head.push_front(2);
        head.push_front(3);
        assert_eq!(
            head.iter().map(|x: Pin<&i32>| *x.get_ref()).collect::<Vec<i32>>(),
            vec![3, 2, 1]
        );
    }
    #[test]
    fn big_list() {
        // should not stack overflow
        let mut head = SingleLinkedListPinHead::default();
        for x in 0..100000 {
            head.push_front(x);
        }
    }
}

use core::cell::{Cell, RefCell, UnsafeCell};
use core::{marker::PhantomPinned, pin::Pin};
use std::rc::Rc;

use crate::items::PropertyAnimation;

/// The return value of a binding
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum BindingResult {
    /// The binding is a normal binding, and we keep it to re-evaluate it once it is dirty
    KeepBinding,
    /// The value of the property is now constant after the binding was evaluated, so
    /// the binding can be removed.
    RemoveBinding,
}

struct BindingVTable {
    drop: unsafe fn(_self: *mut BindingHolder),
    evaluate: unsafe fn(_self: *mut BindingHolder, value: *mut ()) -> BindingResult,
    mark_dirty: unsafe fn(_self: *const BindingHolder, was_dirty: bool),
    intercept_set: unsafe fn(_self: *const BindingHolder, value: *const ()) -> bool,
    intercept_set_binding:
        unsafe fn(_self: *const BindingHolder, new_binding: *mut BindingHolder) -> bool,
}

/// A binding trait object can be used to dynamically produces values for a property.
trait BindingCallable {
    /// This function is called by the property to evaluate the binding and produce a new value. The
    /// previous property value is provided in the value parameter.
    unsafe fn evaluate(self: Pin<&Self>, value: *mut ()) -> BindingResult;

    /// This function is used to notify the binding that one of the dependencies was changed
    /// and therefore this binding may evaluate to a different value, too.
    fn mark_dirty(self: Pin<&Self>) {}

    /// Allow the binding to intercept what happens when the value is set.
    /// The default implementation returns false, meaning the binding will simply be removed and
    /// the property will get the new value.
    /// When returning true, the call was intercepted and the binding will not be removed,
    /// but the property will still have that value
    unsafe fn intercept_set(self: Pin<&Self>, _value: *const ()) -> bool {
        false
    }

    /// Allow the binding to intercept what happens when the value is set.
    /// The default implementation returns false, meaning the binding will simply be removed.
    /// When returning true, the call was intercepted and the binding will not be removed.
    unsafe fn intercept_set_binding(self: Pin<&Self>, _new_binding: *mut BindingHolder) -> bool {
        false
    }
}

impl<F: Fn(*mut ()) -> BindingResult> BindingCallable for F {
    unsafe fn evaluate(self: Pin<&Self>, value: *mut ()) -> BindingResult {
        self(value)
    }
}

scoped_tls_hkt::scoped_thread_local!(static CURRENT_BINDING : for<'a> Pin<&'a BindingHolder>);

#[repr(C)]
struct BindingHolder<B = ()> {
    /// Access to the list of binding which depends on this binding
    dependencies: Cell<usize>,
    /// The binding own the nodes used in the dependencies lists of the properties
    /// From which we depend.
    dep_nodes: RefCell<single_linked_list_pin::SingleLinkedListPinHead<DependencyNode>>,
    vtable: &'static BindingVTable,
    /// The binding is dirty and need to be re_evaluated
    dirty: Cell<bool>,
    pinned: PhantomPinned,
    binding: B,
}

impl BindingHolder {
    fn register_self_as_dependency(
        self: Pin<&Self>,
        property_that_will_notify: *mut DependencyListHead,
    ) {
        let node = DependencyNode::for_binding(self);
        let mut dep_nodes = self.dep_nodes.borrow_mut();
        let node = dep_nodes.push_front(node);
        unsafe { DependencyListHead::append(property_that_will_notify, node.get_ref() as *const _) }
    }
}

fn alloc_binding_holder<B: BindingCallable + 'static>(binding: B) -> *mut BindingHolder {
    /// Safety: _self must be a pointer that comes from a `Box<BindingHolder<B>>::into_raw()`
    unsafe fn binding_drop<B>(_self: *mut BindingHolder) {
        Box::from_raw(_self as *mut BindingHolder<B>);
    }

    /// Safety: _self must be a pointer to a `BindingHolder<B>`
    /// and value must be a pointer to T
    unsafe fn evaluate<B: BindingCallable>(
        _self: *mut BindingHolder,
        value: *mut (),
    ) -> BindingResult {
        let pinned_holder = Pin::new_unchecked(&*_self);
        CURRENT_BINDING.set(pinned_holder, || {
            Pin::new_unchecked(&((*(_self as *mut BindingHolder<B>)).binding)).evaluate(value)
        })
    }

    /// Safety: _self must be a pointer to a `BindingHolder<B>`
    unsafe fn mark_dirty<B: BindingCallable>(_self: *const BindingHolder, _: bool) {
        Pin::new_unchecked(&((*(_self as *const BindingHolder<B>)).binding)).mark_dirty()
    }

    /// Safety: _self must be a pointer to a `BindingHolder<B>`
    unsafe fn intercept_set<B: BindingCallable>(
        _self: *const BindingHolder,
        value: *const (),
    ) -> bool {
        Pin::new_unchecked(&((*(_self as *const BindingHolder<B>)).binding)).intercept_set(value)
    }

    unsafe fn intercept_set_binding<B: BindingCallable>(
        _self: *const BindingHolder,
        new_binding: *mut BindingHolder,
    ) -> bool {
        Pin::new_unchecked(&((*(_self as *const BindingHolder<B>)).binding))
            .intercept_set_binding(new_binding)
    }

    trait HasBindingVTable {
        const VT: &'static BindingVTable;
    }
    impl<B: BindingCallable> HasBindingVTable for B {
        const VT: &'static BindingVTable = &BindingVTable {
            drop: binding_drop::<B>,
            evaluate: evaluate::<B>,
            mark_dirty: mark_dirty::<B>,
            intercept_set: intercept_set::<B>,
            intercept_set_binding: intercept_set_binding::<B>,
        };
    }

    let holder: BindingHolder<B> = BindingHolder {
        dependencies: Cell::new(0),
        dep_nodes: Default::default(),
        vtable: <B as HasBindingVTable>::VT,
        dirty: Cell::new(true), // starts dirty so it evaluates the property when used
        pinned: PhantomPinned,
        binding,
    };
    Box::into_raw(Box::new(holder)) as *mut BindingHolder
}

#[repr(transparent)]
struct DependencyListHead(Cell<usize>);

impl DependencyListHead {
    unsafe fn mem_move(from: *mut Self, to: *mut Self) {
        (*to).0.set((*from).0.get());
        if let Some(next) = ((*from).0.get() as *const DependencyNode).as_ref() {
            debug_assert_eq!(from as *const _, next.prev.get() as *const _);
            next.debug_assert_valid();
            next.prev.set(to as *const _);
            next.debug_assert_valid();
        }
    }
    unsafe fn drop(_self: *mut Self) {
        if let Some(next) = ((*_self).0.get() as *const DependencyNode).as_ref() {
            debug_assert_eq!(_self as *const _, next.prev.get() as *const _);
            next.debug_assert_valid();
            next.prev.set(core::ptr::null());
            next.debug_assert_valid();
        }
    }
    unsafe fn append(_self: *mut Self, node: *const DependencyNode) {
        (*node).debug_assert_valid();
        let old = (*_self).0.get() as *const DependencyNode;
        if let Some(x) = old.as_ref() {
            x.debug_assert_valid();
        }
        (*_self).0.set(node as usize);
        let node = &*node;
        node.next.set(old);
        node.prev.set(_self as *const _);
        if let Some(old) = old.as_ref() {
            old.prev.set((&node.next) as *const _);
            old.debug_assert_valid();
        }
        (*node).debug_assert_valid();
    }
}

/// The node is owned by the binding; so the binding is always valid
/// The next and pref
struct DependencyNode {
    next: Cell<*const DependencyNode>,
    /// This is either null, or a pointer to a pointer to ourself
    prev: Cell<*const Cell<*const DependencyNode>>,
    binding: *const BindingHolder,
}

impl DependencyNode {
    fn for_binding(binding: Pin<&BindingHolder>) -> Self {
        Self {
            next: Cell::new(core::ptr::null()),
            prev: Cell::new(core::ptr::null()),
            binding: binding.get_ref() as *const _,
        }
    }

    /// Assert that the invariant of `next` and `prev` are met.
    fn debug_assert_valid(&self) {
        unsafe {
            debug_assert!(
                self.prev.get().is_null()
                    || (*self.prev.get()).get() == self as *const DependencyNode
            );
            debug_assert!(
                self.next.get().is_null()
                    || (*self.next.get()).prev.get()
                        == (&self.next) as *const Cell<*const DependencyNode>
            );
            // infinite loop?
            debug_assert_ne!(self.next.get(), self as *const DependencyNode);
            debug_assert_ne!(self.prev.get(), (&self.next) as *const Cell<*const DependencyNode>);
        }
    }

    fn remove(&self) {
        self.debug_assert_valid();
        unsafe {
            if let Some(prev) = self.prev.get().as_ref() {
                prev.set(self.next.get());
            }
            if let Some(next) = self.next.get().as_ref() {
                next.debug_assert_valid();
                next.prev.set(self.prev.get());
                next.debug_assert_valid();
            }
        }
        self.prev.set(std::ptr::null());
        self.next.set(std::ptr::null());
    }
}

impl Drop for DependencyNode {
    fn drop(&mut self) {
        self.remove();
    }
}

#[repr(transparent)]
#[derive(Debug, Default)]
struct PropertyHandle {
    /// The handle can either be a pointer to a binding, or a pointer to the list of dependent properties.
    /// The two least significant bit of the pointer are flags, as the pointer will be aligned.
    /// The least significant bit (`0b01`) tells that the binding is borrowed. So no two reference to the
    /// binding exist at the same time.
    /// The second to last bit (`0b10`) tells that the pointer points to a binding. Otherwise, it is the head
    /// node of the linked list of dependent binding
    handle: Cell<usize>,
}

impl PropertyHandle {
    /// The lock flag specify that we can get reference to the Cell or unsafe cell
    fn lock_flag(&self) -> bool {
        self.handle.get() & 0b1 == 1
    }
    /// Sets the lock_flag.
    /// Safety: the lock flag must not be unset if there exist reference to what's inside the cell
    unsafe fn set_lock_flag(&self, set: bool) {
        self.handle.set(if set { self.handle.get() | 0b1 } else { self.handle.get() & !0b1 })
    }

    /// Access the value.
    /// Panics if the function try to recursively access the value
    fn access<R>(&self, f: impl FnOnce(Option<Pin<&mut BindingHolder>>) -> R) -> R {
        assert!(!self.lock_flag(), "Recursion detected");
        unsafe {
            self.set_lock_flag(true);
            scopeguard::defer! { self.set_lock_flag(false); }
            let handle = self.handle.get();
            let binding = if handle & 0b10 == 0b10 {
                Some(Pin::new_unchecked(&mut *((handle & !0b11) as *mut BindingHolder)))
            } else {
                None
            };
            f(binding)
        }
    }

    fn remove_binding(&self) {
        assert!(!self.lock_flag(), "Recursion detected");
        let val = self.handle.get();
        if val & 0b10 == 0b10 {
            unsafe {
                self.set_lock_flag(true);
                let binding = (val & !0b11) as *mut BindingHolder;
                DependencyListHead::mem_move(
                    (&mut (*binding).dependencies) as *mut _ as *mut _,
                    self.handle.as_ptr() as *mut _,
                );
                ((*binding).vtable.drop)(binding);
            }
            debug_assert!(self.handle.get() & 0b11 == 0);
        }
    }

    /// Safety: the BindingCallable must be valid for the type of this property
    unsafe fn set_binding<B: BindingCallable + 'static>(&self, binding: B) {
        let binding = alloc_binding_holder::<B>(binding);
        self.set_binding_impl(binding);
    }

    /// Implementation of Self::set_binding.
    fn set_binding_impl(&self, binding: *mut BindingHolder) {
        if self.access(|b| {
            b.map_or(false, |b| unsafe {
                // Safety: b is a BindingHolder<T>
                (b.vtable.intercept_set_binding)(&*b as *const BindingHolder, binding)
            })
        }) {
            return;
        }

        self.remove_binding();
        debug_assert!((binding as usize) & 0b11 == 0);
        debug_assert!(self.handle.get() & 0b11 == 0);
        unsafe {
            DependencyListHead::mem_move(
                self.handle.as_ptr() as *mut _,
                (&mut (*binding).dependencies) as *mut _ as *mut _,
            );
            self.handle.set((binding as usize) | 0b10);
        }
    }

    fn dependencies(&self) -> *mut DependencyListHead {
        assert!(!self.lock_flag(), "Recursion detected");
        if (self.handle.get() & 0b10) != 0 {
            self.access(|binding| binding.unwrap().dependencies.as_ptr() as *mut DependencyListHead)
        } else {
            self.handle.as_ptr() as *mut DependencyListHead
        }
    }

    // `value` is the content of the unsafe cell and will be only dereferenced if the
    // handle is not locked. (Upholding the requirements of UnsafeCell)
    unsafe fn update<T>(&self, value: *mut T) {
        let remove = self.access(|binding| {
            if let Some(mut binding) = binding {
                if binding.dirty.get() {
                    // clear all the nodes so that we can start from scratch
                    *binding.dep_nodes.borrow_mut() = Default::default();
                    let r = (binding.vtable.evaluate)(
                        binding.as_mut().get_unchecked_mut() as *mut BindingHolder,
                        value as *mut (),
                    );
                    binding.dirty.set(false);
                    if r == BindingResult::RemoveBinding {
                        return true;
                    }
                }
            }
            false
        });
        if remove {
            self.remove_binding()
        }
    }

    /// Register this property as a dependency to the current binding being evaluated
    fn register_as_dependency_to_current_binding(self: Pin<&Self>) {
        if CURRENT_BINDING.is_set() {
            CURRENT_BINDING.with(|cur_binding| {
                cur_binding.register_self_as_dependency(self.dependencies());
            });
        }
    }

    fn mark_dirty(&self) {
        unsafe { mark_dependencies_dirty(self.dependencies()) };
    }
}

impl Drop for PropertyHandle {
    fn drop(&mut self) {
        self.remove_binding();
        debug_assert!(self.handle.get() & 0b11 == 0);
        unsafe {
            DependencyListHead::drop(self.handle.as_ptr() as *mut _);
        }
    }
}

/// Safety: the dependency list must be valid and consistent
unsafe fn mark_dependencies_dirty(dependencies: *mut DependencyListHead) {
    let mut next = (*dependencies).0.get() as *const DependencyNode;
    while let Some(node) = next.as_ref() {
        node.debug_assert_valid();
        next = node.next.get();
        let binding = &*node.binding;
        let was_dirty = binding.dirty.replace(true);
        (binding.vtable.mark_dirty)(node.binding, was_dirty);
        mark_dependencies_dirty(binding.dependencies.as_ptr() as *mut DependencyListHead)
    }
}

/// Types that can be set as bindings for a Property<T>
pub trait Binding<T> {
    /// Evaluate the binding and return the new value
    fn evaluate(&self, old_value: &T) -> T;
}

impl<T, F: Fn() -> T> Binding<T> for F {
    fn evaluate(&self, _value: &T) -> T {
        self()
    }
}

/// A Property that allow binding that track changes
///
/// Property van have be assigned value, or bindings.
/// When a binding is assigned, it is lazily evaluated on demand
/// when calling `get()`.
/// When accessing another property from a binding evaluation,
/// a dependency will be registered, such that when the property
/// change, the binding will automatically be updated
#[repr(C)]
pub struct Property<T> {
    /// This is usually a pointer, but the least significant bit tells what it is
    handle: PropertyHandle,
    /// This is only safe to access when the lock flag is not set on the handle.
    value: UnsafeCell<T>,
    pinned: PhantomPinned,
}

impl<T: std::fmt::Debug + Clone> std::fmt::Debug for Property<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Property({:?}{})",
            self.get_internal(),
            if self.is_dirty() { " (dirty)" } else { "" }
        )
    }
}

impl<T: Default> Default for Property<T> {
    fn default() -> Self {
        Self { handle: Default::default(), value: Default::default(), pinned: PhantomPinned }
    }
}

impl<T: Clone> Property<T> {
    /// Create a new property with this value
    pub fn new(value: T) -> Self {
        Self { handle: Default::default(), value: UnsafeCell::new(value), pinned: PhantomPinned }
    }

    /// Get the value of the property
    ///
    /// This may evaluate the binding if there is a binding and it is dirty
    ///
    /// If the function is called directly or indirectly from a binding evaluation
    /// of another Property, a dependency will be registered.
    ///
    /// Panics if this property is get while evaluating its own binding or
    /// cloning the value.
    pub fn get(self: Pin<&Self>) -> T {
        unsafe { self.handle.update(self.value.get()) };
        let handle = unsafe { Pin::new_unchecked(&self.handle) };
        handle.register_as_dependency_to_current_binding();
        self.get_internal()
    }

    /// Same as get() but without registering a dependency
    ///
    /// This allow to optimize bindings that know that they might not need to
    /// re_evaluate themselves when the property change or that have registered
    /// the dependency in another way.
    ///
    /// ## Example
    /// ```
    /// use std::rc::Rc;
    /// use sixtyfps_corelib::Property;
    /// let prop1 = Rc::pin(Property::new(100));
    /// let prop2 = Rc::pin(Property::<i32>::default());
    /// prop2.as_ref().set_binding({
    ///     let prop1 = prop1.clone(); // in order to move it into the closure.
    ///     move || { prop1.as_ref().get_untracked() + 30 }
    /// });
    /// assert_eq!(prop2.as_ref().get(), 130);
    /// prop1.set(200);
    /// // changing prop1 do not affect the prop2 binding because no dependency was registered
    /// assert_eq!(prop2.as_ref().get(), 130);
    /// ```
    pub fn get_untracked(self: Pin<&Self>) -> T {
        unsafe { self.handle.update(self.value.get()) };
        self.get_internal()
    }

    /// Get the value without registering any dependencies or executing any binding
    fn get_internal(&self) -> T {
        self.handle.access(|_| {
            // Safety: PropertyHandle::access ensure that the value is locked
            unsafe { (*self.value.get()).clone() }
        })
    }

    /// Change the value of this property
    ///
    /// If other properties have binding depending of this property, these properties will
    /// be marked as dirty.
    // FIXME  pub fn set(self: Pin<&Self>, t: T) {
    pub fn set(&self, t: T)
    where
        T: PartialEq,
    {
        if !self.handle.access(|b| {
            b.map_or(false, |b| unsafe {
                // Safety: b is a BindingHolder<T>
                (b.vtable.intercept_set)(&*b as *const BindingHolder, &t as *const T as *const ())
            })
        }) {
            self.handle.remove_binding();
        }
        // Safety: PropertyHandle::access ensure that the value is locked
        if self.handle.access(|_| unsafe {
            *self.value.get() != t && {
                *self.value.get() = t;
                true
            }
        }) {
            self.handle.mark_dirty();
        }
    }

    /// Set a binding to this property.
    ///
    /// Bindings are evaluated lazily from calling get, and the return value of the binding
    /// is the new value.
    ///
    /// If other properties have bindings depending of this property, these properties will
    /// be marked as dirty.
    ///
    /// Closures of type `Fn()->T` implements Binding<T> and can be used as a binding
    ///
    /// ## Example
    /// ```
    /// use std::rc::Rc;
    /// use sixtyfps_corelib::Property;
    /// let prop1 = Rc::pin(Property::new(100));
    /// let prop2 = Rc::pin(Property::<i32>::default());
    /// prop2.as_ref().set_binding({
    ///     let prop1 = prop1.clone(); // in order to move it into the closure.
    ///     move || { prop1.as_ref().get() + 30 }
    /// });
    /// assert_eq!(prop2.as_ref().get(), 130);
    /// prop1.set(200);
    /// // A change in prop1 forced the binding on prop2 to re_evaluate
    /// assert_eq!(prop2.as_ref().get(), 230);
    /// ```
    //FIXME pub fn set_binding(self: Pin<&Self>, f: impl Binding<T> + 'static) {
    pub fn set_binding(&self, binding: impl Binding<T> + 'static) {
        // Safety: This will make a binding callable for the type T
        unsafe {
            self.handle.set_binding(move |val: *mut ()| {
                let val = &mut *(val as *mut T);
                *val = binding.evaluate(val);
                BindingResult::KeepBinding
            });
        }
        self.handle.mark_dirty();
    }

    /// Any of the properties accessed during the last evaluation of the closure called
    /// from the last call to evaluate is potentially dirty.
    pub fn is_dirty(&self) -> bool {
        self.handle.access(|binding| binding.map_or(false, |b| b.dirty.get()))
    }
}

impl<T: Clone + InterpolatedPropertyValue + 'static> Property<T> {
    /// Change the value of this property, by animating (interpolating) from the current property's value
    /// to the specified parameter value. The animation is done according to the parameters described by
    /// the PropertyAnimation object.
    ///
    /// If other properties have binding depending of this property, these properties will
    /// be marked as dirty.
    pub fn set_animated_value(&self, value: T, animation_data: PropertyAnimation) {
        // FIXME if the current value is a dirty binding, we must run it, but we do not have the context
        let d = RefCell::new(PropertyValueAnimationData::new(
            self.get_internal(),
            value,
            animation_data,
        ));
        // Safety: the BindingCallable will cast its argument to T
        unsafe {
            self.handle.set_binding(move |val: *mut ()| {
                let (value, finished) = d.borrow_mut().compute_interpolated_value();
                *(val as *mut T) = value;
                if finished {
                    BindingResult::RemoveBinding
                } else {
                    crate::animations::CURRENT_ANIMATION_DRIVER
                        .with(|driver| driver.set_has_active_animations());
                    BindingResult::KeepBinding
                }
            });
        }
    }

    /// Set a binding to this property.
    ///
    pub fn set_animated_binding(
        &self,
        binding: impl Binding<T> + 'static,
        animation_data: PropertyAnimation,
    ) {
        let binding_callable = AnimatedBindingCallable::<T, _> {
            original_binding: PropertyHandle {
                handle: Cell::new(
                    (alloc_binding_holder(move |val: *mut ()| unsafe {
                        let val = &mut *(val as *mut T);
                        *(val as *mut T) = binding.evaluate(val);
                        BindingResult::KeepBinding
                    }) as usize)
                        | 0b10,
                ),
            },
            state: Cell::new(AnimatedBindingState::NotAnimating),
            animation_data: RefCell::new(PropertyValueAnimationData::new(
                T::default(),
                T::default(),
                animation_data,
            )),
            compute_animation_details: || -> AnimationDetail { None },
        };

        // Safety: the `AnimatedBindingCallable`'s type match the property type
        unsafe { self.handle.set_binding(binding_callable) };
        self.handle.mark_dirty();
    }

    /// Set a binding to this property, providing a callback for the transition animation
    ///
    pub fn set_animated_binding_for_transition(
        &self,
        binding: impl Binding<T> + 'static,
        compute_animation_details: impl Fn() -> (PropertyAnimation, crate::animations::Instant)
            + 'static,
    ) {
        let binding_callable = AnimatedBindingCallable::<T, _> {
            original_binding: PropertyHandle {
                handle: Cell::new(
                    (alloc_binding_holder(move |val: *mut ()| unsafe {
                        let val = &mut *(val as *mut T);
                        *(val as *mut T) = binding.evaluate(val);
                        BindingResult::KeepBinding
                    }) as usize)
                        | 0b10,
                ),
            },
            state: Cell::new(AnimatedBindingState::NotAnimating),
            animation_data: RefCell::new(PropertyValueAnimationData::new(
                T::default(),
                T::default(),
                PropertyAnimation::default(),
            )),
            compute_animation_details: move || Some(compute_animation_details()),
        };

        // Safety: the `AnimatedBindingCallable`'s type match the property type
        unsafe { self.handle.set_binding(binding_callable) };
        self.handle.mark_dirty();
    }
}

#[test]
fn properties_simple_test() {
    use pin_weak::rc::PinWeak;
    use std::rc::Rc;
    fn g(prop: &Property<i32>) -> i32 {
        unsafe { Pin::new_unchecked(prop).get() }
    }

    #[derive(Default)]
    struct Component {
        width: Property<i32>,
        height: Property<i32>,
        area: Property<i32>,
    }

    let compo = Rc::pin(Component::default());
    let w = PinWeak::downgrade(compo.clone());
    compo.area.set_binding(move || {
        let compo = w.upgrade().unwrap();
        g(&compo.width) * g(&compo.height)
    });
    compo.width.set(4);
    compo.height.set(8);
    assert_eq!(g(&compo.width), 4);
    assert_eq!(g(&compo.height), 8);
    assert_eq!(g(&compo.area), 4 * 8);

    let w = PinWeak::downgrade(compo.clone());
    compo.width.set_binding(move || {
        let compo = w.upgrade().unwrap();
        g(&compo.height) * 2
    });
    assert_eq!(g(&compo.width), 8 * 2);
    assert_eq!(g(&compo.height), 8);
    assert_eq!(g(&compo.area), 8 * 8 * 2);
}

impl<T: PartialEq + Clone + 'static> Property<T> {
    /// Link two property such that any change to one property is affecting the other property as if they
    /// where, in fact, a single property.
    /// The value or binding of prop2 is kept.
    pub fn link_two_way(prop1: Pin<&Self>, prop2: Pin<&Self>) {
        struct TwoWayBinding<T> {
            common_property: Pin<Rc<Property<T>>>,
        }
        impl<T: PartialEq + Clone + 'static> BindingCallable for TwoWayBinding<T> {
            unsafe fn evaluate(self: Pin<&Self>, value: *mut ()) -> BindingResult {
                *(value as *mut T) = self.common_property.as_ref().get();
                BindingResult::KeepBinding
            }

            unsafe fn intercept_set(self: Pin<&Self>, value: *const ()) -> bool {
                self.common_property.as_ref().set((*(value as *const T)).clone());
                true
            }

            unsafe fn intercept_set_binding(
                self: Pin<&Self>,
                new_binding: *mut BindingHolder,
            ) -> bool {
                self.common_property.handle.set_binding_impl(new_binding);
                true
            }
        }

        let value = prop2.get();
        let prop2_handle_val = prop2.handle.handle.get();
        let handle = if prop2_handle_val & 0b10 == 0b10 {
            // If prop2 is a binding, just "steal it"
            prop2.handle.handle.set(0);
            PropertyHandle { handle: Cell::new(prop2_handle_val) }
        } else {
            PropertyHandle::default()
        };
        let common_property =
            Rc::pin(Property { handle, value: UnsafeCell::new(value), pinned: PhantomPinned });
        // Safety: TwoWayBinding's T is the same as the type for both properties
        unsafe {
            prop1.handle.set_binding(TwoWayBinding { common_property: common_property.clone() });
            prop2.handle.set_binding(TwoWayBinding { common_property });
        }
        prop1.handle.mark_dirty();
    }
}

#[test]
fn property_two_ways_test() {
    let p1 = Rc::pin(Property::new(42));
    let p2 = Rc::pin(Property::new(88));

    let depends = Box::pin(Property::new(0));
    depends.as_ref().set_binding({
        let p1 = p1.clone();
        move || p1.as_ref().get() + 8
    });
    assert_eq!(depends.as_ref().get(), 42 + 8);
    Property::link_two_way(p1.as_ref(), p2.as_ref());
    assert_eq!(p1.as_ref().get(), 88);
    assert_eq!(p2.as_ref().get(), 88);
    assert_eq!(depends.as_ref().get(), 88 + 8);
    p2.as_ref().set(5);
    assert_eq!(p1.as_ref().get(), 5);
    assert_eq!(p2.as_ref().get(), 5);
    assert_eq!(depends.as_ref().get(), 5 + 8);
    p1.as_ref().set(22);
    assert_eq!(p1.as_ref().get(), 22);
    assert_eq!(p2.as_ref().get(), 22);
    assert_eq!(depends.as_ref().get(), 22 + 8);
}

#[test]
fn property_two_ways_test_binding() {
    let p1 = Rc::pin(Property::new(42));
    let p2 = Rc::pin(Property::new(88));
    let global = Rc::pin(Property::new(23));
    p2.as_ref().set_binding({
        let global = global.clone();
        move || global.as_ref().get() + 9
    });

    let depends = Box::pin(Property::new(0));
    depends.as_ref().set_binding({
        let p1 = p1.clone();
        move || p1.as_ref().get() + 8
    });

    Property::link_two_way(p1.as_ref(), p2.as_ref());
    assert_eq!(p1.as_ref().get(), 23 + 9);
    assert_eq!(p2.as_ref().get(), 23 + 9);
    assert_eq!(depends.as_ref().get(), 23 + 9 + 8);
    global.as_ref().set(55);
    assert_eq!(p1.as_ref().get(), 55 + 9);
    assert_eq!(p2.as_ref().get(), 55 + 9);
    assert_eq!(depends.as_ref().get(), 55 + 9 + 8);
}

struct PropertyValueAnimationData<T> {
    from_value: T,
    to_value: T,
    details: PropertyAnimation,
    start_time: crate::animations::Instant,
    loop_iteration: i32,
}

impl<T: InterpolatedPropertyValue + Clone> PropertyValueAnimationData<T> {
    fn new(from_value: T, to_value: T, details: PropertyAnimation) -> Self {
        let start_time = crate::animations::current_tick();
        Self { from_value, to_value, details, start_time, loop_iteration: 0 }
    }

    fn compute_interpolated_value(&mut self) -> (T, bool) {
        let duration = self.details.duration as u128;
        let new_tick = crate::animations::current_tick();
        let mut time_progress = new_tick.duration_since(self.start_time).as_millis();
        if time_progress >= duration {
            if self.loop_iteration < self.details.loop_count || self.details.loop_count < 0 {
                self.loop_iteration += (time_progress / duration) as i32;
                time_progress %= duration;
                self.start_time = new_tick - std::time::Duration::from_millis(time_progress as u64);
            } else {
                return (self.to_value.clone(), true);
            }
        }
        let progress = time_progress as f32 / self.details.duration as f32;
        assert!(progress <= 1.);
        let t = crate::animations::easing_curve(&self.details.easing, progress);
        let val = self.from_value.interpolate(&self.to_value, t);
        (val, false)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum AnimatedBindingState {
    Animating,
    NotAnimating,
    ShouldStart,
}

struct AnimatedBindingCallable<T, A> {
    original_binding: PropertyHandle,
    state: Cell<AnimatedBindingState>,
    animation_data: RefCell<PropertyValueAnimationData<T>>,
    compute_animation_details: A,
}

type AnimationDetail = Option<(PropertyAnimation, crate::animations::Instant)>;

impl<T: InterpolatedPropertyValue + Clone, A: Fn() -> AnimationDetail> BindingCallable
    for AnimatedBindingCallable<T, A>
{
    unsafe fn evaluate(self: Pin<&Self>, value: *mut ()) -> BindingResult {
        let original_binding = Pin::new_unchecked(&self.original_binding);
        original_binding.register_as_dependency_to_current_binding();
        match self.state.get() {
            AnimatedBindingState::Animating => {
                let (val, finished) = self.animation_data.borrow_mut().compute_interpolated_value();
                *(value as *mut T) = val;
                if finished {
                    self.state.set(AnimatedBindingState::NotAnimating)
                } else {
                    crate::animations::CURRENT_ANIMATION_DRIVER
                        .with(|driver| driver.set_has_active_animations());
                }
            }
            AnimatedBindingState::NotAnimating => {
                self.original_binding.update(value);
            }
            AnimatedBindingState::ShouldStart => {
                let value = &mut *(value as *mut T);
                self.state.set(AnimatedBindingState::Animating);
                let mut animation_data = self.animation_data.borrow_mut();
                animation_data.loop_iteration = 0;
                animation_data.from_value = value.clone();
                self.original_binding.update((&mut animation_data.to_value) as *mut T as *mut ());
                if let Some((details, start_time)) = (self.compute_animation_details)() {
                    animation_data.start_time = start_time;
                    animation_data.details = details;
                }
                let (val, finished) = animation_data.compute_interpolated_value();
                *value = val;
                if finished {
                    self.state.set(AnimatedBindingState::NotAnimating)
                } else {
                    crate::animations::CURRENT_ANIMATION_DRIVER
                        .with(|driver| driver.set_has_active_animations());
                }
            }
        };
        BindingResult::KeepBinding
    }
    fn mark_dirty(self: Pin<&Self>) {
        if self.state.get() == AnimatedBindingState::ShouldStart {
            return;
        }
        let original_dirty = self.original_binding.access(|b| b.unwrap().dirty.get());
        if original_dirty {
            self.state.set(AnimatedBindingState::ShouldStart);
            self.animation_data.borrow_mut().start_time = crate::animations::current_tick();
        }
    }
}

/// InterpolatedPropertyValue is a trait used to enable properties to be used with
/// animations that interpolate values. The basic requirement is the ability to apply
/// a progress that's typically between 0 and 1 to a range.
pub trait InterpolatedPropertyValue: PartialEq + Default + 'static {
    /// Returns the interpolated value between self and target_value according to the
    /// progress parameter t that's usually between 0 and 1. With certain animation
    /// easing curves it may over- or undershoot though.
    fn interpolate(&self, target_value: &Self, t: f32) -> Self;
}

impl InterpolatedPropertyValue for f32 {
    fn interpolate(&self, target_value: &Self, t: f32) -> Self {
        self + t * (target_value - self)
    }
}

impl InterpolatedPropertyValue for i32 {
    fn interpolate(&self, target_value: &Self, t: f32) -> Self {
        self + (t * (target_value - self) as f32) as i32
    }
}

impl InterpolatedPropertyValue for i64 {
    fn interpolate(&self, target_value: &Self, t: f32) -> Self {
        self + (t * (target_value - self) as f32) as Self
    }
}

impl InterpolatedPropertyValue for u8 {
    fn interpolate(&self, target_value: &Self, t: f32) -> Self {
        ((*self as f32) + (t * ((*target_value as f32) - (*self as f32)))).min(255.).max(0.) as u8
    }
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    use crate::items::PropertyAnimation;
    use std::rc::Rc;

    #[derive(Default)]
    struct Component {
        width: Property<i32>,
        width_times_two: Property<i32>,
        feed_property: Property<i32>, // used by binding to feed values into width
    }

    impl Component {
        fn new_test_component() -> Rc<Self> {
            let compo = Rc::new(Component::default());
            let w = Rc::downgrade(&compo);
            compo.width_times_two.set_binding(move || {
                let compo = w.upgrade().unwrap();
                get_prop_value(&compo.width) * 2
            });

            compo
        }
    }

    const DURATION: instant::Duration = instant::Duration::from_millis(10000);

    // Helper just for testing
    fn get_prop_value<T: Clone>(prop: &Property<T>) -> T {
        unsafe { Pin::new_unchecked(prop).get() }
    }

    #[test]
    fn properties_test_animation_triggered_by_set() {
        let compo = Component::new_test_component();

        let animation_details = PropertyAnimation {
            duration: DURATION.as_millis() as _,
            ..PropertyAnimation::default()
        };

        compo.width.set(100);
        assert_eq!(get_prop_value(&compo.width), 100);
        assert_eq!(get_prop_value(&compo.width_times_two), 200);

        let start_time = crate::animations::current_tick();

        compo.width.set_animated_value(200, animation_details);
        assert_eq!(get_prop_value(&compo.width), 100);
        assert_eq!(get_prop_value(&compo.width_times_two), 200);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION / 2));
        assert_eq!(get_prop_value(&compo.width), 150);
        assert_eq!(get_prop_value(&compo.width_times_two), 300);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION));
        assert_eq!(get_prop_value(&compo.width), 200);
        assert_eq!(get_prop_value(&compo.width_times_two), 400);
        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION * 2));
        assert_eq!(get_prop_value(&compo.width), 200);
        assert_eq!(get_prop_value(&compo.width_times_two), 400);

        // the binding should be removed
        compo.width.handle.access(|binding| assert!(binding.is_none()));
    }

    #[test]
    fn properties_test_animation_triggered_by_binding() {
        let compo = Component::new_test_component();

        let start_time = crate::animations::current_tick();

        let animation_details = PropertyAnimation {
            duration: DURATION.as_millis() as _,
            ..PropertyAnimation::default()
        };

        let w = Rc::downgrade(&compo);
        compo.width.set_animated_binding(
            move || {
                let compo = w.upgrade().unwrap();
                get_prop_value(&compo.feed_property)
            },
            animation_details,
        );

        compo.feed_property.set(100);
        assert_eq!(get_prop_value(&compo.width), 100);
        assert_eq!(get_prop_value(&compo.width_times_two), 200);

        compo.feed_property.set(200);
        assert_eq!(get_prop_value(&compo.width), 100);
        assert_eq!(get_prop_value(&compo.width_times_two), 200);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION / 2));

        assert_eq!(get_prop_value(&compo.width), 150);
        assert_eq!(get_prop_value(&compo.width_times_two), 300);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION));

        assert_eq!(get_prop_value(&compo.width), 200);
        assert_eq!(get_prop_value(&compo.width_times_two), 400);
    }

    #[test]
    fn test_loop() {
        let compo = Component::new_test_component();

        let animation_details = PropertyAnimation {
            duration: DURATION.as_millis() as _,
            loop_count: 2,
            ..PropertyAnimation::default()
        };

        compo.width.set(100);

        let start_time = crate::animations::current_tick();

        compo.width.set_animated_value(200, animation_details);
        assert_eq!(get_prop_value(&compo.width), 100);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION / 2));
        assert_eq!(get_prop_value(&compo.width), 150);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION));
        assert_eq!(get_prop_value(&compo.width), 100);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION + DURATION / 2));
        assert_eq!(get_prop_value(&compo.width), 150);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION * 2));
        assert_eq!(get_prop_value(&compo.width), 100);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION * 2 + DURATION / 2));
        assert_eq!(get_prop_value(&compo.width), 150);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION * 3));
        assert_eq!(get_prop_value(&compo.width), 200);

        // the binding should be removed
        compo.width.handle.access(|binding| assert!(binding.is_none()));
    }

    #[test]
    fn test_loop_overshoot() {
        let compo = Component::new_test_component();

        let animation_details = PropertyAnimation {
            duration: DURATION.as_millis() as _,
            loop_count: 2,
            ..PropertyAnimation::default()
        };

        compo.width.set(100);

        let start_time = crate::animations::current_tick();

        compo.width.set_animated_value(200, animation_details);
        assert_eq!(get_prop_value(&compo.width), 100);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION / 2));
        assert_eq!(get_prop_value(&compo.width), 150);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION * 2 + DURATION / 2));
        assert_eq!(get_prop_value(&compo.width), 150);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION * 3));
        assert_eq!(get_prop_value(&compo.width), 200);

        // the binding should be removed
        compo.width.handle.access(|binding| assert!(binding.is_none()));
    }

    #[test]
    fn test_loop_via_binding() {
        // Loop twice, restart the animation and still loop twice.

        let compo = Component::new_test_component();

        let start_time = crate::animations::current_tick();

        let animation_details = PropertyAnimation {
            duration: DURATION.as_millis() as _,
            loop_count: 1,
            ..PropertyAnimation::default()
        };

        let w = Rc::downgrade(&compo);
        compo.width.set_animated_binding(
            move || {
                let compo = w.upgrade().unwrap();
                get_prop_value(&compo.feed_property)
            },
            animation_details,
        );

        compo.feed_property.set(100);
        assert_eq!(get_prop_value(&compo.width), 100);

        compo.feed_property.set(200);
        assert_eq!(get_prop_value(&compo.width), 100);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION / 2));

        assert_eq!(get_prop_value(&compo.width), 150);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION));

        assert_eq!(get_prop_value(&compo.width), 100);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION + DURATION / 2));

        assert_eq!(get_prop_value(&compo.width), 150);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + 2 * DURATION));

        assert_eq!(get_prop_value(&compo.width), 200);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + 2 * DURATION + DURATION / 2));

        assert_eq!(get_prop_value(&compo.width), 200);

        // Restart the animation by setting a new value.

        let start_time = crate::animations::current_tick();

        compo.feed_property.set(300);
        assert_eq!(get_prop_value(&compo.width), 200);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION / 2));

        assert_eq!(get_prop_value(&compo.width), 250);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION));

        assert_eq!(get_prop_value(&compo.width), 200);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + DURATION + DURATION / 2));

        assert_eq!(get_prop_value(&compo.width), 250);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + 2 * DURATION));

        assert_eq!(get_prop_value(&compo.width), 300);

        crate::animations::CURRENT_ANIMATION_DRIVER
            .with(|driver| driver.update_animations(start_time + 2 * DURATION + DURATION / 2));

        assert_eq!(get_prop_value(&compo.width), 300);
    }
}

/// Value of the state property
///
/// A state is just the current state, but also has information about the previous state and the moment it changed
#[repr(C)]
#[derive(Clone, Default, Debug, PartialEq)]
pub struct StateInfo {
    /// The current state value
    pub current_state: i32,
    /// The previous state
    pub previous_state: i32,
    /// The instant in which the state changed last
    pub change_time: crate::animations::Instant,
}

struct StateInfoBinding<F> {
    dirty_time: Cell<Option<crate::animations::Instant>>,
    binding: F,
}

impl<F: Fn() -> i32> crate::properties::BindingCallable for StateInfoBinding<F> {
    unsafe fn evaluate(self: Pin<&Self>, value: *mut ()) -> BindingResult {
        // Safety: We should ony set this binding on a property of type StateInfo
        let value = &mut *(value as *mut StateInfo);
        let new_state = (self.binding)();
        let timestamp = self.dirty_time.take();
        if new_state != value.current_state {
            value.previous_state = value.current_state;
            value.change_time = timestamp.unwrap_or_else(crate::animations::current_tick);
            value.current_state = new_state;
        }
        BindingResult::KeepBinding
    }

    fn mark_dirty(self: Pin<&Self>) {
        if self.dirty_time.get().is_none() {
            self.dirty_time.set(Some(crate::animations::current_tick()))
        }
    }
}

/// Sets a binding that returns a state to a StateInfo property
pub fn set_state_binding(property: Pin<&Property<StateInfo>>, binding: impl Fn() -> i32 + 'static) {
    let bind_callable = StateInfoBinding { dirty_time: Cell::new(None), binding };
    // Safety: The StateInfoBinding is a BindingCallable for type StateInfo
    unsafe { property.handle.set_binding(bind_callable) }
}

#[doc(hidden)]
pub trait PropertyChangeHandler {
    fn notify(&self);
}

impl PropertyChangeHandler for () {
    fn notify(&self) {}
}

impl<F: Fn()> PropertyChangeHandler for F {
    fn notify(&self) {
        self()
    }
}

/// This structure allow to run a closure that queries properties, and can report
/// if any property we accessed have become dirty
pub struct PropertyTracker<ChangeHandler = ()> {
    holder: BindingHolder<ChangeHandler>,
}

impl Default for PropertyTracker<()> {
    fn default() -> Self {
        static VT: &BindingVTable = &BindingVTable {
            drop: |_| (),
            evaluate: |_, _| BindingResult::KeepBinding,
            mark_dirty: |_, _| (),
            intercept_set: |_, _| false,
            intercept_set_binding: |_, _| false,
        };

        let holder = BindingHolder {
            dependencies: Cell::new(0),
            dep_nodes: Default::default(),
            vtable: VT,
            dirty: Cell::new(true), // starts dirty so it evaluates the property when used
            pinned: PhantomPinned,
            binding: (),
        };
        Self { holder }
    }
}

impl<ChangeHandler> Drop for PropertyTracker<ChangeHandler> {
    fn drop(&mut self) {
        unsafe {
            DependencyListHead::drop(self.holder.dependencies.as_ptr() as *mut DependencyListHead);
        }
    }
}

impl<ChangeHandler: PropertyChangeHandler> PropertyTracker<ChangeHandler> {
    /// Register this property tracker as a dependency to the current binding/property tracker being evaluated
    fn register_as_dependency_to_current_binding(self: Pin<&Self>) {
        if CURRENT_BINDING.is_set() {
            CURRENT_BINDING.with(|cur_binding| {
                cur_binding.register_self_as_dependency(
                    self.holder.dependencies.as_ptr() as *mut DependencyListHead
                );
            });
        }
    }

    /// Any of the properties accessed during the last evaluation of the closure called
    /// from the last call to evaluate is potentially dirty.
    pub fn is_dirty(&self) -> bool {
        self.holder.dirty.get()
    }

    /// Evaluate the function, and record dependencies of properties accessed within this function.
    /// If this is called during the evaluation of another property binding or property tracker, then
    /// any changes to accessed properties will also mark the other binding/tracker dirty.
    pub fn evaluate<R>(self: Pin<&Self>, f: impl FnOnce() -> R) -> R {
        self.register_as_dependency_to_current_binding();
        self.evaluate_as_dependency_root(f)
    }

    /// Evaluate the function, and record dependencies of properties accessed within this function.
    /// If this is called during the evaluation of another property binding or property tracker, then
    /// any changes to accessed properties will not propagate to the other tracker.
    pub fn evaluate_as_dependency_root<R>(self: Pin<&Self>, f: impl FnOnce() -> R) -> R {
        // clear all the nodes so that we can start from scratch
        *self.holder.dep_nodes.borrow_mut() = Default::default();

        // Safety: it is safe to project the holder as we don't implement drop or unpin
        let pinned_holder = unsafe {
            self.map_unchecked(|s| {
                core::mem::transmute::<&BindingHolder<ChangeHandler>, &BindingHolder<()>>(&s.holder)
            })
        };
        let r = CURRENT_BINDING.set(pinned_holder, f);
        self.holder.dirty.set(false);
        r
    }

    /// Call [`Self::evaluate`] if and only if it is dirty.
    /// But register a dependency in any case.
    pub fn evaluate_if_dirty<R>(self: Pin<&Self>, f: impl FnOnce() -> R) -> Option<R> {
        self.register_as_dependency_to_current_binding();
        self.is_dirty().then(|| self.evaluate_as_dependency_root(f))
    }

    /// Mark this PropertyTracker as dirty
    pub fn set_dirty(&self) {
        self.holder.dirty.set(true);
        unsafe { mark_dependencies_dirty(self.holder.dependencies.as_ptr() as *mut _) };
    }

    /// Sets the specified callback handler function, which will be called if any
    /// properties that this tracker depends on change their value.
    pub fn new_with_change_handler(handler: ChangeHandler) -> Self {
        /// Safety: _self must be a pointer to a `BindingHolder<ChangeHandler>`
        unsafe fn mark_dirty<B: PropertyChangeHandler>(
            _self: *const BindingHolder,
            was_dirty: bool,
        ) {
            if !was_dirty {
                ((*(_self as *const BindingHolder<B>)).binding).notify();
            }
        }

        trait HasBindingVTable {
            const VT: &'static BindingVTable;
        }
        impl<B: PropertyChangeHandler> HasBindingVTable for B {
            const VT: &'static BindingVTable = &BindingVTable {
                drop: |_| (),
                evaluate: |_, _| BindingResult::KeepBinding,
                mark_dirty: mark_dirty::<B>,
                intercept_set: |_, _| false,
                intercept_set_binding: |_, _| false,
            };
        }

        let holder = BindingHolder {
            dependencies: Cell::new(0),
            dep_nodes: Default::default(),
            vtable: <ChangeHandler as HasBindingVTable>::VT,
            dirty: Cell::new(true), // starts dirty so it evaluates the property when used
            pinned: PhantomPinned,
            binding: handler,
        };
        Self { holder }
    }
}

#[test]
fn test_property_listener_scope() {
    let scope = Box::pin(PropertyTracker::default());
    let prop1 = Box::pin(Property::new(42));
    assert!(scope.is_dirty()); // It is dirty at the beginning

    let r = scope.as_ref().evaluate(|| prop1.as_ref().get());
    assert_eq!(r, 42);
    assert!(!scope.is_dirty()); // It is no longer dirty
    prop1.as_ref().set(88);
    assert!(scope.is_dirty()); // now dirty for prop1 changed.
    let r = scope.as_ref().evaluate(|| prop1.as_ref().get() + 1);
    assert_eq!(r, 89);
    assert!(!scope.is_dirty());
    let r = scope.as_ref().evaluate(|| 12);
    assert_eq!(r, 12);
    assert!(!scope.is_dirty());
    prop1.as_ref().set(1);
    assert!(!scope.is_dirty());
    scope.as_ref().evaluate_if_dirty(|| panic!("should not be dirty"));
    scope.set_dirty();
    let mut ok = false;
    scope.as_ref().evaluate_if_dirty(|| ok = true);
    assert!(ok);
}

#[test]
fn test_nested_property_trackers() {
    let tracker1 = Box::pin(PropertyTracker::default());
    let tracker2 = Box::pin(PropertyTracker::default());
    let prop = Box::pin(Property::new(42));

    let r = tracker1.as_ref().evaluate(|| tracker2.as_ref().evaluate(|| prop.as_ref().get()));
    assert_eq!(r, 42);

    prop.as_ref().set(1);
    assert!(tracker2.as_ref().is_dirty());
    assert!(tracker1.as_ref().is_dirty());

    let r = tracker1
        .as_ref()
        .evaluate(|| tracker2.as_ref().evaluate_as_dependency_root(|| prop.as_ref().get()));
    assert_eq!(r, 1);
    prop.as_ref().set(100);
    assert!(tracker2.as_ref().is_dirty());
    assert!(!tracker1.as_ref().is_dirty());
}

#[test]
fn test_property_change_handler() {
    let call_flag = Rc::new(Cell::new(false));
    let tracker = Box::pin(PropertyTracker::new_with_change_handler({
        let call_flag = call_flag.clone();
        move || {
            (*call_flag).set(true);
        }
    }));
    let prop = Box::pin(Property::new(42));

    let r = tracker.as_ref().evaluate(|| prop.as_ref().get());

    assert_eq!(r, 42);
    assert!(!tracker.as_ref().is_dirty());
    assert!(!call_flag.get());

    prop.as_ref().set(100);
    assert!(tracker.as_ref().is_dirty());
    assert!(call_flag.get());

    // Repeated changes before evaluation should not trigger further
    // change handler calls, otherwise it would be a notification storm.
    call_flag.set(false);
    prop.as_ref().set(101);
    assert!(tracker.as_ref().is_dirty());
    assert!(!call_flag.get());
}

#[test]
fn test_property_tracker_drop() {
    let outer_tracker = Box::pin(PropertyTracker::default());
    let inner_tracker = Box::pin(PropertyTracker::default());
    let prop = Box::pin(Property::new(42));

    let r =
        outer_tracker.as_ref().evaluate(|| inner_tracker.as_ref().evaluate(|| prop.as_ref().get()));
    assert_eq!(r, 42);

    drop(inner_tracker);
    prop.as_ref().set(200); // don't crash
}

#[test]
fn test_nested_property_tracker_dirty() {
    let outer_tracker = Box::pin(PropertyTracker::default());
    let inner_tracker = Box::pin(PropertyTracker::default());
    let prop = Box::pin(Property::new(42));

    let r =
        outer_tracker.as_ref().evaluate(|| inner_tracker.as_ref().evaluate(|| prop.as_ref().get()));
    assert_eq!(r, 42);

    assert!(!outer_tracker.is_dirty());
    assert!(!inner_tracker.is_dirty());

    // Let's pretend that there was another dependency unaccounted first, mark the inner tracker as dirty
    // by hand.
    inner_tracker.as_ref().set_dirty();
    assert!(outer_tracker.is_dirty());
}

#[test]
#[allow(clippy::redundant_closure)]
fn test_nested_property_tracker_evaluate_if_dirty() {
    let outer_tracker = Box::pin(PropertyTracker::default());
    let inner_tracker = Box::pin(PropertyTracker::default());
    let prop = Box::pin(Property::new(42));

    let mut cache = 0;
    let mut cache_or_evaluate = || {
        if let Some(x) = inner_tracker.as_ref().evaluate_if_dirty(|| prop.as_ref().get() + 1) {
            cache = x;
        }
        cache
    };
    let r = outer_tracker.as_ref().evaluate(|| cache_or_evaluate());
    assert_eq!(r, 43);
    assert!(!outer_tracker.is_dirty());
    assert!(!inner_tracker.is_dirty());
    prop.as_ref().set(11);
    assert!(outer_tracker.is_dirty());
    assert!(inner_tracker.is_dirty());
    let r = outer_tracker.as_ref().evaluate(|| cache_or_evaluate());
    assert_eq!(r, 12);
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    use super::*;
    use crate::graphics::{Brush, Color};
    use core::pin::Pin;

    #[allow(non_camel_case_types)]
    type c_void = ();
    #[repr(C)]
    /// Has the same layout as PropertyHandle
    pub struct PropertyHandleOpaque(PropertyHandle);

    /// Initialize the first pointer of the Property. Does not initialize the content.
    /// `out` is assumed to be uninitialized
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_init(out: *mut PropertyHandleOpaque) {
        core::ptr::write(out, PropertyHandleOpaque(PropertyHandle::default()));
    }

    /// To be called before accessing the value
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_update(
        handle: &PropertyHandleOpaque,
        val: *mut c_void,
    ) {
        let handle = Pin::new_unchecked(&handle.0);
        handle.update(val);
        handle.register_as_dependency_to_current_binding();
    }

    /// Mark the fact that the property was changed and that its binding need to be removed, and
    /// the dependencies marked dirty.
    /// To be called after the `value` has been changed
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_changed(
        handle: &PropertyHandleOpaque,
        value: *const c_void,
    ) {
        if !handle.0.access(|b| {
            b.map_or(false, |b| (b.vtable.intercept_set)(&*b as *const BindingHolder, value))
        }) {
            handle.0.remove_binding();
        }
        handle.0.mark_dirty();
    }

    fn make_c_function_binding(
        binding: extern "C" fn(*mut c_void, *mut c_void),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
        intercept_set: Option<
            extern "C" fn(user_data: *mut c_void, pointer_to_value: *const c_void) -> bool,
        >,
        intercept_set_binding: Option<
            extern "C" fn(user_data: *mut c_void, new_binding: *mut c_void) -> bool,
        >,
    ) -> impl BindingCallable {
        struct CFunctionBinding<T> {
            binding_function: extern "C" fn(*mut c_void, *mut T),
            user_data: *mut c_void,
            drop_user_data: Option<extern "C" fn(*mut c_void)>,
            intercept_set: Option<
                extern "C" fn(user_data: *mut c_void, pointer_to_value: *const c_void) -> bool,
            >,
            intercept_set_binding:
                Option<extern "C" fn(user_data: *mut c_void, new_binding: *mut c_void) -> bool>,
        }

        impl<T> Drop for CFunctionBinding<T> {
            fn drop(&mut self) {
                if let Some(x) = self.drop_user_data {
                    x(self.user_data)
                }
            }
        }

        impl<T> BindingCallable for CFunctionBinding<T> {
            unsafe fn evaluate(self: Pin<&Self>, value: *mut ()) -> BindingResult {
                (self.binding_function)(self.user_data, value as *mut T);
                BindingResult::KeepBinding
            }
            unsafe fn intercept_set(self: Pin<&Self>, value: *const ()) -> bool {
                match self.intercept_set {
                    None => false,
                    Some(intercept_set) => intercept_set(self.user_data, value),
                }
            }
            unsafe fn intercept_set_binding(
                self: Pin<&Self>,
                new_binding: *mut BindingHolder,
            ) -> bool {
                match self.intercept_set_binding {
                    None => false,
                    Some(intercept_set_b) => intercept_set_b(self.user_data, new_binding.cast()),
                }
            }
        }

        CFunctionBinding {
            binding_function: binding,
            user_data,
            drop_user_data,
            intercept_set,
            intercept_set_binding,
        }
    }

    /// Set a binding
    ///
    /// The current implementation will do usually two memory allocation:
    ///  1. the allocation from the calling code to allocate user_data
    ///  2. the box allocation within this binding
    /// It might be possible to reduce that by passing something with a
    /// vtable, so there is the need for less memory allocation.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_binding(
        handle: &PropertyHandleOpaque,
        binding: extern "C" fn(user_data: *mut c_void, pointer_to_value: *mut c_void),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
        intercept_set: Option<
            extern "C" fn(user_data: *mut c_void, pointer_to_Value: *const c_void) -> bool,
        >,
        intercept_set_binding: Option<
            extern "C" fn(user_data: *mut c_void, new_binding: *mut c_void) -> bool,
        >,
    ) {
        let binding = make_c_function_binding(
            binding,
            user_data,
            drop_user_data,
            intercept_set,
            intercept_set_binding,
        );
        handle.0.set_binding(binding);
    }

    /// Set a binding using an already allocated building holder
    ///
    //// (take ownership of the binding)
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_binding_internal(
        handle: &PropertyHandleOpaque,
        binding: *mut c_void,
    ) {
        handle.0.set_binding_impl(binding.cast());
    }

    /// Returns whether the property behind this handle is marked as dirty
    #[no_mangle]
    pub extern "C" fn sixtyfps_property_is_dirty(handle: &PropertyHandleOpaque) -> bool {
        handle.0.access(|binding| binding.map_or(false, |b| b.dirty.get()))
    }

    /// Destroy handle
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_drop(handle: *mut PropertyHandleOpaque) {
        core::ptr::drop_in_place(handle);
    }

    fn c_set_animated_value<T: InterpolatedPropertyValue + Clone>(
        handle: &PropertyHandleOpaque,
        from: T,
        to: T,
        animation_data: &PropertyAnimation,
    ) {
        let d = RefCell::new(PropertyValueAnimationData::new(from, to, animation_data.clone()));
        // Safety: The BindingCallable is for type T
        unsafe {
            handle.0.set_binding(move |val: *mut ()| {
                let (value, finished) = d.borrow_mut().compute_interpolated_value();
                *(val as *mut T) = value;
                if finished {
                    BindingResult::RemoveBinding
                } else {
                    crate::animations::CURRENT_ANIMATION_DRIVER
                        .with(|driver| driver.set_has_active_animations());
                    BindingResult::KeepBinding
                }
            })
        };
    }

    /// Internal function to set up a property animation to the specified target value for an integer property.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_animated_value_int(
        handle: &PropertyHandleOpaque,
        from: i32,
        to: i32,
        animation_data: &PropertyAnimation,
    ) {
        c_set_animated_value(handle, from, to, animation_data)
    }

    /// Internal function to set up a property animation to the specified target value for a float property.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_animated_value_float(
        handle: &PropertyHandleOpaque,
        from: f32,
        to: f32,
        animation_data: &PropertyAnimation,
    ) {
        c_set_animated_value(handle, from, to, animation_data)
    }

    /// Internal function to set up a property animation to the specified target value for a color property.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_animated_value_color(
        handle: &PropertyHandleOpaque,
        from: Color,
        to: Color,
        animation_data: &PropertyAnimation,
    ) {
        c_set_animated_value(handle, from, to, animation_data);
    }

    unsafe fn c_set_animated_binding<T: InterpolatedPropertyValue + Clone>(
        handle: &PropertyHandleOpaque,
        binding: extern "C" fn(*mut c_void, *mut T),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
        animation_data: Option<&PropertyAnimation>,
        transition_data: Option<
            extern "C" fn(user_data: *mut c_void, start_instant: &mut u64) -> PropertyAnimation,
        >,
    ) {
        let binding = core::mem::transmute::<
            extern "C" fn(*mut c_void, *mut T),
            extern "C" fn(*mut c_void, *mut ()),
        >(binding);
        let original_binding = PropertyHandle {
            handle: Cell::new(
                (alloc_binding_holder(make_c_function_binding(
                    binding,
                    user_data,
                    drop_user_data,
                    None,
                    None,
                )) as usize)
                    | 0b10,
            ),
        };
        let animation_data = RefCell::new(PropertyValueAnimationData::new(
            T::default(),
            T::default(),
            animation_data.cloned().unwrap_or_default(),
        ));
        if let Some(transition_data) = transition_data {
            handle.0.set_binding(AnimatedBindingCallable::<T, _> {
                original_binding,
                state: Cell::new(AnimatedBindingState::NotAnimating),
                animation_data,
                compute_animation_details: move || -> AnimationDetail {
                    let mut start_instant = 0;
                    let anim = transition_data(user_data, &mut start_instant);
                    Some((anim, crate::animations::Instant(start_instant)))
                },
            });
        } else {
            handle.0.set_binding(AnimatedBindingCallable::<T, _> {
                original_binding,
                state: Cell::new(AnimatedBindingState::NotAnimating),
                animation_data,
                compute_animation_details: || -> AnimationDetail { None },
            });
        }
        handle.0.mark_dirty();
    }

    /// Internal function to set up a property animation between values produced by the specified binding for an integer property.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_animated_binding_int(
        handle: &PropertyHandleOpaque,
        binding: extern "C" fn(*mut c_void, *mut i32),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
        animation_data: Option<&PropertyAnimation>,
        transition_data: Option<
            extern "C" fn(user_data: *mut c_void, start_instant: &mut u64) -> PropertyAnimation,
        >,
    ) {
        c_set_animated_binding(
            handle,
            binding,
            user_data,
            drop_user_data,
            animation_data,
            transition_data,
        );
    }

    /// Internal function to set up a property animation between values produced by the specified binding for a float property.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_animated_binding_float(
        handle: &PropertyHandleOpaque,
        binding: extern "C" fn(*mut c_void, *mut f32),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
        animation_data: Option<&PropertyAnimation>,
        transition_data: Option<
            extern "C" fn(user_data: *mut c_void, start_instant: &mut u64) -> PropertyAnimation,
        >,
    ) {
        c_set_animated_binding(
            handle,
            binding,
            user_data,
            drop_user_data,
            animation_data,
            transition_data,
        );
    }

    /// Internal function to set up a property animation between values produced by the specified binding for a color property.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_animated_binding_color(
        handle: &PropertyHandleOpaque,
        binding: extern "C" fn(*mut c_void, *mut Color),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
        animation_data: Option<&PropertyAnimation>,
        transition_data: Option<
            extern "C" fn(user_data: *mut c_void, start_instant: &mut u64) -> PropertyAnimation,
        >,
    ) {
        c_set_animated_binding(
            handle,
            binding,
            user_data,
            drop_user_data,
            animation_data,
            transition_data,
        );
    }

    /// Internal function to set up a property animation between values produced by the specified binding for a brush property.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_animated_binding_brush(
        handle: &PropertyHandleOpaque,
        binding: extern "C" fn(*mut c_void, *mut Brush),
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
        animation_data: Option<&PropertyAnimation>,
        transition_data: Option<
            extern "C" fn(user_data: *mut c_void, start_instant: &mut u64) -> PropertyAnimation,
        >,
    ) {
        c_set_animated_binding(
            handle,
            binding,
            user_data,
            drop_user_data,
            animation_data,
            transition_data,
        );
    }

    /// Internal function to set up a state binding on a Property<StateInfo>.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_set_state_binding(
        handle: &PropertyHandleOpaque,
        binding: extern "C" fn(*mut c_void) -> i32,
        user_data: *mut c_void,
        drop_user_data: Option<extern "C" fn(*mut c_void)>,
    ) {
        struct CStateBinding {
            binding: extern "C" fn(*mut c_void) -> i32,
            user_data: *mut c_void,
            drop_user_data: Option<extern "C" fn(*mut c_void)>,
        }

        impl Drop for CStateBinding {
            fn drop(&mut self) {
                if let Some(x) = self.drop_user_data {
                    x(self.user_data)
                }
            }
        }

        let c_state_binding = CStateBinding { binding, user_data, drop_user_data };
        let bind_callable = StateInfoBinding {
            dirty_time: Cell::new(None),
            binding: move || (c_state_binding.binding)(c_state_binding.user_data),
        };
        handle.0.set_binding(bind_callable)
    }

    #[repr(C)]
    /// Opaque type representing the PropertyTracker
    pub struct PropertyTrackerOpaque {
        dependencies: usize,
        dep_nodes: [usize; 2],
        vtable: usize,
        dirty: bool,
    }

    static_assertions::assert_eq_align!(PropertyTrackerOpaque, PropertyTracker);
    static_assertions::assert_eq_size!(PropertyTrackerOpaque, PropertyTracker);

    /// Initialize the first pointer of the PropertyTracker.
    /// `out` is assumed to be uninitialized
    /// sixtyfps_property_tracker_drop need to be called after that
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_tracker_init(out: *mut PropertyTrackerOpaque) {
        core::ptr::write(out as *mut PropertyTracker, PropertyTracker::default());
    }

    /// Call the callback with the user data. Any properties access within the callback will be registered.
    /// Any currently evaluated bindings or property trackers will be notified if accessed properties are changed.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_tracker_evaluate(
        handle: *const PropertyTrackerOpaque,
        callback: extern "C" fn(user_data: *mut c_void),
        user_data: *mut c_void,
    ) {
        Pin::new_unchecked(&*(handle as *const PropertyTracker)).evaluate(|| callback(user_data))
    }

    /// Call the callback with the user data. Any properties access within the callback will be registered.
    /// Any currently evaluated bindings or property trackers will be not notified if accessed properties are changed.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_tracker_evaluate_as_dependency_root(
        handle: *const PropertyTrackerOpaque,
        callback: extern "C" fn(user_data: *mut c_void),
        user_data: *mut c_void,
    ) {
        Pin::new_unchecked(&*(handle as *const PropertyTracker))
            .evaluate_as_dependency_root(|| callback(user_data))
    }
    /// Query if the property tracker is dirty
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_tracker_is_dirty(
        handle: *const PropertyTrackerOpaque,
    ) -> bool {
        (*(handle as *const PropertyTracker)).is_dirty()
    }

    /// Destroy handle
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_property_tracker_drop(handle: *mut PropertyTrackerOpaque) {
        core::ptr::drop_in_place(handle as *mut PropertyTracker);
    }
}
