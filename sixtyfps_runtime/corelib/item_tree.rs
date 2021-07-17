/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use crate::component::{ComponentRc, ComponentVTable};
use crate::items::{ItemRef, ItemVTable};
use core::pin::Pin;
use vtable::*;

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TraversalOrder {
    BackToFront,
    FrontToBack,
}

/// The return value of the Component::visit_children_item function
///
/// Represents something like `enum { Continue, Aborted{aborted_at_item: isize} }`.
/// But this is just wrapping a int because it is easier to use ffi with isize than
/// complex enum.
///
/// -1 means the visitor will continue
/// otherwise this is the index of the item that aborted the visit.
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct VisitChildrenResult(i64);
impl VisitChildrenResult {
    /// The result used for a visitor that want to continue the visit
    pub const CONTINUE: Self = Self(-1);

    /// Returns a result that means that the visitor must stop, and convey the item that caused the abort
    pub fn abort(item_index: usize, index_within_repeater: usize) -> Self {
        assert!(item_index < i32::MAX as usize);
        assert!(index_within_repeater < i32::MAX as usize);
        Self(item_index as i64 | (index_within_repeater as i64) << 32)
    }
    /// True if the visitor wants to abort the visit
    pub fn has_aborted(&self) -> bool {
        self.0 != -1
    }
    pub fn aborted_index(&self) -> Option<usize> {
        if self.0 != -1 {
            Some((self.0 & 0xffff_ffff) as usize)
        } else {
            None
        }
    }
    pub fn aborted_indexes(&self) -> Option<(usize, usize)> {
        if self.0 != -1 {
            Some(((self.0 & 0xffff_ffff) as usize, (self.0 >> 32) as usize))
        } else {
            None
        }
    }
}
impl core::fmt::Debug for VisitChildrenResult {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0 == -1 {
            write!(f, "CONTINUE")
        } else {
            write!(f, "({},{})", (self.0 & 0xffff_ffff) as usize, (self.0 >> 32) as usize)
        }
    }
}

/// The item tree is an array of ItemTreeNode representing a static tree of items
/// within a component.
#[repr(u8)]
#[derive(Debug)]
pub enum ItemTreeNode<T> {
    /// Static item
    Item {
        /// byte offset where we can find the item (from the *ComponentImpl)
        item: vtable::VOffset<T, ItemVTable, vtable::AllowPin>,

        /// number of children
        children_count: u32,

        /// index of the first children within the item tree
        children_index: u32,

        /// The index of the parent item (not valid for the root)
        parent_index: u32,
    },
    /// A placeholder for many instance of item in their own component which
    /// are instantiated according to a model.
    DynamicTree {
        /// the index which is passed in the visit_dynamic callback.
        index: usize,

        /// The index of the parent item (not valid for the root)
        parent_index: u32,
    },
}

#[repr(C)]
#[vtable]
/// Object to be passed in visit_item_children method of the Component.
pub struct ItemVisitorVTable {
    /// Called for each children of the visited item
    ///
    /// The `component` parameter is the component in which the item live which might not be the same
    /// as the parent's component.
    /// `index` is to be used again in the visit_item_children function of the Component (the one passed as parameter)
    /// and `item` is a reference to the item itself
    visit_item: fn(
        VRefMut<ItemVisitorVTable>,
        component: &VRc<ComponentVTable, vtable::Dyn>,
        index: usize,
        item: Pin<VRef<ItemVTable>>,
    ) -> VisitChildrenResult,
    /// Destructor
    drop: fn(VRefMut<ItemVisitorVTable>),
}

/// Type alias to `vtable::VRefMut<ItemVisitorVTable>`
pub type ItemVisitorRefMut<'a> = vtable::VRefMut<'a, ItemVisitorVTable>;

impl<T: FnMut(&ComponentRc, usize, Pin<ItemRef>) -> VisitChildrenResult> ItemVisitor for T {
    fn visit_item(
        &mut self,
        component: &ComponentRc,
        index: usize,
        item: Pin<ItemRef>,
    ) -> VisitChildrenResult {
        self(component, index, item)
    }
}
pub enum ItemVisitorResult<State> {
    Continue(State),
    Abort,
}

/// Visit each items recursively
///
/// The state parameter returned by the visitor is passed to each children.
///
/// Returns the index of the item that cancelled, or -1 if nobody cancelled
pub fn visit_items<State>(
    component: &ComponentRc,
    order: TraversalOrder,
    mut visitor: impl FnMut(&ComponentRc, Pin<ItemRef>, usize, &State) -> ItemVisitorResult<State>,
    state: State,
) -> VisitChildrenResult {
    visit_internal(
        component,
        order,
        &mut |component, item, index, state| (visitor(component, item, index, state), ()),
        &mut |_, _, _, r| r,
        -1,
        &state,
    )
}

/// Visit each items recursively
///
/// The state parameter returned by the visitor is passed to each children.
///
/// Returns the index of the item that cancelled, or -1 if nobody cancelled
pub fn visit_items_with_post_visit<State, PostVisitState>(
    component: &ComponentRc,
    order: TraversalOrder,
    mut visitor: impl FnMut(
        &ComponentRc,
        Pin<ItemRef>,
        usize,
        &State,
    ) -> (ItemVisitorResult<State>, PostVisitState),
    mut post_visitor: impl FnMut(
        &ComponentRc,
        Pin<ItemRef>,
        PostVisitState,
        VisitChildrenResult,
    ) -> VisitChildrenResult,
    state: State,
) -> VisitChildrenResult {
    visit_internal(component, order, &mut visitor, &mut post_visitor, -1, &state)
}

fn visit_internal<State, PostVisitState>(
    component: &ComponentRc,
    order: TraversalOrder,
    visitor: &mut impl FnMut(
        &ComponentRc,
        Pin<ItemRef>,
        usize,
        &State,
    ) -> (ItemVisitorResult<State>, PostVisitState),
    post_visitor: &mut impl FnMut(
        &ComponentRc,
        Pin<ItemRef>,
        PostVisitState,
        VisitChildrenResult,
    ) -> VisitChildrenResult,
    index: isize,
    state: &State,
) -> VisitChildrenResult {
    let mut actual_visitor = |component: &ComponentRc,
                              index: usize,
                              item: Pin<ItemRef>|
     -> VisitChildrenResult {
        match visitor(component, item, index, state) {
            (ItemVisitorResult::Continue(state), post_visit_state) => {
                let r =
                    visit_internal(component, order, visitor, post_visitor, index as isize, &state);
                post_visitor(component, item, post_visit_state, r)
            }
            (ItemVisitorResult::Abort, post_visit_state) => post_visitor(
                component,
                item,
                post_visit_state,
                VisitChildrenResult::abort(index, 0),
            ),
        }
    };
    vtable::new_vref!(let mut actual_visitor : VRefMut<ItemVisitorVTable> for ItemVisitor = &mut actual_visitor);
    VRc::borrow_pin(component).as_ref().visit_children_item(index, order, actual_visitor)
}

/// Visit the children within an array of ItemTreeNode
///
/// The dynamic visitor is called for the dynamic nodes, its signature is
/// `fn(base: &Base, visitor: vtable::VRefMut<ItemVisitorVTable>, dyn_index: usize)`
///
/// FIXME: the design of this use lots of indirection and stack frame in recursive functions
/// Need to check if the compiler is able to optimize away some of it.
/// Possibly we should generate code that directly call the visitor instead
pub fn visit_item_tree<Base>(
    base: Pin<&Base>,
    component: &ComponentRc,
    item_tree: &[ItemTreeNode<Base>],
    index: isize,
    order: TraversalOrder,
    mut visitor: vtable::VRefMut<ItemVisitorVTable>,
    visit_dynamic: impl Fn(
        Pin<&Base>,
        TraversalOrder,
        vtable::VRefMut<ItemVisitorVTable>,
        usize,
    ) -> VisitChildrenResult,
) -> VisitChildrenResult {
    let mut visit_at_index = |idx: usize| -> VisitChildrenResult {
        match &item_tree[idx] {
            ItemTreeNode::Item { item, .. } => {
                visitor.visit_item(component, idx, item.apply_pin(base))
            }
            ItemTreeNode::DynamicTree { index, .. } => {
                if let Some(sub_idx) =
                    visit_dynamic(base, order, visitor.borrow_mut(), *index).aborted_index()
                {
                    VisitChildrenResult::abort(idx, sub_idx)
                } else {
                    VisitChildrenResult::CONTINUE
                }
            }
        }
    };
    if index == -1 {
        visit_at_index(0)
    } else {
        match &item_tree[index as usize] {
            ItemTreeNode::Item { children_index, children_count, .. } => {
                for c in 0..*children_count {
                    let idx = match order {
                        TraversalOrder::BackToFront => (*children_index + c),
                        TraversalOrder::FrontToBack => (*children_index + *children_count - c - 1),
                    } as usize;
                    let maybe_abort_index = visit_at_index(idx);
                    if maybe_abort_index.has_aborted() {
                        return maybe_abort_index;
                    }
                }
            }
            ItemTreeNode::DynamicTree { .. } => panic!("should not be called with dynamic items"),
        };
        VisitChildrenResult::CONTINUE
    }
}

#[cfg(feature = "ffi")]
pub(crate) mod ffi {
    #![allow(unsafe_code)]

    use super::*;
    use crate::slice::Slice;

    /// Expose `crate::item_tree::visit_item_tree` to C++
    ///
    /// Safety: Assume a correct implementation of the item_tree array
    #[no_mangle]
    pub unsafe extern "C" fn sixtyfps_visit_item_tree(
        component: &ComponentRc,
        item_tree: Slice<ItemTreeNode<u8>>,
        index: isize,
        order: TraversalOrder,
        visitor: VRefMut<ItemVisitorVTable>,
        visit_dynamic: extern "C" fn(
            base: &u8,
            order: TraversalOrder,
            visitor: vtable::VRefMut<ItemVisitorVTable>,
            dyn_index: usize,
        ) -> VisitChildrenResult,
    ) -> VisitChildrenResult {
        crate::item_tree::visit_item_tree(
            Pin::new_unchecked(&*(&**component as *const Dyn as *const u8)),
            component,
            item_tree.as_slice(),
            index,
            order,
            visitor,
            |a, b, c, d| visit_dynamic(a.get_ref(), b, c, d),
        )
    }
}
