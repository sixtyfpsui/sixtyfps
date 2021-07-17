/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
//! module for the SharedString and related things

#![allow(unsafe_code)]
#![warn(missing_docs)]

use core::mem::MaybeUninit;
use std::{fmt::Debug, fmt::Display, ops::Deref};
use triomphe::{Arc, HeaderWithLength, ThinArc};

/// A string type used by the SixtyFPS run-time.
///
/// SharedString uses implicit data sharing to make it efficient to pass around copies. When
/// cloning, a reference to the data is cloned, not the data itself. The data itself is only copied
/// when modifying it, for example using [push_str](#method.push_str). This is also called copy-on-write.
///
/// Under the hood the string data is UTF-8 encoded and it is always terminated with a null character.
#[derive(Clone)]
#[repr(C)]
pub struct SharedString {
    /// Invariant: The usize header is the `len` of the vector, the contained buffer is `[MaybeUninit<u8>]`
    /// `buffer[0..=len]` is initialized and valid utf8, and `buffer[len]` is '\0'
    inner: ThinArc<usize, MaybeUninit<u8>>,
}

impl SharedString {
    fn as_ptr(&self) -> *const u8 {
        self.inner.slice.as_ptr() as *const u8
    }

    /// Size of the string, in bytes. This excludes the terminating null character.
    pub fn len(&self) -> usize {
        self.inner.header.header
    }

    /// Return a slice to the string
    pub fn as_str(&self) -> &str {
        unsafe {
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.as_ptr(), self.len()))
        }
    }

    /// Append a string to this string
    ///
    /// ```
    /// # use sixtyfps_corelib::SharedString;
    /// let mut hello = SharedString::from("Hello");
    /// hello.push_str(", ");
    /// hello.push_str("World");
    /// hello.push_str("!");
    /// assert_eq!(hello, "Hello, World!");
    /// ```
    pub fn push_str(&mut self, x: &str) {
        let new_len = self.inner.header.header + x.len();
        if new_len + 1 < self.inner.slice.len() {
            let mut arc = Arc::from_thin(self.inner.clone());
            if let Some(inner) = Arc::get_mut(&mut arc) {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        x.as_ptr(),
                        inner.slice.as_mut_ptr().add(inner.header.header) as *mut u8,
                        x.len(),
                    );
                }
                inner.slice[new_len] = MaybeUninit::new(0);
                inner.header.header = new_len;
                return;
            }
        }
        // re-alloc

        struct ReallocIter<'a> {
            pos: usize,
            new_alloc: usize,
            first: &'a [MaybeUninit<u8>],
            second: &'a [u8],
        }

        impl<'a> Iterator for ReallocIter<'a> {
            type Item = MaybeUninit<u8>;
            fn next(&mut self) -> Option<MaybeUninit<u8>> {
                let mut pos = self.pos;
                if pos >= self.new_alloc {
                    return None;
                }

                self.pos += 1;

                if pos < self.first.len() {
                    return Some(self.first[pos]);
                }
                pos -= self.first.len();
                if pos < self.second.len() {
                    return Some(MaybeUninit::new(self.second[pos]));
                }
                pos -= self.second.len();
                if pos == 0 {
                    return Some(MaybeUninit::new(0));
                }
                // I don't know if the compiler will be smart enough to exit the loop here.
                // It would be nice if triomphe::Arc would allow to leave uninitialized memory
                Some(MaybeUninit::uninit())
            }
            fn size_hint(&self) -> (usize, Option<usize>) {
                (self.new_alloc, Some(self.new_alloc))
            }
        }
        impl<'a> core::iter::ExactSizeIterator for ReallocIter<'a> {}

        let align = core::mem::align_of::<usize>();
        let new_alloc = new_len + new_len / 2; // add some growing factor
        let new_alloc = (new_alloc + align) & !(align - 1);
        let iter = ReallocIter {
            pos: 0,
            first: &self.inner.slice[0..self.inner.header.header],
            second: x.as_bytes(),
            new_alloc,
        };

        self.inner = Arc::into_thin(Arc::from_header_and_iter(
            HeaderWithLength::new(new_len, new_alloc),
            iter,
        ));
    }
}

impl Deref for SharedString {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Default for SharedString {
    fn default() -> Self {
        // Unfortunately, the Arc constructor is not const, so we must use a Lazy static for that
        static NULL: once_cell::sync::Lazy<ThinArc<usize, MaybeUninit<u8>>> =
            once_cell::sync::Lazy::new(|| {
                Arc::into_thin(Arc::from_header_and_iter(
                    HeaderWithLength::new(0, core::mem::align_of::<usize>()),
                    [MaybeUninit::new(0); core::mem::align_of::<usize>()].iter().cloned(),
                ))
            });

        SharedString { inner: NULL.clone() }
    }
}

impl From<&str> for SharedString {
    fn from(value: &str) -> Self {
        struct AddNullIter<'a> {
            pos: usize,
            str: &'a [u8],
        }

        impl<'a> Iterator for AddNullIter<'a> {
            type Item = MaybeUninit<u8>;
            fn next(&mut self) -> Option<MaybeUninit<u8>> {
                let pos = self.pos;
                self.pos += 1;
                let align = core::mem::align_of::<usize>();
                if pos < self.str.len() {
                    Some(MaybeUninit::new(self.str[pos]))
                } else if pos < (self.str.len() + align) & !(align - 1) {
                    Some(MaybeUninit::new(0))
                } else {
                    None
                }
            }
            fn size_hint(&self) -> (usize, Option<usize>) {
                let l = self.str.len() + 1;
                // add some padding at the end since the size of the inner will anyway have to be padded
                let align = core::mem::align_of::<usize>();
                let l = (l + align - 1) & !(align - 1);
                let l = l - self.pos;
                (l, Some(l))
            }
        }
        impl<'a> core::iter::ExactSizeIterator for AddNullIter<'a> {}

        let iter = AddNullIter { str: value.as_bytes(), pos: 0 };

        SharedString {
            inner: Arc::into_thin(Arc::from_header_and_iter(
                HeaderWithLength::new(value.len(), iter.size_hint().0),
                iter,
            )),
        }
    }
}

impl Debug for SharedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self.as_str(), f)
    }
}

impl Display for SharedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.as_str(), f)
    }
}

impl AsRef<str> for SharedString {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<std::ffi::CStr> for SharedString {
    #[inline]
    fn as_ref(&self) -> &std::ffi::CStr {
        unsafe {
            std::ffi::CStr::from_bytes_with_nul_unchecked(core::slice::from_raw_parts(
                self.as_ptr(),
                self.len() + 1,
            ))
        }
    }
}

impl AsRef<[u8]> for SharedString {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_str().as_bytes()
    }
}

impl<T> PartialEq<T> for SharedString
where
    T: ?Sized + AsRef<str>,
{
    fn eq(&self, other: &T) -> bool {
        self.as_str() == other.as_ref()
    }
}
impl Eq for SharedString {}

impl<T> PartialOrd<T> for SharedString
where
    T: ?Sized + AsRef<str>,
{
    fn partial_cmp(&self, other: &T) -> Option<std::cmp::Ordering> {
        PartialOrd::partial_cmp(self.as_str(), other.as_ref())
    }
}
impl Ord for SharedString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        Ord::cmp(self.as_str(), other.as_str())
    }
}

impl From<String> for SharedString {
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}

impl From<&String> for SharedString {
    fn from(s: &String) -> Self {
        s.as_str().into()
    }
}

impl Into<String> for SharedString {
    fn into(self) -> String {
        self.as_str().to_string()
    }
}

impl core::ops::AddAssign<&str> for SharedString {
    fn add_assign(&mut self, other: &str) {
        self.push_str(other);
    }
}

impl core::ops::Add<&str> for SharedString {
    type Output = SharedString;
    fn add(mut self, other: &str) -> SharedString {
        self.push_str(other);
        self
    }
}

impl std::hash::Hash for SharedString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

#[test]
fn simple_test() {
    let x = SharedString::from("hello world!");
    assert_eq!(x, "hello world!");
    assert_ne!(x, "hello world?");
    assert_eq!(x, x.clone());
    assert_eq!("hello world!", x.as_str());
    let string = String::from("hello world!");
    assert_eq!(x, string);
    assert_eq!(x.to_string(), string);
    let def = SharedString::default();
    assert_eq!(def, SharedString::default());
    assert_ne!(def, x);
    assert_eq!(
        (&x as &dyn AsRef<std::ffi::CStr>).as_ref(),
        &*std::ffi::CString::new("hello world!").unwrap()
    );
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    use super::*;

    /// for cbindgen.
    #[allow(non_camel_case_types)]
    type c_char = u8;

    #[no_mangle]
    /// Returns a nul-terminated pointer for this string.
    /// The returned value is owned by the string, and should not be used after any
    /// mutable function have been called on the string, and must not be freed.
    pub extern "C" fn sixtyfps_shared_string_bytes(ss: &SharedString) -> *const c_char {
        ss.as_ptr()
    }

    #[no_mangle]
    /// Destroy the shared string
    pub unsafe extern "C" fn sixtyfps_shared_string_drop(ss: *const SharedString) {
        core::ptr::read(ss);
    }

    #[no_mangle]
    /// Increment the reference count of the string.
    /// The resulting structure must be passed to sixtyfps_shared_string_drop
    pub unsafe extern "C" fn sixtyfps_shared_string_clone(
        out: *mut SharedString,
        ss: &SharedString,
    ) {
        core::ptr::write(out, ss.clone())
    }

    #[no_mangle]
    /// Safety: bytes must be a valid utf-8 string of size len without null inside.
    /// The resulting structure must be passed to sixtyfps_shared_string_drop
    pub unsafe extern "C" fn sixtyfps_shared_string_from_bytes(
        out: *mut SharedString,
        bytes: *const c_char,
        len: usize,
    ) {
        let str = core::str::from_utf8(core::slice::from_raw_parts(bytes, len)).unwrap();
        core::ptr::write(out, SharedString::from(str));
    }

    /// Create a string from a number.
    /// The resulting structure must be passed to sixtyfps_shared_string_drop
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_shared_string_from_number(out: *mut SharedString, n: f64) {
        // TODO: implement Write for SharedString so this can be done without allocation
        let str = format!("{}", n);
        core::ptr::write(out, SharedString::from(str.as_str()));
    }

    #[test]
    fn test_sixtyfps_shared_string_from_number() {
        unsafe {
            let mut s = core::mem::MaybeUninit::uninit();
            sixtyfps_shared_string_from_number(s.as_mut_ptr(), 45.);
            assert_eq!(s.assume_init(), "45");

            let mut s = core::mem::MaybeUninit::uninit();
            sixtyfps_shared_string_from_number(s.as_mut_ptr(), 45.12);
            assert_eq!(s.assume_init(), "45.12");

            let mut s = core::mem::MaybeUninit::uninit();
            sixtyfps_shared_string_from_number(s.as_mut_ptr(), -1325466.);
            assert_eq!(s.assume_init(), "-1325466");

            let mut s = core::mem::MaybeUninit::uninit();
            sixtyfps_shared_string_from_number(s.as_mut_ptr(), -0.);
            assert_eq!(s.assume_init(), "0");
        }
    }

    /// Append some bytes to an existing shared string
    ///
    /// bytes must be a valid utf8 array of size `len`, without null bytes inside
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_shared_string_append(
        self_: &mut SharedString,
        bytes: *const c_char,
        len: usize,
    ) {
        let str = core::str::from_utf8(core::slice::from_raw_parts(bytes, len)).unwrap();
        self_.push_str(str);
    }
    #[test]
    fn test_sixtyfps_shared_string_append() {
        let mut s = SharedString::default();
        let mut append = |x: &str| unsafe {
            sixtyfps_shared_string_append(&mut s, x.as_bytes().as_ptr(), x.len());
        };
        append("Hello");
        append(", ");
        append("world");
        append("!");
        assert_eq!(s.as_str(), "Hello, world!");
    }
}
