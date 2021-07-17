/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! module for the SharedVector and related things
#![allow(unsafe_code)]
#![warn(missing_docs)]
use core::fmt::Debug;
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::ptr::NonNull;
use core::sync::atomic;
use std::{alloc, iter::FromIterator};

#[repr(C)]
struct SharedVectorHeader {
    refcount: atomic::AtomicIsize,
    size: usize,
    capacity: usize,
}

#[repr(C)]
struct SharedVectorInner<T> {
    header: SharedVectorHeader,
    data: MaybeUninit<T>,
}

fn compute_inner_layout<T>(capacity: usize) -> alloc::Layout {
    alloc::Layout::new::<SharedVectorHeader>()
        .extend(alloc::Layout::array::<T>(capacity).unwrap())
        .unwrap()
        .0
}

unsafe fn drop_inner<T>(mut inner: NonNull<SharedVectorInner<T>>) {
    debug_assert_eq!(inner.as_ref().header.refcount.load(core::sync::atomic::Ordering::Relaxed), 0);
    let data_ptr = inner.as_mut().data.as_mut_ptr();
    for x in 0..inner.as_ref().header.size {
        core::ptr::drop_in_place(data_ptr.add(x));
    }
    alloc::dealloc(
        inner.as_ptr() as *mut u8,
        compute_inner_layout::<T>(inner.as_ref().header.capacity),
    )
}

/// Allocate the memory for the SharedVector with the given capacity. Return the inner with size and refcount set to 1
fn alloc_with_capacity<T>(capacity: usize) -> NonNull<SharedVectorInner<T>> {
    let ptr = unsafe { alloc::alloc(compute_inner_layout::<T>(capacity)) };
    assert!(!ptr.is_null(), "allocation of {:?} bytes failed", capacity);
    unsafe {
        core::ptr::write(
            ptr as *mut SharedVectorHeader,
            SharedVectorHeader { refcount: 1.into(), size: 0, capacity },
        );
    }
    NonNull::new(ptr).unwrap().cast()
}

/// Return a new capacity suitable for this vector
/// Loosely based on alloc::raw_vec::RawVec::grow_amortized.
fn capacity_for_grow(current_cap: usize, required_cap: usize, elem_size: usize) -> usize {
    if current_cap >= elem_size {
        return current_cap;
    }
    let cap = core::cmp::max(current_cap * 2, required_cap);
    let min_non_zero_cap = if elem_size == 1 {
        8
    } else if elem_size <= 1024 {
        4
    } else {
        1
    };
    core::cmp::max(min_non_zero_cap, cap)
}

#[repr(C)]
/// SharedVector holds a reference-counted read-only copy of `[T]`.
pub struct SharedVector<T> {
    inner: NonNull<SharedVectorInner<T>>,
}

impl<T> Drop for SharedVector<T> {
    fn drop(&mut self) {
        unsafe {
            if self
                .inner
                .cast::<SharedVectorHeader>()
                .as_ref()
                .refcount
                .load(atomic::Ordering::Relaxed)
                < 0
            {
                return;
            }
            if self.inner.as_ref().header.refcount.fetch_sub(1, atomic::Ordering::SeqCst) == 1 {
                drop_inner(self.inner)
            }
        }
    }
}

impl<T> Clone for SharedVector<T> {
    fn clone(&self) -> Self {
        unsafe {
            if self
                .inner
                .cast::<SharedVectorHeader>()
                .as_ref()
                .refcount
                .load(atomic::Ordering::Relaxed)
                > 0
            {
                self.inner.as_ref().header.refcount.fetch_add(1, atomic::Ordering::SeqCst);
            }
            return SharedVector { inner: self.inner };
        }
    }
}

impl<T> SharedVector<T> {
    /// Create a new empty array with a pre-allocated capacity in number of items
    pub fn with_capacity(capacity: usize) -> Self {
        Self { inner: alloc_with_capacity(capacity) }
    }

    fn as_ptr(&self) -> *const T {
        unsafe { self.inner.as_ref().data.as_ptr() }
    }

    /// Number of elements in the array
    pub fn len(&self) -> usize {
        unsafe { self.inner.cast::<SharedVectorHeader>().as_ref().size }
    }

    /// Return a slice to the array
    pub fn as_slice(&self) -> &[T] {
        if self.len() == 0 {
            &[]
        } else {
            // Safety: When len > 0, we know that the pointer holds an array of the size of len
            unsafe { core::slice::from_raw_parts(self.as_ptr(), self.len()) }
        }
    }

    /// Returns the number of elements the vector can hold without reallocating, when not shared
    fn capacity(&self) -> usize {
        unsafe { self.inner.cast::<SharedVectorHeader>().as_ref().capacity }
    }
}

impl<T: Clone> SharedVector<T> {
    /// Create a SharedVector from a slice
    pub fn from_slice(slice: &[T]) -> SharedVector<T> {
        Self::from(slice)
    }

    /// Ensure that the reference count is 1 so the array can be changed.
    /// If that's not tha case, the array will be cloned
    fn detach(&mut self, new_capacity: usize) {
        let is_shared =
            unsafe { self.inner.as_ref().header.refcount.load(atomic::Ordering::Relaxed) } != 1;
        if !is_shared && new_capacity <= self.capacity() {
            return;
        }
        let mut new_array = SharedVector::with_capacity(new_capacity);
        core::mem::swap(&mut self.inner, &mut new_array.inner);
        let mut size = 0;
        let mut iter = new_array.into_iter();
        while let Some(x) = iter.next() {
            assert_ne!(size, new_capacity);
            unsafe {
                core::ptr::write(self.inner.as_mut().data.as_mut_ptr().add(size), x);
                size += 1;
                self.inner.as_mut().header.size = size;
            }
            if size == new_capacity {
                break;
            }
        }
    }

    /// Return a mutable slice to the array. If the array was shared, this will make a copy of the array.
    pub fn as_slice_mut(&mut self) -> &mut [T] {
        self.detach(self.len());
        unsafe { core::slice::from_raw_parts_mut(self.as_ptr() as *mut T, self.len()) }
    }

    /// Add an element to the array. If the array was shared, this will make a copy of the array.
    pub fn push(&mut self, value: T) {
        self.detach(capacity_for_grow(self.capacity(), self.len() + 1, core::mem::size_of::<T>()));
        unsafe {
            core::ptr::write(
                self.inner.as_mut().data.as_mut_ptr().add(self.inner.as_mut().header.size),
                value,
            );
            self.inner.as_mut().header.size += 1;
        }
    }

    /// Resize the array to the given size.
    /// If the array was smaller new elements will be initialized with the value.
    /// If the array was bigger, extra elements will be discarded
    ///
    /// ```
    /// use sixtyfps_corelib::SharedVector;
    /// let mut shared_vector = SharedVector::<u32>::from_slice(&[1, 2, 3]);
    /// shared_vector.resize(5, 8);
    /// assert_eq!(shared_vector.as_slice(), &[1, 2, 3, 8, 8]);
    /// shared_vector.resize(2, 0);
    /// assert_eq!(shared_vector.as_slice(), &[1, 2]);
    /// ```
    pub fn resize(&mut self, new_len: usize, value: T) {
        if self.len() == new_len {
            return;
        }
        self.detach(new_len);
        // Safety: detach ensured that the array is not shared.
        let mut inner = unsafe { self.inner.as_mut() };

        if inner.header.size >= new_len {
            while inner.header.size > new_len {
                inner.header.size -= 1;
                // Safety: The array was of size inner.header.size, so there should be an element there
                unsafe {
                    core::ptr::drop_in_place(inner.data.as_mut_ptr().add(inner.header.size));
                }
            }
        } else {
            while inner.header.size < new_len {
                // Safety: The array must have a capacity of at least new_len because of the detach call earlier
                unsafe {
                    core::ptr::write(inner.data.as_mut_ptr().add(inner.header.size), value.clone());
                }
                inner.header.size += 1;
            }
        }
    }
}

impl<T> Deref for SharedVector<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

/* FIXME: is this a good idea to implement DerefMut knowing what it might detach?
impl<T> DerefMut for SharedVector<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_slice_mut()
    }
}*/

impl<T: Clone> From<&[T]> for SharedVector<T> {
    fn from(slice: &[T]) -> Self {
        let capacity = slice.len();
        let mut result = Self::with_capacity(capacity);
        for x in slice {
            unsafe {
                core::ptr::write(
                    result.inner.as_mut().data.as_mut_ptr().add(result.inner.as_mut().header.size),
                    x.clone(),
                );
                result.inner.as_mut().header.size += 1;
            }
        }
        result
    }
}

macro_rules! from_array {
    ($($n:literal)*) => { $(
        // FIXME: remove the Clone bound
        impl<T: Clone> From<[T; $n]> for SharedVector<T> {
            fn from(array: [T; $n]) -> Self {
                array.iter().cloned().collect()
            }
        }
    )+ };
}

from_array! {0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31}

impl<T> FromIterator<T> for SharedVector<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut iter = iter.into_iter();
        let mut capacity = iter.size_hint().0;
        let mut result = Self::with_capacity(capacity);
        let mut size = 0;
        while let Some(x) = iter.next() {
            if size >= capacity {
                capacity = capacity_for_grow(
                    capacity,
                    size + 1 + iter.size_hint().0,
                    core::mem::size_of::<T>(),
                );
                unsafe {
                    result.inner.as_ref().header.refcount.store(0, atomic::Ordering::Relaxed)
                };
                let mut iter = IntoIter(IntoIterInner::UnShared(result.inner, 0));
                result.inner = alloc_with_capacity::<T>(capacity);
                match &mut iter.0 {
                    IntoIterInner::UnShared(old_inner, begin) => {
                        while *begin < size {
                            unsafe {
                                core::ptr::write(
                                    result.inner.as_mut().data.as_mut_ptr().add(size),
                                    core::ptr::read(old_inner.as_ref().data.as_ptr().add(*begin)),
                                );
                                *begin += 1;
                                result.inner.as_mut().header.size = *begin;
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
            debug_assert_eq!(result.len(), size);
            debug_assert!(result.capacity() > size);
            unsafe {
                core::ptr::write(result.inner.as_mut().data.as_mut_ptr().add(size), x);
                size += 1;
                result.inner.as_mut().header.size = size;
            }
        }
        result
    }
}

impl<T: Clone> Extend<T> for SharedVector<T> {
    fn extend<X: IntoIterator<Item = T>>(&mut self, iter: X) {
        for item in iter {
            self.push(item);
        }
    }
}

static SHARED_NULL: SharedVectorHeader =
    SharedVectorHeader { refcount: std::sync::atomic::AtomicIsize::new(-1), size: 0, capacity: 0 };

impl<T> Default for SharedVector<T> {
    fn default() -> Self {
        SharedVector { inner: NonNull::from(&SHARED_NULL).cast() }
    }
}

impl<T: Debug> Debug for SharedVector<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl<T> AsRef<[T]> for SharedVector<T> {
    #[inline]
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T, U> PartialEq<U> for SharedVector<T>
where
    U: ?Sized + AsRef<[T]>,
    T: PartialEq,
{
    fn eq(&self, other: &U) -> bool {
        self.as_slice() == other.as_ref()
    }
}

impl<T: Eq> Eq for SharedVector<T> {}

impl<T: Clone> IntoIterator for SharedVector<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIter(unsafe {
            if self.inner.as_ref().header.refcount.load(atomic::Ordering::Relaxed) == 1 {
                let inner = self.inner;
                std::mem::forget(self);
                inner.as_ref().header.refcount.store(0, atomic::Ordering::Relaxed);
                IntoIterInner::UnShared(inner, 0)
            } else {
                IntoIterInner::Shared(self, 0)
            }
        })
    }
}

enum IntoIterInner<T> {
    Shared(SharedVector<T>, usize),
    // Elements up to the usize member are already moved out
    UnShared(NonNull<SharedVectorInner<T>>, usize),
}

impl<T> Drop for IntoIterInner<T> {
    fn drop(&mut self) {
        match self {
            IntoIterInner::Shared(..) => { /* drop of SharedVector takes care of it */ }
            IntoIterInner::UnShared(mut inner, begin) => unsafe {
                debug_assert_eq!(inner.as_ref().header.refcount.load(atomic::Ordering::Relaxed), 0);
                let data_ptr = inner.as_mut().data.as_mut_ptr();
                for x in (*begin)..inner.as_ref().header.size {
                    core::ptr::drop_in_place(data_ptr.add(x));
                }
                alloc::dealloc(
                    inner.as_ptr() as *mut u8,
                    compute_inner_layout::<T>(inner.as_ref().header.capacity),
                )
            },
        }
    }
}

/// An iterator that moves out of a SharedVector.
///
/// This `struct` is created by the `into_iter` method on [`SharedVector`] (provided
/// by the [`IntoIterator`] trait).
pub struct IntoIter<T>(IntoIterInner<T>);

impl<T: Clone> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            IntoIterInner::Shared(array, moved) => {
                let result = array.as_slice().get(*moved).cloned();
                *moved += 1;
                result
            }
            IntoIterInner::UnShared(inner, begin) => unsafe {
                if *begin < inner.as_ref().header.size {
                    let r = core::ptr::read(inner.as_ref().data.as_ptr().add(*begin));
                    *begin += 1;
                    Some(r)
                } else {
                    None
                }
            },
        }
    }
}

#[test]
fn simple_test() {
    let x: SharedVector<i32> = SharedVector::from([1, 2, 3]);
    let y: SharedVector<i32> = SharedVector::from([3, 2, 1]);
    assert_eq!(x, x.clone());
    assert_ne!(x, y);
    let z: [i32; 3] = [1, 2, 3];
    assert_eq!(z, x.as_slice());
    let vec: Vec<i32> = vec![1, 2, 3];
    assert_eq!(x, vec);
    let def: SharedVector<i32> = Default::default();
    assert_eq!(def, SharedVector::<i32>::default());
    assert_ne!(def, x);
}

#[test]
fn push_test() {
    let mut x: SharedVector<i32> = SharedVector::from([1, 2, 3]);
    let y = x.clone();
    x.push(4);
    x.push(5);
    x.push(6);
    assert_eq!(x.as_slice(), &[1, 2, 3, 4, 5, 6]);
    assert_eq!(y.as_slice(), &[1, 2, 3]);
}

#[test]
#[should_panic]
fn invalid_capacity_test() {
    let _: SharedVector<u8> = SharedVector::with_capacity(usize::MAX / 2 - 1000);
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    use super::*;

    #[no_mangle]
    /// This function is used for the low-level C++ interface to allocate the backing vector of a SharedVector.
    pub unsafe extern "C" fn sixtyfps_shared_vector_allocate(size: usize, align: usize) -> *mut u8 {
        std::alloc::alloc(std::alloc::Layout::from_size_align(size, align).unwrap())
    }

    #[no_mangle]
    /// This function is used for the low-level C++ interface to deallocate the backing vector of a SharedVector
    pub unsafe extern "C" fn sixtyfps_shared_vector_free(ptr: *mut u8, size: usize, align: usize) {
        std::alloc::dealloc(ptr, std::alloc::Layout::from_size_align(size, align).unwrap())
    }

    #[no_mangle]
    /// This function is used for the low-level C++ interface to initialize the empty SharedVector.
    pub unsafe extern "C" fn sixtyfps_shared_vector_empty() -> *const u8 {
        &SHARED_NULL as *const _ as *const u8
    }
}
