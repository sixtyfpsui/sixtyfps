/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! implementation of vtable::Vrc

use super::*;
use core::convert::TryInto;
use core::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

/// This trait is implemented by the `#[vtable]` macro.
///
/// It is implemented if the macro has a "drop_in_place" function.
///
/// Safety: the implementation of drop_in_place and dealloc must be correct
pub unsafe trait VTableMetaDropInPlace: VTableMeta {
    /// # Safety
    /// The target ptr argument needs to be pointing to a an instance of the VTable
    /// after the call to this function, the memory is still there but no longer contains
    /// a valid object.
    unsafe fn drop_in_place(vtable: &Self::VTable, ptr: *mut u8) -> vrc::Layout;
    /// # Safety
    /// The target ptr must have been allocated by the same allocator as the
    /// one which the vtable will delegate to.
    unsafe fn dealloc(vtable: &Self::VTable, ptr: *mut u8, layout: vrc::Layout);
}

/// This is a marker type to be used in [`VRc`] and [`VWeak`] to mean that the
/// actual type is not known.
pub struct Dyn(());

/// Similar to [`std::alloc::Layout`], but `repr(C)`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Layout {
    /// The size in bytes
    pub size: usize,
    /// The minimum alignment in bytes
    pub align: usize,
}

impl From<core::alloc::Layout> for Layout {
    fn from(layout: core::alloc::Layout) -> Self {
        Self { size: layout.size(), align: layout.align() }
    }
}

impl core::convert::TryFrom<Layout> for core::alloc::Layout {
    type Error = core::alloc::LayoutError;

    fn try_from(value: Layout) -> Result<Self, Self::Error> {
        Self::from_size_align(value.size, value.align)
    }
}

#[repr(C)]
struct VRcInner<'vt, VTable: VTableMeta, X> {
    vtable: &'vt VTable::VTable,
    /// The amount of VRc pointing to this object. When it reaches 0, the object will be dropped
    strong_ref: AtomicU32,
    /// The amount of VWeak +1. When it reaches 0, the memory will be deallocated.
    /// The +1 is there such as all the VRc together hold a weak reference to the memory
    weak_ref: AtomicU32,
    /// offset to the data from the beginning of VRcInner. This is needed to cast a VRcInner<VT, X>
    /// to VRcInner<VT, u8> as "dyn VRc" and then still be able to get the correct data pointer,
    /// since the alignment of X may not be the same as u8.
    data_offset: u16,
    /// Actual data, or an instance of `Self::Layout` iff `strong_ref == 0`.
    /// Can be seen as `union {data: X, layout: Layout}`  (but that's not stable)
    data: X,
}

impl<'vt, VTable: VTableMeta, X> VRcInner<'vt, VTable, X> {
    fn data_ptr(&self) -> *const X {
        let ptr = self as *const Self as *const u8;
        unsafe { ptr.add(self.data_offset as usize) as *const X }
    }
    fn as_ref(&self) -> &X {
        let ptr = self as *const Self as *const u8;
        unsafe { &*(ptr.add(self.data_offset as usize) as *const X) }
    }
}

/// A reference counted pointer to an object matching the virtual table `T`
///
/// Similar to [`std::rc::Rc`] where the `VTable` type parameter is a VTable struct
/// annotated with `#[vtable]`, and the `X` type parameter is the actual instance.
/// When `X` is the [`Dyn`] type marker, this means that the X is not known and the only
/// thing that can be done is to get a [`VRef<VTable>`] through the [`Self::borrow()`] function.
///
/// Other differences with the `std::rc::Rc` types are:
/// - It does not allow to access mutable reference. (No `get_mut` or `make_mut`), meaning it is
/// safe to get a Pin reference with `borrow_pin`.
/// - It is safe to pass it across ffi boundaries.
#[repr(transparent)]
pub struct VRc<VTable: VTableMetaDropInPlace + 'static, X = Dyn> {
    inner: NonNull<VRcInner<'static, VTable, X>>,
}

impl<VTable: VTableMetaDropInPlace + 'static, X> Drop for VRc<VTable, X> {
    fn drop(&mut self) {
        unsafe {
            let inner = self.inner.as_ref();
            if inner.strong_ref.fetch_sub(1, Ordering::SeqCst) == 1 {
                let data = inner.data_ptr() as *const _ as *const u8 as *mut u8;
                let mut layout = VTable::drop_in_place(inner.vtable, data);
                layout = core::alloc::Layout::new::<VRcInner<VTable, ()>>()
                    .extend(layout.try_into().unwrap())
                    .unwrap()
                    .0
                    .pad_to_align()
                    .into();
                if inner.weak_ref.load(Ordering::SeqCst) > 1 {
                    // at this point we are sure that no other thread can access the data
                    // since we still hold a weak reference, so the other weak references
                    // in other thread won't start destroying the object.
                    self.inner.cast::<VRcInner<VTable, Layout>>().as_mut().data = layout;
                }
                if inner.weak_ref.fetch_sub(1, Ordering::SeqCst) == 1 {
                    let vtable = inner.vtable;
                    VTable::dealloc(vtable, self.inner.cast().as_ptr(), layout);
                }
            }
        }
    }
}

impl<VTable: VTableMetaDropInPlace, X: HasStaticVTable<VTable>> VRc<VTable, X> {
    /// Create a new VRc from an instance of a type that can be associated with a VTable.
    ///
    /// Will move the instance on the heap.
    ///
    /// (the `HasStaticVTable` is implemented by the `“MyTrait”VTable_static!` macro generated by
    /// the #[vtable] macro)
    pub fn new(data: X) -> Self {
        let layout = core::alloc::Layout::new::<VRcInner<VTable, X>>().pad_to_align();
        // We must ensure the size is enough to hold a Layout when strong_count becomes 0
        let layout_with_layout = core::alloc::Layout::new::<VRcInner<VTable, Layout>>();
        let layout = core::alloc::Layout::from_size_align(
            layout.size().max(layout_with_layout.size()),
            layout.align().max(layout_with_layout.align()),
        )
        .unwrap();
        let mem = unsafe { std::alloc::alloc(layout) as *mut VRcInner<VTable, X> };
        let inner = NonNull::new(mem).unwrap();
        assert!(!mem.is_null());

        unsafe {
            mem.write(VRcInner {
                vtable: X::static_vtable(),
                strong_ref: AtomicU32::new(1),
                weak_ref: AtomicU32::new(1), // All the VRc together hold a weak_ref to the memory
                data_offset: 0,
                data,
            });
            (*mem).data_offset =
                (&(*mem).data as *const _ as usize - mem as *const _ as usize) as u16;
            VRc { inner }
        }
    }

    /// Convert a VRc of a real instance to a VRc of a Dyn instance
    pub fn into_dyn(this: Self) -> VRc<VTable, Dyn>
    where
        Self: 'static,
    {
        // Safety: they have the exact same representation: just a pointer to the same structure.
        // no Drop will be called here, so no need to increment any ref count
        unsafe { core::mem::transmute(this) }
    }
}

impl<VTable: VTableMetaDropInPlace, X> VRc<VTable, X> {
    /// Create a Pinned reference to the inner.
    ///
    /// This is safe because we don't allow mutable reference to the inner
    pub fn as_pin_ref(&self) -> Pin<&X> {
        unsafe { Pin::new_unchecked(&*self) }
    }

    /// Gets a VRef pointing to this instance
    pub fn borrow(this: &Self) -> VRef<'_, VTable> {
        unsafe {
            let inner = this.inner.cast::<VRcInner<VTable, u8>>();
            VRef::from_raw(
                NonNull::from(inner.as_ref().vtable),
                NonNull::from(&*inner.as_ref().data_ptr()),
            )
        }
    }

    /// Gets a Pin<VRef> pointing to this instance
    ///
    /// This is safe because there is no way to access a mutable reference to the pointee.
    /// (There is no `get_mut` or `make_mut`),
    pub fn borrow_pin<'b>(this: &'b Self) -> Pin<VRef<'b, VTable>> {
        unsafe { Pin::new_unchecked(Self::borrow(this)) }
    }

    /// Construct a [`VWeak`] pointing to this instance.
    pub fn downgrade(this: &Self) -> VWeak<VTable, X> {
        let inner = unsafe { this.inner.as_ref() };
        inner.weak_ref.fetch_add(1, Ordering::SeqCst);
        VWeak { inner: Some(this.inner) }
    }

    /// Gets the number of strong (VRc) pointers to this allocation.
    pub fn strong_count(this: &Self) -> usize {
        unsafe { this.inner.as_ref().strong_ref.load(Ordering::SeqCst) as usize }
    }

    /// Returns true if the two VRc's point to the same allocation
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.inner == other.inner
    }
}

impl<VTable: VTableMetaDropInPlace + 'static, X> Clone for VRc<VTable, X> {
    fn clone(&self) -> Self {
        let inner = unsafe { self.inner.as_ref() };
        inner.strong_ref.fetch_add(1, Ordering::SeqCst);
        Self { inner: self.inner }
    }
}

impl<VTable: VTableMetaDropInPlace, X /*+ HasStaticVTable<VTable>*/> Deref for VRc<VTable, X> {
    type Target = X;
    fn deref(&self) -> &Self::Target {
        let inner = unsafe { self.inner.as_ref() };
        inner.as_ref()
    }
}

// Safety: we use atomic reference count for the internal things
unsafe impl<VTable: VTableMetaDropInPlace + 'static, X: Send + Sync> Send for VRc<VTable, X> {}
unsafe impl<VTable: VTableMetaDropInPlace + 'static, X: Send + Sync> Sync for VRc<VTable, X> {}

/// Weak pointer for the [`VRc`] where `VTable` is a VTable struct, and
/// `X` is the type of the instance, or [`Dyn`] if it is not known
///
/// Similar to [`std::rc::Weak`].
///
/// Can be constructed with [`VRc::downgrade`] and use [`VWeak::upgrade`]
/// to re-create the original VRc.
#[repr(transparent)]
pub struct VWeak<VTable: VTableMetaDropInPlace + 'static, X = Dyn> {
    inner: Option<NonNull<VRcInner<'static, VTable, X>>>,
}

impl<VTable: VTableMetaDropInPlace + 'static, X> Default for VWeak<VTable, X> {
    fn default() -> Self {
        Self { inner: None }
    }
}

impl<VTable: VTableMetaDropInPlace + 'static, X> Clone for VWeak<VTable, X> {
    fn clone(&self) -> Self {
        if let Some(inner) = self.inner {
            let inner = unsafe { inner.as_ref() };
            inner.weak_ref.fetch_add(1, Ordering::SeqCst);
        }
        VWeak { inner: self.inner }
    }
}

impl<T: VTableMetaDropInPlace + 'static, X> Drop for VWeak<T, X> {
    fn drop(&mut self) {
        if let Some(i) = self.inner {
            let inner = unsafe { i.as_ref() };
            if inner.weak_ref.fetch_sub(1, Ordering::SeqCst) == 1 {
                let vtable = inner.vtable;
                unsafe {
                    // Safety: while allocating, we made sure that the size was big enough to
                    // hold a VRcInner<T, Layout>.
                    let layout = i.cast::<VRcInner<T, Layout>>().as_ref().data;
                    T::dealloc(vtable, i.cast().as_ptr(), layout);
                }
            }
        }
    }
}

impl<VTable: VTableMetaDropInPlace + 'static, X> VWeak<VTable, X> {
    /// Returns a new `VRc` if some other instance still holds a strong reference to this item.
    /// Otherwise, returns None.
    pub fn upgrade(&self) -> Option<VRc<VTable, X>> {
        if let Some(i) = self.inner {
            let inner = unsafe { i.as_ref() };
            if inner.strong_ref.load(Ordering::SeqCst) == 0 {
                None
            } else {
                inner.strong_ref.fetch_add(1, Ordering::SeqCst);
                Some(VRc { inner: i })
            }
        } else {
            None
        }
    }
}

impl<VTable: VTableMetaDropInPlace + 'static, X: HasStaticVTable<VTable> + 'static>
    VWeak<VTable, X>
{
    /// Convert a VRc of a real instance to a VRc of a Dyn instance
    pub fn into_dyn(self) -> VWeak<VTable, Dyn> {
        // Safety: they have the exact same representation: just a pointer to the same structure.
        // no Drop will be called here, so no need to increment any ref count
        unsafe { core::mem::transmute(self) }
    }
}
