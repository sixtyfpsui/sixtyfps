/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! FFI-friendly slice

#![allow(unsafe_code)]
#![warn(missing_docs)]

use core::{cmp::PartialEq, fmt::Debug, marker::PhantomData, ptr::NonNull};

/// That's basically the same as `&'a [T]`  but `repr(C)`
///
/// Can be constructed from a slice using the from trait.
///
/// ```
/// use sixtyfps_corelib::slice::Slice;
/// let x = Slice::from_slice(&[1, 2, 3]);
/// assert_eq!(x.len(), 3);
/// assert_eq!(x[1], 2);
/// let slice : &'static [u32] = x.as_slice();
/// ```
///
/// Comparing two Slice compare their pointer, not the content.
/// ```
/// use sixtyfps_corelib::slice::Slice;
/// let a = Slice::from_slice(&[1, 2, 3]);
/// let slice = [1, 2, 3, 4];
/// let b = Slice::from(&slice[..3]);
/// // two slice coming from the same pointer are equal.
/// assert_eq!(b, Slice::from(&slice[..3]));
/// // these are different because the pointers are different
/// assert_ne!(a, b);
/// // use as_slice to compare the contents
/// assert_eq!(a.as_slice(), b.as_slice());
/// ```
#[repr(C)]
#[derive(PartialEq)]
pub struct Slice<'a, T> {
    /// Invariant, this is a valid slice of len `len`
    ptr: NonNull<T>,
    len: usize,
    phantom: PhantomData<&'a [T]>,
}

impl<'a, T: Debug> Debug for Slice<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_slice().fmt(f)
    }
}

// Need to implement manually otherwise it is not implemented if T do not implement Copy / Clone
impl<'a, T> Copy for Slice<'a, T> {}

impl<'a, T> Clone for Slice<'a, T> {
    fn clone(&self) -> Self {
        Self { ptr: self.ptr, len: self.len, phantom: PhantomData }
    }
}

impl<'a, T> Slice<'a, T> {
    /// Return a slice
    pub fn as_slice(self) -> &'a [T] {
        // Safety: it ptr is supposed to be a valid slice of given length
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// Create from a native slice
    pub fn from_slice(x: &'a [T]) -> Self {
        x.into()
    }
}

impl<'a, T> From<&'a [T]> for Slice<'a, T> {
    fn from(slice: &'a [T]) -> Self {
        Slice { ptr: NonNull::from(slice).cast(), len: slice.len(), phantom: PhantomData }
    }
}

impl<'a, T> core::ops::Deref for Slice<'a, T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<'a, T> Default for Slice<'a, T> {
    fn default() -> Self {
        Self::from_slice(&[])
    }
}
