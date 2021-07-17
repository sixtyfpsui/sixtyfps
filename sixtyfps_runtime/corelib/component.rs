/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#![warn(missing_docs)]

//! This module contains the basic datastructures that are exposed to the C API

use crate::item_tree::{ItemVisitorVTable, TraversalOrder, VisitChildrenResult};
use crate::items::{ItemVTable, ItemWeak};
use crate::layout::{LayoutInfo, Orientation};
use crate::window::ComponentWindow;
use vtable::*;

/// A Component is representing an unit that is allocated together
#[vtable]
#[repr(C)]
pub struct ComponentVTable {
    /// Visit the children of the item at index `index`.
    /// Note that the root item is at index 0, so passing 0 would visit the item under root (the children of root).
    /// If you want to visit the root item, you need to pass -1 as an index.
    pub visit_children_item: extern "C" fn(
        core::pin::Pin<VRef<ComponentVTable>>,
        index: isize,
        order: TraversalOrder,
        visitor: VRefMut<ItemVisitorVTable>,
    ) -> VisitChildrenResult,

    /// Return a reference to an item using the given index
    pub get_item_ref: extern "C" fn(
        core::pin::Pin<VRef<ComponentVTable>>,
        index: usize,
    ) -> core::pin::Pin<VRef<ItemVTable>>,

    /// Return the parent item.
    /// The return value is an item weak because it can be null if there is no parent.
    /// And the return value is passed by &mut because ItemWeak has a destructor
    pub parent_item:
        extern "C" fn(core::pin::Pin<VRef<ComponentVTable>>, index: usize, result: &mut ItemWeak),

    /// Returns the layout info for this component
    pub layout_info:
        extern "C" fn(core::pin::Pin<VRef<ComponentVTable>>, Orientation) -> LayoutInfo,

    /// in-place destructor (for VRc)
    pub drop_in_place: unsafe fn(VRefMut<ComponentVTable>) -> vtable::Layout,
    /// dealloc function (for VRc)
    pub dealloc: unsafe fn(&ComponentVTable, ptr: *mut u8, layout: vtable::Layout),
}

/// Alias for `vtable::VRef<ComponentVTable>` which represent a pointer to a `dyn Component` with
/// the associated vtable
pub type ComponentRef<'a> = vtable::VRef<'a, ComponentVTable>;

/// Type alias to the commonly used `Pin<VRef<ComponentVTable>>>`
pub type ComponentRefPin<'a> = core::pin::Pin<ComponentRef<'a>>;

/// Type alias to the commonly used VRc<ComponentVTable, Dyn>>
pub type ComponentRc = vtable::VRc<ComponentVTable, Dyn>;
/// Type alias to the commonly used VWeak<ComponentVTable, Dyn>>
pub type ComponentWeak = vtable::VWeak<ComponentVTable, Dyn>;

/// Call init() on the ItemVTable for each item of the component.
pub fn init_component_items<Base>(
    base: core::pin::Pin<&Base>,
    item_tree: &[crate::item_tree::ItemTreeNode<Base>],
    window: &ComponentWindow,
) {
    item_tree.iter().for_each(|entry| match entry {
        crate::item_tree::ItemTreeNode::Item { item, .. } => {
            item.apply_pin(base).as_ref().init(window)
        }
        crate::item_tree::ItemTreeNode::DynamicTree { .. } => {}
    })
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    #![allow(unsafe_code)]

    use super::*;
    use crate::item_tree::*;
    use crate::slice::Slice;

    /// Call init() on the ItemVTable of each item of the component.
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_component_init_items(
        component: ComponentRefPin,
        item_tree: Slice<ItemTreeNode<u8>>,
        window_handle: *const crate::window::ffi::ComponentWindowOpaque,
    ) {
        let window = &*(window_handle as *const ComponentWindow);
        super::init_component_items(
            core::pin::Pin::new_unchecked(&*(component.as_ptr() as *const u8)),
            item_tree.as_slice(),
            window,
        )
    }
}
